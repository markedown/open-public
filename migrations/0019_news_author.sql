-- The byline of a news article.
--
-- Optional: the author as credited by the outlet, shown on the news item's
-- detail page. Never inferred; only stored when known (an editor enters it, or
-- ingestion reads it from the article's metadata).
--
-- Migrations are append-only; never edit this file once it has been applied.

alter table news_items add column author text;
