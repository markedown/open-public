//! Public data dumps. The trust core: sourcing shows where a fact came from and
//! the anonymous-voting scheme makes ballots verifiable, but neither can be
//! checked without a public record to check against. This serves that record.
//!
//! `/data/polls.json` is a deterministic dump of the anonymous participation
//! data: every poll's tally, its issuer public key, its issued-token count, its
//! ballot-chain head, and every ballot reduced to `(token, options, cast_at,
//! seq, hashes, signature)`. Anyone can recompute the tallies, verify each
//! ballot's blind signature under the poll key, confirm no token was spent
//! twice, walk the chain, and reconcile that ballots never exceed issued tokens.
//! It carries no identity: never a user id, never an email hash; a token is a
//! per-poll nullifier tied to no account.

use axum::extract::State;
use axum::Json;
use base64::Engine;
use serde::Serialize;

use crate::error::PageError;

#[derive(Serialize)]
pub struct PollsDump {
    /// The data is dedicated to the public domain.
    license: &'static str,
    /// The build the dump was produced by, so a snapshot ties to a commit.
    commit: &'static str,
    /// How to recompute and verify, and what this does and does not prove.
    note: &'static str,
    /// Accounts eligible to vote (verified, unbanned): the ceiling for how many
    /// tokens any poll can issue. The reconciliation bound is
    /// `spent <= issued <= eligible`.
    eligible: i64,
    polls: Vec<PollExport>,
    ballots: Vec<BallotExport>,
}

#[derive(Serialize)]
struct PollExport {
    slug: String,
    question: String,
    kind: String,
    /// The database id, published because the ballot chain hashes it. It says
    /// nothing about a person.
    poll_id: i64,
    total_votes: i64,
    /// The issuer public key (base64 SPKI), for verifying each ballot's token.
    public_key: Option<String>,
    /// Tokens issued for this poll. Ballots cast can never exceed this.
    issued: i64,
    /// Ballots cast for this poll. Recomputable by counting this poll's ballots.
    spent: i64,
    chain: Option<ChainHead>,
    options: Vec<OptionExport>,
}

#[derive(Serialize)]
struct ChainHead {
    seq: i64,
    hash: String,
}

#[derive(Serialize)]
struct OptionExport {
    position: i32,
    id: i64,
    label: String,
    votes: i64,
}

#[derive(Serialize)]
struct BallotExport {
    poll: String,
    poll_id: i64,
    seq: i64,
    /// The token (nullifier), the signature over it, and the message randomizer,
    /// all hex. Together they let anyone verify the ballot was issued for this
    /// poll and was spent only once.
    token: String,
    signature: String,
    randomizer: Option<String>,
    /// The option ids this ballot selected (sorted), hashed into the chain.
    options: Vec<i64>,
    cast_at: String,
    content_hash: String,
    prev_hash: Option<String>,
}

const NOTE: &str = "Anonymous poll participation data. Recompute each option's \
tally by counting the ballots that selected it; the counts here must match. A \
`token` is a per-poll nullifier, tied to no account and not linkable across \
polls. Verify each ballot with scripts/verify_chain.py, which checks: the blind \
signature over the token under the poll's public_key (the ballot was issued for \
this poll), that no token repeats (no double vote), the append-only hash chain, \
and the reconciliation `spent <= issued <= eligible` (each poll's ballots do not \
exceed its issued tokens, and issuance does not exceed the eligible-account \
count). This proves ballots were issued, unaltered, and un-double-voted, and \
that the operator cannot link a ballot to a voter. It does not prove one person \
one vote. `issued` and `eligible` are counts we attest; they cannot be recomputed \
from this file, because doing so would need the account data we do not publish. \
Each ballot carries its `cast_at`, so the timeline of when a poll's ballots were \
cast is recomputable by bucketing them.";

/// The anonymous poll-participation dump.
pub async fn polls(State(pool): State<db::Pool>) -> Result<Json<PollsDump>, PageError> {
    let tallies = db::export::ballot_tallies(&pool).await?;
    let raw = db::export::anon_ballots(&pool).await?;
    let eligible = db::export::eligible_voters(&pool).await?;

    // Tallies arrive ordered by (poll slug, option position), so consecutive
    // rows of one poll collect together.
    let mut polls: Vec<PollExport> = Vec::new();
    for t in tallies {
        let opt = OptionExport {
            position: t.position,
            id: t.option_id,
            label: t.label,
            votes: t.votes,
        };
        match polls.last_mut() {
            Some(p) if p.slug == t.slug => {
                p.total_votes += opt.votes;
                p.options.push(opt);
            }
            _ => {
                let chain = t.head_seq.filter(|s| *s > 0).map(|seq| ChainHead {
                    seq,
                    hash: hex(t.head_hash.as_deref().unwrap_or(&[])),
                });
                polls.push(PollExport {
                    slug: t.slug,
                    question: t.question,
                    kind: t.kind,
                    poll_id: t.poll_id,
                    total_votes: opt.votes,
                    public_key: t
                        .public_key
                        .map(|der| base64::engine::general_purpose::STANDARD.encode(der)),
                    issued: t.issued,
                    spent: t.spent,
                    chain,
                    options: vec![opt],
                });
            }
        }
    }

    let ballots = raw
        .into_iter()
        .map(|b| BallotExport {
            poll: b.poll_slug,
            poll_id: b.poll_id,
            seq: b.seq,
            token: hex(&b.token),
            signature: hex(&b.signature),
            randomizer: b.msg_randomizer.as_deref().map(hex),
            options: b.option_ids,
            cast_at: b.cast_at.to_rfc3339(),
            content_hash: hex(&b.content_hash),
            prev_hash: b.prev_hash.as_deref().map(hex),
        })
        .collect();

    Ok(Json(PollsDump {
        license: "CC0-1.0",
        commit: option_env!("GIT_SHA").unwrap_or("unknown"),
        note: NOTE,
        eligible,
        polls,
        ballots,
    }))
}

/// Lowercase hex of a byte slice.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}
