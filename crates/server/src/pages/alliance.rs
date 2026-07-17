use axum::extract::{Path, State};
use maud::{html, Markup};

use crate::auth::AuthSession;
use crate::error::PageError;
use crate::fmt;
use crate::i18n;
use crate::ui;
use crate::ui::breadcrumb::Crumb;

pub async fn detail(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path((country, slug)): Path<(String, String)>,
) -> Result<Markup, PageError> {
    let country_model = db::country::get_by_slug(&pool, &country)
        .await?
        .ok_or(PageError::NotFound)?;

    let alliance = db::alliances::get_by_slug_in_country(&pool, &slug, country_model.id)
        .await?
        .ok_or(PageError::NotFound)?;
    let members = db::alliances::members(&pool, alliance.id).await?;
    let loc = crate::content::Localized::load(&pool, "alliance", alliance.id).await?;
    let name = loc
        .get("name", Some(alliance.name.as_str()))
        .unwrap_or(&alliance.name);
    let summary = loc.get("summary", alliance.summary.as_deref());

    let content = html! {
        article {
            (ui::breadcrumb::breadcrumbs(&[
                Crumb { label: country_model.name.clone(), href: Some(format!("/{}", country_model.slug)) },
                Crumb { label: name.to_string(), href: None },
            ]))

            header class={"mb-12 border-[1.5px] border-ink bg-paper-raised p-6 sm:p-8 " (ui::CORNER_TICK)} {
                div class="flex flex-wrap items-center gap-3" {
                    span class="text-[11px] font-bold uppercase tracking-widest text-ink-muted" {
                        (i18n::t("Alliance"))
                    }
                    @if alliance.dissolved_date.is_none() {
                        span class="border border-accent px-1.5 text-[10px] font-bold uppercase tracking-wide text-accent" {
                            (i18n::t("Active"))
                        }
                    } @else {
                        span class="border border-hairline px-1.5 text-[10px] font-bold uppercase tracking-wide text-ink-muted" {
                            (i18n::t("Inactive"))
                        }
                    }
                }
                h1 class="mt-1 font-serif text-4xl font-semibold tracking-tight text-ink sm:text-[44px]" {
                    (name)
                }
                @if alliance.founded_date.is_some() || alliance.dissolved_date.is_some() {
                    p class="mt-2 font-mono text-xs text-ink-muted" {
                        (fmt::date_range(alliance.founded_date, alliance.dissolved_date))
                    }
                }
                // The member party chips, each linking to its party page.
                @if !members.is_empty() {
                    div class="mt-5 flex flex-wrap gap-1.5" {
                        @for m in &members {
                            @if let Some(ref sn) = m.party_short_name {
                                a href={"/" (country_model.slug) "/parties/" (m.party_slug)}
                                  class="inline-flex transition-opacity hover:opacity-80" {
                                    (ui::badge::party_chip(sn, m.party_color.as_deref()))
                                }
                            }
                        }
                    }
                }
            }

            @if let Some(text) = summary {
                div class="mb-12" {
                    (ui::translated::prose(
                        text,
                        loc.is_translated("summary").then_some(alliance.summary.as_deref()).flatten(),
                    ))
                }
            }

            @if !members.is_empty() {
                section class="mb-12" {
                    h2 class="mb-5 flex items-baseline gap-2 border-b-2 border-accent pb-2 text-xs font-bold uppercase tracking-widest text-ink" {
                        (i18n::t("Parties"))
                        span class="font-mono text-ink-muted" { (members.len()) }
                    }
                    ul class="grid gap-x-10 sm:grid-cols-2" {
                        @for m in &members {
                            li class="flex items-center gap-3 border-b border-hairline-light py-2.5" {
                                @if let Some(ref sn) = m.party_short_name {
                                    span class="shrink-0" { (ui::badge::party_chip(sn, m.party_color.as_deref())) }
                                }
                                a href={"/" (country_model.slug) "/parties/" (m.party_slug)}
                                  class="grow text-sm font-medium text-ink transition-colors hover:text-accent" {
                                    (m.party_name)
                                }
                            }
                        }
                    }
                }
            }
        }
    };

    Ok(ui::layout::document(
        Some(name),
        session.is_some(),
        session.as_ref().is_some_and(|s| s.is_admin),
        content,
    ))
}
