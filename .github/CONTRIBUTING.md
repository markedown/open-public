# Contributing to open-public

Thanks for helping build an open, source-backed record of political data. This project lives or dies
by the trustworthiness of its data, so the rules below are not bureaucracy; they are what makes the
data usable. Please read them before opening a pull request.

By contributing you agree that your contributions are dual licensed under
[MIT](../LICENSE-MIT) OR [Apache-2.0](../LICENSE-APACHE), and you agree to abide by our
[Code of Conduct](./CODE_OF_CONDUCT.md).

## Local setup

Prerequisites: a stable Rust toolchain (pinned in [`rust-toolchain.toml`](../rust-toolchain.toml)),
`sqlx-cli`, the standalone Tailwind CLI, and Docker.

```bash
# install the build tools (one-time)
cargo install sqlx-cli --no-default-features --features rustls,postgres
# Tailwind: download the standalone CLI for your platform from
# https://github.com/tailwindlabs/tailwindcss/releases

# enable the project git hooks (fmt + secret guard on commit, lint on push)
git config core.hooksPath .githooks

# bring up the stack
docker compose up -d
cp .env.example .env
sqlx database create && sqlx migrate run
./scripts/tailwind.sh         # build the CSS (add --watch while developing)
cargo run -p server           # http://127.0.0.1:3000
```

Run the full local check suite before pushing. These are exactly what CI runs:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./scripts/tailwind.sh && cargo build --release --workspace
```

If you changed any SQL query, regenerate the committed offline metadata:

```bash
cargo sqlx prepare --workspace
```

## Tests and coverage

Changes ship with tests. Pure logic gets unit tests next to the code; HTTP
behaviour gets end-to-end tests in `crates/server/tests/`, which drive the real
Axum router with a fresh database per test (`#[sqlx::test]`) and a console
mailer. The `db` layer is covered by `#[sqlx::test]` integration tests.

CI enforces a line-coverage floor (binary entrypoints in `main.rs` are excluded,
since they only bind a socket and serve). Measure it locally with:

```bash
cargo install cargo-llvm-cov      # one-time
cargo llvm-cov --workspace --ignore-filename-regex 'main\.rs' --summary-only
```

## Content rules (treat these like compiler errors)

These are product rules. A PR that violates one will not be merged.

- **Every fact carries a source.** Every row that asserts a fact (a person, a role, a party
  membership, a statement, a news item) references a row in `sources`. No source, no insert. This
  includes seed data and test fixtures.
- **Time-ranged, not flat.** Anything that changes over time (party membership, roles, positions) is
  stored as a relation with `start_date`/`end_date`, never as a mutable column.
- **Neutral wording.** Our own text (summaries, UI copy, commit messages, comments, fixtures) stays
  descriptive and neutral. No editorial adjectives about people or parties.
- **Invented people in fixtures.** Test fixtures use invented names such as *Ayşe Yılmaz, Test
  Partisi*, never real politicians.
- **No full article text, ever.** News items store headline, URL, outlet, date and a short summary in
  our own words. Full article bodies are never stored or displayed.
- **Polls are informal.** Poll results carry the "informal, not a representative survey" label as
  long as `is_informal` is true. Do not add features that imply representativeness.
- **Respectful ingestion.** Rate-limit at least 1s per host, send a descriptive User-Agent with
  contact info, respect `robots.txt`, and never fetch paywalled content. On conflicts between
  sources, log to `data_conflicts` instead of overwriting.

Source trust order when data conflicts: official government and election-authority sources over
Wikidata over party sites over news.

## Code conventions

- **Conventional commits** (`feat:`, `fix:`, `chore:`, `docs:`, …), atomic commits per change.
- `domain` stays dependency-light (no SQLx). Handlers and templates never contain inline SQL. They
  call into the `db` crate.
- Handlers are thin: validate input, call a `db` function, render a template, map errors.
- No `unwrap()`/`expect()` in request paths. Log with `tracing`, never `println!`.
- Migrations are **append-only**: never edit a migration that has already been applied.
- Every page is complete, valid HTML without JavaScript; HTMX only enhances, and every HTMX
  interaction has a plain-form (non-JS) fallback.
- Text handling: use the locale-aware slug and casing helpers in `domain`; never rely on naive
  `to_lowercase()` for non-ASCII, locale-sensitive text.

## Pull requests

1. Fork and branch from `main`.
2. Keep the change focused; separate unrelated changes into separate PRs.
3. Make sure the local check suite above is green.
4. Fill in the PR template checklist, especially: every new fact has a `source_id`, no secrets, and
   migrations are append-only.
5. CI must pass before a maintainer merges. `main` is protected; no force pushes.

## Reporting incorrect data

If a fact about a person or party is wrong, please open a **data correction** issue (there is a
template) with a better source. Everything here is sourced, so a correction is a source question, not
an opinion question.

## Security

Please report security issues privately. See [`SECURITY.md`](./SECURITY.md). Do not open a public
issue for a vulnerability.
