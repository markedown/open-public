//! Public data dumps. The trust core: sourcing shows where a fact came from and
//! the vote hash chain makes votes tamper-evident, but neither can be checked
//! without a public record to check against. This serves that record.
//!
//! `/data/polls.json` is a deterministic, anonymized dump of the participation
//! data: every poll's tally, every poll's chain head, and every vote reduced to
//! `(poll, option, cast_at, opaque voter index)`. Anyone can recompute the
//! tallies from the votes, and verify the chain head against the running poll
//! page. It carries no identity: never a user id, never an email hash.

use axum::extract::State;
use axum::Json;
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
    polls: Vec<PollExport>,
    votes: Vec<VoteExport>,
}

#[derive(Serialize)]
struct PollExport {
    slug: String,
    question: String,
    kind: String,
    total_votes: i64,
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
    label: String,
    votes: i64,
}

#[derive(Serialize)]
struct VoteExport {
    poll: String,
    option: i32,
    cast_at: String,
    voter: i64,
}

const NOTE: &str = "Anonymized poll participation data. Recompute each option's \
tally by counting the votes with that (poll, option); the counts here must match. \
`voter` is an opaque per-poll index, not linkable across polls or to any person. \
`chain` is the append-only vote hash-chain head; check it against the poll page \
and verify no vote was altered or removed with scripts/verify_chain.py. This \
proves votes were not altered or removed after casting, not one-person-one-vote.";

/// The anonymized poll-participation dump.
pub async fn polls(State(pool): State<db::Pool>) -> Result<Json<PollsDump>, PageError> {
    let tallies = db::export::poll_tallies(&pool).await?;
    let raw_votes = db::export::anonymized_votes(&pool).await?;

    // Tallies arrive ordered by (poll slug, option position), so consecutive
    // rows of one poll collect together.
    let mut polls: Vec<PollExport> = Vec::new();
    for t in tallies {
        let opt = OptionExport {
            position: t.position,
            label: t.label,
            votes: t.votes,
        };
        if polls.last().is_some_and(|p| p.slug == t.slug) {
            let p = polls.last_mut().expect("just checked non-empty");
            p.total_votes += opt.votes;
            p.options.push(opt);
        } else {
            let chain = t.head_seq.filter(|s| *s > 0).map(|seq| ChainHead {
                seq,
                hash: hex(t.head_hash.as_deref().unwrap_or(&[])),
            });
            polls.push(PollExport {
                slug: t.slug,
                question: t.question,
                kind: t.kind,
                total_votes: opt.votes,
                chain,
                options: vec![opt],
            });
        }
    }

    let votes = raw_votes
        .into_iter()
        .map(|v| VoteExport {
            poll: v.poll_slug,
            option: v.option_position,
            cast_at: v.cast_at.to_rfc3339(),
            voter: v.voter_index,
        })
        .collect();

    Ok(Json(PollsDump {
        license: "CC0-1.0",
        commit: option_env!("GIT_SHA").unwrap_or("unknown"),
        note: NOTE,
        polls,
        votes,
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
