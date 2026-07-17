CLAUDE.md

open-public is an open platform for structured political data: people, parties, roles, statements, news and polls. Two rules shape the whole codebase:

1. Every fact in the database references a source row. No source, no insert. This includes seed data.
2. Anything that changes over time (party membership, roles, positions) is stored as a time-ranged relation with start and end dates, never as a flat column.

The core promise is trustworthy participation data. There are two tiers of data with different guarantees:

- **Political content** (people, parties, roles, memberships, statements, news) is curated editorial data with provenance. It is correctable: a `sources` row of kind `manual` records an admin edit, and sources are provenance, not locks.
- **Participation data** (poll votes) is append-only and immutable: never updated or deleted, by anyone.

The platform is country-agnostic. The first datasets carry non-ASCII, locale-sensitive names, so text handling (casing, transliteration, sort order) is locale-aware from the start (see the Text handling section).

## Stack

- Rust, stable toolchain, pinned in `rust-toolchain.toml`. No nightly features.
- Axum is the web server. HTML is server-rendered with `maud` templates (type-checked Rust; templates are plain functions). No client-side framework, no WASM, no build step for JS.
- Interactivity via HTMX 2.x, vendored into the server's static assets (never a CDN). Every page is complete, valid HTML without JavaScript; HTMX only enhances (e.g. poll voting, search-as-you-type), and every enhanced interaction has a working non-JS fallback via a normal form POST.
- Styling with Tailwind CSS built by the standalone Tailwind CLI, wired into the dev workflow and CI. No CSS component library.
- Reusable UI is our own `maud` component functions in a `ui` module (layout, button, card, form_field, badge, timeline_entry, poll_widget, source_link, page_meta). One definition per component; pages compose them and never restyle ad hoc.
- PostgreSQL 18. One database for relational data and full text search. No extra search engine.
- SQLx 0.9 with compile-time checked queries and `sqlx migrate`. The `.sqlx` offline data is committed so CI can build without a database.
- `reqwest` and `scraper` for ingestion, `serde` for serialization, `tracing` for logs.
- `lettre` for email (verification codes). All mail settings come from env vars (`SMTP_HOST`, `SMTP_PORT`, `SMTP_USER`, `SMTP_PASS`, `MAIL_FROM`). In dev, a console transport logs mails instead of sending, so local development needs no mail server.
- `thiserror` in library crates, `anyhow` in binaries. No `unwrap()` or `expect()` in request paths.
- Local infra via `docker-compose.yml` (postgres).
- Track the latest stable release of every tool and dependency: latest stable Rust and PostgreSQL, latest stable patch of each crate.

## Repo layout

```
open-public/
├── Cargo.toml            workspace
├── rust-toolchain.toml
├── docker-compose.yml
├── .env.example          DATABASE_URL etc. Never commit .env
├── migrations/           sqlx migrations
├── crates/
│   ├── domain/           shared types, no sqlx, dependency-light
│   ├── db/               sqlx queries and repository functions
│   ├── server/           Axum binary: handlers, maud templates, ui components, static assets
│   └── ingest/           data import binaries
└── CLAUDE.md
```

A gitignored `CLAUDE.local.md` next to this file holds private context and future plans, read alongside this file. When something from there gets implemented, its documentation moves into this file.

Rules that follow from this:

- `domain` defines the vocabulary of the project and stays dependency-light.
- Handlers call into `db`. There is no inline SQL in handlers or templates.
- Vendored assets (`htmx.min.js`, self-hosted font if any) are committed under the server crate's static dir. The Tailwind output CSS is generated, not committed.
- Ingest binaries are idempotent. Running them twice never duplicates rows (upsert on external IDs like `wikidata_id`).

## Database schema (v1)

Implemented as sqlx migrations. Table and column names below are canonical.

```sql
sources (
  id bigserial PK,
  kind text CHECK (kind IN ('wikidata','official_gov','official_election','party_site','news_rss','manual')),
  url text NOT NULL,
  title text,
  outlet text,
  fetched_at timestamptz NOT NULL,
  published_at timestamptz,
  content_hash text,
  raw_ref text,
  UNIQUE (url, content_hash)
)

people (
  id bigserial PK,
  wikidata_id text UNIQUE,
  full_name text NOT NULL,
  slug text UNIQUE NOT NULL,
  birth_date date,
  birth_place text,
  photo_url text,        -- only freely licensed images
  photo_license text,
  summary text,          -- neutral 2-3 sentence bio, our own words
  source_id bigint REFERENCES sources NOT NULL,
  created_at timestamptz, updated_at timestamptz
)

parties (
  id, wikidata_id UNIQUE, name, short_name, slug UNIQUE,
  founded_date, dissolved_date,
  ideology_tags text[],
  summary text, source_id NOT NULL, timestamps
)

party_memberships (
  id, person_id FK, party_id FK,
  start_date date, end_date date,   -- NULL end means current
  source_id FK NOT NULL,
  UNIQUE (person_id, party_id, start_date)
)

roles (
  id, person_id FK,
  role_type text,        -- 'mp','minister','mayor','party_leader','president',...
  title text,            -- display string
  org text,
  district text,
  start_date, end_date, source_id NOT NULL,
  UNIQUE (person_id, role_type, org, start_date)
)

statements (
  id, person_id FK NULL, party_id FK NULL,   -- exactly one of the two is set
  topic_id FK NULL,
  text_original text NOT NULL,    -- short excerpt or paraphrase, keep brief
  is_paraphrase boolean NOT NULL,
  stated_at date,
  source_id FK NOT NULL
)

topics ( id, name, slug UNIQUE, parent_id FK NULL )

news_items (
  id, source_id FK NOT NULL UNIQUE,
  headline text NOT NULL,
  our_summary text        -- max ~50 words, our own words
)
news_item_people ( news_item_id FK, person_id FK, PK(both) )
news_item_parties ( news_item_id FK, party_id FK, PK(both) )

polls (
  id, question text NOT NULL, slug UNIQUE,
  person_id FK NULL, party_id FK NULL, topic_id FK NULL,
  opens_at timestamptz, closes_at timestamptz,
  created_by text DEFAULT 'admin'
)
poll_options ( id, poll_id FK, label text, position int )

users (
  id bigserial PK,
  email_hash text UNIQUE NOT NULL,   -- HMAC-SHA256 of the normalized
                                     -- email with a server-side key.
                                     -- Plaintext emails are never stored.
  password_hash text NOT NULL,       -- argon2id hash
  is_admin boolean NOT NULL DEFAULT false,
  verified_at timestamptz,           -- set after email confirmation
  created_at timestamptz NOT NULL DEFAULT now()
)

sessions (
  id, user_id FK, token_hash text UNIQUE NOT NULL,
  expires_at timestamptz NOT NULL, created_at timestamptz
)

email_verifications (
  id, email_hash text NOT NULL,
  user_id bigint REFERENCES users,
  code text NOT NULL,                -- short-lived one-time code
  expires_at timestamptz NOT NULL,
  consumed_at timestamptz
)

poll_votes (
  id, poll_id FK, option_id FK,
  user_id bigint REFERENCES users NOT NULL,
  cast_at timestamptz,
  UNIQUE (poll_id, user_id)
)

person_aliases ( person_id FK, alias text )
data_conflicts ( ... )   -- ingest writes discrepancies here instead of overwriting
```

Full text search: `tsvector` indexes on `people.full_name`, `parties.name`, `statements.text_original`, `news_items.headline`.

Migrations are append-only. Never edit a migration that has been applied.

## Content rules

These are product rules, treat them like compiler errors:

- Every fact row carries a `source_id`.
- News items store headline, URL, outlet, date and a short summary in our own words. Full article text is never stored or displayed.
- Our own text (summaries, UI copy) stays neutral and descriptive. No editorial adjectives about people or parties. This includes commit messages, comments and test fixtures. Fixtures use invented people ("Ayşe Yılmaz, Test Partisi"), never real politicians.
- Polls are participatory, not statistical: email verification de-duplicates voters but does not sample a population, so poll results are never presented as a representative survey. No feature may claim or imply representativeness until verified participation exists (see the roadmap). Every poll is treated this way; there is no formal/informal flag.
- Users register with email and password. Registration sends a one-time verification link; login is refused until the account is verified. Only the salted HMAC hash of the email is persisted, never the plaintext address. Registration, verification, and login are rate limited at the edge (reverse proxy), not in the application.
- Verification mails are plain text, contain only the verification link and a one-line explanation, and never any tracking.
- `poll_votes` rows are never updated or deleted, not even by admins. Corrections to a poll happen by closing it and opening a new one, never by touching votes. No admin write path to `poll_votes` exists in the codebase.
- Admins (users with `is_admin = true`) can create and edit political content (people, parties, roles, memberships, statements, news, polls), but cannot modify votes.
- Ingest: rate limit at least 1s per host, descriptive User-Agent with contact info, respect robots.txt, no paywalled content. On conflicts between sources, log to `data_conflicts` instead of overwriting.

## Design

The design system lives in [`DESIGN.md`](./DESIGN.md); follow it when building UI. The load-bearing rule: color is data. The chrome stays near-monochrome, organization colors appear only inside data elements (such as party badges), and no interface accent may map to a political party. The only accent is a desaturated ink-blue (`#33527a`), used sparingly for links and focus states.

## Text handling

User-facing text is locale-sensitive and often non-ASCII. Two things break silently if ignored:

- Casing: some locales have casing rules that Unicode-default lowercasing gets wrong. Never use naive `to_lowercase()` for user-facing casing or matching; use the locale-aware helpers in `domain`.
- Slugs: a transliteration helper in `domain` maps the active locale's diacritics to ASCII, then lowercases and hyphenates. The helper has unit tests covering every special character of the active locale.
- Use an ICU collation where sort order is user-visible.
- UI strings live in one module so i18n can be added later.

## Dev setup

```bash
docker compose up -d
cp .env.example .env
sqlx database create && sqlx migrate run

# Build the CSS once, or add --watch in a second terminal during development.
./scripts/tailwind.sh            # wraps the standalone Tailwind CLI

cargo run -p server              # http://127.0.0.1:3000
cargo run -p ingest -- <task>
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo sqlx prepare --workspace   # before committing query changes
```

## Conventions

- Conventional commits (feat:, fix:, chore:, docs:), atomic commits per change.
- One page per handler under `server/src/pages/`; shared UI pieces are `maud` component functions under `server/src/ui/`. Pages compose components.
- Handlers are thin: validate input, call a `db` function, render a template, map errors. No inline SQL in handlers or templates.
- Every page is complete, valid HTML without JavaScript. HTMX only enhances, and every HTMX interaction has a non-JS fallback via a normal form POST.
- Log with `tracing`, never `println!`.
- db layer gets integration tests via `#[sqlx::test]`. Pure helpers get unit tests.

## CI/CD

This repo is public. Git history is permanent, so a leaked secret means rotating the credential, not deleting the file. Only `.env.example` is committed. Deployment specifics (servers, DNS, hosting config) do not belong in this repo; the deployment interface is the Docker image plus documented env vars.

`.github/workflows/ci.yml`, on every push and PR to main:

1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace` against a postgres service container, migrations applied first. `SQLX_OFFLINE=true` for the build step.
4. Build the Tailwind CSS with the standalone CLI, then `cargo build --release --workspace`.
5. Cargo caching (rust-cache), otherwise CI is painfully slow.

`.github/workflows/release.yml`: on `v*` tags, build the multi-stage Docker image, push it to GHCR, and generate a build-provenance attestation (`actions/attest-build-provenance`) that binds the source commit and the public workflow to the resulting image digest. The digest is written to the workflow summary.

Workflow security rules:
- Workflows use the pull_request trigger for PRs, never pull_request_target.
- Every workflow declares least-privilege permissions explicitly (permissions: contents: read as default, write scopes only where needed, e.g. packages write plus id-token and attestations write in release.yml).
- Every third-party action is pinned to a full commit SHA, never a mutable tag, with the human-readable tag kept in a trailing comment.
- Secrets are never available to fork PRs. Nothing in CI may depend on a secret being present for the test job.

Verifiable builds: the trust chain is source commit -> public build workflow -> attested image digest -> the `/version` endpoint reporting that digest. `commit` and `built_at` are injected at build time and never read from `.git` at runtime; `image_digest` is supplied at run time by the deployment, which pulls images by digest and never by mutable tags. Anyone can verify the running version against the public build: the attestation proves which commit and which public workflow produced the digest, and `/version` shows the digest actually running.

Main is protected: merges only via PR, CI must pass before merge, no force pushes. Migrations changes and workflow changes require owner review (see CODEOWNERS).

License: MIT OR Apache-2.0 (dual). Set `license = "MIT OR Apache-2.0"` in every `Cargo.toml`.

## Do not

- No social login. Identity is a verified email+password account; plaintext emails are never stored.
- No plaintext email addresses in the database or in logs.
- No code path that mutates or deletes cast votes.
- No user free-text content.
- No full news article bodies in the database, ever.
- No client-side framework and no WASM. Interactivity is HTMX over server-rendered HTML.
- No CDN dependencies: htmx and any font are vendored/self-hosted.
- No CSS component libraries, no additional JS libraries beyond htmx, no web fonts beyond the chosen one, without discussion.
- No microservices, no Kubernetes, no queues. One binary and Postgres until it hurts.
- No secrets or infrastructure details in any commit.
- No mutable image tags in deployment artifacts: images are referenced by digest only.
- No claims of verifiability in the UI or docs beyond what is actually implemented.
