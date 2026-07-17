-- Diacritic-insensitive text matching for search.
--
-- Names in many locales carry diacritics (é, ö, ş, ğ, ı) that people routinely
-- omit when typing. The `unaccent` extension folds them to ASCII, so a list
-- search can match "ayse" against "Ayşe". It is only used at query time in the
-- searchable list pages; the full-text `fts` columns are unchanged.
--
-- Migrations are append-only; never edit this file once it has been applied.

create extension if not exists unaccent;
