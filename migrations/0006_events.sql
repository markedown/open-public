-- Political events: a sourced timeline at country, party, or person level.
--
-- Foundings, elections, changes of government and comparable milestones. Each
-- event is scoped to at least one entity and, like every fact in the schema,
-- carries a source. This backs the timeline sections on the country and party
-- pages. Anything already modelled as a time-ranged relation (memberships,
-- roles) or as its own table (elections) is not duplicated here; events cover
-- point-in-time milestones that have no other home.
--
-- Migrations are append-only; never edit this file once applied.

create table events (
    id          bigserial primary key,
    country_id  bigint references countries(id),
    party_id    bigint references parties(id),
    person_id   bigint references people(id),
    kind        text not null,
    title       text not null,
    happened_on date,
    source_id   bigint not null references sources(id),
    constraint events_scope_ck check (
        (country_id is not null)::int
      + (party_id  is not null)::int
      + (person_id is not null)::int >= 1
    )
);

create index events_country_id_idx on events(country_id);
create index events_party_id_idx   on events(party_id);
create index events_person_id_idx  on events(person_id);
create index events_happened_on_idx on events(happened_on desc nulls last);
