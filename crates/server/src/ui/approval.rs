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
