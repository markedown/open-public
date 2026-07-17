//! Uploaded image assets. Rows here describe an image that has already been
//! validated and re-encoded by the server; the bytes live on disk addressed by
//! `sha256`. Identical images are stored once (the hash is unique).

use chrono::{DateTime, Utc};

use crate::{Pool, Result};

/// A stored image asset, as queried.
#[derive(Debug, Clone)]
pub struct Asset {
    pub id: i64,
    pub sha256: String,
    pub mime: String,
    pub width: i32,
    pub height: i32,
    pub byte_size: i64,
    pub uploaded_by: i64,
    /// True once referenced by an approved, published poll. Until then the asset
    /// is servable only to its uploader and to admins.
    pub published: bool,
    pub created_at: DateTime<Utc>,
}

/// Fields for recording a freshly stored image.
pub struct NewAsset<'a> {
    pub sha256: &'a str,
    pub mime: &'a str,
    pub width: i32,
    pub height: i32,
    pub byte_size: i64,
    pub uploaded_by: i64,
}

/// Record an asset, or return the existing row if these exact bytes were already
/// stored (dedup on the content hash). The first uploader is kept as the owner.
pub async fn insert(pool: &Pool, a: &NewAsset<'_>) -> Result<Asset> {
    let row = sqlx::query_as!(
        Asset,
        r#"
        insert into assets (sha256, mime, width, height, byte_size, uploaded_by)
        values ($1, $2, $3, $4, $5, $6)
        on conflict (sha256) do update set byte_size = excluded.byte_size
        returning id, sha256, mime, width, height, byte_size, uploaded_by, published, created_at
        "#,
        a.sha256,
        a.mime,
        a.width,
        a.height,
        a.byte_size,
        a.uploaded_by,
    )
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// Look up an asset by its content hash (the value in a `/media/{sha256}` URL).
pub async fn get_by_sha(pool: &Pool, sha256: &str) -> Result<Option<Asset>> {
    let row = sqlx::query_as!(
        Asset,
        r#"
        select id, sha256, mime, width, height, byte_size, uploaded_by, published, created_at
        from assets where sha256 = $1
        "#,
        sha256,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Mark an asset public. Called when the submission that carries it is approved,
/// so its images become servable to everyone at the same moment the poll does.
pub async fn mark_published(pool: &Pool, id: i64) -> Result<()> {
    sqlx::query!("update assets set published = true where id = $1", id)
        .execute(pool)
        .await?;
    Ok(())
}
