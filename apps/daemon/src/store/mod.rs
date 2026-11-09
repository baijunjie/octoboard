//! The coordinator's SQLite storage: hosts, accounts, consoles, projects, sessions, report-panel
//! pages, trusted directories, the information held for interrupted sessions, branch origins, the
//! application settings, and where the holders run the daemon's terminals.
//!
//! `Store` is one `Mutex<Connection>`; each table family has a file of its own holding its methods
//! as a further `impl Store` block, and [`schema`] holds the schema and what opening an existing
//! file does. The row readers and column lists live with the family that owns the table.
//!
//! A session's changing fields are written by methods that each write their own columns, never by
//! writing back a record read earlier: the writers run on tasks of their own, so a whole-record
//! write would undo whatever another wrote in between. A status change that must not undo another
//! matches the status it expects in the statement itself.
//!
//! Enum columns hold the same text the protocol puts on the wire — the conversion goes through
//! serde (`enum_to_text` / `enum_from_text`) rather than a second hand-written mapping, so a
//! renamed variant cannot mean one thing in the database and another on the socket.
//!
//! Values that belong to a provider are rows keyed by the provider's id (an account's and a
//! session's `agent`, the child tables of consoles and projects, `provider_settings`) or entries of
//! a JSON column keyed by it. A provider that is no longer registered may still be named in the
//! file: its rows and entries are kept, left out of what is loaded, and left in place by the writes
//! of the registered providers' values. A default agent naming one reads as the first registered
//! provider, and a project session agent naming one as following the default agent; a write of the
//! record keeps either while it still reads so.
//!
//! A list-valued field is one JSON text column rather than a join table: a project's tags are
//! always read and written whole, so `read_project` stays a single-row read.
//!
//! `Report` is not among these tables: it is an in-flight struct passed between the project
//! session's `report` tool and the console session, never stored of its own accord (see
//! `messaging/reporting.rs`).

mod accounts;
mod agents;
mod branch_origins;
mod consoles;
mod failure;
mod held_terminals;
mod hosts;
mod kept_messages;
mod pages;
mod projects;
mod provider_settings;
mod rows;
mod schema;
mod session_links;
mod sessions;
mod settings;
#[cfg(test)]
mod test_support;

use std::path::Path;
use std::sync::Mutex;

use anyhow::Context;
use rusqlite::Connection;

use schema::{settle_existing_file, user_version, APPLICATION_ID, SCHEMA, SCHEMA_VERSION};

pub use accounts::SharingStart;
pub use agents::ReorderOutcome;
pub use branch_origins::{BranchMove, BranchOrigin};
pub use failure::{Result, StoreError};
pub use held_terminals::{HeldKind, HeldTerminalRecord};
pub use sessions::{cuts_turn_short, RecordUpdate};

/// The single local host record every project and session points at. There is no other host yet,
/// but the column exists so that adding remote hosts needs no data migration ("Data model" in
/// `docs/architecture.md`).
pub const LOCAL_HOST_ID: &str = "local";

/// The settings table has exactly one row, this id, rather than a column per setting with no key —
/// `rusqlite` has no single-row table primitive, so a key lets the row be read and written with
/// the same `WHERE` every other table uses.
const SETTINGS_ID: &str = "singleton";

pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        if path.exists() {
            settle_existing_file(path)?;
        }
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        // A file that is still at version 0 here is empty (a new one, or one moved aside above),
        // so the whole current schema goes in, stamped, in one step.
        if user_version(&conn)? == 0 {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(SCHEMA)?;
            tx.pragma_update(None, "application_id", APPLICATION_ID)?;
            tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            tx.commit()?;
        }
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.ensure_local_host()?;
        store.ensure_settings_row()?;
        Ok(store)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().expect("store mutex poisoned")
    }
}
