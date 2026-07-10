//! CrossTrace core engine.
//!
//! Offline-first ingest → normalize → correlate → search over a user's own
//! exported data. This crate is UI-agnostic and has no GUI dependencies so it
//! can be unit-tested in isolation and embedded behind any front end (the
//! Tauri desktop shell, a CLI, or tests).
//!
//! Pipeline:
//!   `ingest` walks a dropped path (file / folder / zip), `detect`s each file,
//!   dispatches to a parser (`csv`, `json`, `vcard`, `sms_xml`) that yields
//!   [`model::NormalizedRecord`]s, and `db` persists them while `extract`
//!   pulls identifiers into de-duplicated entities. `search`, `correlate` and
//!   `stats` then query the normalized store.

pub mod correlate;
pub mod db;
pub mod extract;
pub mod ingest;
pub mod model;
pub mod search;
pub mod stats;

pub use db::Store;
pub use model::*;

/// Crate-wide error type.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error: {0}")]
    Parse(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Open a store and import a path in one call — convenience for the CLI and
/// tests.
pub fn open(path: impl AsRef<std::path::Path>) -> Result<Store> {
    Store::open(path)
}
