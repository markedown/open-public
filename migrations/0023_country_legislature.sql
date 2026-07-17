-- A country's own name for its national legislature.
--
-- The seat-composition section is headed "Parliament" by default, but that is
-- not what every country calls its legislature (the United States has a
-- Congress, and so on). This optional column holds the country's own term in
-- the source language; the country page translates it and falls back to the
-- generic "Parliament" heading when it is not set.
--
-- Migrations are append-only; never edit this file once it has been applied.

alter table countries add column legislature_name text;
