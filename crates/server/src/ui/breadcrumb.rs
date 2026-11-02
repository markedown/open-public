use maud::{html, Markup};

pub struct Crumb {
    pub label: String,
    pub href: Option<String>,
}

/// The trail with its own bottom margin, for pages that open without a hero
/// band.
pub fn breadcrumbs(items: &[Crumb]) -> Markup {
    if items.is_empty() {
        return html! {};
    }
    html! {
        div class="mb-6" { (trail(items)) }
        // The machine-readable twin of the trail hangs off `breadcrumbs` and
        // not off `trail`: a page that rendered both would otherwise carry the
        // same list twice, and search engines read both.
        (crate::ui::jsonld::breadcrumb_script(items))
    }
}

/// The trail alone, for placing inside a hero band.
pub fn trail(items: &[Crumb]) -> Markup {
    if items.is_empty() {
        return html! {};
    }
    html! {
        nav class="flex flex-wrap items-center gap-x-1.5 gap-y-1 text-[15px] text-ink-muted" aria-label="Breadcrumb" {
            @for (i, item) in items.iter().enumerate() {
                @if i > 0 {
                    span aria-hidden="true" { "›" }
                }
                @if let Some(ref href) = item.href {
                    a href=(href) class="op-link" { (item.label) }
                } @else {
                    span class="min-w-0 break-words" { (item.label) }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_crumbs_render_nothing() {
        assert_eq!(breadcrumbs(&[]).into_string(), "");
        assert_eq!(trail(&[]).into_string(), "");
    }

    #[test]
    fn the_trail_links_every_step_but_the_one_the_visitor_is_on() {
        let crumbs = [
            Crumb {
                label: "Test Ulke".to_string(),
                href: Some("/tr".to_string()),
            },
            Crumb {
                label: "Test Partisi".to_string(),
                href: None,
            },
        ];
        let body = trail(&crumbs).into_string();
        assert!(body.contains(r#"href="/tr""#));
        // The current page is text, not a link back to itself.
        assert!(!body.contains(r#"href="Test Partisi""#));
        assert!(body.contains("Test Partisi"));
        // One separator between the two steps, hidden from a screen reader.
        assert_eq!(
            body.matches(r#"<span aria-hidden="true">›</span>"#).count(),
            1
        );

        // The machine-readable twin belongs to `breadcrumbs`, so a page rendering
        // both cannot hand a crawler the same list twice.
        assert!(!body.contains("BreadcrumbList"));
        assert!(breadcrumbs(&crumbs)
            .into_string()
            .starts_with(r#"<div class="mb-6">"#));
    }
}
