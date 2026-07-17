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
    html! {
        section class="mb-12" {
            div class="mb-5 flex items-center justify-between gap-3 border-b-2 border-accent pb-2" {
                h2 class="text-xs font-bold uppercase tracking-widest text-ink" { (i18n::t("Statements")) }
                @if let Some(href) = add_href {
                    a href=(href) class="text-[11px] font-bold uppercase tracking-wide text-accent transition-colors hover:underline" {
                        "+ " (i18n::t("Add statement"))
                    }
                }
            }
            @if items.is_empty() {
                p class="text-sm text-ink-muted" { (i18n::t("No statements yet.")) }
            } @else {
                ul class="space-y-6" {
                    @for s in items {
                        li class="border-l-2 border-hairline pl-4" {
                            p class="font-serif text-lg leading-relaxed text-ink" { (s.text_original) }
                            div class="mt-2 flex flex-wrap items-center gap-2 font-mono text-xs text-ink-muted" {
                                @if let Some(d) = s.stated_at { span { (fmt::date(Some(d))) } }
                                @if let Some(ref o) = s.outlet { span { (o) } }
                                @if s.is_paraphrase {
                                    span class="border border-hairline px-1.5 py-0.5 uppercase tracking-wide" {
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
