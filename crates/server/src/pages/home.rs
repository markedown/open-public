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
    // The country overview is featured here rather than in the navbar. Headline
    // figures are per country (in each entry), not a global sum, since the
    // platform is a set of independent per-country datasets.
    let countries = db::country::list(&state.pool).await?;

    let content = html! {
        section {
            // A quiet work-in-progress notice, shown only when configured.
            @if let Some(ref notice) = state.site_notice {
                div class="mb-8 flex items-start gap-3 border border-ink bg-paper-raised px-4 py-3" {
                    span class="op-block mt-0.5 h-3.5 w-3.5 shrink-0" {}
                    p class="font-mono text-[13px] leading-relaxed text-ink" { (notice.as_ref()) }
                }
            }

            // Masthead: a plain statement of what the register is.
            header class="border-b border-hairline-strong pb-8" {
                h1 class="font-mono text-4xl font-semibold leading-[1.06] tracking-tight text-ink sm:text-5xl" {
                    (i18n::t("A public register of political data."))
                }
                p class="mt-5 max-w-2xl text-lg leading-relaxed text-ink-muted" {
                    (i18n::t("People, parties, elections and polls. Every fact linked to its source, one verifiable dataset per country."))
                }
            }

            @if !countries.is_empty() {
                div class="mb-3 mt-10 flex items-baseline justify-between" {
                    h2 class="font-mono text-[12px] font-semibold uppercase tracking-widest text-ink-muted" {
                        (i18n::t("Countries"))
                    }
                    span class="font-mono text-[12px] text-ink-faint" {
                        (format!("{:02}", countries.len()))
                    }
                }
                ul class="border border-hairline" {
                    @for (i, c) in countries.iter().enumerate() {
                        li class="border-b border-hairline last:border-b-0" {
                            a href={"/" (c.slug)}
                              class="group flex items-center gap-4 px-4 py-4 transition-colors hover:bg-paper-sunken" {
                                span class="w-6 shrink-0 font-mono text-[11px] text-ink-faint" {
                                    (format!("{:02}", i + 1))
                                }
                                @if let Some(ref flag) = c.flag_url {
                                    img src=(flag) alt="" loading="lazy"
                                        class="h-7 w-auto shrink-0 border border-hairline object-cover";
                                } @else {
                                    span class="op-block grid h-7 w-7 shrink-0 place-items-center font-mono text-[13px] font-semibold" {
                                        (c.name.chars().next().unwrap_or('?'))
                                    }
                                }
                                div class="min-w-0 flex-1" {
                                    div class="font-mono text-[15px] font-semibold tracking-tight text-ink underline-offset-4 group-hover:underline" {
                                        (c.name)
                                    }
                                    @if c.government_type.is_some() || c.capital.is_some() {
                                        div class="mt-0.5 truncate font-mono text-[11px] text-ink-faint" {
                                            @if let Some(ref g) = c.government_type { (i18n::t_dyn(g)) }
                                            @if c.government_type.is_some() && c.capital.is_some() { " · " }
                                            @if let Some(ref cap) = c.capital { (cap) }
                                        }
                                    }
                                }
                                span class="shrink-0 font-mono text-ink-faint transition-colors group-hover:text-ink" { "→" }
                            }
                        }
                    }
                }
            }

            // Data is reached through a country above, or through search here.
            a href="/search"
              class="group mt-4 flex items-center gap-4 border border-hairline px-4 py-4 transition-colors hover:bg-paper-sunken" {
                span class="grid h-7 w-7 shrink-0 place-items-center border border-hairline font-mono text-[13px] text-ink-muted" { "⌕" }
                div class="min-w-0 flex-1" {
                    div class="font-mono text-[15px] font-semibold tracking-tight text-ink underline-offset-4 group-hover:underline" {
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
