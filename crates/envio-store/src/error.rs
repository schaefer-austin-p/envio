//! Errors from the store.

use std::io;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("file error: {0}")]
    Io(#[from] io::Error),
    #[error("schema version {found} is newer than this build supports ({supported})")]
    SchemaTooNew { found: i64, supported: i64 },
    #[error("database is damaged: {0}")]
    Damaged(String),
    #[error("bad record in {file}: {reason}")]
    BadRecord { file: String, reason: String },
    #[error("not found")]
    NotFound,
}

pub type Result<T> = std::result::Result<T, StoreError>;
