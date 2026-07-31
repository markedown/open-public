//! The admin ingest API: the authenticated, machine-to-machine path for
//! editorial content to enter an instance. See docs/admin-api-and-news.md.
//!
//! It writes editorial data only. It has no endpoint that touches a vote, a
//! ballot, an entitlement, a tally or a chain, and it never will: poll results
//! move only through the anonymous, append-only voting path, by voters. That is
//! the one thing the admin path must not be able to do, and it is enforced by
//! there being no such handler here (asserted by a test).
//!
//! Authentication is a single admin bearer key (`ADMIN_API_KEY`). Without it, or
//! with the wrong one, every endpoint answers 404: the surface does not announce
//! itself, the same treatment as the admin pages. With no key configured the API
//! is disabled entirely.

use axum::extract::{Request, State};
use axum::http::{header::AUTHORIZATION, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

/// The `/api/v1` routes, gated by the bearer-key check. Merged into the app as
/// its own router so it carries none of the HTML layers (locale, the refusal
/// pages): the API speaks JSON.
pub fn routes(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/sources", post(upsert_source))
        .route("/api/v1/news", post(upsert_news))
        .route("/api/v1/outlets", post(upsert_outlet))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_key))
        .with_state(state)
}

/// Reject anything without a valid bearer key with a bare 404. A missing config
/// key, a missing header and a wrong key are indistinguishable from outside.
async fn require_key(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let Some(key) = state.admin_api_key.as_deref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let presented = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    if constant_time_eq(presented.as_bytes(), key.as_bytes()) {
        next.run(request).await
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

/// Constant-time byte comparison, so a wrong key cannot be recovered by timing.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// What an ingest write returns: the row's id and whether it was created (versus
/// updated in place), so a caller can tell a fresh item from a refreshed one.
#[derive(Serialize)]
struct Upserted {
    id: i64,
    created: bool,
}

/// A failure a caller can act on: a bad payload or an internal error. Rendered as
/// JSON so the pipeline can read it.
enum ApiError {
    BadRequest(&'static str),
    Internal,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            ApiError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal error"),
        };
        (status, Json(serde_json::json!({ "error": msg }))).into_response()
    }
}

impl From<db::Error> for ApiError {
    fn from(e: db::Error) -> Self {
        tracing::error!(?e, "admin api db error");
        ApiError::Internal
    }
}

#[derive(Deserialize)]
struct SourceIn {
    kind: String,
    url: String,
    title: Option<String>,
    content_hash: Option<String>,
}

/// Upsert a source by its `(url, content_hash)` natural key. Idempotent: a
/// retried delivery reuses the row rather than duplicating it.
async fn upsert_source(
    State(state): State<AppState>,
    Json(body): Json<SourceIn>,
) -> Result<Json<Upserted>, ApiError> {
    let url = body.url.trim();
    let kind = body.kind.trim();
    if url.is_empty() {
        return Err(ApiError::BadRequest("url is required"));
    }
    if kind.is_empty() {
        return Err(ApiError::BadRequest("kind is required"));
    }
    let (id, created) = db::sources::upsert(
        &state.pool,
        kind,
        url,
        body.title.as_deref(),
        body.content_hash.as_deref(),
    )
    .await?;
    Ok(Json(Upserted { id, created }))
}

#[derive(Deserialize)]
struct NewsIn {
    url: String,
    headline: String,
    outlet: Option<String>,
    published_at: Option<DateTime<Utc>>,
    content_hash: Option<String>,
    /// Our neutral short summary. Stored as a draft for review, never published
    /// directly by the API.
    summary: Option<String>,
    /// Slugs of tracked people and parties the article names. Unknown slugs are
    /// skipped rather than failing the write.
    #[serde(default)]
    people: Vec<String>,
    #[serde(default)]
    parties: Vec<String>,
}

#[derive(Serialize)]
struct NewsUpserted {
    id: i64,
    created: bool,
    linked_people: usize,
    linked_parties: usize,
}

/// Upsert a news item by its article URL, link it to the tracked entities it
/// names, and store its summary as a draft. Idempotent on the URL.
async fn upsert_news(
    State(state): State<AppState>,
    Json(body): Json<NewsIn>,
) -> Result<Json<NewsUpserted>, ApiError> {
    let url = body.url.trim();
    let headline = body.headline.trim();
    if url.is_empty() {
        return Err(ApiError::BadRequest("url is required"));
    }
    if headline.is_empty() {
        return Err(ApiError::BadRequest("headline is required"));
    }
    let summary = body
        .summary
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let up = db::news::upsert(
        &state.pool,
        &db::news::ApiNews {
            url,
            outlet: body.outlet.as_deref(),
            published_at: body.published_at,
            content_hash: body.content_hash.as_deref(),
            headline,
            summary_draft: summary,
        },
    )
    .await?;

    let mut linked_people = 0;
    for slug in &body.people {
        if db::news::link_person(&state.pool, up.id, slug.trim()).await? {
            linked_people += 1;
        }
    }
    let mut linked_parties = 0;
    for slug in &body.parties {
        if db::news::link_party(&state.pool, up.id, slug.trim()).await? {
            linked_parties += 1;
        }
    }

    Ok(Json(NewsUpserted {
        id: up.id,
        created: up.created,
        linked_people,
        linked_parties,
    }))
}

#[derive(Deserialize)]
struct OutletIn {
    slug: String,
    name: String,
    /// The country slug this outlet belongs to.
    country: String,
    homepage_url: Option<String>,
    /// One of the five-point spectrum values, or omitted.
    leaning: Option<String>,
    summary: Option<String>,
}

#[derive(Serialize)]
struct OutletUpserted {
    id: i64,
    created: bool,
    /// How many already-delivered articles from this outlet were linked to it.
    linked_articles: u64,
}

/// Upsert a news outlet by slug, and link any articles already delivered under
/// its name to it (so its page and leaning show them). Idempotent.
async fn upsert_outlet(
    State(state): State<AppState>,
    Json(body): Json<OutletIn>,
) -> Result<Json<OutletUpserted>, ApiError> {
    let slug = body.slug.trim();
    let name = body.name.trim();
    if slug.is_empty() {
        return Err(ApiError::BadRequest("slug is required"));
    }
    if name.is_empty() {
        return Err(ApiError::BadRequest("name is required"));
    }
    let leaning = body
        .leaning
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(l) = leaning {
        if !db::outlets::LEANINGS.contains(&l) {
            return Err(ApiError::BadRequest(
                "leaning must be one of: left, lean_left, center, lean_right, right",
            ));
        }
    }
    let country = db::country::get_by_slug(&state.pool, body.country.trim())
        .await?
        .ok_or(ApiError::BadRequest("unknown country"))?;

    let created = db::outlets::get_by_slug(&state.pool, slug).await?.is_none();
    let id = db::outlets::upsert(
        &state.pool,
        &db::outlets::NewOutlet {
            name,
            slug,
            homepage_url: body.homepage_url.as_deref(),
            logo_url: None,
            logo_license: None,
            leaning,
            summary: body
                .summary
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty()),
            country_id: Some(country.id),
        },
    )
    .await?;
    // Connect articles already delivered under this outlet's name (news stores the
    // outlet as text) to the entity, so re-running after a backfill links them.
    let linked = db::outlets::link_sources_by_label(&state.pool, id, name).await?;

    Ok(Json(OutletUpserted {
        id,
        created,
        linked_articles: linked,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The load-bearing guarantee: the admin API can never write a vote or a
    /// result. Nothing here may name a participation table, so no handler can be
    /// added that touches one without this failing. The vote path stays the only
    /// way a ballot enters, by a voter, anonymously.
    #[test]
    fn the_api_never_touches_a_votes_table() {
        // Only the handler code, not this test module (which names the tables it
        // forbids), so the guard checks the surface, not itself.
        let full = include_str!("api.rs");
        let src = full.split("#[cfg(test)]").next().unwrap();
        for table in [
            "poll_votes",
            "vote_ballots",
            "vote_entitlements",
            "ballot_options",
            "poll_chains",
        ] {
            assert!(
                !src.contains(table),
                "the admin API must never reference `{table}`: it can write editorial \
                 content but never a vote or a result"
            );
        }
    }

    #[test]
    fn constant_time_eq_matches_and_rejects() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"secreu"));
        assert!(!constant_time_eq(b"secret", b"secre"));
    }
}
