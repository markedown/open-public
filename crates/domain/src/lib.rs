//! Shared domain vocabulary and pure, locale-aware text helpers.
//!
//! Kept dependency-light on purpose (no database or web dependencies) so
//! every other crate in the workspace can build on it freely.

pub mod models;
pub mod slug;
