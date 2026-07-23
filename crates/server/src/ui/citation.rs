use maud::{html, Markup};

use crate::i18n;

/// A subtle source marker placed next to a sourced fact on a detail page.
/// Navigation stays internal (the fact's title links to its own page); this
/// small, muted link is how a reader reaches the underlying source without it
/// dominating the row. Always present where a fact has a source, never
/// hover-only: it is the product's credibility, kept quiet rather than loud.
pub fn source_marker(url: &str) -> Markup {
    html! {
        a href=(url)
           class="inline-flex items-center gap-0.5 text-[10px] uppercase tracking-wide text-ink-muted/70 transition-colors hover:text-accent"
           target="_blank"
           rel="noopener noreferrer" {
            (i18n::t("source"))
            span aria-hidden="true" { " ↗" }
        }
    }
}
