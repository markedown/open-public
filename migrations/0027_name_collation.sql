-- Locale-aware, Unicode-correct ordering for user-visible name and label sorts.
--
-- The default database collation orders text by byte value, which mis-sorts
-- non-ASCII names: a leading diacritic byte sorts after every ASCII letter, so
-- "Çilek" lands after "Zebra" instead of near "C". This ICU collation uses the
-- root ("undetermined") locale, giving Unicode-aware ordering that is correct
-- across locales without tailoring to any single country. Name-ordered queries
-- on user-visible lists reference it with `collate "name_sort"`.
--
-- Migrations are append-only; never edit this file once it has been applied.

create collation if not exists "name_sort" (provider = icu, locale = 'und');
