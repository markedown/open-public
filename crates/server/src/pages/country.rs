use axum::extract::{Path, State};
use maud::{html, Markup};

use crate::auth::AuthSession;
use crate::error::PageError;
use crate::fmt;
use crate::i18n;
use crate::ui;

pub async fn detail(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path(slug): Path<String>,
) -> Result<Markup, PageError> {
    let country = db::country::get_by_slug(&pool, &slug)
        .await?
        .ok_or(PageError::NotFound)?;
    let counts = db::country::section_counts(&pool, country.id).await?;
    let government = db::country::government(&pool, country.id).await?;
    let seats = db::country::seat_distribution(&pool, country.id).await?;
    let independents = db::country::unaffiliated_mp_count(&pool, country.id).await?;
    let chamber_size = db::country::chamber_size(&pool, country.id).await?;
    let chambers = db::country::chambers(&pool, country.id).await?;
    let alliance_rows = db::country::alliance_parties(&pool, country.id).await?;

    // The country page shows only the most recent elections; the full list is
    // on the elections index.
    let election_list =
        db::elections::list_for_country(&pool, country.id, i18n::lang_code()).await?;
    let mut elections = Vec::new();
    for e in election_list.into_iter().take(2) {
        let rows = db::elections::results(&pool, e.id).await?;
        // The previous comparable election powers the "last time" ghost bars in
        // the compact result cards, as on the full election page.
        let prev = db::elections::previous_comparable(&pool, e.id).await?;
        elections.push((e, rows, prev));
    }

    let mut polls = db::polls::full_for_country(&pool, country.id).await?;
    crate::content::localize_polls(&pool, &mut polls).await?;
    let loc = crate::content::Localized::load(&pool, "country", country.id).await?;
    let summary = loc.get("summary", country.summary.as_deref());

    let add_poll = session
        .as_ref()
        .is_some_and(|s| s.is_admin)
        .then(|| format!("/admin/poll/new?country={}", country.slug));

    let follow_state = match &session {
        Some(s) => {
            if db::follows::is_following(&pool, s.user_id, "country", country.id).await? {
                ui::follow::FollowState::Following
            } else {
                ui::follow::FollowState::NotFollowing
            }
        }
        None => ui::follow::FollowState::Anonymous,
    };
    let follow_next = format!("/{}", country.slug);

    let total_seats: i64 = seats.iter().map(|s| s.seats).sum::<i64>() + independents;
    // A legislature with more than one chamber (a senate and a house) is shown
    // as one composition bar per chamber; a unicameral one keeps the single
    // combined bar with its vacant-seats reading.
    let bicameral = chambers.len() > 1;
    // Seats sitting empty between elections: the elected chamber size minus the
    // members currently seated. Only meaningful when there are genuinely fewer
    // seated members than the chamber holds.
    let vacant: Option<i64> =
        chamber_size.and_then(|c| (c - total_seats > 0).then_some(c - total_seats));

    // Group the flat alliance rows into (name, parties). Rows are ordered by
    // alliance, so consecutive same-alliance rows collect together.
    let mut alliances: Vec<(String, String, Vec<db::country::AllianceParty>)> = Vec::new();
    for row in alliance_rows {
        match alliances.last_mut() {
            Some((name, _, parties)) if *name == row.alliance_name => parties.push(row),
            _ => alliances.push((
                row.alliance_name.clone(),
                row.alliance_slug.clone(),
                vec![row],
            )),
        }
    }

    // Entry points into this country's data. A section whose list would be empty
    // is omitted, so a newly added country shows only what it actually has. The
    // full history (Timeline) lives on its own page and is reached from here,
    // not rendered inline on the country page.
    let theses = db::compass::count_theses(&pool, country.id, db::compass::SCOPE_PARTY).await?;
    // The next election is the reason most of this page matters, so it sits
    // above the data rather than inside the elections box.
    let next_elections =
        db::elections::next_for_country(&pool, country.id, i18n::lang_code()).await?;
    // A presidential election is contested by people, so its compass is the
    // candidate one; every other kind is contested by parties. Each contest
    // links to the set that matches it, and only once that set has questions.
    let person_theses = if next_elections
        .iter()
        .any(|e| e.kind.as_deref() == Some("presidential"))
    {
        db::compass::count_theses(&pool, country.id, db::compass::SCOPE_PERSON).await?
    } else {
        0
    };
    let next_with_compass: Vec<_> = next_elections
        .iter()
        .map(|e| {
            let href = if e.kind.as_deref() == Some("presidential") {
                (person_theses > 0)
                    .then(|| format!("/{}/compass/{}", country.slug, db::compass::SCOPE_PERSON))
            } else {
                (theses > 0).then(|| format!("/{}/compass", country.slug))
            };
            (e, href)
        })
        .collect();
    let chips = [
        (i18n::t("People"), "people", counts.people),
        (i18n::t("Parties"), "parties", counts.parties),
        (i18n::t("Alliances"), "alliances", counts.alliances),
        (i18n::t("Elections"), "elections", counts.elections),
        (i18n::t("News"), "news", counts.news),
        (i18n::t("Polls"), "polls", counts.polls),
        (i18n::t("Compass"), "compass", theses),
        (i18n::t("Timeline"), "history", counts.events),
    ];

    let content = html! {
        article {
            @if !next_with_compass.is_empty() {
                (ui::election::next_elections(&next_with_compass, &country.slug))
            }
            // Identity, key facts and the in-country navigation, all in one
            // compact hero card so the page opens short.
            header class="op-card mb-6 p-6 sm:p-7" {
                div class="flex items-start justify-between gap-4" {
                    div class="flex items-center gap-4" {
                        @if let Some(ref flag) = country.flag_url {
                            img src=(flag) alt="" loading="lazy"
                                class="h-10 w-auto shrink-0 rounded-md border border-hairline";
                        }
                        h1 class="text-3xl font-bold tracking-tight text-ink sm:text-4xl" {
                            (country.name)
                        }
                    }
                    (ui::follow::button("country", country.id, follow_state, &follow_next))
                }
                div class="mt-3 flex flex-wrap gap-x-6 gap-y-1 text-[13px] text-ink-muted" {
                    @if let Some(ref c) = country.capital {
                        span { (i18n::t("Capital")) ": " span class="font-medium text-ink" { (c) } }
                    }
                    @if let Some(ref g) = country.government_type {
                        span class="font-medium text-ink" { (i18n::t_dyn(g)) }
                    }
                    @if let Some(fd) = country.founded_date {
                        span { (i18n::t("Founded")) " " span class="font-mono text-ink" { (fmt::date(Some(fd))) } }
                    }
                }
                div class="mt-5 flex flex-wrap gap-2" {
                    @for (label, path, n) in chips {
                        @if n > 0 {
                            a href={"/" (country.slug) "/" (path)}
                              class="inline-flex items-center gap-1.5 rounded-full border border-hairline bg-paper px-3.5 py-1.5 text-[13px] font-medium text-ink-muted transition-colors hover:border-accent hover:text-accent" {
                                (label)
                                span class="font-mono text-[11px] opacity-70" { (n) }
                            }
                        }
                    }
                }
            }

            @if let Some(text) = summary {
                div class="mb-8 max-w-prose" {
                    (ui::translated::prose(
                        text,
                        loc.is_translated("summary").then_some(country.summary.as_deref()).flatten(),
                    ))
                }
            }

            // Country-level polls sit right under the general information, before
            // the institutional detail: participation is a primary draw. Only the
            // two most recent are shown; the rest are one link away.
            (ui::poll_widget::poll_previews(
                &polls,
                &country.slug,
                Some(2),
                (counts.polls > 2).then_some(format!("/{}/polls", country.slug)).as_deref(),
                add_poll.as_deref(),
            ))

            // Ruling team, with a link to the full roster of people.
            @if !government.is_empty() {
                section class="mb-8" {
                    (ui::section_header(
                        i18n::t("Government"),
                        (counts.people > 0).then(|| ui::see_all_link(&format!("/{}/people", country.slug))),
                    ))
                    ul class="op-card grid gap-x-10 px-5 sm:grid-cols-2" {
                        @for m in &government {
                            li class="flex items-baseline justify-between gap-3 border-b border-hairline-light py-2.5" {
                                a href={"/" (country.slug) "/people/" (m.person_slug)}
                                  class="text-sm font-medium text-ink transition-colors hover:text-accent" {
                                    (m.person_name)
                                }
                                @if let Some(ref t) = m.title {
                                    span class="shrink-0 text-right text-xs text-ink-muted" { (t) }
                                }
                            }
                        }
                    }
                }
            }

            // Legislature composition. Bicameral: one bar per chamber, each
            // headed by the chamber's name. Unicameral: a single combined bar
            // headed by the legislature's name, with the vacant-seats reading.
            @if bicameral {
                section class="mb-8" {
                    (ui::section_header(
                        match country.legislature_name.as_deref() {
                            Some(name) => i18n::t_dyn(name),
                            None => i18n::t("Parliament"),
                        },
                        None,
                    ))
                    @for ch in &chambers {
                        div class="mb-6 last:mb-0" {
                            h3 class="mb-3 flex items-baseline gap-2 text-[13px] font-semibold text-ink" {
                                (i18n::t_dyn(&ch.chamber))
                                span class="font-mono text-xs font-medium text-ink-muted" {
                                    (ch.total) " " (i18n::t("seats"))
                                }
                            }
                            (ui::seat_bar::composition(&ch.parties, ch.independents, None, &country.slug))
                        }
                    }
                }
            } @else if !seats.is_empty() {
                section class="mb-8" {
                    (ui::section_header(
                        match country.legislature_name.as_deref() {
                            Some(name) => i18n::t_dyn(name),
                            None => i18n::t("Parliament"),
                        },
                        Some(html! {
                            span class="shrink-0 font-mono text-xs text-ink-muted" {
                                @if let (Some(c), Some(_)) = (chamber_size, vacant) {
                                    (total_seats) " / " (c) " " (i18n::t("seats"))
                                } @else {
                                    (total_seats) " " (i18n::t("seats"))
                                }
                            }
                        }),
                    ))
                    (ui::seat_bar::composition(&seats, independents, vacant, &country.slug))
                }
            }

            // Coalitions.
            @if !alliances.is_empty() {
                section class="mb-8" {
                    (ui::section_header(
                        i18n::t("Coalitions"),
                        Some(ui::see_all_link(&format!("/{}/alliances", country.slug))),
                    ))
                    div class="grid gap-4 sm:grid-cols-3" {
                        @for (name, alliance_slug, parties) in &alliances {
                            div class="op-card p-4" {
                                a href={"/" (country.slug) "/alliance/" (alliance_slug)}
                                  class="mb-3 block text-sm font-semibold text-ink transition-colors hover:text-accent" {
                                    (name)
                                }
                                div class="flex flex-wrap gap-1.5" {
                                    @for p in parties {
                                        @if let Some(ref sn) = p.short_name {
                                            a href={"/" (country.slug) "/parties/" (p.party_slug)} class="inline-flex transition-opacity hover:opacity-80" {
                                                (ui::badge::party_chip(sn, p.color.as_deref()))
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            (ui::election::country_elections(&elections, &country.slug))
        }
    };

    Ok(ui::layout::document(
        Some(&country.name),
        session.is_some(),
        session.as_ref().is_some_and(|s| s.is_admin),
        content,
    ))
}
