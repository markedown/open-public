use axum::extract::{Path, State};
use maud::{html, Markup};

use crate::auth::AuthSession;
use crate::error::PageError;
use crate::i18n;
use crate::ui;
use crate::ui::breadcrumb::Crumb;

/// A country's full political timeline on its own page.
pub async fn detail(
    State(pool): State<db::Pool>,
    session: Option<AuthSession>,
    Path(country): Path<String>,
) -> Result<Markup, PageError> {
    let country = db::country::get_by_slug(&pool, &country)
        .await?
        .ok_or(PageError::NotFound)?;
    let events = db::events::for_country(&pool, country.id).await?;

    let content = html! {
        section {
            (ui::breadcrumb::breadcrumbs(&[
                Crumb { label: country.name.clone(), href: Some(format!("/{}", country.slug)) },
                Crumb { label: i18n::t("Timeline").to_string(), href: None },
            ]))
            (ui::page_header(i18n::t("Timeline"), None, None))
            @if events.is_empty() {
                p class="py-12 text-center text-sm text-ink-muted" { (i18n::t("No events yet.")) }
            } @else {
                (ui::event::entries(&events))
            }
        }
    };

    Ok(ui::layout::document(
        Some(i18n::t("Timeline")),
        session.is_some(),
        session.as_ref().is_some_and(|s| s.is_admin),
        content,
    ))
}
