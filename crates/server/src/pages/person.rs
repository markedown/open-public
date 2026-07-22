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

    let current_party = memberships.iter().find(|m| m.end_date.is_none());
    let current_role = roles
        .iter()
        .filter(|r| r.end_date.is_none())
        .max_by_key(|r| r.start_date);

    let content = html! {
        article {
            (ui::breadcrumb::breadcrumbs(&[
                Crumb { label: country.name.clone(), href: Some(format!("/{}", country.slug)) },
                Crumb { label: i18n::t("People").to_string(), href: Some(format!("/{}/people", country.slug)) },
                Crumb { label: person.full_name.clone(), href: None },
            ]))
            // Identity block: a hero record card.
            header class="op-card mb-8 flex flex-col gap-6 p-6 sm:flex-row sm:items-start sm:p-8" {
                @if let Some(ref photo) = person.photo_url {
                    img src=(photo) alt=(&person.full_name)
                        class="h-24 w-24 shrink-0 rounded-xl border border-hairline object-cover"
                        loading="lazy";
                } @else {
                    div class="flex h-24 w-24 shrink-0 items-center justify-center rounded-xl border border-hairline bg-paper-sunken" {
                        span class="font-mono text-2xl font-semibold tracking-tight text-ink-muted" {
                            (ui::initials(&person.full_name))
                        }
                    }
                }
                div class="min-w-0" {
                    h1 class="text-3xl font-bold tracking-tight text-ink sm:text-4xl" {
                        (person.full_name)
                    }
                    div class="mt-3 flex flex-wrap items-center gap-2" {
                        @if let Some(m) = current_party {
                            (ui::badge::party_badge(
                                m.party_short_name.as_deref().unwrap_or(&m.party_name),
                                &m.party_slug, m.party_color.as_deref(), &country.slug))
                        }
                        @if let Some(r) = current_role {
                            span class="text-sm font-medium text-ink-muted" {
                                @if let Some(ref title) = r.title { (title) } @else { (r.role_type) }
                                @if let Some(ref d) = r.district { " · " (d) }
                            }
                        }
                    }
                    div class="mt-4 flex flex-wrap gap-x-4 gap-y-1 text-sm text-ink-muted" {
                        @if let Some(bd) = person.birth_date {
                            span class="inline-flex items-center gap-1.5" {
                                svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" {
                                    rect x="3" y="4" width="18" height="18" rx="2" ry="2" {}
                                    line x1="16" y1="2" x2="16" y2="6" {}
                                    line x1="8" y1="2" x2="8" y2="6" {}
                                    line x1="3" y1="10" x2="21" y2="10" {}
                                }
                                span class="font-mono" { (fmt::date(Some(bd))) }
                            }
                        }
                        @if let Some(ref place) = person.birth_place {
                            span class="inline-flex items-center gap-1.5" {
                                svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" {
                                    path d="M21 10c0 7-9 13-9 13s-9-6-9-13a9 9 0 0118 0z" {}
                                    circle cx="12" cy="10" r="3" {}
                                }
                                (place)
                            }
                        }
                    }
                    div class="mt-5 flex flex-wrap items-center gap-2" {
                        (ui::follow::button("person", person.id, follow_state, &follow_next))
                        (ui::election::compass_cta(compass_href.as_deref()))
                        @if let Some(ref href) = manage_href {
                            a href=(href)
                              class="inline-flex items-center gap-1.5 rounded-lg border border-hairline px-3 py-1.5 text-[11px] font-bold uppercase tracking-wide text-ink-muted transition-colors hover:border-accent hover:text-accent" {
                                (i18n::t("Manage"))
                            }
                        }
                    }
                }
            }

            @if let Some(text) = summary {
                section class="mb-8" {
                    (ui::translated::prose(
                        text,
                        loc.is_translated("summary").then_some(person.summary.as_deref()).flatten(),
                    ))
                }
            }

            (ui::background::section(&education, &attributes))

            @if !roles.is_empty() {
                section class="mb-8" {
                    (ui::section_header(i18n::t("Roles"), None))
                    (ui::timeline_entry::timeline_list(
                        roles.iter().map(|r| ui::timeline_entry::Entry {
                            kind: r.role_type.clone(),
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
                section class="mb-8" {
                    (ui::section_header(i18n::t("Party memberships"), None))
                    (ui::timeline_entry::timeline_list(
                        memberships.iter().map(|m| ui::timeline_entry::Entry {
                            kind: i18n::t("Party").to_string(),
                            title: m.party_name.clone(),
                            subtitle: String::new(),
                            date_range: fmt::date_range(m.start_date, m.end_date),
                            link_href: Some(format!("/{}/parties/{}", country.slug, m.party_slug)),
                            source_url: Some(m.source_url.clone()),
                        })
                    ))
                }
            }

            (ui::statement::statement_section(&statements, None))

            (ui::news::news_section(&news, &country.slug, None))

            (ui::poll_widget::poll_previews(&polls, &country.slug, None, None, None))

            (ui::references::references(person.wikidata_id.as_deref(), person.photo_license.as_deref()))
        }
    };

    Ok(ui::layout::document(
        Some(&person.full_name),
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
