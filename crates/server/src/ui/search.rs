use maud::{html, Markup};

use crate::i18n;

/// Case- and accent-insensitive substring match, for filtering short lists
/// (elections, alliances) in the handler where a dedicated SQL query is not
/// worth it. Both sides fold through the slug transliterator, so matching reads
/// the same as the people and party search (accent- and case-insensitive).
pub fn matches(name: &str, query: &str) -> bool {
    let q = query.trim();
    q.is_empty() || domain::slug::slugify(name).contains(&domain::slug::slugify(q))
}

/// A search box that filters a list in place. `action` is the list page's own
/// URL and `target` is the CSS id selector (e.g. `#people-results`) of the
/// container to replace. Typing re-queries the page via HTMX and swaps that same
/// container from the response; without JavaScript, submitting reloads the page
/// with the `?q=` filter applied. `value` prefills the current query.
pub fn bar(action: &str, target: &str, value: &str) -> Markup {
    html! {
        form method="get" action=(action) role="search" class="mb-8" {
            input type="search" name="q" value=(value) autocomplete="off"
                placeholder=(i18n::t("Search"))
                hx-get=(action) hx-trigger="keyup changed delay:250ms, search"
                hx-target=(target) hx-select=(target) hx-swap="outerHTML" hx-push-url="true"
                class="w-full border border-hairline bg-paper-raised px-3 py-2.5 text-sm text-ink focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent";
        }
    }
}
