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
        div class="space-y-0" {
            @for entry in entries {
                (timeline_entry(entry))
            }
        }
    }
}

fn timeline_entry(entry: Entry) -> Markup {
    html! {
        div class="relative ml-2 border-l border-hairline pb-6 pl-8 last:border-l-0 last:pb-0" {
            div class="absolute left-0 top-1.5 h-2.5 w-2.5 -translate-x-1/2 rounded-full border-2 border-accent bg-paper-raised" {}

            @if !entry.kind.is_empty() {
                div class="mb-0.5" {
                    span class="font-mono text-[11px] font-medium uppercase tracking-wider text-ink-muted" {
                        (entry.kind)
                    }
                }
            }

            // Navigation-first: the title links to its own page, not to a source.
            @if let Some(href) = &entry.link_href {
                a href=(href) class="block text-sm font-medium text-ink transition-colors hover:text-accent" {
                    (entry.title)
                }
            } @else {
                p class="text-sm font-medium text-ink" { (entry.title) }
            }

            @if !entry.subtitle.is_empty() {
                p class="mt-0.5 text-xs text-ink-muted" { (entry.subtitle) }
            }

            div class="mt-1 flex flex-wrap items-center gap-2 font-mono text-xs text-ink-muted" {
                span { (entry.date_range) }
                @if let Some(url) = &entry.source_url {
                    (crate::ui::citation::source_marker(url))
                }
            }
        }
    }
}
