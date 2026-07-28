//! The single-use guard for solved proof-of-work captchas.
//!
//! A captcha solution is valid once. The challenge's HMAC signature is recorded
//! on first use; a replay of the same solution finds it already there and is
//! refused. Expired rows are pruned as they are written, so the table never
//! grows beyond the few minutes a challenge lives.

use chrono::{DateTime, Utc};

use crate::{Pool, Result};

/// Record a solved challenge as used. Returns `true` if this is the first time
/// it has been seen (accept), `false` if it was already used (a replay). Prunes
/// anything expired in the same call, so the table stays small.
pub async fn mark_used(pool: &Pool, signature: &str, expires_at: DateTime<Utc>) -> Result<bool> {
    let mut tx = pool.begin().await?;
    sqlx::query!("delete from captcha_used where expires_at < now()")
        .execute(&mut *tx)
        .await?;
    let inserted = sqlx::query!(
        "insert into captcha_used (signature, expires_at) values ($1, $2) \
         on conflict (signature) do nothing",
        signature,
        expires_at,
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    Ok(inserted == 1)
}
