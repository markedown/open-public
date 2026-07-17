-- An optional own-words description of what an election was about (most useful
-- for referendums, where the question needs context).
--
-- Migrations are append-only; never edit this file once applied.

alter table elections add column description text;
