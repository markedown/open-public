use axum::extract::State;
use maud::{html, Markup};

use crate::auth::AuthSession;
use crate::error::PageError;
use crate::i18n::{self, SITE_NAME};
use crate::state::AppState;
use crate::ui;

pub async fn page(
    State(state): State<AppState>,
    session: Option<AuthSession>,
) -> Result<Markup, PageError> {
    // The country overview is featured here rather than in the navbar; the
    // navbar gains a "Countries" link only once more than one country exists.
    // Headline figures are per country (in each card), not a global sum, since
    // the platform is a set of independent per-country datasets.
    let countries = db::country::list(&state.pool).await?;

    let content = html! {
        section class="py-8 sm:py-12" {
            // A quiet work-in-progress notice, shown only when configured.
            @if let Some(ref notice) = state.site_notice {
                p class="mb-8 flex items-start gap-2 rounded-xl border border-accent/30 bg-accent-tint px-4 py-3 text-sm text-ink" {
                    span { "🚧" }
                    span { (notice.as_ref()) }
                }
            }
            div class="max-w-2xl" {
                h1 class="text-5xl font-bold tracking-tight text-ink sm:text-6xl" {
                    (SITE_NAME)
                }
                p class="mt-5 text-lg leading-relaxed text-ink-muted" {
                    (i18n::t("People, parties, elections and polls. Every fact linked to its source, one verifiable dataset per country."))
                }
            }

            @if !countries.is_empty() {
                h2 class="mt-12 mb-4 text-xs font-bold uppercase tracking-widest text-ink-muted" {
                    (i18n::t("Countries"))
                }
                div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3" {
                    @for c in &countries {
                        a href={"/" (c.slug)}
                          class="op-card op-card-link group flex items-center gap-4 p-5" {
                            @if let Some(ref flag) = c.flag_url {
                                img src=(flag) alt="" loading="lazy"
                                    class="h-11 w-11 shrink-0 rounded-lg border border-hairline object-cover";
                            } @else {
                                span class="grid h-11 w-11 shrink-0 place-items-center rounded-lg bg-paper-sunken text-lg font-bold text-ink-muted" {
                                    (c.name.chars().next().unwrap_or('?'))
                                }
                            }
                            div class="min-w-0 flex-1" {
                                h3 class="truncate text-[17px] font-semibold text-ink" { (c.name) }
                                @if c.government_type.is_some() || c.capital.is_some() {
                                    p class="mt-0.5 truncate text-[13px] text-ink-muted" {
                                        @if let Some(ref g) = c.government_type { (i18n::t_dyn(g)) }
                                        @if c.government_type.is_some() && c.capital.is_some() { " · " }
                                        @if let Some(ref cap) = c.capital { (cap) }
                                    }
                                }
                            }
                            span class="shrink-0 text-ink-muted transition-transform group-hover:translate-x-0.5 group-hover:text-accent" { "→" }
                        }
                    }
                }
            }

            // Data is reached through a country above, or through search here.
            a href="/search"
              class="op-card op-card-link mt-4 flex items-center gap-4 p-5" {
                span class="grid h-11 w-11 shrink-0 place-items-center rounded-lg bg-accent-tint text-accent" { "⌕" }
                div class="flex-1" {
                    h3 class="text-[17px] font-semibold text-ink" { (i18n::t("Search")) }
                    p class="mt-0.5 text-[13px] text-ink-muted" {
                        (i18n::t("Full-text search across people and parties."))
                    }
                }
                span class="shrink-0 text-ink-muted" { "→" }
            }
        }
    };

    Ok(ui::layout::document(
        None,
        session.is_some(),
        session.as_ref().is_some_and(|s| s.is_admin),
        content,
    ))
}
