-- Scope people and parties to a country.
--
-- The platform holds one independent dataset per country. Elections, alliances,
-- polls and events already carry a `country_id`; people and parties did not,
-- which meant a per-country list or count could not be expressed. This adds the
-- column (nullable, since a fresh database has no country to backfill to; the
-- data pipeline sets it, and existing single-country data is backfilled by the
-- deployment) and indexes it for the per-country list pages.
--
-- Migrations are append-only; never edit this file once it has been applied.

alter table people add column country_id bigint references countries (id);
alter table parties add column country_id bigint references countries (id);

create index people_country_id_idx on people (country_id);
create index parties_country_id_idx on parties (country_id);
