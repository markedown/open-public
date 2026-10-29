use maud::{html, Markup};

use crate::i18n;

/// Whether the current viewer can follow this entity, and whether they already
/// do.
pub enum FollowState {
    Anonymous,
    Following,
    NotFollowing,
}

/// A follow toggle for an entity. A signed-in visitor gets a POST form
/// (HTMX-enhanced so it swaps itself in place; a plain POST falls back to
/// reloading `next`); an anonymous visitor gets a link to sign in. `next` is the
/// page to return to without JavaScript.
pub fn button(entity_type: &str, entity_id: i64, state: FollowState, next: &str) -> Markup {
    if let FollowState::Anonymous = state {
        return html! {
            a href="/login"
              class="inline-flex min-h-11 items-center gap-1.5 rounded-[10px] border border-hairline-strong bg-paper px-4 text-[16px] font-semibold text-ink transition-colors hover:border-ink" {
                (i18n::t("Log in to follow"))
            }
        };
    }
    let following = matches!(state, FollowState::Following);
    let action = format!("/follow/{entity_type}/{entity_id}");
    html! {
        form method="post" action=(action) hx-post=(action) hx-swap="outerHTML" class="inline-block" {
            input type="hidden" name="next" value=(next);
            @if following {
                button type="submit"
                    class="inline-flex min-h-11 items-center gap-1.5 rounded-[10px] border border-hairline-strong bg-paper px-4 text-[16px] font-semibold text-ink transition-colors hover:border-ink" {
                    "✓ " (i18n::t("Following"))
                }
            } @else {
                button type="submit"
                    class="inline-flex min-h-11 items-center gap-1.5 rounded-[10px] bg-ink px-5 text-[16px] font-semibold text-paper transition-colors hover:bg-accent-strong" {
                    "+ " (i18n::t("Follow"))
                }
            }
        }
    }
}
