use maud::{html, Markup};

/// A party chip: the abbreviation as a hard-edged block filled with the party's
/// organization color, or a black-outlined block when there is no color. Text
/// color flips with the fill's luminance so it stays legible. A plain span, safe
/// inside a link. Color only ever appears inside this data element.
pub fn party_chip(label: &str, color: Option<&str>) -> Markup {
    // Uniform size: fixed width and height, single line, truncated with an
    // ellipsis if an abbreviation is unexpectedly long. leading matches the
    // height so the text centers vertically.
    const BASE: &str =
        "inline-block h-5 w-16 truncate border border-ink px-1 text-center align-middle font-mono text-[10px] font-semibold uppercase leading-5 tracking-wide";
    match color {
        Some(c) => html! {
            span class={(BASE) " " (text_on(c))} style={"background-color:" (c)} { (label) }
        },
        None => html! {
            span class={(BASE) " bg-paper-raised text-ink"} { (label) }
        },
    }
}

/// A party chip that links to the party page. Use where it stands alone (for
/// example a person's current party), never inside another link.
pub fn party_badge(label: &str, slug: &str, color: Option<&str>, country: &str) -> Markup {
    html! {
        a href={"/" (country) "/parties/" (slug)} class="inline-flex transition-opacity hover:opacity-80" {
            (party_chip(label, color))
        }
    }
}

/// Black or white text depending on the fill's perceived brightness.
fn text_on(hex: &str) -> &'static str {
    let h = hex.trim_start_matches('#');
    if h.len() == 6 {
        let channel = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0) as f32;
        let luminance = 0.299 * channel(0) + 0.587 * channel(2) + 0.114 * channel(4);
        if luminance > 140.0 {
            return "text-ink";
        }
    }
    "text-white"
}
