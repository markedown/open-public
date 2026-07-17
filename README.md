<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="crates/server/static/brand/wordmark-dark.svg">
    <img alt="open-public" src="crates/server/static/brand/wordmark.svg" width="300">
  </picture>
</p>

<p align="center">
  <a href="https://github.com/markedown/open-public/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/markedown/open-public/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://github.com/markedown/open-public/actions/workflows/ci.yml"><img alt="Coverage: at least 92%" src="https://img.shields.io/badge/coverage-%E2%89%A592%25-brightgreen.svg"></a>
  <a href="#license"><img alt="License: MIT OR Apache-2.0" src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg"></a>
  <a href="#license"><img alt="Data: CC0-1.0" src="https://img.shields.io/badge/data-CC0--1.0-blue.svg"></a>
  <img alt="Rust: stable" src="https://img.shields.io/badge/rust-stable-orange.svg">
  <img alt="PostgreSQL 18" src="https://img.shields.io/badge/PostgreSQL-18-336791.svg">
  <img alt="No JavaScript framework" src="https://img.shields.io/badge/JS%20framework-none-informational.svg">
</p>

**open-public** is an open, source-backed record of public political life: who holds office, the
parties they belong to, how those roles and memberships shift over time, the news that mentions them,
and polls anyone can vote in. Every fact links to the source it came from, so it can be checked, and
anything that changes over time is kept as dated history instead of being overwritten. The code is
open source, the dataset is public domain, and the platform is country-agnostic.

Two rules shape the whole codebase and are enforced in review:

1. **No fact without a source.** Every record in the database references a `sources` row, seed data
   included. A source is provenance, not a lock: a correction is a new sourced edit.
2. **History, not overwrites.** Anything that varies over time (a membership, a role, a position) is a
   time-ranged relation with start and end dates, never a column that gets rewritten in place.

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
  external IDs. Importers are added per data source; none ship in this repository yet.
- **Verifiable builds:** each release publishes a build-provenance attestation binding the source
  commit to the image digest; `GET /version` reports the running commit and digest, and `GET /health`
  is a liveness probe.

The database schema lives in [`migrations/`](./migrations), the design system in
[`DESIGN.md`](./DESIGN.md), architecture and request flow in [`ARCHITECTURE.md`](./ARCHITECTURE.md),
and contribution conventions in [`CONTRIBUTING.md`](./.github/CONTRIBUTING.md).

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

# 5. Run the server (it starts against an empty database)
cargo run -p server           # http://127.0.0.1:3000
```

## Development

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo sqlx prepare --workspace   # regenerate .sqlx/ after changing any query
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

### Contribution

Unless you state otherwise, any contribution you submit for inclusion in this work, as defined in the
Apache-2.0 license, is dual licensed as above, with no additional terms or conditions.
