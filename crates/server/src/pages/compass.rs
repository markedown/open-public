//! The preference-match compass: a stateless, anonymous questionnaire.
//!
//! A visitor answers where they stand on each of a country's sourced policy
//! positions (theses); on submit the page scores every party by how closely its
//! recorded stances match, and ranks them. Nothing the visitor enters is stored
//! or logged: the answers arrive in the POST body, drive the score in memory,
//! and are gone when the response is rendered. The whole match is computed by
//! the pure [`domain::compass`] scorer.

use std::collections::{BTreeMap, HashMap};

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
    /// Others that rest on exactly the same evidence, shown with this one as a
    /// single row: their name and colour.
    also: Vec<(String, Option<String>)>,
}

impl Contestant {
    fn from_party(p: &Party, country: &str) -> Self {
        Self {
            name: p.name.clone(),
            short: p.short_name.clone().unwrap_or_else(|| p.name.clone()),
            colour: p.color.clone(),
            href: format!("/{}/parties/{}", country, p.slug),
            also: Vec::new(),
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
            also: Vec::new(),
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
        p class="mt-4 max-w-prose text-[14px] text-ink-muted" {
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
                Crumb { label: i18n::t_dyn(&country.name).to_string(), href: Some(format!("/{}", country.slug)) },
                Crumb { label: i18n::t("Compass").to_string(), href: None },
            ]))
            (ui::page_header(
                i18n::t("Compass"),
                Some(html! { (theses.len()) " " (i18n::t("positions")) }),
                None,
            ))

            @if let Some((href, label)) = &other_link {
                p class="-mt-3 mb-6 text-[15px] text-ink-muted" {
                    (i18n::t("Answered about")) ": "
                    @if scope == db::compass::SCOPE_PERSON {
                        span class="font-medium text-ink" { (i18n::t("Candidates")) }
                    } @else {
                        span class="font-medium text-ink" { (i18n::t("Parties")) }
                    }
                    " · "
                    a href=(href) class="text-link hover:underline" { (label) }
                }
            }

            @if theses.is_empty() {
                p class="py-12 text-center text-[16px] text-ink-muted" {
                    (i18n::t("No positions have been recorded for this country yet."))
                }
            } @else {
                div class="mb-8" {
                    @if scope != db::compass::SCOPE_PERSON {
                        p class="mb-2 text-[21px] font-semibold text-ink" {
                            (i18n::t("Which party is closest to your views?"))
                        }
                    }
                    p class="max-w-prose text-[16px] text-ink-muted" {
                        (i18n::t("Nothing you enter is stored: your match is worked out on the spot and never saved."))
                    }
                    details class="mt-2 max-w-prose text-[15px] text-ink-muted" {
                        summary class="cursor-pointer font-semibold text-link" {
                            (i18n::t("How this works"))
                        }
                        p class="mt-2" {
                            (i18n::t("Say where you stand on each position. Mark the ones you care about as important so they count for more."))
                        }
                        (methodology_note(&scope))
                    }
                }
                form method="post" action=(action) class="space-y-8" {
                    ol class="compass-steps space-y-8" {
                        @for (i, t) in theses.iter().enumerate() {
                            (thesis_field(i + 1, theses.len(), t))
                        }
                    }
                    div {
                        button type="submit"
                          class="min-h-12 rounded-[10px] bg-ink px-6 text-[17px] font-semibold text-paper transition-colors hover:bg-accent-strong" {
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
/// One position: its text, the five-point radio group (name `a{id}`, defaulting
/// to a checked "skip"), an "important" checkbox (name `w{id}`), and links to
/// the neighbouring positions.
///
/// The links are why this works without JavaScript. They are ordinary anchors
/// to the next and previous card, so a browser that applies the stylesheet
/// shows one card at a time, and one that does not shows the whole list, which
/// is what this page always was.
fn thesis_field(number: usize, total: usize, t: &db::compass::Thesis) -> Markup {
    let name = format!("a{}", t.id);
    html! {
        li id={"q" (number)} class="compass-step list-none" {
        fieldset class="op-card px-5 py-5 sm:px-7 sm:py-6" {
            legend class="sr-only" { (t.text) }
            div class="flex items-center gap-3 text-[15px] text-ink-muted" {
                span class="shrink-0 font-semibold" { (number) "/" (total) }
                span aria-hidden="true" class="h-1.5 grow rounded-full bg-hairline-light" {
                    span class="block h-1.5 rounded-full bg-ink"
                        style={"width:" (number * 100 / total) "%"} {}
                }
            }
            p class="mt-4 text-[24px] font-bold leading-snug tracking-tight text-ink sm:text-[28px]" { (t.text) }
            @if let Some(topic) = &t.topic {
                span class="mt-1 inline-block text-[15px] text-ink-muted" { (i18n::t_dyn(topic)) }
            }
            div class="mt-5 grid gap-2 sm:grid-cols-5" {
                @for (value, label) in scale() {
                    label class="flex min-h-12 cursor-pointer items-center justify-center rounded-[10px] border border-hairline-strong px-3 py-2 text-center text-[16px] font-medium leading-tight text-ink transition-colors hover:border-ink has-[:focus-visible]:bg-focus has-[:focus-visible]:outline has-[:focus-visible]:outline-[3px] has-[:focus-visible]:outline-ink has-[:checked]:border-ink has-[:checked]:bg-ink has-[:checked]:text-paper" {
                        input type="radio" name=(name) value=(value) class="sr-only";
                        (label)
                    }
                }
            }
            div class="mt-4 flex flex-wrap items-center justify-between gap-x-4 gap-y-2" {
                label class="inline-flex min-h-11 cursor-pointer items-center gap-2 text-[16px] text-ink" {
                    input type="checkbox" name={"w" (t.id)} value="1" class="size-5 rounded accent-ink";
                    (i18n::t("Important"))
                }
                // Skip is the default so an unanswered position is simply left
                // out of the score.
                label class="inline-flex min-h-11 cursor-pointer items-center rounded-[10px] px-3 text-[16px] text-ink-muted hover:text-ink has-[:focus-visible]:bg-focus has-[:focus-visible]:text-ink has-[:checked]:font-semibold has-[:checked]:text-ink" {
                    input type="radio" name=(name) value="skip" checked class="sr-only";
                    (i18n::t("Skip"))
                }
            }
            div class="compass-nav mt-4 items-center justify-between gap-3 border-t border-hairline-light pt-3" {
                @if number > 1 {
                    a href={"#q" (number - 1)}
                      class="rounded-[10px] border border-hairline px-4 py-2 text-[15px] font-medium text-ink-muted transition-colors hover:border-ink hover:text-ink" {
                        (i18n::t("Back"))
                    }
                } @else {
                    span {}
                }
                @if number < total {
                    a href={"#q" (number + 1)}
                      class="rounded-[10px] bg-accent px-5 py-2 text-[15px] font-semibold text-white transition-colors hover:bg-accent-strong" {
                        (i18n::t("Next"))
                    }
                }
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
    let mut positions = db::compass::positions_for_country(&pool, country.id, &scope).await?;
    let mut evidence = db::compass::evidence_for_country(&pool, country.id, &scope).await?;
    let mut by_id = contestants(&pool, country.id, &country.slug, &scope).await?;

    // Contestants resting on exactly the same evidence are one measurement. They
    // are scored once and shown as one row, or they would count twice.
    let mut signatures: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    for e in &evidence {
        signatures.entry(e.contestant_id).or_default().push(format!(
            "{}|{}|{}|{}|{:?}",
            e.thesis_id, e.kind, e.stance, e.source_url, e.occurred_on
        ));
    }
    let stands_for = compass::merge_identical(&signatures);
    for (&id, &rep) in &stands_for {
        if id != rep {
            if let Some(gone) = by_id.remove(&id) {
                if let Some(kept) = by_id.get_mut(&rep) {
                    kept.name = format!("{} / {}", kept.name, gone.name);
                    kept.also.push((gone.short, gone.colour));
                }
            }
        }
    }
    positions.retain(|p| stands_for.get(&p.contestant_id) == Some(&p.contestant_id));
    evidence.retain(|e| stands_for.get(&e.contestant_id) == Some(&e.contestant_id));

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
    let by_topic = topic_scores(&theses, &answers, &stances);

    // Only a set of questions tied to recorded votes can be compared with one
    // member's record, and only then is the question worth asking.
    let member_stances = if scope == db::compass::SCOPE_PARTY && !answers.is_empty() {
        db::compass::member_stances(&pool, country.id).await?
    } else {
        Vec::new()
    };
    let members = if member_stances.is_empty() {
        None
    } else {
        Some(local_members(&pool, country.id, &fields, &answers, &member_stances).await?)
    };

    let compass_url = if scope == db::compass::SCOPE_PARTY {
        format!("/{}/compass", country.slug)
    } else {
        format!("/{}/compass/{}", country.slug, scope)
    };

    let content = html! {
        section class="mx-auto max-w-3xl" {
            (ui::breadcrumb::breadcrumbs(&[
                Crumb { label: i18n::t_dyn(&country.name).to_string(), href: Some(format!("/{}", country.slug)) },
                Crumb { label: i18n::t("Compass").to_string(), href: Some(compass_url.clone()) },
                Crumb { label: i18n::t("Your match").to_string(), href: None },
            ]))
            (ui::page_header(i18n::t("Your match"), None, None))

            @if answers.is_empty() {
                p class="py-8 text-[16px] text-ink-muted" {
                    (i18n::t("You did not answer any positions, so there is nothing to match.")) " "
                    a href=(compass_url) class="text-link hover:underline" { (i18n::t("Go back")) }
                }
            } @else {
                p class="mb-6 text-[16px] text-ink-muted" {
                    (i18n::t("Positions answered")) ": "
                    span class="text-ink" { (answers.len()) " / " (theses.len()) }
                }

                @if scores.is_empty() {
                    p class="py-8 text-[16px] text-ink-muted" {
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
                    p class="mt-5 max-w-prose text-[14px] text-ink-muted" {
                        @if scope == db::compass::SCOPE_PERSON {
                            (i18n::t("Candidates are ranked by how closely their recorded stances match your answers."))
                        } @else {
                            (i18n::t("Parties are ranked by how closely their recorded stances match your answers."))
                        }
                    }
                    (methodology_note(&scope))

                    (topic_section(&by_topic, &by_id))

                    @if let Some(m) = &members {
                        (members_section(m, &answers, &compass_url, &country.slug))
                    }

                    (comparison(&theses, &positions, &evidence, &answered, &by_id, &scope, &country.slug))
                }

                div class="mt-8 border-t border-hairline pt-5" {
                    a href=(compass_url) class="text-[16px] font-semibold text-link hover:underline" {
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

/// A contestant's chip, followed by those of anyone merged into its row.
fn chips(c: &Contestant) -> Markup {
    html! {
        span class="inline-flex flex-wrap gap-1" {
            (ui::badge::party_chip(&c.short, c.colour.as_deref()))
            @for (short, colour) in &c.also {
                (ui::badge::party_chip(short, colour.as_deref()))
            }
        }
    }
}

/// The match worked out separately for each topic the visitor answered on, in
/// the order the questions came. A visitor can be close to one party on migration
/// and another on energy, which a single overall figure hides.
fn topic_scores(
    theses: &[db::compass::Thesis],
    answers: &[Answer],
    stances: &[Stance],
) -> Vec<(String, Vec<compass::ContestantScore>)> {
    let mut topics: Vec<&str> = Vec::new();
    for t in theses {
        if let Some(topic) = t.topic.as_deref() {
            if !topics.contains(&topic) {
                topics.push(topic);
            }
        }
    }
    topics
        .into_iter()
        .filter_map(|topic| {
            let in_topic: Vec<Answer> = answers
                .iter()
                .filter(|a| {
                    theses
                        .iter()
                        .any(|t| t.id == a.thesis_id && t.topic.as_deref() == Some(topic))
                })
                .copied()
                .collect();
            let scores = compass::score(&in_topic, stances);
            (!scores.is_empty()).then(|| (topic.to_string(), scores))
        })
        .collect()
}

/// Who is closest on each topic. Parties level on the top figure all appear.
fn topic_section(
    by_topic: &[(String, Vec<compass::ContestantScore>)],
    by_id: &HashMap<i64, Contestant>,
) -> Markup {
    if by_topic.len() < 2 {
        return html! {};
    }
    html! {
        section class="mt-10" {
            (ui::section_header(i18n::t("Closest to you, by topic"), None))
            ul class="op-card divide-y divide-hairline-light px-4" {
                @for (topic, scores) in by_topic {
                    @let best = scores[0].percent;
                    li class="flex flex-wrap items-center gap-x-3 gap-y-1.5 py-3" {
                        span class="min-w-[12rem] flex-1 text-[16px] font-medium text-ink" { (i18n::t_dyn(topic)) }
                        span class="inline-flex flex-wrap gap-1.5" {
                            @for s in scores.iter().filter(|s| s.percent == best) {
                                @if let Some(c) = by_id.get(&s.contestant_id) {
                                    (chips(c))
                                }
                            }
                        }
                        span class="w-12 text-right text-[16px] font-semibold tabular-nums text-ink" {
                            (format!("{best:.0}%"))
                        }
                    }
                }
            }
        }
    }
}

/// What the visitor's place resolved to, and their members' match with them.
struct LocalMembers {
    query: String,
    /// Several places answer to what was typed, so the visitor picks one.
    choices: Vec<db::compass::Place>,
    place: Option<db::compass::Place>,
    direct: Vec<(db::compass::LocalMember, compass::ContestantScore)>,
    list: Vec<(db::compass::LocalMember, compass::ContestantScore)>,
    /// Members for the place with no vote that could be read against these answers.
    unread: usize,
    has_direct_seat: bool,
}

/// Resolve the place a visitor typed and score its members against their answers.
/// The place, like the answers, is used for this response and kept nowhere.
async fn local_members(
    pool: &db::Pool,
    country_id: i64,
    fields: &HashMap<String, String>,
    answers: &[Answer],
    stances: &[db::compass::MemberStance],
) -> Result<LocalMembers, PageError> {
    let query = fields
        .get("place")
        .map(|q| q.trim())
        .unwrap_or("")
        .to_string();
    let mut choices = Vec::new();
    let mut place = None;
    if let Some(id) = fields.get("place_id").and_then(|v| v.parse::<i64>().ok()) {
        place = db::compass::get_place(pool, country_id, id).await?;
    } else if !query.is_empty() {
        choices = db::compass::find_places(pool, country_id, &query).await?;
        if choices.len() == 1 {
            place = choices.pop();
        }
    }

    let mut out = LocalMembers {
        query,
        choices,
        place: None,
        direct: Vec::new(),
        list: Vec::new(),
        unread: 0,
        has_direct_seat: false,
    };
    let Some(place) = place else {
        return Ok(out);
    };
    let mut districts = vec![place.district.clone()];
    districts.extend(place.region.clone());
    let members = db::compass::members_for_districts(pool, country_id, &districts).await?;
    let ids: Vec<i64> = members.iter().map(|m| m.person_id).collect();
    let readings: Vec<Stance> = stances
        .iter()
        .filter(|s| ids.contains(&s.person_id))
        .map(|s| Stance {
            thesis_id: s.thesis_id,
            contestant_id: s.person_id,
            value: s.stance as i8,
        })
        .collect();
    let scores = compass::score(answers, &readings);
    out.has_direct_seat = members.iter().any(|m| m.district == place.district);
    out.unread = members.len() - scores.len();
    let mut by_person: HashMap<i64, db::compass::LocalMember> =
        members.into_iter().map(|m| (m.person_id, m)).collect();
    for s in scores {
        if let Some(m) = by_person.remove(&s.contestant_id) {
            if m.district == place.district {
                out.direct.push((m, s));
            } else {
                out.list.push((m, s));
            }
        }
    }
    // A figure resting on one or two votes is not as solid as the rest, so those
    // follow them whatever their percentage. The sort is stable within each group.
    out.direct.sort_by_key(|(_, s)| s.matched < SOLID_READINGS);
    out.list.sort_by_key(|(_, s)| s.matched < SOLID_READINGS);
    out.place = Some(place);
    Ok(out)
}

/// How many readings a member's figure needs before it ranks with the others.
const SOLID_READINGS: u32 = 3;

/// How many of a regional list's members are shown before the rest fold away.
const LIST_SHOWN: usize = 5;

/// The visitor's own members, compared on their votes. Nothing here is stored:
/// the form sends the answers back with the place, and they are gone with the
/// response, exactly as on the first submit.
fn members_section(
    m: &LocalMembers,
    answers: &[Answer],
    compass_url: &str,
    country: &str,
) -> Markup {
    let carried = html! {
        @for a in answers {
            input type="hidden" name={"a" (a.thesis_id)} value=(a.value);
            @if a.important {
                input type="hidden" name={"w" (a.thesis_id)} value="1";
            }
        }
    };
    html! {
        section class="mt-10" id="your-members" {
            (ui::section_header(i18n::t("How do your members vote?"), None))
            p class="mb-4 max-w-prose text-[16px] text-ink-muted" {
                (i18n::t("Compare your answers with how the members for your area actually voted. Your place is not stored."))
            }
            form method="post" action={(compass_url) "#your-members"} class="flex flex-wrap items-end gap-3" {
                (carried)
                label class="block min-w-[14rem] flex-1 text-[15px] font-medium text-ink" {
                    (i18n::t("Town or municipality"))
                    input type="text" name="place" value=(m.query) autocomplete="off"
                      class="mt-1 block min-h-12 w-full rounded-[10px] border border-hairline-strong bg-paper px-3 text-[17px] text-ink";
                }
                button type="submit"
                  class="min-h-12 rounded-[10px] border border-ink px-5 text-[16px] font-semibold text-ink transition-colors hover:bg-ink hover:text-paper" {
                    (i18n::t("Compare"))
                }
            }

            @if m.place.is_none() && !m.query.is_empty() {
                @if m.choices.is_empty() {
                    p class="mt-4 text-[16px] text-ink-muted" { (i18n::t("No place found.")) }
                } @else {
                    form method="post" action={(compass_url) "#your-members"} class="mt-4" {
                        (carried)
                        p class="mb-2 text-[16px] font-medium text-ink" { (i18n::t("Which one is yours?")) }
                        ul class="flex flex-wrap gap-2" {
                            @for c in &m.choices {
                                li {
                                    button type="submit" name="place_id" value=(c.id)
                                      class="min-h-11 rounded-[10px] border border-hairline-strong px-3 py-1.5 text-left text-[15px] text-ink transition-colors hover:border-ink" {
                                        (c.place)
                                        @if let Some(part) = &c.part { " (" (part) ")" }
                                        span class="block text-[14px] text-ink-muted" { (c.district) }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            @if let Some(place) = &m.place {
                p class="mt-5 text-[16px] font-semibold text-ink" {
                    (place.place)
                    @if let Some(part) = &place.part { " (" (part) ")" }
                    span class="font-normal text-ink-muted" { " · " (place.district) }
                }
                @if !m.direct.is_empty() {
                    h3 class="mt-4 mb-2 text-[17px] font-bold text-ink" { (i18n::t("Elected here")) }
                    ol class="space-y-2.5" {
                        @for (member, s) in &m.direct { (member_row(member, s, country)) }
                    }
                } @else if !m.has_direct_seat {
                    p class="mt-3 max-w-prose text-[16px] text-ink-muted" {
                        (i18n::t("No member elected in this constituency holds a seat at present."))
                    }
                }
                @if !m.list.is_empty() {
                    h3 class="mt-5 mb-2 text-[17px] font-bold text-ink" {
                        (i18n::t("On the regional list"))
                        @if let Some(region) = &place.region {
                            span class="font-normal text-ink-muted" { " · " (region) }
                        }
                    }
                    ol class="space-y-2.5" {
                        @for (member, s) in m.list.iter().take(LIST_SHOWN) { (member_row(member, s, country)) }
                    }
                    @if m.list.len() > LIST_SHOWN {
                        details class="mt-2.5" {
                            summary class="cursor-pointer text-[16px] font-semibold text-link" {
                                (i18n::t("Show all")) " (" (m.list.len()) ")"
                            }
                            ol class="mt-2.5 space-y-2.5" {
                                @for (member, s) in m.list.iter().skip(LIST_SHOWN) { (member_row(member, s, country)) }
                            }
                        }
                    }
                }
                p class="mt-4 max-w-prose text-[14px] text-ink-muted" {
                    (i18n::t("This rests only on recorded votes linked to the positions you answered. A member who was absent, or who followed their group in voting down an opposition motion, is not counted on that position."))
                    @if m.unread > 0 {
                        " " (m.unread) " " (i18n::t("further members have no vote we can read on your answers."))
                    }
                }
            }
        }
    }
}

/// One member: party chip, name linking to their votes, and the match.
fn member_row(
    member: &db::compass::LocalMember,
    s: &compass::ContestantScore,
    country: &str,
) -> Markup {
    html! {
        li class="op-card flex items-center gap-3 px-4 py-3" {
            @if let Some(short) = &member.party_short {
                (ui::badge::party_chip(short, member.party_color.as_deref()))
            }
            a href={"/" (country) "/people/" (member.slug) "/votes"}
              class="min-w-0 flex-1 text-[16px] font-medium text-ink hover:text-link hover:underline" {
                (member.full_name)
            }
            span class="shrink-0 text-right" {
                span class="block text-[16px] font-semibold tabular-nums text-ink" { (format!("{:.0}%", s.percent)) }
                span class="block text-[15px] text-ink-muted" { (s.matched) " " (i18n::t("positions")) }
            }
        }
    }
}

/// One ranked party row: rank, chip, name, a match bar filled with the party's
/// colour (colour only ever appears inside data), and the percentage.
fn score_row(rank: usize, s: &compass::ContestantScore, c: &Contestant) -> Markup {
    let pct = format!("{:.0}%", s.percent);
    let width = format!("width:{:.1}%", s.percent);
    html! {
        li class="op-card flex items-center gap-3 px-4 py-3" {
            span class="w-5 shrink-0 text-center text-[14px] text-ink-muted" { (rank) }
            (chips(c))
            div class="min-w-0 flex-1" {
                a href=(c.href)
                  class="text-[16px] font-medium text-ink hover:text-link hover:underline" { (i18n::t_dyn(&c.name)) }
                div class="mt-1.5 h-2 overflow-hidden rounded-full bg-paper-sunken" {
                    div class="h-full rounded-full"
                      style={(width) ";background-color:" (c.colour.as_deref().unwrap_or("#33527a"))} {}
                }
            }
            span class="shrink-0 text-right" {
                span class="block text-[16px] font-semibold tabular-nums text-ink" { (pct) }
                // How many positions the percentage actually rests on. Without
                // this a party scored on three positions looks as solid as one
                // scored on twenty.
                span class="block text-[15px] text-ink-muted" {
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
    country: &str,
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
                            p class="text-[16px] font-medium text-ink" { (t.text) }
                            p class="mt-1 text-[14px] text-ink-muted" {
                                (i18n::t("Your answer")) ": "
                                span class="font-semibold text-ink" { (value_label(mine)) }
                            }
                            ul class="mt-2.5 space-y-2.5" {
                                @for p in positions.iter().filter(|p| p.thesis_id == t.id) {
                                    @if let Some(c) = by_id.get(&p.contestant_id) {
                                        li {
                                            (party_stance(t.id, p, c, evidence, country))
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
    country: &str,
) -> Markup {
    let items: Vec<&db::compass::Evidence> = evidence
        .iter()
        .filter(|e| e.thesis_id == thesis_id && e.contestant_id == position.contestant_id)
        .collect();
    // A party is in conflict with itself when its evidence points both ways.
    let diverges = items.iter().any(|e| e.stance > 0) && items.iter().any(|e| e.stance < 0);
    html! {
        div class="flex flex-wrap items-baseline gap-x-2 gap-y-1" {
            (chips(c))
            span class="text-[15px] text-ink-muted" {
                (value_label(position.stance as i8))
            }
            @if diverges {
                span class="rounded border border-hairline px-1.5 py-0.5 text-[15px] text-ink" {
                    (i18n::t("Pledge differs from record"))
                }
            }
        }
        @if diverges {
            // The rule that decides the stance, said where the disagreement is
            // seen rather than only in the schema: a recorded act outranks a
            // pledge, and it does so even when the pledge is more recent,
            // because what a party did is stronger evidence than what it said
            // it would do.
            p class="mt-1 max-w-prose text-[14px] italic text-ink-muted" {
                (i18n::t("The recorded act sets the score, even when the pledge is more recent: what a party did outranks what it said it would do."))
            }
        }
        ul class="mt-1 space-y-0.5 pl-1" {
            @for e in &items {
                li class="flex flex-wrap items-baseline gap-x-2 text-[14px] text-ink-muted" {
                    span class="text-[15px] text-ink" { (kind_label(&e.kind)) }
                    span { (value_label(e.stance as i8)) }
                    @if let Some(d) = e.occurred_on {
                        span class="text-[15px]" { (crate::fmt::date(Some(d))) }
                    }
                    @if let Some(q) = &e.quote { span { (q) } }
                    @if let Some(l) = &e.locator {
                        span class="text-[15px] text-ink" { (l) }
                    }
                    (ui::citation::source_marker(&e.source_url))
                    @if let Some(slug) = &e.roll_call_slug {
                        a href={"/" (country) "/vote/" (slug)}
                          class="text-[15px] text-link hover:underline" {
                            (i18n::t("How each member voted"))
                        }
                    }
                    // Where the original has been archived, the copy is offered
                    // too: these documents disappear, and a dead link would
                    // take the evidence with it.
                    @if let Some(snap) = &e.snapshot_url {
                        a href=(snap) rel="noopener" target="_blank"
                          class="text-[15px] text-ink-muted hover:text-link hover:underline" {
                            (i18n::t("archived"))
                        }
                    }
                }
            }
        }
    }
}
