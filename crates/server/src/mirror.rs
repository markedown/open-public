//! Photos, flags and logos are served from our own origin.
//!
//! The dataset records where an image came from, and that address stays in the
//! data as its provenance. A browser never loads it: an `<img>` pointing at
//! another host hands that host every visitor's IP address, which is a transfer
//! this site has no reason to make. Pages link to `/img/<hash of the address>`
//! instead, and a background task copies the images here, re-encoded the same way
//! as an upload, so what is served holds pixels and nothing else.
//!
//! The file on disk is the only state, so a new photo in the data is picked up by
//! the next pass and a failed download is simply tried again. An image we do not
//! hold yet is served as a neutral placeholder rather than a broken picture.
//!
//! The addresses come from our own tables (the dataset and admin forms), never
//! from a request, so `/img/` cannot be used to make this server fetch anything.

use std::collections::HashSet;
use std::path::{Path as FsPath, PathBuf};
use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::{header, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};

use crate::state::AppState;

/// Longest side kept for a copied image. The largest place one appears is a
/// portrait at twice its displayed size, or a poll image in a text column.
const MAX_SIDE: u32 = 640;
/// A file larger than this is not fetched to the end.
const MAX_DOWNLOAD: usize = 8 * 1024 * 1024;
/// The pause between downloads, the same courtesy the data gatherers keep.
const PAUSE: Duration = Duration::from_secs(1);
/// Shown for an image not copied yet.
const PLACEHOLDER: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect width="24" height="24" fill="#dfe3e8"/><g fill="none" stroke="#69717a" stroke-width="1.2"><circle cx="12" cy="9.5" r="3"/><path d="M6 19c1.1-3 3.4-4.5 6-4.5s4.9 1.5 6 4.5"/></g></svg>"##;

/// The address a page uses for an image. One that is already ours (an approved
/// upload under `/media/`) is left alone.
pub fn local_src(url: &str) -> String {
    if url.starts_with('/') {
        url.to_string()
    } else {
        format!("/img/{}", url_hash(url))
    }
}

fn url_hash(url: &str) -> String {
    crate::media::hex(Sha256::digest(url.as_bytes()))
}

/// Where a copy lives. `hash` is validated hex or our own digest, so it cannot
/// traverse.
fn copy_path(dir: &FsPath, hash: &str) -> PathBuf {
    dir.join("mirror").join(&hash[0..2]).join(hash)
}

/// Serve a copied image by the hash of its original address.
pub async fn serve(State(state): State<AppState>, Path(hash): Path<String>) -> Response {
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let (body, mime, cache) = match tokio::fs::read(copy_path(&state.asset_dir, &hash)).await {
        Ok(data) => {
            let mime = if data.starts_with(b"\x89PNG") {
                "image/png"
            } else {
                "image/jpeg"
            };
            (data, mime, "public, max-age=2592000")
        }
        Err(_) => (PLACEHOLDER.as_bytes().to_vec(), "image/svg+xml", "no-store"),
    };
    let mut resp = body.into_response();
    let h = resp.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; style-src 'unsafe-inline'; sandbox"),
    );
    h.insert(
        HeaderName::from_static("cross-origin-resource-policy"),
        HeaderValue::from_static("same-origin"),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    resp
}

/// What one pass did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Pass {
    pub copied: usize,
    pub failed: usize,
    pub already: usize,
}

/// An HTTP client that names itself, as Wikimedia's policy and our own ingest
/// rules ask.
pub fn client(base_url: &str) -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(format!("open-public-images/0.1 (+{base_url})"))
        .timeout(Duration::from_secs(30))
        .build()?)
}

/// Copy every image the data refers to and we do not hold yet. A host that says
/// we are asking too often is left alone until the next pass.
pub async fn mirror_pending(
    pool: &db::Pool,
    dir: &FsPath,
    http: &reqwest::Client,
    pause: Duration,
) -> anyhow::Result<Pass> {
    let mut pass = Pass::default();
    let mut resting: HashSet<String> = HashSet::new();
    for url in db::assets::external_image_urls(pool).await? {
        let path = copy_path(dir, &url_hash(&url));
        if tokio::fs::try_exists(&path).await.unwrap_or(false) {
            pass.already += 1;
            continue;
        }
        let host = reqwest::Url::parse(&url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_default();
        if resting.contains(&host) {
            pass.failed += 1;
            continue;
        }
        match copy_one(http, &url, &path).await {
            Ok(()) => pass.copied += 1,
            Err(Failure::Rest) => {
                tracing::warn!(host, "image host asked us to slow down; trying later");
                resting.insert(host);
                pass.failed += 1;
            }
            Err(Failure::Other(e)) => {
                tracing::warn!(url, error = %e, "could not copy image");
                pass.failed += 1;
            }
        }
        tokio::time::sleep(pause).await;
    }
    Ok(pass)
}

enum Failure {
    /// 429 or 503: back off from the host.
    Rest,
    Other(anyhow::Error),
}

impl<E: Into<anyhow::Error>> From<E> for Failure {
    fn from(e: E) -> Self {
        Failure::Other(e.into())
    }
}

async fn copy_one(http: &reqwest::Client, url: &str, path: &FsPath) -> Result<(), Failure> {
    let mut resp = http.get(fetch_address(url)).send().await?;
    let status = resp.status();
    if status.as_u16() == 429 || status.as_u16() == 503 {
        return Err(Failure::Rest);
    }
    if !status.is_success() {
        return Err(Failure::Other(anyhow::anyhow!("status {status}")));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        bytes.extend_from_slice(&chunk);
        if bytes.len() > MAX_DOWNLOAD {
            return Err(Failure::Other(anyhow::anyhow!("larger than the limit")));
        }
    }
    let processed = tokio::task::spawn_blocking(move || crate::media::process(bytes, MAX_SIDE))
        .await?
        .map_err(|e| anyhow::anyhow!("not a usable image: {e:?}"))?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    // Written aside and renamed, so a crash cannot leave half an image that the
    // next pass would take for a finished copy.
    let partial = path.with_extension("partial");
    tokio::fs::write(&partial, &processed.bytes).await?;
    tokio::fs::rename(&partial, path).await?;
    Ok(())
}

/// What to actually download. Wikimedia keeps flags as SVG, which we do not
/// decode and could not safely serve; it renders any file to PNG on request.
fn fetch_address(url: &str) -> String {
    const COMMONS: &str = "https://upload.wikimedia.org/wikipedia/commons/";
    if let Some(rest) = url.strip_prefix(COMMONS) {
        if let (true, Some(file)) = (url.ends_with(".svg"), rest.rsplit('/').next()) {
            return format!("{COMMONS}thumb/{rest}/330px-{file}.png");
        }
    }
    url.to_string()
}

/// Keep the copies current for as long as the server runs.
pub async fn run(pool: db::Pool, dir: PathBuf, http: reqwest::Client, interval: Duration) {
    loop {
        match mirror_pending(&pool, &dir, &http, PAUSE).await {
            Ok(p) => tracing::info!(
                copied = p.copied,
                failed = p.failed,
                already = p.already,
                "image copies up to date"
            ),
            Err(e) => tracing::warn!(error = %e, "image copy pass failed"),
        }
        tokio::time::sleep(interval).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_foreign_addresses_are_rewritten() {
        assert_eq!(local_src("/media/abc"), "/media/abc");
        let src = local_src("https://example.org/a.jpg");
        assert!(src.starts_with("/img/"));
        assert_eq!(src.len(), "/img/".len() + 64);
        assert!(!src.contains("example"));
    }

    #[test]
    fn commons_svg_is_fetched_as_a_rendered_png() {
        assert_eq!(
            fetch_address("https://upload.wikimedia.org/wikipedia/commons/b/ba/Flag_of_Germany.svg"),
            "https://upload.wikimedia.org/wikipedia/commons/thumb/b/ba/Flag_of_Germany.svg/330px-Flag_of_Germany.svg.png"
        );
        let jpg = "https://upload.wikimedia.org/wikipedia/commons/b/ba/A.jpg";
        assert_eq!(fetch_address(jpg), jpg);
        let other = "https://example.org/a.svg";
        assert_eq!(fetch_address(other), other);
    }
}
