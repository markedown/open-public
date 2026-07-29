//! Persistence for anonymous-voting issuer keys. See docs/anonymous-voting.md.
//!
//! This layer stores opaque DER key bytes only; the blind-signature crypto lives
//! in the server crate. The private key is destroyed when a poll closes, so a
//! later compromise cannot forge into a closed poll.

use crate::{Pool, Result};

/// Store a poll's issuer keypair (DER-encoded). Idempotent per poll: an existing
/// key is left untouched, because a poll's key must never change under it.
pub async fn insert_issuer_key(
    pool: &Pool,
    poll_id: i64,
    public_key: &[u8],
    private_key: &[u8],
) -> Result<()> {
    sqlx::query!(
        "insert into poll_issuer_keys (poll_id, public_key, private_key) \
         values ($1, $2, $3) on conflict (poll_id) do nothing",
        poll_id,
        public_key,
        private_key,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// A poll's public key (DER), for verifying spent tokens. Present for the poll's
/// whole life, including after the private key is destroyed at close.
pub async fn public_key(pool: &Pool, poll_id: i64) -> Result<Option<Vec<u8>>> {
    let row = sqlx::query!(
        "select public_key from poll_issuer_keys where poll_id = $1",
        poll_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.public_key))
}

/// A poll's private key (DER), for blind-signing. `None` after it is destroyed at
/// close, or if the poll has no key yet.
pub async fn private_key(pool: &Pool, poll_id: i64) -> Result<Option<Vec<u8>>> {
    let row = sqlx::query!(
        "select private_key from poll_issuer_keys where poll_id = $1",
        poll_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|r| r.private_key))
}

/// Whether a poll already has an issuer key.
pub async fn has_issuer_key(pool: &Pool, poll_id: i64) -> Result<bool> {
    let exists = sqlx::query_scalar!(
        "select exists(select 1 from poll_issuer_keys where poll_id = $1)",
        poll_id
    )
    .fetch_one(pool)
    .await?;
    Ok(exists.unwrap_or(false))
}

/// Destroy a poll's private key at close, keeping the public key for
/// verification. Idempotent.
pub async fn destroy_private_key(pool: &Pool, poll_id: i64) -> Result<()> {
    sqlx::query!(
        "update poll_issuer_keys set private_key = null where poll_id = $1",
        poll_id
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Whether an account already holds an entitlement (a token) for a poll.
pub async fn has_entitlement(pool: &Pool, poll_id: i64, user_id: i64) -> Result<bool> {
    let e = sqlx::query_scalar!(
        "select exists(select 1 from vote_entitlements where poll_id = $1 and user_id = $2)",
        poll_id,
        user_id
    )
    .fetch_one(pool)
    .await?;
    Ok(e.unwrap_or(false))
}

/// Record that an account was issued a token for a poll. Returns `true` if this
/// is the first (the insert happened), `false` if one already existed. The unique
/// constraint makes this the atomic one-token-per-account gate under a race.
pub async fn record_entitlement(pool: &Pool, poll_id: i64, user_id: i64) -> Result<bool> {
    let n = sqlx::query!(
        "insert into vote_entitlements (poll_id, user_id) values ($1, $2) \
         on conflict (poll_id, user_id) do nothing",
        poll_id,
        user_id
    )
    .execute(pool)
    .await?
    .rows_affected();
    Ok(n == 1)
}

/// Remove an entitlement, to compensate if signing fails after it was recorded,
/// so the account can request its token again.
pub async fn delete_entitlement(pool: &Pool, poll_id: i64, user_id: i64) -> Result<()> {
    sqlx::query!(
        "delete from vote_entitlements where poll_id = $1 and user_id = $2",
        poll_id,
        user_id
    )
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn a_poll(pool: &Pool) -> i64 {
        sqlx::query_scalar("insert into polls (question, slug) values ('Q?', 'p') returning id")
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn issuer_key_is_stored_read_and_its_private_half_destroyed(pool: Pool) {
        let poll = a_poll(&pool).await;
        assert!(!has_issuer_key(&pool, poll).await.unwrap());

        insert_issuer_key(&pool, poll, b"PUB", b"PRIV")
            .await
            .unwrap();
        assert!(has_issuer_key(&pool, poll).await.unwrap());
        assert_eq!(
            public_key(&pool, poll).await.unwrap().as_deref(),
            Some(&b"PUB"[..])
        );
        assert_eq!(
            private_key(&pool, poll).await.unwrap().as_deref(),
            Some(&b"PRIV"[..])
        );

        // Idempotent: a second insert does not change the key.
        insert_issuer_key(&pool, poll, b"OTHER", b"OTHER")
            .await
            .unwrap();
        assert_eq!(
            public_key(&pool, poll).await.unwrap().as_deref(),
            Some(&b"PUB"[..])
        );

        // At close: the private half is destroyed, the public half remains.
        destroy_private_key(&pool, poll).await.unwrap();
        assert_eq!(private_key(&pool, poll).await.unwrap(), None);
        assert_eq!(
            public_key(&pool, poll).await.unwrap().as_deref(),
            Some(&b"PUB"[..])
        );
    }
}
