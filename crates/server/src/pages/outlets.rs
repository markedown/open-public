use axum::extract::{Path, Query, State};
use maud::{html, Markup};
use serde::Deserialize;

use crate::auth::AuthSession;
use crate::error::PageError;
use crate::i18n;
use crate::ui;
use crate::ui::breadcrumb::Crumb;

/// The outlets index: every news company we save from, with its leaning and how
/// many articles we hold.
pub async fn list(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path(country): Path<String>,
) -> Result<Markup, PageError> {
    let country = db::country::get_by_slug(&pool, &country)
        .await?
        .ok_or(PageError::NotFound)?;
    let outlets = db::outlets::list(&pool, country.id).await?;

    let content = html! {
        section {
            (ui::breadcrumb::breadcrumbs(&[
                Crumb { label: country.name.clone(), href: Some(format!("/{}", country.slug)) },
                Crumb { label: i18n::t("Outlets").to_string(), href: None },
            ]))
            header class="mb-8 border-b-2 border-accent pb-4" {
                h1 class="font-serif text-4xl font-semibold tracking-tight text-ink" {
                    (i18n::t("Outlets"))
                }
                p class="mt-2 text-xs font-bold uppercase tracking-widest text-ink-muted" {
                    span class="font-mono" { (outlets.len()) } " " (i18n::t("Outlets"))
                }
            }
            @if outlets.is_empty() {
                p class="py-12 text-center text-sm text-ink-muted" { (i18n::t("No outlets yet.")) }
            } @else {
                ul class="space-y-2" {
                    @for o in &outlets {
                        li { (ui::outlet::card(o, &country.slug)) }
                    }
                }
            }
        }
    };

    Ok(ui::layout::document(
        Some(i18n::t("Outlets")),
        session.is_some(),
        session.as_ref().is_some_and(|s| s.is_admin),
        content,
    ))
}

/// The page query for pagination.
#[derive(Deserialize)]
pub struct Page {
    p: Option<i64>,
}

const PER_PAGE: i64 = 20;

/// One outlet's page: identity, leaning, our summary, and the articles we hold
/// from it, paginated.
pub async fn detail(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path((country, slug)): Path<(String, String)>,
    Query(page): Query<Page>,
) -> Result<Markup, PageError> {
    let country = db::country::get_by_slug(&pool, &country)
        .await?
        .ok_or(PageError::NotFound)?;
    let outlet = db::outlets::get_by_slug_in_country(&pool, &slug, country.id)
        .await?
        .ok_or(PageError::NotFound)?;
    let loc = crate::content::Localized::load(&pool, "outlet", outlet.id).await?;
    let summary = loc.get("summary", outlet.summary.as_deref());

    let total_pages = (outlet.article_count + PER_PAGE - 1) / PER_PAGE;
    let current = page.p.unwrap_or(1).clamp(1, total_pages.max(1));
    let offset = (current - 1) * PER_PAGE;
    let articles = db::news::for_outlet(&pool, outlet.id, PER_PAGE, offset).await?;
    let base = format!("/{}/outlet/{}", country.slug, outlet.slug);

    let content = html! {
        article class="max-w-2xl" {
            (ui::breadcrumb::breadcrumbs(&[
                Crumb { label: country.name.clone(), href: Some(format!("/{}", country.slug)) },
                Crumb { label: i18n::t("Outlets").to_string(), href: Some(format!("/{}/outlets", country.slug)) },
                Crumb { label: outlet.name.clone(), href: None },
            ]))
            header class={"mb-8 border-[1.5px] border-ink bg-paper-raised p-6 sm:p-8 " (ui::CORNER_TICK)} {
                div class="flex items-center gap-4" {
                    @if let Some(ref logo) = outlet.logo_url {
                        img src=(logo) alt=(&outlet.name) loading="lazy"
                            class="h-12 w-12 shrink-0 border border-hairline object-contain";
                    }
                    h1 class="font-serif text-3xl font-semibold tracking-tight text-ink sm:text-4xl" {
                        (outlet.name)
                    }
                }
                @if let Some(ref l) = outlet.leaning {
                    div class="mt-5" {
                        (ui::outlet::leaning_bar(l, false))
                    }
                }
                @if let Some(text) = summary {
                    div class="mt-4" {
                        p class="max-w-prose text-sm leading-relaxed text-ink" { (text) }
                        (ui::translated::original_disclosure(
                            loc.is_translated("summary").then_some(outlet.summary.as_deref()).flatten(),
                        ))
                    }
                }
                @if let Some(ref home) = outlet.homepage_url {
                    a href=(home) rel="noopener" target="_blank"
                      class="mt-4 inline-block font-mono text-[11px] text-accent hover:underline" {
                        (home) " ↗"
                    }
                }
            }

            h2 class="mb-5 flex items-baseline gap-2 border-b-2 border-accent pb-2 text-xs font-bold uppercase tracking-widest text-ink" {
                (i18n::t("News"))
                span class="font-mono text-ink-muted" { (outlet.article_count) }
            }
            @if articles.is_empty() {
                p class="py-10 text-center text-sm text-ink-muted" { (i18n::t("No news yet.")) }
            } @else {
                (ui::news::index(&articles, &country.slug, session.as_ref().is_some_and(|s| s.is_admin)))
                (ui::pagination::controls(&base, current, total_pages))
            }
        }
    };

    Ok(ui::layout::document(
        Some(&outlet.name),
        session.is_some(),
        session.as_ref().is_some_and(|s| s.is_admin),
        content,
    ))
}
