use std::sync::Arc;

use axum::extract::FromRef;
use db::Pool;

use crate::mail::Mailer;

/// Shared application state. Handlers that only need the database can still
/// extract `State<db::Pool>` thanks to the `FromRef` impl below.
#[derive(Clone)]
pub struct AppState {
    pub pool: Pool,
    pub secret: Arc<Vec<u8>>,
    pub mailer: Mailer,
    pub cookie_secure: bool,
}

impl FromRef<AppState> for Pool {
    fn from_ref(state: &AppState) -> Pool {
        state.pool.clone()
    }
}
