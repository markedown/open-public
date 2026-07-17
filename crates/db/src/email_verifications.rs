use chrono::{DateTime, Utc};

use crate::{Pool, Result};

/// Store a verification token for a user. `code` holds the SHA-256 hash of the
/// token, never the token itself.
pub async fn create(
    pool: &Pool,
    user_id: i64,
    email_hash: &str,
    code_hash: &str,
    expires_at: DateTime<Utc>,
) -> Result<()> {
    sqlx::query!(
        r#"
        insert into email_verifications (user_id, email_hash, code, expires_at)
        values ($1, $2, $3, $4)
        "#,
        user_id,
        email_hash,
        code_hash,
        expires_at,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Consume a verification token and verify its user in one atomic step. Returns
/// the verified user id, or `None` if the token is unknown, expired, or already
/// used.
pub async fn consume_and_verify(pool: &Pool, code_hash: &str) -> Result<Option<i64>> {
    let row = sqlx::query!(
        r#"
        with consumed as (
            update email_verifications
            set consumed_at = now()
            where code = $1 and consumed_at is null and expires_at > now()
            returning user_id
        )
        update users
        set verified_at = now()
        where id = (select user_id from consumed)
        returning id
        "#,
        code_hash,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.id))
}
