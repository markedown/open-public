//! The public correction channel: a visitor says what on a page reads wrong, an
//! editor checks it against that page's source and closes the report.
//!
//! No login is required, and the report is never public: it is read only in the
//! admin queue, so this form needs no moderation pipeline of its own. Neither
//! page is indexable, and neither promises a response time, because none is
//! committed to.

use axum::extract::{Query, State};
use axum::http::{HeaderName, HeaderValue};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use maud::{html, Markup};
use serde::Deserialize;

use crate::error::PageError;
use crate::state::AppState;
use crate::{i18n, ui};

/// The limits the schema enforces too. The form checks them first so a visitor
/// gets a sentence about what to fix rather than a server error.
const MAX_PAGE: usize = 300;
const MIN_DESCRIPTION: usize = 10;
const MAX_DESCRIPTION: usize = 2000;
const MAX_SOURCE_URL: usize = 500;
const MAX_EMAIL: usize = 254;

/// The page the report is about, carried in the query string by the footer link.
#[derive(Deserialize)]
pub struct PageQuery {
    page: Option<String>,
}

/// The submitted report. The two required fields are optional here so that a
/// form posted with one blank answers with a sentence about that field rather
/// than a framework error about a missing key.
#[derive(Deserialize)]
pub struct ReportForm {
    page: Option<String>,
    description: Option<String>,
    source_url: Option<String>,
    contact_email: Option<String>,
    altcha: Option<String>,
}

/// What a report broke, kept apart from its wording so the wording can be
/// translated without anything reading translated text back.
#[derive(Debug, PartialEq, Eq)]
enum Problem {
    Page,
    ShortDescription,
    LongDescription,
    SourceUrl,
    Email,
}

/// The first rule the report breaks, if any.
fn check(
    page: &str,
    description: &str,
    source_url: Option<&str>,
    contact_email: Option<&str>,
) -> Option<Problem> {
    // The queue renders the page as a link, so it is refused here rather than
    // left to the template if it could lead off the site.
    if !is_site_path(page) || page.chars().count() > MAX_PAGE {
        return Some(Problem::Page);
    }
    let len = description.chars().count();
    if len < MIN_DESCRIPTION {
        return Some(Problem::ShortDescription);
    }
    if len > MAX_DESCRIPTION {
        return Some(Problem::LongDescription);
    }
    if let Some(url) = source_url {
        if !starts_with_http(url) || url.chars().count() > MAX_SOURCE_URL {
            return Some(Problem::SourceUrl);
        }
    }
    if let Some(email) = contact_email {
        if !email.contains('@') || email.chars().count() > MAX_EMAIL {
            return Some(Problem::Email);
        }
    }
    None
}

/// Whether a reported page is a path on this site. A second leading slash makes
/// an address protocol-relative, and a backslash is read as a slash by browsers,
/// so `/\\host` is another origin too.
pub fn is_site_path(page: &str) -> bool {
    page.starts_with('/') && !page.starts_with("//") && !page.contains('\\')
}

/// Whether an address is a web address we may link to.
fn starts_with_http(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

fn message(problem: Problem) -> &'static str {
    match problem {
        Problem::Page => i18n::t("The page must be an address on this site, starting with \"/\"."),
        Problem::ShortDescription => {
            i18n::t("Please say what is wrong, in at least 10 characters.")
        }
        Problem::LongDescription => i18n::t("Please keep the description under 2000 characters."),
        Problem::SourceUrl => i18n::t("A source link must start with http:// or https://."),
        Problem::Email => i18n::t("That contact address does not look right."),
    }
}

/// A trimmed optional field; empty becomes `None`.
fn trimmed(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// The page as the visitor left it, so a rejection re-renders what they typed.
struct Fields<'a> {
    page: &'a str,
    description: &'a str,
    source_url: Option<&'a str>,
    contact_email: Option<&'a str>,
}

/// The `page` parameter is trusted only as a path on this site: anything else
/// (an absolute address, an over-long string) leaves the field empty rather than
/// turning the form into a link to somewhere else.
fn prefill(page: Option<&str>) -> String {
    match page {
        Some(p) if is_site_path(p) && p.chars().count() <= MAX_PAGE => p.to_string(),
        _ => String::new(),
    }
}

/// Both public pages of the correction channel are a form and a receipt, not
/// content, so a crawler is told to leave them out of its index.
fn noindex(page: Markup) -> Response {
    (
        [(
            HeaderName::from_static("x-robots-tag"),
            HeaderValue::from_static("noindex"),
        )],
        page,
    )
        .into_response()
}

/// `GET /corrections/new?page=/some/path`: the report form.
pub async fn form(Query(q): Query<PageQuery>) -> Response {
    let page = prefill(q.page.as_deref());
    noindex(form_page(
        &Fields {
            page: &page,
            description: "",
            source_url: None,
            contact_email: None,
        },
        None,
    ))
}

/// `POST /corrections/new`: validate, verify the captcha, store the report.
pub async fn create(State(state): State<AppState>, Form(form): Form<ReportForm>) -> Response {
    let page = form.page.unwrap_or_default();
    let description = form.description.unwrap_or_default();
    let page = page.trim();
    let description = description.trim();
    let source_url = trimmed(form.source_url);
    let contact_email = trimmed(form.contact_email);
    let fields = Fields {
        page,
        description,
        source_url: source_url.as_deref(),
        contact_email: contact_email.as_deref(),
    };

    if let Some(problem) = check(
        page,
        description,
        source_url.as_deref(),
        contact_email.as_deref(),
    ) {
        return noindex(form_page(&fields, Some(message(problem))));
    }
    if !crate::captcha::ok(&state.pool, &state.secret, form.altcha.as_deref()).await {
        return noindex(form_page(
            &fields,
            Some(i18n::t("Please complete the verification and try again.")),
        ));
    }

    let new = db::corrections::NewCorrection {
        page,
        description,
        source_url: source_url.as_deref(),
        contact_email: contact_email.as_deref(),
    };
    if db::corrections::create(&state.pool, &new).await.is_err() {
        return PageError::Server.into_response();
    }
    Redirect::to("/corrections/thanks").into_response()
}

/// `GET /corrections/thanks`: what happens to a report, in three sentences and
/// with no promise of a reply.
pub async fn thanks() -> Response {
    noindex(ui::layout::document(
        Some(i18n::t("Report sent")),
        false,
        false,
        html! {
            section class="mx-auto max-w-xl" {
                h1 class="text-3xl font-bold tracking-tight text-ink" {
                    (i18n::t("Report sent"))
                }
                p class="mt-4 max-w-prose text-[16px] text-ink-muted" {
                    (i18n::t("Every report is read against the source of the page it points to."))
                }
                p class="mt-2 max-w-prose text-[16px] text-ink-muted" {
                    (i18n::t("A confirmed error is corrected on the page, and the correction is recorded with its source."))
                }
                p class="mt-2 max-w-prose text-[16px] text-ink-muted" {
                    (i18n::t("If you left a contact address, it is deleted once the report is closed."))
                }
            }
        },
    ))
}

/// The report form, optionally re-rendered with one message about what to fix.
fn form_page(fields: &Fields, error: Option<&str>) -> Markup {
    let input = "mt-1 block w-full rounded-[10px] border border-hairline bg-paper-raised px-3 py-2 text-[16px] text-ink focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent";
    let note = "mt-1 text-[14px] text-ink-muted";
    ui::layout::document(
        Some(i18n::t("Report an error")),
        false,
        false,
        html! {
            section class="mx-auto max-w-xl" {
                h1 class="text-3xl font-bold tracking-tight text-ink" {
                    (i18n::t("Report an error"))
                }
                p class="mt-2 max-w-prose text-[16px] text-ink-muted" {
                    (i18n::t("Every claim here rests on a source. Say what reads wrong and on which page, and an editor checks it against that source."))
                }

                @if let Some(err) = error {
                    p class="mt-4 rounded-[10px] border-l-2 border-accent bg-accent-tint px-3 py-2 text-[16px] text-ink" { (err) }
                }

                form class="mt-6 space-y-4" method="post" action="/corrections/new" {
                    div {
                        label class="block text-[16px] font-medium text-ink" for="page" {
                            (i18n::t("Page"))
                        }
                        input type="text" name="page" id="page" required maxlength="300"
                            value=(fields.page) class=(input);
                    }
                    div {
                        label class="block text-[16px] font-medium text-ink" for="description" {
                            (i18n::t("What is wrong?"))
                        }
                        textarea name="description" id="description" rows="5" required
                            minlength="10" maxlength="2000" class=(input) {
                            (fields.description)
                        }
                    }
                    div {
                        label class="block text-[16px] font-medium text-ink" for="source_url" {
                            (i18n::t("Source URL (optional)"))
                        }
                        input type="url" name="source_url" id="source_url" maxlength="500"
                            value=(fields.source_url.unwrap_or("")) class=(input);
                        p class=(note) {
                            (i18n::t("Where you read something that says otherwise."))
                        }
                    }
                    div {
                        label class="block text-[16px] font-medium text-ink" for="contact_email" {
                            (i18n::t("Contact email (optional)"))
                        }
                        input type="email" name="contact_email" id="contact_email" maxlength="254"
                            value=(fields.contact_email.unwrap_or("")) class=(input);
                        // The address is optional and its purpose is stated, because it is
                        // personal data and the report is complete without it.
                        p class=(note) {
                            (i18n::t("Only used if an editor needs to ask a question about your report."))
                        }
                    }
                    (ui::captcha::widget())
                    (ui::button::primary(i18n::t("Send report")))
                }
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::{check, is_site_path, message, prefill, starts_with_http, Problem};

    const OK: (&str, &str) = (
        "/tr/people/ayse-yilmaz",
        "The birth date on this page is wrong.",
    );

    #[test]
    fn a_plain_report_passes() {
        assert_eq!(check(OK.0, OK.1, None, None), None);
        assert_eq!(
            check(
                OK.0,
                OK.1,
                Some("https://example.test/registry"),
                Some("reporter@example.test"),
            ),
            None
        );
    }

    #[test]
    fn a_page_must_be_a_path_on_this_site() {
        assert_eq!(check("", OK.1, None, None), Some(Problem::Page));
        assert_eq!(
            check("http://x.test/a", OK.1, None, None),
            Some(Problem::Page)
        );
        assert_eq!(
            check(&"/a".repeat(151), OK.1, None, None),
            Some(Problem::Page)
        );
        assert_eq!(check(&"/a".repeat(150), OK.1, None, None), None);
    }

    #[test]
    fn a_backslash_does_not_make_a_path_that_leaves_the_site() {
        assert!(is_site_path("/tr/people/x"));
        assert!(!is_site_path("/\\evil.test"));
        assert!(!is_site_path("//evil.test"));
        assert_eq!(check("/\\evil.test", OK.1, None, None), Some(Problem::Page));
    }

    #[test]
    fn a_description_must_say_something_and_stay_short() {
        assert_eq!(
            check(OK.0, "wrong", None, None),
            Some(Problem::ShortDescription)
        );
        assert_eq!(
            check(OK.0, &"x".repeat(2001), None, None),
            Some(Problem::LongDescription)
        );
        assert_eq!(check(OK.0, &"x".repeat(2000), None, None), None);
    }

    #[test]
    fn a_source_link_is_a_web_address_of_a_ordinary_length() {
        assert_eq!(
            check(OK.0, OK.1, Some("ftp://example.test/x"), None),
            Some(Problem::SourceUrl)
        );
        assert_eq!(
            check(
                OK.0,
                OK.1,
                Some(&format!("https://e.test/{}", "a".repeat(500))),
                None
            ),
            Some(Problem::SourceUrl)
        );
        assert!(starts_with_http("http://e.test"));
        assert!(!starts_with_http("HTTP://e.test"));
    }

    #[test]
    fn a_contact_address_is_an_address_of_a_ordinary_length() {
        assert_eq!(check(OK.0, OK.1, None, Some("nope")), Some(Problem::Email));
        assert_eq!(
            check(
                OK.0,
                OK.1,
                None,
                Some(&format!("{}@x.test", "a".repeat(248))),
            ),
            Some(Problem::Email)
        );
    }

    #[test]
    fn every_problem_has_its_own_sentence() {
        let problems = [
            Problem::Page,
            Problem::ShortDescription,
            Problem::LongDescription,
            Problem::SourceUrl,
            Problem::Email,
        ];
        let words: Vec<&str> = problems.map(message).to_vec();
        for w in &words {
            assert!(!w.is_empty());
        }
        // Two different problems never leave a visitor with the same sentence.
        for (i, a) in words.iter().enumerate() {
            for b in words.iter().skip(i + 1) {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn only_a_path_prefills_the_page_field() {
        assert_eq!(prefill(Some("/tr/people")), "/tr/people");
        assert_eq!(prefill(Some("http://evil.test/x")), "");
        assert_eq!(prefill(Some("//evil.test/x")), "");
        assert_eq!(prefill(Some("/\\evil.test/x")), "");
        assert_eq!(prefill(None), "");
        assert_eq!(prefill(Some(&"/a".repeat(151))), "");
    }
}
