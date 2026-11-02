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
    // The compass is the way in that asks nothing of a visitor: no account, and
    // nothing stored. A country is offered once it has positions to answer.
    let mut compasses = Vec::new();
    for c in &countries {
        let party = db::compass::count_theses(&state.pool, c.id, db::compass::SCOPE_PARTY).await?;
        let person =
            db::compass::count_theses(&state.pool, c.id, db::compass::SCOPE_PERSON).await?;
        if party > 0 {
            compasses.push((c, format!("/{}/compass", c.slug), party));
        } else if person > 0 {
            compasses.push((
                c,
                format!("/{}/compass/{}", c.slug, db::compass::SCOPE_PERSON),
                person,
            ));
        }
    }

    let content = html! {
        section {
            // What the site is and how to search it, for the engine that
            // reaches the home page first.
            (ui::jsonld::web_site_script())

            @if let Some(ref notice) = state.site_notice {
                div class="mb-8 flex items-start gap-3 border border-ink bg-paper-raised px-4 py-3" {
                    span class="op-block mt-0.5 h-3.5 w-3.5 shrink-0" {}
                    p class="text-[15px] leading-relaxed text-ink" { (notice.as_ref()) }
                }
            }

            // The compass opens the page wherever a country has one: it is what a
            // first-time visitor can do straight away, with no account. The record
            // of each chamber follows.
            @if compasses.is_empty() {
                header class="max-w-3xl" {
                    h1 class="font-display text-[2.6rem] font-extrabold leading-[0.98] tracking-tight text-ink sm:text-6xl" {
                        (i18n::t("Every chamber, at a glance."))
                    }
                    p class="mt-5 max-w-2xl text-lg leading-relaxed text-ink-muted" {
                        (i18n::t("People, parties, elections and polls. Every fact linked to its source, one verifiable dataset per country."))
                    }
                }
            } @else {
                section class="rounded-[12px] bg-accent-tint px-6 py-8 sm:px-10 sm:py-11" {
                    h1 class="font-display text-[2.4rem] font-extrabold leading-[1.02] tracking-tight text-ink sm:text-[3.4rem]" {
                        (i18n::t("Which party is closest to your views?"))
                    }
                    p class="mt-4 max-w-[42em] text-[20px] leading-snug text-ink-muted" {
                        (i18n::t("Answer a few positions and see how the parties compare. No account needed, and nothing you enter is stored."))
                    }
                    ul class="mt-6 flex flex-wrap gap-2.5" {
                        @for (c, href, n) in &compasses {
                            li {
                                a href=(href)
                                  class="inline-flex min-h-12 items-center gap-2.5 rounded-[10px] bg-ink px-4 text-[17px] font-semibold text-paper transition-colors hover:bg-accent-strong" {
                                    @if let Some(ref flag) = c.flag_url {
                                        img src=(crate::mirror::local_src(flag)) alt="" loading="lazy" class="h-4 w-auto rounded-[2px]";
                                    }
                                    (i18n::t_dyn(&c.name))
                                    span class="text-[15px] font-normal opacity-80" { (n) }
                                }
                            }
                        }
                    }
                }
                div class="mt-12 max-w-3xl" {
                    h2 class="font-display text-[28px] font-extrabold tracking-tight text-ink sm:text-[32px]" {
                        (i18n::t("Every chamber, at a glance."))
                    }
                    p class="mt-2 max-w-2xl text-[18px] leading-relaxed text-ink-muted" {
                        (i18n::t("People, parties, elections and polls. Every fact linked to its source, one verifiable dataset per country."))
                    }
                }
            }

            @if !chambers.is_empty() {
                ul class="mt-6 grid gap-4 sm:grid-cols-2 lg:grid-cols-3" {
                    @for (c, seats, independents, total) in &chambers {
                        li {
                            a href={"/" (c.slug)}
                              class="op-card op-card-link group flex h-full flex-col p-5" {
                                div class="px-3 pt-1" {
                                    (ui::hemicycle::hemicycle(seats, *independents, None, 240.0))
                                }
                                div class="mt-5 border-t border-hairline pt-3" {
                                    h3 class="font-display text-2xl font-extrabold tracking-tight text-ink underline-offset-4 group-hover:underline" {
                                        (i18n::t_dyn(&c.name))
                                    }
                                    div class="mt-2 flex items-baseline justify-between gap-2" {
                                        span class="min-w-0 truncate text-[14px] text-ink-faint" {
                                            @if let Some(ref g) = c.government_type {
                                                (i18n::t_dyn(g))
                                            }
                                        }
                                        @if *total > 0 {
                                            span class="shrink-0 text-[15px] font-semibold text-ink" {
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
        }
    };

    Ok(ui::layout::document(
        None,
        session.is_some(),
        session.as_ref().is_some_and(|s| s.is_admin),
        content,
    ))
}
