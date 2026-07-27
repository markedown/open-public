-- Monthly approval on an entity: a person, a party, or a coalition.
--
-- An approval is not a new vote system. It is a poll attached to an entity for
-- one calendar month, so it rides the machinery that already makes a vote
-- trustworthy: the append-only hash chain, the one-vote-per-poll trigger, the
-- verified-voter gate, and the published, recomputable participation dump. A
-- fresh poll each month lets approval move over time while every vote stays
-- append-only.
--
-- Migrations are append-only; never edit this file once applied.

-- A coalition (an alliance) can now be an approval target, alongside a person
-- or a party.
alter table polls add column alliance_id bigint references alliances (id) on delete cascade;

-- The month an approval poll covers, as the first day of that month. Null for
-- every other poll kind.
alter table polls add column approval_period date;

-- Allow the 'approval' kind.
alter table polls drop constraint polls_kind_check;
alter table polls
    add constraint polls_kind_check
        check (kind in ('single', 'yesno', 'scale', 'multi', 'approval'));

-- An approval poll targets exactly one entity and covers exactly one month.
alter table polls
    add constraint polls_approval_shape check (
        kind <> 'approval' or (
            approval_period is not null
            and (person_id is not null)::int
                + (party_id is not null)::int
                + (alliance_id is not null)::int = 1
        )
    );

-- One approval poll per entity per month. Exactly one of the three ids is set,
-- so nulls-not-distinct lets the triple behave as a single entity key.
create unique index polls_approval_unique
    on polls (person_id, party_id, alliance_id, approval_period)
    nulls not distinct
    where kind = 'approval';

-- Option positions are unique within a poll (they always were, in practice);
-- making it a constraint lets an approval poll's three options be created
-- idempotently under a race.
alter table poll_options
    add constraint poll_options_poll_position_key unique (poll_id, position);
