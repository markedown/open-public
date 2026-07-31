use crate::{Pool, Result};

/// Upsert a source by its natural key `(url, content_hash)`, treating a missing
/// hash as the same document, and report whether the row was newly created. This
/// is the admin-API path: idempotent, so a retried delivery never duplicates.
pub async fn upsert(
    pool: &Pool,
    kind: &str,
    url: &str,
    title: Option<&str>,
    content_hash: Option<&str>,
) -> Result<(i64, bool)> {
    let existing = sqlx::query_scalar!(
        "select id from sources where url = $1 and content_hash is not distinct from $2",
        url,
        content_hash,
    )
    .fetch_optional(pool)
    .await?;
    if let Some(id) = existing {
        sqlx::query!("update sources set title = $2 where id = $1", id, title)
            .execute(pool)
            .await?;
        return Ok((id, false));
    }
    let id = sqlx::query_scalar!(
        "insert into sources (kind, url, title, content_hash) values ($1, $2, $3, $4) returning id",
        kind,
        url,
        title,
        content_hash,
    )
    .fetch_one(pool)
    .await?;
    Ok((id, true))
}

/// Insert a source row and return its id.
///
/// Idempotent on `(url, content_hash)`: a repeated import of the same fetched
/// document reuses the existing row rather than creating a duplicate.
pub async fn insert_source(
    pool: &Pool,
    kind: &str,
    url: &str,
    title: Option<&str>,
    content_hash: Option<&str>,
) -> Result<i64> {
    let id = sqlx::query_scalar!(
        r#"
        insert into sources (kind, url, title, content_hash)
        values ($1, $2, $3, $4)
        on conflict (url, content_hash) do update set title = excluded.title
        returning id
        "#,
        kind,
        url,
        title,
        content_hash,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}
