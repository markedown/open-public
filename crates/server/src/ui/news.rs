use domain::models::NewsItem;
use maud::{html, Markup};

use crate::i18n;
use crate::ui;

/// A person mentioned by a news item, as a bordered chip that reads as its own
/// element (distinct from a party's colour badge) and links to the person.
pub fn person_chip(name: &str, slug: &str, country: &str) -> Markup {
    html! {
        a href={"/" (country) "/people/" (slug)}
          class="inline-flex items-center rounded-md border border-hairline px-2 py-0.5 text-xs font-medium text-ink transition-colors hover:border-accent hover:text-accent" {
            (name)
        }
    }
}

/// The parties and people a news item mentions, as chips: party colour badges
/// and bordered person chips, each linking into the platform.
pub fn mentions(
    people: &[db::news::PersonRef],
    parties: &[db::news::PartyRef],
    country: &str,
) -> Markup {
    html! {
        @if !people.is_empty() || !parties.is_empty() {
            div class="mt-2 flex flex-wrap items-center gap-2" {
                @for p in parties {
                    a href={"/" (country) "/parties/" (p.slug)}
                      class="inline-flex transition-opacity hover:opacity-80" {
                        (ui::badge::party_chip(&p.short, p.color.as_deref()))
                    }
                }
                @for p in people {
                    (person_chip(&p.name, &p.slug, country))
                }
            }
        }
    }
}

/// The country-wide news index: each item shows only its headline (linking to
/// the item's own page) and the parties and people it mentions, so the list
/// stays scannable. When `admin`, each item also carries an edit link.
pub fn index(items: &[db::news::NewsCard], country: &str, admin: bool) -> Markup {
    html! {
        ul class="divide-y divide-hairline-light" {
            @for it in items {
                li class="py-4 first:pt-0" {
                    div class="flex items-start justify-between gap-3" {
                        a href={"/" (country) "/news/" (it.id)}
                          class="block text-base font-semibold leading-snug text-ink transition-colors hover:text-accent" {
                            (it.headline)
                        }
                        @if admin {
                            a href={"/admin/news/" (it.id) "/edit"}
                              class="shrink-0 font-mono text-[10px] font-bold uppercase tracking-wide text-accent hover:underline" {
                                (i18n::t("Edit"))
                            }
                        }
                    }
                    (mentions(&it.people, &it.parties, country))
                }
            }
        }
    }
}

/// The "News" section on a person or party page. Each item links to its own page
/// on the platform; the full article body is never stored or shown. `add_href`,
/// set only for admins, surfaces an "add news" affordance and keeps the section
/// visible even when empty so an admin can start adding.
pub fn news_section(items: &[NewsItem], country: &str, add_href: Option<&str>) -> Markup {
    if items.is_empty() && add_href.is_none() {
        return html! {};
    }
    let add = add_href.map(|href| html! {
        a href=(href) class="text-[12px] font-semibold text-accent transition-colors hover:underline" {
            "+ " (i18n::t("Add news"))
        }
    });
    html! {
        section class="mb-8" {
            (ui::section_header(i18n::t("News"), add))
            @if items.is_empty() {
                p class="text-sm text-ink-muted" { (i18n::t("No news yet.")) }
            } @else {
                ul class="space-y-3" {
                    @for it in items {
                        li {
                            a href={"/" (country) "/news/" (it.id)}
                              class="block text-sm font-medium text-ink transition-colors hover:text-accent" {
                                (it.headline)
                            }
                        }
                    }
                }
            }
        }
    }
}
