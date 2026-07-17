use maud::{html, Markup};

pub struct Crumb {
    pub label: String,
    pub href: Option<String>,
}

pub fn breadcrumbs(items: &[Crumb]) -> Markup {
    if items.is_empty() {
        return html! {};
    }
    html! {
        nav class="mb-8 flex items-center gap-1.5 text-sm text-ink-muted" aria-label="Breadcrumb" {
            @for (i, item) in items.iter().enumerate() {
                @if i > 0 {
                    span { "/" }
                }
                @if let Some(ref href) = item.href {
                    a href=(href) class="transition-colors hover:text-accent" {
                        (item.label)
                    }
                } @else {
                    span class="text-ink" { (item.label) }
                }
            }
        }
    }
}
