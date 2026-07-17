-- Drop the polls.is_informal flag.
--
-- Every poll on the platform is informal: there is no formal, representative
-- survey path. The boolean therefore carried no information, and its label
-- implied a formal/informal distinction that does not exist. Removed along with
-- the label in the UI.
--
-- Migrations are append-only; never edit this file once applied.

alter table polls drop column is_informal;
