use chrono::{DateTime, Utc};

use crate::{Pool, Result};

/// A session, looked up by its token hash on each request.
#[derive(Debug, Clone)]
pub struct Session {
    pub id: i64,
    pub user_id: i64,
    pub is_admin: bool,
    pub expires_at: DateTime<Utc>,
}

/// Create a session and return its row id.
pub async fn create(
    pool: &Pool,
    user_id: i64,
    token_hash: &str,
    expires_at: DateTime<Utc>,
) -> Result<i64> {
    let id = sqlx::query_scalar!(
        r#"insert into sessions (user_id, token_hash, expires_at) values ($1, $2, $3) returning id"#,
        user_id,
        token_hash,
        expires_at,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Look up a session by token hash. Returns None if expired.
pub async fn get_by_token_hash(pool: &Pool, token_hash: &str) -> Result<Option<Session>> {
    Ok(sqlx::query!(
        r#"
        select s.id, s.user_id, s.expires_at, u.is_admin as "is_admin!"
        from sessions s
        join users u on u.id = s.user_id
        where s.token_hash = $1 and s.expires_at > now()
        "#,
        token_hash,
    )
    .fetch_optional(pool)
    .await?
    .map(|r| Session {
        id: r.id,
        user_id: r.user_id,
        is_admin: r.is_admin,
        expires_at: r.expires_at,
    }))
}

/// Delete a specific session (logout).
pub async fn delete(pool: &Pool, session_id: i64) -> Result<()> {
    sqlx::query!("delete from sessions where id = $1", session_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Remove all expired sessions.
pub async fn purge_expired(pool: &Pool) -> Result<()> {
    sqlx::query!("delete from sessions where expires_at < now()")
        .execute(pool)
        .await?;
    Ok(())
}
