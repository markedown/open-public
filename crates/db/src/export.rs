//! Read models for the public data dump. The dump publishes the participation
//! data so anyone can recompute the tallies and verify the vote hash chain
//! against the running site. It never exposes identity: votes carry only the
//! opaque per-poll `voter_index` already stored on each row, never a user id or
//! an email hash.

use chrono::{DateTime, Utc};

use crate::{Pool, Result};

/// One poll option's tally, flattened with its poll and chain head.
#[derive(Debug, Clone)]
pub struct PollTally {
    pub slug: String,
    pub question: String,
    pub kind: String,
    pub position: i32,
    pub label: String,
    pub votes: i64,
    /// The vote-chain head sequence and hash, `None` for a poll with no votes.
    pub head_seq: Option<i64>,
    pub head_hash: Option<Vec<u8>>,
}

/// One anonymized vote: the poll, the option, when it was cast, and the opaque
/// per-poll voter index. No identity is present.
#[derive(Debug, Clone)]
pub struct AnonVote {
    pub poll_slug: String,
    pub option_position: i32,
    pub cast_at: DateTime<Utc>,
    pub voter_index: i64,
}

/// Every poll's per-option tally and chain head, deterministically ordered so
/// the dump is diffable.
pub async fn poll_tallies(pool: &Pool) -> Result<Vec<PollTally>> {
    let rows = sqlx::query_as!(
        PollTally,
        r#"
        select p.slug as "slug!", p.question as "question!", p.kind as "kind!",
               o.position as "position!", o.label as "label!", count(v.id) as "votes!",
               c.head_seq as "head_seq?", c.head_hash as "head_hash?"
        from polls p
        join poll_options o on o.poll_id = p.id
        left join poll_votes v on v.option_id = o.id
        left join poll_chains c on c.poll_id = p.id
        group by p.id, o.id, c.head_seq, c.head_hash
        order by p.slug, o.position
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Every vote, anonymized and deterministically ordered.
pub async fn anonymized_votes(pool: &Pool) -> Result<Vec<AnonVote>> {
    let rows = sqlx::query_as!(
        AnonVote,
        r#"
        select p.slug as "poll_slug!", o.position as "option_position!",
               v.cast_at, v.voter_index
        from poll_votes v
        join polls p on p.id = v.poll_id
        join poll_options o on o.id = v.option_id
        order by p.slug, v.voter_index, o.position
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
