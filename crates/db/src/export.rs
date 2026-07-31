//! Read models for the public participation dump. It publishes the anonymous
//! ballots so anyone can recompute the tallies, verify each ballot was issued
//! for its poll (the blind signature), confirm no token was spent twice, walk
//! the hash chain, and reconcile that no more ballots were cast than tokens
//! issued. It never exposes identity: a ballot carries its token (a nullifier
//! tied to no account), never a user id or an email hash, and the entitlement
//! count is a total, not a list.
//!
//! Approval polls use a separate, account-linked participation model and are not
//! part of this dump.

use chrono::{DateTime, Utc};

use crate::{Pool, Result};

/// One poll option's tally, flattened with its poll's chain head, issuer key and
/// issued-token count.
#[derive(Debug, Clone)]
pub struct BallotTally {
    pub slug: String,
    pub question: String,
    pub kind: String,
    pub poll_id: i64,
    pub position: i32,
    pub option_id: i64,
    pub label: String,
    pub votes: i64,
    /// The poll's issuer public key (DER), for verifying each ballot's token.
    pub public_key: Option<Vec<u8>>,
    /// How many tokens were issued for this poll (entitlements). Ballots cast can
    /// never exceed this.
    pub issued: i64,
    /// How many ballots were spent (cast) for this poll. Recomputable by counting
    /// the published ballots; carried explicitly so the reconciliation is legible.
    pub spent: i64,
    /// The ballot-chain head sequence and hash, `None` for a poll with no ballot.
    pub head_seq: Option<i64>,
    pub head_hash: Option<Vec<u8>>,
}

/// One anonymous ballot, carrying everything the chain is hashed from and what a
/// verifier needs to check the signature, and nothing else. No identity is
/// present: the token is a per-poll nullifier, not linkable to an account.
#[derive(Debug, Clone)]
pub struct AnonBallot {
    pub poll_slug: String,
    pub poll_id: i64,
    pub seq: i64,
    pub token: Vec<u8>,
    pub signature: Vec<u8>,
    pub msg_randomizer: Option<Vec<u8>>,
    pub cast_at: DateTime<Utc>,
    pub content_hash: Vec<u8>,
    pub prev_hash: Option<Vec<u8>>,
    pub option_ids: Vec<i64>,
}

/// Every non-approval poll's per-option tally, chain head, issuer key and issued
/// count, deterministically ordered so the dump is diffable.
pub async fn ballot_tallies(pool: &Pool) -> Result<Vec<BallotTally>> {
    let rows = sqlx::query_as!(
        BallotTally,
        r#"
        select p.slug as "slug!", p.question as "question!", p.kind as "kind!",
               p.id as "poll_id!", o.position as "position!", o.id as "option_id!", o.label as "label!",
               count(bo.ballot_id) as "votes!",
               k.public_key as "public_key?",
               (select count(*) from vote_entitlements e where e.poll_id = p.id) as "issued!",
               (select count(*) from vote_ballots b where b.poll_id = p.id) as "spent!",
               hd.seq as "head_seq?", hd.hash as "head_hash?"
        from polls p
        join poll_options o on o.poll_id = p.id
        left join ballot_options bo on bo.option_id = o.id
        left join poll_issuer_keys k on k.poll_id = p.id
        left join lateral (
            select b.seq, b.content_hash as hash
            from vote_ballots b where b.poll_id = p.id order by b.seq desc limit 1
        ) hd on true
        where p.kind <> 'approval'
        group by p.id, o.id, k.public_key, hd.seq, hd.hash
        order by p.slug, o.position
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// The number of accounts eligible to vote (verified and unbanned), the ceiling
/// for how many tokens any poll can issue. A single count, no identity.
pub async fn eligible_voters(pool: &Pool) -> Result<i64> {
    let n = sqlx::query_scalar!(
        r#"select count(*) as "n!" from users
           where verified_at is not null and banned_at is null"#
    )
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// Every anonymous ballot, in chain order, so a reader can walk it straight from
/// the file.
pub async fn anon_ballots(pool: &Pool) -> Result<Vec<AnonBallot>> {
    let rows = sqlx::query_as!(
        AnonBallot,
        r#"
        select p.slug as "poll_slug!", b.poll_id, b.seq, b.token, b.signature,
               b.msg_randomizer as "msg_randomizer?", b.cast_at, b.content_hash,
               b.prev_hash as "prev_hash?",
               array_agg(bo.option_id order by bo.option_id) as "option_ids!"
        from vote_ballots b
        join polls p on p.id = b.poll_id
        join ballot_options bo on bo.ballot_id = b.id
        where p.kind <> 'approval'
        group by b.id, p.slug
        order by p.slug, b.seq
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
