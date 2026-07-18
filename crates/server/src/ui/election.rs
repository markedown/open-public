use maud::{html, Markup};

use crate::fmt;
use crate::i18n;
use crate::ui;

/// A compact vote label: a share of the valid vote when known, else a short
/// vote count ("6.1M", "812k").
fn history_label(e: &db::elections::PartyHistoryEntry) -> String {
    let v = e.votes.unwrap_or(0);
    match e.valid_votes.filter(|vv| *vv > 0) {
        Some(vv) => {
            let tenths = v * 1000 / vv;
            format!("{}.{}%", tenths / 10, tenths % 10)
        }
        None if v >= 1_000_000 => format!("{}.{}M", v / 1_000_000, (v % 1_000_000) / 100_000),
        None if v >= 1_000 => format!("{}k", v / 1_000),
        None => v.to_string(),
    }
}

/// A party's support across elections as a small bar chart, oldest to newest.
/// Bar height tracks the party's vote count (always recorded, unlike seats), and
/// each bar is labelled with the election year and its vote share (or count).
/// Rendered as inline SVG, so it needs no client script; the bars carry the
/// party's own colour, the one place colour is allowed to be data.
pub fn party_history_chart(
    entries: &[db::elections::PartyHistoryEntry],
    color: Option<&str>,
) -> Markup {
    let mut points: Vec<&db::elections::PartyHistoryEntry> =
        entries.iter().filter(|e| e.votes.is_some()).collect();
    // A trend needs at least two data points; history arrives newest-first.
    if points.len() < 2 {
        return html! {};
    }
    points.reverse();
    let max = points
        .iter()
        .filter_map(|e| e.votes)
        .max()
        .unwrap_or(1)
        .max(1);
    let color = color.unwrap_or("#33527a");

    let slot = 72.0_f64;
    let bar_w = 42.0_f64;
    let chart_h = 96.0_f64;
    let width = slot * points.len() as f64;
    let height = chart_h + 30.0;

    html! {
        section class="mb-8" {
            (ui::section_header(i18n::t("Support over time"), None))
            div class="op-card overflow-x-auto p-5" {
                svg viewBox={"0 0 " (width) " " (height)}
                    class="h-40 w-full min-w-[260px]" preserveAspectRatio="xMidYMax meet"
                    role="img" aria-label=(i18n::t("Support over time")) {
                    line x1="0" y1=(chart_h) x2=(width) y2=(chart_h)
                         class="text-hairline" stroke="currentColor" stroke-width="1" {}
                    @for (i, e) in points.iter().enumerate() {
                        @let v = e.votes.unwrap_or(0);
                        // Leave headroom at the top so the tallest bar's value
                        // label is not clipped by the chart's upper edge.
                        @let h = (v as f64 / max as f64) * (chart_h - 20.0);
                        @let x = i as f64 * slot + (slot - bar_w) / 2.0;
                        @let y = chart_h - h;
                        rect x=(x) y=(y) width=(bar_w) height=(h) rx="3" fill=(color) {}
                        text x=(x + bar_w / 2.0) y=(y - 5.0) text-anchor="middle"
                             class="text-ink" fill="currentColor"
                             style="font:600 12px ui-monospace,monospace" {
                            (history_label(e))
                        }
                        text x=(x + bar_w / 2.0) y=(chart_h + 20.0) text-anchor="middle"
                             class="text-ink-muted" fill="currentColor"
                             style="font:12px ui-monospace,monospace" {
                            @if let Some(d) = e.held_on { (d.format("%Y")) } @else { "" }
                        }
                    }
                }
            }
        }
    }
}

/// A party's electoral history: seats (and votes, when known) across elections.
pub fn party_history(entries: &[db::elections::PartyHistoryEntry]) -> Markup {
    if entries.is_empty() {
        return html! {};
    }
    html! {
        section class="mb-8" {
            (ui::section_header(i18n::t("Electoral history"), None))
            ul class="op-card divide-y divide-hairline-light px-5" {
                @for e in entries {
                    li class="flex items-baseline justify-between gap-3 py-2.5" {
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
        section class="mb-8" {
            (ui::section_header(
                i18n::t("Elections"),
                Some(ui::see_all_link(&format!("/{country}/elections"))),
            ))
            div class="grid gap-4 sm:grid-cols-2" {
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
        div class="op-card p-5" {
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
        header class="op-card mb-8 p-6 sm:p-8" {
            h1 class="text-3xl font-bold tracking-tight text-ink sm:text-4xl" { (election.name) }
            @if let Some(d) = election.held_on {
                p class="mt-1 font-mono text-sm text-ink-muted" { (fmt::date(Some(d))) }
            }
            @if let Some(ref desc) = election.description {
                p class="mt-4 max-w-prose text-[15px] leading-relaxed text-ink" { (desc) }
            }
        }

        @if total_seats > 0 {
            section class="mb-8" {
                h2 class="mb-3 text-[13px] font-bold uppercase tracking-wider text-ink-muted" {
                    (i18n::t("Seats")) " " span class="font-mono" { (total_seats) }
                }
                (seat_bar(rows))
                div class="mt-3" { (seat_chips(rows, country)) }
            }
        }

        (turnout_stats(election))

        @if let Some(valid) = election.valid_votes.filter(|v| *v > 0) {
            section class="mb-8" {
                h2 class="mb-3 text-[13px] font-bold uppercase tracking-wider text-ink-muted" {
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
        div class="flex h-6 w-full overflow-hidden rounded-md border border-hairline" {
            @for r in rows {
                @if let Some(s) = r.seats.filter(|s| *s > 0) {
                    div class="h-full border-r border-r-paper-raised last:border-r-0"
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
        div class="op-card p-3" {
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
    // Order the vote-share bars by vote count, not by the seat-first order the
    // rows arrive in. A threshold-exempt party can hold a seat on a tiny vote
    // while a larger party wins none (for example a party just under an
    // electoral threshold), so seat order would misrank the vote-share bars.
    let mut voted: Vec<&db::elections::ResultRow> =
        rows.iter().filter(|r| r.votes.is_some()).collect();
    voted.sort_by_key(|r| std::cmp::Reverse(r.votes));
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
                    span class="relative h-4 grow overflow-hidden rounded bg-paper-sunken" {
                        span class="absolute inset-y-0 left-0 rounded"
                             style={"width:" (tenths as f64 / 10.0) "%;background-color:" (color)} {}
                    }
                    span class="w-12 shrink-0 text-right font-mono text-xs text-ink" { (fmt_pct(tenths)) }
                }
            }
            @if others > 0 {
                @let tenths = others * 1000 / valid;
                div class="flex items-center gap-2" {
                    span class="w-14 shrink-0 truncate text-xs text-ink-muted sm:w-28" { (i18n::t("Others")) }
                    span class="relative h-4 grow overflow-hidden rounded bg-paper-sunken" {
                        span class="absolute inset-y-0 left-0 rounded bg-hairline"
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
