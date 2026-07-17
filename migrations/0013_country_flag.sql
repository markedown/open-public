-- A country's flag image, shown on the country page.
--
-- A freely-licensed image URL (national flags are public-domain symbols); the
-- image is referenced, never hosted here.
--
-- Migrations are append-only; never edit this file once applied.

alter table countries add column flag_url text;
