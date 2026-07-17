-- Optional images on a poll's question and on each option.
--
-- For image polls (e.g. comparing logos or photos). Images are admin-curated
-- editorial content, not user uploads: only a freely-licensed image URL with
-- its license is stored, mirroring people.photo_url / photo_license. The full
-- image is never hosted here, only referenced.
--
-- Migrations are append-only; never edit this file once applied.

alter table polls
    add column media_url text,
    add column media_license text;

alter table poll_options
    add column media_url text,
    add column media_license text;
