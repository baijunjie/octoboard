//! What a store method fails with: SQLite's failures to read or write the database file, which the
//! user can fix (a data folder made read-only, a full disk), become `store_write_failed`, so a
//! client can name the remedy; every other failure passes through unchanged.
//!
//! Classifying here rather than where an error reply is built keeps the code to Octoboard's own
//! database: the daemon also opens the agents' SQLite files (Codex's state database), whose
//! failures say nothing about Octoboard's data folder.

use crate::protocol::{error_code, CodedError};

/// A store method's error. Converting it into `anyhow::Error` hands back the error it holds, so a
/// [`CodedError`] in it is still found by a downcast, as `error_parts` does.
pub struct StoreError(anyhow::Error);

pub type Result<T, E = StoreError> = std::result::Result<T, E>;

impl From<rusqlite::Error> for StoreError {
    fn from(err: rusqlite::Error) -> Self {
        Self(classify(err))
    }
}

/// Passes `err` through unclassified: a rusqlite error turned into `anyhow::Error` before it
/// reaches `?` (by `.context()`, say) is never coded, so store code lets rusqlite errors reach `?`
/// as themselves.
impl From<anyhow::Error> for StoreError {
    fn from(err: anyhow::Error) -> Self {
        Self(err)
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(err: serde_json::Error) -> Self {
        Self(err.into())
    }
}

impl From<StoreError> for anyhow::Error {
    fn from(err: StoreError) -> Self {
        err.0
    }
}

impl std::fmt::Debug for StoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.0, formatter)
    }
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, formatter)
    }
}

/// The `cause` of a `store_write_failed` for SQLite's `error`, or `None` for a failure that is not
/// one of reading or writing the file.
///
/// SQLite reports these without the operating system's error: a read-only data folder is
/// `SQLITE_READONLY` (extended `SQLITE_READONLY_DIRECTORY`), a disk too full to create the
/// rollback journal is `SQLITE_CANTOPEN`, and one too full to grow the file is `SQLITE_FULL`.
/// Every `SQLITE_IOERR` (the operating system failed a read, write, sync or lock on the file) and
/// a file moved or deleted while open (`SQLITE_READONLY_DBMOVED`, which the folder's permissions
/// do not explain) are `other`.
fn write_cause(error: &rusqlite::ffi::Error) -> Option<&'static str> {
    use rusqlite::ErrorCode::*;
    if error.extended_code == rusqlite::ffi::SQLITE_READONLY_DBMOVED {
        return Some("other");
    }
    match error.code {
        ReadOnly | PermissionDenied => Some("access"),
        DiskFull => Some("disk_full"),
        CannotOpen | SystemIoFailure => Some("other"),
        _ => None,
    }
}

fn classify(err: rusqlite::Error) -> anyhow::Error {
    let Some(cause) = err.sqlite_error().and_then(write_cause) else {
        return err.into();
    };
    // The alternate form keeps SQLite's extended code ("SQLITE_READONLY_DIRECTORY"), which the
    // plain message drops.
    let detail = format!("{:#}", anyhow::Error::from(err));
    CodedError::raised(
        error_code::STORE_WRITE_FAILED,
        format!("Octoboard could not read or write its own data: {detail}"),
        &[("cause", cause), ("detail", &detail)],
    )
}

#[cfg(test)]
mod tests {
    use rusqlite::ffi;

    use super::StoreError;
    use crate::protocol::error_parts;

    fn failure(code: i32) -> anyhow::Error {
        let err = rusqlite::Error::SqliteFailure(ffi::Error::new(code), None);
        StoreError::from(err).into()
    }

    /// Each failure to write the file the user can fix carries the cause the client picks the
    /// remedy by, with SQLite's own text as the detail.
    #[test]
    fn a_failed_write_is_coded_with_its_cause() {
        for (code, cause) in [
            (ffi::SQLITE_READONLY_DIRECTORY, "access"),
            (ffi::SQLITE_READONLY, "access"),
            (ffi::SQLITE_PERM, "access"),
            (ffi::SQLITE_FULL, "disk_full"),
            (ffi::SQLITE_CANTOPEN, "other"),
            (ffi::SQLITE_IOERR, "other"),
            (ffi::SQLITE_IOERR_WRITE, "other"),
            (ffi::SQLITE_IOERR_SHORT_READ, "other"),
            (ffi::SQLITE_READONLY_DBMOVED, "other"),
        ] {
            let (wire, params, message) = error_parts(&failure(code));
            assert_eq!(wire.as_str(), "store_write_failed", "{code}");
            assert_eq!(params["cause"], cause, "{code}");
            assert!(message.starts_with("Octoboard could not read or write its own data: "));
            assert!(!params["detail"].is_empty());
        }
    }

    /// Any other failure keeps the error it was, so it is still an internal error on the wire.
    #[test]
    fn another_failure_is_left_as_it_was() {
        let (wire, ..) = error_parts(&failure(ffi::SQLITE_CONSTRAINT));
        assert_eq!(wire.as_str(), "internal_error");
        let (wire, ..) =
            error_parts(&StoreError::from(rusqlite::Error::QueryReturnedNoRows).into());
        assert_eq!(wire.as_str(), "internal_error");
    }

    /// A coded error raised inside a store method is still found once it leaves the store.
    #[test]
    fn a_coded_error_passes_through() {
        let coded = crate::protocol::CodedError::unknown_console("c1");
        let (wire, ..) = error_parts(&StoreError::from(coded).into());
        assert_eq!(wire.as_str(), "unknown_console");
    }
}
