-- Attach a poll directly to a country.
--
-- Polls could already attach to a person, party, or topic. A country-level poll
-- (a general question for a whole country, not tied to one party or person) had
-- no home. Add an optional country_id so such polls surface on the country page.
--
-- Migrations are append-only; never edit this file once applied.

alter table polls add column country_id bigint references countries(id);

create index polls_country_id_idx on polls(country_id);
