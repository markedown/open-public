use axum::extract::{Query, State};
use maud::{html, Markup};
use serde::Deserialize;

use crate::auth::AuthSession;
use crate::error::PageError;
use crate::i18n;
use crate::ui;

#[derive(Deserialize)]
pub struct Params {
    q: Option<String>,
}

/// Global search across people and parties. Result links are prefixed with the
/// primary country's slug; while the platform holds a single country this is
/// exact. A multi-country search would carry each result's own country instead.
pub async fn page(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Query(params): Query<Params>,
) -> Result<Markup, PageError> {
    let country_slug = db::country::list(&pool)
        .await?
        .first()
        .map(|c| c.slug.clone())
        .unwrap_or_default();

    let query_str = params.q.as_deref().unwrap_or("");
    let has_query = !query_str.trim().is_empty();

    let hits = if has_query {
        db::search::search(&pool, query_str, 20).await?
    } else {
        vec![]
    };

    let content = html! {
        section {
            h1 class="text-3xl font-bold tracking-tight text-ink sm:text-4xl" {
                (i18n::t("Search"))
            }

            form class="mt-6" method="get" action="/search" {
                div class="flex gap-2" {
                    input
                        type="search"
                        name="q"
                        value=(query_str)
                        placeholder=(i18n::t("Search people and parties..."))
                        class="flex-1 rounded-lg border border-hairline bg-paper-raised px-4 py-2.5 text-sm text-ink placeholder-ink-muted transition-colors focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent"
                        autofocus
                        hx-get="/search"
                        hx-trigger="keyup changed delay:250ms"
                        hx-target="#search-results"
                        hx-select="#search-results"
                        hx-include="[name='q']";
                    button
                        type="submit"
                        class="inline-flex items-center gap-1.5 rounded-lg bg-accent px-4 py-2.5 text-sm font-semibold text-white shadow-sm transition-colors hover:bg-accent-strong" {
                        svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" {
                            circle cx="11" cy="11" r="8" {}
                            path d="M21 21l-4.35-4.35" {}
                        }
                        (i18n::t("Search"))
                    }
                }
            }

            div id="search-results" class="mt-8" {
                @if has_query {
                    @if hits.is_empty() {
                        div class="flex flex-col items-center py-12 text-center" {
                            p class="text-sm text-ink-muted" {
                                (i18n::t("No results found."))
                            }
                        }
                    } @else {
                        p class="mb-4 font-mono text-xs text-ink-muted" {
                            (format!("{} {}", hits.len(), i18n::t("results")))
                        }
                        ul class="op-card divide-y divide-hairline-light" {
                            @for hit in &hits {
                                @let (href, label) = match hit.kind {
                                    domain::models::SearchKind::Person => (format!("/{}/people/{}", country_slug, hit.slug), i18n::t("Person")),
                                    domain::models::SearchKind::Party => (format!("/{}/parties/{}", country_slug, hit.slug), i18n::t("Party")),
                                };
                                li class="group" {
                                    a href=(href) class="flex items-center gap-4 px-5 py-4 transition-colors hover:bg-paper-sunken" {
                                        span class="w-14 shrink-0 font-mono text-[11px] font-medium uppercase tracking-wider text-ink-muted" {
                                            (label)
                                        }
                                        span class="text-sm font-medium text-ink transition-colors group-hover:text-accent" {
                                            (hit.name)
                                        }
                                        svg xmlns="http://www.w3.org/2000/svg" class="ml-auto h-4 w-4 shrink-0 text-hairline transition-colors group-hover:text-ink-muted" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" {
                                            path d="M9 18l6-6-6-6" {}
                                        }
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
        Some("Search"),
        session.is_some(),
        session.as_ref().is_some_and(|s| s.is_admin),
        content,
    ))
}
