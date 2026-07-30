# Trust-minimized anonymous voting: design

Status: draft for review. No code yet.

This describes how a vote becomes **anonymous** and **provably one-per-account**
while minimizing what a voter must trust the operator with. It does not eliminate
that trust: the operator still holds the signing key and could over-issue tokens,
a bound that is public and checkable (see section 8). It is the participation half
of the trust-minimization goal, and it sits on the same cryptographic primitive as
the bot-resistance work (Private Access Tokens): **RFC 9474 RSA blind signatures**.

## 1. What this delivers, and what it does not

Delivers:

- **Anonymity the operator cannot break.** The operator (us) cannot link a cast
  vote to the account that cast it, even with full database access and the
  server keys. Not "we promise not to look": we *cannot*.
- **One vote per eligible account per poll, publicly.** A second vote from the
  same entitlement is refused, and anyone can recompute from the public dump
  that no entitlement voted twice.
- **Recomputable tallies.** Anyone can recompute each poll's result from the
  published data and confirm the count.

Does **not** deliver, and we will never claim it does:

- **Uniqueness / one-person-one-vote.** This makes a vote anonymous and
  un-double-votable; it does not prove one human is behind one account. Sybil
  resistance stays where it is: the cost of minting a verified account (captcha,
  disposable-domain block, email canonicalization) plus the optional PAT device
  signal. An anonymous vote is one-per-*account*, and an account is not a person.
- **Receipt-freeness.** A voter who *wants* to prove to a third party how they
  voted can. Preventing vote-buying needs coordinator-based machinery (MACI
  class) that is heavy and reintroduces a trusted party; out of scope. Stated
  honestly.
- **Protection against a network-global observer.** See the timing caveat in §7.

## 2. Threat model

Actors and what the design allows each:

| Actor | Can | Cannot |
|---|---|---|
| **The operator** (full DB + server keys) | See who *requested* a token for a poll; see the set of anonymous votes; forge at most as many votes as it can create eligible accounts (bounded, see §8) | Link a vote to the account that cast it; deanonymize a voter |
| **A curious outsider** (has the public dump) | Recompute tallies; verify no double-spend; verify every vote's token signature | Learn who anyone voted for; forge a vote |
| **A ballot-stuffer** | Vote once per account they control | Vote twice on one entitlement; vote without a verified, unbanned account |
| **A passive network observer** | Correlate a token *request* and a *vote* that happen close together in time (§7) | Break the cryptographic unlinkability itself |

The design's integrity against a *malicious operator* reduces to the same Sybil
bound everyone faces (§8): the operator can stuff at most one vote per eligible
account it can create, which is exactly the uniqueness limit we are already
honest about. It cannot deanonymize regardless.

## 3. Why blind RSA (and not zk / Semaphore)

We evaluated Semaphore (Groth16 zk-membership) and chose RFC 9474 blind RSA. On
the stated priorities, usability first, then trust minimization, then professional
engineering:

- **Usability.** Blind RSA needs **no persistent client key** (nothing to lose or
  back up, works on any device by just logging in), is **near-instant**, needs a
  **small pure-JS crypto lib and no WASM**. Semaphore needs a persistent identity
  secret, multi-megabyte WASM proving artifacts, and a 1-3s proof on a phone.
- **Trust minimization.** Identical: under both, the operator cannot relink a vote.
- **Professional / standards.** RFC 9474 is an IETF standard, audited, deployed at
  scale (it is the primitive behind Privacy Pass and PAT). Rust:
  [`blind-rsa-signatures`](https://crates.io/crates/blind-rsa-signatures)
  (the PAT crate). JS: the `@noble` family (audited, tiny, pure JS).
- **Coherence.** The same blind-token machinery serves both "a real device"
  (PAT, §9) and "an eligible anonymous voter" (this). One trust story, not two.

The one place Semaphore is genuinely stronger is the timing caveat (§7); we close
most of that gap with token pre-fetch instead of paying Semaphore's UX cost.

## 4. Cryptographic construction

Base RFC 9474 RSA blind signatures. **One issuer keypair per poll**, generated
when the poll is created. A token signed by poll P's key verifies only for poll
P, so a token cannot be moved between polls, and we avoid the newer
partially-blind (public-metadata) extension, staying on the mature base RFC.

Per poll `P` with issuer keypair `(sk_P, pk_P)`:

```
issue (authenticated, sees the account):
  client:  t   <- random 256-bit token nonce
           m   <- H(t)                       # the message to be signed
           (bm, r) <- blind(pk_P, m)         # bm blinded with secret factor r
  server:  require account verified, unbanned, and no prior token for (account, P)
           record issuance (account, P, issued_at)   # NO token value stored
           bs  <- blind_sign(sk_P, bm)       # signs without seeing m
  client:  s   <- finalize(pk_P, bs, r)      # unblind -> signature s over m
           store (t, s) locally, briefly, until voting

vote (anonymous, no account, ideally a separate request/session):
  client:  submit (P, option_set, t, s)
  server:  require valid(pk_P, H(t), s)      # signature check
           require t not in spent[P]         # no double-spend
           append vote (P, option_set, t, cast_at) to the chain
           insert t into spent[P]
```

The issuer signs `bm` without learning `m`; the blinding factor `r` makes the
signed `bm` and the later-revealed `(t, s)` cryptographically uncorrelatable. The
token `t` is the **nullifier**: unique, published, tied to no account.

## 5. Data model

Two tables that share only `poll_id`. Nothing links a spend to an account.

```
poll_issuer_keys (
  poll_id       PK FK polls,
  public_key    bytea NOT NULL,   -- published; verifiers and clients use it
  private_key   bytea NOT NULL,   -- server only; used to blind-sign
  created_at    timestamptz
)

vote_entitlements (              -- "account A was issued a token for poll P"
  id, poll_id FK, user_id FK,
  issued_at timestamptz,
  UNIQUE (poll_id, user_id)       -- one entitlement per account per poll
)                                 -- NO token, NO signature: the server never
                                  -- saw them (they were blinded)

vote_ballots (                   -- the anonymous votes
  id, poll_id FK,
  token        bytea NOT NULL,    -- the nullifier t
  seq          bigint,            -- per-poll position (existing chain)
  content_hash bytea,             -- chain hash of (poll, option_set, token, seq, prev)
  prev_hash    bytea,
  cast_at      timestamptz,
  UNIQUE (poll_id, token)         -- no double-spend
)
ballot_options ( ballot_id FK, option_id FK, PK(both) )
```

`vote_ballots` replaces the account-keyed `poll_votes` for anonymous polls.
Because production has **zero real votes**, this is a clean cutover, not a
migration of live data. The existing per-poll hash chain (`poll_chains`) and its
verify script carry over, hashing the token instead of a voter index.

## 6. What the operator can see, precisely

- In `vote_entitlements`: that account A asked for a token for poll P, and when.
- In `vote_ballots`: the set of anonymous votes for P, each with a token and time.
- The two share only `poll_id`. The token in a ballot is uncorrelatable to any
  blinded message the issuer signed. So the operator sees *that A participated in
  P* but not *how A voted*, and cannot compute the link even with `sk_P`.

"A participated in P" is itself metadata worth minimizing; see §7.

## 7. The timing caveat and its mitigation

Blind signatures give cryptographic unlinkability, but issuance and voting are two
observable events. If account A requests a token at 12:00:00 and a vote lands at
12:00:03, a party watching *both* events (the operator's logs, or a network-global
observer) can correlate them by timing, even though the crypto does not link them.

Mitigations, in the design:

- **Pre-fetch.** The client fetches the entitlement token as soon as the poll page
  loads (or the poll opens), then votes whenever the person actually decides.
  Most voters naturally separate the two by minutes, collapsing the timing window.
- **No coupling in the request.** The vote is submitted as its own request, not
  piggy-backed on the authenticated session that issued the token; ideally the
  client is encouraged to let time pass.
- **Batching.** Tokens are issued the moment a poll opens for everyone who loads
  it, so issuance events cluster and carry little signal.

We will **state this limit plainly** wherever the guarantee is described: "a party
able to observe both your token request and your vote, close in time, could
correlate them; the cryptography does not link them." We do not claim protection
against a global passive observer. If that ever becomes a requirement, the upgrade
path is a reusable anonymous credential with a zk show (Semaphore/BBS+), which
decouples issuance from voting entirely, at the UX cost we chose to avoid now.

## 8. Trust in the issuer, and its bound

The operator holds `sk_P`, so it *could* sign extra tokens and stuff a poll. This
is **ballot-stuffing, not deanonymization**: even a malicious issuer still cannot
link real votes to people. And it is **bounded and detectable**:

- Publish, per poll, the **issued count** (rows in `vote_entitlements`) and the
  **spent count** (rows in `vote_ballots`). Anyone can check `spent <= issued`.
- Issuance is authenticated: `issued <= number of eligible accounts`. So the
  operator's stuffing power is at most one extra vote per eligible account it can
  fabricate, which is exactly the Sybil bound everyone faces. Over-issuance beyond
  the eligible-account count is publicly visible.

An issuer that need not be trusted at all (threshold signing, or a public
append-only issuance log with distributed keys) is possible later but out of scope
now; the reconciliation above is the honest, professional bound we ship with, and
we document it.

## 9. Relationship to PAT (uniqueness, separate layer)

PAT (Private Access Tokens) is the *uniqueness* lever and uses the *same* blind
primitive from the other side: the browser/Cloudflare attests "a real device"
with a zero-knowledge token we verify but learn nothing from. It raises the cost
of minting fake participation; it is a **bonus tier**, never a gate (coverage is
uneven). PAT and this voting layer compose: PAT makes the *eligible set* harder to
fake, blind-sig voting makes each vote *anonymous and un-double-votable*. Neither
is one-person-one-vote; together they are the honest best we can do without
collecting identity.

## 10. Client: the voting island

Voting becomes a small **JavaScript island** (blinding, unblinding, submission)
using a pure-JS blind-RSA implementation, **no WASM**. Anonymity requires
client-side crypto, so casting a vote requires JavaScript: the client blinds the
token itself, because the moment the server does the blinding it sees what it
signs and the anonymity is gone. The rest of the site stays server-rendered and
works without JS. The poll page renders fully (question, options, results) without
JS; only *casting* an anonymous vote needs the island. A no-JS visitor sees the
poll and results and a clear message that voting needs JavaScript enabled, rather
than a broken control.

## 11. Public verifiability

The nightly dump (already published) gains, per poll: `pk_P`, the issued count,
and every `(token, option_set, cast_at, seq, hashes)`. Then anyone can:

1. verify each ballot's token signature under `pk_P`,
2. verify no token repeats (no double-spend),
3. verify the hash chain head,
4. recompute the tally,
5. check `spent <= issued <= eligible-accounts`.

`scripts/verify_chain.py` is extended to do 1-3 and 5; the tally recompute is
already documented. This closes the loop: integrity (chain), anonymity (blind
sig), no-double-vote (nullifier), and honest bounds (reconciliation), all
checkable by a stranger.

## 12. Honest claims (the exact words)

Wherever this surfaces, we say only:

- "Your vote is anonymous: we cannot tell how you voted, even with full access to
  our own systems."
- "Each account can vote once per poll, and anyone can verify from the public data
  that none voted twice."
- "This does not prove one person, one vote (an account is not a person), and a
  party watching both your token request and your vote, close in time, could
  correlate them. The cryptography does not link them."

No stronger than the mechanism proves. Same discipline as the hash chain and the
recomputable dump.

## 13. Rollout (epic breakdown)

1. **Spike (throwaway).** Prove the full loop end-to-end: Rust `blind-rsa-signatures`
   for keygen + blind-sign + verify, and a pure-JS client for blind + finalize.
   Confirm a token issued for poll P verifies at spend and cannot be linked. Report
   sizes and timings on a phone. **Gate before the epic.**
2. **Schema + issuer.** Migration for the three tables; per-poll keygen on poll
   creation; key storage. No endpoints yet.
3. **Issuance endpoint.** Authenticated: eligibility + one-per-(account,poll) +
   blind-sign. Records the entitlement, never the token.
4. **Vote/spend path.** Anonymous: verify signature, check unspent, append to the
   chain, mark spent. Replace the account-keyed vote path for anonymous polls.
5. **JS voting island.** Blinding/unblinding/submit; pre-fetch; no-JS message.
6. **Public dump + verify script.** Extend both; document the recompute + reconcile
   commands.
7. **UI + honest claims.** The exact wording in §12; tiered results (PAT) slot.
8. **Cutover.** Since prod has zero votes, switch anonymous polls onto the new path
   and retire the old `poll_votes` write path.

Each is its own PR with tests. The spike (1) comes back for review before 2-8.

## 14. Resolved decisions

- **Scope:** *all* participation polls become anonymous under this, approval polls
  included. There is no simple account-vote path in parallel.
- **Self-state:** a person sees their own "you have voted on this" state, derived
  from `vote_entitlements` (which their account owns) without revealing the token
  and without linking to any ballot.
- **Key destruction at close:** the poll's private key `sk_P` is destroyed when the
  poll closes; only `pk_P` is kept, for verification. A later compromise cannot
  forge into a closed poll.
- **Timing hardening:** pre-fetch now (grab the token on poll-page load, vote
  whenever). Mandatory delay / token pool is a later option if needed, not built
  now.
