use axum::extract::{Path, State};
use maud::{html, Markup};

use crate::auth::AuthSession;
use crate::error::PageError;
use crate::fmt;
use crate::i18n;
use crate::ui;
use crate::ui::breadcrumb::Crumb;

/// The alliances (coalitions) index for a country: active ones first, each with
/// its member count and lifespan.
pub async fn list(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path(country): Path<String>,
) -> Result<Markup, PageError> {
    let country = db::country::get_by_slug(&pool, &country)
        .await?
        .ok_or(PageError::NotFound)?;
    let alliances = db::alliances::list_for_country(&pool, country.id, i18n::lang_code()).await?;

    let content = html! {
        section {
            (ui::breadcrumb::breadcrumbs(&[
                Crumb { label: country.name.clone(), href: Some(format!("/{}", country.slug)) },
                Crumb { label: i18n::t("Alliances").to_string(), href: None },
            ]))
            (ui::page_header(
                i18n::t("Alliances"),
                Some(html! { span class="font-mono" { (alliances.len()) } " " (i18n::t("Alliances")) }),
                None,
            ))

            @if alliances.is_empty() {
                p class="py-12 text-center text-sm text-ink-muted" { (i18n::t("No alliances yet.")) }
            } @else {
                ul class="grid gap-3 sm:grid-cols-2" {
                    @for a in &alliances {
                        li {
                            a href={"/" (country.slug) "/alliance/" (a.slug)}
                              class="op-card op-card-link block px-4 py-3.5" {
                                div class="flex items-baseline justify-between gap-3" {
                                    span class="text-sm font-medium text-ink" { (a.name) }
                                    // Active while not dissolved.
                                    @if a.dissolved_date.is_none() {
                                        span class="shrink-0 rounded-full border border-accent px-2 py-0.5 text-[10px] font-bold uppercase tracking-wide text-accent" {
                                            (i18n::t("Active"))
                                        }
                                    } @else {
                                        span class="shrink-0 rounded-full border border-hairline px-2 py-0.5 text-[10px] font-bold uppercase tracking-wide text-ink-muted" {
                                            (i18n::t("Inactive"))
                                        }
                                    }
                                }
                                div class="mt-1 flex flex-wrap gap-x-4 font-mono text-[11px] text-ink-muted" {
                                    span { (a.member_count) " " (i18n::t("parties")) }
                                    @if let Some(f) = a.founded_date {
                                        span { (fmt::date(Some(f))) }
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
        Some(i18n::t("Alliances")),
        session.is_some(),
        session.as_ref().is_some_and(|s| s.is_admin),
        content,
    ))
}
