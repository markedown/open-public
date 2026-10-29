use maud::{html, Markup};

/// A party chip: a rounded pill with a dot in the party's colour and its short
/// name. A party with no documented colour gets a neutral grey dot rather than
/// one we picked. A plain span, safe inside a link. Colour only ever appears in
/// the dot, inside this data element.
pub fn party_chip(label: &str, color: Option<&str>) -> Markup {
    html! {
        span class="inline-flex max-w-full items-center gap-1.5 rounded-full border border-hairline bg-paper px-2.5 py-0.5 align-middle text-[14px] font-semibold leading-6 text-ink" {
            span aria-hidden="true" class="size-2.5 shrink-0 rounded-full"
                style={"background-color:" (super::css_color(color, "#b7bec6"))} {}
            span class="truncate" { (label) }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chip_shows_the_party_colour_or_a_neutral_grey() {
        let coloured = party_chip("TP", Some("#123456")).into_string();
        assert!(coloured.contains("background-color:#123456"));
        assert!(coloured.contains("TP"));
        // No documented colour: a neutral grey, never one we picked.
        let plain = party_chip("TP", None).into_string();
        assert!(plain.contains("background-color:#b7bec6"));
        // A malformed colour cannot escape the style attribute.
        let bad = party_chip("TP", Some("red;x:y")).into_string();
        assert!(bad.contains("background-color:#b7bec6"));
        let badge = party_badge("TP", "test", Some("#fff"), "tr").into_string();
        assert!(badge.contains("parties/test"));
    }
}
