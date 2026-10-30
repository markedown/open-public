-- Recorded votes in a chamber, and how each member voted.
--
-- A roll call is an official record: the chamber publishes who voted which way,
-- so the row cites that publication and the choices are read from it. "absent"
-- is a choice of the record, not a judgement: a chamber's list says a member did
-- not vote, not why.
--
-- A member's party is deliberately not stored here. It is read from
-- party_memberships as of the day of the vote, so a later party change neither
-- rewrites the past nor needs a second copy of a fact we already hold.
--
-- Migrations are append-only; never edit this file once it has been applied.

create table roll_calls (
    id bigserial primary key,
    country_id bigint not null references countries (id),
    slug text not null unique,
    chamber text not null,
    held_on date not null,
    title text not null,
    source_id bigint not null references sources (id),
    created_at timestamptz not null default now()
);

create index roll_calls_country_held_idx on roll_calls (country_id, held_on desc);

create table roll_call_votes (
    roll_call_id bigint not null references roll_calls (id) on delete cascade,
    person_id bigint not null references people (id),
    choice text not null check (choice in ('yes', 'no', 'abstain', 'absent')),
    primary key (roll_call_id, person_id)
);

create index roll_call_votes_person_idx on roll_call_votes (person_id);
