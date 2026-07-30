use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Form,
};
use base64::Engine;
use maud::{html, Markup};
use serde::Deserialize;

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
    let chain = db::voting::ballot_chain_head(&state.pool, poll.id).await?;

    // Only a viewer who can vote on an open poll needs the issuer public key and
    // the island. Generating the key here (idempotent) means it exists by the
    // time anyone can vote; the island blinds a token against it.
    let pubkey =
        if matches!(viewer, Viewer::CanVote) && db::polls::is_open(&state.pool, poll.id).await? {
            crate::voting::ensure_issuer_key(&state.pool, poll.id)
                .await
                .map_err(|_| PageError::Server)?;
            db::voting::public_key(&state.pool, poll.id)
                .await?
                .map(|der| base64::engine::general_purpose::STANDARD.encode(der))
        } else {
            None
        };

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
                (ui::poll_widget::poll_widget(&poll, viewer, &country_model.slug, pubkey.as_deref()))

                // The anonymous-voting island: blinds a token, has it signed, and
                // casts the ballot, all in the browser. Loaded only where a vote
                // can happen.
                @if pubkey.is_some() {
                    script type="module" src="/static/vote.min.js" defer {}
                }

                // The ballot-chain fingerprint: anyone can check it against the
                // published dump to confirm no ballot was altered or removed.
                @if let Some((seq, ref head)) = chain {
                    div class="mt-8" {
                        div class="flex flex-wrap items-baseline gap-x-2 gap-y-1 font-mono text-xs text-ink-muted" {
                            span class="font-semibold uppercase tracking-wide text-ink" {
                                (i18n::t("Integrity"))
                            }
                            span { "#" (seq) }
                            span { (hex_prefix(head)) "…" }
                            a href="/data/polls.json" class="text-accent hover:underline" {
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

/// The blinded token request a client submits to be blind-signed. `blinded` is
/// base64 of the client's blinded message (it hides the real token from us).
#[derive(Deserialize)]
pub struct TokenRequest {
    blinded: String,
}

/// Issue one blind-signed voting token to a verified account for a poll. The
/// server signs the blinded message without ever seeing the token inside it, and
/// records the entitlement (one per account per poll). It learns that the account
/// participated, never how it will vote, and cannot link the two. See
/// docs/anonymous-voting.md.
pub async fn issue_token(
    session: AuthSession,
    State(state): State<AppState>,
    Path((_country, slug)): Path<(String, String)>,
    Form(req): Form<TokenRequest>,
) -> Result<Response, PageError> {
    let poll = db::polls::get_by_slug(&state.pool, &slug)
        .await?
        .ok_or(PageError::NotFound)?;
    if !db::polls::is_open(&state.pool, poll.id).await? {
        return Ok(StatusCode::CONFLICT.into_response());
    }

    // Validate the blinded message before consuming the entitlement: it must be
    // base64 and exactly the RSA-2048 modulus length. A malformed request must
    // not burn the account's single token.
    let Ok(blinded) = base64::engine::general_purpose::STANDARD.decode(req.blinded.trim()) else {
        return Ok(StatusCode::BAD_REQUEST.into_response());
    };
    if blinded.len() != crate::voting::MODULUS_BYTES {
        return Ok(StatusCode::BAD_REQUEST.into_response());
    }

    crate::voting::ensure_issuer_key(&state.pool, poll.id)
        .await
        .map_err(|_| PageError::Server)?;
    let Some(private_key) = db::voting::private_key(&state.pool, poll.id).await? else {
        // No usable key (poll closed and key destroyed).
        return Ok(StatusCode::CONFLICT.into_response());
    };

    // The entitlement insert is the atomic one-token-per-account gate.
    if !db::voting::record_entitlement(&state.pool, poll.id, session.user_id).await? {
        return Ok(StatusCode::CONFLICT.into_response());
    }

    match crate::voting::blind_sign(&private_key, &blinded) {
        Ok(blind_sig) => {
            let body = base64::engine::general_purpose::STANDARD.encode(blind_sig);
            Ok(([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], body).into_response())
        }
        Err(e) => {
            tracing::error!(?e, "blind sign failed");
            // Give the token back so the account can retry.
            let _ = db::voting::delete_entitlement(&state.pool, poll.id, session.user_id).await;
            Err(PageError::Server)
        }
    }
}

/// Cast an anonymous ballot by spending a blind-signed token. No account is
/// involved: the token proves the voter is eligible, and the ballot cannot be
/// linked to whoever the token was issued to. `token`, `signature`, and the
/// optional `randomizer` are URL-safe base64 (no padding); `option_id` fields
/// carry the chosen options. See docs/anonymous-voting.md.
pub async fn cast(
    State(state): State<AppState>,
    Path((country, slug)): Path<(String, String)>,
    body: String,
) -> Result<Response, PageError> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    let poll = db::polls::get_by_slug(&state.pool, &slug)
        .await?
        .ok_or(PageError::NotFound)?;
    if !db::polls::is_open(&state.pool, poll.id).await? {
        return Ok(StatusCode::CONFLICT.into_response());
    }

    let (mut token, mut signature, mut randomizer) = (None, None, None);
    let mut options: Vec<i64> = Vec::new();
    for kv in body.split('&') {
        let Some((key, value)) = kv.split_once('=') else {
            continue;
        };
        match key {
            "token" => token = URL_SAFE_NO_PAD.decode(value).ok(),
            "signature" => signature = URL_SAFE_NO_PAD.decode(value).ok(),
            "randomizer" => randomizer = URL_SAFE_NO_PAD.decode(value).ok(),
            "option_id" => {
                if let Ok(id) = value.parse::<i64>() {
                    if poll.options.iter().any(|o| o.id == id) {
                        options.push(id);
                    }
                }
            }
            _ => {}
        }
    }

    let (Some(token), Some(signature)) = (token, signature) else {
        return Ok(StatusCode::BAD_REQUEST.into_response());
    };
    // A single-choice poll records one option; multi records all chosen.
    if poll.kind != "multi" {
        options.truncate(1);
    }
    if options.is_empty() {
        return Ok(StatusCode::BAD_REQUEST.into_response());
    }

    // The token must be validly signed by this poll's issuer key.
    let Some(public_key) = db::voting::public_key(&state.pool, poll.id).await? else {
        return Ok(StatusCode::CONFLICT.into_response());
    };
    if !crate::voting::verify_token(&public_key, &token, &signature, randomizer.as_deref()) {
        return Ok(StatusCode::FORBIDDEN.into_response());
    }

    match db::voting::cast_ballot(
        &state.pool,
        poll.id,
        &token,
        &signature,
        randomizer.as_deref(),
        &options,
    )
    .await?
    {
        db::voting::CastOutcome::Cast { .. } => {
            Ok(Redirect::to(&format!("/{country}/poll/{slug}")).into_response())
        }
        // The token was already spent: the ballot stands, this is a no-op.
        db::voting::CastOutcome::AlreadySpent => Ok(StatusCode::CONFLICT.into_response()),
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
            // Anonymous ballots cannot be linked to an account, so "have you
            // voted" is derived from the entitlement (were you issued a token),
            // not from any ballot. A visitor who has their token has taken part.
            let taken_part = db::voting::has_entitlement(&state.pool, poll_id, s.user_id).await?;
            Ok(if taken_part {
                Viewer::Voted
            } else {
                Viewer::CanVote
            })
        }
    }
}

/// The first eight bytes of a hash as hex, for the short fingerprint.
fn hex_prefix(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().take(8).fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}
