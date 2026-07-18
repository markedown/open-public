use axum::extract::{Path, Query, State};
use maud::{html, Markup};
use serde::Deserialize;

use crate::auth::AuthSession;
use crate::error::PageError;
use crate::i18n;
use crate::ui;
use crate::ui::breadcrumb::Crumb;

#[derive(Deserialize)]
pub struct Params {
    q: Option<String>,
}

/// The polls index for a country: every poll with its vote count, searchable by
/// question.
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
    let polls = db::polls::list_filtered_for_country(&pool, country.id, &query).await?;
    let list_url = format!("/{}/polls", country.slug);

    // A poll is closed once its close time has passed; everything else (no close
    // time, or a future one) is open.
    let now = chrono::Utc::now();
    let (closed, open): (Vec<_>, Vec<_>) = polls
        .iter()
        .partition(|p| p.closes_at.is_some_and(|c| c <= now));

    let propose = html! {
        // Signed-in visitors can propose a poll; it is reviewed before it
        // appears. Anonymous visitors are pointed to sign in.
        a href=(if session.is_some() { format!("/{}/polls/submit", country.slug) } else { "/login".to_string() })
          class="rounded-lg bg-accent px-4 py-2 text-[12px] font-semibold text-white shadow-sm transition-colors hover:bg-accent-strong" {
            (i18n::t("Propose a poll"))
        }
    };

    let content = html! {
        section {
            (ui::breadcrumb::breadcrumbs(&[
                Crumb { label: country.name.clone(), href: Some(format!("/{}", country.slug)) },
                Crumb { label: i18n::t("Polls").to_string(), href: None },
            ]))
            (ui::page_header(
                i18n::t("Polls"),
                Some(html! { span class="font-mono" { (polls.len()) } " " (i18n::t("Polls")) }),
                Some(propose),
            ))
            (ui::search::bar(&list_url, "#polls-results", &query))

            // The search box swaps this container in place as the query changes.
            div id="polls-results" {
                @if polls.is_empty() {
                    p class="py-12 text-center text-sm text-ink-muted" { (i18n::t("No polls yet.")) }
                }
                @if !open.is_empty() {
                    (poll_group(i18n::t("Open"), &open, &country.slug))
                }
                @if !closed.is_empty() {
                    (poll_group(i18n::t("Closed"), &closed, &country.slug))
                }
            }
        }
    };

    Ok(ui::layout::document(
        Some(i18n::t("Polls")),
        session.is_some(),
        session.as_ref().is_some_and(|s| s.is_admin),
        content,
    ))
}

/// One labelled group of polls (open or closed), each row showing its kind and
/// vote count.
fn poll_group(title: &str, polls: &[&db::polls::PollListItem], country: &str) -> Markup {
    html! {
        section class="mb-10" {
            h2 class="mb-3 text-xs font-bold uppercase tracking-widest text-ink-muted" {
                (title) " " span class="font-mono" { "(" (polls.len()) ")" }
            }
            ul class="grid gap-3 sm:grid-cols-2" {
                @for p in polls {
                    li {
                        a href={"/" (country) "/poll/" (p.slug)}
                          class="op-card op-card-link block px-4 py-3.5" {
                            div class="flex items-baseline justify-between gap-3" {
                                span class="text-sm font-medium text-ink" { (p.question) }
                                span class="flex shrink-0 items-baseline gap-3 font-mono text-xs text-ink-muted" {
                                    span class="uppercase tracking-wide" { (kind_label(&p.kind)) }
                                    span { (p.votes) " " (i18n::t("votes")) }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A short localized label for a poll kind, shown on the index rows.
fn kind_label(kind: &str) -> &str {
    match kind {
        "multi" => i18n::t("Multiple choice"),
        "yesno" => i18n::t("Yes / No"),
        "scale" => i18n::t("Rating scale"),
        _ => i18n::t("Single choice"),
    }
}
