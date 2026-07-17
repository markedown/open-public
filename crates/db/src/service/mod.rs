//! Service layer: operation-level functions that validate input, resolve
//! references, orchestrate several repository calls, and enforce business
//! rules.
//!
//! Every write path goes through here, so the web forms, a future JSON API,
//! and our own data-enrichment tools all behave identically rather than each
//! reimplementing the logic. Repositories (the other `db` modules) stay
//! low-level; services compose them.

pub mod news;
pub mod polls;
pub mod statements;

/// An error from a service operation.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Input failed a business rule (an empty required field, nothing linked,
    /// and so on). The message is safe to show to the caller.
    #[error("{0}")]
    Validation(String),
    /// A referenced entity does not exist.
    #[error("{kind} not found: {slug}")]
    NotFound { kind: &'static str, slug: String },
    /// A database error underneath.
    #[error(transparent)]
    Db(#[from] crate::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub(crate) fn validation(message: impl Into<String>) -> Self {
        Error::Validation(message.into())
    }
    pub(crate) fn not_found(kind: &'static str, slug: &str) -> Self {
        Error::NotFound {
            kind,
            slug: slug.to_string(),
        }
    }
}

/// A trimmed, non-empty view of an optional string.
pub(crate) fn trimmed(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty())
}
