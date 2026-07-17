//! The open-public server as a library, so the request handlers can be driven
//! by end-to-end tests. The binary in `main.rs` is a thin wrapper that reads
//! configuration and serves [`app`].

pub mod auth;
pub mod config;
pub mod content;
pub mod error;
pub mod fmt;
pub mod i18n;
pub mod mail;
pub mod pages;
pub mod state;
pub mod ui;

use std::path::Path;

use axum::extract::{Path as UrlPath, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::{
    middleware,
    routing::{get, post},
    Json, Router,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use serde::Serialize;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use i18n::Lang;
use state::AppState;

/// Build the application router. `static_dir` is served under `/static`.
pub fn app(state: AppState, static_dir: &Path) -> Router {
    Router::new()
        .route("/", get(pages::home::page))
        .route("/health", get(health))
        .route("/readyz", get(readyz))
        .route("/version", get(version))
        .route("/data/polls.json", get(pages::data::polls))
        .route("/{country}", get(pages::country::detail))
        .route("/{country}/people", get(pages::people::list))
        .route("/{country}/people/{slug}", get(pages::person::detail))
        .route("/{country}/parties", get(pages::parties::list))
        .route("/{country}/parties/{slug}", get(pages::party::detail))
        .route("/{country}/alliances", get(pages::alliances::list))
        .route("/{country}/alliance/{slug}", get(pages::alliance::detail))
        .route("/{country}/elections", get(pages::elections::list))
        .route("/{country}/election/{slug}", get(pages::elections::detail))
        .route("/{country}/history", get(pages::history::detail))
        .route("/{country}/news", get(pages::news::list))
        .route("/{country}/news/{id}", get(pages::news::detail))
        .route("/{country}/outlets", get(pages::outlets::list))
        .route("/{country}/outlet/{slug}", get(pages::outlets::detail))
        .route("/search", get(pages::search::page))
        .route("/{country}/polls", get(pages::polls::list))
        .route("/{country}/poll/{slug}", get(pages::poll::detail))
        .route("/{country}/poll/{slug}/vote", post(pages::poll::vote))
        .route("/{country}/poll/{slug}/chain", get(pages::poll::chain))
        .route(
            "/register",
            get(pages::auth::register_form).post(pages::auth::register_submit),
        )
        .route("/admin", get(pages::admin::index))
        .route("/admin/summaries", get(pages::admin::summaries))
        .route(
            "/admin/summaries/{id}/publish",
            post(pages::admin::summary_publish),
        )
        .route(
            "/admin/summaries/{id}/discard",
            post(pages::admin::summary_discard),
        )
        .route("/admin/translations", get(pages::admin::translations))
        .route(
            "/admin/translations/{id}/publish",
            post(pages::admin::translation_publish),
        )
        .route(
            "/admin/translations/{id}/discard",
            post(pages::admin::translation_discard),
        )
        .route("/admin/conflicts", get(pages::admin::conflicts))
        .route(
            "/admin/conflicts/{id}/resolve",
            post(pages::admin::conflict_resolve),
        )
        .route("/admin/outlet/new", get(pages::admin::outlet_new))
        .route("/admin/outlet", post(pages::admin::outlet_save))
        .route("/admin/outlet/{slug}/edit", get(pages::admin::outlet_edit))
        .route("/admin/party/{slug}", get(pages::admin::party_manage))
        .route("/admin/person/{slug}", get(pages::admin::person_manage))
        .route(
            "/admin/person/{slug}/education",
            post(pages::admin::person_education_add),
        )
        .route(
            "/admin/person/{slug}/education/{id}/delete",
            post(pages::admin::person_education_delete),
        )
        .route(
            "/admin/person/{slug}/attribute",
            post(pages::admin::person_attribute_add),
        )
        .route(
            "/admin/person/{slug}/attribute/{id}/delete",
            post(pages::admin::person_attribute_delete),
        )
        .route("/admin/news/new", get(pages::admin::news_form))
        .route("/admin/news", post(pages::admin::news_create))
        .route("/admin/news/{id}/edit", get(pages::admin::news_edit))
        .route("/admin/news/{id}", post(pages::admin::news_update))
        .route(
            "/admin/news/{id}/search",
            get(pages::admin::news_relation_search),
        )
        .route("/admin/news/{id}/link", post(pages::admin::news_link))
        .route("/admin/news/{id}/unlink", post(pages::admin::news_unlink))
        .route("/admin/poll/new", get(pages::admin::poll_form))
        .route("/admin/poll/option-row", get(pages::admin::poll_option_row))
        .route("/admin/poll", post(pages::admin::poll_create))
        .route("/admin/statement/new", get(pages::admin::statement_form))
        .route("/admin/statement", post(pages::admin::statement_create))
        .route("/verify", get(pages::auth::verify))
        .route(
            "/login",
            get(pages::auth::login_form).post(pages::auth::login_submit),
        )
        .route("/logout", post(pages::auth::logout))
        .route("/lang/{code}", get(set_language))
        .nest_service("/static", ServeDir::new(static_dir))
        // The locale layer runs before every handler, so `i18n::t` in the
        // templates reads the request's chosen language.
        .layer(middleware::from_fn(locale))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Resolve the request's locale (a `lang` cookie, else `Accept-Language`, else
/// the deployment default) and run the handler with it active.
async fn locale(req: axum::extract::Request, next: middleware::Next) -> axum::response::Response {
    let jar = CookieJar::from_headers(req.headers());
    let cookie_lang = jar.get("lang").map(|c| c.value().to_string());
    let accept = req
        .headers()
        .get(header::ACCEPT_LANGUAGE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let lang = i18n::resolve(cookie_lang.as_deref(), accept.as_deref());
    i18n::with_lang(lang, next.run(req)).await
}

/// Set the visitor's language cookie and return to the page they were on.
async fn set_language(
    UrlPath(code): UrlPath<String>,
    headers: HeaderMap,
    jar: CookieJar,
) -> (CookieJar, Redirect) {
    let lang = Lang::known(&code).unwrap_or(Lang::En);
    let cookie = Cookie::build(("lang", lang.code()))
        .path("/")
        .max_age(cookie::time::Duration::days(365))
        .same_site(SameSite::Lax)
        .build();
    let back = headers
        .get(header::REFERER)
        .and_then(|v| v.to_str().ok())
        .and_then(same_site_path)
        .unwrap_or_else(|| "/".to_string());
    (jar.add(cookie), Redirect::to(&back))
}

/// The path portion of a referer, forcing a same-site redirect (an external or
/// malformed referer collapses to the path, never a cross-site redirect).
fn same_site_path(referer: &str) -> Option<String> {
    if let Some(rest) = referer
        .strip_prefix("http://")
        .or_else(|| referer.strip_prefix("https://"))
    {
        return Some(rest[rest.find('/').unwrap_or(rest.len())..].to_string())
            .filter(|p| p.starts_with('/'));
    }
    referer.starts_with('/').then(|| referer.to_string())
}

/// Liveness probe for load balancers and uptime checks.
async fn health() -> &'static str {
    "ok"
}

/// Readiness probe: the database is reachable, so this instance can serve
/// requests. A blue-green cutover flips traffic to a new color only once its
/// `/readyz` returns 200, so a half-started instance never receives traffic.
async fn readyz(State(pool): State<db::Pool>) -> Response {
    match db::ping(&pool).await {
        Ok(()) => (StatusCode::OK, "ready").into_response(),
        Err(_) => (StatusCode::SERVICE_UNAVAILABLE, "not ready").into_response(),
    }
}

/// Build provenance, for verifying the running binary against the public build.
///
/// `commit` and `built_at` are baked in at build time (never read from `.git`
/// at runtime). `image_digest` is supplied at run time by the deployment, which
/// pulls images by digest, so the value here can be checked against the digest
/// attested by the public release workflow.
#[derive(Serialize)]
struct Version {
    commit: &'static str,
    built_at: &'static str,
    image_digest: Option<String>,
}

async fn version() -> Json<Version> {
    Json(Version {
        commit: option_env!("GIT_SHA").unwrap_or("unknown"),
        built_at: option_env!("BUILD_TIME").unwrap_or("unknown"),
        image_digest: std::env::var("IMAGE_DIGEST").ok().filter(|s| !s.is_empty()),
    })
}
