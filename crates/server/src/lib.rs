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
pub mod media;
pub mod pages;
pub mod reviewer;
pub mod state;
pub mod ui;

use std::path::Path;

use axum::extract::{DefaultBodyLimit, Path as UrlPath, State};
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
    // Construction mode gates the whole platform behind a single coming-soon
    // page; only the operational endpoints stay live so deploys and monitoring
    // still work. Nothing else about the site is reachable.
    if state.construction {
        return Router::new()
            .route("/health", get(health))
            .route("/readyz", get(readyz))
            .route("/version", get(version))
            .fallback(coming_soon)
            .with_state(state);
    }
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
        .route(
            "/{country}/compass",
            get(pages::compass::form).post(pages::compass::result),
        )
        .route("/{country}/news", get(pages::news::list))
        .route("/{country}/news/{id}", get(pages::news::detail))
        .route("/{country}/outlets", get(pages::outlets::list))
        .route("/{country}/outlet/{slug}", get(pages::outlets::detail))
        .route("/search", get(pages::search::page))
        .route("/{country}/polls", get(pages::polls::list))
        .route(
            "/{country}/polls/submit",
            get(pages::submit::form)
                .post(pages::submit::create)
                .layer(DefaultBodyLimit::max(media::MAX_UPLOAD_BODY)),
        )
        .route(
            "/{country}/polls/submit/row",
            get(pages::submit::option_row),
        )
        .route("/{country}/poll/{slug}", get(pages::poll::detail))
        .route("/{country}/poll/{slug}/vote", post(pages::poll::vote))
        .route("/{country}/poll/{slug}/chain", get(pages::poll::chain))
        .route("/submissions", get(pages::submit::mine))
        .route("/feed", get(pages::feed::page))
        .route(
            "/follow/{entity_type}/{entity_id}",
            post(pages::follow::toggle),
        )
        .route("/media/{sha}", get(media::serve))
        .route(
            "/register",
            get(pages::auth::register_form).post(pages::auth::register_submit),
        )
        .route("/admin", get(pages::admin::index))
        .route("/admin/submissions", get(pages::admin::submissions))
        .route(
            "/admin/submissions/{id}/approve",
            post(pages::admin::submission_approve),
        )
        .route(
            "/admin/submissions/{id}/reject",
            post(pages::admin::submission_reject),
        )
        .route("/admin/summaries", get(pages::admin::summaries))
        .route(
            "/admin/summaries/{id}/publish",
            post(pages::admin::summary_publish),
        )
        .route(
            "/admin/summaries/{id}/discard",
            post(pages::admin::summary_discard),
        )
        .route("/admin/bios", get(pages::admin::bios))
        .route("/admin/bios/{id}/publish", post(pages::admin::bio_publish))
        .route("/admin/bios/{id}/discard", post(pages::admin::bio_discard))
        .route("/admin/translations", get(pages::admin::translations))
        .route(
            "/admin/translations/{id}/publish",
            post(pages::admin::translation_publish),
        )
        .route(
            "/admin/translations/{id}/discard",
            post(pages::admin::translation_discard),
        )
        .route("/admin/compass", get(pages::admin::compass))
        .route(
            "/admin/compass/thesis",
            post(pages::admin::compass_thesis_create),
        )
        .route(
            "/admin/compass/thesis/{id}",
            get(pages::admin::compass_thesis),
        )
        .route(
            "/admin/compass/thesis/{id}/delete",
            post(pages::admin::compass_thesis_delete),
        )
        .route(
            "/admin/compass/thesis/{id}/evidence",
            post(pages::admin::compass_evidence_add),
        )
        .route(
            "/admin/compass/thesis/{id}/evidence/{evidence_id}/delete",
            post(pages::admin::compass_evidence_delete),
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

/// The whole public surface while construction mode is on: a single, static,
/// self-contained coming-soon page (no external assets, so it renders even with
/// nothing else served). Monochrome and monospace, the platform's own voice, not
/// a stock placeholder.
async fn coming_soon() -> Response {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        COMING_SOON_HTML,
    )
        .into_response()
}

const COMING_SOON_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>open-public</title>
<style>
:root{--paper:#f0f1f3;--ink:#22242a;--muted:#6a6c72;--rule:#d5d7db;--block:#22242a}
*{margin:0;padding:0;box-sizing:border-box}
html,body{height:100%}
body{background:var(--paper);color:var(--ink);font:400 15px/1.55 ui-monospace,"SF Mono",SFMono-Regular,Menlo,Consolas,"Liberation Mono",monospace;display:flex;align-items:center;justify-content:center;padding:6vw;-webkit-font-smoothing:antialiased}
main{width:100%;max-width:640px}
.rule{height:1px;background:var(--rule)}
.top{display:flex;align-items:center;gap:.7rem;padding-bottom:1.1rem}
.block{width:20px;height:20px;background:var(--block);flex:none}
.word{font-weight:600;font-size:clamp(19px,5vw,24px);letter-spacing:-.02em}
.body{padding:2.6rem 0}
.status{font-size:clamp(30px,9vw,56px);font-weight:600;letter-spacing:-.03em;line-height:1.02}
.cursor{display:inline-block;width:.5em;height:.92em;background:var(--ink);vertical-align:-.06em;margin-left:.14em;animation:blink 1.1s steps(1) infinite}
@keyframes blink{50%{opacity:0}}
.desc{margin-top:1.5rem;max-width:48ch;color:var(--muted);font-size:14px}
.foot{display:flex;justify-content:space-between;gap:1rem;padding-top:1.1rem;color:var(--muted);font-size:11px;letter-spacing:.06em;text-transform:uppercase}
</style>
</head>
<body>
<main>
  <div class="top"><span class="block"></span><span class="word">open-public</span></div>
  <div class="rule"></div>
  <div class="body">
    <div class="status">Under<br>construction<span class="cursor"></span></div>
    <p class="desc">An open, source-backed record of political data and public participation. Still being assembled. Back soon.</p>
  </div>
  <div class="rule"></div>
  <div class="foot"><span>Open political data</span><span>Coming soon</span></div>
</main>
</body>
</html>
"#;

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
