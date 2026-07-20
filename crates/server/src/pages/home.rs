use axum::extract::State;
use maud::{html, Markup};

use crate::auth::AuthSession;
use crate::error::PageError;
use crate::i18n;
use crate::state::AppState;
use crate::ui;

pub async fn page(
    State(state): State<AppState>,
    session: Option<AuthSession>,
) -> Result<Markup, PageError> {
    // The home is a gallery of chambers: each country shown as the composition
    // of its legislature, so the visitor sees who holds power at a glance.
    let countries = db::country::list(&state.pool).await?;
    let mut chambers = Vec::with_capacity(countries.len());
    for c in &countries {
        let seats = db::country::seat_distribution(&state.pool, c.id).await?;
        let independents = db::country::unaffiliated_mp_count(&state.pool, c.id).await?;
        let total: i64 = seats.iter().map(|s| s.seats).sum::<i64>() + independents;
        chambers.push((c, seats, independents, total));
    }

    let content = html! {
        section {
            @if let Some(ref notice) = state.site_notice {
                div class="mb-8 flex items-start gap-3 border border-ink bg-paper-raised px-4 py-3" {
                    span class="op-block mt-0.5 h-3.5 w-3.5 shrink-0" {}
                    p class="font-mono text-[13px] leading-relaxed text-ink" { (notice.as_ref()) }
                }
            }

            header class="max-w-3xl" {
                p class="font-mono text-[12px] font-semibold uppercase tracking-widest text-ink-faint" {
                    (i18n::t("Open political data."))
                }
                h1 class="mt-3 font-display text-[2.6rem] font-extrabold leading-[0.98] tracking-tight text-ink sm:text-6xl" {
                    (i18n::t("Every chamber, at a glance."))
                }
                p class="mt-5 max-w-2xl text-lg leading-relaxed text-ink-muted" {
                    (i18n::t("People, parties, elections and polls. Every fact linked to its source, one verifiable dataset per country."))
                }
            }

            @if !chambers.is_empty() {
                div class="mb-4 mt-12 flex items-baseline justify-between border-b border-hairline-strong pb-2" {
                    h2 class="font-mono text-[12px] font-semibold uppercase tracking-widest text-ink-muted" {
                        (i18n::t("Countries"))
                    }
                }
                ul class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3" {
                    @for (c, seats, independents, total) in &chambers {
                        li {
                            a href={"/" (c.slug)}
                              class="op-card op-card-link group flex h-full flex-col p-5" {
                                div class="px-3 pt-1" {
                                    (ui::hemicycle::hemicycle(seats, *independents, None, 240.0))
                                }
                                div class="mt-5 border-t border-hairline pt-3" {
                                    h3 class="font-display text-2xl font-extrabold tracking-tight text-ink underline-offset-4 group-hover:underline" {
                                        (c.name)
                                    }
                                    div class="mt-2 flex items-baseline justify-between gap-2" {
                                        span class="min-w-0 truncate font-mono text-[11px] text-ink-faint" {
                                            @if let Some(ref g) = c.government_type {
                                                (i18n::t_dyn(g))
                                            }
                                        }
                                        @if *total > 0 {
                                            span class="shrink-0 font-mono text-[13px] font-semibold text-ink" {
                                                (total) " " span class="text-ink-faint" { (i18n::t("seats")) }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            a href="/search"
              class="group mt-8 flex items-center gap-4 border border-hairline bg-paper-raised px-4 py-4 transition-colors hover:border-ink" {
                span class="grid h-8 w-8 shrink-0 place-items-center border border-hairline font-mono text-[14px] text-ink-muted" { "⌕" }
                div class="min-w-0 flex-1" {
                    div class="font-display text-lg font-bold tracking-tight text-ink underline-offset-4 group-hover:underline" {
                        (i18n::t("Search"))
                    }
                    div class="mt-0.5 font-mono text-[11px] text-ink-faint" {
                        (i18n::t("Full-text search across people and parties."))
                    }
                }
                span class="shrink-0 font-mono text-ink-faint transition-colors group-hover:text-ink" { "→" }
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
