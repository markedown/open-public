use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use chrono::NaiveDate;
use domain::models::{Education, PersonAttribute};
use maud::{html, Markup};
use serde::Deserialize;

use crate::auth::AuthSession;
use crate::error::PageError;
use crate::i18n;
use crate::ui;

/// Admin routes are invisible to everyone else: a non-admin (or signed-out)
/// visitor gets a plain 404 rather than any hint the area exists.
fn require_admin(session: &Option<AuthSession>) -> Result<(), PageError> {
    match session {
        Some(s) if s.is_admin => Ok(()),
        _ => Err(PageError::NotFound),
    }
}

/// The backoffice landing page: an admin-only hub into the editing workflow.
/// News, statements and polls are added from a person or party page (the
/// per-entity "add" buttons), so this page routes an admin there per country
/// and offers direct poll creation. Cast votes are never editable from here.
pub async fn index(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
) -> Result<Markup, PageError> {
    require_admin(&session)?;
    let countries = db::country::list(&pool).await?;
    let mut country_outlets = Vec::with_capacity(countries.len());
    for c in &countries {
        country_outlets.push((c, db::outlets::list(&pool, c.id).await?));
    }
    let open_conflicts = db::conflicts::count_open(&pool).await?;
    let pending_drafts = db::news::pending_draft_count(&pool).await?;
    let pending_translations = db::translations::pending_count(&pool).await?;
    let pending_submissions = db::submissions::pending_admin_count(&pool).await?;

    let content = html! {
        section class="mx-auto max-w-2xl" {
            h1 class="font-serif text-3xl font-semibold tracking-tight text-ink" {
                (i18n::t("Admin panel"))
            }
            p class="mt-4 max-w-prose text-sm text-ink-muted" {
                (i18n::t("Curated content is editable and always sourced. Cast votes are immutable and are never editable here."))
            }

            // Review queues, always shown so they stay discoverable even at zero.
            div class="mt-6 space-y-2" {
                @for (label, href, count) in [
                    (i18n::t("Review poll submissions"), "/admin/submissions", pending_submissions),
                    (i18n::t("Review summaries"), "/admin/summaries", pending_drafts),
                    (i18n::t("Review translations"), "/admin/translations", pending_translations),
                    (i18n::t("Review data conflicts"), "/admin/conflicts", open_conflicts),
                ] {
                    a href=(href)
                      class="flex items-center justify-between gap-3 border-[1.5px] border-ink px-5 py-3 transition-colors hover:border-accent" {
                        span class="text-sm font-medium text-ink" { (label) }
                        span class={"font-mono text-xs font-bold "
                            (if count > 0 { "text-accent" } else { "text-ink-muted" })} {
                            (count)
                        }
                    }
                }
            }

            // Each country's editing entry points, including its own outlets.
            @for (c, outlets) in &country_outlets {
                section class={"mt-8 border-[1.5px] border-ink p-5 " (ui::CORNER_TICK)} {
                    div class="flex items-baseline justify-between gap-3" {
                        h2 class="font-serif text-xl font-semibold text-ink" { (c.name) }
                    }
                    div class="mt-4 flex flex-wrap gap-2" {
                        a href={"/" (c.slug) "/people"}
                          class="border border-ink px-3 py-1.5 text-[11px] font-bold uppercase tracking-wide text-ink transition-colors hover:border-accent hover:text-accent" {
                            (i18n::t("People"))
                        }
                        a href={"/" (c.slug) "/parties"}
                          class="border border-ink px-3 py-1.5 text-[11px] font-bold uppercase tracking-wide text-ink transition-colors hover:border-accent hover:text-accent" {
                            (i18n::t("Parties"))
                        }
                        a href={"/admin/poll/new?country=" (c.slug)}
                          class="border border-ink bg-ink px-3 py-1.5 text-[11px] font-bold uppercase tracking-wide text-paper transition-colors hover:border-accent hover:bg-accent" {
                            (i18n::t("Add poll"))
                        }
                        a href={"/admin/outlet/new?country=" (c.slug)}
                          class="border border-ink bg-ink px-3 py-1.5 text-[11px] font-bold uppercase tracking-wide text-paper transition-colors hover:border-accent hover:bg-accent" {
                            (i18n::t("Add outlet"))
                        }
                    }
                    @if !outlets.is_empty() {
                        div class="mt-4" {
                            h3 class="mb-2 text-[11px] font-bold uppercase tracking-wide text-ink-muted" {
                                (i18n::t("Outlets"))
                            }
                            ul class="divide-y divide-hairline-light" {
                                @for o in outlets {
                                    li class="flex items-center justify-between gap-3 py-2" {
                                        span class="text-sm text-ink" { (o.name) }
                                        a href={"/admin/outlet/" (o.slug) "/edit?country=" (c.slug)}
                                          class="font-mono text-[11px] font-bold uppercase tracking-wide text-accent hover:underline" {
                                            (i18n::t("Edit"))
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            p class="mt-8 text-xs text-ink-muted" {
                (i18n::t("News and statements are added from a person or party page."))
            }
        }
    };
    Ok(ui::layout::document(
        Some(i18n::t("Admin panel")),
        true,
        true,
        content,
    ))
}

/// The summary review queue: draft summaries (for example from the automated
/// summarizer) awaiting an editor. The editor may edit the text before
/// publishing it as the item's summary, or discard the draft. Readers only ever
/// see published summaries, never drafts.
pub async fn summaries(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
) -> Result<Markup, PageError> {
    require_admin(&session)?;
    let drafts = db::news::pending_drafts(&pool).await?;

    let content = html! {
        section class="mx-auto max-w-3xl" {
            a href="/admin" class="text-xs text-ink-muted transition-colors hover:text-accent" {
                "← " (i18n::t("Admin panel"))
            }
            h1 class="mt-2 font-serif text-3xl font-semibold tracking-tight text-ink" {
                (i18n::t("Review summaries"))
            }
            p class="mt-2 max-w-prose text-sm text-ink-muted" {
                (i18n::t("Each draft summary stays unpublished until you review it. Edit the text if needed, then publish it, or discard the draft."))
            }

            @if drafts.is_empty() {
                p class="mt-8 py-10 text-center text-sm text-ink-muted" {
                    (i18n::t("No drafts to review."))
                }
            } @else {
                ul class="mt-6 space-y-4" {
                    @for d in &drafts {
                        li class="border border-hairline p-4" {
                            a href=(d.url) rel="noopener" target="_blank"
                              class="block text-sm font-medium text-ink transition-colors hover:text-accent" {
                                (d.headline)
                            }
                            @if let Some(ref o) = d.outlet {
                                span class="font-mono text-xs text-ink-muted" { (o) }
                            }
                            form method="post" action={"/admin/summaries/" (d.id) "/publish"}
                                 class="mt-3 flex flex-col gap-2" {
                                textarea name="summary" rows="3"
                                  class="w-full border border-hairline bg-paper p-2 text-sm text-ink" {
                                    (d.summary_draft)
                                }
                                div class="flex gap-2" {
                                    button type="submit"
                                      class="border border-ink bg-ink px-4 py-1.5 text-[11px] font-bold uppercase tracking-wide text-paper transition-colors hover:border-accent hover:bg-accent" {
                                        (i18n::t("Publish"))
                                    }
                                    button type="submit" formaction={"/admin/summaries/" (d.id) "/discard"}
                                      formnovalidate
                                      class="border border-hairline px-4 py-1.5 text-[11px] font-bold uppercase tracking-wide text-ink-muted transition-colors hover:border-ink hover:text-ink" {
                                        (i18n::t("Discard"))
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };
    Ok(ui::layout::document(
        Some(i18n::t("Review summaries")),
        true,
        true,
        content,
    ))
}

/// The published-summary form field.
#[derive(Deserialize)]
pub struct SummaryForm {
    summary: String,
}

/// Publish a reviewed summary (as edited), then return to the queue.
pub async fn summary_publish(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path(id): Path<i64>,
    Form(form): Form<SummaryForm>,
) -> Result<Redirect, PageError> {
    require_admin(&session)?;
    let text = form.summary.trim();
    if text.is_empty() {
        db::news::discard_draft(&pool, id).await?;
    } else {
        db::news::publish_summary(&pool, id, text).await?;
    }
    Ok(Redirect::to("/admin/summaries"))
}

/// Discard a draft without publishing, then return to the queue.
pub async fn summary_discard(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path(id): Path<i64>,
) -> Result<Redirect, PageError> {
    require_admin(&session)?;
    db::news::discard_draft(&pool, id).await?;
    Ok(Redirect::to("/admin/summaries"))
}

/// The translation review queue: draft translations shown against their source
/// text, to edit and publish or discard. Publishing is what makes a translation
/// visible to readers.
pub async fn translations(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
) -> Result<Markup, PageError> {
    require_admin(&session)?;
    let drafts = db::translations::pending(&pool, 100).await?;
    let mut rows = Vec::with_capacity(drafts.len());
    for d in drafts {
        let original =
            db::translations::original(&pool, &d.entity_type, d.entity_id, &d.field).await?;
        rows.push((d, original));
    }

    let content = html! {
        section class="mx-auto max-w-3xl" {
            a href="/admin" class="text-xs text-ink-muted transition-colors hover:text-accent" {
                "← " (i18n::t("Admin panel"))
            }
            h1 class="mt-2 font-serif text-3xl font-semibold tracking-tight text-ink" {
                (i18n::t("Review translations"))
            }
            p class="mt-2 max-w-prose text-sm text-ink-muted" {
                (i18n::t("Each draft translation stays hidden until you review it. Compare it with the original, edit the text if needed, then publish it, or discard the draft."))
            }

            @if rows.is_empty() {
                p class="mt-8 py-10 text-center text-sm text-ink-muted" {
                    (i18n::t("No drafts to review."))
                }
            } @else {
                ul class="mt-6 space-y-4" {
                    @for (d, original) in &rows {
                        li class="border border-hairline p-4" {
                            div class="flex flex-wrap items-center gap-x-3 gap-y-1 font-mono text-[11px] uppercase tracking-wide text-ink-muted" {
                                span class="font-bold text-ink" { (d.entity_type) " · " (d.field) }
                                span { (d.lang) }
                                @if let Some(ref src) = d.source_lang { span { "← " (src) } }
                                span { (d.origin) }
                            }
                            @if let Some(orig) = original {
                                p class="mt-2 border-l-2 border-hairline pl-3 text-sm text-ink-muted" { (orig) }
                            }
                            form method="post" action={"/admin/translations/" (d.id) "/publish"}
                                 class="mt-3 flex flex-col gap-2" {
                                textarea name="text" rows="3"
                                  class="w-full border border-hairline bg-paper p-2 text-sm text-ink" {
                                    (d.text)
                                }
                                div class="flex gap-2" {
                                    button type="submit"
                                      class="border border-ink bg-ink px-4 py-1.5 text-[11px] font-bold uppercase tracking-wide text-paper transition-colors hover:border-accent hover:bg-accent" {
                                        (i18n::t("Publish"))
                                    }
                                    button type="submit" formaction={"/admin/translations/" (d.id) "/discard"}
                                      formnovalidate
                                      class="border border-hairline px-4 py-1.5 text-[11px] font-bold uppercase tracking-wide text-ink-muted transition-colors hover:border-ink hover:text-ink" {
                                        (i18n::t("Discard"))
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };
    Ok(ui::layout::document(
        Some(i18n::t("Review translations")),
        true,
        true,
        content,
    ))
}

/// The translation-review form field (the reviewed, possibly edited text).
#[derive(Deserialize)]
pub struct TranslationForm {
    text: String,
}

/// Publish a reviewed translation (as edited), then return to the queue.
pub async fn translation_publish(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path(id): Path<i64>,
    Form(form): Form<TranslationForm>,
) -> Result<Redirect, PageError> {
    require_admin(&session)?;
    let reviewer = session.as_ref().map(|s| s.user_id).unwrap_or_default();
    let text = form.text.trim();
    if text.is_empty() {
        db::translations::discard(&pool, id).await?;
    } else {
        db::translations::set_text(&pool, id, text).await?;
        db::translations::publish(&pool, id, reviewer).await?;
    }
    Ok(Redirect::to("/admin/translations"))
}

/// Discard a draft translation without publishing, then return to the queue.
pub async fn translation_discard(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path(id): Path<i64>,
) -> Result<Redirect, PageError> {
    require_admin(&session)?;
    db::translations::discard(&pool, id).await?;
    Ok(Redirect::to("/admin/translations"))
}

/// The data-integrity review queue: source disagreements that ingestion logged
/// instead of overwriting a trusted value. Read-only apart from resolving; the
/// values themselves are corrected through the normal editing flow.
pub async fn conflicts(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
) -> Result<Markup, PageError> {
    require_admin(&session)?;
    let items = db::conflicts::list_open(&pool).await?;

    let content = html! {
        section class="mx-auto max-w-3xl" {
            a href="/admin" class="text-xs text-ink-muted transition-colors hover:text-accent" {
                "← " (i18n::t("Admin panel"))
            }
            h1 class="mt-2 font-serif text-3xl font-semibold tracking-tight text-ink" {
                (i18n::t("Data conflicts"))
            }
            p class="mt-2 max-w-prose text-sm text-ink-muted" {
                (i18n::t("A source disagreed with a value we already hold. Nothing was overwritten. Correct the record through the normal editing flow, then mark the conflict resolved."))
            }

            @if items.is_empty() {
                p class="mt-8 py-10 text-center text-sm text-ink-muted" {
                    (i18n::t("No open conflicts."))
                }
            } @else {
                ul class="mt-6 space-y-3" {
                    @for c in &items {
                        li class="border border-hairline p-4" {
                            div class="flex flex-wrap items-baseline justify-between gap-2" {
                                span class="text-sm font-medium text-ink" {
                                    (c.entity_label.as_deref().unwrap_or(&c.entity_type))
                                    " · "
                                    span class="font-mono text-xs text-ink-muted" { (c.field) }
                                }
                                form method="post" action={"/admin/conflicts/" (c.id) "/resolve"} {
                                    button type="submit"
                                      class="border border-ink px-3 py-1 text-[11px] font-bold uppercase tracking-wide text-ink transition-colors hover:border-accent hover:text-accent" {
                                        (i18n::t("Mark resolved"))
                                    }
                                }
                            }
                            div class="mt-3 grid gap-3 sm:grid-cols-2" {
                                (conflict_side(
                                    i18n::t("Ours"),
                                    c.existing_value.as_deref(),
                                    c.existing_source_url.as_deref(),
                                ))
                                (conflict_side(
                                    i18n::t("Incoming"),
                                    c.incoming_value.as_deref(),
                                    c.incoming_source_url.as_deref(),
                                ))
                            }
                        }
                    }
                }
            }
        }
    };
    Ok(ui::layout::document(
        Some(i18n::t("Data conflicts")),
        true,
        true,
        content,
    ))
}

/// One side of a conflict: a labelled value with its source link.
fn conflict_side(label: &str, value: Option<&str>, source: Option<&str>) -> Markup {
    html! {
        div class="border border-hairline-light p-3" {
            div class="text-[10px] font-bold uppercase tracking-widest text-ink-muted" { (label) }
            div class="mt-1 text-sm text-ink" { (value.unwrap_or("-")) }
            @if let Some(url) = source {
                a href=(url) rel="noopener" target="_blank"
                  class="mt-1 block truncate font-mono text-[11px] text-accent hover:underline" { (url) }
            }
        }
    }
}

/// Mark a conflict resolved, then return to the review queue.
pub async fn conflict_resolve(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path(id): Path<i64>,
) -> Result<Redirect, PageError> {
    require_admin(&session)?;
    db::conflicts::resolve(&pool, id).await?;
    Ok(Redirect::to("/admin/conflicts"))
}

/// The entity a backoffice form was reached from, carried through the form.
#[derive(Deserialize)]
pub struct EntityQuery {
    country: Option<String>,
    person: Option<String>,
    party: Option<String>,
}

/// The country an entity backoffice page was reached from.
#[derive(Deserialize)]
pub struct CountryQuery {
    country: Option<String>,
}

/// The backoffice page for one entity (a party or person): all the add-content
/// actions in one place, so the public page stays clean. `kind` is "party" or
/// "person"; `back` links to the public page.
fn manage_page(name: &str, country: &str, kind: &str, slug: &str, back: &str) -> Markup {
    let add = |what: &str| format!("/admin/{what}/new?country={country}&{kind}={slug}");
    let content = html! {
        section class="mx-auto max-w-xl" {
            a href=(back) class="text-xs text-ink-muted transition-colors hover:text-accent" {
                "← " (name)
            }
            h1 class="mt-2 font-serif text-3xl font-semibold tracking-tight text-ink" {
                (i18n::t("Manage")) " · " (name)
            }
            p class="mt-2 text-sm text-ink-muted" {
                (i18n::t("Add sourced content for this entity."))
            }
            div class="mt-6 flex flex-wrap gap-2" {
                @for (label, what) in [
                    (i18n::t("Add news"), "news"),
                    (i18n::t("Add statement"), "statement"),
                    (i18n::t("Add poll"), "poll"),
                ] {
                    a href=(add(what))
                      class="border border-ink bg-ink px-4 py-2 text-[11px] font-bold uppercase tracking-wide text-paper transition-colors hover:border-accent hover:bg-accent" {
                        (label)
                    }
                }
            }
        }
    };
    ui::layout::document(Some(i18n::t("Manage")), true, true, content)
}

/// Backoffice page for a party.
pub async fn party_manage(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path(slug): Path<String>,
    Query(q): Query<CountryQuery>,
) -> Result<Markup, PageError> {
    require_admin(&session)?;
    let party = db::parties::get_by_slug(&pool, &slug)
        .await?
        .ok_or(PageError::NotFound)?;
    let country = q.country.unwrap_or_default();
    let back = format!("/{country}/parties/{slug}");
    Ok(manage_page(&party.name, &country, "party", &slug, &back))
}

/// Backoffice page for a person: the add-content actions plus editing of the
/// biographical enrichment (education and attributes).
pub async fn person_manage(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path(slug): Path<String>,
    Query(q): Query<CountryQuery>,
) -> Result<Markup, PageError> {
    require_admin(&session)?;
    let person = db::people::get_by_slug(&pool, &slug)
        .await?
        .ok_or(PageError::NotFound)?;
    let country = q.country.unwrap_or_default();
    let education = db::people::education(&pool, person.id).await?;
    let attributes = db::people::attributes(&pool, person.id).await?;
    Ok(person_manage_page(
        &person.full_name,
        &slug,
        &country,
        &education,
        &attributes,
    ))
}

fn person_manage_page(
    name: &str,
    slug: &str,
    country: &str,
    education: &[Education],
    attributes: &[PersonAttribute],
) -> Markup {
    let back = format!("/{country}/people/{slug}");
    let base = format!("/admin/person/{slug}");
    let del_btn = html! {
        button type="submit"
          class="font-mono text-[10px] font-bold uppercase tracking-wide text-ink-muted transition-colors hover:text-accent" {
            (i18n::t("Delete"))
        }
    };
    let content = html! {
        section class="mx-auto max-w-xl" {
            a href=(back) class="text-xs text-ink-muted transition-colors hover:text-accent" {
                "← " (name)
            }
            h1 class="mt-2 font-serif text-3xl font-semibold tracking-tight text-ink" {
                (i18n::t("Manage")) " · " (name)
            }
            div class="mt-6 flex flex-wrap gap-2" {
                @for (label, what) in [
                    (i18n::t("Add news"), "news"),
                    (i18n::t("Add statement"), "statement"),
                    (i18n::t("Add poll"), "poll"),
                ] {
                    a href={"/admin/" (what) "/new?country=" (country) "&person=" (slug)}
                      class="border border-ink bg-ink px-4 py-2 text-[11px] font-bold uppercase tracking-wide text-paper transition-colors hover:border-accent hover:bg-accent" {
                        (label)
                    }
                }
            }

            // Education.
            h2 class="mt-10 mb-3 border-b-2 border-accent pb-2 text-xs font-bold uppercase tracking-widest text-ink" {
                (i18n::t("Education"))
            }
            @if !education.is_empty() {
                ul class="mb-4 divide-y divide-hairline-light" {
                    @for e in education {
                        li class="flex items-center justify-between gap-3 py-2" {
                            span class="text-sm text-ink" {
                                (e.institution)
                                @if let Some(ref d) = e.degree { span class="text-ink-muted" { " · " (d) } }
                            }
                            form method="post" action={(base) "/education/" (e.id) "/delete?country=" (country)} {
                                (del_btn)
                            }
                        }
                    }
                }
            }
            form class="space-y-3" method="post" action={(base) "/education?country=" (country)} {
                (prefilled("institution", i18n::t("Institution"), "text", true, None))
                (prefilled("degree", i18n::t("Degree"), "text", false, None))
                (prefilled("field", i18n::t("Field of study"), "text", false, None))
                (prefilled("start_date", i18n::t("Start date"), "date", false, None))
                (prefilled("end_date", i18n::t("End date"), "date", false, None))
                (prefilled("source_url", i18n::t("Source URL"), "url", true, None))
                (ui::button::primary(i18n::t("Add education")))
            }

            // Attributes.
            h2 class="mt-10 mb-3 border-b-2 border-accent pb-2 text-xs font-bold uppercase tracking-widest text-ink" {
                (i18n::t("Attributes"))
            }
            @if !attributes.is_empty() {
                ul class="mb-4 divide-y divide-hairline-light" {
                    @for a in attributes {
                        li class="flex items-center justify-between gap-3 py-2" {
                            span class="text-sm text-ink" {
                                span class="font-mono text-xs uppercase text-ink-muted" { (a.kind) } " · " (a.value)
                            }
                            form method="post" action={(base) "/attribute/" (a.id) "/delete?country=" (country)} {
                                (del_btn)
                            }
                        }
                    }
                }
            }
            form class="space-y-3" method="post" action={(base) "/attribute?country=" (country)} {
                div {
                    label class="block text-sm font-medium text-ink" for="kind" { (i18n::t("Kind")) }
                    select name="kind" id="kind"
                        class="mt-1 block w-full border border-hairline bg-paper-raised px-3 py-2 text-sm text-ink focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent" {
                        @for k in db::people::ATTRIBUTE_KINDS {
                            option value=(k) { (k) }
                        }
                    }
                }
                (prefilled("value", i18n::t("Value"), "text", true, None))
                (prefilled("source_url", i18n::t("Source URL"), "url", true, None))
                (ui::button::primary(i18n::t("Add attribute")))
            }
        }
    };
    ui::layout::document(Some(i18n::t("Manage")), true, true, content)
}

/// A trimmed optional string, empty becomes `None`.
fn trimmed(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

fn parse_date(s: Option<String>) -> Option<NaiveDate> {
    trimmed(s).and_then(|v| NaiveDate::parse_from_str(&v, "%Y-%m-%d").ok())
}

/// The add-education form.
#[derive(Deserialize)]
pub struct EducationForm {
    country: String,
    institution: String,
    degree: Option<String>,
    field: Option<String>,
    start_date: Option<String>,
    end_date: Option<String>,
    source_url: String,
}

/// Add a sourced education entry to a person, then return to the manage page.
pub async fn person_education_add(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path(slug): Path<String>,
    Form(form): Form<EducationForm>,
) -> Result<Redirect, PageError> {
    require_admin(&session)?;
    let person = db::people::get_by_slug(&pool, &slug)
        .await?
        .ok_or(PageError::NotFound)?;
    let institution = form.institution.trim();
    let url = form.source_url.trim();
    if institution.is_empty() || url.is_empty() {
        return Err(PageError::Server);
    }
    let source_id = db::sources::insert_source(&pool, "manual", url, None, None).await?;
    db::people::upsert_education(
        &pool,
        person.id,
        institution,
        None,
        trimmed(form.degree).as_deref(),
        trimmed(form.field).as_deref(),
        parse_date(form.start_date),
        parse_date(form.end_date),
        source_id,
    )
    .await?;
    Ok(Redirect::to(&format!(
        "/admin/person/{slug}?country={}",
        form.country
    )))
}

/// Delete a person's education entry.
pub async fn person_education_delete(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path((slug, id)): Path<(String, i64)>,
    Query(q): Query<CountryQuery>,
) -> Result<Redirect, PageError> {
    require_admin(&session)?;
    db::people::delete_education(&pool, id).await?;
    Ok(Redirect::to(&format!(
        "/admin/person/{slug}?country={}",
        q.country.unwrap_or_default()
    )))
}

/// The add-attribute form.
#[derive(Deserialize)]
pub struct AttributeForm {
    country: String,
    kind: String,
    value: String,
    source_url: String,
}

/// Add a sourced attribute to a person, then return to the manage page.
pub async fn person_attribute_add(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path(slug): Path<String>,
    Form(form): Form<AttributeForm>,
) -> Result<Redirect, PageError> {
    require_admin(&session)?;
    let person = db::people::get_by_slug(&pool, &slug)
        .await?
        .ok_or(PageError::NotFound)?;
    let value = form.value.trim();
    let url = form.source_url.trim();
    if value.is_empty()
        || url.is_empty()
        || !db::people::ATTRIBUTE_KINDS.contains(&form.kind.as_str())
    {
        return Err(PageError::Server);
    }
    let source_id = db::sources::insert_source(&pool, "manual", url, None, None).await?;
    db::people::upsert_attribute(&pool, person.id, &form.kind, value, None, source_id).await?;
    Ok(Redirect::to(&format!(
        "/admin/person/{slug}?country={}",
        form.country
    )))
}

/// Delete a person's attribute.
pub async fn person_attribute_delete(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path((slug, id)): Path<(String, i64)>,
    Query(q): Query<CountryQuery>,
) -> Result<Redirect, PageError> {
    require_admin(&session)?;
    db::people::delete_attribute(&pool, id).await?;
    Ok(Redirect::to(&format!(
        "/admin/person/{slug}?country={}",
        q.country.unwrap_or_default()
    )))
}

/// Map a service-layer error to an HTTP page error. A missing entity is a 404;
/// everything else (validation, database) is a 500 for now.
fn map_service_err(e: db::service::Error) -> PageError {
    match e {
        db::service::Error::NotFound { .. } => PageError::NotFound,
        db::service::Error::Validation(_) | db::service::Error::Db(_) => PageError::Server,
    }
}

/// The outlet create/edit form. `outlet` is `Some` when editing (fields are
/// prefilled and the slug is kept), `None` when adding. Upserting on slug means
/// one handler serves both.
fn outlet_form_page(o: Option<&db::outlets::Outlet>, country: &str) -> Markup {
    let text = |v: Option<&str>| v.unwrap_or("").to_string();
    let heading = if o.is_some() {
        i18n::t("Edit outlet")
    } else {
        i18n::t("Add outlet")
    };
    let content = html! {
        section class="mx-auto max-w-xl" {
            a href="/admin" class="text-xs text-ink-muted transition-colors hover:text-accent" {
                "← " (i18n::t("Admin panel"))
            }
            h1 class="mt-2 font-serif text-3xl font-semibold tracking-tight text-ink" { (heading) }
            form class="mt-6 space-y-4" method="post" action="/admin/outlet" {
                input type="hidden" name="country" value=(country);
                (prefilled("name", i18n::t("Name"), "text", true, o.map(|x| x.name.as_str())))
                (prefilled("slug", i18n::t("Slug"), "text", true, o.map(|x| x.slug.as_str())))
                (prefilled("homepage_url", i18n::t("Homepage"), "url", false, o.map(|x| text(x.homepage_url.as_deref())).as_deref()))
                (prefilled("logo_url", i18n::t("Logo URL"), "url", false, o.map(|x| text(x.logo_url.as_deref())).as_deref()))
                (prefilled("logo_license", i18n::t("Logo license"), "text", false, o.map(|x| text(x.logo_license.as_deref())).as_deref()))
                div {
                    label class="block text-sm font-medium text-ink" for="leaning" { (i18n::t("Political leaning")) }
                    select name="leaning" id="leaning"
                        class="mt-1 block w-full border border-hairline bg-paper-raised px-3 py-2 text-sm text-ink focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent" {
                        option value="" { (i18n::t("Unrated")) }
                        @for l in db::outlets::LEANINGS {
                            option value=(l) selected[o.and_then(|x| x.leaning.as_deref()) == Some(l)] {
                                (crate::ui::outlet::leaning_label(l))
                            }
                        }
                    }
                }
                div {
                    label class="block text-sm font-medium text-ink" for="summary" { (i18n::t("Summary")) }
                    textarea name="summary" id="summary" rows="3"
                        class="mt-1 block w-full border border-hairline bg-paper-raised px-3 py-2 text-sm text-ink focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent" {
                        (o.map(|x| text(x.summary.as_deref())).unwrap_or_default())
                    }
                }
                (ui::button::primary(i18n::t("Save")))
            }
        }
    };
    ui::layout::document(Some(heading), true, true, content)
}

/// A text field prefilled with an optional current value.
fn prefilled(name: &str, label: &str, ty: &str, req: bool, value: Option<&str>) -> Markup {
    html! {
        div {
            label class="block text-sm font-medium text-ink" for=(name) { (label) }
            input type=(ty) name=(name) id=(name) required[req] value=(value.unwrap_or(""))
                class="mt-1 block w-full border border-hairline bg-paper-raised px-3 py-2 text-sm text-ink focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent";
        }
    }
}

/// The add-outlet form.
pub async fn outlet_new(
    session: Option<AuthSession>,
    Query(q): Query<CountryQuery>,
) -> Result<Markup, PageError> {
    require_admin(&session)?;
    Ok(outlet_form_page(None, &q.country.unwrap_or_default()))
}

/// The edit-outlet form, prefilled from the existing row.
pub async fn outlet_edit(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path(slug): Path<String>,
    Query(q): Query<CountryQuery>,
) -> Result<Markup, PageError> {
    require_admin(&session)?;
    let outlet = db::outlets::get_by_slug(&pool, &slug)
        .await?
        .ok_or(PageError::NotFound)?;
    Ok(outlet_form_page(
        Some(&outlet),
        &q.country.unwrap_or_default(),
    ))
}

/// The submitted outlet form.
#[derive(Deserialize)]
pub struct OutletForm {
    country: String,
    name: String,
    slug: String,
    homepage_url: Option<String>,
    logo_url: Option<String>,
    logo_license: Option<String>,
    leaning: Option<String>,
    summary: Option<String>,
}

/// Create or update an outlet (upsert on slug), then return to its public page.
/// An outlet carries no provenance source: its website is its reference and its
/// leaning is our own assessment.
pub async fn outlet_save(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Form(form): Form<OutletForm>,
) -> Result<Redirect, PageError> {
    require_admin(&session)?;
    let clean = |s: Option<String>| s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let name = form.name.trim();
    let slug = form.slug.trim();
    if name.is_empty() || slug.is_empty() {
        return Err(PageError::Server);
    }
    let leaning = form
        .leaning
        .as_deref()
        .map(str::trim)
        .filter(|l| db::outlets::LEANINGS.contains(l));
    let country_id = db::country::get_by_slug(&pool, form.country.trim())
        .await?
        .map(|c| c.id);

    db::outlets::upsert(
        &pool,
        &db::outlets::NewOutlet {
            name,
            slug,
            homepage_url: clean(form.homepage_url).as_deref(),
            logo_url: clean(form.logo_url).as_deref(),
            logo_license: clean(form.logo_license).as_deref(),
            leaning,
            summary: clean(form.summary).as_deref(),
            country_id,
        },
    )
    .await?;

    Ok(Redirect::to(&format!("/{}/outlet/{}", form.country, slug)))
}

/// The news edit page: headline and summary, plus the people and parties the
/// item is linked to, each removable, and a search box to attach more.
pub async fn news_edit(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path(id): Path<i64>,
) -> Result<Markup, PageError> {
    require_admin(&session)?;
    let item = db::news::get_edit(&pool, id)
        .await?
        .ok_or(PageError::NotFound)?;

    let linked = |title: &str, kind: &str, entities: &[db::news::LinkedEntity]| -> Markup {
        html! {
            div {
                div class="text-[10px] font-bold uppercase tracking-widest text-ink-muted" { (title) }
                @if entities.is_empty() {
                    p class="mt-1 text-sm text-ink-muted" { "-" }
                } @else {
                    ul class="mt-1 flex flex-wrap gap-2" {
                        @for e in entities {
                            li class="inline-flex items-center gap-1.5 border border-hairline px-2 py-1 text-xs text-ink" {
                                (e.name)
                                form method="post" action={"/admin/news/" (id) "/unlink"} {
                                    input type="hidden" name="kind" value=(kind);
                                    input type="hidden" name="id" value=(e.id);
                                    button type="submit" class="text-ink-muted transition-colors hover:text-accent" { "×" }
                                }
                            }
                        }
                    }
                }
            }
        }
    };

    let content = html! {
        section class="mx-auto max-w-xl" {
            a href="/admin" class="text-xs text-ink-muted transition-colors hover:text-accent" {
                "← " (i18n::t("Admin panel"))
            }
            h1 class="mt-2 font-serif text-3xl font-semibold tracking-tight text-ink" {
                (i18n::t("Edit news"))
            }
            a href=(item.url) rel="noopener" target="_blank"
              class="mt-1 block truncate font-mono text-[11px] text-accent hover:underline" { (item.url) }

            form class="mt-6 space-y-4" method="post" action={"/admin/news/" (id)} {
                (prefilled("headline", i18n::t("Headline"), "text", true, Some(item.headline.as_str())))
                (prefilled("author", i18n::t("Author"), "text", false, item.author.as_deref()))
                div {
                    label class="block text-sm font-medium text-ink" for="our_summary" { (i18n::t("Summary")) }
                    textarea name="our_summary" id="our_summary" rows="3"
                        class="mt-1 block w-full border border-hairline bg-paper-raised px-3 py-2 text-sm text-ink focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent" {
                        (item.our_summary.clone().unwrap_or_default())
                    }
                }
                (ui::button::primary(i18n::t("Save")))
            }

            section class="mt-8 space-y-4" {
                h2 class="border-b-2 border-accent pb-2 text-xs font-bold uppercase tracking-widest text-ink" {
                    (i18n::t("Relations"))
                }
                (linked(i18n::t("People"), "person", &item.people))
                (linked(i18n::t("Parties"), "party", &item.parties))

                // Search-as-you-type (HTMX) with a plain-GET fallback: without
                // JavaScript the same form submits to the search page.
                form method="get" action={"/admin/news/" (id) "/search"} class="mt-4" {
                    label class="block text-sm font-medium text-ink" for="q" { (i18n::t("Attach a person or party")) }
                    input type="search" name="q" id="q" autocomplete="off"
                        hx-get={"/admin/news/" (id) "/search"} hx-target="#search-results"
                        hx-trigger="keyup changed delay:300ms, search"
                        class="mt-1 block w-full border border-hairline bg-paper-raised px-3 py-2 text-sm text-ink focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent";
                    noscript { button type="submit" class="mt-2 text-xs font-bold uppercase tracking-wide text-accent" { (i18n::t("Search")) } }
                }
                div id="search-results" {}
            }
        }
    };
    Ok(ui::layout::document(
        Some(i18n::t("Edit news")),
        true,
        true,
        content,
    ))
}

/// The search box query for attaching a relation.
#[derive(Deserialize)]
pub struct RelationSearch {
    q: Option<String>,
}

/// Search results for the relation attach box. Returns just the list fragment to
/// an HTMX request, or a standalone page (with the results) without JavaScript.
pub async fn news_relation_search(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path(id): Path<i64>,
    headers: axum::http::HeaderMap,
    Query(q): Query<RelationSearch>,
) -> Result<Response, PageError> {
    require_admin(&session)?;
    let query = q.q.unwrap_or_default();
    let hits = if query.trim().is_empty() {
        Vec::new()
    } else {
        db::search::search(&pool, &query, 10).await?
    };

    let list = html! {
        ul id="search-results" class="mt-2 space-y-1" {
            @for h in &hits {
                @let kind = match h.kind { domain::models::SearchKind::Person => "person", _ => "party" };
                @let kind_label = match h.kind { domain::models::SearchKind::Person => i18n::t("Person"), _ => i18n::t("Party") };
                li {
                    form method="post" action={"/admin/news/" (id) "/link"} {
                        input type="hidden" name="kind" value=(kind);
                        input type="hidden" name="slug" value=(h.slug);
                        button type="submit"
                          class="flex w-full items-center justify-between gap-2 border border-hairline px-3 py-1.5 text-left text-sm text-ink transition-colors hover:border-accent" {
                            span { "+ " (h.name) }
                            span class="font-mono text-[10px] uppercase tracking-wide text-ink-muted" { (kind_label) }
                        }
                    }
                }
            }
        }
    };

    if headers.contains_key("hx-request") {
        Ok(list.into_response())
    } else {
        Ok(ui::layout::document(Some(i18n::t("Search")), true, true, html! {
            section class="mx-auto max-w-xl" {
                a href={"/admin/news/" (id) "/edit"} class="text-xs text-ink-muted hover:text-accent" {
                    "← " (i18n::t("Edit news"))
                }
                h1 class="mt-2 font-serif text-2xl font-semibold text-ink" { (i18n::t("Attach a person or party")) }
                (list)
            }
        })
        .into_response())
    }
}

/// The edited news fields.
#[derive(Deserialize)]
pub struct NewsFieldsForm {
    headline: String,
    our_summary: Option<String>,
    author: Option<String>,
}

/// Update a news item's headline, summary and author.
pub async fn news_update(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path(id): Path<i64>,
    Form(form): Form<NewsFieldsForm>,
) -> Result<Redirect, PageError> {
    require_admin(&session)?;
    let headline = form.headline.trim();
    if headline.is_empty() {
        return Err(PageError::Server);
    }
    let trimmed = |s: Option<String>| s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let summary = trimmed(form.our_summary);
    let author = trimmed(form.author);
    db::news::update_fields(&pool, id, headline, summary.as_deref(), author.as_deref()).await?;
    Ok(Redirect::to(&format!("/admin/news/{id}/edit")))
}

/// Attach a person or party (by slug) to a news item.
#[derive(Deserialize)]
pub struct LinkForm {
    kind: String,
    slug: String,
}

pub async fn news_link(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path(id): Path<i64>,
    Form(form): Form<LinkForm>,
) -> Result<Redirect, PageError> {
    require_admin(&session)?;
    match form.kind.as_str() {
        "person" => db::news::link_person(&pool, id, form.slug.trim()).await?,
        "party" => db::news::link_party(&pool, id, form.slug.trim()).await?,
        _ => return Err(PageError::Server),
    };
    Ok(Redirect::to(&format!("/admin/news/{id}/edit")))
}

/// Remove a person or party link from a news item.
#[derive(Deserialize)]
pub struct UnlinkForm {
    kind: String,
    id: i64,
}

pub async fn news_unlink(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path(news_id): Path<i64>,
    Form(form): Form<UnlinkForm>,
) -> Result<Redirect, PageError> {
    require_admin(&session)?;
    match form.kind.as_str() {
        "person" => db::news::unlink_person(&pool, news_id, form.id).await?,
        "party" => db::news::unlink_party(&pool, news_id, form.id).await?,
        _ => return Err(PageError::Server),
    }
    Ok(Redirect::to(&format!("/admin/news/{news_id}/edit")))
}

/// The "add news" form, reached from a person or party page. The linked entity
/// is carried through as hidden fields.
pub async fn news_form(
    session: Option<AuthSession>,
    Query(q): Query<EntityQuery>,
) -> Result<Markup, PageError> {
    require_admin(&session)?;
    let country = q.country.unwrap_or_default();

    let content = html! {
        section class="mx-auto max-w-xl" {
            h1 class="font-serif text-3xl font-semibold tracking-tight text-ink" {
                (i18n::t("Add news"))
            }
            form class="mt-6 space-y-4" method="post" action="/admin/news" {
                input type="hidden" name="country" value=(country);
                @if let Some(ref p) = q.person { input type="hidden" name="person" value=(p); }
                @if let Some(ref p) = q.party { input type="hidden" name="party" value=(p); }

                (field("headline", i18n::t("Headline"), "text", true))
                (field("url", i18n::t("Source URL"), "url", true))
                (field("outlet", i18n::t("Outlet"), "text", false))
                (field("published_at", i18n::t("Date"), "date", false))
                div {
                    label class="block text-sm font-medium text-ink" for="our_summary" {
                        (i18n::t("Summary"))
                    }
                    textarea name="our_summary" id="our_summary" rows="3"
                        class="mt-1 block w-full border border-hairline bg-paper-raised px-3 py-2 text-sm text-ink focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent" {}
                }
                (ui::button::primary(i18n::t("Save")))
            }
        }
    };
    Ok(ui::layout::document(
        Some(i18n::t("Add news")),
        true,
        true,
        content,
    ))
}

fn field(name: &str, label: &str, ty: &str, req: bool) -> Markup {
    html! {
        div {
            label class="block text-sm font-medium text-ink" for=(name) { (label) }
            input type=(ty) name=(name) id=(name) required[req]
                class="mt-1 block w-full border border-hairline bg-paper-raised px-3 py-2 text-sm text-ink focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent";
        }
    }
}

#[derive(Deserialize)]
pub struct NewsForm {
    country: String,
    person: Option<String>,
    party: Option<String>,
    headline: String,
    url: String,
    outlet: Option<String>,
    published_at: Option<String>,
    our_summary: Option<String>,
}

/// Create a news item, link it to the entity it came from, and return to that
/// entity's page. The handler only adapts the form to the service call; all
/// validation and orchestration live in `db::service::news` so a future API can
/// reuse them unchanged.
pub async fn news_create(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Form(form): Form<NewsForm>,
) -> Result<Response, PageError> {
    require_admin(&session)?;

    let redirect = match (
        form.person.as_deref().filter(|s| !s.is_empty()),
        form.party.as_deref().filter(|s| !s.is_empty()),
    ) {
        (Some(s), _) => format!("/{}/people/{}", form.country, s),
        (_, Some(s)) => format!("/{}/parties/{}", form.country, s),
        _ => format!("/{}", form.country),
    };

    let published_on = form
        .published_at
        .as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

    db::service::news::create(
        &pool,
        db::service::news::CreateNews {
            person_slug: form.person,
            party_slug: form.party,
            headline: form.headline,
            url: form.url,
            outlet: form.outlet,
            published_on,
            our_summary: form.our_summary,
        },
    )
    .await
    .map_err(map_service_err)?;

    Ok(Redirect::to(&redirect).into_response())
}

/// The "add statement" form, reached from a person or party page.
pub async fn statement_form(
    session: Option<AuthSession>,
    Query(q): Query<EntityQuery>,
) -> Result<Markup, PageError> {
    require_admin(&session)?;
    let country = q.country.unwrap_or_default();

    let content = html! {
        section class="mx-auto max-w-xl" {
            h1 class="font-serif text-3xl font-semibold tracking-tight text-ink" {
                (i18n::t("Add statement"))
            }
            form class="mt-6 space-y-4" method="post" action="/admin/statement" {
                input type="hidden" name="country" value=(country);
                @if let Some(ref p) = q.person { input type="hidden" name="person" value=(p); }
                @if let Some(ref p) = q.party { input type="hidden" name="party" value=(p); }

                div {
                    label class="block text-sm font-medium text-ink" for="text" {
                        (i18n::t("Statement"))
                    }
                    textarea name="text" id="text" rows="3" required
                        class="mt-1 block w-full border border-hairline bg-paper-raised px-3 py-2 text-sm text-ink focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent" {}
                }
                (field("url", i18n::t("Source URL"), "url", true))
                (field("outlet", i18n::t("Outlet"), "text", false))
                (field("stated_at", i18n::t("Date"), "date", false))
                label class="flex items-center gap-2 pt-1 text-sm text-ink" {
                    input type="checkbox" name="is_paraphrase"
                        class="h-4 w-4 border-[1.5px] border-ink accent-ink";
                    (i18n::t("Paraphrase in our own words"))
                }
                (ui::button::primary(i18n::t("Save")))
            }
        }
    };
    Ok(ui::layout::document(
        Some(i18n::t("Add statement")),
        true,
        true,
        content,
    ))
}

#[derive(Deserialize)]
pub struct StatementForm {
    country: String,
    person: Option<String>,
    party: Option<String>,
    text: String,
    url: String,
    outlet: Option<String>,
    stated_at: Option<String>,
    is_paraphrase: Option<String>,
}

/// Create a statement and return to the entity it is attributed to.
pub async fn statement_create(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Form(form): Form<StatementForm>,
) -> Result<Response, PageError> {
    require_admin(&session)?;

    let redirect = match (
        form.person.as_deref().filter(|s| !s.is_empty()),
        form.party.as_deref().filter(|s| !s.is_empty()),
    ) {
        (Some(s), _) => format!("/{}/people/{}", form.country, s),
        (_, Some(s)) => format!("/{}/parties/{}", form.country, s),
        _ => format!("/{}", form.country),
    };

    let stated_on = form
        .stated_at
        .as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

    db::service::statements::create(
        &pool,
        db::service::statements::CreateStatement {
            person_slug: form.person,
            party_slug: form.party,
            text: form.text,
            is_paraphrase: form.is_paraphrase.is_some(),
            stated_on,
            url: form.url,
            outlet: form.outlet,
        },
    )
    .await
    .map_err(map_service_err)?;

    Ok(Redirect::to(&redirect).into_response())
}

/// The "add poll" form, reached from a person or party page.
pub async fn poll_form(
    session: Option<AuthSession>,
    Query(q): Query<EntityQuery>,
) -> Result<Markup, PageError> {
    require_admin(&session)?;
    let country = q.country.unwrap_or_default();

    let content = html! {
        section class="mx-auto max-w-xl" {
            h1 class="font-serif text-3xl font-semibold tracking-tight text-ink" {
                (i18n::t("Add poll"))
            }
            form class="mt-6 space-y-4" method="post" action="/admin/poll" {
                input type="hidden" name="country" value=(country);
                @if let Some(ref p) = q.person { input type="hidden" name="person" value=(p); }
                @if let Some(ref p) = q.party { input type="hidden" name="party" value=(p); }

                (field("question", i18n::t("Question"), "text", true))

                div {
                    label class="block text-sm font-medium text-ink" for="kind" { (i18n::t("Poll type")) }
                    select name="kind" id="kind"
                        class="mt-1 block w-full border border-hairline bg-paper-raised px-3 py-2 text-sm text-ink focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent" {
                        option value="single" { (i18n::t("Single choice")) }
                        option value="multi" { (i18n::t("Multiple choice")) }
                        option value="yesno" { (i18n::t("Yes / No")) }
                        option value="scale" { (i18n::t("Rating scale")) }
                    }
                }

                (field("media_url", i18n::t("Question image URL"), "url", false))
                (field("media_license", i18n::t("Image license"), "text", false))

                p class="pt-2 text-xs font-bold uppercase tracking-widest text-ink-muted" {
                    (i18n::t("Options"))
                }
                // Repeated option rows, so a poll can have any number of options.
                // The "add option" button appends a row via HTMX; without JS the
                // starting rows still work.
                div id="poll-options" class="space-y-2" {
                    @for _ in 0..3 { (option_row()) }
                }
                button type="button"
                    hx-get="/admin/poll/option-row" hx-target="#poll-options" hx-swap="beforeend"
                    class="text-[11px] font-bold uppercase tracking-wide text-accent transition-colors hover:underline" {
                    "+ " (i18n::t("Add option"))
                }
                (ui::button::primary(i18n::t("Save")))
            }
        }
    };
    Ok(ui::layout::document(
        Some(i18n::t("Add poll")),
        true,
        true,
        content,
    ))
}

/// One poll-option input row: a label and an optional image URL, both using
/// repeated field names so the form can carry any number of options.
fn option_row() -> Markup {
    html! {
        div class="grid grid-cols-1 gap-2 sm:grid-cols-2" {
            input type="text" name="option" placeholder=(i18n::t("Option"))
                class="block w-full border border-hairline bg-paper-raised px-3 py-2 text-sm text-ink focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent";
            input type="url" name="option_media" placeholder=(i18n::t("image URL"))
                class="block w-full border border-hairline bg-paper-raised px-3 py-2 text-sm text-ink focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent";
        }
    }
}

/// HTMX fragment: one more option row to append to the poll form.
pub async fn poll_option_row(session: Option<AuthSession>) -> Result<Markup, PageError> {
    require_admin(&session)?;
    Ok(option_row())
}

/// Percent-decode a urlencoded form value (`+` is a space).
fn urldecode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < b.len() => match (hex_val(b[i + 1]), hex_val(b[i + 2])) {
                (Some(h), Some(l)) => {
                    out.push(h * 16 + l);
                    i += 3;
                }
                _ => {
                    out.push(b'%');
                    i += 1;
                }
            },
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Ordered (key, value) pairs from a urlencoded body, decoded. Used where a
/// form repeats a field name (the poll options), which the flat `Form`
/// extractor cannot represent.
fn form_pairs(body: &str) -> Vec<(String, String)> {
    body.split('&')
        .filter(|kv| !kv.is_empty())
        .map(|kv| {
            let mut it = kv.splitn(2, '=');
            (
                urldecode(it.next().unwrap_or("")),
                urldecode(it.next().unwrap_or("")),
            )
        })
        .collect()
}

/// Create a poll and return to it. The body is parsed by hand because the
/// options are repeated fields (`option`, `option_media`), which the flat form
/// extractor cannot carry.
pub async fn poll_create(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    body: String,
) -> Result<Response, PageError> {
    require_admin(&session)?;

    let pairs = form_pairs(&body);
    let first = |key: &str| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
    let all = |key: &str| {
        pairs
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .collect::<Vec<_>>()
    };

    let country = first("country").unwrap_or_default();

    let slug = db::service::polls::create(
        &pool,
        db::service::polls::CreatePoll {
            country_slug: Some(country.clone()),
            person_slug: first("person"),
            party_slug: first("party"),
            question: first("question").unwrap_or_default(),
            kind: first("kind").unwrap_or_default(),
            media_url: first("media_url"),
            media_license: first("media_license"),
            options: all("option"),
            option_media: all("option_media"),
        },
    )
    .await
    .map_err(map_service_err)?;

    Ok(Redirect::to(&format!("/{}/poll/{}", country, slug)).into_response())
}

/// The poll-submission review queue: proposals the automated screen has passed,
/// awaiting a human. The admin approves (creating a real poll) or rejects, and a
/// rejection may be marked a policy violation, which counts toward a ban.
pub async fn submissions(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
) -> Result<Markup, PageError> {
    require_admin(&session)?;
    let subs = db::submissions::pending_admin(&pool).await?;
    let mut rows = Vec::with_capacity(subs.len());
    for s in subs {
        let opts = db::submissions::options(&pool, s.id).await?;
        rows.push((s, opts));
    }

    let content = html! {
        section class="mx-auto max-w-3xl" {
            a href="/admin" class="text-xs text-ink-muted transition-colors hover:text-accent" {
                "← " (i18n::t("Admin panel"))
            }
            h1 class="mt-2 font-serif text-3xl font-semibold tracking-tight text-ink" {
                (i18n::t("Review poll submissions"))
            }
            p class="mt-2 max-w-prose text-sm text-ink-muted" {
                (i18n::t("Each proposal was screened automatically and waits for you. Approving creates the poll. Mark a rejection as a violation only for genuine policy breaches; repeated violations suspend the account."))
            }

            @if rows.is_empty() {
                p class="mt-8 py-10 text-center text-sm text-ink-muted" {
                    (i18n::t("No submissions to review."))
                }
            } @else {
                ul class="mt-6 space-y-4" {
                    @for (s, opts) in &rows {
                        li class="border border-hairline p-4" {
                            div class="flex flex-wrap items-center gap-x-3 gap-y-1 font-mono text-[11px] uppercase tracking-wide text-ink-muted" {
                                span class="font-bold text-ink" { (s.country_name) }
                                span { (s.kind) }
                            }
                            h2 class="mt-1 text-sm font-medium text-ink" { (s.question) }
                            @if let Some(ref sha) = s.question_sha {
                                img src={"/media/" (sha)} alt="" loading="lazy"
                                    class="mt-2 max-h-40 border border-hairline object-contain";
                            }
                            @if let Some(ref reason) = s.ai_reason {
                                p class="mt-2 text-xs text-ink-muted" {
                                    span class="font-bold uppercase tracking-wide" { (i18n::t("Automated note")) } ": " (reason)
                                }
                            }
                            @if let Some(cats) = s.ai_categories.as_ref().filter(|c| !c.is_empty()) {
                                p class="mt-1 font-mono text-[11px] text-ink-muted" { (cats.join(", ")) }
                            }
                            ul class="mt-3 space-y-1.5" {
                                @for o in opts {
                                    li class="flex items-center gap-2 text-sm text-ink" {
                                        @if let Some(ref sha) = o.asset_sha {
                                            img src={"/media/" (sha)} alt="" loading="lazy"
                                                class="h-9 w-9 border border-hairline object-cover";
                                        }
                                        span { (o.label) }
                                    }
                                }
                            }

                            div class="mt-4 flex flex-wrap items-start gap-3 border-t border-hairline-light pt-3" {
                                form method="post" action={"/admin/submissions/" (s.id) "/approve"} {
                                    button type="submit"
                                        class="border border-ink bg-ink px-4 py-1.5 text-[11px] font-bold uppercase tracking-wide text-paper transition-colors hover:border-accent hover:bg-accent" {
                                        (i18n::t("Approve"))
                                    }
                                }
                                form method="post" action={"/admin/submissions/" (s.id) "/reject"}
                                     class="flex flex-1 flex-col gap-2" {
                                    input type="text" name="note" placeholder=(i18n::t("Reason (optional)"))
                                        class="w-full border border-hairline bg-paper px-2 py-1.5 text-sm text-ink";
                                    label class="flex items-center gap-2 text-xs text-ink-muted" {
                                        input type="checkbox" name="violation" value="on";
                                        (i18n::t("Policy violation (counts toward a ban)"))
                                    }
                                    button type="submit"
                                        class="self-start border border-hairline px-4 py-1.5 text-[11px] font-bold uppercase tracking-wide text-ink-muted transition-colors hover:border-ink hover:text-ink" {
                                        (i18n::t("Reject"))
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };
    Ok(ui::layout::document(
        Some(i18n::t("Review poll submissions")),
        true,
        true,
        content,
    ))
}

/// Approve a submission: create the poll and return to the queue.
pub async fn submission_approve(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path(id): Path<i64>,
) -> Result<Redirect, PageError> {
    require_admin(&session)?;
    let admin_id = session.as_ref().map(|s| s.user_id).unwrap_or_default();
    db::submissions::approve(&pool, id, admin_id).await?;
    Ok(Redirect::to("/admin/submissions"))
}

/// The reject form: an optional reason and whether it is a policy violation.
#[derive(Deserialize)]
pub struct RejectForm {
    note: Option<String>,
    violation: Option<String>,
}

/// Reject a submission (optionally marking a violation) and return to the queue.
pub async fn submission_reject(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path(id): Path<i64>,
    Form(form): Form<RejectForm>,
) -> Result<Redirect, PageError> {
    require_admin(&session)?;
    let admin_id = session.as_ref().map(|s| s.user_id).unwrap_or_default();
    let note = form
        .note
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    db::submissions::reject(&pool, id, admin_id, note, form.violation.is_some()).await?;
    Ok(Redirect::to("/admin/submissions"))
}

#[cfg(test)]
mod tests {
    use super::{form_pairs, urldecode};

    #[test]
    fn urldecode_handles_plus_percent_and_invalid() {
        assert_eq!(urldecode("a+b"), "a b"); // plus is a space
        assert_eq!(urldecode("%C3%A7"), "ç"); // percent-encoded UTF-8
        assert_eq!(urldecode("100%25"), "100%"); // encoded percent sign
        assert_eq!(urldecode("bad%zz"), "bad%zz"); // invalid escape left intact
        assert_eq!(urldecode("plain"), "plain");
    }

    #[test]
    fn form_pairs_preserves_order_and_repeats() {
        let pairs = form_pairs("question=Q%3F&option=A&option=B&option=");
        let options: Vec<&str> = pairs
            .iter()
            .filter(|(k, _)| k == "option")
            .map(|(_, v)| v.as_str())
            .collect();
        assert_eq!(options, ["A", "B", ""]); // order and the empty trailing row kept
        assert_eq!(pairs.iter().find(|(k, _)| k == "question").unwrap().1, "Q?");
    }
}
