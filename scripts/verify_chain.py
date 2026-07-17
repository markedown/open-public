#!/usr/bin/env python3
"""Verify a poll's vote hash chain from a published dump.

Reads a JSON array of the vote records for one poll (the shape published in the
public data dump), recomputes each row hash from the poll's genesis, and checks
the whole chain plus the head. Any alteration, reordering, insertion, or removal
after casting produces a hash mismatch. No third-party dependencies.

Each record has: poll_id, seq, option_id, voter_index, cast_at (ISO-8601 UTC),
row_hash (hex). The hashing must match migrations/0004_vote_hash_chain.sql
exactly:

  content  = big-endian 8-byte integers of
             poll_id, seq, option_id, voter_index, cast_at_micros
  row_hash = sha256(prev_hash || content)
  genesis  = sha256("open-public/poll/" || poll_id)   # prev for seq 1

cast_at_micros is whole microseconds since the Unix epoch (UTC), computed with
exact integer arithmetic so it matches Postgres' numeric extract(epoch ...).

Usage:
  verify_chain.py votes.json [expected_head_hash_hex]
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


def row_hash(prev: bytes, v: dict) -> bytes:
    content = (
        be(v["poll_id"])
        + be(v["seq"])
        + be(v["option_id"])
        + be(v["voter_index"])
        + be(cast_at_micros(v["cast_at"]))
    )
    return hashlib.sha256(prev + content).digest()


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 1
    votes = sorted(json.load(open(sys.argv[1])), key=lambda v: v["seq"])
    if not votes:
        print("OK: empty chain")
        return 0

    prev = genesis(votes[0]["poll_id"])
    for i, v in enumerate(votes, start=1):
        if v["seq"] != i:
            print(f"FAIL: expected seq {i}, got {v['seq']} (gap or reorder)")
            return 1
        h = row_hash(prev, v)
        if h.hex() != v["row_hash"]:
            print(f"FAIL: vote seq {i} hash mismatch (altered or removed)")
            return 1
        prev = h

    print(f"OK: {len(votes)} votes verified; head {prev.hex()}")
    if len(sys.argv) > 2 and sys.argv[2].lower() != prev.hex():
        print(f"FAIL: head mismatch (published {sys.argv[2]})")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
