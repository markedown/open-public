use axum::extract::{Path, Query, State};
use maud::{html, Markup};
use serde::Deserialize;

use crate::auth::AuthSession;
use crate::error::PageError;
use crate::i18n;
use crate::ui;
use crate::ui::breadcrumb::Crumb;

const PAGE_SIZE: i64 = 50;

#[derive(Deserialize)]
pub struct Params {
    page: Option<i64>,
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
    let page = params.page.unwrap_or(1).max(1);
    let offset = (page - 1) * PAGE_SIZE;
    let people = db::people::list_filtered(&pool, country.id, &query, PAGE_SIZE, offset).await?;
    // The party each person currently sits in, so a visitor scanning the list
    // sees affiliation without opening each page. Absent for anyone with no
    // current membership (a head of state, a former member).
    let party_of: std::collections::HashMap<i64, db::people::PersonParty> =
        db::people::current_parties(&pool, country.id)
            .await?
            .into_iter()
            .map(|pp| (pp.person_id, pp))
            .collect();
    let total = db::people::count_filtered(&pool, country.id, &query).await?;
    let list_url = format!("/{}/people", country.slug);

    let content = html! {
        section {
            (ui::breadcrumb::breadcrumbs(&[
                Crumb { label: country.name.clone(), href: Some(format!("/{}", country.slug)) },
                Crumb { label: i18n::t("People").to_string(), href: None },
            ]))
            (ui::page_header(i18n::t("People"), None, None))
            (ui::search::bar(&list_url, "#people-results", &query))

            // The search box swaps this container in place as the query changes.
            div id="people-results" {
                p class="mb-4 text-xs font-bold uppercase tracking-widest text-ink-muted" {
                    span class="font-mono" { (total) }
                    " " (i18n::t("people listed"))
                }
                @if people.is_empty() {
                    p class="py-12 text-center text-sm text-ink-muted" { (i18n::t("No people found.")) }
                } @else {
                    ul class="op-card divide-y divide-hairline-light px-4" {
                        @for p in &people {
                            li {
                                a href={"/" (country.slug) "/people/" (p.slug)}
                                  class="group flex items-center gap-3 py-3" {
                                    span class="flex h-7 w-7 shrink-0 items-center justify-center rounded-full border border-hairline bg-paper-sunken font-mono text-[10px] font-semibold text-ink-muted" {
                                        (ui::initials(&p.full_name))
                                    }
                                    span class="grow text-sm font-medium text-ink transition-colors group-hover:text-accent" {
                                        (p.full_name)
                                    }
                                    @if let Some(pp) = party_of.get(&p.id) {
                                        @if let Some(ref sn) = pp.short_name {
                                            span class="shrink-0" { (ui::badge::party_chip(sn, pp.color.as_deref())) }
                                        }
                                    }
                                    @if let Some(ref place) = p.birth_place {
                                        span class="hidden shrink-0 text-xs text-ink-muted sm:inline" { (place) }
                                    }
                                }
                            }
                        }
                    }
                    (pagination(page, total, &country.slug, &query))
                }
            }
        }
    };

    Ok(ui::layout::document(
        Some("People"),
        session.is_some(),
        session.as_ref().is_some_and(|s| s.is_admin),
        content,
    ))
}

/// Percent-encode a query for a URL parameter (each non-unreserved byte as %XX,
/// so UTF-8 and spaces round-trip).
fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn pagination(page: i64, total: i64, country_slug: &str, query: &str) -> Markup {
    let total_pages = ((total as f64) / (PAGE_SIZE as f64)).ceil() as i64;
    if total_pages <= 1 {
        return html! {};
    }
    // Carry the search query across pages (percent-encoded so spaces and
    // non-ASCII survive in the URL).
    let q = if query.is_empty() {
        String::new()
    } else {
        format!("&q={}", percent_encode(query))
    };
    let box_active = "rounded-lg border border-hairline px-4 py-2 text-ink-muted transition-colors hover:border-accent hover:text-accent";
    let box_disabled =
        "rounded-lg border border-hairline px-4 py-2 text-ink-muted/40 cursor-default";
    html! {
        nav class="mt-8 flex items-center justify-center gap-4 text-sm" {
            @if page > 1 {
                a href={"/" (country_slug) "/people?page=" (page - 1) (q)} class=(box_active) { (i18n::t("Previous")) }
            } @else {
                span class=(box_disabled) { (i18n::t("Previous")) }
            }
            span class="font-mono text-ink-muted" {
                (page) " / " (total_pages)
            }
            @if page < total_pages {
                a href={"/" (country_slug) "/people?page=" (page + 1) (q)} class=(box_active) { (i18n::t("Next")) }
            } @else {
                span class=(box_disabled) { (i18n::t("Next")) }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::percent_encode;

    #[test]
    fn percent_encodes_spaces_and_utf8() {
        assert_eq!(percent_encode("mehmet"), "mehmet");
        assert_eq!(percent_encode("ali veli"), "ali%20veli");
        // Non-ASCII is encoded per UTF-8 byte.
        assert_eq!(percent_encode("ş"), "%C5%9F");
    }
}
