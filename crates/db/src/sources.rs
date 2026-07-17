use crate::{Pool, Result};

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
