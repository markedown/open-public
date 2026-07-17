use maud::{html, Markup};

/// The primary action: a flat ink-filled block. No shadow, no press animation;
/// hover shifts the fill to the accent.
const PRIMARY: &str = "inline-flex items-center justify-center border border-ink bg-ink px-4 py-2 text-sm font-bold uppercase tracking-wide text-paper transition-colors hover:border-accent hover:bg-accent";

/// The primary submit button.
pub fn primary(label: &str) -> Markup {
    html! {
        button type="submit" class=(PRIMARY) { (label) }
    }
}

/// A link styled as the primary button, for moving to the next step.
pub fn link(href: &str, label: &str) -> Markup {
    html! {
        a href=(href) class=(PRIMARY) { (label) }
    }
}
