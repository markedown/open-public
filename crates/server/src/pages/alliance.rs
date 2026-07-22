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
    // The coalition's combined size, so the page has a headline figure of its
    // own rather than only echoing the chips already on the country page.
    let combined_seats: i64 = members.iter().map(|m| m.seats).sum();
    let loc = crate::content::Localized::load(&pool, "alliance", alliance.id).await?;
    let name = loc
        .get("name", Some(alliance.name.as_str()))
        .unwrap_or(&alliance.name);
    let summary = loc.get("summary", alliance.summary.as_deref());

    let content = html! {
        article {
            (ui::breadcrumb::breadcrumbs(&[
                Crumb { label: country_model.name.clone(), href: Some(format!("/{}", country_model.slug)) },
                Crumb { label: i18n::t("Alliances").to_string(), href: Some(format!("/{}/alliances", country_model.slug)) },
                Crumb { label: name.to_string(), href: None },
            ]))

            header class="op-card mb-8 p-6 sm:p-8" {
                div class="flex flex-wrap items-center gap-3" {
                    span class="text-[11px] font-bold uppercase tracking-widest text-ink-muted" {
                        (i18n::t("Alliance"))
                    }
                    @if alliance.dissolved_date.is_none() {
                        span class="rounded-full border border-accent px-2 py-0.5 text-[10px] font-bold uppercase tracking-wide text-accent" {
                            (i18n::t("Active"))
                        }
                    } @else {
                        span class="rounded-full border border-hairline px-2 py-0.5 text-[10px] font-bold uppercase tracking-wide text-ink-muted" {
                            (i18n::t("Inactive"))
                        }
                    }
                }
                h1 class="mt-2 text-3xl font-bold tracking-tight text-ink sm:text-4xl" {
                    (name)
                }
                @if alliance.founded_date.is_some() || alliance.dissolved_date.is_some() {
                    p class="mt-2 font-mono text-xs text-ink-muted" {
                        (fmt::date_range(alliance.founded_date, alliance.dissolved_date))
                    }
                }
                @if combined_seats > 0 {
                    div class="mt-5" {
                        div class="font-mono text-3xl font-semibold text-ink" { (combined_seats) }
                        div class="mt-0.5 text-[11px] font-bold uppercase tracking-widest text-ink-muted" {
                            (i18n::t("combined members"))
                        }
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
                div class="mb-8 max-w-prose" {
                    (ui::translated::prose(
                        text,
                        loc.is_translated("summary").then_some(alliance.summary.as_deref()).flatten(),
                    ))
                }
            }

            @if !members.is_empty() {
                section class="mb-8" {
                    (ui::section_header(
                        i18n::t("Parties"),
                        Some(html! { span class="font-mono text-xs text-ink-muted" { (members.len()) } }),
                    ))
                    ul class="op-card grid gap-x-10 px-5 sm:grid-cols-2" {
                        @for m in &members {
                            li class="flex items-center gap-3 border-b border-hairline-light py-2.5" {
                                @if let Some(ref sn) = m.party_short_name {
                                    span class="shrink-0" { (ui::badge::party_chip(sn, m.party_color.as_deref())) }
                                }
                                a href={"/" (country_model.slug) "/parties/" (m.party_slug)}
                                  class="grow text-sm font-medium text-ink transition-colors hover:text-accent" {
                                    (m.party_name)
                                }
                                @if m.seats > 0 {
                                    span class="shrink-0 font-mono text-xs text-ink-muted" {
                                        (m.seats) " " (i18n::t("members"))
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
        Some(name),
        session.is_some(),
        session.as_ref().is_some_and(|s| s.is_admin),
        content,
    ))
}
