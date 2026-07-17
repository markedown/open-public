-- Initial schema (v1).
--
-- Invariants encoded here:
--   * Every fact references a row in `sources` (source_id NOT NULL on fact tables).
--   * Time-varying facts (memberships, roles) are relations with start/end dates.
--   * Cast votes are immutable, enforced by foreign-key restrictions and a trigger.
-- Migrations are append-only; never edit this file once it has been applied.

-- Provenance for every fact. `kind` is country-neutral; a deployment maps the
-- official kinds to its own institutions.
create table sources (
    id           bigserial primary key,
    kind         text not null check (kind in
                     ('wikidata', 'official_gov', 'official_election', 'party_site', 'news_rss', 'manual')),
    url          text not null,
    title        text,
    outlet       text,
    fetched_at   timestamptz not null default now(),
    published_at timestamptz,
    content_hash text,
    raw_ref      text,
    unique (url, content_hash)
);

create table people (
    id            bigserial primary key,
    wikidata_id   text unique,
    full_name     text not null,
    slug          text not null unique,
    birth_date    date,
    birth_place   text,
    photo_url     text,
    photo_license text,
    summary       text,
    source_id     bigint not null references sources (id),
    created_at    timestamptz not null default now(),
    updated_at    timestamptz not null default now(),
    fts           tsvector generated always as (to_tsvector('simple', full_name)) stored
);

create table parties (
    id             bigserial primary key,
    wikidata_id    text unique,
    name           text not null,
    short_name     text,
    slug           text not null unique,
    founded_date   date,
    dissolved_date date,
    ideology_tags  text[] not null default '{}',
    summary        text,
    source_id      bigint not null references sources (id),
    created_at     timestamptz not null default now(),
    updated_at     timestamptz not null default now(),
    fts            tsvector generated always as
                       (to_tsvector('simple', name || ' ' || coalesce(short_name, ''))) stored
);

-- A person's party membership over a time range. NULL end_date means current.
create table party_memberships (
    id         bigserial primary key,
    person_id  bigint not null references people (id) on delete cascade,
    party_id   bigint not null references parties (id) on delete cascade,
    start_date date,
    end_date   date,
    source_id  bigint not null references sources (id),
    unique (person_id, party_id, start_date)
);

-- A role or position held over a time range.
create table roles (
    id         bigserial primary key,
    person_id  bigint not null references people (id) on delete cascade,
    role_type  text not null,
    title      text,
    org        text,
    district   text,
    start_date date,
    end_date   date,
    source_id  bigint not null references sources (id),
    unique (person_id, role_type, org, start_date)
);

create table topics (
    id        bigserial primary key,
    name      text not null,
    slug      text not null unique,
    parent_id bigint references topics (id)
);

create table statements (
    id            bigserial primary key,
    person_id     bigint references people (id) on delete cascade,
    party_id      bigint references parties (id) on delete cascade,
    topic_id      bigint references topics (id),
    text_original text not null,
    is_paraphrase boolean not null,
    stated_at     date,
    source_id     bigint not null references sources (id),
    fts           tsvector generated always as (to_tsvector('simple', text_original)) stored,
    -- Exactly one of person_id / party_id is set.
    check (num_nonnulls(person_id, party_id) = 1)
);

create table news_items (
    id          bigserial primary key,
    source_id   bigint not null unique references sources (id),
    headline    text not null,
    our_summary text,
    fts         tsvector generated always as (to_tsvector('simple', headline)) stored
);

create table news_item_people (
    news_item_id bigint not null references news_items (id) on delete cascade,
    person_id    bigint not null references people (id) on delete cascade,
    primary key (news_item_id, person_id)
);

create table news_item_parties (
    news_item_id bigint not null references news_items (id) on delete cascade,
    party_id     bigint not null references parties (id) on delete cascade,
    primary key (news_item_id, party_id)
);

create table polls (
    id          bigserial primary key,
    question    text not null,
    slug        text not null unique,
    person_id   bigint references people (id) on delete cascade,
    party_id    bigint references parties (id) on delete cascade,
    topic_id    bigint references topics (id),
    opens_at    timestamptz,
    closes_at   timestamptz,
    created_by  text not null default 'admin',
    is_informal boolean not null default true
);

create table poll_options (
    id       bigserial primary key,
    poll_id  bigint not null references polls (id) on delete cascade,
    label    text not null,
    position int not null
);

-- A verified voter, identified only by a salted HMAC of their email.
create table voters (
    id          bigserial primary key,
    email_hash  text not null unique,
    verified_at timestamptz not null default now()
);

create table email_verifications (
    id          bigserial primary key,
    email_hash  text not null,
    code        text not null,
    expires_at  timestamptz not null,
    consumed_at timestamptz,
    created_at  timestamptz not null default now()
);

-- Cast votes. The references to polls/options are RESTRICT (no cascade): a poll
-- or option with votes cannot be deleted, so votes are never removed as a side
-- effect. One vote per verified voter per poll.
create table poll_votes (
    id        bigserial primary key,
    poll_id   bigint not null references polls (id),
    option_id bigint not null references poll_options (id),
    voter_id  bigint not null references voters (id),
    cast_at   timestamptz not null default now(),
    unique (poll_id, voter_id)
);

-- Enforce vote immutability at the database level: no updates, no deletes.
create function reject_poll_vote_mutation() returns trigger
language plpgsql as $$
begin
    raise exception 'poll_votes rows are immutable';
end;
$$;

create trigger poll_votes_block_update
    before update on poll_votes
    for each row execute function reject_poll_vote_mutation();

create trigger poll_votes_block_delete
    before delete on poll_votes
    for each row execute function reject_poll_vote_mutation();

create table person_aliases (
    person_id bigint not null references people (id) on delete cascade,
    alias     text not null,
    primary key (person_id, alias)
);

-- Ingest writes discrepancies here instead of overwriting a trusted value.
create table data_conflicts (
    id                 bigserial primary key,
    entity_type        text not null,
    entity_id          bigint,
    field              text not null,
    existing_value     text,
    incoming_value     text,
    existing_source_id bigint references sources (id),
    incoming_source_id bigint references sources (id),
    detected_at        timestamptz not null default now(),
    resolved_at        timestamptz
);

-- Full-text search indexes.
create index people_fts_idx on people using gin (fts);
create index parties_fts_idx on parties using gin (fts);
create index statements_fts_idx on statements using gin (fts);
create index news_items_fts_idx on news_items using gin (fts);

-- Foreign-key indexes for the common lookups.
create index party_memberships_person_idx on party_memberships (person_id);
create index party_memberships_party_idx on party_memberships (party_id);
create index roles_person_idx on roles (person_id);
create index statements_person_idx on statements (person_id);
create index statements_party_idx on statements (party_id);
create index poll_options_poll_idx on poll_options (poll_id);
create index poll_votes_poll_idx on poll_votes (poll_id);
create index email_verifications_hash_idx on email_verifications (email_hash);
