//! A provider's own registry of conversation threads, for the provider that keeps one: a database
//! whose rows name where each thread's records are. It has to be kept in step with the records
//! when they move between directories, or the provider lists and resumes the thread by a row that
//! names a place it is no longer in. The accounts' sharing and the account switch do that work
//! through [`ThreadRegistry`]; what a registry is and how it is written is the provider's.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::adapter::provider;
use crate::protocol::{AgentId, SharingCause};

/// Why a home's row of a thread could not be pointed at its record: the kind of failure, which is
/// all the callers act on, and the provider's own account of it in English.
#[derive(Debug)]
pub(crate) struct RowError {
    pub(crate) cause: RowCause,
    pub(crate) detail: String,
}

/// The kind of failure that kept a row from being pointed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowCause {
    /// The provider's configuration that says where its registry is could not be read, for the
    /// file system's cause, or was read and could not be understood ([`SharingCause::Other`]).
    Configuration(SharingCause),
    /// Where the registry is cannot be told from the settings in effect.
    Unlocatable,
    /// The registry, or the directory it is in, could not be read or written, for this cause.
    Registry(SharingCause),
}

impl RowError {
    pub(crate) fn new(cause: RowCause, detail: impl Into<String>) -> Self {
        Self {
            cause,
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for RowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.detail)
    }
}

impl std::error::Error for RowError {}

/// A rewrite of the paths rows record: every record path that starts with `from` (a directory) is
/// made to start with `to` instead.
#[derive(Debug, Clone)]
pub(crate) struct Repoint {
    pub from: PathBuf,
    pub to: PathBuf,
}

/// The registry of a provider that keeps its threads in a database (`AccountOps::thread_registry`).
/// The errors are SQLite's, which the database's callers turn into a cause for the user
/// (`SharingCause::of_database`).
pub(crate) trait ThreadRegistry: Send + Sync {
    /// Whether `name`, an entry of a configuration directory, is one of the provider's thread
    /// databases, of any schema generation and catalogued as a shared item or not.
    fn is_database(&self, name: &str) -> bool;

    /// The subdirectories of a configuration directory that the rows name records under.
    fn record_dirs(&self) -> &'static [&'static str];

    /// How long a busy database is waited for.
    fn patience(&self) -> Duration;

    /// Points every row of `database` that names a record under one of `repoints`' `from`
    /// directories at the same record under its `to`.
    fn repoint_rows(&self, database: &Path, repoints: &[Repoint]) -> rusqlite::Result<()>;

    /// Brings the rows of `set_aside` (an account's own database, moved out of the way) into
    /// `shared`, then applies `repoints` to `shared`. Answers how many threads were added.
    fn bring_rows(
        &self,
        shared: &Path,
        set_aside: &Path,
        repoints: &[Repoint],
    ) -> rusqlite::Result<usize>;

    /// Removes the rows of `threads` from `database`: their records have left the directory it
    /// covers.
    fn forget_threads(&self, database: &Path, threads: &[&str]) -> rusqlite::Result<()>;

    /// Points the rows the databases of the home `home` keep of `thread` at `head`, the record at
    /// the head of the thread's chain in that home. `launch_env` is the environment the home's
    /// sessions launch with, which says where the home keeps its databases. Where that cannot be
    /// told, or a database cannot be written to or stays busy past `patience`, it is an error: a
    /// row left standing would resume the stale head with no sign of it.
    fn point_thread_at(
        &self,
        home: &Path,
        head: &Path,
        thread: &str,
        launch_env: &HashMap<String, String>,
        patience: Duration,
    ) -> Result<(), RowError>;

    /// After `thread`'s records were taken back from the default directory into the account's own:
    /// carries what else in the default directory names the thread.
    fn after_take_back(&self, _default_dir: &Path, _account_dir: &Path, _thread: &str) {}
}

/// The thread registry of `agent`'s provider, if it keeps one.
pub(crate) fn thread_registry(agent: AgentId) -> Option<&'static dyn ThreadRegistry> {
    provider(agent).accounts()?.thread_registry()
}
