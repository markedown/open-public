use maud::{html, Markup};

use crate::i18n;

pub mod approval;
pub mod background;
pub mod badge;
pub mod breadcrumb;
pub mod button;
pub mod captcha;
pub mod citation;
pub mod election;
pub mod event;
pub mod follow;
pub mod hemicycle;
pub mod layout;
pub mod news;
pub mod outlet;
pub mod pagination;
pub mod poll_widget;
pub mod references;
pub mod search;
pub mod seat_bar;
pub mod statement;
pub mod timeline_entry;
pub mod translated;

/// A section heading: a plain sentence-case title, often phrased as the
/// question the section answers, with an optional right-aligned action (a "see
/// all" link, an admin add link). Kept in one place so every section on every
/// page reads the same.
pub fn section_header(title: &str, action: Option<Markup>) -> Markup {
    html! {
        div class="mb-5 flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1" {
            h2 class="text-[26px] font-extrabold leading-tight tracking-tight text-ink sm:text-[30px]" { (title) }
            @if let Some(a) = action { (a) }
        }
    }
}

/// The standard "see all" link for a section header, pointing at a fuller
/// index page.
pub fn see_all_link(href: &str) -> Markup {
    html! {
        a href=(href) class="op-link shrink-0 text-[16px] font-semibold" {
            (i18n::t("See all"))
        }
    }
}

/// A page title block for an index page: a large title, an optional meta line
/// (usually a count), and an optional right-aligned action (an add/propose
/// button). Kept in one place so every index page opens the same way.
pub fn page_header(title: &str, meta: Option<Markup>, action: Option<Markup>) -> Markup {
    html! {
        header class="mb-8 flex flex-wrap items-end justify-between gap-4" {
            div {
                h1 class="text-4xl font-extrabold leading-[1.05] tracking-tight text-ink sm:text-5xl" { (title) }
                @if let Some(m) = meta {
                    p class="mt-2 text-lg text-ink-muted" { (m) }
                }
            }
            @if let Some(a) = action { (a) }
        }
    }
}

/// Up to two initials from a name's first and last word, for the initials
/// square shown on person rows.
pub fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    let first = words.first().and_then(|w| w.chars().next());
    let last = words
        .last()
        .filter(|_| words.len() > 1)
        .and_then(|w| w.chars().next());
    first.into_iter().chain(last).collect()
}

/// A party colour narrowed to something that cannot escape the attribute it is
/// written into.
///
/// Colours come from the database as free text, and one of them is written into
/// an SVG built by string concatenation, which does not go through the template
/// escaping the rest of the interface relies on. Nothing user-facing sets a
/// colour today, so this closes no known hole; it makes the assumption explicit
/// instead of leaving a trusted-by-accident value one careless edit away from
/// mattering.
///
/// Anything that is not a plain `#rgb` or `#rrggbb` becomes the fallback.
pub fn css_color(color: Option<&str>, fallback: &'static str) -> String {
    match color {
        Some(c)
            if (c.len() == 4 || c.len() == 7)
                && c.starts_with('#')
                && c[1..].bytes().all(|b| b.is_ascii_hexdigit()) =>
        {
            c.to_string()
        }
        _ => fallback.to_string(),
    }
}
