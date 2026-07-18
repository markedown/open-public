use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use serde::Deserialize;

use crate::auth::AuthSession;
use crate::error::PageError;
use crate::state::AppState;
use crate::ui;
use crate::ui::follow::FollowState;

#[derive(Deserialize)]
pub struct ToggleForm {
    /// Where to return without JavaScript. Only in-site paths are honoured, so
    /// this cannot be used as an open redirect.
    next: Option<String>,
}

/// Toggle whether the signed-in user follows an entity. On an HTMX request the
/// updated button is swapped in place; without JavaScript the entity page
/// reloads. Requires a session (the route is behind auth).
pub async fn toggle(
    session: AuthSession,
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((entity_type, entity_id)): Path<(String, i64)>,
    Form(form): Form<ToggleForm>,
) -> Result<Response, PageError> {
    if !db::follows::is_kind(&entity_type) {
        return Err(PageError::NotFound);
    }

    let now_following = if db::follows::is_following(
        &state.pool,
        session.user_id,
        &entity_type,
        entity_id,
    )
    .await?
    {
        db::follows::unfollow(&state.pool, session.user_id, &entity_type, entity_id).await?;
        false
    } else {
        db::follows::follow(&state.pool, session.user_id, &entity_type, entity_id).await?;
        true
    };

    // Only in-site paths are accepted, so the toggle cannot bounce a visitor to
    // another origin.
    let next = form
        .next
        .filter(|n| n.starts_with('/') && !n.starts_with("//"))
        .unwrap_or_else(|| "/".to_string());

    if headers.contains_key("hx-request") {
        let state = if now_following {
            FollowState::Following
        } else {
            FollowState::NotFollowing
        };
        Ok(ui::follow::button(&entity_type, entity_id, state, &next).into_response())
    } else {
        Ok(Redirect::to(&next).into_response())
    }
}
