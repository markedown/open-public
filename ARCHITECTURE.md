# Architecture

This document is the map of the codebase: the pieces, how they fit, and the invariants that hold them
together. It is deliberately high-level. For exact table and column names, content rules, the design
system and conventions, see [`CLAUDE.md`](./CLAUDE.md).

## Shape

open-public is one Rust binary plus one PostgreSQL database. There is no separate search engine, no
message queue, no service mesh, and no client-side framework. Postgres handles both relational data
and full-text search, and the binary server-renders every page. This is a deliberate constraint: one
binary and Postgres until it hurts.

```mermaid
flowchart LR
    browser["Browser"]

    subgraph server["server (Axum, single binary)"]
        handlers["routes, handlers,<br/>maud templates"]
        assets["static assets:<br/>vendored htmx.min.js + generated CSS"]
    end

    db["db<br/>(SQLx repositories)"]
    pg[("PostgreSQL")]
    ingest["ingest<br/>(binaries)"]

    browser -- "request" --> handlers
    handlers -- "HTML page or fragment" --> browser
    assets -- "same origin" --> browser
    handlers --> db
    ingest --> db
    db --> pg
```

The client runs no application code of ours. HTMX (a single vendored script) turns ordinary links and
forms into partial-page requests; the server answers those with small HTML fragments instead of a full
page. Nothing depends on JavaScript being present.

## Crates and dependency direction

The workspace is four crates. Dependencies point one way; nothing lower depends on anything higher.

- **`domain`** holds the project's vocabulary: shared types and pure, locale-aware text helpers (slug
  generation, casing). It has no SQLx and stays dependency-light, so every other crate can depend on
  it freely. Pure helpers here are unit-tested.
- **`db`** holds every SQL query and repository function, using SQLx with compile-time-checked
  queries. Nothing above this layer writes SQL; the `db` API is the only way to touch Postgres.
- **`server`** is the Axum binary and the deployable artifact. It holds the request handlers, the
  `maud` HTML templates (one page per handler), the reusable `ui` component functions (layout, cards,
  badges, timeline entries, the poll widget, source links), and the static assets (the vendored HTMX
  script and the generated Tailwind CSS). Handlers are thin: validate input, call a `db` function,
  render a template. No inline SQL lives here.
- **`ingest`** holds standalone data-import binaries and runs outside the request path. What ships
  here is the loading direction: `ingest import` reads the published dataset back into a database.
  The harvesters that fetch from third-party sites are operational tooling and are not part of this
  repository, so the code that reaches out into the world and the code anyone can run over the
  published files are deliberately separate.

## Data model invariants

Two rules are enforced everywhere, including in seed data and test fixtures:

1. **Every fact references a source.** Each row that asserts a fact (a person, party, role,
   membership, statement, or news item) carries a `source_id` into the `sources` table. No source,
   no insert.
2. **Time-varying facts are relations, not columns.** Party membership, roles and positions are
   stored with `start_date`/`end_date` (a `NULL` end means "current"), never as a mutable flat field.
   The record of *when* something was true is never overwritten.
3. **A position is derived, never asserted.** Nowhere does a row say "this party holds this view".
   A stance is resolved from the evidence recorded for it, so it cannot contradict what it rests on.

Full-text search is Postgres-native: `tsvector` indexes over the searchable text columns, queried
through the `db` layer. There is no external index to keep in sync.

## Positions and evidence

A compass matches a visitor's answers against the contestants of an election, and the interesting part
is where a contestant's position comes from.

A **thesis** is one policy proposition a visitor answers, scoped to what the election is contested by:
parties for a parliamentary election, people for a presidential one. Against each thesis sits
**evidence**: a dated, typed, sourced reading, attached to exactly one party or one person.

Evidence comes in two tiers. *Recorded action* is what a contestant did (a vote, a law, a decree, a
governing alliance, a bill it tabled, an application to a constitutional court). *Stated intention* is
what it said it would do (a manifesto, a statement). The effective stance is the strongest evidence
available: recorded action first, then the most recent within a tier. Two consequences follow, and
both are deliberate.

- A party that pledges one thing and legislates another is scored on what it legislated, and the page
  says so rather than averaging the two away.
- Opposition parties can be judged on their record too, which is why tabling a bill and going to court
  count as action. Without them, only whoever holds power could ever be measured by deeds.

The questionnaire itself is stateless and anonymous: answers arrive in the POST body, drive the score
in memory, and are gone when the response is rendered. Nothing a visitor enters is stored or logged.

## The published dataset

`dataset/` holds everything the platform knows, one JSON file per entity type, released under CC0. It
is produced by an export that runs outside this repository and read back by `ingest import`.

The design constraint is that a dataset nobody can check is not worth publishing:

- **Natural keys, never database ids.** Rows name each other by slug, and a source by its url and
  content hash together. Ids renumber when a database is rebuilt, and that must not read as a change.
- **Deterministic output.** Sorted keys, sorted rows, and a *total* ordering in every file, so two
  rows can never tie and be separated by whatever order they happen to sit in on disk. A day with no
  editorial change produces an empty diff.
- **A round trip that is checked, not asserted.** Importing the published files into an empty database
  and exporting again reproduces them byte for byte. The test runs against the real files, so the
  published data and the schema it describes cannot drift apart unnoticed.
- **Guarantees as code.** `scripts/validate_data.py` fails if any personal or unreviewed field
  appears, if a fact cites a source that is not in the export, if a reference does not resolve, or if
  a file collapses against the previous manifest. It runs in CI.

What a source row now carries is part of the same argument. A url and a title say which document; they
do not say *which version* of it, so a source also records when it was fetched, the SHA-256 of the
bytes where we downloaded them, and an archived copy where one exists. A quotation additionally
carries its locator (a page, an article, a roll call) on the citation rather than on the source,
because two readings routinely cite different pages of one programme.

## Request lifecycle

A request enters the `server` binary and is routed to a handler. The handler validates its input,
fetches data through `db` repository functions (compile-time-checked queries against Postgres), and
renders a `maud` template into HTML. A normal navigation returns a complete page; an HTMX-driven
interaction returns just the fragment that changed, which HTMX swaps into the DOM. Because every page
is fully rendered server-side and every enhanced interaction has a plain-form fallback, the site works
without JavaScript.

## Styling and assets

Styling is Tailwind CSS compiled by the standalone Tailwind CLI into a single stylesheet, served as a
static asset. HTMX is vendored into the static directory and served from the same origin, with no CDN
and no external font host. The interface is deliberately near-monochrome; the only saturated color on
a page comes from data (such as a party badge).

## Ingestion

Ingest binaries are idempotent: they upsert on external identifiers (such as `wikidata_id`), so
running them twice never duplicates rows. They rate-limit per host, send a descriptive User-Agent with
contact information, respect `robots.txt`, and never fetch paywalled content. When two sources
disagree, the discrepancy is written to `data_conflicts` rather than overwriting the existing value,
following the project's source-trust order.

## Accounts, voting, and identity

Participation runs on lightweight accounts. A person registers with an email and a password: the
password is stored only as an argon2 hash, and the email is never kept in plaintext, only as an HMAC
hash keyed by a server secret. Registration is not finished until the address is confirmed. A one-time
verification link is mailed out, and login is refused until it is used. Sessions are opaque random
tokens; only their SHA-256 hash is stored server-side, and the cookie is `HttpOnly`, `SameSite=Lax`,
and `Secure` when the site is served over https.

One vote per account per poll is enforced by a uniqueness constraint, and `poll_votes` rows are never
updated or deleted, not even by admins. Corrections happen by closing a poll and opening a new one.
Verification deduplicates accounts; it does not sample a population, so no result is presented as a
representative survey. Registration, verification, and login are rate limited at the edge (reverse
proxy), not in the application.

Votes are additionally tamper-evident. Each poll has a hash chain: every vote hashes its own content
together with the previous row's hash, so altering or removing a vote after the fact breaks every hash
after it. The chain head is shown on the poll page, and `GET /data/polls.json` publishes the
participation record it can be checked against: each poll's tally, its chain head, and every vote
reduced to `(poll, option, cast_at, opaque per-poll voter index)`. Anyone can recompute the tallies
from the votes and compare the head against the running site. The dump carries no identity, never a
user id and never an email hash. What this proves is that votes have not been altered, not that one
person cast one vote, and the difference is never blurred.

User-submitted polls pass a two-tier gate before anyone else sees them: an automated content
pre-screen behind a pluggable trait, then admin approval. With no reviewer configured, submissions
queue for a human rather than auto-publishing. Uploaded images are never trusted by their declared
type: the format is detected from content, decoded under strict size and dimension limits, and
re-encoded to a normalized raster, which strips metadata and defeats polyglot files. Only the
re-encoded bytes are stored, content-addressed, and they stay visible to their uploader and admins
until the poll is approved.

## Migrations and offline builds

Migrations are SQLx migrations and are **append-only**: an applied migration is never edited. The
`.sqlx/` offline query metadata is committed to the repository so that CI (and any build) can compile
the compile-time-checked queries with `SQLX_OFFLINE=true` and no live database.

The runtime image carries `migrations/` as well as the binary. A deployment therefore applies the
schema that belongs to the code it is starting, instead of trusting whatever copy of the repository
happens to sit next to it on the host.

## Deployment interface

The repository's deployment interface is intentionally narrow: a multi-stage Docker image plus a set
of documented environment variables (database URL, mail settings, the HMAC secret). Concrete hosting,
DNS and infrastructure live outside this repository and are never committed.

## Verifiable builds

The release path is auditable end to end, so anyone can check that a running instance corresponds to a
specific public commit.

- On a `v*` tag, the public `release.yml` workflow builds the image and generates a build-provenance
  attestation (`actions/attest-build-provenance`) that binds the source commit and the workflow run to
  the resulting image digest. The attestation is pushed to the registry and can be checked with
  `gh attestation verify`.
- Every third-party action in the workflows is pinned to a full commit SHA, so a moved tag cannot
  silently change what the build does.
- Images are referenced by digest, never by a mutable tag; deployment pulls by digest.
- The server exposes `GET /version`, which returns the commit and build timestamp (baked into the
  binary at build time, never read from `.git` at run time) and the image digest (supplied at run time
  by the deployment). `GET /health` is a liveness probe.

The chain is: source commit, then public build workflow, then attested image digest, then `/version`
reporting that digest. Each link can be checked on its own.
