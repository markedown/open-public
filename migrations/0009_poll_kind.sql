-- A poll kind, driving how the widget renders and how many options a voter
-- picks.
--
--   single  one option, shown as stacked result bars (the existing behaviour)
--   yesno   one option, shown as two prominent buttons
--   scale   one option on an ordered 1..N rating row
--
-- All three are single-choice, so the vote model is unchanged. Multi-select is
-- a separate migration because it changes the one-vote-per-poll invariant and
-- the integrity chain.
--
-- Migrations are append-only; never edit this file once applied.

alter table polls
    add column kind text not null default 'single'
        check (kind in ('single', 'yesno', 'scale'));
