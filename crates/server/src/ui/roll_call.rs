//! Recorded votes: the pill for one member's choice, the stacked split of a
//! vote, and a person's list of votes. All of it is monochrome, because a vote is
//! not good or bad, and colour here would say it was.

use maud::{html, Markup};

use crate::fmt;
use crate::i18n;

/// The label for a choice.
pub fn choice_label(choice: &str) -> &'static str {
    match choice {
        "yes" => i18n::t("Yes"),
        "no" => i18n::t("No"),
        "abstain" => i18n::t("Abstained"),
        _ => i18n::t("Did not vote"),
    }
}

/// One member's choice as a pill. The fill differs so the column can be scanned,
/// and the word is always there, so nothing depends on seeing the difference.
pub fn choice_pill(choice: &str) -> Markup {
    let class = match choice {
        "yes" => "bg-ink text-paper border-ink",
        "no" => "bg-paper text-ink border-ink",
        "abstain" => "bg-hairline text-ink border-hairline",
        _ => "bg-paper text-ink-muted border-dashed border-hairline-strong",
    };
    html! {
        span class={"inline-block shrink-0 rounded-full border px-3 py-0.5 text-[15px] font-semibold " (class)} {
            (choice_label(choice))
        }
    }
}

/// The share of a vote each choice took, as one stacked bar. The numbers beside
/// it say the same thing for anyone who cannot see it.
pub fn stacked_bar(yes: i64, no: i64, abstain: i64, absent: i64) -> Markup {
    let total = (yes + no + abstain + absent).max(1) as f64;
    let width = |n: i64| format!("width:{:.2}%", n as f64 * 100.0 / total);
    html! {
        div aria-hidden="true" class="flex h-3 overflow-hidden rounded-full bg-paper-sunken" {
            div class="bg-ink" style=(width(yes)) {}
            div class="bg-[#7d8590]" style=(width(no)) {}
            div class="bg-hairline-strong" style=(width(abstain)) {}
        }
    }
}

/// What the colours in the bars mean.
pub fn legend() -> Markup {
    html! {
        p class="mt-3 flex flex-wrap gap-x-5 gap-y-1 text-[15px] text-ink-muted" {
            span class="inline-flex items-center gap-2" {
                span aria-hidden="true" class="size-3 rounded-full bg-ink" {} (i18n::t("Yes"))
            }
            span class="inline-flex items-center gap-2" {
                span aria-hidden="true" class="size-3 rounded-full bg-[#7d8590]" {} (i18n::t("No"))
            }
            span class="inline-flex items-center gap-2" {
                span aria-hidden="true" class="size-3 rounded-full bg-hairline-strong" {} (i18n::t("Abstained"))
            }
            span class="inline-flex items-center gap-2" {
                span aria-hidden="true" class="size-3 rounded-full border border-hairline-strong bg-paper-sunken" {} (i18n::t("Did not vote"))
            }
        }
    }
}

/// The honest limit of the record, said wherever absence is shown.
pub fn absence_note() -> Markup {
    html! {
        p class="mt-3 max-w-prose text-[15px] text-ink-muted" {
            (i18n::t("The record says that a member did not vote, not why. Illness, official travel and agreements between groups all look the same here."))
        }
    }
}

/// A person's votes as rows, each linking to the vote.
pub fn person_list(country: &str, votes: &[db::roll_calls::PersonVote]) -> Markup {
    html! {
        ul class="divide-y divide-hairline-light" {
            @for v in votes {
                li class="flex items-start gap-3 py-3" {
                    (choice_pill(&v.choice))
                    div class="min-w-0" {
                        a href={"/" (country) "/vote/" (v.slug)} class="op-link text-[17px] font-semibold" { (v.title) }
                        div class="mt-0.5 flex flex-wrap items-center gap-x-2 text-[15px] text-ink-muted" {
                            span { (fmt::date(Some(v.held_on))) }
                            @if db::roll_calls::differs_from_party(v) {
                                span class="rounded border border-hairline px-1.5 font-semibold text-ink" {
                                    (i18n::t("Differs from most of their party"))
                                }
                            }
                            (crate::ui::citation::source_marker(&v.source_url))
                        }
                    }
                }
            }
        }
    }
}

/// The section on a person's page: their record in one line, then the latest
/// votes. Hidden for someone with no recorded votes.
pub fn person_section(
    country: &str,
    person_slug: &str,
    record: db::roll_calls::PersonRecord,
    latest: &[db::roll_calls::PersonVote],
) -> Markup {
    if latest.is_empty() {
        return html! {};
    }
    let all = format!("/{country}/people/{person_slug}/votes");
    html! {
        section class="mb-12" {
            (crate::ui::section_header(
                i18n::t("How did they vote?"),
                (record.total > latest.len() as i64).then(|| crate::ui::see_all_link(&all)),
            ))
            (record_line(record))
            (person_list(country, latest))
            (absence_note())
        }
    }
}

/// A person's record as one plain sentence of numbers.
pub fn record_line(record: db::roll_calls::PersonRecord) -> Markup {
    html! {
        p class="mb-2 text-[17px] text-ink" {
            (record.total) " " (i18n::t("recorded votes"))
            " · " (record.total - record.absent) " " (i18n::t("took part"))
            " · " (record.against_party) " " (i18n::t("differing from most of their party"))
        }
    }
}
