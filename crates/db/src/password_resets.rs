//! Single-use tokens for setting a new password.
//!
//! Only the hash of a token is stored, as with sessions, so a copy of the
//! database does not hand over working reset links.

use chrono::{DateTime, Utc};

use crate::{Pool, Result};

/// Issue a reset token for a user.
///
/// Any token issued earlier is consumed first: asking for a second link
/// invalidates the first, so only the most recent mail works and an older one
/// sitting in an inbox cannot be replayed.
pub async fn create(
    pool: &Pool,
    user_id: i64,
    token_hash: &str,
    expires_at: DateTime<Utc>,
) -> Result<i64> {
    let mut tx = pool.begin().await?;
    sqlx::query!(
        "update password_resets set consumed_at = now()
         where user_id = $1 and consumed_at is null",
        user_id,
    )
    .execute(&mut *tx)
    .await?;
    let id = sqlx::query_scalar!(
        "insert into password_resets (user_id, token_hash, expires_at)
         values ($1, $2, $3) returning id",
        user_id,
        token_hash,
        expires_at,
    )
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(id)
}

/// The user a live token belongs to: not consumed, not expired.
pub async fn user_for_token(pool: &Pool, token_hash: &str) -> Result<Option<i64>> {
    let row = sqlx::query_scalar!(
        "select user_id from password_resets
         where token_hash = $1 and consumed_at is null and expires_at > now()",
        token_hash,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Set the new password and spend the token, in one transaction.
///
/// Every session of that account is dropped at the same time. Whoever asked for
/// the reset may be doing so because someone else is signed in, and leaving
/// those sessions alive would hand the account back to them.
///
/// Returns false when the token was already spent or has expired, which is what
/// makes a reloaded reset page harmless.
pub async fn consume(pool: &Pool, token_hash: &str, password_hash: &str) -> Result<bool> {
    let mut tx = pool.begin().await?;
    let user_id = sqlx::query_scalar!(
        "update password_resets set consumed_at = now()
         where token_hash = $1 and consumed_at is null and expires_at > now()
         returning user_id",
        token_hash,
    )
    .fetch_optional(&mut *tx)
    .await?;
    let Some(user_id) = user_id else {
        tx.rollback().await?;
        return Ok(false);
    };
    sqlx::query!(
        "update users set password_hash = $2 where id = $1",
        user_id,
        password_hash,
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!("delete from sessions where user_id = $1", user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(true)
}
