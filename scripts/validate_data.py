#!/usr/bin/env python3
"""Check the published dataset in `dataset/` for the guarantees it claims.

CC0 is permanent and git history is permanent, so a bad export is not something
that can be taken back. This refuses to let one through, and it runs in CI on
every change to the data, so the guarantees below are not a promise in a README
but something anyone can re-run.

It checks four things:

1. Nothing personal or unreviewed is in the files. Not by trusting the exporter,
   but by looking for the columns and shapes that would carry it.
2. Every fact row cites a source, and that source is in the export.
3. Every reference resolves: a membership names a person and a party that exist.
4. Nothing collapsed. Row counts are compared against the previous manifest, and
   a large unexplained drop fails.

Exits non-zero on any failure. Usage: validate_data.py [dir] [--baseline m.json]
"""
import argparse
import json
import os
import sys

# Columns that must never appear in a published file, whatever the exporter did.
FORBIDDEN_FIELDS = {
    "summary_draft", "email", "email_hash", "password_hash", "token_hash",
    "user_id", "submitter_id", "reviewed_by", "ai_reason", "admin_note",
    "id", "person_id", "party_id", "country_id", "source_id", "thesis_id",
}
# Files whose every row states a fact about the world and therefore needs a
# source. The rest are either joins between rows that carry their own, or our
# own vocabulary.
NEEDS_SOURCE = {
    "countries", "people", "parties", "party_memberships", "roles", "elections",
    "election_results", "alliances", "party_alliances", "events", "statements",
    "news_items", "person_attributes", "person_education", "theses",
    "position_evidence",
}
# file -> (field in that file, file the value must exist in, key there)
REFERENCES = [
    ("people", "country", "countries", "slug"),
    ("parties", "country", "countries", "slug"),
    ("party_memberships", "person", "people", "slug"),
    ("party_memberships", "party", "parties", "slug"),
    ("roles", "person", "people", "slug"),
    ("elections", "country", "countries", "slug"),
    ("election_results", "election", "elections", "slug"),
    ("election_results", "party", "parties", "slug"),
    ("alliances", "country", "countries", "slug"),
    ("party_alliances", "party", "parties", "slug"),
    ("party_alliances", "alliance", "alliances", "slug"),
    ("person_attributes", "person", "people", "slug"),
    ("person_education", "person", "people", "slug"),
    ("theses", "country", "countries", "slug"),
    ("position_evidence", "country", "countries", "slug"),
    ("position_evidence", "party", "parties", "slug"),
    ("position_evidence", "person", "people", "slug"),
    ("news_item_people", "person", "people", "slug"),
    ("news_item_parties", "party", "parties", "slug"),
    ("poll_options", "poll", "polls", "slug"),
    ("outlets", "country", "countries", "slug"),
]
# How far a file may shrink before it looks like something broke rather than
# something was corrected.
MAX_SHRINK = 0.10


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("dir", nargs="?", default="dataset")
    ap.add_argument("--baseline", help="a previous manifest.json to compare counts against")
    args = ap.parse_args()

    # Every data file is an array of rows. The descriptors that travel with the
    # export (manifest, datapackage) are not, and are checked by being present
    # rather than by being read as data.
    files, descriptors = {}, []
    for name in sorted(os.listdir(args.dir)):
        if not name.endswith(".json"):
            continue
        with open(os.path.join(args.dir, name), encoding="utf-8") as fh:
            content = json.load(fh)
        if isinstance(content, list):
            files[name[:-5]] = content
        else:
            descriptors.append(name)

    failures = []

    # 0. The export must be what it says it is.
    #
    # Every check below reads the files that are present, so on its own it would
    # report a clean export for one that had lost a file entirely: nothing to
    # read is not the same as nothing wrong. The export describes itself in
    # datapackage.json and manifest.json, and holding it to that description is
    # what turns "these rows are fine" into "these are the rows".
    #
    # It also closes the gap in the check below it. FORBIDDEN_FIELDS is a list
    # of what may not be published, so a column nobody thought to add to it
    # travels. Requiring every field to be one the datapackage declares reverses
    # that: a new column has to be declared, in a reviewed file, before it can
    # reach anyone.
    declared, counts = {}, {}
    try:
        with open(os.path.join(args.dir, "datapackage.json"), encoding="utf-8") as fh:
            declared = {r["name"]: {f["name"] for f in r["schema"]["fields"]}
                        for r in json.load(fh)["resources"]}
        with open(os.path.join(args.dir, "manifest.json"), encoding="utf-8") as fh:
            counts = json.load(fh)["counts"]
    except (OSError, KeyError, ValueError) as e:
        failures.append(f"the export does not describe itself: {e}")
    for name in sorted(set(declared) - set(files)):
        failures.append(f"{name}: declared in datapackage.json, no such file")
    for name in sorted(set(files) - set(declared)):
        failures.append(f"{name}: published but not declared in datapackage.json")
    for name in sorted(set(declared) & set(files)):
        if name in counts and len(files[name]) != counts[name]:
            failures.append(f"{name}: {counts[name]} rows in the manifest, "
                            f"{len(files[name])} in the file")
        undeclared = {k for r in files[name] for k in r} - declared[name]
        if undeclared:
            failures.append(f"{name}: field(s) not declared in datapackage.json "
                            f"{sorted(undeclared)}")
    # News is not part of the dataset. Every other file states a structural fact
    # sourced to a document; a news summary is our own prose about what someone
    # is accused of, and it decays: the story moves on, the dataset should not.
    for banned in ("news_items", "news_item_people", "news_item_parties"):
        if banned in files:
            failures.append(f"{banned}: news is not part of the published dataset")
    for required in ("manifest.json", "datapackage.json"):
        if required not in descriptors:
            failures.append(f"{required} is missing from the export")

    # 1. Nothing personal or unreviewed.
    for name, rows in files.items():
        fields = {k for r in rows for k in r}
        leaked = fields & FORBIDDEN_FIELDS
        if leaked:
            failures.append(f"{name}: forbidden field(s) {sorted(leaked)}")
        for i, r in enumerate(rows):
            for k, v in r.items():
                if isinstance(v, str) and v.startswith("$argon2"):
                    failures.append(f"{name}[{i}].{k}: looks like a password hash")
                if isinstance(v, str) and "@" in v and "." in v and " " not in v \
                        and not v.startswith("http") and k not in ("url", "source_url"):
                    failures.append(f"{name}[{i}].{k}: looks like an email address")

    # 2. Every fact cites a source that is in the export.
    known_sources = {(s["url"], s["content_hash"]) for s in files.get("sources", [])}
    for name in NEEDS_SOURCE:
        for i, r in enumerate(files.get(name, [])):
            key = (r.get("source_url"), r.get("source_hash"))
            if key[0] is None:
                failures.append(f"{name}[{i}]: no source")
            elif key not in known_sources:
                failures.append(f"{name}[{i}]: source not in sources.json ({key[0]})")

    # 3. Every reference resolves. A null reference is allowed where the schema
    #    allows one; a value that points at nothing is not.
    for name, field, target, key in REFERENCES:
        if name not in files or target not in files:
            continue
        valid = {r[key] for r in files[target]}
        for i, r in enumerate(files[name]):
            v = r.get(field)
            if v is not None and v not in valid:
                failures.append(f"{name}[{i}].{field}: no such {target} '{v}'")

    # 4. Nothing collapsed since the last export.
    if args.baseline and os.path.exists(args.baseline):
        with open(args.baseline, encoding="utf-8") as fh:
            before = json.load(fh).get("counts", {})
        for name, was in before.items():
            if name not in files:
                # A file that is gone entirely is either a deliberate decision
                # or a broken export, and the two must not look alike: say so
                # and make the baseline be refreshed on purpose.
                failures.append(f"{name}: in the previous export, absent now")
            elif was and len(files[name]) < was * (1 - MAX_SHRINK):
                failures.append(f"{name}: {was} rows before, {len(files[name])} now")

    for f in failures[:40]:
        print(f"FAIL  {f}", file=sys.stderr)
    if len(failures) > 40:
        print(f"      ... and {len(failures) - 40} more", file=sys.stderr)
    if failures:
        print(f"\n{len(failures)} problem(s); not fit to publish", file=sys.stderr)
        return 1
    rows = sum(len(r) for r in files.values())
    print(f"ok: {len(files)} files, {rows} rows, every fact sourced, "
          f"every reference resolved", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
