-- Scope news outlets to a country.
--
-- Outlets were a single shared registry, but the platform holds one independent
-- dataset per country, and the per-country news index links to a per-country
-- outlet list. This adds the country column (nullable, like the people/parties
-- scoping: a fresh database has no country to backfill to, the data pipeline
-- sets it, and existing single-country data is backfilled by the deployment).
--
-- Migrations are append-only; never edit this file once it has been applied.

alter table outlets add column country_id bigint references countries (id);

create index outlets_country_id_idx on outlets (country_id);
