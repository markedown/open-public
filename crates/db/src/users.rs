use chrono::{DateTime, Utc};

use crate::{Pool, Result};

/// A user as stored and queried.
#[derive(Debug, Clone)]
pub struct User {
    pub id: i64,
    pub email_hash: String,
    pub is_admin: bool,
    pub verified_at: Option<DateTime<Utc>>,
    /// Set when the account is permanently suspended; login is then refused.
    pub banned_at: Option<DateTime<Utc>>,
}

/// Create a new unverified user, keyed on email_hash.
pub async fn insert(pool: &Pool, email_hash: &str, password_hash: &str) -> Result<i64> {
    let id = sqlx::query_scalar!(
        r#"
        insert into users (email_hash, password_hash) values ($1, $2)
        on conflict (email_hash) do nothing
        returning id
        "#,
        email_hash,
        password_hash,
    )
    .fetch_optional(pool)
    .await?;
    match id {
        Some(id) => Ok(id),
        None => Err(crate::Error::UniqueViolation),
    }
}

/// Fetch a user by email hash (for login).
pub async fn get_by_email_hash(pool: &Pool, email_hash: &str) -> Result<Option<User>> {
    let user = sqlx::query_as!(
        User,
        r#"select id, email_hash, is_admin, verified_at, banned_at from users where email_hash = $1"#,
        email_hash,
    )
    .fetch_optional(pool)
    .await?;
    Ok(user)
}

/// Fetch a user by id.
pub async fn get_by_id(pool: &Pool, user_id: i64) -> Result<Option<User>> {
    let user = sqlx::query_as!(
        User,
        r#"select id, email_hash, is_admin, verified_at, banned_at from users where id = $1"#,
        user_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(user)
}

/// Mark a user as verified (after email confirmation).
pub async fn mark_verified(pool: &Pool, user_id: i64) -> Result<()> {
    sqlx::query!(
        r#"update users set verified_at = now() where id = $1"#,
        user_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Get the stored password hash for a user (for verification during login).
pub async fn password_hash(pool: &Pool, user_id: i64) -> Result<Option<String>> {
    let row = sqlx::query!(r#"select password_hash from users where id = $1"#, user_id,)
        .fetch_optional(pool)
        .await?;
    Ok(row.and_then(|r| r.password_hash))
}
