-- Outlets do not need a provenance source.
--
-- An outlet is a publisher, not a curated political fact: its own website is its
-- reference, and its political leaning is our editorial assessment, not a
-- formally citable claim. Requiring a `sources` row for an outlet only produced
-- placeholder rows, so the source reference becomes optional. The political
-- content tables (people, parties, statements, elections, votes) keep their
-- mandatory sources; this relaxation is scoped to outlet metadata.
--
-- Migrations are append-only; never edit this file once it has been applied.

alter table outlets alter column source_id drop not null;
