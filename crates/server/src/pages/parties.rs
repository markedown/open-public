use axum::extract::{Path, Query, State};
use maud::{html, Markup};
use serde::Deserialize;

use crate::auth::AuthSession;
use crate::error::PageError;
use crate::i18n;
use crate::ui;
use crate::ui::breadcrumb::Crumb;

#[derive(Deserialize)]
pub struct Params {
    q: Option<String>,
}

pub async fn list(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path(country): Path<String>,
    Query(params): Query<Params>,
) -> Result<Markup, PageError> {
    let country = db::country::get_by_slug(&pool, &country)
        .await?
        .ok_or(PageError::NotFound)?;

    let query = params.q.unwrap_or_default();
    let parties = db::parties::list_filtered(&pool, country.id, &query).await?;
    let total = parties.len();
    // Each row shows the party's current size, the same figure the country
    // seat bar computes, so the index conveys who is large and who is small.
    let members: std::collections::HashMap<i64, i64> =
        db::parties::member_counts(&pool, country.id)
            .await?
            .into_iter()
            .collect();
    let list_url = format!("/{}/parties", country.slug);

    let content = html! {
        section {
            (ui::breadcrumb::breadcrumbs(&[
                Crumb { label: country.name.clone(), href: Some(format!("/{}", country.slug)) },
                Crumb { label: i18n::t("Parties").to_string(), href: None },
            ]))
            (ui::page_header(i18n::t("Parties"), None, None))
            (ui::search::bar(&list_url, "#parties-results", &query))

            div id="parties-results" {
                p class="mb-4 text-xs font-bold uppercase tracking-widest text-ink-muted" {
                    span class="font-mono" { (total) }
                    " " (i18n::t("parties listed"))
                }
            @if parties.is_empty() {
                p class="py-12 text-center text-sm text-ink-muted" { (i18n::t("No parties found.")) }
            } @else {
                ul class="op-card divide-y divide-hairline-light px-4" {
                    @for p in &parties {
                        li {
                            a href={"/" (country.slug) "/parties/" (p.slug)}
                              class="group flex items-center gap-3 py-3.5" {
                                span class="w-16 shrink-0" {
                                    @if let Some(ref sn) = p.short_name {
                                        (ui::badge::party_chip(sn, p.color.as_deref()))
                                    }
                                }
                                span class="text-sm font-medium text-ink transition-colors group-hover:text-accent" {
                                    (p.name)
                                }
                                @let n = members.get(&p.id).copied().unwrap_or(0);
                                @if n > 0 {
                                    span class="ml-auto shrink-0 font-mono text-xs text-ink-muted" {
                                        (n) " " (i18n::t("members"))
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
        Some("Parties"),
        session.is_some(),
        session.as_ref().is_some_and(|s| s.is_admin),
        content,
    ))
}
