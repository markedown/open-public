-- Elections and their per-party results.
--
-- An election belongs to a country and has a date and kind. Each result ties a
-- party to its seats and votes in that election, sourced like every fact. This
-- underpins a party's electoral history and a country's election overview.
--
-- Migrations are append-only; never edit this file once applied.

create table elections (
    id         bigserial primary key,
    country_id bigint not null references countries(id),
    name       text not null,
    slug       text not null unique,
    held_on    date,
    kind       text,
    source_id  bigint not null references sources(id)
);

create index elections_country_id_idx on elections(country_id);

create table election_results (
    id          bigserial primary key,
    election_id bigint not null references elections(id),
    party_id    bigint not null references parties(id),
    seats       integer,
    votes       bigint,
    source_id   bigint not null references sources(id),
    unique (election_id, party_id)
);

create index election_results_election_id_idx on election_results(election_id);
create index election_results_party_id_idx    on election_results(party_id);
