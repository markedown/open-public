use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use maud::Markup;
use serde::Deserialize;

use db::approvals::{self, Entity, APPROVE, DISAPPROVE, NO_OPINION};

use crate::auth::AuthSession;
use crate::error::PageError;
use crate::state::AppState;
use crate::ui;

/// Build the approval panel for an entity, ready to drop into its page: the
/// current month's counts, plus the vote control appropriate to the viewer
/// (sign-in prompt, votable, or their recorded choice).
pub async fn panel_for(
    pool: &db::Pool,
    entity: Entity,
    entity_type: &str,
    entity_id: i64,
    session: Option<&AuthSession>,
    next: &str,
) -> Result<Markup, PageError> {
    let period = approvals::current_period();
    let tally = approvals::tally(pool, entity, period).await?;
    let viewer = match session {
        None => ui::approval::Viewer::Anonymous,
        Some(s) => match approvals::my_choice(pool, entity, period, s.user_id).await? {
            Some(pos) => ui::approval::Viewer::Voted(pos),
            None => ui::approval::Viewer::CanVote,
        },
    };
    Ok(ui::approval::panel(
        entity_type,
        entity_id,
        &tally,
        period,
        viewer,
        next,
    ))
}

/// The approval-over-time chart for an entity, ready to drop into its page.
/// Renders nothing until there are two months to compare.
pub async fn history_for(pool: &db::Pool, entity: Entity) -> Result<Markup, PageError> {
    let months = approvals::history(pool, entity, 12).await?;
    Ok(ui::approval::history_chart(&months))
}

#[derive(Deserialize)]
pub struct CastForm {
    /// `approve` | `disapprove` | `no_opinion`.
    choice: Option<String>,
    /// Where a no-JavaScript submit returns. Same-origin paths only.
    next: Option<String>,
}

/// Register the signed-in user's approval of an entity for the current month.
/// On an HTMX request the updated panel is swapped in place; without JavaScript
/// the entity page reloads. Requires a session (the route is behind auth). The
/// vote-chain trigger enforces one approval per user per entity per month, so a
/// repeat is refused rather than replacing the first.
pub async fn cast(
    session: AuthSession,
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((entity_type, entity_id)): Path<(String, i64)>,
    Form(form): Form<CastForm>,
) -> Result<Response, PageError> {
    let entity = match entity_type.as_str() {
        "person" => Entity::Person(entity_id),
        "party" => Entity::Party(entity_id),
        "coalition" => Entity::Coalition(entity_id),
        _ => return Err(PageError::NotFound),
    };
    if !approvals::exists(&state.pool, entity).await? {
        return Err(PageError::NotFound);
    }

    let position = match form.choice.as_deref() {
        Some("approve") => Some(APPROVE),
        Some("disapprove") => Some(DISAPPROVE),
        Some("no_opinion") => Some(NO_OPINION),
        _ => None,
    };
    let period = approvals::current_period();
    if let Some(position) = position {
        approvals::cast(&state.pool, entity, period, session.user_id, position).await?;
    }

    // Only in-site paths, never an off-origin bounce; a backslash is rejected
    // because a browser normalizes it to a slash in a Location header.
    let next = form
        .next
        .filter(|n| n.starts_with('/') && !n.starts_with("//") && !n.contains('\\'))
        .unwrap_or_else(|| "/".to_string());

    if headers.contains_key("hx-request") {
        let tally = approvals::tally(&state.pool, entity, period).await?;
        let choice = approvals::my_choice(&state.pool, entity, period, session.user_id).await?;
        let viewer = match choice {
            Some(pos) => ui::approval::Viewer::Voted(pos),
            None => ui::approval::Viewer::CanVote,
        };
        Ok(
            ui::approval::panel(&entity_type, entity_id, &tally, period, viewer, &next)
                .into_response(),
        )
    } else {
        Ok(Redirect::to(&next).into_response())
    }
}
