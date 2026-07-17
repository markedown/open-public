-- Turnout and vote totals for an election, enabling vote-share and turnout
-- figures alongside seats.
--
--   electorate   registered voters
--   votes_cast   total ballots cast (turnout numerator)
--   valid_votes  valid votes (the denominator for a party's vote share)
--
-- All optional: an election may be recorded with seats only until the totals
-- are ingested.
--
-- Migrations are append-only; never edit this file once applied.

alter table elections
    add column electorate  bigint,
    add column votes_cast  bigint,
    add column valid_votes bigint;
