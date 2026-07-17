-- Give an alliance a country and its lifespan, so alliances can be listed per
-- country with an active/inactive status. An alliance is active while it has no
-- dissolution date.
--
-- Migrations are append-only; never edit this file once applied.

alter table alliances
    add column country_id bigint references countries(id),
    add column founded_date date,
    add column dissolved_date date;

create index alliances_country_id_idx on alliances(country_id);
