use domain::models::NewsItem;
use maud::{html, Markup};

use crate::fmt;
use crate::i18n;
use crate::ui;

/// A person mentioned by a news item, as a bordered chip that reads as its own
/// element (distinct from a party's colour badge) and links to the person.
pub fn person_chip(name: &str, slug: &str, country: &str) -> Markup {
    html! {
        a href={"/" (country) "/people/" (slug)}
          class="inline-flex items-center rounded-md border border-hairline px-2 py-0.5 text-[14px] font-medium text-ink transition-colors hover:border-accent hover:text-link" {
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
                          class="block text-base font-semibold leading-snug text-ink transition-colors hover:text-link" {
                            (it.headline)
                        }
                        @if admin {
                            a href={"/admin/news/" (it.id) "/edit"}
                              class="shrink-0 text-[15px] font-bold text-link hover:underline" {
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
        a href=(href) class="text-[15px] font-semibold text-link transition-colors hover:underline" {
            "+ " (i18n::t("Add news"))
        }
    });
    html! {
        section class="mb-8" {
            (ui::section_header(i18n::t("News"), add))
            @if items.is_empty() {
                p class="text-[16px] text-ink-muted" { (i18n::t("No news yet.")) }
            } @else {
                ul class="space-y-3" {
                    @for it in items {
                        li {
                            a href={"/" (country) "/news/" (it.id)}
                              class="block text-[16px] font-medium text-ink transition-colors hover:text-link" {
                                (it.headline)
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The "also reported by" block on a news item's page: the other outlets that
/// covered the same story, each linking to its own page. It shows the spread of
/// coverage rather than judging any of it; renders nothing when there is none.
pub fn related_coverage(items: &[db::news::RelatedNews], country: &str) -> Markup {
    if items.is_empty() {
        return html! {};
    }
    html! {
        section class="mt-8 border-t border-hairline pt-6" {
            h2 class="mb-3 text-[15px] font-bold text-ink-muted" {
                (i18n::t("Also reported by"))
            }
            ul class="space-y-2.5" {
                @for it in items {
                    li {
                        a href={"/" (country) "/news/" (it.id)}
                          class="group block" {
                            @if let Some(ref o) = it.outlet {
                                span class="text-[15px] text-ink-muted" {
                                    (o)
                                    @if let Some(d) = it.published_at {
                                        " · " (fmt::date(Some(d.date_naive())))
                                    }
                                }
                            }
                            span class="block text-[16px] font-medium text-ink transition-colors group-hover:text-link" {
                                (it.headline)
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn empty_news_and_coverage_renders_nothing() {
        assert_eq!(news_section(&[], "tr", None).into_string(), "");
        assert_eq!(related_coverage(&[], "tr").into_string(), "");
    }

    #[test]
    fn an_empty_section_stays_open_for_an_admin_who_can_add_news() {
        // A visitor sees no section at all; an admin sees the emptiness and the
        // way to fix it.
        let body = news_section(&[], "tr", Some("/tr/admin/news/add")).into_string();
        assert!(body.contains(r#"href="/tr/admin/news/add""#));
        assert!(body.contains("Add news"));
        assert!(body.contains("No news yet."));
    }

    #[test]
    fn related_coverage_names_the_outlet_and_when_it_published() {
        let items = [db::news::RelatedNews {
            id: 7,
            headline: "Test Partisi yeni tasari sundu".to_string(),
            outlet: Some("Test Gazetesi".to_string()),
            published_at: Some(Utc.with_ymd_and_hms(2024, 3, 5, 12, 0, 0).unwrap()),
        }];
        let body = related_coverage(&items, "tr").into_string();
        assert!(body.contains("Also reported by"));
        assert!(body.contains(r#"href="/tr/news/7""#));
        assert!(body.contains("Test Gazetesi"));
        assert!(body.contains("5 Mar 2024"));
    }

    #[test]
    fn related_coverage_omits_a_publication_date_the_source_does_not_carry() {
        let items = [db::news::RelatedNews {
            id: 8,
            headline: "Test Partisi yeni tasari sundu".to_string(),
            outlet: Some("Test Gazetesi".to_string()),
            published_at: None,
        }];
        let body = related_coverage(&items, "tr").into_string();
        assert!(body.contains("Test Gazetesi"));
        // A missing date is left out, not rendered as a placeholder.
        assert!(!body.contains('?'));
    }

    #[test]
    fn the_news_index_links_a_headline_and_offers_an_edit_to_admins_only() {
        let card = db::news::NewsCard {
            id: 9,
            headline: "Test Partisi toplandi".to_string(),
            our_summary: None,
            url: "https://example.test/haber".to_string(),
            outlet: Some("Test Gazetesi".to_string()),
            published_at: None,
            people: vec![db::news::PersonRef {
                slug: "ayse-yilmaz".to_string(),
                name: "Ayse Yilmaz".to_string(),
            }],
            parties: vec![db::news::PartyRef {
                slug: "test-partisi".to_string(),
                short: "TP".to_string(),
                color: Some("#0055aa".to_string()),
            }],
        };
        let cards = [card];
        let body = index(&cards, "tr", true).into_string();
        assert!(body.contains(r#"href="/tr/news/9""#));
        assert!(body.contains(r#"href="/tr/people/ayse-yilmaz""#));
        assert!(body.contains(r#"href="/tr/parties/test-partisi""#));
        assert!(body.contains(r#"href="/admin/news/9/edit""#));

        // A visitor is not shown the edit affordance.
        let plain = index(&cards, "tr", false).into_string();
        assert!(!plain.contains("/edit"));
    }
}
