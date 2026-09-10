# Participation integrity: Sybil-resistance and detectability

Status: design for review. No code yet.

Anonymous voting proves a ballot cannot be linked to a voter and cannot be
double-spent on one entitlement. It does not prove that one account is one
person, and we say so. This document is about that gap: what we do, and
deliberately do not, do about it.

## 1. The decision, stated plainly

We are not pursuing cryptographic one person, one vote, and we do not promise it.
The honest, reachable goal for this platform is **Sybil-resistance and
detectability**: raise the cost of an extra vote, make mass manipulation
publicly visible, frame every number honestly, and never claim a uniqueness the
mechanism cannot deliver.

This follows from choices already made. Polls here are participatory, not
statistical, and no result is presented as representative of any population. A
real proof of personhood needs an identity anchor (a government ID, a payment
card, a phone number, a biometric), and every one of those fights the platform's
core values: anonymity, no stored identity, open access, and independence from
any one country. So the uniqueness *source* is the thing we refuse, not the
mechanism. Until a personhood primitive exists that is anonymous, self-contained
and values-consistent, we build resistance and visibility instead of a gate.

## 2. What this delivers, and what it does not

Delivers:

- **A public, honest picture of how each poll's numbers formed.** Anyone can see
  how many tokens a poll issued, how many were spent, how the votes were
  distributed over time, and how new the participating accounts were.
- **Detectability of the common attack.** Just-in-time account farms and burst
  voting leave a visible shape in these signals, even when they are not prevented.
- **A framing that cannot be read as more than it is.** Every signal ships with a
  plain-language note on what it does and does not show.

Does **not** deliver, and we will not claim it does:

- **One person, one vote.** An account is not a person. These signals make an
  attack costly and visible; they do not make it impossible.
- **A verdict on a poll.** We publish signals, not judgements. There is no
  "suspicious" label (see section 6).
- **Network-level attribution.** We do not, and by construction cannot, tie a
  ballot to an IP, a device, or a session (see section 3).

## 3. The one hard guardrail: detectability must never touch anonymity

The instinct for Sybil-detection is network signals: IP address, ASN, request
velocity, a device fingerprint. **None of these may ever be attached to a
ballot.** A ballot is anonymous precisely because nothing links it to the account
or the request that produced it; logging an IP next to a vote silently undoes the
entire blind-signature guarantee.

So the rule is absolute and load-bearing:

> Network and velocity signals live on the account-minting side (registration,
> sign-in), never on the ballot path. The vote handler learns no IP, sets no
> fingerprint, and writes nothing that could correlate a ballot to a request.

Everything in section 5 is therefore built from data that is either already
public (the anonymous ballots) or an aggregate that reveals nothing about any
single voter.

## 4. The two sides, kept separate

Sybil-resistance has two halves, and they must not bleed into each other:

- **Account-minting cost** (future epic, not this one): make each verified
  account expensive to create. Proof-of-work (already shipped via the captcha),
  disposable-domain rejection (shipped), email canonicalization (shipped), and
  later IP/ASN signup-velocity limits and an optional account-age gate before
  voting. This is where network signals belong, because registration is not
  anonymous the way a ballot is.
- **Detectability** (this epic): given whatever accounts exist, publish signals
  that show how a poll's participation formed, without ever deanonymizing a voter.

An optional third tier, an **anonymous device attestation** (Privacy Pass / PAT),
can later mark a ballot as carrying a real-device signal without revealing any
identity. It is a non-gating tier and a separate epic; it is noted here only so
the layered picture is on record. No external identity provider is in scope, ever.

## 5. The signals

All three are per poll. Two are recomputable from the public dump; two are
operator-attested aggregates that cannot be recomputed from anonymous data,
because recomputing them would need the very account data we refuse to publish.
That trade is stated openly wherever they appear: the signal exists, and you take
it on the same trust you already extend to the issued-token count.

### 5.1 Reconciliation (recomputable, except `eligible`)

Publish, per poll:

- `issued`  = number of entitlements (accounts that requested a token). Already
  published. Operator-attested.
- `spent`   = number of ballots. Recomputable by counting the published ballots.
- `eligible` = total verified, unbanned accounts on the platform at the time.
  Operator-attested, a ceiling, not a per-poll figure.

Anyone can confirm `spent <= issued <= eligible`. A poll whose `issued` jumped and
whose `spent` sits right at `issued` is the honest tell that participation was
manufactured, without any claim that it was.

### 5.2 Temporal distribution (recomputable)

A coarse histogram of ballot `cast_at` times (per hour, or per day for long
polls), rendered as a small monochrome sparkline on the poll page. Natural
participation spreads out; a burst is a spike. This is derived entirely from the
already-published ballots, so a third party computes the identical figure.

### 5.3 Cohort age (operator-attested aggregate)

For the accounts issued a token for the poll, the share that were **younger than
seven days when they were issued it**. Just-in-time account farms are the main
Sybil vector, and this is the signal that catches them.

It is anonymity-safe because it is an aggregate over **entitlements**, never over
**ballots**. It reports a property of the accounts that took part; it says
nothing about which ballots those accounts cast, so every vote stays hidden. It
is not recomputable from the public dump (it needs account creation times, which
we never publish), so it is published as an attested number, like `issued`.

### 5.4 Small-poll suppression

The temporal and cohort signals are suppressed (published as null, hidden on the
page) below a minimum participant count, so a poll with a handful of voters cannot
leak through a fine-grained aggregate. Proposed threshold: 25 participating
accounts. The reconciliation counts are always shown; they carry no per-voter
resolution.

## 6. How the signals are surfaced: data, not a verdict

We publish signals and let the reader and third parties conclude. We do **not**
show a "suspicious" or "verified" badge. Three reasons, each grounded in an
existing rule:

- It would be us **characterizing** a poll, which the content rules forbid for
  parties and people and which applies no less to participation.
- Any fixed threshold is **gameable**: an attacker simply stays under it, and the
  absence of a flag then reads as a clean bill of health it never earned.
- A false flag is an **accusation** against whoever ran or answered the poll.

So the poll page shows the neutral signals (the reconciliation line, the timeline
sparkline, the cohort share) and links to a public methodology page, "How to read
these numbers", that explains each one and its limits in plain language. The
strongest on-page interpretation we allow is a soft, neutral, threshold-gated
sentence stating a fact ("most of this poll's votes were cast within a short
window", "many participating accounts were new"), never a judgement and never a
label.

## 7. Data model and computation

No schema change is required for reconciliation or temporal: `vote_entitlements`,
`vote_ballots` and `users.created_at` already hold everything. The aggregates are
computed in the `db` layer:

- `spent`, `issued`: counts already available.
- `eligible`: count of verified, unbanned users.
- temporal histogram: bucket `vote_ballots.cast_at` by hour.
- cohort share: join `vote_entitlements` to `users` on `user_id`, the share with
  `issued_at - users.created_at < interval '7 days'`, returned only when the
  entitlement count meets the suppression threshold.

The cohort query touches `users` but returns a single ratio; it never returns
account rows and is never joined to ballots. That boundary is the invariant to
test.

## 8. Public dump and recompute

`GET /data/polls.json` gains, per poll: `eligible`, the `cast_at` histogram (or a
note that it is derivable from the ballots already present), and `cohort_new_share`
(nullable under suppression). `scripts/verify_chain.py` already checks
`spent <= issued`; a companion, documented recompute confirms the temporal
histogram against the published ballots and re-derives `spent`. The two attested
aggregates (`issued`, `cohort_new_share`) are labelled as attested, not
recomputable, so the dump never overstates what a stranger can independently check.

## 9. Honest claims (the exact words)

Wherever this surfaces:

- "These signals show how a poll's numbers formed: how many tokens it issued and
  spent, when the votes were cast, and how new the accounts were. They make
  large-scale manipulation costly and visible."
- "They do not prove one person, one vote (an account is not a person), nor that
  participants form a representative sample of any population."
- "Two of these figures (the issued count and the new-account share) are reported
  by us and cannot be recomputed from the anonymous data, because doing so would
  need the account details we do not publish."

No stronger than the mechanism supports, same discipline as the hash chain and the
recomputable tally.

## 10. Rollout (epic breakdown)

Each is its own PR with tests and, where copy is added, translations in all
catalogs.

1. **Reconciliation.** Add `eligible`; surface `issued` / `spent` / `eligible` on
   the poll page and in the dump; stub the methodology page.
2. **Temporal.** Compute the `cast_at` histogram; the sparkline on the poll page;
   publish it (or document its derivation); the recompute check.
3. **Cohort age.** The `db` aggregate with suppression; publish `cohort_new_share`;
   show the neutral share on the page.
4. **Methodology + honest claims.** The "How to read these numbers" page, linked
   from every poll, in all languages; wire the soft neutral notes behind their
   thresholds.

Out of scope here, tracked separately: account-minting cost (IP/ASN velocity, the
account-age gate) and the anonymous device-attestation tier.

## 11. Resolved decisions

- **Goal:** Sybil-resistance and detectability, not cryptographic one person, one
  vote, and never claimed as such.
- **Trust anchor:** self-contained, plus at most an anonymous device signal later.
  No external identity provider, ever.
- **First build:** this detectability layer, before any account-minting-cost work.
- **Cohort signal:** included, as an anonymity-safe aggregate over entitlements,
  with small-poll suppression.
- **Surfacing:** neutral signals and a methodology page, no verdict label.
