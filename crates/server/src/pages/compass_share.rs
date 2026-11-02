//! Shareable, result-only links for the party compass.
//!
//! A visitor can share the ranking their compass produced without sharing the
//! answers behind it. The link carries the ranking itself: each ranked party's
//! slug, percentage and matched-position count, signed with the application
//! secret so nobody can forge or alter a result. Nothing is written when a link
//! is made or read, and the visitor's answers, which are never stored, are not
//! in the token either. A token that fails to verify is a plain not-found: the
//! page never says which check failed, because a broken link and a forged one
//! deserve the same answer.

use axum::extract::{Path, State};
use axum::http::{HeaderName, HeaderValue};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use hmac::{Hmac, Mac};
use maud::html;
use serde::Deserialize;
use sha2::Sha256;

use crate::auth::AuthSession;
use crate::error::PageError;
use crate::i18n;
use crate::state::AppState;
use crate::ui;

use super::compass::{methodology_note, score_row, Contestant};
use domain::compass;

type HmacSha256 = Hmac<Sha256>;

/// The longest token worth reading. A real token is a few hundred bytes; the
/// cap keeps a hostile URL away from the parser.
const MAX_TOKEN_LEN: usize = 4096;

/// Bytes of the HMAC-SHA256 kept as the tag. 128 bits is far past any forgery
/// attempt against a compass result, and a short tag keeps the URL shareable.
const TAG_LEN: usize = 16;

/// Rows one token may carry. The compass ranks a country's parties, which is
/// far below this; the bound keeps a token from turning into a page-sized
/// payload.
const MAX_ROWS: usize = 40;

/// Parties one row may name: the largest group that shares exactly the same
/// evidence is far smaller, so this only bounds a hostile token.
const MAX_MERGED: usize = 8;

/// The token payload. Decoding is strict: unknown or missing fields, wrong
/// types, or row entries that are not exactly `[slug, percent, matched]` all
/// fail, so the token cannot smuggle a shape this page does not understand.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    v: u8,
    c: String,
    /// When the link was made. The page does not read it (a result does not
    /// expire), but it is part of the signed shape, so the key is required.
    #[serde(rename = "t")]
    _created_at: i64,
    r: Vec<(String, u32, u32)>,
}

/// The ranking a share token carries, in rank order.
pub struct SharedResult {
    pub rows: Vec<(String, u32, u32)>,
}

/// The payload text for a ranking. A row names the party it is scored under
/// and, joined with `+`, the parties folded into the same row.
fn payload_json(country_slug: &str, created_at: i64, rows: &[(String, u32, u32)]) -> String {
    serde_json::json!({ "v": 1, "c": country_slug, "t": created_at, "r": rows }).to_string()
}

/// The tag: the first [`TAG_LEN`] bytes of the HMAC-SHA256 of the payload.
fn tag(secret: &[u8], payload: &[u8]) -> Option<Vec<u8>> {
    let mut mac = HmacSha256::new_from_slice(secret).ok()?;
    mac.update(payload);
    Some(mac.finalize().into_bytes()[..TAG_LEN].to_vec())
}

/// A share token for a ranking: `<payload>.<tag>`, both base64url without
/// padding. Stateless by construction; the secret is the only thing the link
/// depends on beyond the ranking itself.
pub fn encode(secret: &[u8], country_slug: &str, rows: &[(String, u32, u32)]) -> String {
    let payload = payload_json(country_slug, chrono::Utc::now().timestamp(), rows);
    let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    // An empty secret is rejected by config, so the fallback only guards
    // against a misconfigured instance; its token then fails to verify.
    let tag = tag(secret, payload.as_bytes()).unwrap_or_default();
    format!("{}.{}", engine.encode(payload), engine.encode(tag))
}

/// Read a share token: verify the signature, then parse the payload, then
/// check it names this country and stays within the bounds the compass can
/// produce. `None` means "not found", with the reason kept to ourselves.
pub fn decode(secret: &[u8], token: &str, country_slug: &str) -> Option<SharedResult> {
    // The token is base64url, so bytes and characters coincide.
    if token.len() > MAX_TOKEN_LEN {
        return None;
    }
    let (payload_b64, tag_b64) = token.split_once('.')?;
    let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let payload = engine.decode(payload_b64).ok()?;
    let tag = engine.decode(tag_b64).ok()?;
    if tag.len() != TAG_LEN {
        return None;
    }
    // The signature first: a forged payload never reaches the parser. The
    // comparison is the hmac crate's constant-time truncated check, not `==`.
    let mut mac = HmacSha256::new_from_slice(secret).ok()?;
    mac.update(&payload);
    mac.verify_truncated_left(&tag).ok()?;

    let parsed: Payload = serde_json::from_slice(&payload).ok()?;
    if parsed.v != 1
        || parsed.c != country_slug
        || parsed.r.len() > MAX_ROWS
        || parsed.r.iter().any(|(slug, percent, _)| {
            *percent > 100 || slug.is_empty() || slug.split('+').count() > MAX_MERGED
        })
    {
        return None;
    }
    Some(SharedResult { rows: parsed.r })
}

/// GET `/{country}/compass/shared/{token}`: the ranking from a share token.
/// The parties are looked up by slug at view time, scoped to the country, so a
/// party dissolved since the link was made simply drops out of the page. The
/// page is a shared artifact rather than a place to be found, so it asks
/// crawlers to keep it out of their index.
pub async fn shared(
    State(state): State<AppState>,
    session: Option<AuthSession>,
    Path((country, token)): Path<(String, String)>,
) -> Result<Response, PageError> {
    let country = db::country::get_by_slug(&state.pool, &country)
        .await?
        .ok_or(PageError::NotFound)?;
    let result = decode(&state.secret, &token, &country.slug).ok_or(PageError::NotFound)?;

    let mut rows = Vec::new();
    for (row, percent, matched) in result.rows {
        // The first slug is the party the row is scored under, the rest are the
        // parties folded into it. One that no longer exists drops out.
        let mut contestant: Option<Contestant> = None;
        for slug in row.split('+') {
            if let Some(party) =
                db::parties::get_by_slug_in_country(&state.pool, slug, country.id).await?
            {
                let next = Contestant::from_party(&party, &country.slug);
                match contestant.as_mut() {
                    Some(c) => c.absorb(next),
                    None => contestant = Some(next),
                }
            }
        }
        if let Some(contestant) = contestant {
            rows.push((
                // The row component reads only the percentage and the matched
                // count; the id is not part of a shared result.
                compass::ContestantScore {
                    contestant_id: 0,
                    percent: f64::from(percent),
                    matched,
                },
                contestant,
            ));
        }
    }

    let compass_url = format!("/{}/compass", country.slug);
    let content = html! {
        section class="mx-auto max-w-3xl" {
            (ui::breadcrumb::breadcrumbs(&[
                ui::breadcrumb::Crumb { label: i18n::t_dyn(&country.name).to_string(), href: Some(format!("/{}", country.slug)) },
                ui::breadcrumb::Crumb { label: i18n::t("Compass").to_string(), href: Some(compass_url.clone()) },
                ui::breadcrumb::Crumb { label: i18n::t("A shared compass result").to_string(), href: None },
            ]))
            (ui::page_header(i18n::t("A shared compass result"), None, None))
            p class="mb-6 max-w-prose text-[16px] text-ink-muted" {
                (i18n::t("This is a shared result. It shows percentages only, not the answers behind them."))
            }

            ol class="space-y-2.5" {
                @for (rank, (score, contestant)) in rows.iter().enumerate() {
                    (score_row(rank + 1, score, contestant))
                }
            }
            (methodology_note(db::compass::SCOPE_PARTY))

            div class="mt-8 border-t border-hairline pt-5" {
                a href=(compass_url) class="text-[16px] font-semibold text-link hover:underline" {
                    (i18n::t("Take the compass yourself"))
                }
            }
        }
    };

    Ok((
        [(
            HeaderName::from_static("x-robots-tag"),
            HeaderValue::from_static("noindex, nofollow"),
        )],
        ui::layout::document(
            Some(i18n::t("A shared compass result")),
            session.is_some(),
            session.as_ref().is_some_and(|s| s.is_admin),
            content,
        ),
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"a-server-secret-of-some-length!!";
    const OTHER_SECRET: &[u8] = b"a-different-secret-different!";

    fn rows() -> Vec<(String, u32, u32)> {
        vec![
            ("test-partisi".to_string(), 100, 2),
            ("diger-parti".to_string(), 0, 2),
        ]
    }

    /// A token over arbitrary payload text, signed with `secret`, for crafting
    /// tokens that `encode` would never produce.
    fn signed(secret: &[u8], payload: &str) -> String {
        let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let tag = tag(secret, payload.as_bytes()).expect("a non-empty key");
        format!("{}.{}", engine.encode(payload), engine.encode(tag))
    }

    /// The same token with its first payload character replaced by another
    /// valid base64url character: decodes, but to different bytes.
    fn tamper_payload(token: &str) -> String {
        let mut chars: Vec<char> = token.chars().collect();
        chars[0] = if chars[0] == 'B' { 'C' } else { 'B' };
        chars.into_iter().collect()
    }

    #[test]
    fn a_token_round_trips_to_the_same_ranking() {
        let token = encode(SECRET, "tr", &rows());
        let got = decode(SECRET, &token, "tr").expect("a fresh token is valid");
        assert_eq!(got.rows, rows());
    }

    #[test]
    fn a_tampered_payload_fails_the_signature() {
        let token = encode(SECRET, "tr", &rows());
        assert!(decode(SECRET, &tamper_payload(&token), "tr").is_none());
    }

    #[test]
    fn a_tampered_tag_is_rejected() {
        let token = encode(SECRET, "tr", &rows());
        let mut chars: Vec<char> = token.chars().collect();
        let last = chars.len() - 1;
        chars[last] = if chars[last] == 'A' { 'B' } else { 'A' };
        let tampered: String = chars.into_iter().collect();
        assert!(decode(SECRET, &tampered, "tr").is_none());
    }

    #[test]
    fn a_token_signed_with_another_secret_is_rejected() {
        let token = encode(OTHER_SECRET, "tr", &rows());
        assert!(decode(SECRET, &token, "tr").is_none());
    }

    #[test]
    fn a_truncated_token_is_rejected() {
        let token = encode(SECRET, "tr", &rows());
        assert!(decode(SECRET, &token[..token.len() - 4], "tr").is_none());
        // And one with no dot at all.
        assert!(decode(SECRET, "notatoken", "tr").is_none());
    }

    #[test]
    fn an_oversized_token_is_rejected_without_parsing_it() {
        let token = encode(SECRET, "tr", &rows());
        let long = format!("{token}{}", "A".repeat(4096));
        assert!(decode(SECRET, &long, "tr").is_none());
    }

    #[test]
    fn a_payload_of_another_version_is_rejected() {
        let token = signed(SECRET, r#"{"v":2,"c":"tr","t":1,"r":[["p",50,1]]}"#);
        assert!(decode(SECRET, &token, "tr").is_none());
    }

    #[test]
    fn a_payload_with_too_many_rows_is_rejected() {
        let entries: Vec<String> = (0..=MAX_ROWS as u32)
            .map(|i| format!("[\"p{i}\",50,1]"))
            .collect();
        let payload = format!(
            "{{\"v\":1,\"c\":\"tr\",\"t\":1,\"r\":[{}]}}",
            entries.join(",")
        );
        assert!(decode(SECRET, &signed(SECRET, &payload), "tr").is_none());
    }

    #[test]
    fn a_row_may_name_the_parties_folded_into_it() {
        let rows = vec![("a-party+b-party".to_string(), 80, 3)];
        let token = encode(SECRET, "tr", &rows);
        assert_eq!(decode(SECRET, &token, "tr").unwrap().rows, rows);
    }

    #[test]
    fn a_row_naming_too_many_parties_or_none_is_rejected() {
        let many = (0..=MAX_MERGED)
            .map(|i| format!("p{i}"))
            .collect::<Vec<_>>()
            .join("+");
        for slug in [many, String::new()] {
            let token = encode(SECRET, "tr", &[(slug, 50, 1)]);
            assert!(decode(SECRET, &token, "tr").is_none());
        }
    }

    #[test]
    fn a_percent_over_100_is_rejected() {
        let token = signed(SECRET, r#"{"v":1,"c":"tr","t":1,"r":[["p",101,1]]}"#);
        assert!(decode(SECRET, &token, "tr").is_none());
    }

    #[test]
    fn a_payload_that_is_not_exactly_the_shape_is_rejected() {
        // An extra field, a missing field, and a row that is not a
        // [slug, percent, matched] triple.
        for payload in [
            r#"{"v":1,"c":"tr","t":1,"r":[],"x":1}"#,
            r#"{"v":1,"c":"tr","r":[["p",50,1]]}"#,
            r#"{"v":1,"c":"tr","t":1,"r":[["p",50]}"#,
            r#"{"v":1,"c":"tr","t":1,"r":[[50,"p",1]]}"#,
        ] {
            let token = signed(SECRET, payload);
            assert!(decode(SECRET, &token, "tr").is_none(), "{payload}");
        }
    }

    #[test]
    fn a_token_signed_for_one_country_is_not_read_under_another() {
        let token = encode(SECRET, "tr", &rows());
        assert!(decode(SECRET, &token, "de").is_none());
    }
}
