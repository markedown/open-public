//! Database access layer: the connection pool and the repository functions
//! that are the only path from the rest of the workspace to Postgres.

use sqlx::postgres::{PgPool, PgPoolOptions};

pub mod alliances;
pub mod conflicts;
pub mod country;
pub mod elections;
pub mod email_verifications;
pub mod events;
pub mod export;
pub mod news;
pub mod outlets;
pub mod parties;
pub mod people;
pub mod polls;
pub mod search;
pub mod service;
pub mod sessions;
pub mod sources;
pub mod statements;
pub mod translations;
pub mod users;

/// A Postgres connection pool.
pub type Pool = PgPool;

/// Errors returned by the repository layer.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error("already exists")]
    UniqueViolation,
}

pub type Result<T> = std::result::Result<T, Error>;

/// Open a connection pool to the given database URL.
pub async fn connect(database_url: &str) -> Result<Pool> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;
    Ok(pool)
}
