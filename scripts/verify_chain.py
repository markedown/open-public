#!/usr/bin/env python3
"""Verify the anonymous poll dump at /data/polls.json.

Reads the published participation dump and, for every poll, checks:

  1. the hash chain: each ballot's content_hash recomputed from the previous
     head, in gap-free sequence, matching the published head;
  2. no double vote: no token appears twice in a poll;
  3. reconciliation: a poll never has more ballots than it issued tokens;
  4. tallies: each option's count recomputed from the ballots matches;
  5. signatures (only if the `cryptography` package is installed): each ballot's
     blind signature verifies over its token under the poll's public key, i.e.
     the ballot really was issued for this poll.

Any alteration, reordering, insertion or removal after casting produces a
mismatch. Checks 1-4 use the standard library only; check 5 is skipped with a
note when `cryptography` is absent.

What this proves and does not prove:
This proves ballots were issued for the poll, are unaltered, and are not
double-voted, and that the published tallies match the ballots.
It does not prove that an account is a unique living person, nor that
participants form a representative sample or cross-section of any
population.

The chain hashing must match crates/db/src/voting.rs exactly:

  content_hash = sha256(prev || poll_id_be8 || token || option_id_be8... || seq_be8)
  genesis      = sha256("open-public/ballot/" || poll_id)     # prev for seq 1

with the option ids sorted ascending, each an 8-byte big-endian integer.

Usage:
  curl -s https://<host>/data/polls.json | verify_chain.py -
  verify_chain.py polls.json
Exit code 0 on success, 1 on any mismatch.
"""

import hashlib
import json
import struct
import sys

try:
    import base64

    from cryptography.exceptions import InvalidSignature
    from cryptography.hazmat.primitives import hashes, serialization
    from cryptography.hazmat.primitives.asymmetric import padding

    HAVE_CRYPTO = True
except ImportError:
    HAVE_CRYPTO = False


def be(n: int) -> bytes:
    """8-byte big-endian signed integer (matches Rust i64::to_be_bytes)."""
    return struct.pack(">q", n)


def genesis(poll_id: int) -> bytes:
    return hashlib.sha256(b"open-public/ballot/" + str(poll_id).encode()).digest()


def content_hash(prev: bytes, poll_id: int, token: bytes, options, seq: int) -> bytes:
    h = hashlib.sha256()
    h.update(prev)
    h.update(be(poll_id))
    h.update(token)
    for o in sorted(options):
        h.update(be(o))
    h.update(be(seq))
    return h.digest()


def verify_signature(public_key_b64: str, ballot: dict) -> bool:
    """RSA-PSS (SHA-384, salt 48) over randomizer||token, i.e. RFC 9474
    RSABSSA-SHA384-PSS-Randomized."""
    pub = serialization.load_der_public_key(base64.b64decode(public_key_b64))
    randomizer = bytes.fromhex(ballot.get("randomizer") or "")
    message = randomizer + bytes.fromhex(ballot["token"])
    try:
        pub.verify(
            bytes.fromhex(ballot["signature"]),
            message,
            padding.PSS(mgf=padding.MGF1(hashes.SHA384()), salt_length=48),
            hashes.SHA384(),
        )
        return True
    except InvalidSignature:
        return False


def verify_poll(poll: dict, ballots, label: str, check_sigs: bool) -> bool:
    ballots = sorted(ballots, key=lambda b: b["seq"])
    published = poll.get("chain") or {}

    # A poll with no ballots publishes no head.
    if not ballots:
        if published.get("hash") or published.get("seq"):
            print(f"FAIL {label}: head published but the file has no ballots")
            return False

    # 3. reconciliation: ballots cast never exceed tokens issued.
    if len(ballots) > poll.get("issued", 0):
        print(f"FAIL {label}: {len(ballots)} ballots but only {poll.get('issued', 0)} issued")
        return False

    # 1. + 2. chain and no-double-spend.
    seen = set()
    prev = genesis(poll["poll_id"]) if ballots else None
    for i, b in enumerate(ballots, start=1):
        if b["seq"] != i:
            print(f"FAIL {label}: expected seq {i}, got {b['seq']} (gap or reorder)")
            return False
        if b["token"] in seen:
            print(f"FAIL {label}: token {b['token'][:16]}... spent twice")
            return False
        seen.add(b["token"])
        if i > 1 and (b.get("prev_hash") or "") != prev.hex():
            print(f"FAIL {label}: ballot {i} prev_hash does not link to the chain")
            return False
        h = content_hash(prev, poll["poll_id"], bytes.fromhex(b["token"]), b["options"], i)
        if h.hex() != b["content_hash"]:
            print(f"FAIL {label}: ballot seq {i} hash mismatch (altered or removed)")
            return False
        if check_sigs and not verify_signature(poll["public_key"], b):
            print(f"FAIL {label}: ballot seq {i} signature invalid (not issued for this poll)")
            return False
        prev = h

    if ballots and published.get("hash") and published["hash"].lower() != prev.hex():
        print(f"FAIL {label}: head mismatch (published {published['hash']})")
        return False

    # 4. tallies: recompute each option's count from the ballots.
    counts = {}
    for b in ballots:
        for o in b["options"]:
            counts[o] = counts.get(o, 0) + 1
    for opt in poll.get("options", []):
        if counts.get(opt["id"], 0) != opt["votes"]:
            print(f"FAIL {label}: option {opt['label']!r} tally {opt['votes']} != recomputed "
                  f"{counts.get(opt['id'], 0)}")
            return False

    note = "" if (check_sigs or not ballots) else " (signatures not checked)"
    print(f"OK  {label}: {len(ballots)} ballot(s) verified{note}")
    return True


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 1
    source = sys.stdin if sys.argv[1] == "-" else open(sys.argv[1], encoding="utf-8")
    with source as fh:
        data = json.load(fh)

    check_sigs = HAVE_CRYPTO
    if not HAVE_CRYPTO:
        print("note: install the `cryptography` package to also verify signatures", file=sys.stderr)

    by_poll = {p["slug"]: [] for p in data.get("polls", [])}
    for b in data.get("ballots", []):
        by_poll.setdefault(b["poll"], []).append(b)

    ok = True
    total = 0
    for poll in sorted(data.get("polls", []), key=lambda p: p["slug"]):
        ballots = by_poll.get(poll["slug"], [])
        total += len(ballots)
        ok = verify_poll(poll, ballots, poll["slug"], check_sigs) and ok

    print(f"{'checked' if ok else 'FAILED'}: {len(data.get('polls', []))} poll(s), {total} ballot(s)")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
