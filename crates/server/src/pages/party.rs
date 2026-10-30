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

    let party = db::parties::get_by_slug_in_country(&pool, &slug, country_model.id)
        .await?
        .ok_or(PageError::NotFound)?;

    let leader = db::parties::leader(&pool, party.id).await?;
    let seats = db::parties::parliament_seats(&pool, party.id).await?;
    let alliance_rows = db::parties::alliance_members(&pool, party.id).await?;
    let members = db::parties::members(&pool, party.id).await?;
    let mut polls = db::polls::full_for_party(&pool, party.id).await?;
    crate::content::localize_polls(&pool, &mut polls).await?;
    let news = db::news::for_party(&pool, party.id).await?;
    let mut statements = db::statements::for_party(&pool, party.id).await?;
    crate::content::localize_statements(&pool, &mut statements).await?;
    let electoral_history = db::elections::history_for_party(&pool, party.id).await?;
    let events = db::events::for_party(&pool, party.id).await?;
    let loc = crate::content::Localized::load(&pool, "party", party.id).await?;
    let summary = loc.get("summary", party.summary.as_deref());
    // The compass scores this party, so its page should offer a way into it,
    // but only when the country actually has party-scope positions.
    let compass_href =
        (db::compass::count_theses(&pool, country_model.id, db::compass::SCOPE_PARTY).await? > 0)
            .then(|| format!("/{}/compass", country_model.slug));

    // Empty content sections stay hidden for readers; an admin edits from the
    // dedicated backoffice page for this entity, not inline on the public page.
    let is_admin = session.as_ref().is_some_and(|s| s.is_admin);
    let manage_href =
        is_admin.then(|| format!("/admin/party/{}?country={}", party.slug, country_model.slug));

    let follow_state = match &session {
        Some(s) => {
            if db::follows::is_following(&pool, s.user_id, "party", party.id).await? {
                ui::follow::FollowState::Following
            } else {
                ui::follow::FollowState::NotFollowing
            }
        }
        None => ui::follow::FollowState::Anonymous,
    };
    let follow_next = format!("/{}/parties/{}", country_model.slug, party.slug);
    let approval = crate::pages::approve::panel_for(
        &pool,
        db::approvals::Entity::Party(party.id),
        "party",
        party.id,
        session.as_ref(),
        &follow_next,
    )
    .await?;
    let approval_history =
        crate::pages::approve::history_for(&pool, db::approvals::Entity::Party(party.id)).await?;

    let current_members: Vec<_> = members.iter().filter(|m| m.end_date.is_none()).collect();
    let former_members: Vec<_> = members.iter().filter(|m| m.end_date.is_some()).collect();

    let mut alliances: Vec<(String, Vec<db::parties::AllianceMemberRow>)> = Vec::new();
    for row in alliance_rows {
        match alliances.last_mut() {
            Some((name, parties)) if *name == row.alliance_name => parties.push(row),
            _ => alliances.push((row.alliance_name.clone(), vec![row])),
        }
    }

    let content = html! {
        article {
            (ui::hero_band(html! {
                (ui::breadcrumb::trail(&[
                    Crumb { label: i18n::t_dyn(&country_model.name).to_string(), href: Some(format!("/{}", country_model.slug)) },
                    Crumb { label: i18n::t("Parties").to_string(), href: Some(format!("/{}/parties", country_model.slug)) },
                    Crumb { label: party.name.clone(), href: None },
                ]))
                div class="mt-4 flex flex-wrap items-start justify-between gap-4" {
                    div class="min-w-0" {
                        h1 class="flex flex-wrap items-center gap-x-4 gap-y-2 text-[40px] font-extrabold leading-[1.05] tracking-tight text-ink sm:text-[48px]" {
                            // The party's colour shows only as this dot, inside
                            // the identity, never as page chrome.
                            span aria-hidden="true" class="size-6 shrink-0 rounded-full"
                                style={"background-color:" (ui::css_color(party.color.as_deref(), "#b7bec6"))} {}
                            (party.name)
                        }
                        @if let Some(ref sn) = party.short_name {
                            p class="mt-2 text-[18px] text-ink-muted" { (sn) }
                        }
                    }
                    div class="flex flex-wrap items-center gap-2" {
                        (ui::follow::button("party", party.id, follow_state, &follow_next))
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
                    div class="mt-5 max-w-[42em] text-[20px] leading-relaxed text-ink" {
                        (ui::translated::prose(
                            text,
                            loc.is_translated("summary").then_some(party.summary.as_deref()).flatten(),
                        ))
                    }
                }
                @if !party.ideology_tags.is_empty() {
                    div class="mt-4 flex flex-wrap gap-1.5" {
                        @for tag in &party.ideology_tags {
                            span class="rounded-full border border-hairline bg-paper px-3 py-0.5 text-[15px] text-ink-muted" {
                                (tag)
                            }
                        }
                    }
                }
                @if seats > 0 || party.founded_date.is_some() || leader.is_some() {
                    div class="mt-7 grid gap-4 sm:grid-cols-3" {
                        @if seats > 0 {
                            (ui::fact_card(html! { (seats) }, i18n::t("seats"), None))
                        }
                        @if let Some(ref l) = leader {
                            (ui::fact_card(
                                html! {
                                    a href={"/" (country_model.slug) "/people/" (l.person_slug)}
                                      class="op-link text-[24px] font-bold" { (l.person_name) }
                                },
                                i18n::t("Leader"),
                                Some(ui::citation::source_marker(&l.source_url)),
                            ))
                        }
                        @if let Some(fd) = party.founded_date {
                            (ui::fact_card(html! { (fd.format("%Y")) }, i18n::t("Founded"), Some(html! { (fmt::date(Some(fd))) })))
                        }
                    }
                }
            }))

            section class="mb-12" { (approval) }

            (approval_history)

            // Alliance membership: the coalition and its full roster, this party
            // ringed.
            @for (name, parties) in &alliances {
                section class="mb-12" {
                    (ui::section_header(&format!("{} · {}", i18n::t("Alliance"), name), None))
                    div class="flex flex-wrap gap-1.5" {
                        @for p in parties {
                            @if let Some(ref sn) = p.short_name {
                                @if p.party_id == party.id {
                                    span class="inline-flex rounded-full ring-2 ring-ink ring-offset-2 ring-offset-paper" {
                                        (ui::badge::party_chip(sn, p.color.as_deref()))
                                    }
                                } @else {
                                    a href={"/" (country_model.slug) "/parties/" (p.slug)} class="inline-flex transition-opacity hover:opacity-80" {
                                        (ui::badge::party_chip(sn, p.color.as_deref()))
                                    }
                                }
                            }
                        }
                    }
                }
            }

            @if !members.is_empty() {
                section class="mb-12" {
                    (ui::section_header(
                        i18n::t("Members"),
                        Some(html! { span class="text-[16px] text-ink-muted" { (members.len()) } }),
                    ))

                    @if !current_members.is_empty() {
                        ul class="grid gap-x-8 sm:grid-cols-2 lg:grid-cols-3" {
                            @for m in &current_members {
                                li class="flex items-center gap-3 border-b border-hairline-light py-2.5" {
                                    (ui::portrait(None, &m.person_name, "size-8"))
                                    a href={"/" (country_model.slug) "/people/" (m.person_slug)}
                                      class="grow text-[16px] text-ink hover:underline" {
                                        (m.person_name)
                                    }
                                }
                            }
                        }
                    }

                    @if !former_members.is_empty() {
                        h3 class="mb-4 mt-10 flex items-baseline gap-2 text-[20px] font-bold text-ink" {
                            (i18n::t("Former members"))
                            span class="text-[16px] font-normal text-ink-muted" { (former_members.len()) }
                        }
                        (ui::timeline_entry::timeline_list(former_members.iter().map(|m| {
                            ui::timeline_entry::Entry {
                                kind: String::new(),
                                title: m.person_name.clone(),
                                subtitle: String::new(),
                                date_range: fmt::date_range(m.start_date, m.end_date),
                                link_href: Some(format!("/{}/people/{}", country_model.slug, m.person_slug)),
                                source_url: Some(m.source_url.clone()),
                            }
                        })))
                    }
                }
            }

            (ui::election::party_history_chart(&electoral_history, party.color.as_deref()))

            (ui::election::party_history(&electoral_history, &country_model.slug))

            (ui::event::timeline(&events, None))

            (ui::statement::statement_section(&statements, None))

            (ui::news::news_section(&news, &country_model.slug, None))

            (ui::poll_widget::poll_previews(&polls, &country_model.slug, None, None, None))

            (ui::references::references(party.wikidata_id.as_deref(), None))
        }
    };

    // A party is searched by name; the description says where it sits and how
    // large it is, which is what distinguishes it in a list of results.
    let description = if seats > 0 {
        format!(
            "{} · {} · {} {}",
            party.name,
            i18n::t_dyn(&country_model.name),
            seats,
            i18n::t("members")
        )
    } else {
        format!("{} · {}", party.name, i18n::t_dyn(&country_model.name))
    };

    Ok(ui::layout::document_described(
        Some(&party.name),
        Some(&description),
        session.is_some(),
        session.as_ref().is_some_and(|s| s.is_admin),
        content,
    ))
}
