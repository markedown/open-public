//! A seat-composition bar and its legend, for a legislature or one of its
//! chambers. Party colours appear only here, inside the data element, never in
//! the interface chrome (see DESIGN.md).

use maud::{html, Markup};

use crate::i18n;

/// Neutral grey for independents, deliberately not any party colour.
const INDEPENDENT_COLOR: &str = "oklch(62% 0.01 70)";
/// A diagonal hairline hatch reads as "empty" rather than any party's colour,
/// and stays visible in both light and dark themes.
const VACANT_FILL: &str =
    "background-image:repeating-linear-gradient(45deg,transparent 0 3px,rgba(115,115,115,0.5) 3px 4px)";

/// A proportional seat bar followed by a legend. `seats` are the parties (each a
/// coloured segment), `independents` a neutral segment, and `vacant` an optional
/// hatched segment for seats sitting empty between elections. Party chips link
/// to the party page.
pub fn composition(
    seats: &[db::country::SeatCount],
    independents: i64,
    vacant: Option<i64>,
    country_slug: &str,
) -> Markup {
    html! {
        // Proportional flex segments fill the bar exactly; integer width
        // percentages truncated small parties to 0 and left a gap.
        div class="mb-4 flex h-7 w-full overflow-hidden rounded-md border border-hairline" {
            @for s in seats {
                div class="h-full border-r border-r-paper-raised last:border-r-0"
                    style={"flex:" (s.seats) " 0 0; background-color:" (s.color.as_deref().unwrap_or("#171717"))}
                    title={(s.name) " · " (s.seats)} {}
            }
            @if independents > 0 {
                div class="h-full"
                    style={"flex:" (independents) " 0 0; background-color:" (INDEPENDENT_COLOR)}
                    title={(i18n::t("Independent")) " · " (independents)} {}
            }
            @if let Some(v) = vacant {
                div class="h-full"
                    style={"flex:" (v) " 0 0; " (VACANT_FILL)}
                    title={(i18n::t("Vacant")) " · " (v)} {}
            }
        }
        div class="flex flex-wrap gap-x-5 gap-y-2" {
            @for s in seats {
                a href={"/" (country_slug) "/parties/" (s.slug)} class="flex items-center gap-2 text-sm transition-opacity hover:opacity-80" {
                    span class="h-3 w-3 shrink-0 rounded-sm border border-hairline"
                        style={"background-color:" (s.color.as_deref().unwrap_or("#171717"))} {}
                    span class="font-mono text-xs font-semibold text-ink" {
                        (s.short_name.as_deref().unwrap_or(&s.name))
                    }
                    span class="font-mono text-xs text-ink-muted" { (s.seats) }
                }
            }
            @if independents > 0 {
                span class="flex items-center gap-2 text-sm" {
                    span class="h-3 w-3 shrink-0 rounded-sm border border-hairline"
                        style={"background-color:" (INDEPENDENT_COLOR)} {}
                    span class="font-mono text-xs font-semibold text-ink" { (i18n::t("Independent")) }
                    span class="font-mono text-xs text-ink-muted" { (independents) }
                }
            }
            @if let Some(v) = vacant {
                span class="flex items-center gap-2 text-sm" {
                    span class="h-3 w-3 shrink-0 rounded-sm border border-hairline" style=(VACANT_FILL) {}
                    span class="font-mono text-xs font-semibold text-ink" { (i18n::t("Vacant")) }
                    span class="font-mono text-xs text-ink-muted" { (v) }
                }
            }
        }
    }
}
