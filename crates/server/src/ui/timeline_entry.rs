use maud::{html, Markup};

pub struct Entry {
    pub kind: String,
    pub title: String,
    pub subtitle: String,
    pub date_range: String,
    /// Internal navigation for the title (e.g. a party or person page). When
    /// set, the title is a link; navigation stays inside the site.
    pub link_href: Option<String>,
    /// The underlying source, shown as a subtle marker on the meta line rather
    /// than by making the whole fact link away.
    pub source_url: Option<String>,
}

pub fn timeline_list(entries: impl Iterator<Item = Entry>) -> Markup {
    html! {
        ol class="ml-2 border-l-2 border-hairline" {
            @for entry in entries {
                (timeline_entry(entry))
            }
        }
    }
}

fn timeline_entry(entry: Entry) -> Markup {
    html! {
        li class="relative pb-7 pl-7 last:pb-1" {
            span aria-hidden="true" class="absolute -left-[9px] top-1.5 size-4 rounded-full bg-ink" {}
            div class="text-[15px] text-ink-muted" {
                (entry.date_range)
                @if !entry.kind.is_empty() {
                    " · " (entry.kind)
                }
            }
            // Navigation-first: the title links to its own page, not to a source.
            @if let Some(href) = &entry.link_href {
                a href=(href) class="op-link block text-[20px] font-bold" { (entry.title) }
            } @else {
                p class="text-[20px] font-bold text-ink" { (entry.title) }
            }
            @if !entry.subtitle.is_empty() || entry.source_url.is_some() {
                div class="mt-0.5 flex flex-wrap items-center gap-x-2 text-[16px] text-ink-muted" {
                    @if !entry.subtitle.is_empty() {
                        span { (entry.subtitle) }
                    }
                    @if let Some(url) = &entry.source_url {
                        (crate::ui::citation::source_marker(url))
                    }
                }
            }
        }
    }
}
