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
    let country = db::country::get_by_slug(&pool, &country)
        .await?
        .ok_or(PageError::NotFound)?;

    let person = db::people::get_by_slug_in_country(&pool, &slug, country.id)
        .await?
        .ok_or(PageError::NotFound)?;

    let memberships = db::people::memberships(&pool, person.id).await?;
    let roles = db::people::roles(&pool, person.id).await?;
    let mut education = db::people::education(&pool, person.id).await?;
    crate::content::localize_education(&pool, &mut education).await?;
    let mut attributes = db::people::attributes(&pool, person.id).await?;
    crate::content::localize_attributes(&pool, &mut attributes).await?;
    let mut polls = db::polls::full_for_person(&pool, person.id).await?;
    crate::content::localize_polls(&pool, &mut polls).await?;
    let news = db::news::for_person(&pool, person.id).await?;
    let mut statements = db::statements::for_person(&pool, person.id).await?;
    crate::content::localize_statements(&pool, &mut statements).await?;
    let loc = crate::content::Localized::load(&pool, "person", person.id).await?;
    let summary = loc.get("summary", person.summary.as_deref());
    // In a presidential country a person is a compass contestant, so their page
    // links to the candidate compass that ranks them; a parliamentary politician
    // is not scored individually and gets no such link.
    let compass_href = db::compass::person_is_contestant(&pool, person.id)
        .await?
        .then(|| format!("/{}/compass/{}", country.slug, db::compass::SCOPE_PERSON));

    // Empty content sections stay hidden for readers; an admin edits from the
    // dedicated backoffice page for this entity, not inline on the public page.
    let is_admin = session.as_ref().is_some_and(|s| s.is_admin);
    let manage_href =
        is_admin.then(|| format!("/admin/person/{}?country={}", person.slug, country.slug));

    // A signed-in visitor can follow this person to surface it in their feed.
    let follow_state = match &session {
        Some(s) => {
            if db::follows::is_following(&pool, s.user_id, "person", person.id).await? {
                ui::follow::FollowState::Following
            } else {
                ui::follow::FollowState::NotFollowing
            }
        }
        None => ui::follow::FollowState::Anonymous,
    };
    let follow_next = format!("/{}/people/{}", country.slug, person.slug);
    let approval = crate::pages::approve::panel_for(
        &pool,
        db::approvals::Entity::Person(person.id),
        "person",
        person.id,
        session.as_ref(),
        &follow_next,
    )
    .await?;
    let approval_history =
        crate::pages::approve::history_for(&pool, db::approvals::Entity::Person(person.id)).await?;

    let current_party = memberships.iter().find(|m| m.end_date.is_none());
    let current_role = roles
        .iter()
        .filter(|r| r.end_date.is_none())
        .max_by_key(|r| r.start_date);

    let content = html! {
        article {
            (ui::hero_band(html! {
                (ui::breadcrumb::trail(&[
                    Crumb { label: country.name.clone(), href: Some(format!("/{}", country.slug)) },
                    Crumb { label: i18n::t("People").to_string(), href: Some(format!("/{}/people", country.slug)) },
                    Crumb { label: person.full_name.clone(), href: None },
                ]))
                div class="mt-5 flex flex-wrap items-center gap-x-7 gap-y-5" {
                    (ui::portrait(person.photo_url.as_deref(), &person.full_name, "size-28 sm:size-32"))
                    div class="min-w-0 flex-[1_1_20rem]" {
                        h1 class="text-[40px] font-extrabold leading-[1.05] tracking-tight text-ink sm:text-[48px]" {
                            (person.full_name)
                        }
                        div class="mt-3 flex flex-wrap items-center gap-2" {
                            @if let Some(r) = current_role {
                                span class="inline-flex items-center rounded-full border border-hairline bg-paper px-3 py-0.5 text-[15px] font-semibold leading-6 text-ink" {
                                    @if let Some(ref title) = r.title { (title) } @else { (r.role_type) }
                                    @if let Some(ref d) = r.district { " · " (d) }
                                }
                            }
                            @if let Some(m) = current_party {
                                (ui::badge::party_badge(
                                    m.party_short_name.as_deref().unwrap_or(&m.party_name),
                                    &m.party_slug, m.party_color.as_deref(), &country.slug))
                            }
                        }
                    }
                    div class="flex flex-wrap items-center gap-2" {
                        (ui::follow::button("person", person.id, follow_state, &follow_next))
                        (ui::election::compass_cta(compass_href.as_deref()))
                        @if let Some(ref href) = manage_href {
                            a href=(href)
                              class="inline-flex min-h-11 items-center rounded-[10px] border border-hairline-strong bg-paper px-4 text-[16px] font-semibold text-ink transition-colors hover:border-ink" {
                                (i18n::t("Manage"))
                            }
                        }
                    }
                }
                @if let Some(text) = summary {
                    div class="mt-6 max-w-[42em] text-[20px] leading-relaxed text-ink" {
                        (ui::translated::prose(
                            text,
                            loc.is_translated("summary").then_some(person.summary.as_deref()).flatten(),
                        ))
                    }
                } @else if let Some(r) = current_role {
                    // No summary written yet: the current role, in a sentence's
                    // place, is what a visitor came to learn.
                    p class="mt-6 max-w-[42em] text-[20px] leading-relaxed text-ink" {
                        @if let Some(ref title) = r.title { (title) } @else { (r.role_type) }
                        @if let Some(ref org) = r.org { ", " (org) }
                        @if let Some(d) = r.start_date {
                            span class="text-ink-muted" { " · " (i18n::t("In office since")) " " (fmt::date(Some(d))) }
                        }
                    }
                }
            }))

            div class="flex flex-wrap items-start gap-x-12 gap-y-8" {
                div class="min-w-0 flex-[999_1_36rem]" {
                    @if !roles.is_empty() {
                        section class="mb-12" {
                            (ui::section_header(i18n::t("Roles"), None))
                            (ui::timeline_entry::timeline_list(
                                roles.iter().map(|r| ui::timeline_entry::Entry {
                                    // The role's own name is the title; a category
                                    // label would only repeat the section heading.
                                    kind: String::new(),
                                    title: r.title.clone().unwrap_or_else(|| r.role_type.clone()),
                                    subtitle: role_subtitle(r),
                                    date_range: fmt::date_range(r.start_date, r.end_date),
                                    link_href: None,
                                    source_url: Some(r.source_url.clone()),
                                })
                            ))
                        }
                    }

                    @if !memberships.is_empty() {
                        section class="mb-12" {
                            (ui::section_header(i18n::t("Party memberships"), None))
                            (ui::timeline_entry::timeline_list(
                                memberships.iter().map(|m| ui::timeline_entry::Entry {
                                    kind: String::new(),
                                    title: m.party_name.clone(),
                                    subtitle: String::new(),
                                    date_range: fmt::date_range(m.start_date, m.end_date),
                                    link_href: Some(format!("/{}/parties/{}", country.slug, m.party_slug)),
                                    source_url: Some(m.source_url.clone()),
                                })
                            ))
                        }
                    }

                    section class="mb-12" { (approval) }

                    (approval_history)

                    (ui::background::section(&education, &attributes))
            (ui::statement::statement_section(&statements, None))

            (ui::news::news_section(&news, &country.slug, None))

            (ui::poll_widget::poll_previews(&polls, &country.slug, None, None, None))

                    (ui::references::references(person.wikidata_id.as_deref(), person.photo_license.as_deref()))
                }

                aside aria-label=(i18n::t("At a glance"))
                    class="min-w-0 flex-[1_1_18rem] rounded-[12px] bg-paper-sunken px-5 py-5 lg:max-w-sm" {
                    h2 class="mb-2 text-[18px] font-extrabold text-ink" { (i18n::t("At a glance")) }
                    dl class="text-[16px]" {
                        @if let Some(bd) = person.birth_date {
                            div class="border-b border-hairline py-2.5" {
                                dt class="text-ink-muted" { (i18n::t("Born")) }
                                dd class="mt-0.5 font-semibold text-ink" {
                                    (fmt::date(Some(bd)))
                                    @if let Some(ref place) = person.birth_place { ", " (place) }
                                }
                            }
                        }
                        @if let Some(m) = current_party {
                            div class="border-b border-hairline py-2.5" {
                                dt class="text-ink-muted" { (i18n::t("Party")) }
                                dd class="mt-0.5 font-semibold" {
                                    a href={"/" (country.slug) "/parties/" (m.party_slug)} class="op-link" { (m.party_name) }
                                }
                            }
                        }
                        div class="border-b border-hairline py-2.5 last:border-0" {
                            dt class="text-ink-muted" { (i18n::t("Country")) }
                            dd class="mt-0.5 font-semibold" {
                                a href={"/" (country.slug)} class="op-link" { (country.name) }
                            }
                        }
                        @if let Some(ref q) = person.wikidata_id {
                            div class="py-2.5" {
                                dt class="text-ink-muted" { "Wikidata" }
                                dd class="mt-0.5 font-semibold" {
                                    a href={"https://www.wikidata.org/wiki/" (q)} rel="noopener" class="op-link" { (q) }
                                }
                            }
                        }
                    }
                }
            }
        }
    };

    // What a search result or a shared link should say about this person: their
    // current role and party, which is what someone searching their name wants
    // to see, falling back to our own summary and then to the country.
    let current_role = roles.iter().find(|r| r.end_date.is_none());
    let current_party = memberships.iter().find(|m| m.end_date.is_none());
    let description = match (current_role, current_party) {
        (Some(r), Some(m)) => format!(
            "{} · {} · {}",
            person.full_name,
            r.title.as_deref().unwrap_or(&r.role_type),
            m.party_name
        ),
        (Some(r), None) => format!(
            "{} · {}",
            person.full_name,
            r.title.as_deref().unwrap_or(&r.role_type)
        ),
        (None, Some(m)) => format!("{} · {}", person.full_name, m.party_name),
        (None, None) => summary
            .map(str::to_string)
            .unwrap_or_else(|| format!("{} · {}", person.full_name, country.name)),
    };

    Ok(ui::layout::document_described(
        Some(&person.full_name),
        Some(&description),
        session.is_some(),
        session.as_ref().is_some_and(|s| s.is_admin),
        content,
    ))
}

fn role_subtitle(r: &domain::models::Role) -> String {
    let mut parts = Vec::new();
    if let Some(ref org) = r.org {
        parts.push(org.as_str());
    }
    if let Some(ref dist) = r.district {
        parts.push(dist.as_str());
    }
    parts.join(" · ")
}
