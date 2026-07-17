use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::{IntoResponse, Redirect, Response},
    Json,
};
use maud::{html, Markup};
use serde::Serialize;

use crate::auth::AuthSession;
use crate::error::PageError;
use crate::i18n;
use crate::state::AppState;
use crate::ui::{self, breadcrumb::Crumb, poll_widget::Viewer};

pub async fn detail(
    State(state): State<AppState>,
    session: Option<AuthSession>,
    Path((country, slug)): Path<(String, String)>,
) -> Result<Markup, PageError> {
    let country_model = db::country::get_by_slug(&state.pool, &country)
        .await?
        .ok_or(PageError::NotFound)?;

    let mut poll = db::polls::get_by_slug_in_country(&state.pool, &slug, country_model.id)
        .await?
        .ok_or(PageError::NotFound)?;
    crate::content::localize_poll(&state.pool, &mut poll).await?;

    let viewer = viewer_for(&state, session.as_ref(), poll.id).await?;
    let chain = db::polls::chain_head(&state.pool, poll.id).await?;

    Ok(ui::layout::document(
        Some(&poll.question),
        session.is_some(),
        session.as_ref().is_some_and(|s| s.is_admin),
        html! {
            section class="mx-auto max-w-xl" {
                (ui::breadcrumb::breadcrumbs(&[
                    Crumb { label: country_model.name.clone(), href: Some(format!("/{}", country_model.slug)) },
                    Crumb { label: i18n::t("Polls").to_string(), href: Some(format!("/{}/polls", country_model.slug)) },
                    Crumb { label: poll.question.clone(), href: None },
                ]))
                (ui::poll_widget::poll_widget(&poll, viewer, &country_model.slug))

                @if let Some(ref head) = chain {
                    div class="mt-8" {
                        div class="flex flex-wrap items-baseline gap-x-2 gap-y-1 font-mono text-xs text-ink-muted" {
                            span class="font-semibold uppercase tracking-wide text-ink" {
                                (i18n::t("Integrity"))
                            }
                            span { "#" (head.head_seq) }
                            span { (hex_prefix(&head.head_hash)) "…" }
                            a href={"/" (country_model.slug) "/poll/" (poll.slug) "/chain"}
                              class="text-accent hover:underline" {
                                (i18n::t("Verify"))
                            }
                        }
                        p class="mt-2 max-w-prose text-xs text-ink-muted" {
                            (i18n::t("This fingerprint lets anyone confirm no vote was altered or removed after casting."))
                        }
                    }
                }
            }
        },
    ))
}

/// The chain head as JSON, so the running fingerprint can be checked against a
/// published dump.
#[derive(Serialize)]
pub struct ChainResponse {
    slug: String,
    votes: i64,
    head_hash: String,
}

pub async fn chain(
    State(state): State<AppState>,
    Path((_country, slug)): Path<(String, String)>,
) -> Result<Json<ChainResponse>, PageError> {
    let poll = db::polls::get_by_slug(&state.pool, &slug)
        .await?
        .ok_or(PageError::NotFound)?;
    let head = db::polls::chain_head(&state.pool, poll.id).await?;
    let (votes, head_hash) = match head {
        Some(h) => (h.head_seq, hex(&h.head_hash)),
        None => (0, String::new()),
    };
    Ok(Json(ChainResponse {
        slug,
        votes,
        head_hash,
    }))
}

pub async fn vote(
    session: AuthSession,
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((country, slug)): Path<(String, String)>,
    body: String,
) -> Result<Response, PageError> {
    let poll = db::polls::get_by_slug(&state.pool, &slug)
        .await?
        .ok_or(PageError::NotFound)?;

    // The chosen option ids from the urlencoded body: one field for
    // single-choice, several repeated `option_id` fields for multi-select.
    // Values are integers, so no percent-unescaping is needed. Ids that do not
    // belong to this poll are dropped.
    let chosen: Vec<i64> = body
        .split('&')
        .filter_map(|kv| kv.strip_prefix("option_id="))
        .filter_map(|v| v.parse::<i64>().ok())
        .filter(|id| poll.options.iter().any(|o| o.id == *id))
        .collect();

    if poll.kind == "multi" {
        if !chosen.is_empty() {
            db::polls::cast_votes(&state.pool, poll.id, &chosen, session.user_id).await?;
        }
    } else if let Some(&option_id) = chosen.first() {
        db::polls::cast_vote(&state.pool, poll.id, option_id, session.user_id).await?;
    }

    if headers.contains_key("hx-request") {
        let mut poll = db::polls::get_by_slug(&state.pool, &slug)
            .await?
            .ok_or(PageError::NotFound)?;
        crate::content::localize_poll(&state.pool, &mut poll).await?;
        let viewer = viewer_for(&state, Some(&session), poll.id).await?;
        Ok(ui::poll_widget::poll_widget(&poll, viewer, &country).into_response())
    } else {
        Ok(Redirect::to(&format!("/{}/poll/{}", country, slug)).into_response())
    }
}

async fn viewer_for(
    state: &AppState,
    session: Option<&AuthSession>,
    poll_id: i64,
) -> Result<Viewer, PageError> {
    match session {
        None => Ok(Viewer::Anonymous),
        Some(s) => {
            let voted = db::polls::has_voted(&state.pool, poll_id, s.user_id).await?;
            Ok(if voted {
                Viewer::Voted
            } else {
                Viewer::CanVote
            })
        }
    }
}

/// Lowercase hex of a byte slice.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// The first eight bytes of a hash as hex, for the short fingerprint.
fn hex_prefix(bytes: &[u8]) -> String {
    hex(&bytes[..bytes.len().min(8)])
}
