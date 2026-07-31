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

use sha2::{Digest, Sha256};

/// The outcome of trying to spend a token as an anonymous ballot.
pub enum CastOutcome {
    /// Recorded; carries the new chain position and the poll's new head hash.
    Cast { seq: i64, head_hash: Vec<u8> },
    /// The token was already spent for this poll; nothing changed.
    AlreadySpent,
}

/// The genesis hash that seeds a poll's ballot chain.
fn ballot_genesis(poll_id: i64) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(b"open-public/ballot/");
    h.update(poll_id.to_string().as_bytes());
    h.finalize().to_vec()
}

/// One ballot's chained hash: over the previous head, the poll, the token, the
/// selected options (sorted), and the sequence number.
fn ballot_hash(prev: &[u8], poll_id: i64, token: &[u8], options: &[i64], seq: i64) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(prev);
    h.update(poll_id.to_be_bytes());
    h.update(token);
    for o in options {
        h.update(o.to_be_bytes());
    }
    h.update(seq.to_be_bytes());
    h.finalize().to_vec()
}

/// Spend a token as an anonymous ballot: append it to the poll's hash chain and
/// record its options, all under an advisory lock so the chain stays linear and
/// gap-free. The unique (poll, token) constraint makes a replay a no-op
/// (`AlreadySpent`). The signature is verified by the caller; this layer only
/// stores it for public verification and never sees an account.
pub async fn cast_ballot(
    pool: &Pool,
    poll_id: i64,
    token: &[u8],
    signature: &[u8],
    msg_randomizer: Option<&[u8]>,
    option_ids: &[i64],
) -> Result<CastOutcome> {
    let mut options = option_ids.to_vec();
    options.sort_unstable();
    options.dedup();

    let mut tx = pool.begin().await?;
    // Serialize spends on this poll so the chain is linear.
    sqlx::query!("select pg_advisory_xact_lock($1)", poll_id)
        .execute(&mut *tx)
        .await?;

    let prev = sqlx::query!(
        "select seq, content_hash from vote_ballots where poll_id = $1 order by seq desc limit 1",
        poll_id
    )
    .fetch_optional(&mut *tx)
    .await?;
    let (prev_seq, prev_hash) = match prev {
        Some(r) => (r.seq, r.content_hash),
        None => (0, ballot_genesis(poll_id)),
    };
    let seq = prev_seq + 1;
    let content = ballot_hash(&prev_hash, poll_id, token, &options, seq);

    let ballot_id: Option<i64> = sqlx::query_scalar!(
        "insert into vote_ballots \
           (poll_id, token, signature, msg_randomizer, seq, content_hash, prev_hash) \
         values ($1, $2, $3, $4, $5, $6, $7) \
         on conflict (poll_id, token) do nothing returning id",
        poll_id,
        token,
        signature,
        msg_randomizer,
        seq,
        content,
        prev_hash
    )
    .fetch_optional(&mut *tx)
    .await?;

    let Some(ballot_id) = ballot_id else {
        tx.rollback().await?;
        return Ok(CastOutcome::AlreadySpent);
    };

    for option_id in &options {
        sqlx::query!(
            "insert into ballot_options (ballot_id, option_id) values ($1, $2)",
            ballot_id,
            option_id
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(CastOutcome::Cast {
        seq,
        head_hash: content,
    })
}

/// The ballot-chain head for a poll: the sequence number and content hash of the
/// most recent ballot, or `None` if none has been cast. This is the fingerprint
/// shown on the poll page and published in the dump.
pub async fn ballot_chain_head(pool: &Pool, poll_id: i64) -> Result<Option<(i64, Vec<u8>)>> {
    let row = sqlx::query!(
        "select seq, content_hash from vote_ballots where poll_id = $1 order by seq desc limit 1",
        poll_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| (r.seq, r.content_hash)))
}

/// The reconciliation counts for a poll: how many tokens it issued, how many
/// ballots were spent, and how many accounts are eligible to vote at all. The
/// public bound is `spent <= issued <= eligible`, checkable by anyone. None of
/// these carries per-voter resolution: `issued` and `eligible` are counts, and
/// `spent` is the number of anonymous ballots, linkable to no account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reconciliation {
    /// Accounts issued a token for this poll (entitlements).
    pub issued: i64,
    /// Anonymous ballots cast for this poll (distinct ballots, not option picks).
    pub spent: i64,
    /// Verified, unbanned accounts: the ceiling for how many any poll can issue.
    pub eligible: i64,
}

/// Compute a poll's reconciliation counts in one round trip.
pub async fn reconciliation(pool: &Pool, poll_id: i64) -> Result<Reconciliation> {
    let row = sqlx::query!(
        r#"
        select
          (select count(*) from vote_entitlements where poll_id = $1) as "issued!",
          (select count(*) from vote_ballots where poll_id = $1) as "spent!",
          (select count(*) from users
             where verified_at is not null and banned_at is null) as "eligible!"
        "#,
        poll_id
    )
    .fetch_one(pool)
    .await?;
    Ok(Reconciliation {
        issued: row.issued,
        spent: row.spent,
        eligible: row.eligible,
    })
}

/// Below this many ballots, the fine-grained participation signals (the cast-time
/// timeline, and later the account-age cohort) are suppressed on the page: too
/// few points to read as a shape, and a low-resolution aggregate could leak. The
/// reconciliation counts have no such resolution and are always shown.
pub const MIN_PARTICIPANTS: i64 = 25;

/// A coarse histogram of when a poll's ballots were cast: `buckets` equal time
/// slices spanning the first ballot to the last, each carrying its count. Returns
/// all zeros when the poll has no ballots. This is aggregate timing only: it is
/// derived from `cast_at` (already public per ballot) and names no voter.
pub async fn cast_histogram(pool: &Pool, poll_id: i64, buckets: i32) -> Result<Vec<i64>> {
    let rows = sqlx::query!(
        r#"
        with b as (
            select extract(epoch from cast_at)::float8 as t
            from vote_ballots where poll_id = $1
        ),
        bounds as (select min(t) as lo, max(t) as hi from b)
        select least($2::int, greatest(1,
                 case when bounds.hi > bounds.lo
                      then width_bucket(b.t, bounds.lo, bounds.hi, $2::int)
                      else 1 end)) as "slot!",
               count(*) as "n!"
        from b, bounds
        group by 1
        order by 1
        "#,
        poll_id,
        buckets
    )
    .fetch_all(pool)
    .await?;
    let mut out = vec![0i64; buckets.max(1) as usize];
    for r in rows {
        let idx = (r.slot - 1).clamp(0, buckets - 1) as usize;
        out[idx] = r.n;
    }
    Ok(out)
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

    async fn a_user(pool: &Pool) -> i64 {
        sqlx::query_scalar(
            "insert into users (email_hash, password_hash) values ('h', 'p') returning id",
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn reconciliation_counts_issued_spent_and_eligible(pool: Pool) {
        let poll = a_poll(&pool).await;
        let opt: i64 = sqlx::query_scalar(
            "insert into poll_options (poll_id, label, position) values ($1, 'A', 0) returning id",
        )
        .bind(poll)
        .fetch_one(&pool)
        .await
        .unwrap();

        // Three accounts: verified and unbanned (eligible), unverified, and
        // verified but banned. Only the first counts toward `eligible`.
        let eligible_uid: i64 = sqlx::query_scalar(
            "insert into users (email_hash, password_hash, verified_at) \
             values ('a', 'p', now()) returning id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query("insert into users (email_hash, password_hash) values ('b', 'p')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "insert into users (email_hash, password_hash, verified_at, banned_at) \
             values ('c', 'p', now(), now())",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Before anyone takes part: no issue, no spend, one eligible account.
        assert_eq!(
            reconciliation(&pool, poll).await.unwrap(),
            Reconciliation {
                issued: 0,
                spent: 0,
                eligible: 1
            }
        );

        // Two accounts request a token; one casts a ballot.
        record_entitlement(&pool, poll, eligible_uid).await.unwrap();
        let other = a_user(&pool).await;
        record_entitlement(&pool, poll, other).await.unwrap();
        cast_ballot(&pool, poll, b"tok", b"sig", None, &[opt])
            .await
            .unwrap();

        assert_eq!(
            reconciliation(&pool, poll).await.unwrap(),
            Reconciliation {
                issued: 2,
                spent: 1,
                eligible: 1
            }
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn cast_histogram_buckets_ballots_over_time(pool: Pool) {
        let poll = a_poll(&pool).await;

        // No ballots: every slice is zero.
        assert_eq!(
            cast_histogram(&pool, poll, 4).await.unwrap(),
            vec![0, 0, 0, 0]
        );

        // One ballot in the earliest slice, a cluster in the latest.
        sqlx::query(
            "insert into vote_ballots (poll_id, token, signature, seq, content_hash, cast_at) \
             values \
               ($1, 'a', 's', 1, 'h1', now() - interval '10 hours'), \
               ($1, 'b', 's', 2, 'h2', now() - interval '20 minutes'), \
               ($1, 'c', 's', 3, 'h3', now() - interval '10 minutes'), \
               ($1, 'd', 's', 4, 'h4', now())",
        )
        .bind(poll)
        .execute(&pool)
        .await
        .unwrap();

        let h = cast_histogram(&pool, poll, 4).await.unwrap();
        assert_eq!(h.len(), 4);
        assert_eq!(h.iter().sum::<i64>(), 4, "every ballot lands in a slice");
        assert_eq!(h[0], 1, "the earliest ballot is in the first slice");
        assert!(h[3] >= 2, "the late cluster falls in the last slice");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn ballots_chain_and_reject_a_replay(pool: Pool) {
        let poll = a_poll(&pool).await;
        let opt: i64 = sqlx::query_scalar(
            "insert into poll_options (poll_id, label, position) values ($1, 'A', 0) returning id",
        )
        .bind(poll)
        .fetch_one(&pool)
        .await
        .unwrap();

        // First ballot: seq 1.
        let head1 = match cast_ballot(&pool, poll, b"token-a", b"sig-a", None, &[opt])
            .await
            .unwrap()
        {
            CastOutcome::Cast { seq, head_hash } => {
                assert_eq!(seq, 1);
                head_hash
            }
            CastOutcome::AlreadySpent => panic!("first ballot should be cast"),
        };

        // Second ballot: seq 2, chained onto the first.
        match cast_ballot(&pool, poll, b"token-b", b"sig-b", None, &[opt])
            .await
            .unwrap()
        {
            CastOutcome::Cast { seq, .. } => assert_eq!(seq, 2),
            CastOutcome::AlreadySpent => panic!("second ballot should be cast"),
        }
        let prev2: Vec<u8> =
            sqlx::query_scalar("select prev_hash from vote_ballots where poll_id = $1 and seq = 2")
                .bind(poll)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(prev2, head1, "each ballot chains onto the previous head");

        // Re-spending token-a is a no-op.
        assert!(matches!(
            cast_ballot(&pool, poll, b"token-a", b"sig-a", None, &[opt])
                .await
                .unwrap(),
            CastOutcome::AlreadySpent
        ));
        let count: i64 = sqlx::query_scalar("select count(*) from vote_ballots where poll_id = $1")
            .bind(poll)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 2);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn entitlements_record_once_and_delete(pool: Pool) {
        let poll = a_poll(&pool).await;
        let user = a_user(&pool).await;

        assert!(!has_entitlement(&pool, poll, user).await.unwrap());
        // First record inserts; a repeat is the one-per-account refusal.
        assert!(record_entitlement(&pool, poll, user).await.unwrap());
        assert!(has_entitlement(&pool, poll, user).await.unwrap());
        assert!(!record_entitlement(&pool, poll, user).await.unwrap());
        // Delete lets the account request again (the sign-failure compensation).
        delete_entitlement(&pool, poll, user).await.unwrap();
        assert!(!has_entitlement(&pool, poll, user).await.unwrap());
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
