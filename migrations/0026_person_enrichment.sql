-- Biographical enrichment for person pages: education and generic attributes.
--
-- Person pages carry roles, memberships and statements but little biography.
-- These two tables add sourced, correctable biographical facts, following the
-- platform's rules: every row references a source, and anything that changes
-- over time is time-ranged.
--
-- Two shapes for two kinds of data. Education is a structured, time-ranged
-- relation to an institution (institution, degree, field, dates). Occupation,
-- ideology and religion are flat sourced tags, so they share one generic table
-- keyed by `kind`, and a new kind of attribute needs no migration. Positions
-- held reuse the existing `roles` table, so there is no table for them here.
--
-- Migrations are append-only; never edit this file once it has been applied.

create table person_education (
    id bigserial primary key,
    person_id bigint not null references people (id),
    institution text not null,
    institution_wikidata_id text,
    -- The academic degree earned (P512), e.g. a bachelor of laws.
    degree text,
    -- The field of study, when known.
    field text,
    start_date date,
    end_date date,
    source_id bigint not null references sources (id),
    -- `nulls not distinct` so a re-run with a null degree or start date updates
    -- the same row rather than inserting a duplicate.
    unique nulls not distinct (person_id, institution, degree, start_date)
);

create index person_education_person_idx on person_education (person_id);

create table person_attributes (
    id bigserial primary key,
    person_id bigint not null references people (id),
    -- The attribute kind: 'occupation', 'ideology', 'religion', ... The
    -- application validates this against a fixed set.
    kind text not null,
    value text not null,
    value_wikidata_id text,
    -- Optional, for the rare attribute that is genuinely time-ranged.
    start_date date,
    end_date date,
    source_id bigint not null references sources (id),
    unique (person_id, kind, value)
);

create index person_attributes_person_idx on person_attributes (person_id);
