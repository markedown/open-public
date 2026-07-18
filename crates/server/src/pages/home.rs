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
        section class="py-14 sm:py-20" {
            // A quiet work-in-progress notice, shown only when configured.
            @if let Some(ref notice) = state.site_notice {
                p class="mb-8 max-w-prose border-l-2 border-accent bg-paper-raised px-4 py-2.5 text-sm text-ink-muted" {
                    (notice.as_ref())
                }
            }
            div class="max-w-3xl" {
                h1 class="font-serif text-6xl font-semibold tracking-tight text-ink" {
                    (SITE_NAME)
                }
                p class="mt-6 max-w-prose text-lg leading-relaxed text-ink-muted" {
                    (i18n::t("People, parties, elections and polls. Every fact linked to its source, one verifiable dataset per country."))
                }
            }

            @if !countries.is_empty() {
                h2 class="mt-14 mb-5 border-b-2 border-accent pb-2 text-xs font-bold uppercase tracking-widest text-ink" {
                    (i18n::t("Countries"))
                }
                div class="grid gap-5 sm:grid-cols-2" {
                    @for c in &countries {
                        a href={"/" (c.slug)}
                          class={"flex flex-col justify-between border-[1.5px] border-ink bg-paper-raised p-6 transition-colors hover:border-accent sm:p-8 " (ui::CORNER_TICK)} {
                            div {
                                span class="font-mono text-[10px] uppercase tracking-[0.06em] text-accent" {
                                    (i18n::t("Overview"))
                                }
                                h2 class="mt-2.5 flex items-center gap-2.5 font-serif text-2xl font-semibold tracking-tight text-ink" {
                                    @if let Some(ref flag) = c.flag_url {
                                        img src=(flag) alt="" loading="lazy"
                                            class="h-5 w-auto border border-hairline";
                                    }
                                    (c.name)
                                }
                                @if c.government_type.is_some() || c.capital.is_some() {
                                    p class="mt-2 text-sm text-ink-muted" {
                                        @if let Some(ref g) = c.government_type { (i18n::t_dyn(g)) }
                                        @if c.government_type.is_some() && c.capital.is_some() { " · " }
                                        @if let Some(ref cap) = c.capital { (cap) }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Data is reached through a country above, or through search here.
            a href="/search"
              class="mt-5 block border-[1.5px] border-ink bg-paper-raised p-6 transition-colors hover:border-accent" {
                h2 class="text-sm font-bold uppercase tracking-wide text-ink" {
                    (i18n::t("Search"))
                }
                p class="mt-2 text-sm leading-relaxed text-ink-muted" {
                    (i18n::t("Full-text search across people and parties."))
                }
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
