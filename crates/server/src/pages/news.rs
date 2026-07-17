use axum::extract::{Path, State};
use maud::{html, Markup};

use crate::auth::AuthSession;
use crate::error::PageError;
use crate::fmt;
use crate::i18n;
use crate::ui;
use crate::ui::breadcrumb::Crumb;

/// The country-wide news index: recent sourced news, newest first, each linking
/// out to its source and to the people and parties it mentions.
pub async fn list(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path(country): Path<String>,
) -> Result<Markup, PageError> {
    let country = db::country::get_by_slug(&pool, &country)
        .await?
        .ok_or(PageError::NotFound)?;
    let is_admin = session.as_ref().is_some_and(|s| s.is_admin);
    let items = db::news::recent(&pool, country.id, i18n::lang_code(), 60).await?;

    let content = html! {
        section {
            (ui::breadcrumb::breadcrumbs(&[
                Crumb { label: country.name.clone(), href: Some(format!("/{}", country.slug)) },
                Crumb { label: i18n::t("News").to_string(), href: None },
            ]))
            header class="mb-8 border-b-2 border-accent pb-4" {
                div class="flex items-baseline justify-between gap-3" {
                    h1 class="font-serif text-4xl font-semibold tracking-tight text-ink" {
                        (i18n::t("News"))
                    }
                    a href={"/" (country.slug) "/outlets"}
                      class="text-[11px] font-bold uppercase tracking-wide text-accent transition-colors hover:underline" {
                        (i18n::t("Outlets")) " →"
                    }
                }
                p class="mt-2 text-xs font-bold uppercase tracking-widest text-ink-muted" {
                    span class="font-mono" { (items.len()) } " " (i18n::t("News"))
                }
            }

            @if items.is_empty() {
                p class="py-12 text-center text-sm text-ink-muted" { (i18n::t("No news yet.")) }
            } @else {
                (ui::news::index(&items, &country.slug, is_admin))
            }
        }
    };

    Ok(ui::layout::document(
        Some(i18n::t("News")),
        session.is_some(),
        session.as_ref().is_some_and(|s| s.is_admin),
        content,
    ))
}

/// One news item's own page: the outlet that published it (with logo), the
/// people and parties it mentions (linked into the platform), our summary, the
/// author, and a link to the article at the source. The article body is never
/// stored or shown here.
pub async fn detail(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path((country, id)): Path<(String, i64)>,
) -> Result<Markup, PageError> {
    let country = db::country::get_by_slug(&pool, &country)
        .await?
        .ok_or(PageError::NotFound)?;
    let item = db::news::get_detail(&pool, id)
        .await?
        .ok_or(PageError::NotFound)?;
    let is_admin = session.as_ref().is_some_and(|s| s.is_admin);
    let loc = crate::content::Localized::load(&pool, "news_item", item.id).await?;
    // The headline shows in the reader's language; the original headline stays
    // reachable through the "read at the source" link, so it needs no separate
    // disclosure. Our own summary carries one, like other prose.
    let headline = loc
        .get("headline", Some(item.headline.as_str()))
        .unwrap_or(&item.headline);
    let summary = loc.get("our_summary", item.our_summary.as_deref());

    let content = html! {
        article class="max-w-2xl" {
            (ui::breadcrumb::breadcrumbs(&[
                Crumb { label: country.name.clone(), href: Some(format!("/{}", country.slug)) },
                Crumb { label: i18n::t("News").to_string(), href: Some(format!("/{}/news", country.slug)) },
                Crumb { label: headline.to_string(), href: None },
            ]))

            header class={"mb-6 border-[1.5px] border-ink bg-paper-raised p-6 sm:p-8 " (ui::CORNER_TICK)} {
                // The outlet that published it.
                @if let Some(ref o) = item.outlet {
                    a href={"/" (country.slug) "/outlet/" (o.slug)}
                      class="mb-4 inline-flex items-center gap-2 transition-opacity hover:opacity-80" {
                        @if let Some(ref logo) = o.logo_url {
                            img src=(logo) alt="" loading="lazy"
                                class="h-6 w-6 border border-hairline object-contain";
                        }
                        span class="text-xs font-bold uppercase tracking-widest text-ink-muted" { (o.name) }
                    }
                }
                h1 class="font-serif text-2xl font-semibold leading-snug tracking-tight text-ink sm:text-3xl" {
                    (headline)
                }
                div class="mt-3 flex flex-wrap items-center gap-x-4 gap-y-1 font-mono text-xs text-ink-muted" {
                    @if let Some(ref a) = item.author {
                        span { (i18n::t("By")) " " span class="text-ink" { (a) } }
                    }
                    @if let Some(d) = item.published_at {
                        span { (fmt::date(Some(d.date_naive()))) }
                    }
                }
                @if is_admin {
                    a href={"/admin/news/" (item.id) "/edit"}
                      class="mt-4 inline-block font-mono text-[10px] font-bold uppercase tracking-wide text-accent hover:underline" {
                        (i18n::t("Edit"))
                    }
                }
            }

            @if let Some(text) = summary {
                div class="mb-6" {
                    p class="max-w-prose text-[15px] leading-relaxed text-ink" { (text) }
                    (ui::translated::original_disclosure(
                        loc.is_translated("our_summary").then_some(item.our_summary.as_deref()).flatten(),
                    ))
                }
            }

            @if !item.people.is_empty() || !item.parties.is_empty() {
                section class="mb-6" {
                    h2 class="mb-2 text-[10px] font-bold uppercase tracking-widest text-ink-muted" {
                        (i18n::t("Mentions"))
                    }
                    (ui::news::mentions(&item.people, &item.parties, &country.slug))
                }
            }

            a href=(item.url) rel="noopener" target="_blank"
              class="inline-flex items-center gap-1.5 border border-ink px-4 py-2 text-[11px] font-bold uppercase tracking-wide text-ink transition-colors hover:border-accent hover:text-accent" {
                (i18n::t("Read at the source")) " ↗"
            }
        }
    };

    Ok(ui::layout::document(
        Some(headline),
        session.is_some(),
        is_admin,
        content,
    ))
}
