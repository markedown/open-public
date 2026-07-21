#!/usr/bin/env python3
"""Verify a poll's vote hash chain from a published dump.

Reads the published participation dump, recomputes every poll's chain from its
genesis, and checks each row hash and each published head. Any alteration,
reordering, insertion, or removal after casting produces a mismatch. No
third-party dependencies.

Each vote carries: poll_id, seq, option_id, voter (the opaque per-poll index),
cast_at (ISO-8601 UTC) and row_hash (hex). The hashing must match
migrations/0004_vote_hash_chain.sql exactly:

  content  = big-endian 8-byte integers of
             poll_id, seq, option_id, voter_index, cast_at_micros
  row_hash = sha256(prev_hash || content)
  genesis  = sha256("open-public/poll/" || poll_id)   # prev for seq 1

cast_at_micros is whole microseconds since the Unix epoch (UTC), computed with
exact integer arithmetic so it matches Postgres' numeric extract(epoch ...).

Usage:
  curl -s https://<host>/data/polls.json | verify_chain.py -
  verify_chain.py polls.json
  verify_chain.py one-poll-votes.json [expected_head_hash_hex]
Exit code 0 on success, 1 on any mismatch.
"""

import hashlib
import json
import struct
import sys
from datetime import datetime, timedelta, timezone

EPOCH = datetime(1970, 1, 1, tzinfo=timezone.utc)


def be(n: int) -> bytes:
    """8-byte big-endian signed integer (matches Postgres int8send)."""
    return struct.pack(">q", n)


def cast_at_micros(iso: str) -> int:
    dt = datetime.fromisoformat(iso)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return (dt.astimezone(timezone.utc) - EPOCH) // timedelta(microseconds=1)


def genesis(poll_id: int) -> bytes:
    return hashlib.sha256(b"open-public/poll/" + str(poll_id).encode()).digest()


def voter_index(v):
    """The opaque per-poll index. The published dump calls it `voter`."""
    return v["voter_index"] if "voter_index" in v else v["voter"]


def row_hash(prev: bytes, v: dict) -> bytes:
    content = (
        be(v["poll_id"])
        + be(v["seq"])
        + be(v["option_id"])
        + be(voter_index(v))
        + be(cast_at_micros(v["cast_at"]))
    )
    return hashlib.sha256(prev + content).digest()


def votes_of(data):
    """Group votes by poll, from the published dump or a bare array.

    The dump at /data/polls.json carries every poll together, with each poll's
    published chain head. A bare array of one poll's votes is also accepted,
    which is what an extract from the database looks like.
    """
    if isinstance(data, list):
        return {None: (data, None)}
    heads = {}
    for poll in data.get("polls", []):
        chain = poll.get("chain") or {}
        heads[poll["slug"]] = (chain.get("hash"), chain.get("seq"))
    out = {}
    for v in data.get("votes", []):
        out.setdefault(v["poll"], []).append(v)
    return {slug: (rows, heads.get(slug)) for slug, rows in out.items()}


def verify(rows, published_head, label) -> bool:
    rows = sorted(rows, key=lambda v: v["seq"])
    if not rows:
        print(f"OK  {label}: empty chain")
        return True

    prev = genesis(rows[0]["poll_id"])
    for i, v in enumerate(rows, start=1):
        if v["seq"] != i:
            print(f"FAIL {label}: expected seq {i}, got {v['seq']} (gap or reorder)")
            return False
        h = row_hash(prev, v)
        if h.hex() != v["row_hash"]:
            print(f"FAIL {label}: vote seq {i} hash mismatch (altered or removed)")
            return False
        prev = h

    if published_head:
        head_hash, head_seq = published_head
        if head_hash and head_hash.lower() != prev.hex():
            print(f"FAIL {label}: head mismatch (published {head_hash})")
            return False
        if head_seq is not None and head_seq != len(rows):
            print(f"FAIL {label}: published head is at seq {head_seq}, file has {len(rows)}")
            return False

    print(f"OK  {label}: {len(rows)} votes verified; head {prev.hex()}")
    return True


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 1
    source = sys.stdin if sys.argv[1] == "-" else open(sys.argv[1], encoding="utf-8")
    with source as fh:
        data = json.load(fh)

    ok = True
    for slug, (rows, head) in sorted(votes_of(data).items(), key=lambda kv: kv[0] or ""):
        # An explicit head on the command line still wins, for the bare-array
        # case where the file itself carries none.
        if slug is None and len(sys.argv) > 2:
            head = (sys.argv[2], None)
        ok = verify(rows, head, slug or "chain") and ok
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
