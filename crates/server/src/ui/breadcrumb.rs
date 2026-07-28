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
        nav class="mb-8 flex flex-wrap items-center gap-x-1.5 gap-y-1 text-sm text-ink-muted" aria-label="Breadcrumb" {
            @for (i, item) in items.iter().enumerate() {
                @if i > 0 {
                    span { "/" }
                }
                @if let Some(ref href) = item.href {
                    a href=(href) class="transition-colors hover:text-accent" {
                        (item.label)
                    }
                } @else {
                    span class="min-w-0 break-words text-ink" { (item.label) }
                }
            }
        }
    }
}
