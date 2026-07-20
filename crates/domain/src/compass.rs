//! Pure scoring for the preference-match compass.
//!
//! No storage and no I/O: given a visitor's answers and the parties' recorded
//! stances, compute how much each party agrees with the visitor. Answers are
//! never persisted; the whole match is derived here from values passed in by
//! the page layer, so the compass stays stateless and anonymous.
//!
//! Answers and stances share one five-point scale, `-2` (strongly disagree) to
//! `+2` (strongly agree). A thesis a visitor skips is simply left out of the
//! `answers` slice.

use std::collections::BTreeMap;

/// A visitor's answer to one thesis. `important` doubles the thesis's weight in
/// the match, so the positions a visitor cares about count for more.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Answer {
    pub thesis_id: i64,
    pub value: i8,
    pub important: bool,
}

/// A party's recorded stance on one thesis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stance {
    pub thesis_id: i64,
    pub party_id: i64,
    pub value: i8,
}

/// A party's computed match over the answered theses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PartyScore {
    pub party_id: i64,
    /// Agreement in `0.0..=100.0`. The exact value drives the ranking; the page
    /// rounds it for display.
    pub percent: f64,
    /// How many theses actually counted: answered by the visitor and with a
    /// recorded stance for this party.
    pub matched: u32,
}

/// The most a single thesis contributes before weighting. An identical answer
/// and stance score the full `4`; opposite extremes (`-2` vs `+2`) score `0`.
const MAX_POINTS: i32 = 4;

/// Points for one thesis: `4` minus the distance between answer and stance, so
/// the closer the two positions, the more they contribute. Always in `0..=4`
/// for in-range values.
pub fn agreement_points(answer: i8, stance: i8) -> i32 {
    MAX_POINTS - (i32::from(answer) - i32::from(stance)).abs()
}

/// Score every party that has at least one stance on a thesis the visitor
/// answered.
///
/// Each such thesis contributes `agreement_points(answer, stance) * weight` out
/// of `4 * weight`, where `weight` is `2` for an answer flagged important and
/// `1` otherwise. A party's percentage is its earned points over its possible
/// points. Parties are returned ranked by percentage descending, ties broken by
/// `party_id` ascending so the order is stable and reproducible.
///
/// Parties with no stance on any answered thesis are omitted: there is nothing
/// to compare, so reporting `0%` would be misleading rather than informative.
pub fn score(answers: &[Answer], stances: &[Stance]) -> Vec<PartyScore> {
    // thesis_id -> (answer value, weight). A BTreeMap keeps the walk
    // deterministic, which keeps the accumulation order stable.
    let answered: BTreeMap<i64, (i8, i32)> = answers
        .iter()
        .map(|a| (a.thesis_id, (a.value, if a.important { 2 } else { 1 })))
        .collect();

    // party_id -> (earned points, possible points, matched theses).
    let mut acc: BTreeMap<i64, (i32, i32, u32)> = BTreeMap::new();
    for s in stances {
        if let Some(&(value, weight)) = answered.get(&s.thesis_id) {
            let entry = acc.entry(s.party_id).or_insert((0, 0, 0));
            entry.0 += agreement_points(value, s.value) * weight;
            entry.1 += MAX_POINTS * weight;
            entry.2 += 1;
        }
    }

    let mut scores: Vec<PartyScore> = acc
        .into_iter()
        .map(|(party_id, (earned, possible, matched))| PartyScore {
            party_id,
            percent: if possible == 0 {
                0.0
            } else {
                f64::from(earned) / f64::from(possible) * 100.0
            },
            matched,
        })
        .collect();

    scores.sort_by(|a, b| {
        b.percent
            .partial_cmp(&a.percent)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.party_id.cmp(&b.party_id))
    });
    scores
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(thesis_id: i64, value: i8) -> Answer {
        Answer {
            thesis_id,
            value,
            important: false,
        }
    }

    #[test]
    fn agreement_points_reward_closeness() {
        assert_eq!(agreement_points(2, 2), 4); // identical
        assert_eq!(agreement_points(2, 1), 3);
        assert_eq!(agreement_points(0, 0), 4);
        assert_eq!(agreement_points(-2, 2), 0); // opposite extremes
        assert_eq!(agreement_points(1, -1), 2);
        // Distance is symmetric.
        assert_eq!(agreement_points(-1, 1), agreement_points(1, -1));
    }

    #[test]
    fn perfect_agreement_is_100_percent() {
        let answers = [answer(1, 2), answer(2, -2)];
        let stances = [
            Stance {
                thesis_id: 1,
                party_id: 10,
                value: 2,
            },
            Stance {
                thesis_id: 2,
                party_id: 10,
                value: -2,
            },
        ];
        let scores = score(&answers, &stances);
        assert_eq!(scores.len(), 1);
        assert_eq!(scores[0].party_id, 10);
        assert_eq!(scores[0].percent, 100.0);
        assert_eq!(scores[0].matched, 2);
    }

    #[test]
    fn total_disagreement_is_0_percent() {
        let answers = [answer(1, 2)];
        let stances = [Stance {
            thesis_id: 1,
            party_id: 10,
            value: -2,
        }];
        let scores = score(&answers, &stances);
        assert_eq!(scores[0].percent, 0.0);
        assert_eq!(scores[0].matched, 1);
    }

    #[test]
    fn parties_are_ranked_by_percentage_descending() {
        let answers = [answer(1, 2), answer(2, 2)];
        let stances = [
            // Party 10 agrees fully, party 20 agrees on one, party 30 opposes.
            Stance {
                thesis_id: 1,
                party_id: 10,
                value: 2,
            },
            Stance {
                thesis_id: 2,
                party_id: 10,
                value: 2,
            },
            Stance {
                thesis_id: 1,
                party_id: 20,
                value: 2,
            },
            Stance {
                thesis_id: 2,
                party_id: 20,
                value: -2,
            },
            Stance {
                thesis_id: 1,
                party_id: 30,
                value: -2,
            },
            Stance {
                thesis_id: 2,
                party_id: 30,
                value: -2,
            },
        ];
        let scores = score(&answers, &stances);
        assert_eq!(
            scores.iter().map(|s| s.party_id).collect::<Vec<_>>(),
            vec![10, 20, 30]
        );
        assert!(scores[0].percent > scores[1].percent);
        assert!(scores[1].percent > scores[2].percent);
    }

    #[test]
    fn important_answers_weigh_double() {
        // A party that agrees on the important thesis and disagrees on the
        // unimportant one should outrank a party with the reverse profile.
        let answers = [
            Answer {
                thesis_id: 1,
                value: 2,
                important: true,
            },
            Answer {
                thesis_id: 2,
                value: 2,
                important: false,
            },
        ];
        let stances = [
            Stance {
                thesis_id: 1,
                party_id: 10,
                value: 2,
            }, // agrees on important
            Stance {
                thesis_id: 2,
                party_id: 10,
                value: -2,
            },
            Stance {
                thesis_id: 1,
                party_id: 20,
                value: -2,
            }, // disagrees on important
            Stance {
                thesis_id: 2,
                party_id: 20,
                value: 2,
            },
        ];
        let scores = score(&answers, &stances);
        assert_eq!(scores[0].party_id, 10);
        assert!(scores[0].percent > 50.0);
        assert!(scores[1].percent < 50.0);
    }

    #[test]
    fn skipped_theses_do_not_count() {
        // The visitor answered only thesis 1; thesis 2 is skipped (absent), so
        // the party's stance on thesis 2 is ignored entirely.
        let answers = [answer(1, 2)];
        let stances = [
            Stance {
                thesis_id: 1,
                party_id: 10,
                value: 2,
            },
            Stance {
                thesis_id: 2,
                party_id: 10,
                value: -2,
            },
        ];
        let scores = score(&answers, &stances);
        assert_eq!(scores[0].matched, 1);
        assert_eq!(scores[0].percent, 100.0);
    }

    #[test]
    fn a_party_without_stances_on_answered_theses_is_omitted() {
        let answers = [answer(1, 2)];
        let stances = [
            Stance {
                thesis_id: 1,
                party_id: 10,
                value: 1,
            },
            // Party 20 only has a stance on an unanswered thesis.
            Stance {
                thesis_id: 2,
                party_id: 20,
                value: 2,
            },
        ];
        let scores = score(&answers, &stances);
        assert_eq!(scores.len(), 1);
        assert_eq!(scores[0].party_id, 10);
    }

    #[test]
    fn no_answers_yields_no_scores() {
        let stances = [Stance {
            thesis_id: 1,
            party_id: 10,
            value: 2,
        }];
        assert!(score(&[], &stances).is_empty());
    }

    #[test]
    fn ties_break_by_party_id_ascending() {
        let answers = [answer(1, 0)];
        let stances = [
            Stance {
                thesis_id: 1,
                party_id: 30,
                value: 0,
            },
            Stance {
                thesis_id: 1,
                party_id: 10,
                value: 0,
            },
            Stance {
                thesis_id: 1,
                party_id: 20,
                value: 0,
            },
        ];
        let scores = score(&answers, &stances);
        assert_eq!(
            scores.iter().map(|s| s.party_id).collect::<Vec<_>>(),
            vec![10, 20, 30]
        );
    }
}
