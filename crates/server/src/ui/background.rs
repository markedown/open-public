//! The "Background" section on a person page: sourced biographical facts,
//! education and attributes (occupation, ideology, religion). Each item carries
//! a source link, and the whole section is hidden when there is nothing to show,
//! so it stays invisible to readers until a person is enriched.

use domain::models::{Education, PersonAttribute};
use maud::{html, Markup};

use crate::{fmt, i18n};

pub fn section(education: &[Education], attributes: &[PersonAttribute]) -> Markup {
    if education.is_empty() && attributes.is_empty() {
        return html! {};
    }
    html! {
        section class="mb-8" {
            (crate::ui::section_header(i18n::t("Background"), None))
            @if !education.is_empty() {
                div class="mb-6" {
                    h3 class="mb-3 text-[11px] font-bold uppercase tracking-wide text-ink-muted" {
                        (i18n::t("Education"))
                    }
                    ul class="space-y-3" {
                        @for e in education {
                            li class="border-l-2 border-hairline pl-4" {
                                div class="text-sm font-medium text-ink" {
                                    (e.institution)
                                    @if let Some(ref d) = e.degree {
                                        span class="text-ink-muted" { " · " (d) }
                                    }
                                }
                                div class="mt-1 flex flex-wrap items-center gap-2 font-mono text-xs text-ink-muted" {
                                    @if e.start_date.is_some() || e.end_date.is_some() {
                                        span { (fmt::date_range(e.start_date, e.end_date)) }
                                    }
                                    @if let Some(ref f) = e.field { span { (f) } }
                                    a href=(e.source_url) target="_blank" rel="noopener noreferrer"
                                      class="uppercase tracking-wide text-ink-muted/70 transition-colors hover:text-accent" {
                                        (i18n::t("source")) " ↗"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            @for (kind, label) in [
                ("occupation", i18n::t("Occupation")),
                ("ideology", i18n::t("Ideology")),
                ("religion", i18n::t("Religion")),
            ] {
                @let vals: Vec<&PersonAttribute> = attributes.iter().filter(|a| a.kind == kind).collect();
                @if !vals.is_empty() {
                    div class="mb-4" {
                        h3 class="mb-2 text-[11px] font-bold uppercase tracking-wide text-ink-muted" {
                            (label)
                        }
                        div class="flex flex-wrap gap-2" {
                            @for a in &vals {
                                a href=(a.source_url) target="_blank" rel="noopener noreferrer"
                                  class="inline-flex items-center rounded-md border border-hairline bg-paper-raised px-2.5 py-1 text-xs text-ink transition-colors hover:border-accent hover:text-accent" {
                                    (a.value)
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
