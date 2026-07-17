-- Countries, party alliances, and a party display colour.
--
-- Country-agnostic structure only. No country's data lives here: the tables
-- are populated by a deployment's own dataset. The reference seed and any
-- national dataset ship separately.
--
-- Rationale for each piece:
--   * parties.color   a party's own brand colour, shown only inside data
--                     elements (badges, seat bars), never as interface chrome.
--   * alliances       electoral or parliamentary coalitions of parties. A
--                     party's membership is time-ranged, like every other
--                     changing relation, and carries a source.
--   * countries       the top-level entity a dataset describes: capital, form
--                     of government, founding date, population, and a neutral
--                     summary in our own words. Sourced like every fact.
--
-- Migrations are append-only; never edit this file once applied.

-- 1. Party display colour --------------------------------------------------------

alter table parties
    add column color text;

-- 2. Alliances (coalitions) ------------------------------------------------------

create table alliances (
    id        bigserial primary key,
    name      text not null,
    slug      text not null unique,
    summary   text,
    source_id bigint not null references sources(id)
);

create table party_alliances (
    id          bigserial primary key,
    party_id    bigint not null references parties(id),
    alliance_id bigint not null references alliances(id),
    start_date  date,
    end_date    date,
    source_id   bigint not null references sources(id),
    unique nulls not distinct (party_id, alliance_id, start_date)
);

create index party_alliances_party_id_idx    on party_alliances(party_id);
create index party_alliances_alliance_id_idx on party_alliances(alliance_id);

-- 3. Countries -------------------------------------------------------------------

create table countries (
    id              bigserial primary key,
    name            text not null,
    slug            text not null unique,
    capital         text,
    government_type text,
    founded_date    date,
    population      bigint,
    summary         text,
    source_id       bigint not null references sources(id)
);
