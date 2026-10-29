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
        section class="mb-12" {
            (crate::ui::section_header(i18n::t("Background"), None))
            @if !education.is_empty() {
                div class="mb-6" {
                    h3 class="mb-3 text-[18px] font-bold text-ink" {
                        (i18n::t("Education"))
                    }
                    ul class="space-y-3" {
                        @for e in education {
                            li class="border-b border-hairline-light pb-3" {
                                div class="text-[17px] font-semibold text-ink" {
                                    (e.institution)
                                    @if let Some(ref d) = e.degree {
                                        span class="text-ink-muted" { " · " (d) }
                                    }
                                }
                                div class="mt-1 flex flex-wrap items-center gap-2 text-[15px] text-ink-muted" {
                                    @if e.start_date.is_some() || e.end_date.is_some() {
                                        span { (fmt::date_range(e.start_date, e.end_date)) }
                                    }
                                    @if let Some(ref f) = e.field { span { (f) } }
                                    a href=(e.source_url) target="_blank" rel="noopener noreferrer"
                                      class="op-link" {
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
                        h3 class="mb-2 text-[18px] font-bold text-ink" {
                            (label)
                        }
                        div class="flex flex-wrap gap-2" {
                            @for a in &vals {
                                a href=(a.source_url) target="_blank" rel="noopener noreferrer"
                                  class="inline-flex items-center rounded-full border border-hairline bg-paper px-3 py-1 text-[15px] text-ink transition-colors hover:border-ink" {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_background_renders_nothing() {
        assert_eq!(section(&[], &[]).into_string(), "");
    }
}
