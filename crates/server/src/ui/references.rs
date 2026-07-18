use maud::{html, Markup};

use crate::i18n;

/// An "External references" section: outbound links and credits for a record.
/// Currently the Wikidata item and the photo licence. Renders nothing when
/// there is nothing to show.
pub fn references(wikidata_id: Option<&str>, photo_license: Option<&str>) -> Markup {
    if wikidata_id.is_none() && photo_license.is_none() {
        return html! {};
    }
    html! {
        section class="mb-8" {
            (crate::ui::section_header(i18n::t("References"), None))
            ul class="space-y-1.5 text-sm text-ink-muted" {
                @if let Some(qid) = wikidata_id {
                    li {
                        a href={"https://www.wikidata.org/wiki/" (qid)}
                          class="text-accent hover:underline" target="_blank" rel="noopener noreferrer" {
                            "Wikidata · " (qid)
                        }
                    }
                }
                @if let Some(license) = photo_license {
                    li { (i18n::t("Photo")) ": " (license) }
                }
            }
        }
    }
}
