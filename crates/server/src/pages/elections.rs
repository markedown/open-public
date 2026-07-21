use axum::extract::{Path, Query, State};
use maud::{html, Markup};
use serde::Deserialize;

use crate::auth::AuthSession;
use crate::error::PageError;
use crate::fmt;
use crate::i18n;
use crate::ui;
use crate::ui::breadcrumb::Crumb;

#[derive(Deserialize)]
pub struct Params {
    q: Option<String>,
}

/// A localized label for an election kind.
pub(crate) fn kind_label(kind: Option<&str>) -> &'static str {
    match kind {
        Some("presidential") => i18n::t("Presidential"),
        Some("referendum") => i18n::t("Referendum"),
        Some("local") => i18n::t("Local"),
        _ => i18n::t("Parliamentary"),
    }
}

/// The elections index for a country: every election, most recent first, each
/// linking to its own page.
pub async fn list(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path(country): Path<String>,
    Query(params): Query<Params>,
) -> Result<Markup, PageError> {
    let country = db::country::get_by_slug(&pool, &country)
        .await?
        .ok_or(PageError::NotFound)?;
    let query = params.q.unwrap_or_default();
    let mut elections =
        db::elections::list_for_country(&pool, country.id, i18n::lang_code()).await?;
    elections.retain(|e| ui::search::matches(&e.name, &query));
    // An election still to come has no results to show and is the one a reader
    // is most likely looking for, so it is listed separately.
    let (upcoming, held): (Vec<_>, Vec<_>) =
        elections.iter().partition(|e| ui::election::is_upcoming(e));
    let list_url = format!("/{}/elections", country.slug);

    let content = html! {
        section {
            (ui::breadcrumb::breadcrumbs(&[
                Crumb { label: country.name.clone(), href: Some(format!("/{}", country.slug)) },
                Crumb { label: i18n::t("Elections").to_string(), href: None },
            ]))
            (ui::page_header(i18n::t("Elections"), None, None))
            (ui::search::bar(&list_url, "#elections-results", &query))

            div id="elections-results" {
                p class="mb-4 text-xs font-bold uppercase tracking-widest text-ink-muted" {
                    span class="font-mono" { (elections.len()) } " " (i18n::t("Elections"))
                }
                @if elections.is_empty() {
                    p class="py-12 text-center text-sm text-ink-muted" { (i18n::t("No elections yet.")) }
                }
                @if !upcoming.is_empty() {
                    h2 class="mb-3 text-xs font-bold uppercase tracking-widest text-ink-muted" {
                        (i18n::t("Upcoming"))
                    }
                    ul class="mb-8 grid gap-3 sm:grid-cols-2" {
                        @for e in &upcoming {
                            li {
                                a href={"/" (country.slug) "/election/" (e.slug)}
                                  class="op-card op-card-link block px-4 py-3.5" {
                                    div class="flex items-baseline justify-between gap-3" {
                                        span class="text-sm font-medium text-ink" { (e.name) }
                                        @if let Some(d) = e.held_on {
                                            span class="shrink-0 font-mono text-xs text-ink-muted" { (fmt::date(Some(d))) }
                                        }
                                    }
                                    span class="mt-1 block text-[11px] font-bold uppercase tracking-wide text-ink-muted" {
                                        (kind_label(e.kind.as_deref()))
                                    }
                                    @if let Some(note) = &e.expected_note {
                                        p class="mt-1 text-xs text-ink-muted" { (note) }
                                    }
                                }
                            }
                        }
                    }
                }
                @if !held.is_empty() {
                    ul class="grid gap-3 sm:grid-cols-2" {
                        @for e in &held {
                            li {
                                a href={"/" (country.slug) "/election/" (e.slug)}
                                  class="op-card op-card-link block px-4 py-3.5" {
                                    div class="flex items-baseline justify-between gap-3" {
                                        span class="text-sm font-medium text-ink" { (e.name) }
                                        @if let Some(d) = e.held_on {
                                            span class="shrink-0 font-mono text-xs text-ink-muted" { (fmt::date(Some(d))) }
                                        }
                                    }
                                    span class="mt-1 block text-[11px] font-bold uppercase tracking-wide text-ink-muted" {
                                        (kind_label(e.kind.as_deref()))
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
        Some(i18n::t("Elections")),
        session.is_some(),
        session.as_ref().is_some_and(|s| s.is_admin),
        content,
    ))
}

/// One election's own page: the full result breakdown plus a link to the
/// official source for details we do not reproduce here.
pub async fn detail(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path((country, slug)): Path<(String, String)>,
) -> Result<Markup, PageError> {
    let country = db::country::get_by_slug(&pool, &country)
        .await?
        .ok_or(PageError::NotFound)?;
    let mut election = db::elections::get_by_slug_in_country(&pool, &slug, country.id)
        .await?
        .ok_or(PageError::NotFound)?;
    // Overlay the election's own-words name and description in the reader's
    // language; the certified figures beside them speak for themselves and the
    // source link carries the original wording.
    let tr =
        db::translations::published_for_entity(&pool, "election", election.id, i18n::lang_code())
            .await?;
    if let Some(t) = tr.get("name") {
        election.name.clone_from(t);
    }
    if let (Some(t), Some(d)) = (tr.get("description"), election.description.as_mut()) {
        d.clone_from(t);
    }
    let rows = db::elections::results(&pool, election.id).await?;
    // The previous comparable election (same kind, held earlier) powers the
    // "last time" ghost bars and swing figures; for a runoff it is the first
    // round.
    let prev = db::elections::previous_comparable(&pool, election.id).await?;
    let source = db::elections::source_url(&pool, election.id).await?;

    let content = html! {
        article class="mx-auto max-w-2xl" {
            (ui::breadcrumb::breadcrumbs(&[
                Crumb { label: country.name.clone(), href: Some(format!("/{}", country.slug)) },
                Crumb { label: i18n::t("Elections").to_string(), href: Some(format!("/{}/elections", country.slug)) },
                Crumb { label: election.name.clone(), href: None },
            ]))
            (ui::election::election_detail(&election, &rows, prev.as_ref(), &country.slug))

            @if let Some(url) = source {
                p class="mt-4" {
                    a href=(url) rel="noopener" target="_blank"
                      class="text-xs font-medium text-accent hover:underline" {
                        (i18n::t("source")) " ↗"
                    }
                }
            }
        }
    };

    Ok(ui::layout::document(
        Some(&election.name),
        session.is_some(),
        session.as_ref().is_some_and(|s| s.is_admin),
        content,
    ))
}
