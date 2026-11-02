# How the compass works, and how its balance is checked

Status: describes shipped code. Every rule and number here is read from
`crates/domain/src/compass.rs` (the scorer), `crates/db/src/compass.rs` (where a stance is resolved)
and `scripts/check_compass.py` (the balance check). When that code changes, this page changes with
it.

## 1. What the compass is

A compass is a questionnaire of positions for one country. A position is one proposition, and the
visitor answers it on a five-point scale: strongly disagree (-2), disagree (-1), neutral (0), agree
(+1), strongly agree (+2). Stances sit on the same scale, so an answer and a stance are directly
comparable.

A position can be left unanswered: every answer control starts on "skip", and a skipped position
simply does not count. Any position can be marked important, which doubles its weight in the result.

The result is a ranking. Each contestant with a recorded stance on at least one answered position is
listed with its percentage of agreement and the number of positions that percentage rests on. A
parliamentary compass ranks parties; a presidential one ranks people, because that is what the
election is contested by.

Nothing the visitor answers is stored. Answers arrive in the POST body, drive the score in memory,
and are gone once the response is rendered; the scorer takes values in and returns values out, with
no storage and no I/O. A shared result link carries only the finished ranking (each contestant's
slug, percentage and matched-position count) and is signed so an altered one is refused; it holds
nothing from which the answers could be rebuilt, and making or reading one writes nothing.

## 2. How the score is computed

For one answered position and one contestant:

- points = `4 - |answer - stance|`. Identical positions score 4, opposite extremes (-2 against +2)
  score 0, and every other case lands in `0..=4`. The distance is symmetric.
- weight = 2 when the visitor marked that position important, otherwise 1.
- the position contributes `points * weight` out of a possible `4 * weight`.

A contestant's percentage is its earned points over its possible points, summed only over the
positions the visitor answered and on which that contestant has a recorded stance. The exact figure
drives the ranking; the page shows it rounded to a whole number.

Ordering is by percentage descending. Equal percentages are ordered by the contestant's internal id
ascending, so the same inputs always produce the same order.

A contestant with no recorded stance on any answered position is left off the list rather than shown
as 0%: there is nothing to compare, so a zero would mislead rather than inform. A position the
visitor skipped is ignored for everyone, whatever the contestants say about it.

Contestants resting on exactly the same evidence (one programme, one parliamentary group) are one
measurement: they are scored and shown as a single row, mapped to the lowest id among them. Having
no evidence at all is not treated as sameness.

Worked example, invented numbers. A visitor answers three positions and marks the first important:

| position | answer | weight | stance | points | possible |
| --- | --- | --- | --- | --- | --- |
| A | +2 | 2 | +2 | 4 x 2 = 8 | 8 |
| B | +1 | 1 | -1 | 2 | 4 |
| C | 0 | 1 | +2 | 2 | 4 |

Earned 12 of a possible 16, so 75%, resting on 3 positions. A party holding a stance only on a
fourth position that the visitor skipped does not appear at all. A party also on 75% is placed by
the lower id.

## 3. Where a stance comes from

No row in the database says "this party holds this view". A stance is resolved when it is needed,
from `position_evidence`: dated, typed, sourced readings, each attached to exactly one party or one
person, each with a source row, and with the quotation verbatim and a locator (the page, article,
decree number or roll call it comes from) where the reading rests on a document.

Evidence comes in two tiers, distinguished by kind:

- recorded acts: what the contestant did, a vote, a law, a decree, a governing alliance, a bill or
  motion it tabled, an application to a constitutional court (`bill`, `motion`, `court`, `vote`,
  `law`, `decree`, `alliance`)
- stated intentions: what it said it would do, a manifesto or a statement (`manifesto`, `statement`)

The site's rule is in `positions_for_country` in `crates/db/src/compass.rs`: one row per position
and contestant, and the strongest evidence wins. Recorded act before stated intention, then the most
recent date (an undated row ranks below any dated one), then the latest row entered. So a party that
pledges one thing and legislates another is scored on what it legislated, even where the pledge is
the newer document. The acts open to a party out of power (tabling a bill, going to court) count as
action, because otherwise only a party that holds power could ever be measured by deeds.

`scripts/check_compass.py` carries its own copy of the same rule, `stance_of`: among one
contestant's evidence on one position, the row with the highest `(is a recorded act, date)` wins.
The two rules must match, and neither may drift on its own. The checker measures the compass by
re-deriving its stances from the published data, so the moment its rule stopped being the site's
rule it would be grading a compass that no longer exists.

Where one contestant's evidence points both ways, nothing is averaged. The page shows the resolved
stance, marks it ("Pledge differs from record"), states the rule that settled it, and lists every
piece of evidence with its source, so the disagreement is visible instead of resolved away.

## 4. How the questions are kept balanced

Wording can lean without a single wrong fact. Positions phrased so that one side agrees reward a
visitor who tends to say yes. And a contestant with stances on only a few positions gets a figure
that swings more than the others'. Both are properties of the published data, so both can be
measured from `dataset/` alone, without the site.

`scripts/check_compass.py` reads the published positions and evidence, merges contestants resting on
identical evidence so each measurement is counted once (as it is on the site), resolves every stance
with the rule above, and checks four things against its constants:

1. **Mean stance per party.** The average of a party's stances must stay within +/-0.3 (`MAX_MEAN`).
   A party's mean is its answer to "how many of these positions are worded my way", so holding it
   near zero is what stops saying yes to everything from favouring a side.
2. **Spread between parties.** Each party is also scored as if the visitor answered +2 on every
   position, and as if they answered -2 on every position. Under either, the gap between the highest
   and the lowest party may not exceed 10 points (`MAX_SPREAD`).
3. **Every position separates someone.** At least two contestants must hold a stance on it, and they
   must not all hold the same one. A position everyone agrees on answers nothing.
4. **Coverage and topics.** A party must hold a stance on at least half the positions
   (`MIN_COVERAGE` = 0.5), so its figure rests on enough of them to mean something, and every
   position must have a topic.

The simulated answers carry no importance marks: weighting is a visitor's choice, so the check scores
every position at weight 1.

Enforcement is per country and deliberately partial. Countries listed in
`dataset/compass_reviewed.txt` have had their positions reviewed for this and must pass: CI runs
`python3 scripts/check_compass.py dataset`, and a failure in a reviewed country exits non-zero.
Every other country is still measured and reported in full, with its failures printed as notes, and
the run passes. The random draws are seeded, so a report reproduces run to run.
`python3 scripts/check_compass.py --selftest` checks the measurement itself on invented positions,
with no dataset involved.

## 5. What is reported but not enforced

**Share of random voters.** For 20000 simulated voters (`VOTERS`) who answer every position at
random, the report gives each party the share of voters for whom it comes first, with a tie split
evenly between the leaders. It runs twice: once over all five scale values, once over the moderate
three (-1, 0, 1). This is reported and never enforced. A party in the middle attracts a larger share
because a moderate voter is closer to the middle, and that is a real fact about the positions, not an
artifact to flatten. A threshold here would move the numbers away from what the recorded positions
say, which is falsifying the result rather than balancing it.

**Recall.** This measures whether the compass works rather than where a party sits. For each party,
2000 simulated voters hold that party's positions: they answer its stance on each position, a random
30% of those answers are shifted one step in a random direction (clamped to the ends of the scale),
and where the party holds no stance the answer is random. The report gives the share of those voters
for whom the compass puts that party first, computed both for exact positions and for the shifted
ones. A voter who holds a party's positions should find that party first; a low figure says the
measurement is not working, not that the party is unpopular.

**Nearest party.** For each party's own simulated voters, the report also names the party that most
often ranks second. Alongside the percentages it says which contestants are hard to tell apart on
this set of positions, which is worth seeing rather than smoothing over.

## 6. What the compass does not tell you

- It measures agreement with recorded positions on a chosen set of questions. It does not say which
  party is right, and it is neither a recommendation nor a prediction. The data is a record of what
  documents say.
- A missing stance is a gap in the sources, not a neutral position. Coverage is uneven by nature:
  contestants publish different amounts, and documents are silent on whole topics. A contestant with
  nothing recorded on a position you answered is left off the list. The count beside every figure
  says how many positions it rests on, so a thin reading is visible as one.
- The choice of questions matters. The checks in section 4 establish that a set is not leaning by
  construction; they cannot establish that this set is the right set. Another set of positions would
  give another ranking, and that is a fair objection to any compass.
- A stance is our reading of documents, not a statement by a party or a person. Every stance links to
  its source and gives its locator so the reading can be checked, and a document that does not
  address a position leaves it unrecorded rather than guessed at.
- Nothing about your answers is kept, so the compass cannot tell you how other visitors answered.

## 7. How to challenge a stance or a question

A stance is a claim about documents, so a challenge is a source question rather than an opinion.

- A wrong fact on the site: open a data-correction issue
  ([.github/ISSUE_TEMPLATE/data_correction.yml](../.github/ISSUE_TEMPLATE/data_correction.yml))
  with a source at least as trustworthy as the one we cite.
- A stance, a position, or the evidence behind either: change the published data in a pull request.
  [dataset/README.md](../dataset/README.md) states the rules the checkers enforce: add the source row
  first, reference other rows by slug, keep the quotation exactly as the document words it and put
  the page, article or number in `locator`. Then run `python3 scripts/validate_data.py dataset` and
  `python3 scripts/check_compass.py dataset`, and load the result into a scratch database with
  `cargo run -p ingest -- import dataset`.
- A new position has to clear the balance checks in section 4 for its country, and it has to ask one
  thing: two propositions in one sentence cannot be answered.
- Rewording a position so that it says the opposite must reverse every stance on it in the same
  change, or every reading silently flips. Only a true opposite may be reversed this way ("kept"
  against "abolished"), never a looser or a different proposition. A reword that keeps the
  direction leaves the stances as they are.
