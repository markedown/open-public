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
}
