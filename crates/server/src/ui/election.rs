use std::collections::HashMap;

use maud::{html, Markup};

use crate::fmt;
use crate::i18n;
use crate::ui;

/// A previous election's vote shares keyed by party slug (or, for label
/// contests like presidential candidates, by label), in tenths of a percent.
/// Used to draw a faint "last time" ghost behind each current bar and a swing.
type PrevShares = HashMap<String, i64>;

/// Build the comparison shares from a previous election and its rows. Empty when
/// there is no previous election or it has no valid-vote total to divide by.
fn prev_shares(
    prev: Option<&(db::elections::Election, Vec<db::elections::ResultRow>)>,
) -> PrevShares {
    let mut map = PrevShares::new();
    if let Some((e, rows)) = prev {
        if let Some(valid) = e.valid_votes.filter(|v| *v > 0) {
            for r in rows {
                if let (Some(v), Some(key)) =
                    (r.votes, r.party_slug.clone().or_else(|| r.label.clone()))
                {
                    map.insert(key, v * 1000 / valid);
                }
            }
        }
    }
    map
}

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
pub fn party_history(entries: &[db::elections::PartyHistoryEntry], country: &str) -> Markup {
    if entries.is_empty() {
        return html! {};
    }
    html! {
        section class="mb-8" {
            (ui::section_header(i18n::t("Electoral history"), None))
            ul class="op-card divide-y divide-hairline-light px-5" {
                @for e in entries {
                    li class="flex items-baseline justify-between gap-3 py-2.5" {
                        // Each result links to the election it belongs to, so a
                        // party's own record is a way into every election it
                        // contested rather than a list of plain names.
                        a href={"/" (country) "/election/" (e.election_slug)}
                          class="text-sm font-medium text-ink transition-colors hover:text-link" {
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
/// one result box carrying its previous-election comparison; the heading links
/// to the elections index.
#[allow(clippy::type_complexity)]
pub fn country_elections(
    elections: &[(
        db::elections::Election,
        Vec<db::elections::ResultRow>,
        Option<(db::elections::Election, Vec<db::elections::ResultRow>)>,
    )],
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
                @for (election, rows, prev) in elections {
                    (election_box(election, rows, prev.as_ref(), country, true, Some(6)))
                }
            }
        }
    }
}

/// One election's compact result box for the country overview: name/date,
/// description, party seat chips, the top vote-share bars (capped by `limit`
/// with an "others" aggregate) with a "last time" ghost when a previous
/// comparable election is supplied, and a turnout line. When `linked`, the title
/// links to the election's own page.
pub fn election_box(
    election: &db::elections::Election,
    rows: &[db::elections::ResultRow],
    prev: Option<&(db::elections::Election, Vec<db::elections::ResultRow>)>,
    country: &str,
    linked: bool,
    limit: Option<usize>,
) -> Markup {
    let prev_map = prev_shares(prev);
    html! {
        div class="op-card p-5 sm:p-6" {
            div class="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1" {
                @if linked {
                    a href={"/" (country) "/election/" (election.slug)} class="op-link text-[19px] font-bold" {
                        (election.name)
                    }
                } @else {
                    h3 class="text-[19px] font-bold text-ink" { (election.name) }
                }
                @if let Some(d) = election.held_on {
                    span class="text-[15px] text-ink-muted" { (fmt::date(Some(d))) }
                }
            }

            @if let Some(ref desc) = election.description {
                p class="mt-2 max-w-prose text-[15px] leading-relaxed text-ink-muted" { (desc) }
            }

            @if let Some(valid) = election.valid_votes.filter(|v| *v > 0) {
                div class="mt-5" {
                    (previous_legend(prev.map(|(e, _)| e), &prev_map))
                    (vote_share_list(rows, valid, country, limit, &prev_map))
                }
            } @else {
                (seat_chips(rows, country))
            }

            @if let (Some(cast), Some(elect)) = (election.votes_cast, election.electorate.filter(|e| *e > 0)) {
                p class="mt-4 text-[15px] text-ink-muted" {
                    (i18n::t("Turnout")) " " span class="font-semibold text-ink" { (fmt_pct(cast * 1000 / elect)) }
                    " · " (thousands(cast)) " / " (thousands(elect))
                }
            }
        }
    }
}

/// One election's full detail: a seat-composition bar (seat contests), turnout
/// statistic cards, and the complete contestant vote-share list. When a previous
/// comparable election is supplied, each vote-share bar carries a faint ghost of
/// that election's share and a swing figure (for a runoff, the ghost is the first
/// round).
/// Whether an election is still to come. An election says so by carrying an
/// `expected_note`, since a date is often not fixed yet; a date, when there is
/// one, only has to still be ahead.
pub fn is_upcoming(election: &db::elections::Election) -> bool {
    election.expected_note.is_some()
        && election
            .held_on
            .is_none_or(|d| d >= chrono::Utc::now().date_naive())
}

/// How many days until an election with a date that has not arrived.
pub fn days_ahead(election: &db::elections::Election) -> Option<i64> {
    let day = election.held_on?;
    let today = chrono::Utc::now().date_naive();
    (day > today).then(|| (day - today).num_days())
}

pub fn election_detail(
    election: &db::elections::Election,
    rows: &[db::elections::ResultRow],
    prev: Option<&(db::elections::Election, Vec<db::elections::ResultRow>)>,
    country: &str,
    crumbs: &[ui::breadcrumb::Crumb],
) -> Markup {
    let total_seats: i32 = rows.iter().filter_map(|r| r.seats).sum();
    let prev_map = prev_shares(prev);
    let leader_by_seats = rows
        .iter()
        .filter(|r| r.seats.is_some_and(|s| s > 0))
        .max_by_key(|r| r.seats);
    let leader_by_votes = rows
        .iter()
        .filter(|r| r.votes.is_some())
        .max_by_key(|r| r.votes);
    let valid = election.valid_votes.filter(|v| *v > 0);
    html! {
        (ui::hero_band(html! {
            (ui::breadcrumb::trail(crumbs))
            h1 class="mt-4 text-[42px] font-extrabold leading-[1.05] tracking-tight text-ink sm:text-[50px]" {
                (election.name)
            }
            @if let Some(d) = election.held_on {
                p class="mt-2 text-[18px] text-ink-muted" { (fmt::date(Some(d))) }
            }
            @if let Some(ref desc) = election.description {
                p class="mt-4 max-w-[42em] text-[20px] leading-relaxed text-ink" { (desc) }
            }
            // An election dated ahead of today has not been held, so the page
            // says so rather than presenting an empty result set as a result.
            @if is_upcoming(election) {
                div class="mt-5 max-w-[46em] rounded-[12px] bg-accent-tint px-5 py-4" {
                    p class="font-bold text-ink" {
                        (i18n::t("Not held yet"))
                        @if let Some(days) = days_ahead(election) {
                            " · " (days) " " (i18n::t("days"))
                        }
                    }
                    @if let Some(note) = &election.expected_note {
                        p class="mt-1 text-[16px] text-ink-muted" { (note) }
                    }
                }
            }
            @if leader_by_votes.is_some() || election.electorate.is_some() {
                div class="mt-7 grid gap-4 sm:grid-cols-3" {
                    @if let Some(r) = leader_by_seats.filter(|_| total_seats > 0) {
                        (ui::fact_card(
                            html! {
                                (r.seats.unwrap_or(0))
                                span class="text-[20px] font-bold text-ink-muted" { " / " (total_seats) }
                            },
                            i18n::t("Most seats"),
                            Some(html! { (row_name(r)) }),
                        ))
                    } @else if let (Some(r), Some(v)) = (leader_by_votes, valid) {
                        (ui::fact_card(
                            html! { (fmt_pct(r.votes.unwrap_or(0) * 1000 / v)) },
                            i18n::t("Most votes"),
                            Some(html! { (row_name(r)) }),
                        ))
                    }
                    @if let (Some(cast), Some(elect)) = (election.votes_cast, election.electorate.filter(|e| *e > 0)) {
                        @let tenths = cast * 1000 / elect;
                        div class="rounded-[12px] border border-hairline bg-paper px-5 py-4" {
                            div class="text-[38px] font-extrabold leading-none tracking-tight text-ink" { (fmt_pct(tenths)) }
                            div class="mt-2 font-semibold text-ink" { (i18n::t("Turnout")) }
                            div aria-hidden="true" class="mt-2 h-2 rounded-full bg-hairline-light" {
                                div class="h-2 rounded-full bg-ink" style={"width:" (tenths as f64 / 10.0) "%"} {}
                            }
                            div class="mt-1.5 text-[15px] text-ink-muted" {
                                (thousands(cast)) " / " (thousands(elect)) " " (i18n::t("Registered"))
                            }
                        }
                    } @else if let Some(e) = election.electorate {
                        (ui::fact_card(html! { (thousands(e)) }, i18n::t("Registered"), None))
                    }
                    @if let Some(v) = valid {
                        (ui::fact_card(
                            html! { (thousands(v)) },
                            i18n::t("Valid votes"),
                            None,
                        ))
                    }
                }
            }
        }))

        @if total_seats > 0 {
            section class="mb-12" {
                (ui::section_header(i18n::t("Seats"), Some(html! {
                    span class="text-[16px] text-ink-muted" { (total_seats) " " (i18n::t("seats")) }
                })))
                (seat_bar(rows))
                (seat_chips(rows, country))
            }
        }

        @if let Some(valid) = valid {
            section class="mb-12" {
                (ui::section_header(i18n::t("Vote share"), None))
                (previous_legend(prev.map(|(e, _)| e), &prev_map))
                (vote_share_list(rows, valid, country, None, &prev_map))
            }
        }
    }
}

/// A result row's display name: the party's name, or the label of a list,
/// coalition or candidate that is not one of our parties.
fn row_name(r: &db::elections::ResultRow) -> &str {
    r.party_name
        .as_deref()
        .or(r.label.as_deref())
        .unwrap_or_default()
}

/// The key to the faint "previous election" bars, naming the compared
/// election, shown only when there is one.
fn previous_legend(prev: Option<&db::elections::Election>, map: &PrevShares) -> Markup {
    html! {
        @if !map.is_empty() {
            div class="mb-3 flex items-center gap-2 text-[15px] text-ink-muted" {
                span aria-hidden="true" class="inline-block h-2.5 w-5 shrink-0 rounded-full bg-ink-muted/25" {}
                (i18n::t("Previous"))
                @if let Some(e) = prev {
                    " · " (e.name)
                }
            }
        }
    }
}

/// The seat chips, each with its seat count: a linked party chip for a party
/// row, and the label with a neutral swatch for a seated label row (a list or
/// coalition that is not one of our parties), which would otherwise hold seats
/// in the bar with no name beside them.
fn seat_chips(rows: &[db::elections::ResultRow], country: &str) -> Markup {
    html! {
        ul class="mt-4 flex flex-wrap gap-x-5 gap-y-2" {
            @for r in rows {
                @if let (Some(name), Some(slug), Some(s)) = (r.party_name.as_deref(), r.party_slug.as_deref(), r.seats.filter(|s| *s > 0)) {
                    li class="inline-flex items-center gap-2 text-[16px]" {
                        span aria-hidden="true" class="size-3 shrink-0 rounded-full"
                            style={"background-color:" (ui::css_color(r.party_color.as_deref(), UNCOLOURED_SEAT))} {}
                        a href={"/" (country) "/parties/" (slug)} class="text-ink hover:underline" { (name) }
                        span class="font-bold text-ink" { (s) }
                    }
                } @else if let (Some(label), Some(s)) = (r.label.as_deref(), r.seats.filter(|s| *s > 0)) {
                    li class="inline-flex items-center gap-2 text-[16px]" {
                        span aria-hidden="true" class="size-3 shrink-0 rounded-full"
                            style={"background-color:" (UNCOLOURED_SEAT)} {}
                        span class="text-ink" { (label) }
                        span class="font-bold text-ink" { (s) }
                    }
                }
            }
        }
    }
}

/// The bar colour of a seat held by a row with no party colour.
const UNCOLOURED_SEAT: &str = "#545b64";

/// A proportional seat-composition bar from the per-party seat counts.
fn seat_bar(rows: &[db::elections::ResultRow]) -> Markup {
    html! {
        // Decorative: the composition list beside the bar names each party and
        // its seats as text, so the bar is hidden from assistive tech rather than
        // repeating that as a native tooltip.
        div class="flex h-5 w-full overflow-hidden rounded-full bg-hairline-light"
            aria-hidden="true" {
            @for r in rows {
                @if let Some(s) = r.seats.filter(|s| *s > 0) {
                    div class="h-full border-r-2 border-r-paper last:border-r-0"
                        style={"flex:" (s) " 0 0;background-color:" (ui::css_color(r.party_color.as_deref(), UNCOLOURED_SEAT))} {}
                }
            }
        }
    }
}

/// The vote-share bars, most first, capped by `limit` with an "others" row.
/// When `prev` holds a share for a row (keyed by party slug or label), the bar
/// carries a faint ghost of that previous share and a swing figure.
fn vote_share_list(
    rows: &[db::elections::ResultRow],
    valid: i64,
    country: &str,
    limit: Option<usize>,
    prev: &PrevShares,
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
    // Reserve the swing column only when at least one shown row has a previous
    // share, so plain lists keep their tighter layout.
    let has_swing = shown.iter().any(|r| {
        r.party_slug
            .as_deref()
            .or(r.label.as_deref())
            .is_some_and(|k| prev.contains_key(k))
    });
    let has_seats = rows.iter().any(|r| r.seats.is_some_and(|s| s > 0));
    let cols = if has_seats {
        "grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-4 gap-y-1.5 border-b border-hairline-light py-2.5 sm:grid-cols-[minmax(10rem,2fr)_minmax(0,3fr)_4.5rem]"
    } else {
        "grid grid-cols-1 items-center gap-x-4 gap-y-1.5 border-b border-hairline-light py-2.5 sm:grid-cols-[minmax(10rem,2fr)_minmax(0,3fr)]"
    };
    html! {
        div {
            @if has_seats {
                div class="hidden text-[14px] text-ink-muted sm:grid sm:grid-cols-[minmax(10rem,2fr)_minmax(0,3fr)_4.5rem] sm:gap-x-4" {
                    span {}
                    span { (i18n::t("Vote share")) }
                    span class="text-right" { (i18n::t("Seats")) }
                }
            }
            @for r in shown {
                @let votes = r.votes.unwrap_or(0);
                @let tenths = votes * 1000 / valid;
                @let seated = r.seats.is_some_and(|s| s > 0);
                // A party's own colour where it has one; otherwise slate for a
                // contestant that won seats and grey for one that did not.
                @let color = match r.party_color.as_deref() {
                    Some(_) => ui::css_color(r.party_color.as_deref(), "#1b1f24"),
                    None if seated || !has_seats => "#1b1f24".to_string(),
                    None => "#a7afb8".to_string(),
                };
                @let prev_tenths = r.party_slug.as_deref().or(r.label.as_deref())
                    .and_then(|k| prev.get(k).copied());
                div class=(cols) {
                    div class="order-1 min-w-0 text-[16px] font-semibold leading-snug text-ink" {
                        @if let (Some(name), Some(slug)) = (r.party_name.as_deref(), r.party_slug.as_deref()) {
                            a href={"/" (country) "/parties/" (slug)} class="hover:underline" { (name) }
                            @if let Some(sn) = r.party_short_name.as_deref().filter(|sn| *sn != name) {
                                " " span class="font-normal text-ink-muted" { (sn) }
                            }
                        } @else if let Some(lbl) = r.label.as_deref() {
                            (lbl)
                        }
                    }
                    div class="order-3 col-span-full flex items-center gap-3 sm:order-2 sm:col-span-1" {
                        span class="relative h-3 grow rounded-full bg-hairline-light" {
                            // The previous share sits behind the current one, so
                            // when the contestant lost ground its faint ghost
                            // shows past the current bar.
                            @if let Some(pt) = prev_tenths {
                                span class="absolute inset-y-0 left-0 rounded-full opacity-25"
                                     style={"width:" (pt as f64 / 10.0) "%;background-color:" (color)} {}
                            }
                            span class="absolute inset-y-0 left-0 min-w-1.5 rounded-full"
                                 style={"width:" (tenths as f64 / 10.0) "%;background-color:" (color)} {}
                            // A tick at the previous share marks "last time" even
                            // when the bar has grown past it.
                            @if let Some(pt) = prev_tenths {
                                span class="absolute -inset-y-1 w-0.5 bg-ink/50"
                                     style={"left:" (pt as f64 / 10.0) "%"} {}
                            }
                        }
                        span class="w-14 shrink-0 text-right text-[16px] text-ink" { (fmt_pct(tenths)) }
                        @if has_swing {
                            span class="w-12 shrink-0 text-right text-[14px]" {
                                @if let Some(pt) = prev_tenths { (swing(tenths - pt)) }
                            }
                        }
                    }
                    @if has_seats {
                        div class="order-2 text-right text-[16px] sm:order-3" {
                            @if seated {
                                span class="inline-block min-w-9 rounded-full bg-ink px-2.5 py-0.5 text-center font-bold text-paper" {
                                    (r.seats.unwrap_or(0))
                                }
                            } @else {
                                span class="text-ink-faint" { "0" }
                            }
                        }
                    }
                }
            }
            @if others > 0 {
                @let tenths = others * 1000 / valid;
                div class=(cols) {
                    div class="order-1 text-[16px] text-ink-muted" { (i18n::t("Others")) }
                    div class="order-3 col-span-full flex items-center gap-3 sm:order-2 sm:col-span-1" {
                        span class="relative h-3 grow rounded-full bg-hairline-light" {
                            span class="absolute inset-y-0 left-0 rounded-full bg-hairline-strong"
                                 style={"width:" (tenths as f64 / 10.0) "%"} {}
                        }
                        span class="w-14 shrink-0 text-right text-[16px] text-ink-muted" { (fmt_pct(tenths)) }
                        @if has_swing { span class="w-12 shrink-0" {} }
                    }
                    @if has_seats { div class="order-2 sm:order-3" {} }
                }
            }
        }
    }
}

/// A swing figure between two shares in tenths of a percent, as an up/down
/// triangle and a one-decimal magnitude ("▲2.6", "▼1.3", "±0.0"). Kept
/// monochrome so it never reads as a party colour; direction carries the sign.
fn swing(delta: i64) -> Markup {
    let (glyph, label, mag) = match delta {
        d if d > 0 => ("▲", i18n::t("gained"), d),
        d if d < 0 => ("▼", i18n::t("lost"), -d),
        _ => ("±", i18n::t("unchanged"), 0),
    };
    html! {
        // The arrow is the whole meaning for a sighted reader, and nothing at
        // all for a screen reader, so the direction is also said in words. The
        // words are visually hidden, and the arrow hidden from assistive
        // technology, so neither reader gets the other's version twice.
        span class="text-ink-muted" {
            span aria-hidden="true" { (glyph) }
            span class="sr-only" { (label) " " }
            (mag / 10) "." (mag % 10)
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

/// A "compare your positions" link into the compass, for the pages the compass
/// scores. `href` is the scope-appropriate URL, already resolved by the caller
/// (the party set for a party page, the candidate set for a presidential one);
/// when there is no matching thesis set the caller passes None and nothing
/// renders, so a page never links to an empty compass.
pub fn compass_cta(href: Option<&str>) -> Markup {
    html! {
        @if let Some(href) = href {
            a href=(href)
              class="inline-flex min-h-11 items-center rounded-[10px] bg-ink px-5 text-[16px] font-semibold text-paper transition-colors hover:bg-accent-strong" {
                (i18n::t("Compare your positions"))
            }
        }
    }
}

/// The country's next election, shown above its data because an election is the
/// event that makes the rest of the page matter.
///
/// `days` is the count until polling day. Some systems fix the date by law and
/// others only a deadline, so `expected_note` is printed whenever it is set
/// rather than presenting a provisional date as a certainty.
pub fn next_elections(
    elections: &[(&db::elections::Election, Option<String>)],
    country: &str,
) -> Markup {
    let Some((first, _)) = elections.first() else {
        return html! {};
    };
    html! {
        section class="mb-12 flex gap-4 rounded-[12px] bg-accent-tint px-5 py-5" {
            svg aria-hidden="true" focusable="false" width="24" height="24" viewBox="0 0 24 24"
                fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"
                class="mt-0.5 shrink-0 text-ink" {
                circle cx="12" cy="12" r="9" {}
                path d="M12 7v6l3 2" {}
            }
            div class="min-w-0" {
                p class="font-bold text-ink" {
                    // One round, so one date and one countdown, however many
                    // contests are decided on it.
                    @if elections.len() > 1 {
                        (i18n::t("Next elections"))
                    } @else {
                        (i18n::t("Next election"))
                    }
                    ": "
                    @if let Some(d) = first.held_on {
                        (crate::fmt::date(Some(d)))
                        @if let Some(days) = days_ahead(first) {
                            " · " (days) " " (i18n::t("days"))
                        }
                    } @else {
                        // No date fixed yet: the note below says what decides it.
                        (i18n::t("Date not fixed"))
                    }
                }
                @for (election, compass_href) in elections {
                    (next_election_entry(election, country, compass_href.as_deref()))
                }
            }
        }
    }
}

/// One contest of the next round: what is being elected, the note about the
/// date, and the compass that matches what it is contested by.
fn next_election_entry(
    election: &db::elections::Election,
    country: &str,
    compass_href: Option<&str>,
) -> Markup {
    html! {
        div class="mt-3" {
            a href={"/" (country) "/election/" (election.slug)} class="op-link text-[19px] font-bold" {
                (election.name)
            }
            span class="ml-2 text-[15px] text-ink-muted" {
                (crate::pages::elections::kind_label(election.kind.as_deref()))
            }
            // Whether the date is fixed or merely a legal deadline.
            @if let Some(note) = &election.expected_note {
                p class="mt-1 max-w-prose text-[16px] text-ink-muted" { (note) }
            }
            @if let Some(href) = compass_href {
                a href=(href)
                  class="mt-3 inline-flex min-h-11 items-center rounded-[10px] bg-ink px-5 text-[16px] font-semibold text-paper transition-colors hover:bg-accent-strong" {
                    (i18n::t("Compare your positions"))
                }
            }
        }
    }
}
