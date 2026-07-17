-- A pending summary draft for a news item.
--
-- Summaries are written as drafts (for example by an automated summarizer) and
-- stay unpublished until an editor reviews them. `our_summary` remains the
-- published text shown to readers; `summary_draft` holds a proposed summary
-- awaiting review. Approving copies the (possibly edited) draft into
-- `our_summary` and clears the draft; discarding just clears it.
--
-- Migrations are append-only; never edit this file once it has been applied.

alter table news_items add column summary_draft text;
