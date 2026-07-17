-- Translations of content into other languages.
--
-- The gettext catalogs translate the fixed UI strings, but content (a person's
-- summary, a news headline, a poll question) lives in the database in the
-- language it was authored in. This table holds translations of that content,
-- keyed by (entity, field, language), so a reader sees the platform in their
-- own language while the sourced original stays canonical.
--
-- Rules the application enforces:
--   * The base-table value stays the canonical original; a translation never
--     overwrites it. Readers see a translation only when it is `published`, and
--     fall back to the original otherwise.
--   * A translation carries provenance like every other fact: whether a human or
--     a machine produced it, the language it came from, and who reviewed it. A
--     machine translation lands as a `draft` and is shown only after review.
--
-- entity_id is polymorphic (it names a row in one of several content tables), so
-- it carries no foreign key; the application validates entity_type against a
-- fixed registry of translatable (entity, field) pairs.
--
-- Migrations are append-only; never edit this file once it has been applied.

create table translations (
    id bigserial primary key,
    entity_type text not null,
    entity_id bigint not null,
    field text not null,
    lang text not null,
    text text not null,
    origin text not null check (origin in ('human', 'machine')),
    status text not null default 'draft' check (status in ('draft', 'published')),
    source_lang text,
    translated_at timestamptz not null default now(),
    reviewed_by bigint references users (id),
    reviewed_at timestamptz,
    unique (entity_type, entity_id, field, lang)
);

-- Batch-load every published translation an entity needs on a page, in one
-- language.
create index translations_entity_idx on translations (entity_type, entity_id, lang, status);

-- The admin review queue lists drafts across all entities.
create index translations_status_idx on translations (status);
