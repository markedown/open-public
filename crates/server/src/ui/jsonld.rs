//! schema.org structured data, so a search engine can describe a page rather
//! than guess at it.
//!
//! The objects repeat only what the page itself shows: a key the page leaves out
//! is left out of the object too, so the structured data can never claim more
//! than a reader can verify on the page. Every address in it is absolute, and
//! when the deployment has not been told its own origin the whole object is
//! omitted, because a relative address there is a wrong claim about where the
//! site lives.

use chrono::NaiveDate;
use maud::{html, Markup, PreEscaped};
use serde_json::{json, Map, Value};

use crate::i18n;
use crate::ui::breadcrumb::Crumb;

const CONTEXT: &str = "https://schema.org";

/// A person as the page shows them. A field left `None` is a fact the page does
/// not display, and it stays out of the object.
pub struct PersonData<'a> {
    pub name: &'a str,
    /// The photo address the page uses, as the page uses it (a same-origin
    /// path); the object makes it absolute.
    pub image: Option<&'a str>,
    pub birth_date: Option<NaiveDate>,
    /// The current office, in the words the page names it.
    pub job_title: Option<&'a str>,
    /// The current party: its name and the page's own link to it.
    pub affiliation: Option<(&'a str, &'a str)>,
    pub wikidata_id: Option<&'a str>,
}

/// Render `value` as an `application/ld+json` script element.
///
/// The serialized JSON is escaped on the way out: a name or a quotation could
/// contain `</script>`, and written raw that would close the element early and
/// leave the rest of the document to be parsed as markup. The JSON unicode
/// escapes stand for the same characters, so what a search engine parses is
/// unchanged.
pub fn script(value: Value) -> Markup {
    let Ok(json) = serde_json::to_string(&value) else {
        // A `Value` assembled here always serializes; if one somehow cannot,
        // emit nothing rather than a half-written element.
        return html! {};
    };
    let safe = json.replace('<', "\\u003c").replace('&', "\\u0026");
    html! { script type="application/ld+json" { (PreEscaped(safe)) } }
}

/// The site itself, with the search box the header carries on every page.
pub fn web_site(origin: &str) -> Value {
    json!({
        "@context": CONTEXT,
        "@type": "WebSite",
        "name": i18n::SITE_NAME,
        "url": origin,
        "potentialAction": {
            "@type": "SearchAction",
            "target": format!("{origin}/search?q={{search_term_string}}"),
            "query-input": "required name=search_term_string",
        },
    })
}

/// The trail behind the current page. `None` under two crumbs: one crumb is
/// where the visitor already stands, and a list of it says nothing.
pub fn breadcrumb_list(origin: &str, current_url: &str, items: &[Crumb]) -> Option<Value> {
    if items.len() < 2 {
        return None;
    }
    let entries: Vec<Value> = items
        .iter()
        .enumerate()
        .map(|(i, crumb)| {
            json!({
                "@type": "ListItem",
                "position": i + 1,
                "name": crumb.label,
                // The last crumb is the page itself, which the trail does not
                // link; its address is the request's own.
                "item": match &crumb.href {
                    Some(href) => format!("{origin}{href}"),
                    None => current_url.to_string(),
                },
            })
        })
        .collect();
    Some(json!({
        "@context": CONTEXT,
        "@type": "BreadcrumbList",
        "itemListElement": entries,
    }))
}

/// A person, from the facts the detail page displays.
pub fn person(origin: &str, url: &str, data: &PersonData) -> Value {
    let mut obj = object("Person");
    obj.insert("name".to_string(), json!(data.name));
    obj.insert("url".to_string(), json!(url));
    if let Some(src) = data.image {
        obj.insert("image".to_string(), json!(format!("{origin}{src}")));
    }
    if let Some(birth_date) = data.birth_date {
        obj.insert(
            "birthDate".to_string(),
            json!(birth_date.format("%Y-%m-%d").to_string()),
        );
    }
    if let Some(title) = data.job_title {
        obj.insert("jobTitle".to_string(), json!(title));
    }
    if let Some((name, href)) = data.affiliation {
        obj.insert(
            "affiliation".to_string(),
            json!({
                "@type": "Organization",
                "name": name,
                "url": format!("{origin}{href}"),
            }),
        );
    }
    if let Some(id) = data.wikidata_id {
        obj.insert("sameAs".to_string(), json!([wikidata_url(id)]));
    }
    Value::Object(obj)
}

/// A party, from the facts the detail page displays.
pub fn political_party(
    url: &str,
    name: &str,
    wikidata_id: Option<&str>,
    founding_date: Option<NaiveDate>,
) -> Value {
    let mut obj = object("PoliticalParty");
    obj.insert("name".to_string(), json!(name));
    obj.insert("url".to_string(), json!(url));
    if let Some(id) = wikidata_id {
        obj.insert("sameAs".to_string(), json!([wikidata_url(id)]));
    }
    if let Some(date) = founding_date {
        obj.insert(
            "foundingDate".to_string(),
            json!(date.format("%Y-%m-%d").to_string()),
        );
    }
    Value::Object(obj)
}

/// The `WebSite` object for the home page, empty without a known origin.
pub fn web_site_script() -> Markup {
    match i18n::site_origin() {
        Some(origin) => script(web_site(&origin)),
        None => html! {},
    }
}

/// The `BreadcrumbList` for a rendered trail, empty without a known origin or
/// under two crumbs.
pub fn breadcrumb_script(items: &[Crumb]) -> Markup {
    match (i18n::site_origin(), i18n::request_url()) {
        (Some(origin), Some(url)) => match breadcrumb_list(&origin, &url, items) {
            Some(value) => script(value),
            None => html! {},
        },
        _ => html! {},
    }
}

/// The `Person` object for the detail page, empty without a known origin.
pub fn person_script(data: &PersonData) -> Markup {
    match (i18n::site_origin(), i18n::request_url()) {
        (Some(origin), Some(url)) => script(person(&origin, &url, data)),
        _ => html! {},
    }
}

/// The `PoliticalParty` object for the detail page, empty without a known
/// origin.
pub fn political_party_script(
    name: &str,
    wikidata_id: Option<&str>,
    founding_date: Option<NaiveDate>,
) -> Markup {
    match i18n::request_url() {
        Some(url) => script(political_party(&url, name, wikidata_id, founding_date)),
        None => html! {},
    }
}

/// An object with its type context already in place.
fn object(kind: &str) -> Map<String, Value> {
    let mut obj = Map::new();
    obj.insert("@context".to_string(), json!(CONTEXT));
    obj.insert("@type".to_string(), json!(kind));
    obj
}

/// The Wikidata page for an entity id, the same address the page links.
fn wikidata_url(id: &str) -> String {
    format!("https://www.wikidata.org/wiki/{id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The JSON inside a rendered script element.
    fn embedded(markup: Markup) -> Value {
        let page = markup.into_string();
        let open = r#"<script type="application/ld+json">"#;
        let start = page.find(open).expect("a jsonld script element") + open.len();
        let end = page[start..].find("</script>").expect("the closing tag");
        serde_json::from_str(&page[start..start + end]).expect("the jsonld parses")
    }

    fn crumb(label: &str, href: Option<&str>) -> Crumb {
        Crumb {
            label: label.to_string(),
            href: href.map(str::to_string),
        }
    }

    fn bare_person() -> PersonData<'static> {
        PersonData {
            name: "Nobody",
            image: None,
            birth_date: None,
            job_title: None,
            affiliation: None,
            wikidata_id: None,
        }
    }

    #[test]
    fn a_name_cannot_close_the_script_element_it_sits_in() {
        let data = PersonData {
            name: "Ada</script><b>Lovelace & Co",
            ..bare_person()
        };
        let page = script(person("https://site.test", "https://site.test/p", &data)).into_string();
        // The injected tag never appears raw; only the element's own closing
        // tag does.
        assert!(!page.contains("</script><b>"));
        assert!(page.contains("\\u003c/script>\\u003cb>"));
        assert!(page.contains("\\u0026"));
        // And the escapes parse back to the original string.
        let value = embedded(script(person(
            "https://site.test",
            "https://site.test/p",
            &data,
        )));
        assert_eq!(value["name"], "Ada</script><b>Lovelace & Co");
    }

    #[test]
    fn person_keeps_only_the_facts_the_page_shows() {
        let value = person("https://site.test", "https://site.test/p", &bare_person());
        let obj = value.as_object().unwrap();
        assert_eq!(obj["@type"], "Person");
        assert_eq!(obj["url"], "https://site.test/p");
        for key in ["image", "birthDate", "jobTitle", "affiliation", "sameAs"] {
            assert!(!obj.contains_key(key), "{key} should be omitted");
        }

        let data = PersonData {
            name: "Ayşe Yılmaz",
            image: Some("/img/abc"),
            birth_date: NaiveDate::from_ymd_opt(1970, 5, 3),
            job_title: Some("Milletvekili"),
            affiliation: Some(("Test Partisi", "/tr/parties/test-partisi")),
            wikidata_id: Some("Q100"),
        };
        let value = person("https://site.test", "https://site.test/p", &data);
        assert_eq!(value["image"], "https://site.test/img/abc");
        assert_eq!(value["birthDate"], "1970-05-03");
        assert_eq!(value["jobTitle"], "Milletvekili");
        assert_eq!(
            value["affiliation"]["url"],
            "https://site.test/tr/parties/test-partisi"
        );
        assert_eq!(
            value["sameAs"],
            json!(["https://www.wikidata.org/wiki/Q100"])
        );
    }

    #[test]
    fn a_party_omits_what_the_page_does_not_display() {
        let value = political_party("https://site.test/p", "Test Partisi", None, None);
        let obj = value.as_object().unwrap();
        assert_eq!(obj["@type"], "PoliticalParty");
        assert!(!obj.contains_key("sameAs"));
        assert!(!obj.contains_key("foundingDate"));

        let value = political_party(
            "https://site.test/p",
            "Test Partisi",
            Some("Q9"),
            NaiveDate::from_ymd_opt(2010, 1, 1),
        );
        assert_eq!(value["foundingDate"], "2010-01-01");
        assert_eq!(value["sameAs"], json!(["https://www.wikidata.org/wiki/Q9"]));
    }

    #[test]
    fn a_breadcrumb_list_needs_two_crumbs_and_absolute_items() {
        assert!(breadcrumb_list(
            "https://site.test",
            "https://site.test/x",
            &[crumb("People", None)]
        )
        .is_none());

        let crumbs = [
            crumb("Test Ulke", Some("/tr")),
            crumb("People", Some("/tr/people")),
            crumb("Ayşe Yılmaz", None),
        ];
        let value = breadcrumb_list(
            "https://site.test",
            "https://site.test/tr/people/ayse",
            &crumbs,
        )
        .unwrap();
        let items = value["itemListElement"].as_array().unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0]["position"], 1);
        assert_eq!(items[0]["item"], "https://site.test/tr");
        // The unlinked last crumb is the current page.
        assert_eq!(items[2]["item"], "https://site.test/tr/people/ayse");
    }

    #[test]
    fn no_origin_means_no_structured_data() {
        // Outside a request the deployment's origin is unknown, so every
        // emitter stays silent rather than writing relative URLs.
        assert_eq!(web_site_script().into_string(), "");
        assert_eq!(
            breadcrumb_script(&[crumb("A", Some("/a")), crumb("B", None)]).into_string(),
            ""
        );
        assert_eq!(person_script(&bare_person()).into_string(), "");
        assert_eq!(political_party_script("N", None, None).into_string(), "");
    }

    #[tokio::test]
    async fn a_known_origin_makes_the_same_calls_emit() {
        let crumbs = [crumb("Test Ulke", Some("/tr")), crumb("People", None)];
        let data = bare_person();
        let (site, trail, person, party) =
            crate::i18n::with_url("https://site.test/tr/people".to_string(), async {
                (
                    web_site_script().into_string(),
                    breadcrumb_script(&crumbs).into_string(),
                    person_script(&data).into_string(),
                    political_party_script("N", None, None).into_string(),
                )
            })
            .await;
        for markup in [site, trail, person, party] {
            assert!(!markup.is_empty());
        }
    }
}
