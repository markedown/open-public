//! Rendering content that may be a translation, with the original always one
//! click away. On a political-data platform the sourced original is
//! authoritative and a translation is a reading aid, so translated content
//! keeps a no-JavaScript disclosure that reveals the original text.

use maud::{html, Markup};

use crate::i18n;

/// A paragraph of prose in the reader's language. When `original` is `Some`, the
/// shown text is a translation and a "show original" disclosure reveals the
/// source text; when `None`, the text is itself the original and nothing extra
/// is shown.
pub fn prose(text: &str, original: Option<&str>) -> Markup {
    html! {
        p class="max-w-prose text-[17px] leading-relaxed text-ink" { (text) }
        (original_disclosure(original))
    }
}

/// The "show original" disclosure, shown only when `original` is present (i.e.
/// the accompanying text is a translation). Reusable for any translated field.
pub fn original_disclosure(original: Option<&str>) -> Markup {
    html! {
        @if let Some(orig) = original {
            details class="mt-1 max-w-prose" {
                summary class="cursor-pointer list-none text-[11px] font-medium uppercase tracking-wide text-ink-muted transition-colors hover:text-accent [&::-webkit-details-marker]:hidden" {
                    (i18n::t("Show original"))
                }
                p class="mt-2 text-[15px] leading-relaxed text-ink-muted" { (orig) }
            }
        }
    }
}
