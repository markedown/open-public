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

    // Empty content sections stay hidden for readers; an admin edits from the
    // dedicated backoffice page for this entity, not inline on the public page.
    let is_admin = session.as_ref().is_some_and(|s| s.is_admin);
    let manage_href =
        is_admin.then(|| format!("/admin/party/{}?country={}", party.slug, country_model.slug));

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
            (ui::breadcrumb::breadcrumbs(&[
                Crumb { label: country_model.name.clone(), href: Some(format!("/{}", country_model.slug)) },
                Crumb { label: i18n::t("Parties").to_string(), href: Some(format!("/{}/parties", country_model.slug)) },
                Crumb { label: party.name.clone(), href: None },
            ]))
            // Identity block (direction 1a, subtle accent): the party color shows
            // only as the chip and a short 3px underline rule. A hero record card.
            header class={"mb-12 border-[1.5px] border-ink bg-paper-raised p-6 sm:p-8 " (ui::CORNER_TICK)} {
                div class="flex flex-wrap items-center gap-3" {
                    @if let Some(ref sn) = party.short_name {
                        (ui::badge::party_chip(sn, party.color.as_deref()))
                    }
                    h1 class="font-serif text-4xl font-semibold tracking-tight text-ink sm:text-[44px]" {
                        (party.name)
                    }
                }
                div class="mt-3 h-[3px] w-16"
                    style={"background-color:" (party.color.as_deref().unwrap_or("#33527a"))} {}

                @if let Some(ref l) = leader {
                    p class="mt-5 flex flex-wrap items-baseline gap-2 text-sm text-ink" {
                        span class="text-[11px] font-bold uppercase tracking-widest text-ink-muted" {
                            (i18n::t("Leader"))
                        }
                        a href={"/" (country_model.slug) "/people/" (l.person_slug)}
                          class="font-medium text-ink transition-colors hover:text-accent" {
                            (l.person_name)
                        }
                        (ui::citation::source_marker(&l.source_url))
                    }
                }

                // Headline stats as verifiable figures (mono).
                div class="mt-6 flex flex-wrap gap-x-10 gap-y-4" {
                    @if seats > 0 {
                        div {
                            div class="font-mono text-3xl font-semibold text-ink" { (seats) }
                            div class="mt-0.5 text-[11px] font-bold uppercase tracking-widest text-ink-muted" {
                                (i18n::t("seats"))
                            }
                        }
                    }
                    @if let Some(fd) = party.founded_date {
                        div {
                            div class="font-mono text-3xl font-semibold text-ink" { (fd.format("%Y")) }
                            div class="mt-0.5 text-[11px] font-bold uppercase tracking-widest text-ink-muted" {
                                (i18n::t("Founded"))
                            }
                        }
                    }
                }

                @if !party.ideology_tags.is_empty() {
                    div class="mt-6 flex flex-wrap gap-1.5" {
                        @for tag in &party.ideology_tags {
                            span class="border border-hairline px-2 py-0.5 text-xs text-ink-muted" {
                                (tag)
                            }
                        }
                    }
                }

                @if let Some(ref href) = manage_href {
                    a href=(href)
                      class="mt-6 inline-flex items-center gap-1.5 border border-hairline px-3 py-1.5 text-[11px] font-bold uppercase tracking-wide text-ink-muted transition-colors hover:border-accent hover:text-accent" {
                        (i18n::t("Manage"))
                    }
                }
            }

            @if let Some(text) = summary {
                div class="mb-12" {
                    (ui::translated::prose(
                        text,
                        loc.is_translated("summary").then_some(party.summary.as_deref()).flatten(),
                    ))
                }
            }

            // Alliance membership: the coalition and its full roster, this party
            // ringed.
            @for (name, parties) in &alliances {
                section class="mb-12" {
                    h2 class="mb-5 border-b-2 border-accent pb-2 text-xs font-bold uppercase tracking-widest text-ink" {
                        (i18n::t("Alliance")) " · " (name)
                    }
                    div class="flex flex-wrap gap-1.5" {
                        @for p in parties {
                            @if let Some(ref sn) = p.short_name {
                                @if p.party_id == party.id {
                                    span class="inline-flex ring-1 ring-ink ring-offset-2 ring-offset-paper" {
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
                    h2 class="mb-5 flex items-baseline gap-2 border-b-2 border-accent pb-2 text-xs font-bold uppercase tracking-widest text-ink" {
                        (i18n::t("Members"))
                        span class="font-mono text-ink-muted" { (members.len()) }
                    }

                    @if !current_members.is_empty() {
                        ul class="grid gap-x-10 sm:grid-cols-2" {
                            @for m in &current_members {
                                li class="flex items-center gap-3 border-b border-hairline-light py-2.5" {
                                    span class="flex h-7 w-7 shrink-0 items-center justify-center border border-ink font-mono text-[10px] font-semibold text-ink" {
                                        (ui::initials(&m.person_name))
                                    }
                                    a href={"/" (country_model.slug) "/people/" (m.person_slug)}
                                      class="grow text-sm font-medium text-ink transition-colors hover:text-accent" {
                                        (m.person_name)
                                    }
                                }
                            }
                        }
                    }

                    @if !former_members.is_empty() {
                        h3 class="mb-3 mt-8 flex items-baseline gap-2 text-xs font-bold uppercase tracking-widest text-ink-muted" {
                            (i18n::t("Former members"))
                            span class="font-mono" { (former_members.len()) }
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

            (ui::election::party_history(&electoral_history))

            (ui::event::timeline(&events, None))

            (ui::statement::statement_section(&statements, None))

            (ui::news::news_section(&news, &country_model.slug, None))

            (ui::poll_widget::poll_previews(&polls, &country_model.slug, None))

            (ui::references::references(party.wikidata_id.as_deref(), None))
        }
    };

    Ok(ui::layout::document(
        Some(&party.name),
        session.is_some(),
        session.as_ref().is_some_and(|s| s.is_admin),
        content,
    ))
}
