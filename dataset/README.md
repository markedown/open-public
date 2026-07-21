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

## What this dataset is not

It is a record of what documents say, not a judgement of who is right. Coverage is uneven by
nature: parties publish different amounts, and documents are silent on whole topics, so a
contestant with no recorded stance on a question is a gap in the sources rather than a
neutral position. Poll results elsewhere on the platform are participatory and are never a
representative survey.
