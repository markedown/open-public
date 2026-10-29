//! A chamber's composition: one dot per seat, grouped by party, next to a list
//! of the parties with their seat counts. Party colours appear only here, inside
//! the data element, never in the interface chrome (see DESIGN.md).

use maud::{html, Markup};

use crate::i18n;

/// Two neutral greys, alternated between consecutive parties that publish no
/// colour, so neighbouring blocks stay countable without inventing a colour for
/// either.
const UNCOLOURED: [&str; 2] = ["#b7bec6", "#d5dae0"];
/// The ring drawn for a seat whose member belongs to no party.
const RING: &str = "#9aa3ad";
/// How many parties the list names before folding the rest into one line.
const LISTED: usize = 9;

/// The dots and the party list. `seats` are the parties, `independents` the
/// members with no party (hollow dots), and `vacant` the seats sitting empty
/// between elections (dotted rings). Each listed party links to its page.
pub fn composition(
    seats: &[db::country::SeatCount],
    independents: i64,
    vacant: Option<i64>,
    country_slug: &str,
) -> Markup {
    let total: i64 =
        seats.iter().map(|s| s.seats).sum::<i64>() + independents + vacant.unwrap_or(0);
    // Large chambers (several hundred seats) get smaller dots so the block stays
    // a readable size.
    let dot = if total > 400 { "size-2.5" } else { "size-3.5" };
    let mut grey = 0;
    let fills: Vec<String> = seats
        .iter()
        .map(|s| match s.color.as_deref() {
            Some(_) => super::css_color(s.color.as_deref(), UNCOLOURED[0]),
            None => {
                grey += 1;
                UNCOLOURED[(grey - 1) % 2].to_string()
            }
        })
        .collect();
    let label = summary(seats, independents, vacant);
    let rest: i64 = seats.iter().skip(LISTED).map(|s| s.seats).sum();
    let rest_count = seats.len().saturating_sub(LISTED);
    html! {
        div class="flex flex-wrap items-start gap-x-12 gap-y-6" {
            div role="img" aria-label=(label)
                class="flex max-w-full flex-[0_1_34rem] flex-wrap gap-[5px]" {
                @for (s, fill) in seats.iter().zip(&fills) {
                    @for _ in 0..s.seats {
                        span class={(dot) " rounded-full"} style={"background-color:" (fill)} {}
                    }
                }
                @for _ in 0..independents {
                    span class={(dot) " box-border rounded-full border-2 bg-paper"}
                        style={"border-color:" (RING)} {}
                }
                @for _ in 0..vacant.unwrap_or(0) {
                    span class={(dot) " box-border rounded-full border-2 border-dotted bg-paper"}
                        style={"border-color:" (RING)} {}
                }
            }
            ul class="grid min-w-0 flex-[1_1_20rem] grid-cols-[repeat(auto-fill,minmax(15rem,1fr))] gap-x-6" {
                @for (s, fill) in seats.iter().zip(&fills).take(LISTED) {
                    li class="flex items-baseline gap-2.5 border-b border-hairline-light py-2" {
                        span aria-hidden="true" class="size-3 shrink-0 rounded-full"
                            style={"background-color:" (fill)} {}
                        a href={"/" (country_slug) "/parties/" (s.slug)}
                          class="min-w-0 flex-1 text-[16px] text-ink hover:underline" {
                            (s.name)
                        }
                        span class="text-[16px] font-bold text-ink" { (s.seats) }
                    }
                }
                @if rest_count > 0 {
                    li class="flex items-baseline gap-2.5 border-b border-hairline-light py-2" {
                        span aria-hidden="true" class="size-3 shrink-0 rounded-full bg-hairline-strong" {}
                        a href={"/" (country_slug) "/parties"}
                          class="min-w-0 flex-1 text-[16px] text-ink hover:underline" {
                            (i18n::t("Other parties")) " (" (rest_count) ")"
                        }
                        span class="text-[16px] font-bold text-ink" { (rest) }
                    }
                }
                @if independents > 0 {
                    li class="flex items-baseline gap-2.5 border-b border-hairline-light py-2" {
                        span aria-hidden="true" class="box-border size-3 shrink-0 rounded-full border-2 bg-paper"
                            style={"border-color:" (RING)} {}
                        span class="min-w-0 flex-1 text-[16px] text-ink" { (i18n::t("Independent")) }
                        span class="text-[16px] font-bold text-ink" { (independents) }
                    }
                }
                @if let Some(v) = vacant {
                    li class="flex items-baseline gap-2.5 border-b border-hairline-light py-2" {
                        span aria-hidden="true" class="box-border size-3 shrink-0 rounded-full border-2 border-dotted bg-paper"
                            style={"border-color:" (RING)} {}
                        span class="min-w-0 flex-1 text-[16px] text-ink" { (i18n::t("Vacant")) }
                        span class="text-[16px] font-bold text-ink" { (v) }
                    }
                }
            }
        }
        p class="mt-3 text-[15px] text-ink-muted" {
            (i18n::t("Each dot is one seat. Grey: a party with no documented colour. Hollow: a member of no party."))
        }
    }
}

/// The composition as one sentence, for assistive technology: the dots are a
/// picture of the list beside them.
fn summary(seats: &[db::country::SeatCount], independents: i64, vacant: Option<i64>) -> String {
    let mut parts: Vec<String> = seats
        .iter()
        .map(|s| format!("{} {}", s.name, s.seats))
        .collect();
    if independents > 0 {
        parts.push(format!("{} {independents}", i18n::t("Independent")));
    }
    if let Some(v) = vacant {
        parts.push(format!("{} {v}", i18n::t("Vacant")));
    }
    parts.join(", ")
}
