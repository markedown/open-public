use maud::{html, Markup};

use crate::fmt;
use crate::i18n;
use crate::ui;

/// A party's electoral history: seats (and votes, when known) across elections.
pub fn party_history(entries: &[db::elections::PartyHistoryEntry]) -> Markup {
    if entries.is_empty() {
        return html! {};
    }
    html! {
        section class="mb-12" {
            h2 class="mb-5 border-b-2 border-accent pb-2 text-xs font-bold uppercase tracking-widest text-ink" {
                (i18n::t("Electoral history"))
            }
            ul class="space-y-0" {
                @for e in entries {
                    li class="flex items-baseline justify-between gap-3 border-b border-hairline-light py-2.5" {
                        span class="text-sm font-medium text-ink" {
                            (e.election_name)
                            @if let Some(d) = e.held_on {
                                " "
                                span class="font-mono text-xs text-ink-muted" { "(" (d.format("%Y")) ")" }
                            }
                        }
                        span class="flex shrink-0 items-baseline gap-3 font-mono text-xs text-ink-muted" {
                            @if let Some(s) = e.seats {
                                span class="font-semibold text-ink" { (s) " " (i18n::t("seats")) }
                            }
                            @if let Some(v) = e.votes { span { (thousands(v)) " " (i18n::t("votes")) } }
                        }
                    }
                }
            }
        }
    }
}

/// A country's elections as a section with a "see all" link. Each election is
/// one result box; the heading links to the elections index.
pub fn country_elections(
    elections: &[(db::elections::Election, Vec<db::elections::ResultRow>)],
    country: &str,
) -> Markup {
    if elections.is_empty() {
        return html! {};
    }
    html! {
        section class="mb-12" {
            div class="mb-5 flex items-center justify-between gap-3 border-b-2 border-accent pb-2" {
                h2 class="text-xs font-bold uppercase tracking-widest text-ink" { (i18n::t("Elections")) }
                a href={"/" (country) "/elections"}
                  class="text-[11px] font-bold uppercase tracking-wide text-accent transition-colors hover:underline" {
                    (i18n::t("See all"))
                }
            }
            div class="space-y-4" {
                @for (election, rows) in elections {
                    (election_box(election, rows, country, true, Some(6)))
                }
            }
        }
    }
}

/// One election's compact result box for the country overview: name/date,
/// description, party seat chips, the top vote-share bars (capped by `limit`
/// with an "others" aggregate), and a turnout line. When `linked`, the title
/// links to the election's own page.
pub fn election_box(
    election: &db::elections::Election,
    rows: &[db::elections::ResultRow],
    country: &str,
    linked: bool,
    limit: Option<usize>,
) -> Markup {
    html! {
        div class="border-[1.5px] border-ink p-4" {
            div class="flex flex-wrap items-baseline justify-between gap-2" {
                @if linked {
                    a href={"/" (country) "/election/" (election.slug)}
                      class="text-sm font-semibold text-ink transition-colors hover:text-accent" {
                        (election.name)
                    }
                } @else {
                    h3 class="text-sm font-semibold text-ink" { (election.name) }
                }
                @if let Some(d) = election.held_on {
                    span class="font-mono text-xs text-ink-muted" { (fmt::date(Some(d))) }
                }
            }

            @if let Some(ref desc) = election.description {
                p class="mt-1 max-w-prose text-xs leading-relaxed text-ink-muted" { (desc) }
            }

            (seat_chips(rows, country))

            @if let Some(valid) = election.valid_votes.filter(|v| *v > 0) {
                div class="mt-4" { (vote_share_list(rows, valid, country, limit)) }
            }

            @if let (Some(cast), Some(elect)) = (election.votes_cast, election.electorate.filter(|e| *e > 0)) {
                p class="mt-3 font-mono text-[11px] text-ink-muted" {
                    (i18n::t("Turnout")) " " (fmt_pct(cast * 1000 / elect))
                    " · " (thousands(cast)) " / " (thousands(elect))
                }
            }
        }
    }
}

/// One election's full detail: a seat-composition bar (seat contests), turnout
/// statistic cards, and the complete contestant vote-share list.
pub fn election_detail(
    election: &db::elections::Election,
    rows: &[db::elections::ResultRow],
    country: &str,
) -> Markup {
    let total_seats: i32 = rows.iter().filter_map(|r| r.seats).sum();
    html! {
        header class={"mb-8 border-[1.5px] border-ink bg-paper-raised p-6 sm:p-8 " (ui::CORNER_TICK)} {
            h1 class="font-serif text-3xl font-semibold tracking-tight text-ink sm:text-4xl" { (election.name) }
            @if let Some(d) = election.held_on {
                p class="mt-1 font-mono text-sm text-ink-muted" { (fmt::date(Some(d))) }
            }
            @if let Some(ref desc) = election.description {
                p class="mt-4 max-w-prose text-[15px] leading-relaxed text-ink" { (desc) }
            }
        }

        @if total_seats > 0 {
            section class="mb-8" {
                h2 class="mb-3 text-xs font-bold uppercase tracking-widest text-ink-muted" {
                    (i18n::t("Seats")) " " span class="font-mono" { (total_seats) }
                }
                (seat_bar(rows))
                div class="mt-3" { (seat_chips(rows, country)) }
            }
        }

        (turnout_stats(election))

        @if let Some(valid) = election.valid_votes.filter(|v| *v > 0) {
            section class="mb-8" {
                h2 class="mb-3 text-xs font-bold uppercase tracking-widest text-ink-muted" {
                    (i18n::t("Vote share"))
                }
                (vote_share_list(rows, valid, country, None))
            }
        }
    }
}

/// The party seat chips (party contests only), each with its seat count.
fn seat_chips(rows: &[db::elections::ResultRow], country: &str) -> Markup {
    html! {
        div class="mt-3 flex flex-wrap gap-x-4 gap-y-2" {
            @for r in rows {
                @if let (Some(sn), Some(slug)) = (r.party_short_name.as_deref(), r.party_slug.as_deref()) {
                    a href={"/" (country) "/parties/" (slug)}
                      class="inline-flex items-center gap-1.5 transition-opacity hover:opacity-80" {
                        (ui::badge::party_chip(sn, r.party_color.as_deref()))
                        @if let Some(s) = r.seats {
                            span class="font-mono text-xs font-semibold text-ink" { (s) }
                        }
                    }
                }
            }
        }
    }
}

/// A proportional seat-composition bar from the per-party seat counts.
fn seat_bar(rows: &[db::elections::ResultRow]) -> Markup {
    html! {
        div class="flex h-6 w-full overflow-hidden border-[1.5px] border-ink" {
            @for r in rows {
                @if let Some(s) = r.seats.filter(|s| *s > 0) {
                    div class="h-full border-r-[1.5px] border-r-paper last:border-r-0"
                        style={"flex:" (s) " 0 0;background-color:" (r.party_color.as_deref().unwrap_or("#171717"))}
                        title={(r.party_name.as_deref().unwrap_or("")) " · " (s)} {}
                }
            }
        }
    }
}

/// Turnout statistic cards: registered voters, valid votes, and turnout share.
fn turnout_stats(election: &db::elections::Election) -> Markup {
    html! {
        @if election.electorate.is_some() || election.valid_votes.is_some() {
            div class="mb-8 grid grid-cols-2 gap-3 sm:grid-cols-4" {
                @if let (Some(cast), Some(elect)) = (election.votes_cast, election.electorate.filter(|e| *e > 0)) {
                    (stat_card(i18n::t("Turnout"), fmt_pct(cast * 1000 / elect)))
                }
                @if let Some(e) = election.electorate {
                    (stat_card(i18n::t("Registered"), thousands(e)))
                }
                @if let Some(c) = election.votes_cast {
                    (stat_card(i18n::t("Votes cast"), thousands(c)))
                }
                @if let Some(v) = election.valid_votes {
                    (stat_card(i18n::t("Valid votes"), thousands(v)))
                }
            }
        }
    }
}

/// One labelled statistic card (a mono figure over an uppercase caption).
fn stat_card(label: &str, value: String) -> Markup {
    html! {
        div class="border border-hairline p-3" {
            div class="font-mono text-lg font-semibold text-ink" { (value) }
            div class="mt-0.5 text-[10px] font-bold uppercase tracking-widest text-ink-muted" { (label) }
        }
    }
}

/// The vote-share bars, most first, capped by `limit` with an "others" row.
fn vote_share_list(
    rows: &[db::elections::ResultRow],
    valid: i64,
    country: &str,
    limit: Option<usize>,
) -> Markup {
    let voted: Vec<&db::elections::ResultRow> = rows.iter().filter(|r| r.votes.is_some()).collect();
    let (shown, others): (&[&db::elections::ResultRow], i64) = match limit {
        Some(n) if voted.len() > n => {
            (&voted[..n], voted[n..].iter().filter_map(|r| r.votes).sum())
        }
        _ => (&voted[..], 0),
    };
    html! {
        div class="space-y-1.5" {
            @for r in shown {
                @let votes = r.votes.unwrap_or(0);
                @let tenths = votes * 1000 / valid;
                @let color = r.party_color.as_deref().unwrap_or("#33527a");
                div class="flex items-center gap-2" {
                    @if let (Some(sn), Some(slug)) = (r.party_short_name.as_deref(), r.party_slug.as_deref()) {
                        a href={"/" (country) "/parties/" (slug)}
                          class="w-14 shrink-0 transition-opacity hover:opacity-80" {
                            (ui::badge::party_chip(sn, r.party_color.as_deref()))
                        }
                    } @else if let Some(lbl) = r.label.as_deref() {
                        span class="w-28 shrink-0 truncate text-xs font-medium text-ink" { (lbl) }
                    }
                    span class="relative h-4 grow overflow-hidden border border-hairline-light" {
                        span class="absolute inset-y-0 left-0"
                             style={"width:" (tenths as f64 / 10.0) "%;background-color:" (color)} {}
                    }
                    span class="w-12 shrink-0 text-right font-mono text-xs text-ink" { (fmt_pct(tenths)) }
                }
            }
            @if others > 0 {
                @let tenths = others * 1000 / valid;
                div class="flex items-center gap-2" {
                    span class="w-14 shrink-0 truncate text-xs text-ink-muted sm:w-28" { (i18n::t("Others")) }
                    span class="relative h-4 grow overflow-hidden border border-hairline-light" {
                        span class="absolute inset-y-0 left-0 bg-hairline"
                             style={"width:" (tenths as f64 / 10.0) "%"} {}
                    }
                    span class="w-12 shrink-0 text-right font-mono text-xs text-ink-muted" { (fmt_pct(tenths)) }
                }
            }
        }
    }
}

/// Format tenths of a percent (e.g. 3556) as a one-decimal percentage
/// ("35.6%").
fn fmt_pct(tenths: i64) -> String {
    format!("{}.{}%", tenths / 10, tenths % 10)
}

/// Group an integer's digits into threes with a thin space, locale-neutral.
fn thousands(n: i64) -> String {
    let digits = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push('\u{202f}');
        }
        out.push(c);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}
