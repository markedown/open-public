use maud::{html, Markup};

/// The primary action: an accent-filled rounded button with a soft shadow that
/// deepens on hover, matching the dashboard card language.
const PRIMARY: &str = "inline-flex items-center justify-center rounded-[10px] bg-accent px-4 py-2.5 text-[16px] font-semibold text-white shadow-sm transition-colors hover:bg-accent-strong";

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
