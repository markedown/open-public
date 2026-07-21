//! Data import tasks.
//!
//! The harvesting that reaches out to the world lives outside this repository.
//! What is here is the other direction: loading the published dataset back into
//! a database, so the files under `dataset/` are enough to reconstruct the
//! platform.

pub mod import;
