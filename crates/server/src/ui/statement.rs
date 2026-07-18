use domain::models::Statement;
use maud::{html, Markup};

use crate::fmt;
use crate::i18n;

/// The "Statements" section on a person or party page: short sourced excerpts or
/// paraphrases. `add_href`, set only for admins, surfaces an add affordance and
/// keeps the section visible when empty.
pub fn statement_section(items: &[Statement], add_href: Option<&str>) -> Markup {
    if items.is_empty() && add_href.is_none() {
        return html! {};
    }
    let add = add_href.map(|href| html! {
        a href=(href) class="text-[12px] font-semibold text-accent transition-colors hover:underline" {
            "+ " (i18n::t("Add statement"))
        }
    });
    html! {
        section class="mb-8" {
            (crate::ui::section_header(i18n::t("Statements"), add))
            @if items.is_empty() {
                p class="text-sm text-ink-muted" { (i18n::t("No statements yet.")) }
            } @else {
                ul class="space-y-5" {
                    @for s in items {
                        li class="border-l-2 border-accent/25 pl-4" {
                            p class="text-[15px] leading-relaxed text-ink" { (s.text_original) }
                            div class="mt-2 flex flex-wrap items-center gap-2 font-mono text-xs text-ink-muted" {
                                @if let Some(d) = s.stated_at { span { (fmt::date(Some(d))) } }
                                @if let Some(ref o) = s.outlet { span { (o) } }
                                @if s.is_paraphrase {
                                    span class="rounded border border-hairline px-1.5 py-0.5 uppercase tracking-wide" {
                                        (i18n::t("paraphrase"))
                                    }
                                }
                                a href=(s.url) target="_blank" rel="noopener noreferrer"
                                  class="uppercase tracking-wide text-ink-muted/70 transition-colors hover:text-accent" {
                                    (i18n::t("source")) " ↗"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
