use maud::{html, Markup};

use crate::i18n;

/// Previous / next controls with the current position. `base` is the page path
/// without a query; navigation appends `?p=N`. Renders nothing for a single
/// page. `page` is 1-based.
pub fn controls(base: &str, page: i64, total_pages: i64) -> Markup {
    if total_pages <= 1 {
        return html! {};
    }
    let link = "border border-ink px-3 py-1.5 text-[11px] font-bold uppercase tracking-wide text-ink transition-colors hover:border-accent hover:text-accent";
    let disabled = "border border-hairline px-3 py-1.5 text-[11px] font-bold uppercase tracking-wide text-hairline";
    html! {
        nav class="mt-8 flex items-center justify-between gap-3" aria-label=(i18n::t("Pagination")) {
            @if page > 1 {
                a href={(base) "?p=" (page - 1)} class=(link) { "‹ " (i18n::t("Previous")) }
            } @else {
                span class=(disabled) { "‹ " (i18n::t("Previous")) }
            }
            span class="font-mono text-xs text-ink-muted" { (page) " / " (total_pages) }
            @if page < total_pages {
                a href={(base) "?p=" (page + 1)} class=(link) { (i18n::t("Next")) " ›" }
            } @else {
                span class=(disabled) { (i18n::t("Next")) " ›" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::controls;

    #[test]
    fn single_page_renders_nothing() {
        assert_eq!(controls("/x", 1, 1).into_string(), "");
    }

    #[test]
    fn middle_page_links_both_ways() {
        let html = controls("/tr/outlet/x", 2, 3).into_string();
        assert!(html.contains(r#"href="/tr/outlet/x?p=1""#)); // previous
        assert!(html.contains(r#"href="/tr/outlet/x?p=3""#)); // next
        assert!(html.contains("2 / 3")); // position
    }

    #[test]
    fn edges_disable_the_unavailable_direction() {
        let first = controls("/x", 1, 3).into_string();
        assert!(!first.contains("?p=0")); // no previous link on page 1
        assert!(first.contains("?p=2")); // but a next link

        let last = controls("/x", 3, 3).into_string();
        assert!(last.contains("?p=2")); // a previous link
        assert!(!last.contains("?p=4")); // no next link on the last page
    }
}
