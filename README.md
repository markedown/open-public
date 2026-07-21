<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="crates/server/static/brand/wordmark-dark.svg">
    <img alt="open-public" src="crates/server/static/brand/wordmark.svg" width="300">
  </picture>
</p>

<p align="center">
  <a href="https://github.com/markedown/open-public/releases"><img alt="Latest release" src="https://img.shields.io/github/v/release/markedown/open-public?label=version&color=33527a"></a>
  <a href="https://github.com/markedown/open-public/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/markedown/open-public/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://github.com/markedown/open-public/actions/workflows/verify-production.yml"><img alt="Production verified" src="https://github.com/markedown/open-public/actions/workflows/verify-production.yml/badge.svg"></a>
  <a href="https://github.com/markedown/open-public/actions/workflows/ci.yml"><img alt="Coverage: at least 97%" src="https://img.shields.io/badge/coverage-%E2%89%A597%25-brightgreen.svg"></a>
  <a href="#license"><img alt="License: MIT OR Apache-2.0" src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg"></a>
  <a href="#license"><img alt="Data: CC0-1.0" src="https://img.shields.io/badge/data-CC0--1.0-blue.svg"></a>
  <img alt="Rust: stable" src="https://img.shields.io/badge/rust-stable-orange.svg">
  <img alt="PostgreSQL 18" src="https://img.shields.io/badge/PostgreSQL-18-336791.svg">
</p>

**open-public** is an open, source-backed record of public political life: who holds office, the
parties they belong to, how those roles and memberships shift over time, the elections that put them
there, where they stand on the questions of the day, the news that mentions them, and polls anyone can
vote in. Every fact links to the source it came from, so it can be checked, and anything that changes
over time is kept as dated history instead of being overwritten. The code is open source, the
[dataset](#the-dataset) is public domain, and the platform is country-agnostic.

Three rules shape the whole codebase and are enforced in review:

1. **No fact without a source.** Every record in the database references a `sources` row, seed data
   included. A source is provenance, not a lock: a correction is a new sourced edit.
2. **History, not overwrites.** Anything that varies over time (a membership, a role, a position) is a
   time-ranged relation with start and end dates, never a column that gets rewritten in place.
3. **A position is derived, never asserted.** Where a party stands on a question is read from dated,
   typed, sourced evidence, so a stance can never contradict what it rests on. What a party did
   outranks what it promised, and where the two disagree, both are shown.

## Architecture

One Rust binary and PostgreSQL. There is no separate search engine, no message queue, and no
client-side framework.

| Crate | Responsibility |
| --- | --- |
| `domain` | Shared types and locale-aware text helpers (slugs, casing). |
| `db` | SQLx queries and repository functions, checked at compile time. |
| `server` | Axum binary: request handlers, `maud` HTML templates, reusable `ui` components, static assets. |
| `ingest` | Standalone data-import binaries, run outside the request path. |

Full detail is in [`ARCHITECTURE.md`](./ARCHITECTURE.md).

- **Web:** [Axum](https://github.com/tokio-rs/axum) serves server-rendered HTML built with
  [`maud`](https://maud.lambda.xyz), a macro that turns Rust into type-checked templates. Pages work
  without JavaScript. [HTMX](https://htmx.org) (vendored, never loaded from a CDN) adds progressive
  enhancement for actions like voting and search, and every enhanced action has a plain-form fallback.
- **Styling:** [Tailwind CSS](https://tailwindcss.com) built with the standalone Tailwind CLI. No CSS
  component library.
- **Data:** PostgreSQL for both relational data and full-text search.
  [SQLx](https://github.com/launchbadge/sqlx) 0.9 with compile-time-checked queries. The `.sqlx/`
  offline metadata is committed so CI builds without a live database.
- **Ingestion:** the `ingest` crate holds standalone import binaries, idempotent by upserting on
  natural keys. `ingest import` loads the published [dataset](#the-dataset) into a database. The
  harvesters that fetch from third-party sites are run separately and are not part of this
  repository.
- **Verifiable deployment:** each release is an attested build, production is pinned to and
  continuously verified against those attested digests, and `GET /version` reports the running commit
  and digest (`GET /health` is liveness, `GET /readyz` readiness). See
  [Verifiable deployment](#verifiable-deployment) below.

The database schema lives in [`migrations/`](./migrations), the design system in
[`DESIGN.md`](./DESIGN.md), architecture and request flow in [`ARCHITECTURE.md`](./ARCHITECTURE.md),
and contribution conventions in [`CONTRIBUTING.md`](./.github/CONTRIBUTING.md).

## Verifiable deployment

Production runs only released, attested code, and anyone can check it independently.

1. **Attested builds.** Each release (a `v*` tag) is built by a public GitHub Actions workflow that
   pushes the image to GHCR and attaches a [SLSA](https://slsa.dev) build-provenance attestation,
   recorded in the public [Rekor](https://docs.sigstore.dev/logs/overview/) transparency log. Anyone
   can confirm which commit and which public workflow produced a given digest:

   ```
   gh attestation verify oci://ghcr.io/markedown/open-public@<digest> --repo markedown/open-public
   ```

2. **Digest-pinned deploys.** Production pulls the image by digest, never a mutable tag, and refuses
   any image that is not an attested release build. The running digest, commit, and build time are
   reported at `GET /version`. The image also carries the migrations that belong to that build under
   `/app/migrations`, so a deployment applies the schema the code expects rather than whatever copy
   of the repository happens to sit beside it.

3. **Continuous public verification.** A scheduled workflow, [`verify-production.yml`](./.github/workflows/verify-production.yml),
   runs on GitHub's infrastructure (independent of the host), reads `/version`, and verifies the
   reported digest is an attested release build. Any drift is a public, failing check.

What this proves, and what it does not: it proves every production image is publicly attested to the
source commit and public workflow that built it, that production is pinned to and continuously checked
against those attested digests, and that an independent public job confirms the running digest. It
does not, on its own, defeat a malicious host that forges `/version`; that would require hardware
remote attestation, which is out of scope and is never claimed.

## The dataset

Everything the platform knows is published in [`dataset/`](./dataset), one JSON file per entity type,
under [CC0](./LICENSE-DATA). It is the same data the site serves.

```bash
# load it into a database of your own
cargo run -p ingest -- import dataset

# check the guarantees it claims, the same check CI runs
python3 scripts/validate_data.py dataset
```

- **Nothing is keyed by a database id.** Rows reference each other by slug, and a source by its `url`
  and `content_hash` together, so a rebuilt database does not read as changed data.
- **Deterministic.** Sorted rows, sorted keys, a total ordering in every file. A day with no editorial
  change produces an empty diff, so the git history is a record of what actually changed and when.
- **Provenance travels with each row.** Every fact carries its source; each cited document records
  when it was fetched, the SHA-256 of the bytes where we downloaded them, and an archived copy where
  one exists. Recorded positions also carry the page, article or roll call the quote comes from.
- **It round-trips.** Importing the dataset into an empty database and exporting it again reproduces
  the files byte for byte, which is checked in CI against the real files rather than a fixture.
- **Only approved content is in it.** No accounts, no votes, no drafts, no unpublished translations,
  and no news: a news summary is our own prose about what someone is accused of, and it decays as the
  story moves on.

Participation data is separate and lives at `GET /data/polls.json` on a running instance: every poll's
tally, its vote-chain head, and every vote reduced to `(poll, option, cast_at, opaque voter index)`.
Anyone can recompute the tallies and check the chain head against the poll page. It carries no
identity, never a user id and never an email hash.

## Versioning

Releases follow [Semantic Versioning](https://semver.org). The project is pre-1.0, which under SemVer
means what it says: this is initial development, and anything may still change.

While on `0.x`:

- **`0.x.0` (minor)** for user-visible features, and for any release carrying a database migration.
  Migrations are append-only and are applied to production deliberately, so a release that changes
  the schema is always worth signalling.
- **`0.x.y` (patch)** for fixes, copy and translation updates: no migration, no new surface.

`1.0.0` is reserved for public launch: the construction notice off, and a documented, stable schema
for the public data dumps. Those dumps are the closest thing this project has to a public API, so
they are the compatibility surface a major version should track once anyone depends on them.

Pushing a `v*` tag builds and attests the image and publishes a [GitHub Release](https://github.com/markedown/open-public/releases)
recording the attested digest, so every version is traceable to exactly what was deployed.

## Quickstart

Prerequisites: a stable Rust toolchain (see [`rust-toolchain.toml`](./rust-toolchain.toml)),
[`sqlx-cli`](https://crates.io/crates/sqlx-cli) (`--no-default-features --features rustls,postgres`),
the [standalone Tailwind CLI](https://github.com/tailwindlabs/tailwindcss/releases), and Docker.

```bash
# 1. Start PostgreSQL
docker compose up -d

# 2. Configure the environment
cp .env.example .env          # then edit if needed

# 3. Create the database and run migrations
sqlx database create
sqlx migrate run

# 4. Build the CSS (add --watch during development)
./scripts/tailwind.sh

# 5. Load the published dataset (optional; the server also runs against an empty database)
cargo run -p ingest -- import dataset

# 6. Run the server
cargo run -p server           # http://127.0.0.1:3000
```

## Development

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo sqlx prepare --workspace   # regenerate .sqlx/ after changing any query
python3 scripts/validate_data.py dataset   # check the published data
```

Please read [`CONTRIBUTING.md`](./.github/CONTRIBUTING.md) before opening a pull request. The content
rules there (every fact needs a source, neutral wording, append-only migrations) are enforced in
review.

## Branding and fonts

The identity is intentionally plain, and temporary. Fonts are self-hosted, with no web-font CDN, for
privacy:

- **Interface:** [Public Sans](https://public-sans.digital.gov) (SIL OFL), a self-hosted variable
  `woff2` in `crates/server/static/fonts/`, with its license committed next to it. It covers the
  extended-Latin diacritics the locale-aware UI needs.
- **Wordmark:** [Spectral](https://github.com/productiontype/Spectral) (SIL OFL), converted to SVG
  paths in `crates/server/static/brand/wordmark.svg` (with a dark variant) so it renders the same
  without the font.
- **Accent:** a desaturated ink-blue (`#33527a`), used sparingly for links, focus, and the wordmark
  hyphen. Party colors appear only inside data elements, never as interface chrome.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](./LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](./LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

This license covers the source code only. The database contents (the political dataset and poll
results) are dedicated to the public domain under [CC0 1.0 Universal](./LICENSE-DATA), so anyone can
copy, adapt, redistribute, and independently verify them without restriction. CC0 matches the main
upstream source (Wikidata, itself CC0) and keeps the published data dumps frictionless to reuse.

One limit, stated plainly: **quoted excerpts are not ours to relicense.** Where a row quotes a party
programme, a law or a parliamentary record, those words belong to whoever wrote them and appear as
citations. CC0 covers the compilation, our own prose and the structure, not the quotations inside it.

### Contribution

Unless you state otherwise, any contribution you submit for inclusion in this work, as defined in the
Apache-2.0 license, is dual licensed as above, with no additional terms or conditions.
