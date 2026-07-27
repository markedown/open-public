//! The monthly approval panel shown on a person, party, or coalition page.
//!
//! Everyone sees the counts; only a verified visitor can register their own.
//! The counts are shown as raw numbers among the people who took part here,
//! never as a share of a population, and the panel says so, because this
//! platform does not present participation as a representative sample. The bars
//! are deliberately monochrome: colour on this platform is data (a party's
//! colour), never a sentiment cue, so approve and disapprove are told apart by
//! their label, not by green and red.

use chrono::{Datelike, NaiveDate};
use maud::{html, Markup};

use db::approvals::{Tally, APPROVE, DISAPPROVE, NO_OPINION};

use crate::i18n;

/// Who is looking at the panel.
pub enum Viewer {
    /// Not signed in: can see the counts, is invited to log in to take part.
    Anonymous,
    /// Signed in and has not registered an approval this month.
    CanVote,
    /// Signed in and already registered this choice this month (a position).
    Voted(i32),
}

/// The approval panel for an entity. `entity_type` is one of `person`, `party`,
/// `coalition`; `next` is where a no-JavaScript submit returns.
pub fn panel(
    entity_type: &str,
    entity_id: i64,
    tally: &Tally,
    period: NaiveDate,
    viewer: Viewer,
    next: &str,
) -> Markup {
    let dom_id = format!("approval-{entity_type}-{entity_id}");
    let total = tally.total();
    let month = format!("{} {}", i18n::month_abbr(period.month()), period.year());
    let rows = [
        (APPROVE, i18n::t("Approve"), tally.approve),
        (DISAPPROVE, i18n::t("Disapprove"), tally.disapprove),
        (NO_OPINION, i18n::t("No opinion"), tally.no_opinion),
    ];
    let action = format!("/approve/{entity_type}/{entity_id}");

    html! {
        section id=(dom_id) class="op-card p-5" {
            div class="flex items-baseline justify-between gap-3" {
                h2 class="text-sm font-bold uppercase tracking-widest text-ink-muted" {
                    (i18n::t("Approval"))
                }
                span class="font-mono text-xs text-ink-muted" { (month) }
            }

            // The counts, visible to everyone.
            div class="mt-4 space-y-2" {
                @for (_pos, label, count) in &rows {
                    @let pct = if total > 0 { (*count * 100 / total).max(0) } else { 0 };
                    div {
                        div class="flex items-baseline justify-between gap-3 text-sm" {
                            span class="text-ink" { (label) }
                            span class="font-mono text-ink-muted" { (count) }
                        }
                        div class="mt-1 h-1.5 w-full overflow-hidden rounded-full bg-paper-sunken" {
                            div class="h-full rounded-full bg-ink-muted" style={"width:" (pct) "%"} {}
                        }
                    }
                }
            }

            p class="mt-3 font-mono text-[11px] text-ink-muted" {
                (total) " " (i18n::t("took part"))
            }

            // The vote control, or the invitation to sign in.
            div class="mt-4 border-t border-hairline-light pt-4" {
                (control(&action, &viewer, next))
            }

            // The honest limit, always.
            p class="mt-3 text-[11px] leading-snug text-ink-muted" {
                (i18n::t("Counts among verified participants here, not a representative sample."))
            }
        }
    }
}

/// Approval month by month: one bar per month, its height the number who took
/// part, split into approve / disapprove / no-opinion. Counts over time, not a
/// rate, and monochrome, for the same reasons as the panel. Hidden until there
/// are two months to compare, so it stays out of the way until it has something
/// to show.
pub fn history_chart(entries: &[db::approvals::MonthTally]) -> Markup {
    if entries.len() < 2 {
        return html! {};
    }
    // Arrives newest-first; a timeline reads oldest-to-newest.
    let mut months: Vec<&db::approvals::MonthTally> = entries.iter().collect();
    months.reverse();
    let max = months.iter().map(|m| m.total()).max().unwrap_or(1).max(1);

    let slot = 56.0_f64;
    let bar_w = 30.0_f64;
    let chart_h = 96.0_f64;
    let width = slot * months.len() as f64;
    let height = chart_h + 30.0;

    html! {
        section class="mb-8" {
            (crate::ui::section_header(i18n::t("Approval over time"), None))
            div class="op-card overflow-x-auto p-5" {
                svg viewBox={"0 0 " (width) " " (height)}
                    class="h-40 w-full min-w-[240px]" preserveAspectRatio="xMidYMax meet"
                    role="img" aria-label=(i18n::t("Approval over time")) {
                    line x1="0" y1=(chart_h) x2=(width) y2=(chart_h)
                         class="text-hairline" stroke="currentColor" stroke-width="1" {}
                    @for (i, m) in months.iter().enumerate() {
                        @let total = m.total();
                        @let full = (total as f64 / max as f64) * (chart_h - 20.0);
                        @let x = i as f64 * slot + (slot - bar_w) / 2.0;
                        // Segments stack from the baseline up: approve, then
                        // disapprove, then no-opinion, dark to light.
                        @let seg = |count: i64| if total > 0 { (count as f64 / total as f64) * full } else { 0.0 };
                        @let h_a = seg(m.approve);
                        @let h_d = seg(m.disapprove);
                        @let h_n = full - h_a - h_d;
                        rect x=(x) y=(chart_h - h_a) width=(bar_w) height=(h_a)
                             class="text-ink" fill="currentColor" {}
                        rect x=(x) y=(chart_h - h_a - h_d) width=(bar_w) height=(h_d)
                             class="text-ink-muted" fill="currentColor" {}
                        rect x=(x) y=(chart_h - full) width=(bar_w) height=(h_n)
                             class="text-hairline" fill="currentColor" {}
                        text x=(x + bar_w / 2.0) y=(chart_h - full - 5.0) text-anchor="middle"
                             class="text-ink-muted" fill="currentColor"
                             style="font:600 11px ui-monospace,monospace" {
                            (total)
                        }
                        text x=(x + bar_w / 2.0) y=(chart_h + 20.0) text-anchor="middle"
                             class="text-ink-muted" fill="currentColor"
                             style="font:11px ui-monospace,monospace" {
                            (i18n::month_abbr(m.period.month()))
                        }
                    }
                }
                // Which shade is which, since the bars are monochrome.
                p class="mt-3 flex flex-wrap gap-x-4 gap-y-1 font-mono text-[11px] text-ink-muted" {
                    span { span class="mr-1 inline-block h-2 w-2 rounded-sm bg-ink" {} (i18n::t("Approve")) }
                    span { span class="mr-1 inline-block h-2 w-2 rounded-sm bg-ink-muted" {} (i18n::t("Disapprove")) }
                    span { span class="mr-1 inline-block h-2 w-2 rounded-sm bg-hairline" {} (i18n::t("No opinion")) }
                }
            }
        }
    }
}

fn control(action: &str, viewer: &Viewer, next: &str) -> Markup {
    match viewer {
        Viewer::Anonymous => html! {
            a href="/login"
              class="inline-flex items-center gap-1.5 rounded-lg border border-hairline px-3 py-1.5 text-[12px] font-semibold text-ink-muted transition-colors hover:border-accent hover:text-accent" {
                (i18n::t("Log in to take part"))
            }
        },
        Viewer::Voted(pos) => {
            let label = match *pos {
                APPROVE => i18n::t("Approve"),
                DISAPPROVE => i18n::t("Disapprove"),
                _ => i18n::t("No opinion"),
            };
            html! {
                p class="text-[12px] text-ink-muted" {
                    (i18n::t("You said")) ": "
                    span class="font-semibold text-ink" { (label) }
                }
            }
        }
        Viewer::CanVote => html! {
            // One form, three choices. HTMX swaps the whole panel with the
            // updated one; without JavaScript the entity page reloads via `next`.
            form method="post" action=(action) hx-post=(action) hx-target={"#" (dom_target(action))} hx-swap="outerHTML"
                 class="flex flex-wrap gap-2" {
                input type="hidden" name="next" value=(next);
                @for (choice, label) in [("approve", i18n::t("Approve")), ("disapprove", i18n::t("Disapprove")), ("no_opinion", i18n::t("No opinion"))] {
                    button type="submit" name="choice" value=(choice)
                        class="inline-flex items-center rounded-lg border border-hairline px-3 py-1.5 text-[12px] font-semibold text-ink transition-colors hover:border-accent hover:bg-accent-tint hover:text-accent" {
                        (label)
                    }
                }
            }
        },
    }
}

/// The panel's dom id, derived from the action path `/approve/{type}/{id}` so the
/// HTMX form targets the panel it lives in.
fn dom_target(action: &str) -> String {
    // action is "/approve/<type>/<id>"
    let rest = action.strip_prefix("/approve/").unwrap_or_default();
    format!("approval-{}", rest.replace('/', "-"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn month(y: i32, m: u32, a: i64, d: i64, n: i64) -> db::approvals::MonthTally {
        db::approvals::MonthTally {
            period: NaiveDate::from_ymd_opt(y, m, 1).unwrap(),
            approve: a,
            disapprove: d,
            no_opinion: n,
        }
    }

    #[test]
    fn the_history_chart_needs_two_months_to_draw() {
        // Nothing to compare yet: the chart stays out of the way.
        assert!(history_chart(&[]).into_string().is_empty());
        assert!(history_chart(&[month(2026, 7, 3, 1, 0)])
            .into_string()
            .is_empty());
    }

    #[test]
    fn the_history_chart_draws_a_bar_per_month() {
        let svg = history_chart(&[month(2026, 8, 5, 2, 1), month(2026, 7, 3, 1, 0)]).into_string();
        // Assert on the language-independent SVG structure, not the localized
        // heading (a unit test has no active locale set).
        assert!(svg.contains("<svg"));
        // Three stacked segments per month, two months.
        assert_eq!(svg.matches("<rect").count(), 6);
        // A month with no participants would divide by zero; the guard keeps the
        // bars flat instead. Confirm that path still renders.
        let with_empty =
            history_chart(&[month(2026, 8, 0, 0, 0), month(2026, 7, 3, 1, 0)]).into_string();
        assert!(with_empty.contains("<svg"));
    }
}
