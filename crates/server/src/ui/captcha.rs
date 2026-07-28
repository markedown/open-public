//! The proof-of-work captcha widget, placed inside a protected form.
//!
//! The vendored ALTCHA custom element fetches a challenge from our own origin,
//! solves it in a worker at submit time, and writes the solution into a hidden
//! field named `altcha` that the form carries along. Nothing loads from a third
//! party and no cookie or behavioural signal is taken. Because the work runs in
//! JavaScript, a form carrying this widget is not usable without JavaScript;
//! it is placed only on the account and submission forms, never on voting, which
//! keeps its plain form-POST fallback.

use maud::{html, Markup};

use crate::i18n;

/// The widget for a protected form. Solves on submit (`auto="onsubmit"`), so no
/// work runs until the visitor actually submits, and the branding is stripped so
/// nothing external is referenced.
pub fn widget() -> Markup {
    html! {
        div class="mt-1" {
            // Themed to the monochrome chrome; colour on this platform is data,
            // never UI accent, so the widget stays neutral.
            altcha-widget
                challengeurl="/altcha/challenge"
                name="altcha"
                auto="onsubmit"
                hidelogo
                hidefooter
                style="--altcha-max-width:100%;--altcha-border-radius:8px;" {}
            noscript {
                p class="text-sm text-ink-muted" {
                    (i18n::t("This form needs JavaScript enabled to verify your request."))
                }
            }
        }
        // Vendored, served from our origin, executes once per page.
        script type="module" src="/static/altcha.min.js" defer {}
    }
}
