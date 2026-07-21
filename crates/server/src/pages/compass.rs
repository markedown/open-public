//! The preference-match compass: a stateless, anonymous questionnaire.
//!
//! A visitor answers where they stand on each of a country's sourced policy
//! positions (theses); on submit the page scores every party by how closely its
//! recorded stances match, and ranks them. Nothing the visitor enters is stored
//! or logged: the answers arrive in the POST body, drive the score in memory,
//! and are gone when the response is rendered. The whole match is computed by
//! the pure [`domain::compass`] scorer.

use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::Form;
use maud::{html, Markup};

use crate::auth::AuthSession;
use crate::error::PageError;
use crate::i18n;
use crate::ui::{self, breadcrumb::Crumb};

use domain::compass::{self, Answer, Stance};
use domain::models::Party;

/// A contestant in the compass: a party in a parliamentary thesis set, a person
/// in a presidential one. The page renders both the same way, so the scoring and
/// the breakdown do not care which kind an election has.
struct Contestant {
    name: String,
    short: String,
    colour: Option<String>,
    href: String,
}

impl Contestant {
    fn from_party(p: &Party, country: &str) -> Self {
        Self {
            name: p.name.clone(),
            short: p.short_name.clone().unwrap_or_else(|| p.name.clone()),
            colour: p.color.clone(),
            href: format!("/{}/parties/{}", country, p.slug),
        }
    }
    fn from_person(p: &db::compass::PersonContestant, country: &str) -> Self {
        Self {
            name: p.full_name.clone(),
            short: crate::ui::initials(&p.full_name),
            // A person carries no organisation colour, and inventing one would
            // make colour stop meaning "this party's colour".
            colour: None,
            href: format!("/{}/people/{}", country, p.slug),
        }
    }
}

/// Load every contestant of the given scope, keyed by id.
async fn contestants(
    pool: &db::Pool,
    country_id: i64,
    country_slug: &str,
    scope: &str,
) -> Result<HashMap<i64, Contestant>, PageError> {
    let mut out = HashMap::new();
    if scope == db::compass::SCOPE_PERSON {
        for p in db::compass::person_contestants(pool, country_id).await? {
            out.insert(p.id, Contestant::from_person(&p, country_slug));
        }
    } else {
        for p in db::parties::list(pool, country_id).await? {
            out.insert(p.id, Contestant::from_party(&p, country_slug));
        }
    }
    Ok(out)
}

/// The five-point answer scale, from strongly disagree to strongly agree, as
/// `(value, label)` pairs. Answers and party stances share this scale.
fn scale() -> [(i8, &'static str); 5] {
    [
        (-2, i18n::t("Strongly disagree")),
        (-1, i18n::t("Disagree")),
        (0, i18n::t("Neutral")),
        (1, i18n::t("Agree")),
        (2, i18n::t("Strongly agree")),
    ]
}

/// How the party stances were arrived at, stated plainly wherever they are
/// shown. They are our readings of each party's published programme, not
/// statements by the parties, and every one links to its source so a reader can
/// check it. If a party states its own position, that becomes the stance's
/// source instead, so this claim never overstates what is behind the data.
fn methodology_note(scope: &str) -> Markup {
    html! {
        p class="mt-4 max-w-prose text-xs text-ink-muted" {
            @if scope == db::compass::SCOPE_PERSON {
                (i18n::t("Candidate stances here are our readings of what each candidate published or did in office, not statements by the candidates themselves. Every stance links to its source, so any of them can be checked, and corrected if it is wrong."))
            } @else {
                (i18n::t("Party stances here are our readings of what each party published and how it voted, not statements by the parties themselves. Every stance links to its source, so any of them can be checked, and corrected if it is wrong."))
            }
        }
    }
}

/// The label for a stance or answer value on the shared scale.
fn value_label(value: i8) -> &'static str {
    scale()
        .into_iter()
        .find(|(v, _)| *v == value)
        .map(|(_, label)| label)
        .unwrap_or("")
}

/// GET `/{country}/compass`: the questionnaire as a plain form. It works with no
/// JavaScript: every position is a radio group defaulting to "skip", so a
/// visitor answers only what they want and submits a normal POST.
pub async fn form(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path(country): Path<String>,
) -> Result<Markup, PageError> {
    form_scoped(pool, session, country, db::compass::SCOPE_PARTY.to_string()).await
}

/// GET `/{country}/compass/{scope}`: the same questionnaire for a scope other
/// than the default. A presidential compass ranks people, so its thesis set is
/// separate from the parliamentary one.
pub async fn form_for_scope(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path((country, scope)): Path<(String, String)>,
) -> Result<Markup, PageError> {
    form_scoped(pool, session, country, scope).await
}

async fn form_scoped(
    pool: db::Pool,
    session: Option<AuthSession>,
    country: String,
    scope: String,
) -> Result<Markup, PageError> {
    if scope != db::compass::SCOPE_PARTY && scope != db::compass::SCOPE_PERSON {
        return Err(PageError::NotFound);
    }
    let country = db::country::get_by_slug(&pool, &country)
        .await?
        .ok_or(PageError::NotFound)?;
    let theses = db::compass::theses_for_country(&pool, country.id, &scope).await?;
    let action = if scope == db::compass::SCOPE_PARTY {
        format!("/{}/compass", country.slug)
    } else {
        format!("/{}/compass/{}", country.slug, scope)
    };
    // A country can ask both kinds of question: one set about parties, one
    // about the people contesting a presidential election. Offer the other set
    // only when it has questions in it.
    let other = if scope == db::compass::SCOPE_PARTY {
        db::compass::SCOPE_PERSON
    } else {
        db::compass::SCOPE_PARTY
    };
    let other_link = (db::compass::count_theses(&pool, country.id, other).await? > 0).then(|| {
        let href = if other == db::compass::SCOPE_PARTY {
            format!("/{}/compass", country.slug)
        } else {
            format!("/{}/compass/{}", country.slug, other)
        };
        let label = if other == db::compass::SCOPE_PARTY {
            i18n::t("Parties")
        } else {
            i18n::t("Candidates")
        };
        (href, label)
    });

    let content = html! {
        section class="mx-auto max-w-3xl" {
            (ui::breadcrumb::breadcrumbs(&[
                Crumb { label: country.name.clone(), href: Some(format!("/{}", country.slug)) },
                Crumb { label: i18n::t("Compass").to_string(), href: None },
            ]))
            (ui::page_header(
                i18n::t("Compass"),
                Some(html! { span class="font-mono" { (theses.len()) } " " (i18n::t("positions")) }),
                None,
            ))

            @if let Some((href, label)) = &other_link {
                p class="-mt-3 mb-6 text-[13px] text-ink-muted" {
                    (i18n::t("Answered about")) ": "
                    @if scope == db::compass::SCOPE_PERSON {
                        span class="font-medium text-ink" { (i18n::t("Candidates")) }
                    } @else {
                        span class="font-medium text-ink" { (i18n::t("Parties")) }
                    }
                    " · "
                    a href=(href) class="text-accent hover:underline" { (label) }
                }
            }

            @if theses.is_empty() {
                p class="py-12 text-center text-sm text-ink-muted" {
                    (i18n::t("No positions have been recorded for this country yet."))
                }
            } @else {
                div class="mb-8" {
                    p class="max-w-prose text-sm text-ink-muted" {
                        (i18n::t("Say where you stand on each position. Mark the ones you care about as important so they count for more. Nothing you enter is stored: your match is worked out on the spot and never saved."))
                    }
                    (methodology_note(&scope))
                }
                form method="post" action=(action) class="space-y-8" {
                    @for (i, t) in theses.iter().enumerate() {
                        (thesis_field(i + 1, t))
                    }
                    div class="sticky bottom-0 -mx-4 border-t border-hairline bg-paper/90 px-4 py-4 backdrop-blur-sm" {
                        button type="submit"
                          class="rounded-lg bg-accent px-5 py-2.5 text-sm font-semibold text-white shadow-sm transition-colors hover:bg-accent-strong" {
                            (i18n::t("See my match"))
                        }
                    }
                }
            }
        }
    };

    Ok(ui::layout::document(
        Some(i18n::t("Compass")),
        session.is_some(),
        session.as_ref().is_some_and(|s| s.is_admin),
        content,
    ))
}

/// One position in the questionnaire: its text, a topic chip when set, the
/// five-point radio group (name `a{id}`, defaulting to a checked "skip"), and an
/// "important" checkbox (name `w{id}`).
fn thesis_field(number: usize, t: &db::compass::Thesis) -> Markup {
    let name = format!("a{}", t.id);
    html! {
        fieldset class="op-card px-4 py-4 sm:px-5" {
            legend class="sr-only" { (t.text) }
            div class="mb-3 flex items-start gap-3" {
                span class="mt-0.5 font-mono text-xs font-semibold text-ink-muted" { (number) }
                div class="min-w-0" {
                    p class="text-sm font-medium text-ink" { (t.text) }
                    @if let Some(topic) = &t.topic {
                        span class="mt-1 inline-block font-mono text-[10px] uppercase tracking-wide text-ink-muted" { (topic) }
                    }
                }
            }
            div class="flex flex-wrap gap-2" {
                // Skip is the default so an unanswered position is simply left
                // out of the score.
                label class="inline-flex cursor-pointer items-center gap-1.5 rounded-md border border-hairline px-2.5 py-1.5 text-xs text-ink-muted has-[:checked]:border-accent has-[:checked]:bg-accent-tint has-[:checked]:text-accent" {
                    input type="radio" name=(name) value="skip" checked class="sr-only";
                    (i18n::t("Skip"))
                }
                @for (value, label) in scale() {
                    label class="inline-flex cursor-pointer items-center gap-1.5 rounded-md border border-hairline px-2.5 py-1.5 text-xs text-ink has-[:checked]:border-accent has-[:checked]:bg-accent-tint has-[:checked]:font-semibold has-[:checked]:text-accent" {
                        input type="radio" name=(name) value=(value) class="sr-only";
                        (label)
                    }
                }
                label class="ml-auto inline-flex cursor-pointer items-center gap-1.5 rounded-md px-2.5 py-1.5 text-xs text-ink-muted has-[:checked]:text-accent" {
                    input type="checkbox" name={"w" (t.id)} value="1"
                      class="h-3.5 w-3.5 rounded border-hairline text-accent";
                    (i18n::t("Important"))
                }
            }
        }
    }
}

/// A short label for what a piece of evidence is, so a reader can tell a
/// promise from a recorded act at a glance.
fn kind_label(kind: &str) -> &'static str {
    match kind {
        "bill" => i18n::t("Bill"),
        "court" => i18n::t("Court application"),
        "vote" => i18n::t("Vote"),
        "law" => i18n::t("Law"),
        "decree" => i18n::t("Decree"),
        "alliance" => i18n::t("Alliance"),
        "statement" => i18n::t("Statement"),
        _ => i18n::t("Manifesto"),
    }
}

/// POST `/{country}/compass`: score the submitted answers and render the match.
/// The body is read as raw key/value pairs because the field set is dynamic (one
/// group per position). Answers are used only to compute the result here.
pub async fn result(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path(country): Path<String>,
    Form(pairs): Form<Vec<(String, String)>>,
) -> Result<Markup, PageError> {
    result_scoped(
        pool,
        session,
        country,
        db::compass::SCOPE_PARTY.to_string(),
        pairs,
    )
    .await
}

/// POST `/{country}/compass/{scope}`: score a non-default scope.
pub async fn result_for_scope(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path((country, scope)): Path<(String, String)>,
    Form(pairs): Form<Vec<(String, String)>>,
) -> Result<Markup, PageError> {
    result_scoped(pool, session, country, scope, pairs).await
}

async fn result_scoped(
    pool: db::Pool,
    session: Option<AuthSession>,
    country: String,
    scope: String,
    pairs: Vec<(String, String)>,
) -> Result<Markup, PageError> {
    if scope != db::compass::SCOPE_PARTY && scope != db::compass::SCOPE_PERSON {
        return Err(PageError::NotFound);
    }
    let country = db::country::get_by_slug(&pool, &country)
        .await?
        .ok_or(PageError::NotFound)?;
    let theses = db::compass::theses_for_country(&pool, country.id, &scope).await?;
    let positions = db::compass::positions_for_country(&pool, country.id, &scope).await?;
    let evidence = db::compass::evidence_for_country(&pool, country.id, &scope).await?;
    let by_id = contestants(&pool, country.id, &country.slug, &scope).await?;

    let fields: HashMap<String, String> = pairs.into_iter().collect();

    // Read one answer per position: an `a{id}` field holding a value in -2..=2,
    // with "skip" (or anything else) meaning unanswered. `w{id}` marks it
    // important. Values from the fixed radio set are trusted only after parsing.
    let mut answers: Vec<Answer> = Vec::new();
    for t in &theses {
        if let Some(value) = fields
            .get(&format!("a{}", t.id))
            .and_then(|v| v.parse::<i8>().ok())
            .filter(|v| (-2..=2).contains(v))
        {
            answers.push(Answer {
                thesis_id: t.id,
                value,
                important: fields.contains_key(&format!("w{}", t.id)),
            });
        }
    }

    let answered: HashMap<i64, i8> = answers.iter().map(|a| (a.thesis_id, a.value)).collect();
    let stances: Vec<Stance> = positions
        .iter()
        .map(|p| Stance {
            thesis_id: p.thesis_id,
            contestant_id: p.contestant_id,
            value: p.stance as i8,
        })
        .collect();
    let scores = compass::score(&answers, &stances);

    let compass_url = if scope == db::compass::SCOPE_PARTY {
        format!("/{}/compass", country.slug)
    } else {
        format!("/{}/compass/{}", country.slug, scope)
    };

    let content = html! {
        section class="mx-auto max-w-3xl" {
            (ui::breadcrumb::breadcrumbs(&[
                Crumb { label: country.name.clone(), href: Some(format!("/{}", country.slug)) },
                Crumb { label: i18n::t("Compass").to_string(), href: Some(compass_url.clone()) },
                Crumb { label: i18n::t("Your match").to_string(), href: None },
            ]))
            (ui::page_header(i18n::t("Your match"), None, None))

            @if answers.is_empty() {
                p class="py-8 text-sm text-ink-muted" {
                    (i18n::t("You did not answer any positions, so there is nothing to match.")) " "
                    a href=(compass_url) class="text-accent hover:underline" { (i18n::t("Go back")) }
                }
            } @else {
                p class="mb-6 text-sm text-ink-muted" {
                    (i18n::t("Positions answered")) ": "
                    span class="font-mono text-ink" { (answers.len()) " / " (theses.len()) }
                }

                @if scores.is_empty() {
                    p class="py-8 text-sm text-ink-muted" {
                        @if scope == db::compass::SCOPE_PERSON {
                            (i18n::t("No candidate has a recorded stance on the positions you answered."))
                        } @else {
                            (i18n::t("No party has recorded a stance on the positions you answered."))
                        }
                    }
                } @else {
                    ol class="space-y-2.5" {
                        @for (rank, s) in scores.iter().enumerate() {
                            @if let Some(p) = by_id.get(&s.contestant_id) {
                                (score_row(rank + 1, s, p))
                            }
                        }
                    }
                    p class="mt-5 max-w-prose text-xs text-ink-muted" {
                        @if scope == db::compass::SCOPE_PERSON {
                            (i18n::t("Candidates are ranked by how closely their recorded stances match your answers."))
                        } @else {
                            (i18n::t("Parties are ranked by how closely their recorded stances match your answers."))
                        }
                    }
                    (methodology_note(&scope))

                    (comparison(&theses, &positions, &evidence, &answered, &by_id, &scope))
                }

                div class="mt-8 border-t border-hairline pt-5" {
                    a href=(compass_url) class="text-sm font-semibold text-accent hover:underline" {
                        (i18n::t("Start over"))
                    }
                }
            }
        }
    };

    Ok(ui::layout::document(
        Some(i18n::t("Your match")),
        session.is_some(),
        session.as_ref().is_some_and(|s| s.is_admin),
        content,
    ))
}

/// One ranked party row: rank, chip, name, a match bar filled with the party's
/// colour (colour only ever appears inside data), and the percentage.
fn score_row(rank: usize, s: &compass::ContestantScore, c: &Contestant) -> Markup {
    let label = c.short.as_str();
    let pct = format!("{:.0}%", s.percent);
    let width = format!("width:{:.1}%", s.percent);
    html! {
        li class="op-card flex items-center gap-3 px-4 py-3" {
            span class="w-5 shrink-0 text-center font-mono text-xs text-ink-muted" { (rank) }
            (ui::badge::party_chip(label, c.colour.as_deref()))
            div class="min-w-0 flex-1" {
                a href=(c.href)
                  class="text-sm font-medium text-ink hover:text-accent hover:underline" { (c.name) }
                div class="mt-1.5 h-2 overflow-hidden rounded-full bg-paper-sunken" {
                    div class="h-full rounded-full"
                      style={(width) ";background-color:" (c.colour.as_deref().unwrap_or("#33527a"))} {}
                }
            }
            span class="shrink-0 text-right" {
                span class="block font-mono text-sm font-semibold tabular-nums text-ink" { (pct) }
                // How many positions the percentage actually rests on. Without
                // this a party scored on three positions looks as solid as one
                // scored on twenty.
                span class="block font-mono text-[10px] text-ink-muted" {
                    (s.matched) " " (i18n::t("positions"))
                }
            }
        }
    }
}

/// The per-position breakdown: for each answered position, the visitor's answer
/// and every party's stance, with the evidence each stance rests on. Where a
/// party's pledges and its recorded acts point opposite ways, both are shown and
/// the disagreement is marked rather than hidden.
fn comparison(
    theses: &[db::compass::Thesis],
    positions: &[db::compass::Position],
    evidence: &[db::compass::Evidence],
    answered: &HashMap<i64, i8>,
    by_id: &HashMap<i64, Contestant>,
    scope: &str,
) -> Markup {
    html! {
        section class="mt-10" {
            (ui::section_header(
                if scope == db::compass::SCOPE_PERSON {
                    i18n::t("How the candidates compare")
                } else {
                    i18n::t("How the parties compare")
                },
                None,
            ))
            div class="space-y-4" {
                @for t in theses {
                    @if let Some(&mine) = answered.get(&t.id) {
                        div class="op-card px-4 py-3.5" {
                            p class="text-sm font-medium text-ink" { (t.text) }
                            p class="mt-1 text-xs text-ink-muted" {
                                (i18n::t("Your answer")) ": "
                                span class="font-semibold text-ink" { (value_label(mine)) }
                            }
                            ul class="mt-2.5 space-y-2.5" {
                                @for p in positions.iter().filter(|p| p.thesis_id == t.id) {
                                    @if let Some(c) = by_id.get(&p.contestant_id) {
                                        li {
                                            (party_stance(t.id, p, c, evidence))
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One party's stance on one position: the resolved value, a marker when its
/// pledges and its record disagree, and every piece of evidence with its source.
fn party_stance(
    thesis_id: i64,
    position: &db::compass::Position,
    c: &Contestant,
    evidence: &[db::compass::Evidence],
) -> Markup {
    let items: Vec<&db::compass::Evidence> = evidence
        .iter()
        .filter(|e| e.thesis_id == thesis_id && e.contestant_id == position.contestant_id)
        .collect();
    // A party is in conflict with itself when its evidence points both ways.
    let diverges = items.iter().any(|e| e.stance > 0) && items.iter().any(|e| e.stance < 0);
    html! {
        div class="flex flex-wrap items-baseline gap-x-2 gap-y-1" {
            (ui::badge::party_chip(&c.short, c.colour.as_deref()))
            span class="font-mono text-[10px] uppercase tracking-wide text-ink-muted" {
                (value_label(position.stance as i8))
            }
            @if diverges {
                span class="rounded border border-hairline px-1.5 py-0.5 font-mono text-[10px] uppercase tracking-wide text-ink" {
                    (i18n::t("Pledge differs from record"))
                }
            }
        }
        ul class="mt-1 space-y-0.5 pl-1" {
            @for e in &items {
                li class="flex flex-wrap items-baseline gap-x-2 text-xs text-ink-muted" {
                    span class="font-mono text-[10px] uppercase tracking-wide text-ink" { (kind_label(&e.kind)) }
                    span { (value_label(e.stance as i8)) }
                    @if let Some(d) = e.occurred_on {
                        span class="font-mono text-[10px]" { (crate::fmt::date(Some(d))) }
                    }
                    @if let Some(q) = &e.quote { span { (q) } }
                    (ui::citation::source_marker(&e.source_url))
                }
            }
        }
    }
}
