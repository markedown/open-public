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
    let country = db::country::get_by_slug(&pool, &country)
        .await?
        .ok_or(PageError::NotFound)?;
    let theses = db::compass::theses_for_country(&pool, country.id).await?;
    let action = format!("/{}/compass", country.slug);

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

            @if theses.is_empty() {
                p class="py-12 text-center text-sm text-ink-muted" {
                    (i18n::t("No positions have been recorded for this country yet."))
                }
            } @else {
                p class="mb-8 max-w-prose text-sm text-ink-muted" {
                    (i18n::t("Say where you stand on each position. Mark the ones you care about as important so they count for more. Nothing you enter is stored: your match is worked out on the spot and never saved."))
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

/// POST `/{country}/compass`: score the submitted answers and render the match.
/// The body is read as raw key/value pairs because the field set is dynamic (one
/// group per position). Answers are used only to compute the result here.
pub async fn result(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path(country): Path<String>,
    Form(pairs): Form<Vec<(String, String)>>,
) -> Result<Markup, PageError> {
    let country = db::country::get_by_slug(&pool, &country)
        .await?
        .ok_or(PageError::NotFound)?;
    let theses = db::compass::theses_for_country(&pool, country.id).await?;
    let positions = db::compass::positions_for_country(&pool, country.id).await?;
    let parties = db::parties::list(&pool, country.id).await?;
    let by_id: HashMap<i64, &Party> = parties.iter().map(|p| (p.id, p)).collect();

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
            party_id: p.party_id,
            value: p.stance as i8,
        })
        .collect();
    let scores = compass::score(&answers, &stances);

    let compass_url = format!("/{}/compass", country.slug);

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
                        (i18n::t("No party has recorded a stance on the positions you answered."))
                    }
                } @else {
                    ol class="space-y-2.5" {
                        @for (rank, s) in scores.iter().enumerate() {
                            @if let Some(p) = by_id.get(&s.party_id) {
                                (score_row(rank + 1, s, p, &country.slug))
                            }
                        }
                    }
                    p class="mt-5 max-w-prose text-xs text-ink-muted" {
                        (i18n::t("Parties are ranked by how closely their recorded stances match your answers. Every stance is sourced on the party's page."))
                    }

                    (comparison(&theses, &positions, &answered, &by_id))
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
fn score_row(rank: usize, s: &compass::PartyScore, party: &Party, country: &str) -> Markup {
    let label = party.short_name.as_deref().unwrap_or(&party.name);
    let pct = format!("{:.0}%", s.percent);
    let width = format!("width:{:.1}%", s.percent);
    html! {
        li class="op-card flex items-center gap-3 px-4 py-3" {
            span class="w-5 shrink-0 text-center font-mono text-xs text-ink-muted" { (rank) }
            (ui::badge::party_chip(label, party.color.as_deref()))
            div class="min-w-0 flex-1" {
                a href={"/" (country) "/parties/" (party.slug)}
                  class="text-sm font-medium text-ink hover:text-accent hover:underline" { (party.name) }
                div class="mt-1.5 h-2 overflow-hidden rounded-full bg-paper-sunken" {
                    div class="h-full rounded-full"
                      style={(width) ";background-color:" (party.color.as_deref().unwrap_or("#33527a"))} {}
                }
            }
            span class="shrink-0 font-mono text-sm font-semibold tabular-nums text-ink" { (pct) }
        }
    }
}

/// The per-position breakdown: for each answered position, the visitor's answer
/// and every party's recorded stance on it, so the ranking above is inspectable.
fn comparison(
    theses: &[db::compass::Thesis],
    positions: &[db::compass::Position],
    answered: &HashMap<i64, i8>,
    by_id: &HashMap<i64, &Party>,
) -> Markup {
    html! {
        section class="mt-10" {
            (ui::section_header(i18n::t("How the parties compare"), None))
            div class="space-y-4" {
                @for t in theses {
                    @if let Some(&mine) = answered.get(&t.id) {
                        div class="op-card px-4 py-3.5" {
                            p class="text-sm font-medium text-ink" { (t.text) }
                            p class="mt-1 text-xs text-ink-muted" {
                                (i18n::t("Your answer")) ": "
                                span class="font-semibold text-ink" { (value_label(mine)) }
                            }
                            div class="mt-2.5 flex flex-wrap gap-1.5" {
                                @for p in positions.iter().filter(|p| p.thesis_id == t.id) {
                                    @if let Some(party) = by_id.get(&p.party_id) {
                                        span title={(party.name) ": " (value_label(p.stance as i8))}
                                          class="inline-flex items-center gap-1.5 rounded-md border border-hairline px-1.5 py-1" {
                                            (ui::badge::party_chip(
                                                party.short_name.as_deref().unwrap_or(&party.name),
                                                party.color.as_deref(),
                                            ))
                                            span class="font-mono text-[10px] text-ink-muted" { (value_label(p.stance as i8)) }
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
