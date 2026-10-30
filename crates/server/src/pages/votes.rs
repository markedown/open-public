//! Recorded votes in a country's chamber: the list, one vote with its split by
//! party and every member's choice, and all of one person's votes.

use axum::extract::{Path, Query, State};
use maud::{html, Markup};
use serde::Deserialize;

use crate::auth::AuthSession;
use crate::error::PageError;
use crate::fmt;
use crate::i18n;
use crate::ui;
use crate::ui::breadcrumb::Crumb;
use crate::ui::roll_call::{absence_note, choice_label, choice_pill, legend, stacked_bar};

const PAGE_SIZE: i64 = 30;

#[derive(Deserialize)]
pub struct Params {
    page: Option<i64>,
    q: Option<String>,
}

/// Which page to show of `total` rows, kept inside what exists so a very large
/// number cannot overflow the offset.
fn page_of(page: Option<i64>, total: i64) -> (i64, i64) {
    let last = ((total + PAGE_SIZE - 1) / PAGE_SIZE).max(1);
    (page.unwrap_or(1).clamp(1, last), last)
}

fn pager(base: &str, page: i64, last: i64, query: &str) -> Markup {
    if last <= 1 {
        return html! {};
    }
    let q = if query.is_empty() {
        String::new()
    } else {
        format!("&q={}", urlencoding(query))
    };
    html! {
        nav class="mt-8 flex items-center justify-center gap-4 text-[16px]" {
            @if page > 1 {
                a href={(base) "?page=" (page - 1) (q)} class="op-link" { (i18n::t("Previous")) }
            }
            span class="text-ink-muted" { (page) " / " (last) }
            @if page < last {
                a href={(base) "?page=" (page + 1) (q)} class="op-link" { (i18n::t("Next")) }
            }
        }
    }
}

fn urlencoding(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
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
    let total = db::roll_calls::count(&pool, country.id, &query).await?;
    let (page, last) = page_of(params.page, total);
    let rows =
        db::roll_calls::list(&pool, country.id, &query, PAGE_SIZE, (page - 1) * PAGE_SIZE).await?;
    let url = format!("/{}/votes", country.slug);

    let content = html! {
        section {
            (ui::breadcrumb::breadcrumbs(&[
                Crumb { label: i18n::t_dyn(&country.name).to_string(), href: Some(format!("/{}", country.slug)) },
                Crumb { label: i18n::t("Votes").to_string(), href: None },
            ]))
            (ui::page_header(i18n::t("Votes"), None, None))
            p class="mb-6 max-w-prose text-[18px] text-ink-muted" {
                (i18n::t("Votes in which the chamber recorded how each member voted. Most decisions are taken without such a record, so this is not everything a parliament decided."))
            }
            (ui::search::bar(&url, "#vote-results", &query))
            div id="vote-results" {
                p class="mb-4 text-[14px] font-bold text-ink-muted" {
                    (total) " " (i18n::t("recorded votes"))
                }
                @if rows.is_empty() {
                    p class="py-12 text-center text-[16px] text-ink-muted" { (i18n::t("No votes found.")) }
                } @else {
                    ul class="space-y-3" {
                        @for r in &rows {
                            li class="op-card px-5 py-4" {
                                a href={"/" (country.slug) "/vote/" (r.slug)} class="op-link text-[18px] font-bold" { (r.title) }
                                div class="mt-0.5 text-[15px] text-ink-muted" { (fmt::date(Some(r.held_on))) }
                                div class="mt-3" { (stacked_bar(r.yes, r.no, r.abstain, r.absent)) }
                                (totals(r.yes, r.no, r.abstain, r.absent))
                            }
                        }
                    }
                    (legend())
                    (pager(&url, page, last, &query))
                }
            }
        }
    };
    Ok(ui::layout::document(
        Some(i18n::t("Votes")),
        session.is_some(),
        session.as_ref().is_some_and(|s| s.is_admin),
        content,
    ))
}

fn totals(yes: i64, no: i64, abstain: i64, absent: i64) -> Markup {
    html! {
        p class="mt-2 flex flex-wrap gap-x-4 text-[15px] text-ink-muted" {
            @for (choice, n) in [("yes", yes), ("no", no), ("abstain", abstain), ("absent", absent)] {
                span { (choice_label(choice)) " " strong class="font-bold text-ink" { (n) } }
            }
        }
    }
}

pub async fn detail(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path((country, slug)): Path<(String, String)>,
) -> Result<Markup, PageError> {
    let country = db::country::get_by_slug(&pool, &country)
        .await?
        .ok_or(PageError::NotFound)?;
    let vote = db::roll_calls::get(&pool, country.id, &slug)
        .await?
        .ok_or(PageError::NotFound)?;
    let parties = db::roll_calls::by_party(&pool, vote.id).await?;
    let members = db::roll_calls::members(&pool, vote.id).await?;

    let content = html! {
        (ui::hero_band(html! {
            (ui::breadcrumb::breadcrumbs(&[
                Crumb { label: i18n::t_dyn(&country.name).to_string(), href: Some(format!("/{}", country.slug)) },
                Crumb { label: i18n::t("Votes").to_string(), href: Some(format!("/{}/votes", country.slug)) },
                Crumb { label: vote.title.clone(), href: None },
            ]))
            h1 class="text-[34px] font-extrabold leading-[1.1] tracking-tight text-ink sm:text-[42px]" { (vote.title) }
            p class="mt-2 text-[18px] text-ink-muted" {
                (i18n::t_dyn(&vote.chamber)) " · " (fmt::date(Some(vote.held_on))) " "
                (ui::citation::source_marker(&vote.source_url))
            }
            div class="mt-6 grid grid-cols-2 gap-4 sm:grid-cols-4" {
                (ui::fact_card(html! { (vote.yes) }, i18n::t("Yes"), None))
                (ui::fact_card(html! { (vote.no) }, i18n::t("No"), None))
                (ui::fact_card(html! { (vote.abstain) }, i18n::t("Abstained"), None))
                (ui::fact_card(html! { (vote.absent) }, i18n::t("Did not vote"), None))
            }
        }))

        section class="mb-12" {
            (ui::section_header(i18n::t("How the parties voted"), None))
            ul class="divide-y divide-hairline-light" {
                @for p in &parties {
                    li class="py-3" {
                        div class="flex flex-wrap items-baseline justify-between gap-x-4" {
                            @if let Some(ref party) = p.party_slug {
                                a href={"/" (country.slug) "/parties/" (party)} class="op-link text-[17px] font-semibold" {
                                    (p.name.as_deref().unwrap_or(party))
                                }
                            } @else {
                                span class="text-[17px] font-semibold text-ink" { (i18n::t("Independent")) }
                            }
                            (totals(p.yes, p.no, p.abstain, p.absent))
                        }
                        div class="mt-2" { (stacked_bar(p.yes, p.no, p.abstain, p.absent)) }
                    }
                }
            }
            (legend())
            (absence_note())
            p class="mt-3 max-w-prose text-[15px] text-ink-muted" {
                (i18n::t("This shows how members voted. Whether the motion passed can depend on rules this count does not show, so see the official record."))
            }
        }

        section class="mb-12" {
            (ui::section_header(i18n::t("How each member voted"), None))
            @for p in &parties {
                details class="border-b border-hairline-light py-3" {
                    summary class="cursor-pointer text-[17px] font-semibold text-ink" {
                        (p.name.as_deref().unwrap_or(i18n::t("Independent")))
                    }
                    ul class="mt-3 grid gap-x-6 gap-y-2 sm:grid-cols-2" {
                        @for m in members.iter().filter(|m| m.party_slug == p.party_slug) {
                            li class="flex items-center justify-between gap-3" {
                                a href={"/" (country.slug) "/people/" (m.person_slug)} class="op-link min-w-0 truncate text-[16px]" { (m.person_name) }
                                (choice_pill(&m.choice))
                            }
                        }
                    }
                }
            }
        }
    };
    Ok(ui::layout::document(
        Some(&vote.title),
        session.is_some(),
        session.as_ref().is_some_and(|s| s.is_admin),
        content,
    ))
}

pub async fn for_person(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path((country, slug)): Path<(String, String)>,
    Query(params): Query<Params>,
) -> Result<Markup, PageError> {
    let country = db::country::get_by_slug(&pool, &country)
        .await?
        .ok_or(PageError::NotFound)?;
    let person = db::people::get_by_slug_in_country(&pool, &slug, country.id)
        .await?
        .ok_or(PageError::NotFound)?;
    let record = db::roll_calls::record(&pool, person.id).await?;
    let (page, last) = page_of(params.page, record.total);
    let votes =
        db::roll_calls::for_person(&pool, person.id, PAGE_SIZE, (page - 1) * PAGE_SIZE).await?;
    let url = format!("/{}/people/{}/votes", country.slug, person.slug);

    let content = html! {
        section {
            (ui::breadcrumb::breadcrumbs(&[
                Crumb { label: i18n::t_dyn(&country.name).to_string(), href: Some(format!("/{}", country.slug)) },
                Crumb { label: person.full_name.clone(), href: Some(format!("/{}/people/{}", country.slug, person.slug)) },
                Crumb { label: i18n::t("Votes").to_string(), href: None },
            ]))
            (ui::page_header(&person.full_name, None, None))
            (ui::roll_call::record_line(record))
            (ui::roll_call::person_list(&country.slug, &votes))
            (absence_note())
            (pager(&url, page, last, ""))
        }
    };
    Ok(ui::layout::document(
        Some(i18n::t("Votes")),
        session.is_some(),
        session.as_ref().is_some_and(|s| s.is_admin),
        content,
    ))
}
