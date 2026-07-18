use axum::extract::State;
use axum::response::{IntoResponse, Redirect, Response};
use maud::html;

use crate::auth::AuthSession;
use crate::error::PageError;
use crate::fmt;
use crate::i18n;
use crate::state::AppState;
use crate::ui;

const FEED_LIMIT: i64 = 40;

/// A short localized tag for a feed item kind.
fn kind_label(kind: &str) -> &'static str {
    match kind {
        "poll" => i18n::t("Poll"),
        "election" => i18n::t("Election"),
        _ => i18n::t("News"),
    }
}

/// The signed-in visitor's personal feed: recent polls, news and elections about
/// the people, parties and countries they follow. Anonymous visitors are sent to
/// sign in.
pub async fn page(
    session: Option<AuthSession>,
    State(state): State<AppState>,
) -> Result<Response, PageError> {
    let Some(session) = session else {
        return Ok(Redirect::to("/login").into_response());
    };

    let following = db::follows::count_for_user(&state.pool, session.user_id).await?;
    let items = db::follows::feed(&state.pool, session.user_id, FEED_LIMIT).await?;

    let content = html! {
        section {
            (ui::page_header(
                i18n::t("Feed"),
                Some(html! {
                    span class="font-mono" { (following) } " " (i18n::t("followed"))
                }),
                None,
            ))

            @if items.is_empty() {
                div class="op-card p-10 text-center" {
                    p class="mx-auto max-w-prose text-sm text-ink-muted" {
                        @if following == 0 {
                            (i18n::t("Follow people, parties and countries to see new polls, news and elections here."))
                        } @else {
                            (i18n::t("Nothing new from what you follow yet."))
                        }
                    }
                    a href="/" class="mt-4 inline-flex rounded-lg bg-accent px-4 py-2 text-[12px] font-semibold text-white shadow-sm transition-colors hover:bg-accent-strong" {
                        (i18n::t("Explore"))
                    }
                }
            } @else {
                ul class="space-y-3" {
                    @for it in &items {
                        li {
                            a href=(it.href) class="op-card op-card-link flex items-baseline gap-3 p-4" {
                                span class="shrink-0 rounded-full bg-accent-tint px-2 py-0.5 text-[10px] font-bold uppercase tracking-wide text-accent" {
                                    (kind_label(&it.kind))
                                }
                                span class="grow text-sm font-medium text-ink" { (it.title) }
                                @if let Some(d) = it.occurred_at {
                                    span class="shrink-0 font-mono text-[11px] text-ink-muted" {
                                        (fmt::date(Some(d.date_naive())))
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };

    Ok(
        ui::layout::document(Some(i18n::t("Feed")), true, session.is_admin, content)
            .into_response(),
    )
}
