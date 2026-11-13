//! What going back to the previous build (`<name>.previous`, kept by every replacement) has to
//! know first: which version that build is, and whether it can read the database as it stands. A
//! build stops on a database at a schema version above its own (`crate::store::schema`), and the
//! update that migrated it kept a copy from before (`<db>.pre-v<N>`), so a rollback across a schema
//! change is refused, naming that copy, rather than leaving a daemon that cannot start.
//!
//! Shared by `octoboardd update --rollback` (`crate::cli::update`) and the planned restart that
//! falls back to the previous build when the new one does not start (`successor`).

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::probe;
use super::record::{Files, Installed};
use super::replace::previous_path;

/// How long the previous build may take to say its version, where no record names it.
const VERSION_TIMEOUT: Duration = Duration::from_secs(10);

/// Whether the previous build can read the database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatabaseFit {
    /// It reads the database's schema version (or there is no database yet).
    Reads,
    /// It cannot be told: nothing says which schema version it reads (the replacement was not
    /// recorded by an update of this kind: an install script made it, a rollback forgot the record,
    /// a build before the version was recorded wrote it), or the database could not be read.
    Unknown,
    /// The database is at `database`, above the `reads` the previous build reads. `backups` are the
    /// copies made before the database was migrated from `reads`, newest first.
    TooNew {
        database: i64,
        reads: i64,
        backups: Vec<PathBuf>,
    },
}

/// The record of the replacement that put the file now at `real` there, when it names that file.
fn record_of(files: &Files, real: &Path) -> Option<Installed> {
    files
        .read_installed()
        .filter(|record| Path::new(&record.path) == real)
}

/// Whether the build kept beside `real` (the installed file, links resolved) can read the
/// database at `database`. Blocking.
pub fn previous_fits_database(files: &Files, real: &Path, database: &Path) -> DatabaseFit {
    if !database.exists() {
        return DatabaseFit::Reads;
    }
    let Some(at) = crate::store::Store::peek_schema_version(database) else {
        return DatabaseFit::Unknown;
    };
    let Some(reads) = record_of(files, real).and_then(|record| record.from_schema) else {
        return DatabaseFit::Unknown;
    };
    if at <= reads {
        return DatabaseFit::Reads;
    }
    DatabaseFit::TooNew {
        database: at,
        reads,
        backups: crate::store::Store::migration_backups(database, reads),
    }
}

/// The version of the build kept beside `real`: as the record of the replacement names it, else as
/// the build says when run with `--version`. `None` when neither tells. Blocking.
pub fn previous_version(files: &Files, real: &Path) -> Option<String> {
    if let Some(record) = record_of(files, real) {
        return Some(record.from_version);
    }
    let previous = previous_path(real)?;
    if !previous.is_file() {
        return None;
    }
    probe::reported_version(&previous, VERSION_TIMEOUT).ok()
}
