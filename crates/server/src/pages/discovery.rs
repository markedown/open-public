//! Telling crawlers what is here, and what is not.
//!
//! A political record is only useful if it can be found, so the site publishes
//! a sitemap of everything a visitor can read. It also publishes a robots file
//! that keeps crawlers out of the parts that are not content: sign-in, the
//! admin area, and anything a session would change.
//!
//! While construction mode is on, the whole site is a single coming-soon page,
//! so the robots file disallows everything. Indexing a placeholder would leave
//! the search result wrong long after the site opened.

use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};

use crate::error::PageError;
use crate::state::AppState;

/// The paths no crawler should follow: nothing here is content, and some of it
/// only exists for someone with a session.
const DISALLOWED: [&str; 6] = [
    "/admin",
    "/login",
    "/register",
    "/verify",
    "/reset",
    "/forgot",
];

/// The most URLs one sitemap file may contain, per the sitemap protocol. The
/// dataset is far below this, and it is enforced anyway: a file over the limit
/// is rejected whole, so the failure would be silent and total. Growing past it
/// calls for a sitemap index, not a bigger file.
const MAX_URLS: usize = 50_000;

pub async fn robots(State(state): State<AppState>) -> Response {
    let body = if state.construction {
        // One page, and it is a placeholder. Nothing here should be indexed.
        "User-agent: *\nDisallow: /\n".to_string()
    } else {
        let mut out = String::from("User-agent: *\n");
        for path in DISALLOWED {
            out.push_str(&format!("Disallow: {path}\n"));
        }
        out.push_str("Allow: /\n");
        if !state.base_url.is_empty() {
            out.push_str(&format!("\nSitemap: {}/sitemap.xml\n", state.base_url));
        }
        out
    };
    ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], body).into_response()
}

/// Every page a visitor can read, as a sitemap.
///
/// Built from the database rather than a hand-kept list, so a country added
/// tomorrow is discoverable without anyone remembering to edit a file. Under
/// construction there is nothing to list, and saying so is more honest than
/// publishing a map of pages that all serve the same placeholder.
pub async fn sitemap(State(state): State<AppState>) -> Result<Response, PageError> {
    let mut urls: Vec<String> = Vec::new();
    if !state.construction {
        let origin = state.base_url.to_string();
        urls.push(origin.clone());
        urls.push(format!("{origin}/privacy"));

        for c in db::country::list(&state.pool).await? {
            let base = format!("{origin}/{}", c.slug);
            urls.push(base.clone());
            for section in [
                "people",
                "parties",
                "alliances",
                "elections",
                "polls",
                "news",
                "outlets",
                "history",
                "compass",
            ] {
                urls.push(format!("{base}/{section}"));
            }
            for p in db::people::list(&state.pool, c.id, MAX_URLS as i64, 0).await? {
                urls.push(format!("{base}/people/{}", p.slug));
            }
            for p in db::parties::list(&state.pool, c.id).await? {
                urls.push(format!("{base}/parties/{}", p.slug));
            }
            for e in db::elections::list_for_country(&state.pool, c.id, "en").await? {
                urls.push(format!("{base}/election/{}", e.slug));
            }
            for a in db::alliances::list_for_country(&state.pool, c.id, "en").await? {
                urls.push(format!("{base}/alliance/{}", a.slug));
            }
        }
    }

    if urls.len() > MAX_URLS {
        tracing::warn!(
            urls = urls.len(),
            limit = MAX_URLS,
            "sitemap is over the protocol limit and was truncated; it needs a sitemap index"
        );
        urls.truncate(MAX_URLS);
    }

    let mut body =
        String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");
    for url in urls {
        body.push_str(&format!("  <url><loc>{}</loc></url>\n", escape_xml(&url)));
    }
    body.push_str("</urlset>\n");
    Ok((
        [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
        body,
    )
        .into_response())
}

/// Escape the characters a URL may legally contain that would break XML.
fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
