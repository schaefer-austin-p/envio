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

impl StoreError {
    /// The disk is full: the caller stops downloads and keeps going.
    pub fn is_disk_full(&self) -> bool {
        match self {
            Self::Io(error) => error.kind() == io::ErrorKind::StorageFull,
            Self::Sqlite(error) => error.sqlite_error_code() == Some(rusqlite::ErrorCode::DiskFull),
            _ => false,
        }
    }

    /// The cache file can't be trusted and should be rebuilt.
    pub(crate) fn is_damage(&self) -> bool {
        match self {
            Self::SchemaTooNew { .. } | Self::Damaged(_) => true,
            Self::Sqlite(error) => matches!(
                error.sqlite_error_code(),
                Some(rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase)
            ),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sqlite(code: i32) -> StoreError {
        StoreError::Sqlite(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(code),
            None,
        ))
    }

    #[test]
    fn disk_full_is_recognised_from_files_and_sqlite() {
        assert!(StoreError::Io(io::Error::from(io::ErrorKind::StorageFull)).is_disk_full());
        assert!(sqlite(rusqlite::ffi::SQLITE_FULL).is_disk_full());
        assert!(!StoreError::Io(io::Error::from(io::ErrorKind::NotFound)).is_disk_full());
        assert!(!StoreError::NotFound.is_disk_full());
    }

    #[test]
    fn damage_is_corruption_or_a_newer_schema() {
        assert!(sqlite(rusqlite::ffi::SQLITE_CORRUPT).is_damage());
        assert!(sqlite(rusqlite::ffi::SQLITE_NOTADB).is_damage());
        assert!(
            StoreError::SchemaTooNew {
                found: 2,
                supported: 1
            }
            .is_damage()
        );
        assert!(StoreError::Damaged("x".into()).is_damage());
        assert!(!sqlite(rusqlite::ffi::SQLITE_FULL).is_damage());
        assert!(!sqlite(rusqlite::ffi::SQLITE_CANTOPEN).is_damage());
    }
}
