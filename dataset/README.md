# The dataset

Everything the platform knows about people, parties, roles, elections and recorded
positions, as one JSON file per entity type. This is the same data the site serves, so
anything that can be read here can be checked against the source it came from.

## Licence

The data is released under [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/):
no rights reserved, no attribution required, no permission needed. The repository's code is
licensed separately (MIT or Apache-2.0); this directory is not.

One limit, stated plainly: **quoted excerpts are not ours to relicense.** Where a row quotes
a party programme, a law or a parliamentary record, those words belong to whoever wrote them
and appear here as citations. CC0 covers our compilation, our own prose and the structure,
not the quotations inside it.

## Shape

One file per entity type, an array of objects, two-space indent, sorted keys, rows in a
stable order. A day with no editorial change produces an empty diff, so the git history is a
record of what actually changed and when.

**Nothing is keyed by a database id.** Rows reference each other by slug, and a source by its
`url` and `content_hash` together. Ids are an implementation detail that changes when a
database is rebuilt, and that must not read as a change in the data.

`datapackage.json` describes every file, its fields and their types, as a
[Frictionless](https://specs.frictionlessdata.io/data-package/) data package, so the dataset
loads with standard tooling without reading this file first. `manifest.json` carries the row
counts of the export.

`roll_calls.json` holds one row per recorded vote, with the members who voted each way as
lists of slugs. `absent` means the chamber's record says the member did not vote; it does not
say why.

`thesis_roll_calls.json` says how each side of a vote reads on a compass thesis. A side left null does
not say where someone stands, and a `position_evidence` row with a `roll_call` is a party's vote read
that way. `constituency_places.json` lists municipalities and the constituency they vote in. It is the
one file under another licence: Datenlizenz Deutschland Namensnennung 2.0
(https://www.govdata.de/dl-de/by-2-0), © Die Bundeswahlleiterin, Statistische Ämter des Bundes und der
Länder, Wiesbaden 2024.

`initiatives.json` and `group_votes.json` are not under CC0. They are read from official parliamentary
material whose [terms of use](https://dip.bundestag.de/documents/nutzungsbedingungen_dip.pdf) allow any
reuse on condition of attribution. Pass them on with the source "Deutscher Bundestag/Bundesrat – DIP"
and the document designation each row carries (BT-Drs., BT-PlPr.). The sides in `group_votes.json` are
our reading of the record, so they count as processed data; `excerpt` is the record's wording.

`initiatives.json` lists the bills and motions tabled, with the parties behind them.
`group_votes.json` holds votes by show of hands, each with the record's wording in `excerpt`. `for`,
`against` and `abstain` are relative to the bill or motion, even where the chamber voted on a
recommendation to reject it.

`compass_reviewed.txt` lists the countries whose theses have been reviewed for balance.
`scripts/check_compass.py` holds those to its limits and only reports on the others.

## Provenance

Every fact row carries `source_url` and `source_hash`, and `sources.json` holds the full
record of each cited document: what it is, when we fetched it, and, where we downloaded the
bytes ourselves, `content_sha256` of exactly what we read. Where a document has been
archived, `snapshot_url` points at the copy, because political documents rot.

`position_evidence.json` additionally carries a `locator`: the page, article, decree number
or roll call the quote comes from. A stance in this dataset is never asserted on its own. It
is derived from dated, typed, sourced evidence, and where a party's pledge and its record
disagree, both are here and the disagreement is visible.

## What is deliberately not here

- **Anything about a user.** Accounts, sessions, verification codes, poll submissions.
- **Votes.** Poll results are participation data with their own guarantees and are not part
  of this export.
- **Drafts.** Summaries awaiting review and unpublished translations never leave the
  database. Only approved content is exported.
- **News.** Every other file states a structural fact about a public institution, sourced to
  a document. A news summary is our own prose about what someone is accused of, and it
  decays as the story moves on. The platform carries news; the dataset does not.

## Checking it

```
python3 scripts/validate_data.py dataset
```

This is the same check CI runs on every change here. It verifies that no personal or
unreviewed field is present, that every fact cites a source that is in the export, that every
reference resolves, and that no file has collapsed since the previous export.

## Contributing a correction or a country

Open a pull request that changes the JSON files here. The rules are the ones the checker
enforces:

- Every fact row cites a source. Add the source to `sources.json` first (its `url`, what it
  is, when you read it) and reference it from the row with `source_url` and `source_hash`.
- Reference other rows by slug, never by a number. Keep two-space indent, sorted keys and the
  existing row order, so the diff shows only what changed.
- A stance on a compass thesis is a `position_evidence` row with the page, article or number
  in `locator` and the quotation exactly as the document words it. Do not paraphrase inside
  the quotation marks.
- Run `python3 scripts/validate_data.py dataset` and `python3 scripts/check_compass.py dataset`,
  and load the result into a scratch database with `cargo run -p ingest -- import dataset`
  to see that it imports.

The harvesters that gather the data are not part of this repository, so a change to this
directory is the way to contribute data. A merged change is loaded into the maintainers'
database through the same importer, so the next export keeps it. For a single wrong fact
on the site, a data-correction issue with the source is enough.

## What this dataset is not

It is a record of what documents say, not a judgement of who is right. Coverage is uneven by
nature: parties publish different amounts, and documents are silent on whole topics, so a
contestant with no recorded stance on a question is a gap in the sources rather than a
neutral position. Poll results elsewhere on the platform are participatory and are never a
representative survey.
