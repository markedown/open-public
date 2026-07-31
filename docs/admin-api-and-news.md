# Admin ingest API and the news pipeline: design

Status: design for review. No code yet.

This describes two linked pieces of work. The first is an **admin-only write API**:
the durable way editorial data enters a running instance, replacing hand-applied
SQL and direct database access. The second is a **news pipeline** that runs
privately, reads the world, and delivers what it finds through that API so every
person and party page carries current, sourced coverage and readers can search
it.

Two layers, kept apart, the same split the project already draws between what we
publish and how we gather it:

- **Public mechanism** (this repository): the API, the schema, the related-coverage
  model, and the display. Country-agnostic, no data.
- **Private pipeline** (local, on our own machine): the fetchers, the language
  model calls, the embeddings, the source lists. Never committed, the same as the
  existing harvesters.

## 1. What this delivers, and what it does not

Delivers:

- A single, authenticated, admin-only path for editorial content to reach an
  instance: news, people, parties, roles, memberships, statements, sources,
  compass evidence, translations.
- Current news across every country we cover, each item entity-matched to the
  people and parties it names, given a short neutral summary, and sourced.
- Related coverage: for a story, the other outlets that covered the same thing,
  so a reader sees the spread rather than one telling.
- A one-year historical base to build on, then a job that keeps it current.

Does **not** deliver, and this is structural, not a promise:

- **Any way to write a vote or a result.** The API can create and correct
  editorial content. It has no endpoint that touches `poll_votes`, `vote_ballots`,
  `vote_entitlements`, tallies, or a chain. There is no mutation path to a cast
  ballot anywhere in the codebase, and this API adds none. Poll results move only
  through the anonymous, append-only voting path, by voters, and nothing else.
  That is the whole point of the platform, so the one thing the admin path must
  never be able to do is change an outcome.
- **News as compass evidence.** A stance is derived from documentary facts (a
  vote, a law, a manifesto), never from news commentary. News feeds discovery,
  not the compass (section 6).

## 2. The admin write API

The delivery mechanism for everything editorial. It is a small, boring, strongly
typed HTTP API, not a generic database proxy.

### 2.1 Authentication

A single administrative bearer credential, provided to the instance as a secret
(`ADMIN_API_KEY`) and sent as `Authorization: Bearer <key>`. Requests without it
get a plain 404 (the same treatment as the admin pages: the surface does not
announce itself). The key is compared in constant time. It is distinct from a
user session; the API is for machine-to-machine ingest, not the browser.

Later this can grow to per-key scopes or short-lived tokens; the first cut is one
key with full editorial write access, because there is exactly one caller (our
pipeline) and one operator.

### 2.2 Shape

Resource-oriented, JSON in and out, one endpoint per entity kind under
`/api/v1/…`:

```
POST /api/v1/sources           upsert a source (by url + content_hash)
POST /api/v1/people            upsert a person (by slug)
POST /api/v1/parties           upsert a party (by slug)
POST /api/v1/news              upsert a news item (by url) + its entity links
POST /api/v1/statements        upsert a statement
POST /api/v1/evidence          upsert compass evidence (documentary only)
POST /api/v1/translations      upsert a translation (draft)
```

Every write is **idempotent on a natural key** (the same keys the public dataset
uses: slugs, and a source's `url` + `content_hash`), so the pipeline can re-run
without duplicating, and a retry after a network failure is safe. Every writable
fact requires a `source` (by natural key) exactly as the schema does; the API
refuses a fact with no source, the same rule enforced everywhere else.

Writes that create editorial prose (a news summary, a bio) land as a **draft**
where the schema already has that notion (`summary_draft`, translation
`status='draft'`), so a human still approves before anything is shown. The API
speeds ingest; it does not bypass review.

### 2.3 Guarantees, as tests

- A request to any endpoint without the key is a 404, always.
- No endpoint accepts, references, or can reach a votes table. This is asserted by
  a test that greps the API module for the vote tables and fails if they appear,
  and by there being no such handler.
- Every create-a-fact endpoint rejects a payload with no resolvable source.
- Re-posting the same payload is a no-op, not a duplicate.

## 3. News schema

The existing `news_items`, `outlets`, and the entity-link tables carry the base.
This adds what the pipeline needs.

- **Embeddings for related coverage.** A migration adds the `pgvector` extension
  and an `embedding vector(1024)` column on `news_items` (the same column the
  phase-3 plan always intended, arriving now because related coverage needs it).
  A vector index (HNSW or IVFFlat) supports nearest-neighbour search.
- **Story clusters.** A `news_related` table (or a nullable `story_id`) links
  items the pipeline judged to be the same story, so a page can show "also
  reported by" without recomputing. The link is symmetric and carries a method
  tag (how it was matched) for auditability.
- **Provenance of the fetch.** `news_items` already stores url, outlet, date and
  our summary; the source row for each item records when it was fetched and the
  hash of what we read, the same discipline as every other source.

No article body is ever stored. We keep the headline, the url, the outlet, the
date, and our own short summary, and nothing of the original text, the rule the
project already holds.

## 4. Related coverage

The "other news that mentioned this" is same-story clustering across outlets, and
it is the neutrality payoff: a reader sees how the spread of outlets covered one
event.

- **Embed locally.** Each item's title and summary are embedded by a local
  multilingual model (no per-item API cost, and it handles all our languages),
  stored in the `embedding` column.
- **Cluster by nearness in time and meaning.** Candidate matches are the nearest
  neighbours within a time window that also share at least one tracked entity. A
  language-model check resolves only the borderline pairs, so cost stays low.
- **Show the spread, not a verdict.** A story shows its members with their outlets
  and each outlet's recorded leaning (monochrome, as the outlet bars already are).
  We do not label a story or a take; we show who reported it.

## 5. The private pipeline (described, not shipped here)

The fetchers, model calls and embeddings live on our own machine and are never
committed, the same as the existing harvesters. In outline:

- **Historical base from GDELT.** The GDELT article index (free, global, every
  language we need, more than a year of history) is queried for articles that name
  our tracked entities in each country, giving urls, titles and times. Each is
  fetched once, summarized, entity-matched and delivered through the API. This is
  how we get a year of base without scraping every outlet's archive by hand.
- **Ongoing from RSS and light scraping.** The spectrum of outlets per country
  (the neutrality strategy) is polled on a schedule for new items, the same
  high-precision matching as today (a person by full name, a party by its full
  name or a three-or-more-character abbreviation as a whole word).
- **Summaries and analysis by the language model**, producing a neutral
  short summary and nothing that reads as an opinion, the same constraint as our
  own prose everywhere.
- **Always on**, a scheduled job with per-host rate limits, `robots.txt`
  respected, a descriptive contact User-Agent, and no paywalled content, the
  ingest rules the project already documents.

Only items that name a **tracked public figure or party** are ever stored. That is
both the relevance filter and the guardrail: a private individual who is not a
tracked public figure never enters the system.

## 6. News and the compass

News powers the person and party pages and search. It does not become a compass
stance, because a stance is derived only from documentary facts and news
commentary is not one.

Where an article reports a documentary act (a bill tabled, a vote, a decree),
the pipeline may **surface that act as a candidate** for a human to review and
cite from its primary source. The compass then rests on the document, as it does
now; the news was only the pointer that found it. So news broadens what we notice
without lowering the evidence bar.

## 7. Privacy and what stays private

- News is **not in the public dataset** and never will be: a news summary is our
  prose about what a named person is reported to have done, and it decays. This is
  already decided and enforced by the export validator.
- The app **serves** news, and when the platform is public those pages name
  politicians, which is inherent to a political platform: they are public figures,
  discussed from sourced, dated coverage with a link to the outlet. The guardrail
  above (tracked entities only) keeps private individuals out.
- The pipeline, the model, the embeddings and the source lists are private and
  local; only the API, schema and display are public.

## 8. Rollout (epic breakdown)

Each is its own PR (public) or private change, with tests where there is code.

1. **Admin API, core.** Auth, the source/person/party/news endpoints, idempotency,
   the source requirement, and the no-votes guarantee with its tests. Public.
2. **News schema.** pgvector + the embedding column + the related table, and the
   fetch-provenance fields. Public.
3. **Related coverage display.** Nearest-neighbour query, the "also reported by"
   block on a news item, per-entity news on person and party pages. Public.
4. **Pipeline: ongoing.** RSS + scraping per country, matching, summary, delivery
   through the API, for new items. Private.
5. **Pipeline: backfill.** GDELT historical pull for the last year, delivered
   through the same API. Private.
6. **Pipeline: embeddings + clustering.** Local embedding of every item, the
   clustering job, the borderline model check. Private.
7. **Always on.** The scheduled job and its guardrails. Private.
8. **Data-quality pass.** With real coverage flowing, audit the existing data for
   thin or stale entities and fill the gaps. Follow-on.

The remaining endpoints (statements, evidence, translations) are added to the API
as the pipeline needs them, not before.

## 9. Resolved decisions

- **Backfill:** GDELT as the historical backbone, RSS and light scraping for
  ongoing. Free and realistic across all five countries.
- **Related coverage:** local multilingual embeddings in pgvector, clustered by
  time and meaning, a language-model check only for borderline pairs.
- **Sequencing:** the admin API first, then the pipeline delivers through it.
- **News and the compass:** discovery only. News surfaces candidate documentary
  sources for review; the compass still rests only on documents.
- **The hard line:** the admin API can write editorial content and can never write
  a vote or a result, by construction and by test.
