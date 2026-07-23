use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use maud::{html, Markup};

use crate::i18n;
use crate::ui;

/// An error a page handler can return. Handlers use `Result<Markup, PageError>`
/// and `?`; a `db::Error` converts to a server error, and a missing row becomes
/// `NotFound`. This replaces the per-page `not_found`/`error_page` helpers and
/// sends the correct HTTP status instead of rendering an error body with 200.
pub enum PageError {
    NotFound,
    Server,
}

impl From<db::Error> for PageError {
    /// Every database failure in a page handler arrives here, because handlers
    /// use `?` rather than matching. That makes this the one place where the
    /// cause still exists, so it is recorded before it is turned into a status
    /// code: without it a production 500 leaves nothing behind to explain
    /// itself, and there are over two hundred `?` sites that would all fail
    /// silently.
    ///
    /// The message goes to the log, never to the response. What went wrong with
    /// a query is an operational detail, not something a visitor is told.
    fn from(e: db::Error) -> Self {
        tracing::error!(error = %e, "a database call failed while rendering a page");
        PageError::Server
    }
}

impl IntoResponse for PageError {
    fn into_response(self) -> Response {
        match self {
            PageError::NotFound => (StatusCode::NOT_FOUND, not_found()).into_response(),
            PageError::Server => {
                (StatusCode::INTERNAL_SERVER_ERROR, server_error()).into_response()
            }
        }
    }
}

/// The 404 page, also reused where a handler wants to render "not found" inline.
pub fn not_found() -> Markup {
    ui::layout::document(
        Some("Not found"),
        false,
        false,
        html! {
            div class="flex flex-col items-center py-20 text-center" {
                p class="font-mono text-4xl font-semibold text-ink" { "404" }
                p class="mt-2 text-sm text-ink-muted" { (i18n::t("Page not found.")) }
                a href="/" class="mt-6 text-sm text-accent hover:underline" {
                    (i18n::t("Back to home"))
                }
            }
        },
    )
}

fn server_error() -> Markup {
    ui::layout::document(
        Some("Error"),
        false,
        false,
        html! {
            div class="flex flex-col items-center py-20 text-center" {
                p class="font-mono text-4xl font-semibold text-ink" { "500" }
                p class="mt-2 max-w-prose text-sm text-ink-muted" {
                    (i18n::t("Something went wrong. Please try again."))
                }
                a href="/" class="mt-6 text-sm text-accent hover:underline" {
                    (i18n::t("Back to home"))
                }
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_found_sends_404() {
        assert_eq!(
            PageError::NotFound.into_response().status(),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn server_error_sends_500() {
        assert_eq!(
            PageError::Server.into_response().status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn a_database_failure_becomes_a_server_error_and_says_nothing_to_the_visitor() {
        let e: PageError = db::Error::UniqueViolation.into();
        assert!(matches!(e, PageError::Server));
        // The page a visitor gets carries no detail about the query that
        // failed: that belongs in the log, which the conversion above writes.
        let body = server_error().into_string();
        assert!(body.contains("500"));
        assert!(!body.to_lowercase().contains("sql"));
        assert!(!body.to_lowercase().contains("database"));
    }
}
