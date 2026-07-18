use std::path::PathBuf;
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
    /// Directory holding re-encoded uploaded images, served under `/media`.
    pub asset_dir: Arc<PathBuf>,
    /// Optional home-page notice (a work-in-progress flag for production).
    pub site_notice: Option<Arc<str>>,
    /// When true, the whole site is gated behind a single "coming soon" page.
    pub construction: bool,
}

impl FromRef<AppState> for Pool {
    fn from_ref(state: &AppState) -> Pool {
        state.pool.clone()
    }
}
