-- What ties the compass to the recorded votes, and where a visitor's members sit.
--
-- A party's vote already counts as evidence for a stance. These additions let
-- that evidence point at the vote it was read from, and say how each side of a
-- vote reads on a thesis, so one member's own votes can be compared with a
-- visitor's answers the same way a party's are.
--
-- yes_stance and no_stance are nullable on purpose. A no can mean "not far
-- enough" as easily as "no", and a vote on rejecting a motion says little about
-- the motion's subject; a side that does not say where someone stands is left
-- unread rather than guessed.
--
-- Migrations are append-only; never edit this file once it has been applied.

alter table position_evidence
    add column roll_call_id bigint references roll_calls (id) on delete set null;

create index position_evidence_roll_call_idx on position_evidence (roll_call_id);

create table thesis_roll_calls (
    thesis_id bigint not null references theses (id) on delete cascade,
    roll_call_id bigint not null references roll_calls (id) on delete cascade,
    yes_stance smallint check (yes_stance between -2 and 2),
    no_stance smallint check (no_stance between -2 and 2),
    -- Why the vote reads this way, in a sentence a reader can check.
    note text not null,
    primary key (thesis_id, roll_call_id),
    check (yes_stance is not null or no_stance is not null)
);

create index thesis_roll_calls_roll_call_idx on thesis_roll_calls (roll_call_id);

-- Places and the constituency they vote in. `district` and `region` are the same
-- text roles.district holds, so a member elected there or on that region's list
-- is found without a second copy of who sits where. A place split across
-- constituencies has one row per part.
create table constituency_places (
    id bigserial primary key,
    country_id bigint not null references countries (id),
    place text not null,
    part text,
    postal_code text,
    district text not null,
    region text,
    source_id bigint not null references sources (id),
    unique nulls not distinct (country_id, place, part, district)
);

create index constituency_places_country_place_idx on constituency_places (country_id, place);
