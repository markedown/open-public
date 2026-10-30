#!/usr/bin/env python3
"""Check that the compass does not lean by construction.

A voting advice tool can favour a party without a single wrong fact: wording most
theses so one side agrees rewards anyone who tends to say yes, and a party with
few recorded stances gets a figure that swings more than the others'. Those are
properties of the published data, so they can be measured from `dataset/` alone,
with the same rule the site uses to turn evidence into a stance: a recorded act
before a pledge, then the most recent.

Contestants resting on exactly the same evidence are one measurement and are
counted once, as they are on the site.

It reports every country. A country listed in `compass_reviewed.txt`, next to the
data, has had its theses reviewed for this and must also pass:

1. Every party's mean stance is close to zero, so saying yes to everything does
   not lean. A party's mean is its answer to "how many theses are worded my way".
2. Agreeing with everything and disagreeing with everything do not favour a side:
   no party scores far from the others under either.
3. Every thesis separates someone: at least two contestants hold a stance on it,
   and they do not all hold the same one.
4. Every party has a stance on enough of the theses for its figure to mean
   something, and every thesis has a topic.

The share of random voters each party attracts is reported and never enforced. A
party in the middle is closer to a moderate voter because it is in the middle, and
evening that out would be falsifying the result.

Exits non-zero if a reviewed country fails. Usage: check_compass.py [dir]
"""
import argparse
import collections
import json
import os
import random
import sys

RECORDED = {"bill", "court", "vote", "law", "decree", "alliance"}
MAX_MEAN = 0.5
MAX_SPREAD = 25
MIN_COVERAGE = 0.5
VOTERS = 20000


def load(root, name):
    with open(os.path.join(root, f"{name}.json"), encoding="utf-8") as fh:
        return json.load(fh)


def stance_of(rows):
    """The stance the site derives from one contestant's evidence on one thesis."""
    def strength(e):
        return (e["kind"] in RECORDED, e.get("occurred_on") or "")
    return max(rows, key=strength)["stance"]


def build(evidence, theses):
    """{contestant: {thesis: stance}} for one country's party theses, merged where
    the evidence is identical, and how many recorded acts stand behind each."""
    by = collections.defaultdict(lambda: collections.defaultdict(list))
    for e in evidence:
        if e.get("party") and e["thesis"] in theses:
            by[e["party"]][e["thesis"]].append(e)
    merged = {}
    for party in sorted(by):
        sig = tuple(sorted(
            (t, e["kind"], e["stance"], e.get("source_url") or "", e.get("occurred_on") or "")
            for t, rows in by[party].items() for e in rows))
        merged.setdefault(sig, []).append(party)
    stances, recorded = {}, {}
    for parties in merged.values():
        rows = by[parties[0]]
        label = " + ".join(parties)
        stances[label] = {t: stance_of(r) for t, r in rows.items()}
        recorded[label] = sum(e["kind"] in RECORDED for r in rows.values() for e in r)
    return stances, recorded


def score(answers, stance):
    earned = possible = 0
    for thesis, value in stance.items():
        if thesis in answers:
            earned += 4 - abs(answers[thesis] - value)
            possible += 4
    return 100 * earned / possible if possible else None


def top_shares(stances, theses, choices):
    rng = random.Random(1)
    wins = collections.Counter()
    for _ in range(VOTERS):
        answers = {t: rng.choice(choices) for t in theses}
        scores = {p: score(answers, s) for p, s in stances.items()}
        best = max(v for v in scores.values() if v is not None)
        leaders = [p for p, v in scores.items() if v == best]
        for p in leaders:
            wins[p] += 1 / len(leaders)
    return {p: round(100 * wins[p] / VOTERS) for p in stances}


def check(country, theses, evidence):
    texts = {t["text"] for t in theses}
    stances, recorded = build(evidence, texts)
    failures = []
    if not stances:
        return ["no party holds a stance"], {}
    report = {"theses": len(texts), "parties": {}}
    for party, st in sorted(stances.items()):
        mean = sum(st.values()) / len(st)
        report["parties"][party] = {
            "stances": len(st),
            "recorded_acts": recorded[party],
            "mean_stance": round(mean, 2),
            "all_agree": round(score({t: 2 for t in texts}, st)),
            "all_disagree": round(score({t: -2 for t in texts}, st)),
        }
        if abs(mean) > MAX_MEAN:
            failures.append(f"{party}: mean stance {mean:+.2f}, beyond +/-{MAX_MEAN}: "
                            "the theses are worded towards or against it")
        if len(st) / len(texts) < MIN_COVERAGE:
            failures.append(f"{party}: a stance on only {len(st)} of {len(texts)} theses")
    for key in ("all_agree", "all_disagree"):
        values = [p[key] for p in report["parties"].values()]
        if max(values) - min(values) > MAX_SPREAD:
            failures.append(f"{key}: {min(values)} to {max(values)} between parties, "
                            f"more than {MAX_SPREAD} apart")
    for text in sorted(texts):
        held = [st[text] for st in stances.values() if text in st]
        if len(held) < 2:
            failures.append(f"only {len(held)} contestant on: {text}")
        elif len(set(held)) == 1:
            failures.append(f"everyone agrees, so it separates nobody: {text}")
    for t in theses:
        if not t.get("topic"):
            failures.append(f"no topic: {t['text']}")
    report["top_share_any_answer"] = top_shares(stances, texts, [-2, -1, 0, 1, 2])
    report["top_share_moderate"] = top_shares(stances, texts, [-1, 0, 1])
    return failures, report


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("dir", nargs="?", default="dataset")
    args = ap.parse_args()
    with open(os.path.join(args.dir, "compass_reviewed.txt"), encoding="utf-8") as fh:
        reviewed = set(fh.read().split())
    theses = [t for t in load(args.dir, "theses") if t.get("scope", "party") == "party"]
    evidence = load(args.dir, "position_evidence")
    failed = False
    for country in sorted({t["country"] for t in theses}):
        rows = [t for t in theses if t["country"] == country]
        ev = [e for e in evidence if e["country"] == country]
        failures, report = check(country, rows, ev)
        strict = country in reviewed
        print(f"{country}{' (reviewed)' if strict else ''}: {report.get('theses', 0)} theses")
        for party, r in report.get("parties", {}).items():
            print(f"  {party}: {r['stances']} stances, {r['recorded_acts']} recorded acts, "
                  f"mean {r['mean_stance']:+.2f}, all agree {r['all_agree']}, "
                  f"all disagree {r['all_disagree']}")
        if report:
            print(f"  first place, any answers: {report['top_share_any_answer']}")
            print(f"  first place, moderate answers: {report['top_share_moderate']}")
        for f in failures:
            print(f"  {'FAIL' if strict else 'note'}: {f}")
        failed = failed or (strict and bool(failures))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
