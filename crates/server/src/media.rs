//! Uploaded image handling: validate, re-encode, store, and serve.
//!
//! The security model for uploads lives here. Bytes are never trusted by their
//! declared type or extension: the format is detected from content, the image
//! is decoded under strict size limits (a decompression-bomb guard), and then
//! it is re-encoded into a normalized raster format. Re-encoding strips all
//! metadata (EXIF, colour profiles, embedded payloads) and defeats polyglot
//! files, so what is stored and served can only be a plain image we produced.
//! Stored images are content-addressed by the hash of the re-encoded bytes.

use std::io::Cursor;
use std::path::{Path as FsPath, PathBuf};

use axum::extract::{Path, State};
use axum::http::{header, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use image::ImageFormat;
use sha2::{Digest, Sha256};

use crate::auth::AuthSession;
use crate::state::AppState;

/// Largest accepted upload per image (before decoding).
pub const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;
/// Whole-request body cap for the submission form (a few images plus fields).
pub const MAX_UPLOAD_BODY: usize = 24 * 1024 * 1024;
/// Reject images whose dimensions exceed this, before allocating for decode.
const MAX_DIMENSION: u32 = 6000;
/// Cap on memory the decoder may allocate, another decompression-bomb guard.
const MAX_ALLOC: u64 = 256 * 1024 * 1024;
/// Re-encoded images are scaled to fit within this on the longest side.
const OUTPUT_MAX_DIM: u32 = 1600;

/// Why an upload was not stored. `Rejected` carries a message safe to show the
/// user (a bad or oversized image); `Io` is an internal failure.
#[derive(Debug)]
pub enum AssetError {
    Rejected(String),
    Io(anyhow::Error),
}

/// The result of processing an upload, ready to store.
struct Processed {
    bytes: Vec<u8>,
    mime: &'static str,
    width: i32,
    height: i32,
    sha: String,
}

/// Validate, re-encode, store (content-addressed), and record an uploaded image.
/// The heavy decode/encode runs off the async runtime.
pub async fn store_upload(
    pool: &db::Pool,
    asset_dir: &FsPath,
    uploaded_by: i64,
    bytes: Vec<u8>,
) -> Result<db::assets::Asset, AssetError> {
    if bytes.is_empty() {
        return Err(AssetError::Rejected("the image was empty".to_string()));
    }
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(AssetError::Rejected(
            "the image is too large (5 MB maximum)".to_string(),
        ));
    }

    let processed = tokio::task::spawn_blocking(move || process(bytes))
        .await
        .map_err(|e| AssetError::Io(anyhow::anyhow!(e)))??;

    let path = asset_path(asset_dir, &processed.sha);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| AssetError::Io(e.into()))?;
    }
    // Content-addressed: identical bytes hash to the same name, so a re-upload
    // is a harmless no-op rather than a rewrite.
    if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
        tokio::fs::write(&path, &processed.bytes)
            .await
            .map_err(|e| AssetError::Io(e.into()))?;
    }

    let asset = db::assets::insert(
        pool,
        &db::assets::NewAsset {
            sha256: &processed.sha,
            mime: processed.mime,
            width: processed.width,
            height: processed.height,
            byte_size: processed.bytes.len() as i64,
            uploaded_by,
        },
    )
    .await
    .map_err(|e| AssetError::Io(e.into()))?;
    Ok(asset)
}

/// The synchronous, CPU-bound part: decode under limits, downscale, re-encode.
fn process(bytes: Vec<u8>) -> Result<Processed, AssetError> {
    let reject = |m: &str| AssetError::Rejected(m.to_string());

    let format =
        image::guess_format(&bytes).map_err(|_| reject("the file is not a recognized image"))?;
    if !matches!(
        format,
        ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP
    ) {
        return Err(reject("only PNG, JPEG, or WebP images are accepted"));
    }

    let mut reader = image::ImageReader::new(Cursor::new(&bytes));
    reader.set_format(format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits);
    let img = reader
        .decode()
        .map_err(|_| reject("the image could not be read, or is too large"))?;

    // Only ever shrink, never upscale.
    let img = if img.width() > OUTPUT_MAX_DIM || img.height() > OUTPUT_MAX_DIM {
        img.thumbnail(OUTPUT_MAX_DIM, OUTPUT_MAX_DIM)
    } else {
        img
    };

    // Images with transparency become PNG; everything else becomes JPEG, which
    // keeps photographs small. Either way, only pixels survive re-encoding.
    let (out, mime) = if img.color().has_alpha() {
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
            .map_err(|_| reject("the image could not be processed"))?;
        (buf, "image/png")
    } else {
        let rgb = img.to_rgb8();
        let mut buf = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 82);
        encoder
            .encode_image(&rgb)
            .map_err(|_| reject("the image could not be processed"))?;
        (buf, "image/jpeg")
    };

    let sha = hex(Sha256::digest(&out));
    Ok(Processed {
        width: img.width() as i32,
        height: img.height() as i32,
        bytes: out,
        mime,
        sha,
    })
}

/// The on-disk path for an asset, sharded by the first two hex characters to
/// keep any one directory small. `sha` is validated hex, so it cannot traverse.
fn asset_path(dir: &FsPath, sha: &str) -> PathBuf {
    dir.join(&sha[0..2]).join(sha)
}

/// Serve an image by its content hash. Published images are public; a
/// not-yet-approved image is served only to its uploader or an admin, and is
/// otherwise indistinguishable from one that does not exist.
pub async fn serve(
    State(state): State<AppState>,
    session: Option<AuthSession>,
    Path(sha): Path<String>,
) -> Response {
    if sha.len() != 64 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let asset = match db::assets::get_by_sha(&state.pool, &sha).await {
        Ok(Some(a)) => a,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    if !asset.published {
        let allowed = session
            .as_ref()
            .is_some_and(|s| s.is_admin || s.user_id == asset.uploaded_by);
        if !allowed {
            return StatusCode::NOT_FOUND.into_response();
        }
    }

    let data = match tokio::fs::read(asset_path(&state.asset_dir, &sha)).await {
        Ok(d) => d,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };

    let ext = if asset.mime == "image/png" {
        "png"
    } else {
        "jpg"
    };
    let cache = if asset.published {
        "public, max-age=31536000, immutable"
    } else {
        "private, no-store"
    };

    let mut resp = data.into_response();
    let h = resp.headers_mut();
    if let Ok(v) = HeaderValue::from_str(&asset.mime) {
        h.insert(header::CONTENT_TYPE, v);
    }
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; sandbox"),
    );
    h.insert(
        HeaderName::from_static("cross-origin-resource-policy"),
        HeaderValue::from_static("same-origin"),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    if let Ok(v) = HeaderValue::from_str(&format!("inline; filename=\"{sha}.{ext}\"")) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    resp
}

fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // A 2x3 red PNG, built in-memory, exercises the real decode + re-encode path.
    fn tiny_png() -> Vec<u8> {
        let img = image::RgbImage::from_pixel(2, 3, image::Rgb([200, 10, 10]));
        let mut buf = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
            .unwrap();
        buf
    }

    #[test]
    fn processes_a_valid_png_into_a_normalized_image() {
        let p = process(tiny_png()).unwrap();
        assert_eq!(p.width, 2);
        assert_eq!(p.height, 3);
        // No alpha, so it becomes JPEG; the hash is over the re-encoded bytes.
        assert_eq!(p.mime, "image/jpeg");
        assert_eq!(p.sha.len(), 64);
        assert!(p.sha.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn rejects_non_images_and_svg() {
        assert!(matches!(
            process(b"not an image".to_vec()),
            Err(AssetError::Rejected(_))
        ));
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#;
        assert!(matches!(
            process(svg.to_vec()),
            Err(AssetError::Rejected(_))
        ));
    }

    #[test]
    fn identical_bytes_hash_identically() {
        let a = process(tiny_png()).unwrap();
        let b = process(tiny_png()).unwrap();
        assert_eq!(a.sha, b.sha);
    }

    #[test]
    fn asset_path_sharded_by_prefix() {
        let p = asset_path(FsPath::new("/data"), "abcdef0000");
        assert_eq!(p, PathBuf::from("/data/ab/abcdef0000"));
    }
}
