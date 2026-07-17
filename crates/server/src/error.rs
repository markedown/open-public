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
    fn from(_: db::Error) -> Self {
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
            p class="text-ink" { (i18n::t("Something went wrong. Please try again.")) }
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
}
