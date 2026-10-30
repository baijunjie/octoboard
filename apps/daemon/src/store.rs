//! The coordinator's SQLite storage: hosts, accounts, consoles, projects, sessions, report-panel
//! pages, trusted directories, the information held for interrupted sessions, branch origins and
//! the application settings.
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
//! A list-valued field is one JSON text column rather than a join table: a project's tags are
//! always read and written whole, so `read_project` stays a single-row read.
//!
//! `Report` is not among these tables: it is an in-flight struct passed between the project
//! session's `report` tool and the console session, never stored of its own accord (see
//! `reporting.rs`).

use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::git_workflow::{GitWorkflowOverride, GitWorkflowSettings, GitWorkflowSettingsChanges};
use crate::protocol::{
    error_code, now_millis, Account, Agent, ApprovalReviewer, ClaudeApprovalMode, CodedError,
    Console, ConsoleSessionColour, Host, HostKind, Origin, Page, Project, ProjectAccount,
    ProjectSource, Role, Session, SessionStatus, Settings,
};
use crate::session_title::RecordTitles;

/// The single local host record every project and session points at. There is no other host yet,
/// but the column exists so that adding remote hosts needs no data migration ("Data model" in
/// `docs/architecture.md`).
pub const LOCAL_HOST_ID: &str = "local";

/// The settings table has exactly one row, this id, rather than a column per setting with no key —
/// `rusqlite` has no single-row table primitive, so a key lets the row be read and written with
/// the same `WHERE` every other table uses.
const SETTINGS_ID: &str = "singleton";

/// Where a clone goes while the user has not chosen a directory.
const BUILT_IN_CLONE_DIR: &str = "~/Projects";

/// What `Store::reorder_added_agents` did.
#[derive(Debug, PartialEq, Eq)]
pub enum ReorderOutcome {
    /// The list has not been decided, so there is nothing to put in order.
    Undecided,
    /// The order is not a permutation of the list: an agent is missing, repeated or not added.
    NotAPermutation,
    /// The list already had this order.
    Unchanged,
    Reordered,
}

/// The schema version a database is created at, recorded in SQLite's `user_version`. A database
/// at any other version is not opened as it stands; see [`settle_existing_file`].
///
/// Every change to [`SCHEMA`] raises this by one: `SCHEMA` only ever runs on an empty file, so
/// an existing database at an unchanged version keeps its old shape and fails at query time —
/// while every test, starting from a fresh file, still passes. Once a build at the old version
/// has been released, the change also adds the step from that version to `settle_existing_file`,
/// keyed on `user_version` rather than on probing for columns, with a test that opens a database
/// at the old version; without the step, a released user's database is moved aside.
const SCHEMA_VERSION: i64 = 4;

/// Every table, index and trigger of the current schema.
const SCHEMA: &str = r#"
CREATE TABLE hosts (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    kind        TEXT NOT NULL,
    ssh_config  TEXT
);
CREATE TABLE accounts (
    id         TEXT PRIMARY KEY,
    agent      TEXT NOT NULL,
    name       TEXT NOT NULL,
    config_dir TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    -- Whether the account reads the default account's configuration, and where that
    -- stands: a change that did not finish reads as incomplete, for the user to retry.
    share_default_config           INTEGER NOT NULL DEFAULT 0,
    share_default_config_incomplete INTEGER NOT NULL DEFAULT 0,
    share_default_config_completed INTEGER NOT NULL DEFAULT 0,
    share_default_config_backup    TEXT,
    share_default_config_dir       TEXT,
    -- 1 while the account is switched off.
    disabled                       INTEGER NOT NULL DEFAULT 0
);
-- Backstop for the rule that an account's name is required and unique within its
-- agent: `coordinator` checks this before every write that can affect a name, but
-- the invariant belongs on the table it protects, not only on the callers that happen
-- to remember to check it. `LOWER` matches `coordinator::account_name_key`'s
-- case-insensitive comparison; it is ASCII-only in SQLite, which is a narrower fold
-- than `str::to_lowercase`, so this index is a backstop and not a full substitute for
-- that check. A write that trips it surfaces through `Store::account_write_error` as
-- the protocol's own `account_name_taken` rather than a raw SQLite error.
CREATE UNIQUE INDEX accounts_agent_name_unique
    ON accounts (agent, LOWER(name));
CREATE TABLE consoles (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    workdir       TEXT NOT NULL,
    default_agent TEXT NOT NULL,
    -- The agent a project session here falls back to when its project names none;
    -- NULL follows `default_agent`.
    project_session_agent TEXT,
    -- The account each agent's sessions here read, by id; NULL means that agent's
    -- default account. No `REFERENCES`: `Store::delete_account` clears these itself,
    -- in the same locked step as the row's removal, rather than relying on a cascade.
    claude_account_id TEXT,
    codex_account_id  TEXT,
    grok_account_id   TEXT,
    -- The model and reasoning effort a console session here is launched with, per
    -- agent because each has its own model names; NULL means the agent's own default.
    -- Read at launch, so nothing is copied onto the session.
    claude_console_session_model  TEXT,
    claude_console_session_effort TEXT,
    codex_console_session_model   TEXT,
    codex_console_session_effort  TEXT,
    grok_console_session_model    TEXT,
    grok_console_session_effort   TEXT,
    -- The model and reasoning effort a project session here is launched with when
    -- neither it nor its project names one; NULL means the agent's own default.
    claude_project_session_model TEXT,
    claude_project_session_effort TEXT,
    codex_project_session_model TEXT,
    codex_project_session_effort TEXT,
    grok_project_session_model TEXT,
    grok_project_session_effort TEXT,
    icon          TEXT,
    -- The user's own addition to the console session's instruction file; NULL means
    -- none.
    console_session_prompt TEXT,
    -- The ordinal a console session created in this console is given next, one past
    -- the highest ever used here. Kept on the console rather than derived from the
    -- sessions in it, because a console session that is later deleted must not free its
    -- number for reuse (see `Store::insert_console_session`).
    next_console_session_ordinal INTEGER NOT NULL DEFAULT 1,
    created_at    INTEGER NOT NULL
);
CREATE TABLE projects (
    id            TEXT PRIMARY KEY,
    console_id    TEXT NOT NULL REFERENCES consoles(id) ON DELETE CASCADE,
    host_id       TEXT NOT NULL,
    name          TEXT NOT NULL,
    path          TEXT NOT NULL,
    default_agent TEXT,
    source        TEXT NOT NULL,
    remote_url    TEXT,
    -- The account each agent's sessions here read, by id; NULL with the flag clear
    -- inherits the console's reference for that agent. The flag, set only where the id
    -- is NULL, says the project pins the agent's default account instead, even where
    -- the console names an account: a separate column so that no stored value in the
    -- id column can be mistaken for an account id. The id is cleared by
    -- `Store::delete_account`, like the console's; the flag names no account.
    claude_account_id TEXT,
    codex_account_id  TEXT,
    grok_account_id   TEXT,
    claude_account_default INTEGER NOT NULL DEFAULT 0,
    codex_account_default  INTEGER NOT NULL DEFAULT 0,
    grok_account_default   INTEGER NOT NULL DEFAULT 0,
    -- The model and reasoning effort a session here is launched with, per agent;
    -- NULL means the agent's own default. Read at launch, so nothing is copied onto
    -- the session.
    claude_session_model  TEXT,
    claude_session_effort TEXT,
    codex_session_model   TEXT,
    codex_session_effort  TEXT,
    grok_session_model    TEXT,
    grok_session_effort   TEXT,
    trust_consent INTEGER NOT NULL DEFAULT 0,
    pinned        INTEGER NOT NULL DEFAULT 0,
    tags          TEXT NOT NULL DEFAULT '[]',
    description   TEXT,
    -- NULL follows the application-wide value; see `Store::git_workflow_settings`.
    development         TEXT,
    squash_before_delivery INTEGER,
    rebase_before_merge INTEGER,
    delivery            TEXT,
    revert_gate         INTEGER
);
CREATE TABLE sessions (
    id               TEXT PRIMARY KEY,
    agent            TEXT NOT NULL,
    agent_session_id TEXT,
    console_id       TEXT NOT NULL REFERENCES consoles(id) ON DELETE CASCADE,
    project_id       TEXT REFERENCES projects(id) ON DELETE CASCADE,
    host_id          TEXT NOT NULL,
    role             TEXT NOT NULL,
    origin           TEXT NOT NULL,
    title            TEXT NOT NULL,
    terminal_title   TEXT,
    -- What the agent's own records call the session: a title it was named with, and a
    -- stand-in from its first prompt.
    record_title     TEXT,
    record_prompt    TEXT,
    context          TEXT,
    -- The model and reasoning effort the agent's records show the session running with.
    observed_model   TEXT,
    observed_effort  TEXT,
    -- The old-record model boundary of the last restart, private to the collector.
    observed_boundary TEXT,
    status           TEXT NOT NULL,
    has_conversation INTEGER NOT NULL DEFAULT 0,
    -- The console session this (project) session reports to, or NULL outside the
    -- orchestration. Always NULL for a console session itself. No `REFERENCES`, because
    -- what follows a console session is decided by the coordinator, not by the
    -- database: archiving one archives its interrupted bound sessions, and deleting an
    -- archived one deletes its archived bound sessions. An `ON DELETE CASCADE` would be
    -- wrong twice over: it would take a bound session that is not archived, and it
    -- would remove rows behind the daemon's back, so no `session_deleted` would be
    -- broadcast and every client would keep a ghost row.
    bound_to         TEXT,
    -- 1 for a lead session: a project session bound to a console session that can
    -- still own sessions of its own (see `Session::lead`).
    lead             INTEGER NOT NULL DEFAULT 0,
    -- Above 0 while the session has a change of its owner it was not running to be
    -- told of (one count per change): its next launch tells it, from the binding as
    -- it then stands, and clears what it saw.
    owner_notice_pending INTEGER NOT NULL DEFAULT 0,
    -- The parting note of the owner that released the session while it was not running,
    -- told with the notice while the session is still unbound; NULL for none. Written
    -- with the release, or with the mark a failed write to the running session left,
    -- and cleared by the next change of owner and by the delivery.
    owner_notice_note TEXT,
    -- Set only for a console session (`role = 'console'`); NULL for a project session.
    colour           TEXT,
    ordinal          INTEGER,
    -- The account this session's own agent reads, by id; NULL means the default
    -- account. Written with `config_dir` below, which is that account's directory at
    -- the time, when the session is opened and when its account is switched — see
    -- `Session::account_id`.
    account_id       TEXT,
    config_dir       TEXT,
    -- A clear override flag inherits the project's or console's current setting.
    -- A set flag with a NULL value explicitly leaves the choice to the agent.
    model            TEXT,
    effort           TEXT,
    model_override   INTEGER NOT NULL DEFAULT 0,
    effort_override  INTEGER NOT NULL DEFAULT 0,
    pinned           INTEGER NOT NULL DEFAULT 0,
    locked           INTEGER NOT NULL DEFAULT 0,
    started_at       INTEGER NOT NULL,
    ended_at         INTEGER
);
CREATE TRIGGER protect_locked_session_delete
BEFORE DELETE ON sessions WHEN OLD.locked = 1
BEGIN SELECT RAISE(ABORT, 'locked sessions cannot be deleted'); END;
CREATE TRIGGER protect_locked_session_archive
BEFORE UPDATE OF status ON sessions
WHEN OLD.locked = 1 AND NEW.status = 'archived' AND OLD.status != 'archived'
BEGIN SELECT RAISE(ABORT, 'locked sessions cannot be archived'); END;
CREATE TABLE trusted_directories (
    path TEXT PRIMARY KEY
);
CREATE TABLE pages (
    id                TEXT PRIMARY KEY,
    -- The session that pushed the page, a console session or a project session that
    -- faces the user; deleting it takes its pages, and deleting a console or a project
    -- therefore takes all of theirs.
    console_session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    html              TEXT NOT NULL,
    anchor_message_id TEXT,
    created_at        INTEGER NOT NULL
);
-- Messages for a session that was interrupted (shared information, and the reports and
-- notices for an owner), held to be written into it once it is resumed
-- (`Store::add_kept_message`), oldest first by `id`. Deleting the session takes its rows.
-- The table keeps the name it was made with.
CREATE TABLE pending_info (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    text       TEXT NOT NULL
);
-- The session working on a branch, keyed on the branch because one session works on
-- many and a branch has one responsible session. It starts as the session whose own git
-- reported creating the branch and moves only by a handover. No `REFERENCES` on
-- `responsible_session_id`: a project's deletion cascades its sessions away and the
-- record outlives them, which is what `responsible_title`, the title the session had
-- when it became responsible, is for. `responsible_since` is when that was, in
-- milliseconds. `repository` is the repository's canonical common directory, so a row
-- belongs to the repository and not to any project.
-- `birth_commit` and `birth_time` are the commit and the second the branch was created
-- at, which tell a branch deleted and made again under the same name from the one the
-- row was made for; `created_at` is that time in milliseconds, or when the creation was
-- reported.
-- `target_branch` is the branch the branch was created from, which the revert gate
-- holds its commits against, together with `birth_commit` as the fork point; NULL for
-- one created from something that is no branch.
CREATE TABLE branch_origins (
    repository     TEXT NOT NULL,
    branch         TEXT NOT NULL,
    created_at     INTEGER NOT NULL,
    birth_commit   TEXT,
    birth_time     INTEGER,
    responsible_session_id TEXT,
    responsible_since      INTEGER,
    responsible_title      TEXT,
    target_branch          TEXT,
    PRIMARY KEY (repository, branch)
);
CREATE TABLE settings (
    id                     TEXT PRIMARY KEY,
    auto_pull_repositories INTEGER NOT NULL DEFAULT 0,
    auto_delete_unresumable_archives INTEGER NOT NULL DEFAULT 1,
    auto_approve_session_requests INTEGER NOT NULL DEFAULT 0,
    auto_approve_resume_requests INTEGER NOT NULL DEFAULT 0,
    auto_approve_account_switches INTEGER NOT NULL DEFAULT 0,
    auto_approve_handoffs INTEGER NOT NULL DEFAULT 0,
    ask_questions_as_choices INTEGER NOT NULL DEFAULT 1,
    auto_select_account INTEGER NOT NULL DEFAULT 0,
    auto_switch_on_usage_limit INTEGER NOT NULL DEFAULT 0,
    sidebar_order_by_status INTEGER NOT NULL DEFAULT 1,
    auto_start_quota_windows INTEGER NOT NULL DEFAULT 0,
    -- NULL until the user sets one; see `Store::default_clone_dir`.
    default_clone_dir      TEXT,
    -- NULL until a client reports the language the user picked; see
    -- `Store::interface_language`.
    interface_language     TEXT,
    -- NULL until the list is first decided; see `Store::added_agents`.
    added_agents           TEXT,
    account_order          TEXT,
    -- NULL until the user names a default account; see `Store::default_account_names`.
    default_account_names  TEXT,
    -- NULL until a default account is turned off; see `Store::disabled_default_accounts`.
    disabled_default_accounts TEXT,
    -- One choice per agent; NULL until the user makes one.
    codex_approval_reviewer TEXT,
    claude_approval_mode   TEXT,
    -- NULL until the user makes a choice for an agent's Claude compatibility; see
    -- `Store::claude_compatibility_choices`.
    claude_compatibility   TEXT,
    development         TEXT NOT NULL DEFAULT 'worktree',
    squash_before_delivery INTEGER NOT NULL DEFAULT 0,
    rebase_before_merge INTEGER NOT NULL DEFAULT 1,
    delivery            TEXT NOT NULL DEFAULT 'ask',
    revert_gate         INTEGER NOT NULL DEFAULT 0
);
"#;

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

    /// Makes every later read or write of the branch records fail.
    #[cfg(test)]
    pub fn drop_branch_origins(&self) -> Result<()> {
        self.lock().execute_batch("DROP TABLE branch_origins")?;
        Ok(())
    }

    fn ensure_local_host(&self) -> Result<()> {
        self.lock().execute(
            "INSERT OR IGNORE INTO hosts (id, name, kind, ssh_config) VALUES (?1, ?2, ?3, NULL)",
            params![
                LOCAL_HOST_ID,
                "This machine",
                enum_to_text(&HostKind::Local)
            ],
        )?;
        Ok(())
    }

    /// The settings row, at its defaults, for a freshly created database.
    fn ensure_settings_row(&self) -> Result<()> {
        self.lock().execute(
            "INSERT OR IGNORE INTO settings (id, auto_pull_repositories) VALUES (?1, 0)",
            params![SETTINGS_ID],
        )?;
        Ok(())
    }

    // -- hosts ---------------------------------------------------------------

    pub fn list_hosts(&self) -> Result<Vec<Host>> {
        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT id, name, kind, ssh_config FROM hosts ORDER BY id")?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Host {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    kind: enum_from_row(row, 2)?,
                    ssh_config: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    // -- consoles ------------------------------------------------------------

    pub fn insert_console(&self, console: &Console) -> Result<()> {
        self.lock().execute(
            "INSERT INTO consoles (id, name, workdir, default_agent, claude_account_id,
                                   codex_account_id, grok_account_id, claude_console_session_model,
                                   claude_console_session_effort, codex_console_session_model,
                                   codex_console_session_effort, grok_console_session_model,
                                   grok_console_session_effort, icon, console_session_prompt,
                                   created_at, project_session_agent,
                                   claude_project_session_model, claude_project_session_effort,
                                   codex_project_session_model, codex_project_session_effort,
                                   grok_project_session_model, grok_project_session_effort)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                     ?18, ?19, ?20, ?21, ?22, ?23)",
            params![
                console.id,
                console.name,
                console.workdir,
                enum_to_text(&console.default_agent),
                console.claude_account_id,
                console.codex_account_id,
                console.grok_account_id,
                console.claude_console_session_model,
                console.claude_console_session_effort,
                console.codex_console_session_model,
                console.codex_console_session_effort,
                console.grok_console_session_model,
                console.grok_console_session_effort,
                console.icon,
                console.console_session_prompt,
                console.created_at,
                console.project_session_agent.as_ref().map(enum_to_text),
                console.claude_project_session_model,
                console.claude_project_session_effort,
                console.codex_project_session_model,
                console.codex_project_session_effort,
                console.grok_project_session_model,
                console.grok_project_session_effort,
            ],
        )?;
        Ok(())
    }

    pub fn update_console(&self, console: &Console) -> Result<()> {
        self.lock().execute(
            "UPDATE consoles SET name = ?2, default_agent = ?3,
                                 claude_account_id = ?4, codex_account_id = ?5,
                                 grok_account_id = ?6, claude_console_session_model = ?7,
                                 claude_console_session_effort = ?8,
                                 codex_console_session_model = ?9,
                                 codex_console_session_effort = ?10,
                                 grok_console_session_model = ?11,
                                 grok_console_session_effort = ?12, icon = ?13,
                                 console_session_prompt = ?14, project_session_agent = ?15,
                                 claude_project_session_model = ?16,
                                 claude_project_session_effort = ?17,
                                 codex_project_session_model = ?18,
                                 codex_project_session_effort = ?19,
                                 grok_project_session_model = ?20,
                                 grok_project_session_effort = ?21
             WHERE id = ?1",
            params![
                console.id,
                console.name,
                enum_to_text(&console.default_agent),
                console.claude_account_id,
                console.codex_account_id,
                console.grok_account_id,
                console.claude_console_session_model,
                console.claude_console_session_effort,
                console.codex_console_session_model,
                console.codex_console_session_effort,
                console.grok_console_session_model,
                console.grok_console_session_effort,
                console.icon,
                console.console_session_prompt,
                console.project_session_agent.as_ref().map(enum_to_text),
                console.claude_project_session_model,
                console.claude_project_session_effort,
                console.codex_project_session_model,
                console.codex_project_session_effort,
                console.grok_project_session_model,
                console.grok_project_session_effort,
            ],
        )?;
        Ok(())
    }

    pub fn delete_console(&self, id: &str) -> Result<()> {
        self.lock()
            .execute("DELETE FROM consoles WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn get_console(&self, id: &str) -> Result<Option<Console>> {
        let console = self
            .lock()
            .query_row(
                &format!("SELECT {CONSOLE_COLUMNS} FROM consoles WHERE id = ?1"),
                params![id],
                read_console,
            )
            .optional()?;
        Ok(console)
    }

    pub fn list_consoles(&self) -> Result<Vec<Console>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {CONSOLE_COLUMNS} FROM consoles ORDER BY created_at"
        ))?;
        let consoles = stmt
            .query_map([], read_console)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(consoles)
    }

    // -- accounts --------------------------------------------------------------

    pub fn insert_account(&self, account: &Account) -> Result<()> {
        self.lock()
            .execute(
                "INSERT INTO accounts (id, agent, name, config_dir, created_at, share_default_config, share_default_config_incomplete, disabled) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    account.id,
                    enum_to_text(&account.agent),
                    account.name,
                    account.config_dir,
                    now_millis(),
                    account.share_default_config,
                    account.share_default_config_incomplete,
                    account.disabled,
                ],
            )
            .map_err(|err| account_write_error(err, account))?;
        Ok(())
    }

    /// Renames the account, repoints it, or both — whatever the caller already decided to write;
    /// this does not itself check the name for a collision, which is `coordinator`'s business. The
    /// table's own `accounts_agent_name_unique` index is the backstop for that check, not a
    /// replacement for it.
    ///
    /// The coordinator turns an account's sharing off before repointing it, so a new directory
    /// starts out not sharing: the choice is recorded off and complete, and what turning sharing
    /// on recorded of the old directory is forgotten. A directory spelled differently that
    /// compares as the same one (`account_dir_key`, folding case on macOS) is no repoint: what
    /// sharing recorded stays, so its backups are still restored.
    pub fn update_account(&self, account: &Account) -> Result<()> {
        let conn = self.lock();
        let current: Option<String> = conn
            .query_row(
                "SELECT config_dir FROM accounts WHERE id = ?1",
                params![account.id],
                |row| row.get(0),
            )
            .optional()?;
        let same_dir = current.is_some_and(|current| {
            crate::account_dirs::account_dir_key(&current)
                == crate::account_dirs::account_dir_key(&account.config_dir)
        });
        conn.execute(
            "UPDATE accounts SET name = ?2, config_dir = ?3,
                 share_default_config = CASE WHEN ?4 THEN share_default_config ELSE 0 END,
                 share_default_config_incomplete = CASE WHEN ?4
                     THEN share_default_config_incomplete ELSE 0 END,
                 share_default_config_backup = CASE WHEN ?4
                     THEN share_default_config_backup ELSE NULL END,
                 share_default_config_dir = CASE WHEN ?4
                     THEN share_default_config_dir ELSE NULL END,
                 share_default_config_completed = CASE WHEN ?4
                     THEN share_default_config_completed ELSE 0 END
             WHERE id = ?1",
            params![account.id, account.name, account.config_dir, same_dir],
        )
        .map_err(|err| account_write_error(err, account))?;
        Ok(())
    }

    /// Turns the account off for new work, or back on. Whether the stored flag changed.
    pub fn set_account_disabled(&self, id: &str, disabled: bool) -> Result<bool> {
        let changed = self.lock().execute(
            "UPDATE accounts SET disabled = ?2 WHERE id = ?1 AND disabled != ?2",
            params![id, disabled],
        )?;
        Ok(changed > 0)
    }

    /// Records the account's sharing choice and whether the last change toward it is incomplete.
    pub fn set_account_sharing(&self, id: &str, share: bool, incomplete: bool) -> Result<()> {
        self.lock().execute(
            "UPDATE accounts SET share_default_config = ?2, share_default_config_incomplete = ?3
             WHERE id = ?1",
            params![id, share, incomplete],
        )?;
        Ok(())
    }

    /// What turning sharing on recorded when it started on the account's directory: the stamp
    /// its backups are named with (`<item>.bak-<stamp>`), the default directory, resolved, its
    /// links lead into, and whether it has completed since. `None` when sharing was never started
    /// there, or was undone since.
    pub fn account_sharing_start(&self, id: &str) -> Result<Option<SharingStart>> {
        let start = self
            .lock()
            .query_row(
                "SELECT share_default_config_backup, share_default_config_dir,
                     share_default_config_completed
                 FROM accounts WHERE id = ?1",
                params![id],
                |row| {
                    let (stamp, default_dir) = (
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                    );
                    Ok(stamp
                        .zip(default_dir)
                        .map(|(stamp, default_dir)| SharingStart {
                            stamp,
                            default_dir,
                            completed: row.get(2).unwrap_or(false),
                        }))
                },
            )
            .optional()?;
        Ok(start.flatten())
    }

    /// Records that turning sharing on starts on `config_dir`, unless the account has been
    /// repointed meanwhile; answers whether it was recorded.
    pub fn start_account_sharing(
        &self,
        id: &str,
        config_dir: &str,
        start: &SharingStart,
    ) -> Result<bool> {
        let changed = self.lock().execute(
            "UPDATE accounts SET share_default_config_backup = ?3, share_default_config_dir = ?4,
                 share_default_config_completed = 0
             WHERE id = ?1 AND config_dir = ?2",
            params![id, config_dir, start.stamp, start.default_dir],
        )?;
        Ok(changed > 0)
    }

    /// Records the account's directory `config_dir` brought in line with the choice `share`: the
    /// start is marked completed, or forgotten once sharing is undone, and the account is no
    /// longer incomplete. Nothing is written, and `false` answered, when the account has been
    /// repointed or its choice changed meanwhile.
    pub fn finish_account_sharing(&self, id: &str, config_dir: &str, share: bool) -> Result<bool> {
        let changed = self.lock().execute(
            "UPDATE accounts SET share_default_config_incomplete = 0,
                 share_default_config_backup = CASE WHEN ?3 THEN share_default_config_backup END,
                 share_default_config_dir = CASE WHEN ?3 THEN share_default_config_dir END,
                 share_default_config_completed = ?3
             WHERE id = ?1 AND config_dir = ?2 AND share_default_config = ?3",
            params![id, config_dir, share],
        )?;
        Ok(changed > 0)
    }

    pub fn get_account(&self, id: &str) -> Result<Option<Account>> {
        let account = self
            .lock()
            .query_row(
                "SELECT id, agent, name, config_dir, share_default_config, share_default_config_incomplete, disabled FROM accounts WHERE id = ?1",
                params![id],
                read_account,
            )
            .optional()?;
        Ok(account)
    }

    /// The directory of the account `account_id` names, `None` for the default account, which pins
    /// nothing, and for an id that is no longer on record.
    pub fn account_dir(&self, account_id: &Option<String>) -> Result<Option<String>> {
        Ok(match account_id {
            None => None,
            Some(id) => self.get_account(id)?.map(|account| account.config_dir),
        })
    }

    /// Every account of every agent, oldest first.
    pub fn list_accounts(&self) -> Result<Vec<Account>> {
        let conn = self.lock();
        let mut stmt =
            conn.prepare("SELECT id, agent, name, config_dir, share_default_config, share_default_config_incomplete, disabled FROM accounts ORDER BY created_at")?;
        let rows = stmt
            .query_map([], read_account)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Removes the account, after clearing it from every console and project that refers to it —
    /// which puts each console back on its agent's default account and each project back to
    /// inheriting its console's — in the same locked step, so no reader ever sees one still
    /// pointing at an id the `accounts` table no longer has. A project that pins the default
    /// account refers to no id, so it is left alone.
    pub fn delete_account(&self, id: &str) -> Result<()> {
        let conn = self.lock();
        for column in ["claude_account_id", "codex_account_id", "grok_account_id"] {
            for table in ["consoles", "projects"] {
                conn.execute(
                    &format!("UPDATE {table} SET {column} = NULL WHERE {column} = ?1"),
                    params![id],
                )?;
            }
        }
        conn.execute("DELETE FROM accounts WHERE id = ?1", params![id])?;
        Ok(())
    }

    // -- projects ------------------------------------------------------------

    pub fn insert_project(&self, project: &Project) -> Result<()> {
        self.lock().execute(
            "INSERT INTO projects (id, console_id, host_id, name, path, default_agent, source,
                                   remote_url, claude_account_id, codex_account_id,
                                   grok_account_id, claude_account_default,
                                   codex_account_default, grok_account_default,
                                   claude_session_model, claude_session_effort,
                                   codex_session_model, codex_session_effort, grok_session_model,
                                   grok_session_effort, trust_consent, pinned, tags,
                                   description, development, rebase_before_merge, delivery,
                                   revert_gate, squash_before_delivery)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                     ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29)",
            params![
                project.id,
                project.console_id,
                project.host_id,
                project.name,
                project.path,
                project.default_agent.as_ref().map(enum_to_text),
                enum_to_text(&project.source),
                project.remote_url,
                project.claude_account_id.account_id(),
                project.codex_account_id.account_id(),
                project.grok_account_id.account_id(),
                project.claude_account_id == ProjectAccount::Default,
                project.codex_account_id == ProjectAccount::Default,
                project.grok_account_id == ProjectAccount::Default,
                project.claude_session_model,
                project.claude_session_effort,
                project.codex_session_model,
                project.codex_session_effort,
                project.grok_session_model,
                project.grok_session_effort,
                project.trust_consent,
                project.pinned,
                tags_to_text(&project.tags),
                project.description,
                project.git_workflow.development.as_ref().map(enum_to_text),
                project.git_workflow.rebase_before_merge,
                project.git_workflow.delivery.as_ref().map(enum_to_text),
                project.git_workflow.revert_gate,
                project.git_workflow.squash_before_delivery,
            ],
        )?;
        Ok(())
    }

    /// Records the user's permission (or its withdrawal) for Octoboard to press any agent's trust
    /// confirmation for this project without asking. Its own statement rather than a field of
    /// `update_project`, which is the user's edit of the project and never reaches this decision.
    pub fn set_project_trust_consent(&self, id: &str, consent: bool) -> Result<()> {
        self.lock().execute(
            "UPDATE projects SET trust_consent = ?2 WHERE id = ?1",
            params![id, consent],
        )?;
        Ok(())
    }

    /// Records the git remote URL read from the project's repository, or clears it. Its own
    /// statement rather than a field of `update_project`, which is the user's edit of the project:
    /// the URL is read off the repository by the daemon, never entered by the user, and an edit
    /// must not overwrite a value the git status sweep has just corrected.
    pub fn set_project_remote_url(&self, id: &str, remote_url: Option<&str>) -> Result<()> {
        self.lock().execute(
            "UPDATE projects SET remote_url = ?2 WHERE id = ?1",
            params![id, remote_url],
        )?;
        Ok(())
    }

    pub fn update_project(&self, project: &Project) -> Result<()> {
        self.lock().execute(
            "UPDATE projects SET name = ?2, default_agent = ?3, claude_account_id = ?4,
                                 codex_account_id = ?5, grok_account_id = ?6,
                                 claude_account_default = ?7, codex_account_default = ?8,
                                 grok_account_default = ?9,
                                 claude_session_model = ?10, claude_session_effort = ?11,
                                 codex_session_model = ?12, codex_session_effort = ?13,
                                 grok_session_model = ?14, grok_session_effort = ?15,
                                 pinned = ?16, tags = ?17, description = ?18,
                                 development = ?19, rebase_before_merge = ?20, delivery = ?21,
                                 revert_gate = ?22, squash_before_delivery = ?23
             WHERE id = ?1",
            params![
                project.id,
                project.name,
                project.default_agent.as_ref().map(enum_to_text),
                project.claude_account_id.account_id(),
                project.codex_account_id.account_id(),
                project.grok_account_id.account_id(),
                project.claude_account_id == ProjectAccount::Default,
                project.codex_account_id == ProjectAccount::Default,
                project.grok_account_id == ProjectAccount::Default,
                project.claude_session_model,
                project.claude_session_effort,
                project.codex_session_model,
                project.codex_session_effort,
                project.grok_session_model,
                project.grok_session_effort,
                project.pinned,
                tags_to_text(&project.tags),
                project.description,
                project.git_workflow.development.as_ref().map(enum_to_text),
                project.git_workflow.rebase_before_merge,
                project.git_workflow.delivery.as_ref().map(enum_to_text),
                project.git_workflow.revert_gate,
                project.git_workflow.squash_before_delivery,
            ],
        )?;
        Ok(())
    }

    pub fn delete_project(&self, id: &str) -> Result<()> {
        self.lock()
            .execute("DELETE FROM projects WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn get_project(&self, id: &str) -> Result<Option<Project>> {
        let conn = self.lock();
        let project = conn
            .query_row(
                &format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE id = ?1"),
                params![id],
                read_project,
            )
            .optional()?;
        Ok(project)
    }

    /// The paths of this console's projects, as stored: absolute and lexically normalised.
    pub fn project_paths(&self, console_id: &str) -> Result<Vec<String>> {
        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT path FROM projects WHERE console_id = ?1")?;
        let paths = stmt
            .query_map(params![console_id], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(paths)
    }

    pub fn list_projects(&self) -> Result<Vec<Project>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {PROJECT_COLUMNS} FROM projects ORDER BY name"
        ))?;
        let rows = stmt
            .query_map([], read_project)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    // -- trusted directories -------------------------------------------------

    /// The directories under which the user has agreed Octoboard may press any agent's trust
    /// confirmation, as stored: absolute and lexically normalised by whoever wrote them.
    pub fn trusted_directories(&self) -> Result<Vec<String>> {
        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT path FROM trusted_directories ORDER BY path")?;
        let rows = stmt
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(rows)
    }

    /// Whether the directory was not already trusted.
    pub fn add_trusted_directory(&self, path: &str) -> Result<bool> {
        let added = self.lock().execute(
            "INSERT OR IGNORE INTO trusted_directories (path) VALUES (?1)",
            params![path],
        )?;
        Ok(added > 0)
    }

    /// Whether the directory was trusted.
    pub fn remove_trusted_directory(&self, path: &str) -> Result<bool> {
        let removed = self.lock().execute(
            "DELETE FROM trusted_directories WHERE path = ?1",
            params![path],
        )?;
        Ok(removed > 0)
    }

    // -- settings --------------------------------------------------------------

    pub fn get_settings(&self) -> Result<Settings> {
        let account_order = read_account_order(&self.lock())?;
        Ok(Settings {
            auto_pull_repositories: self.auto_pull_repositories()?,
            auto_delete_unresumable_archives: self.auto_delete_unresumable_archives()?,
            auto_approve_session_requests: self.auto_approve_session_requests()?,
            auto_approve_resume_requests: self.auto_approve_resume_requests()?,
            auto_approve_account_switches: self.auto_approve_account_switches()?,
            auto_approve_handoffs: self.auto_approve_handoffs()?,
            ask_questions_as_choices: self.ask_questions_as_choices()?,
            interface_language: self.interface_language()?,
            auto_select_account: self.auto_select_account()?,
            auto_switch_on_usage_limit: self.auto_switch_on_usage_limit()?,
            sidebar_order_by_status: self.sidebar_order_by_status()?,
            auto_start_quota_windows: self.auto_start_quota_windows()?,
            default_clone_dir: self.default_clone_dir()?,
            accounts: self.list_accounts()?,
            account_order,
            added_agents: self.added_agents()?,
            codex_approval_reviewer: self.codex_approval_reviewer()?,
            claude_approval_mode: self.claude_approval_mode()?,
            claude_compatibility: self.claude_compatibility()?,
            default_account_names: self.default_account_names()?,
            disabled_default_accounts: self.disabled_default_accounts()?,
            git_workflow: self.git_workflow_settings()?,
        })
    }

    /// The names the user gave the agents' default accounts, by the agent's wire name. A value that
    /// is not a JSON object of strings reads as no names.
    pub fn default_account_names(&self) -> Result<std::collections::BTreeMap<String, String>> {
        read_default_account_names(&self.lock())
    }

    /// The choices the user made for the Claude compatibility setting, by the agent's wire name.
    /// An agent they never chose for has no entry. A value that is not a JSON object of booleans
    /// reads as no choices.
    pub fn claude_compatibility_choices(&self) -> Result<std::collections::BTreeMap<String, bool>> {
        read_claude_compatibility_choices(&self.lock())
    }

    /// Whether `agent`'s sessions are to load Claude Code's instructions, skills and the rest: the
    /// user's choice when they made one. Until then Grok Build follows its own default, which is
    /// on, and Codex is on while Claude Code is among the added agents, so adding or removing it
    /// moves the value. Claude Code itself has no such setting and reads as off.
    pub fn claude_compatibility_of(&self, agent: Agent) -> Result<bool> {
        let conn = self.lock();
        let choices = read_claude_compatibility_choices(&conn)?;
        Ok(effective_claude_compatibility(
            agent,
            &choices,
            read_added_agents(&conn)?.as_deref(),
        ))
    }

    /// The effective setting for every agent that has one, by wire name.
    pub fn claude_compatibility(&self) -> Result<std::collections::BTreeMap<String, bool>> {
        let conn = self.lock();
        let choices = read_claude_compatibility_choices(&conn)?;
        let added = read_added_agents(&conn)?;
        Ok(Agent::ALL
            .into_iter()
            .filter(|agent| agent.has_claude_compatibility())
            .map(|agent| {
                (
                    agent.wire_name().to_string(),
                    effective_claude_compatibility(agent, &choices, added.as_deref()),
                )
            })
            .collect())
    }

    /// Records the user's choice of `enabled` for `agent`'s Claude compatibility. The read and the
    /// write are one step under the store's lock. Whether the stored choice changed.
    pub fn set_claude_compatibility(&self, agent: Agent, enabled: bool) -> Result<bool> {
        let conn = self.lock();
        let mut choices = read_claude_compatibility_choices(&conn)?;
        if choices.insert(agent.wire_name().to_string(), enabled) == Some(enabled) {
            return Ok(false);
        }
        conn.execute(
            "UPDATE settings SET claude_compatibility = ?2 WHERE id = ?1",
            params![
                SETTINGS_ID,
                serde_json::to_string(&choices).expect("a map of booleans serializes to JSON")
            ],
        )?;
        Ok(true)
    }

    /// The agents, by wire name, whose default account is turned off for new work. A value that is
    /// not a JSON array of strings reads as none.
    pub fn disabled_default_accounts(&self) -> Result<std::collections::BTreeSet<String>> {
        read_disabled_default_accounts(&self.lock())
    }

    /// Turns `agent`'s default account off for new work, or back on. The read and the write are
    /// one step under the store's lock. Whether the stored set changed.
    pub fn set_default_account_disabled(&self, agent: Agent, disabled: bool) -> Result<bool> {
        let conn = self.lock();
        let mut agents = read_disabled_default_accounts(&conn)?;
        let changed = if disabled {
            agents.insert(agent.wire_name().to_string())
        } else {
            agents.remove(agent.wire_name())
        };
        if !changed {
            return Ok(false);
        }
        let stored = (!agents.is_empty())
            .then(|| serde_json::to_string(&agents).expect("a set of strings serializes to JSON"));
        conn.execute(
            "UPDATE settings SET disabled_default_accounts = ?2 WHERE id = ?1",
            params![SETTINGS_ID, stored],
        )?;
        Ok(true)
    }

    /// Whether the user turned `account_id` (`None` the default account) of `agent` off for new
    /// work. An id that is no account of `agent`, or no longer on record, is not disabled.
    pub fn account_disabled(&self, agent: Agent, account_id: Option<&str>) -> Result<bool> {
        match account_id {
            None => Ok(self
                .disabled_default_accounts()?
                .contains(agent.wire_name())),
            Some(id) => Ok(self
                .get_account(id)?
                .is_some_and(|account| account.agent == agent && account.disabled)),
        }
    }

    /// The name of `agent`'s default account: the user's own, else the daemon's word for it.
    pub fn default_account_name(&self, agent: Agent) -> Result<String> {
        Ok(
            crate::protocol::default_account_name_in(&self.default_account_names()?, agent)
                .to_string(),
        )
    }

    /// What an account is called: the stored account's name (its id when it is gone), or for
    /// `None`, the default account's name.
    pub fn account_display_name(&self, agent: Agent, account_id: Option<&str>) -> Result<String> {
        match account_id {
            Some(id) => Ok(self
                .get_account(id)?
                .map_or_else(|| id.to_string(), |account| account.name)),
            None => self.default_account_name(agent),
        }
    }

    /// Names the agent's default account, or clears the name with `None`. The read and the write
    /// are one step under the store's lock. Whether the stored names changed.
    pub fn set_default_account_name(&self, agent: Agent, name: Option<&str>) -> Result<bool> {
        let conn = self.lock();
        let mut names = read_default_account_names(&conn)?;
        let before = names.clone();
        match name {
            Some(name) => names.insert(agent.wire_name().to_string(), name.to_string()),
            None => names.remove(agent.wire_name()),
        };
        if names == before {
            return Ok(false);
        }
        let stored = (!names.is_empty())
            .then(|| serde_json::to_string(&names).expect("a map of strings serializes to JSON"));
        conn.execute(
            "UPDATE settings SET default_account_names = ?2 WHERE id = ?1",
            params![SETTINGS_ID, stored],
        )?;
        Ok(true)
    }

    pub fn codex_approval_reviewer(&self) -> Result<Option<ApprovalReviewer>> {
        Ok(self.lock().query_row(
            "SELECT codex_approval_reviewer FROM settings WHERE id = ?1",
            params![SETTINGS_ID],
            |row| enum_from_row_opt(row, 0),
        )?)
    }

    pub fn set_codex_approval_reviewer(&self, reviewer: Option<ApprovalReviewer>) -> Result<()> {
        self.lock().execute(
            "UPDATE settings SET codex_approval_reviewer = ?2 WHERE id = ?1",
            params![SETTINGS_ID, reviewer.as_ref().map(enum_to_text)],
        )?;
        Ok(())
    }

    pub fn claude_approval_mode(&self) -> Result<Option<ClaudeApprovalMode>> {
        Ok(self.lock().query_row(
            "SELECT claude_approval_mode FROM settings WHERE id = ?1",
            params![SETTINGS_ID],
            |row| enum_from_row_opt(row, 0),
        )?)
    }

    pub fn set_claude_approval_mode(&self, mode: Option<ClaudeApprovalMode>) -> Result<()> {
        self.lock().execute(
            "UPDATE settings SET claude_approval_mode = ?2 WHERE id = ?1",
            params![SETTINGS_ID, mode.as_ref().map(enum_to_text)],
        )?;
        Ok(())
    }

    /// The application-wide git workflow settings. A project's override is resolved against them by
    /// `git_workflow::effective`, never by reading this directly.
    pub fn git_workflow_settings(&self) -> Result<GitWorkflowSettings> {
        let conn = self.lock();
        read_git_workflow_settings(&conn)
    }

    /// Applies `changes` to the application-wide git workflow settings in one read-modify-write.
    /// Whether anything actually changed.
    pub fn change_git_workflow_settings(
        &self,
        changes: &GitWorkflowSettingsChanges,
    ) -> Result<bool> {
        let conn = self.lock();
        let current = read_git_workflow_settings(&conn)?;
        let next = changes.applied_to(current);
        if next == current {
            return Ok(false);
        }
        conn.execute(
            "UPDATE settings SET development = ?2, rebase_before_merge = ?3, delivery = ?4,
                                 revert_gate = ?5, squash_before_delivery = ?6
             WHERE id = ?1",
            params![
                SETTINGS_ID,
                enum_to_text(&next.development),
                next.rebase_before_merge,
                enum_to_text(&next.delivery),
                next.revert_gate,
                next.squash_before_delivery,
            ],
        )?;
        Ok(true)
    }

    pub fn auto_pull_repositories(&self) -> Result<bool> {
        Ok(self.lock().query_row(
            "SELECT auto_pull_repositories FROM settings WHERE id = ?1",
            params![SETTINGS_ID],
            |row| row.get(0),
        )?)
    }

    /// Whether the setting actually changed.
    pub fn set_auto_pull_repositories(&self, value: bool) -> Result<bool> {
        let conn = self.lock();
        let current: bool = conn.query_row(
            "SELECT auto_pull_repositories FROM settings WHERE id = ?1",
            params![SETTINGS_ID],
            |row| row.get(0),
        )?;
        if current == value {
            return Ok(false);
        }
        conn.execute(
            "UPDATE settings SET auto_pull_repositories = ?2 WHERE id = ?1",
            params![SETTINGS_ID, value],
        )?;
        Ok(true)
    }

    pub fn auto_delete_unresumable_archives(&self) -> Result<bool> {
        Ok(self.lock().query_row(
            "SELECT auto_delete_unresumable_archives FROM settings WHERE id = ?1",
            params![SETTINGS_ID],
            |row| row.get(0),
        )?)
    }

    /// Whether the setting actually changed.
    pub fn set_auto_delete_unresumable_archives(&self, value: bool) -> Result<bool> {
        let changed = self.lock().execute(
            "UPDATE settings SET auto_delete_unresumable_archives = ?2 \
             WHERE id = ?1 AND auto_delete_unresumable_archives != ?2",
            params![SETTINGS_ID, value],
        )?;
        Ok(changed > 0)
    }

    pub fn auto_approve_session_requests(&self) -> Result<bool> {
        Ok(self.lock().query_row(
            "SELECT auto_approve_session_requests FROM settings WHERE id = ?1",
            params![SETTINGS_ID],
            |row| row.get(0),
        )?)
    }

    /// Whether the setting actually changed.
    pub fn set_auto_approve_session_requests(&self, value: bool) -> Result<bool> {
        let changed = self.lock().execute(
            "UPDATE settings SET auto_approve_session_requests = ?2 \
             WHERE id = ?1 AND auto_approve_session_requests != ?2",
            params![SETTINGS_ID, value],
        )?;
        Ok(changed > 0)
    }

    pub fn auto_approve_resume_requests(&self) -> Result<bool> {
        Ok(self.lock().query_row(
            "SELECT auto_approve_resume_requests FROM settings WHERE id = ?1",
            params![SETTINGS_ID],
            |row| row.get(0),
        )?)
    }

    /// Whether the setting actually changed.
    pub fn set_auto_approve_resume_requests(&self, value: bool) -> Result<bool> {
        let changed = self.lock().execute(
            "UPDATE settings SET auto_approve_resume_requests = ?2 \
             WHERE id = ?1 AND auto_approve_resume_requests != ?2",
            params![SETTINGS_ID, value],
        )?;
        Ok(changed > 0)
    }

    pub fn auto_approve_account_switches(&self) -> Result<bool> {
        Ok(self.lock().query_row(
            "SELECT auto_approve_account_switches FROM settings WHERE id = ?1",
            params![SETTINGS_ID],
            |row| row.get(0),
        )?)
    }

    /// Whether the setting actually changed.
    pub fn set_auto_approve_account_switches(&self, value: bool) -> Result<bool> {
        let changed = self.lock().execute(
            "UPDATE settings SET auto_approve_account_switches = ?2 \
             WHERE id = ?1 AND auto_approve_account_switches != ?2",
            params![SETTINGS_ID, value],
        )?;
        Ok(changed > 0)
    }

    pub fn auto_approve_handoffs(&self) -> Result<bool> {
        Ok(self.lock().query_row(
            "SELECT auto_approve_handoffs FROM settings WHERE id = ?1",
            params![SETTINGS_ID],
            |row| row.get(0),
        )?)
    }

    /// Whether the setting actually changed.
    pub fn set_auto_approve_handoffs(&self, value: bool) -> Result<bool> {
        let changed = self.lock().execute(
            "UPDATE settings SET auto_approve_handoffs = ?2 \
             WHERE id = ?1 AND auto_approve_handoffs != ?2",
            params![SETTINGS_ID, value],
        )?;
        Ok(changed > 0)
    }

    pub fn ask_questions_as_choices(&self) -> Result<bool> {
        Ok(self.lock().query_row(
            "SELECT ask_questions_as_choices FROM settings WHERE id = ?1",
            params![SETTINGS_ID],
            |row| row.get(0),
        )?)
    }

    /// Whether the setting actually changed.
    pub fn set_ask_questions_as_choices(&self, value: bool) -> Result<bool> {
        let changed = self.lock().execute(
            "UPDATE settings SET ask_questions_as_choices = ?2 \
             WHERE id = ?1 AND ask_questions_as_choices != ?2",
            params![SETTINGS_ID, value],
        )?;
        Ok(changed > 0)
    }

    /// The tag of the language the user last picked for the interface, as a client reported it;
    /// `None` until one has.
    pub fn interface_language(&self) -> Result<Option<String>> {
        Ok(self.lock().query_row(
            "SELECT interface_language FROM settings WHERE id = ?1",
            params![SETTINGS_ID],
            |row| row.get(0),
        )?)
    }

    /// Stores an already-validated tag. Whether the setting actually changed.
    pub fn set_interface_language(&self, tag: &str) -> Result<bool> {
        let changed = self.lock().execute(
            "UPDATE settings SET interface_language = ?2 \
             WHERE id = ?1 AND interface_language IS NOT ?2",
            params![SETTINGS_ID, tag],
        )?;
        Ok(changed > 0)
    }

    pub fn auto_select_account(&self) -> Result<bool> {
        Ok(self.lock().query_row(
            "SELECT auto_select_account FROM settings WHERE id = ?1",
            params![SETTINGS_ID],
            |row| row.get(0),
        )?)
    }

    /// Whether the setting actually changed.
    pub fn set_auto_select_account(&self, value: bool) -> Result<bool> {
        let changed = self.lock().execute(
            "UPDATE settings SET auto_select_account = ?2 \
             WHERE id = ?1 AND auto_select_account != ?2",
            params![SETTINGS_ID, value],
        )?;
        Ok(changed > 0)
    }

    pub fn auto_switch_on_usage_limit(&self) -> Result<bool> {
        Ok(self.lock().query_row(
            "SELECT auto_switch_on_usage_limit FROM settings WHERE id = ?1",
            params![SETTINGS_ID],
            |row| row.get(0),
        )?)
    }

    /// Whether the setting actually changed.
    pub fn set_auto_switch_on_usage_limit(&self, value: bool) -> Result<bool> {
        let changed = self.lock().execute(
            "UPDATE settings SET auto_switch_on_usage_limit = ?2 \
             WHERE id = ?1 AND auto_switch_on_usage_limit != ?2",
            params![SETTINGS_ID, value],
        )?;
        Ok(changed > 0)
    }

    pub fn sidebar_order_by_status(&self) -> Result<bool> {
        Ok(self.lock().query_row(
            "SELECT sidebar_order_by_status FROM settings WHERE id = ?1",
            params![SETTINGS_ID],
            |row| row.get(0),
        )?)
    }

    /// Whether the setting actually changed.
    pub fn set_sidebar_order_by_status(&self, value: bool) -> Result<bool> {
        let changed = self.lock().execute(
            "UPDATE settings SET sidebar_order_by_status = ?2 \
             WHERE id = ?1 AND sidebar_order_by_status != ?2",
            params![SETTINGS_ID, value],
        )?;
        Ok(changed > 0)
    }

    pub fn auto_start_quota_windows(&self) -> Result<bool> {
        Ok(self.lock().query_row(
            "SELECT auto_start_quota_windows FROM settings WHERE id = ?1",
            params![SETTINGS_ID],
            |row| row.get(0),
        )?)
    }

    /// Whether the setting actually changed.
    pub fn set_auto_start_quota_windows(&self, value: bool) -> Result<bool> {
        let changed = self.lock().execute(
            "UPDATE settings SET auto_start_quota_windows = ?2 \
             WHERE id = ?1 AND auto_start_quota_windows != ?2",
            params![SETTINGS_ID, value],
        )?;
        Ok(changed > 0)
    }

    /// The directory a clone lands in when none is named: the stored one, else `~/Projects`
    /// expanded, which is as absolute and lexically normalised as the home it expands from.
    pub fn default_clone_dir(&self) -> Result<String> {
        let stored: Option<String> = self.lock().query_row(
            "SELECT default_clone_dir FROM settings WHERE id = ?1",
            params![SETTINGS_ID],
            |row| row.get(0),
        )?;
        Ok(stored.unwrap_or_else(|| {
            crate::hostfs::expand(BUILT_IN_CLONE_DIR)
                .to_string_lossy()
                .into_owned()
        }))
    }

    /// Stores the already-normalised directory, or `None` to go back to the built-in default.
    /// Whether the setting actually changed.
    pub fn set_default_clone_dir(&self, value: Option<&str>) -> Result<bool> {
        let conn = self.lock();
        let current: Option<String> = conn.query_row(
            "SELECT default_clone_dir FROM settings WHERE id = ?1",
            params![SETTINGS_ID],
            |row| row.get(0),
        )?;
        if current.as_deref() == value {
            return Ok(false);
        }
        conn.execute(
            "UPDATE settings SET default_clone_dir = ?2 WHERE id = ?1",
            params![SETTINGS_ID, value],
        )?;
        Ok(true)
    }

    /// The agents the user has added, in the order the user set (a newly added agent goes last);
    /// `None` while the list has never been decided (a fresh database, until
    /// `availability::fill_added_agents` fills it). A list saved empty on purpose is `Some` of
    /// nothing, which is what tells the two apart.
    pub fn added_agents(&self) -> Result<Option<Vec<Agent>>> {
        read_added_agents(&self.lock())
    }

    /// Stores the list only while it is still undecided (unset, or unreadable), so a list the user
    /// has since edited is never overwritten. Whether it was stored. The read and the write are
    /// one step under the store's lock.
    pub fn set_added_agents_if_undecided(&self, agents: &[Agent]) -> Result<bool> {
        let conn = self.lock();
        if read_added_agents(&conn)?.is_some() {
            return Ok(false);
        }
        write_added_agents(&conn, agents)?;
        Ok(true)
    }

    /// Appends the agent to a decided list. Whether it was appended: `false` when it was already
    /// there. Refused with an error while the list is undecided, since appending would decide it
    /// as just this agent; the caller fills it first. The read and the write are one step under
    /// the store's lock.
    pub fn add_added_agent(&self, agent: Agent) -> Result<bool> {
        let conn = self.lock();
        let mut agents = read_added_agents(&conn)?
            .ok_or_else(|| anyhow::anyhow!("the added agents are not decided yet"))?;
        if agents.contains(&agent) {
            return Ok(false);
        }
        agents.push(agent);
        write_added_agents(&conn, &agents)?;
        Ok(true)
    }

    /// Puts a decided list in the given order, and says what came of it. The read and the write are
    /// one step under the store's lock.
    pub fn reorder_added_agents(&self, order: &[Agent]) -> Result<ReorderOutcome> {
        let conn = self.lock();
        let Some(current) = read_added_agents(&conn)? else {
            return Ok(ReorderOutcome::Undecided);
        };
        let mut sorted_current = current.clone();
        let mut sorted_order = order.to_vec();
        sorted_current.sort_by_key(|agent| agent.wire_name());
        sorted_order.sort_by_key(|agent| agent.wire_name());
        if sorted_current != sorted_order {
            return Ok(ReorderOutcome::NotAPermutation);
        }
        if current == order {
            return Ok(ReorderOutcome::Unchanged);
        }
        write_added_agents(&conn, order)?;
        Ok(ReorderOutcome::Reordered)
    }

    pub fn account_order(&self, agent: Agent) -> Result<Vec<Option<String>>> {
        Ok(read_account_order(&self.lock())?
            .remove(agent.wire_name())
            .unwrap_or_default())
    }

    /// Validates and saves an agent's display order under one lock, so an account edit cannot
    /// race the validation. Other agents' orders and the accounts' storage order stay untouched.
    pub fn reorder_accounts(&self, agent: Agent, order: &[Option<String>]) -> Result<bool> {
        let conn = self.lock();
        let mut current_accounts = conn
            .prepare("SELECT id FROM accounts WHERE agent = ?1")?
            .query_map(params![enum_to_text(&agent)], |row| row.get::<_, String>(0))?
            .map(|id| id.map(Some))
            .collect::<rusqlite::Result<Vec<_>>>()?;
        current_accounts.push(None);
        current_accounts.sort();
        let mut sorted_order = order.to_vec();
        sorted_order.sort();
        if current_accounts != sorted_order {
            return Err(CodedError::raised(
                error_code::ACCOUNT_ORDER_INVALID,
                "the new order must name each account of the agent and its default exactly once"
                    .to_string(),
                &[],
            ));
        }
        let mut orders = read_account_order(&conn)?;
        if orders
            .get(agent.wire_name())
            .is_some_and(|current| current == order)
        {
            return Ok(false);
        }
        orders.insert(agent.wire_name().to_string(), order.to_vec());
        conn.execute(
            "UPDATE settings SET account_order = ?2 WHERE id = ?1",
            params![SETTINGS_ID, serde_json::to_string(&orders)?],
        )?;
        Ok(true)
    }

    /// Takes the agent off the list, leaving its accounts alone. Whether it was on it. Refused with
    /// an error while the list is undecided, as `add_added_agent` is.
    pub fn remove_added_agent(&self, agent: Agent) -> Result<bool> {
        let conn = self.lock();
        let mut agents = read_added_agents(&conn)?
            .ok_or_else(|| anyhow::anyhow!("the added agents are not decided yet"))?;
        let before = agents.len();
        agents.retain(|added| *added != agent);
        if agents.len() == before {
            return Ok(false);
        }
        write_added_agents(&conn, &agents)?;
        Ok(true)
    }

    // -- messages held for interrupted sessions -------------------------------

    /// Holds `text`, a message for the interrupted session `session_id`, until it is resumed and
    /// the message has been written into it. Returns the row it is kept in.
    pub fn add_kept_message(&self, session_id: &str, text: &str) -> Result<i64> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO pending_info (session_id, text) VALUES (?1, ?2)",
            params![session_id, text],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// What is held for the session with the row each is kept in, oldest first.
    pub fn kept_messages(&self, session_id: &str) -> Result<Vec<(i64, String)>> {
        let conn = self.lock();
        let mut stmt =
            conn.prepare("SELECT id, text FROM pending_info WHERE session_id = ?1 ORDER BY id")?;
        let rows = stmt
            .query_map(params![session_id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Drops the held messages in these rows, which have been written into their session, in one
    /// transaction.
    pub fn delete_kept_messages(&self, rows: &[i64]) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        for row in rows {
            tx.execute("DELETE FROM pending_info WHERE id = ?1", params![row])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Drops everything held for the session, which will not take it.
    pub fn clear_kept_messages(&self, session_id: &str) -> Result<()> {
        self.lock().execute(
            "DELETE FROM pending_info WHERE session_id = ?1",
            params![session_id],
        )?;
        Ok(())
    }

    // -- pages ---------------------------------------------------------------

    pub fn get_page(&self, id: &str) -> Result<Option<Page>> {
        let conn = self.lock();
        let page = conn
            .query_row(
                "SELECT id, console_session_id, html, anchor_message_id, created_at
                 FROM pages WHERE id = ?1",
                params![id],
                read_page,
            )
            .optional()?;
        Ok(page)
    }

    /// Stores `page` unless its session has the role of a bound session by now (it is bound and
    /// not a lead session, see [`Session::is_worker`]), checked in the same statement so that
    /// a binding written meanwhile cannot leave a page behind it. Returns whether it was stored.
    pub fn insert_page_unless_bound(&self, page: &Page) -> Result<bool> {
        let inserted = self.lock().execute(
            "INSERT INTO pages (id, console_session_id, html, anchor_message_id, created_at)
             SELECT ?1, ?2, ?3, ?4, ?5
             WHERE NOT EXISTS (SELECT 1 FROM sessions
                               WHERE id = ?2 AND bound_to IS NOT NULL AND lead = 0)",
            params![
                page.id,
                page.console_session_id,
                page.html,
                page.anchor_message_id,
                page.created_at,
            ],
        )?;
        Ok(inserted > 0)
    }

    #[cfg(test)]
    pub fn insert_page(&self, page: &Page) -> Result<()> {
        self.lock().execute(
            "INSERT INTO pages (id, console_session_id, html, anchor_message_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                page.id,
                page.console_session_id,
                page.html,
                page.anchor_message_id,
                page.created_at,
            ],
        )?;
        Ok(())
    }

    /// Oldest first: `created_at` alone can tie at millisecond resolution, so `id` breaks the tie
    /// deterministically rather than leaving the order to SQLite's whim.
    pub fn list_pages(&self, session_id: &str) -> Result<Vec<Page>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, console_session_id, html, anchor_message_id, created_at
             FROM pages WHERE console_session_id = ?1 ORDER BY created_at, id",
        )?;
        let rows = stmt
            .query_map(params![session_id], read_page)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Deletes pages of the session `session_id` and returns the ids it removed: the `named` ones,
    /// each once and in the order given, or with `None` every page it has, oldest first. Checked
    /// and deleted in one transaction, so a page two callers delete at once is reported by only
    /// one of them. A named id that is not one of the session's pages (another session's, one
    /// already gone, one that never was) deletes nothing and comes back as the `Err`.
    pub fn delete_pages(
        &self,
        session_id: &str,
        named: Option<&[String]>,
    ) -> Result<std::result::Result<Vec<String>, String>> {
        let conn = self.lock();
        let tx = conn.unchecked_transaction()?;
        let ids: Vec<String> = match named {
            Some(named) => {
                let mut ids: Vec<String> = Vec::new();
                for id in named {
                    if !ids.contains(id) {
                        ids.push(id.clone());
                    }
                }
                ids
            }
            None => {
                let mut stmt = tx.prepare(
                    "SELECT id FROM pages WHERE console_session_id = ?1 ORDER BY created_at, id",
                )?;
                let ids = stmt
                    .query_map(params![session_id], |row| row.get(0))?
                    .collect::<rusqlite::Result<Vec<String>>>()?;
                ids
            }
        };
        let mut deleted = Vec::with_capacity(ids.len());
        for id in ids {
            let removed: Option<String> = tx
                .query_row(
                    "DELETE FROM pages WHERE id = ?1 AND console_session_id = ?2 RETURNING id",
                    params![id, session_id],
                    |row| row.get(0),
                )
                .optional()?;
            match removed {
                Some(removed) => deleted.push(removed),
                // Dropping the transaction rolls back the ones deleted before this one.
                None => return Ok(Err(id)),
            }
        }
        tx.commit()?;
        Ok(Ok(deleted))
    }

    /// The ids of the sessions that have a page and the role of a bound session (bound and not a
    /// lead session, see [`Session::is_worker`]), whose pages no tool of theirs can list.
    pub fn session_ids_with_pages_and_bound_role(&self) -> Result<Vec<String>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT DISTINCT p.console_session_id FROM pages p
             JOIN sessions s ON s.id = p.console_session_id
             WHERE s.bound_to IS NOT NULL AND s.lead = 0
             ORDER BY p.console_session_id",
        )?;
        let ids = stmt
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(ids)
    }

    /// The id of the session's newest page, or `None` if it has none yet. The ordering is the exact
    /// reverse of [`Self::list_pages`]'s, so the two never disagree about which page is newest when
    /// `created_at` ties.
    pub fn newest_page_id(&self, session_id: &str) -> Result<Option<String>> {
        let id = self
            .lock()
            .query_row(
                "SELECT id FROM pages WHERE console_session_id = ?1 ORDER BY created_at DESC, id DESC LIMIT 1",
                params![session_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(id)
    }

    // -- sessions ------------------------------------------------------------

    /// Inserts a session that is not a console session, or a console session whose colour and
    /// ordinal are already decided (a test fixture; the daemon itself always creates a console
    /// session through [`Self::insert_console_session`] instead, which decides them).
    pub fn insert_session(&self, session: &Session) -> Result<()> {
        insert_session_row(&self.lock(), session)
    }

    /// Inserts a project session bound to `owner`, only while `owner` can still own sessions — a
    /// console session, or a project session that is unbound or a lead session, the condition
    /// [`Self::set_binding`] keeps — and says whether it did. The check and the insert run under
    /// one lock, so a binding that lands between the caller's own check and this write either
    /// came first, and is seen here, or comes after, and sees the new session as a child of
    /// `owner` (`set_binding`'s `childless`): orchestration cannot reach a third level either way.
    /// `false` also covers an `owner` that is gone.
    pub fn insert_session_under_owner(&self, session: &Session, owner: &str) -> Result<bool> {
        let conn = self.lock();
        let can_own: bool = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM sessions AS owner WHERE owner.id = ?1
                              AND (owner.role = ?2 OR owner.bound_to IS NULL OR owner.lead = 1))",
            params![owner, enum_to_text(&Role::Console)],
            |row| row.get(0),
        )?;
        if !can_own {
            return Ok(false);
        }
        insert_session_row(&conn, session)?;
        Ok(true)
    }

    /// Inserts a freshly-opened console session, assigning its colour and ordinal under the same
    /// lock as the row insert. With no explicit title, it uses the console's current name.
    ///
    /// The lock prevents concurrent opens from choosing the same available colour or ordinal.
    /// Ordinals are consumed before launch and are not given back if the agent fails to start.
    /// `session.colour`, `session.ordinal` and `session.title` are overwritten.
    pub fn insert_console_session(
        &self,
        mut session: Session,
        title: Option<String>,
    ) -> Result<Session> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT colour FROM sessions
             WHERE console_id = ?1 AND role = ?2 AND status != ?3 AND colour IS NOT NULL",
        )?;
        let used_colours: Vec<String> = stmt
            .query_map(
                params![
                    session.console_id,
                    enum_to_text(&Role::Console),
                    enum_to_text(&SessionStatus::Archived),
                ],
                |row| row.get(0),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        let held: Vec<ConsoleSessionColour> = used_colours
            .iter()
            .filter_map(|text| enum_from_text(text).ok())
            .collect();
        // Every entry already held: pick among all rather than refuse, since the badge is a
        // convenience, not a uniqueness guarantee the user was ever promised.
        let colour = assign_console_session_colour(&held, uuid::Uuid::new_v4().as_u128() as usize);
        let (ordinal, console_name): (i64, String) = conn.query_row(
            "UPDATE consoles SET next_console_session_ordinal = next_console_session_ordinal + 1
             WHERE id = ?1
             RETURNING next_console_session_ordinal - 1, name",
            params![session.console_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        session.colour = Some(colour);
        session.ordinal = Some(ordinal);
        session.title = title.unwrap_or(console_name);
        insert_session_row(&conn, &session)?;
        Ok(session)
    }

    /// Writes a whole record's changing fields back, for tests to put a session in a state. The
    /// daemon itself never writes a record it read earlier back whole: whatever another task wrote
    /// to the row in between would be lost, so each change goes through the method that writes its
    /// own columns ([`Self::set_session_title`], [`Self::set_running_status`],
    /// [`Self::archive_session`] and the others below). Identity, placement, the binding, the
    /// account, `pinned`, `locked` and `colour` are left alone, as those methods leave them.
    /// Returns whether a row was written: `false` means the session is gone.
    #[cfg(test)]
    pub fn update_session(&self, session: &Session) -> Result<bool> {
        let changed = self.lock().execute(
            "UPDATE sessions SET agent = ?2, agent_session_id = ?3, title = ?4, status = ?5,
                                 has_conversation = ?6, started_at = ?7, ended_at = ?8,
                                 context = CASE WHEN agent = ?2 AND agent_session_id IS ?3 THEN context ELSE NULL END,
                                 record_title = CASE WHEN agent = ?2 AND agent_session_id IS ?3 THEN record_title ELSE NULL END,
                                 record_prompt = CASE WHEN agent = ?2 AND agent_session_id IS ?3 THEN record_prompt ELSE NULL END,
                                 observed_model = CASE WHEN agent = ?2 AND agent_session_id IS ?3 THEN observed_model ELSE NULL END,
                                 observed_effort = CASE WHEN agent = ?2 AND agent_session_id IS ?3 THEN observed_effort ELSE NULL END,
                                 observed_boundary = CASE WHEN agent = ?2 AND agent_session_id IS ?3 THEN observed_boundary ELSE NULL END
             WHERE id = ?1",
            params![
                session.id,
                enum_to_text(&session.agent),
                session.agent_session_id,
                session.title,
                enum_to_text(&session.status),
                session.has_conversation,
                session.started_at,
                session.ended_at,
            ],
        )?;
        Ok(changed > 0)
    }

    /// Runs `update`, one `UPDATE` of session `id`'s row, and returns the row as it then stands
    /// when the statement matched it, `None` when it did not. Both run under one take of the lock,
    /// so the record returned is the one the write produced.
    fn update_session_row(
        &self,
        id: &str,
        update: &str,
        params: impl rusqlite::Params,
    ) -> Result<Option<Session>> {
        let conn = self.lock();
        if conn.execute(update, params)? == 0 {
            return Ok(None);
        }
        Ok(conn
            .query_row(
                &format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1"),
                params![id],
                read_session,
            )
            .optional()?)
    }

    /// Writes the session's title and nothing else. Returns the record as it now stands, or `None`
    /// when the session is gone.
    pub fn set_session_title(&self, id: &str, title: &str) -> Result<Option<Session>> {
        self.update_session_row(
            id,
            "UPDATE sessions SET title = ?2 WHERE id = ?1",
            params![id, title],
        )
    }

    /// Records the id the agent gave the session's conversation, unless it already holds that id.
    /// What the agent's records gave the session (context, record titles, observed model and
    /// effort) belonged to the previous conversation and is dropped with it. Returns the record as
    /// it now stands, or `None` when nothing changed or the session is gone.
    pub fn set_agent_session_id(
        &self,
        id: &str,
        agent_session_id: &str,
    ) -> Result<Option<Session>> {
        self.update_session_row(
            id,
            "UPDATE sessions SET agent_session_id = ?2, context = NULL, record_title = NULL,
                                 record_prompt = NULL, observed_model = NULL,
                                 observed_effort = NULL, observed_boundary = NULL
             WHERE id = ?1 AND agent_session_id IS NOT ?2",
            params![id, agent_session_id],
        )
    }

    /// Records that the session has had a turn. Returns the record as it now stands, or `None`
    /// when it was already recorded or the session is gone.
    pub fn mark_conversation_started(&self, id: &str) -> Result<Option<Session>> {
        self.update_session_row(
            id,
            "UPDATE sessions SET has_conversation = 1 WHERE id = ?1 AND has_conversation = 0",
            params![id],
        )
    }

    /// Forgets the session's conversation, its id and what the agent's records gave it, as for a
    /// session that has had no turn. Returns the record as it now stands, or `None` when the
    /// session is gone.
    pub fn forget_conversation(&self, id: &str) -> Result<Option<Session>> {
        self.update_session_row(
            id,
            "UPDATE sessions SET has_conversation = 0, agent_session_id = NULL, context = NULL,
                                 record_title = NULL, record_prompt = NULL, observed_model = NULL,
                                 observed_effort = NULL, observed_boundary = NULL
             WHERE id = ?1",
            params![id],
        )
    }

    /// Moves a session to `status`, which must not be dormant, but only while the session is not
    /// dormant itself and is at another status. A running session's status follows its agent's
    /// events, which arrive on tasks of their own; matching in the statement means one that lands
    /// after an archive or an exit cannot bring the session back to life. Returns the record as it
    /// now stands, or `None` when nothing changed or the session is gone.
    pub fn set_running_status(&self, id: &str, status: SessionStatus) -> Result<Option<Session>> {
        debug_assert!(!status.is_dormant());
        self.update_session_row(
            id,
            "UPDATE sessions SET status = ?2
             WHERE id = ?1 AND status != ?2 AND status NOT IN (?3, ?4)",
            params![
                id,
                enum_to_text(&status),
                enum_to_text(&SessionStatus::Interrupted),
                enum_to_text(&SessionStatus::Archived),
            ],
        )
    }

    /// Archives the session as of `ended_at`, from whatever status it is at; a locked session is
    /// refused by the schema. Returns the record as it now stands, or `None` when the session is
    /// gone.
    pub fn archive_session(&self, id: &str, ended_at: i64) -> Result<Option<Session>> {
        self.update_session_row(
            id,
            "UPDATE sessions SET status = ?2, ended_at = ?3 WHERE id = ?1",
            params![id, enum_to_text(&SessionStatus::Archived), ended_at],
        )
    }

    /// Interrupts the session as of `ended_at`, unless it is archived: an archive is the user's
    /// decision and outlasts the process going away. Returns the record as it now stands, or
    /// `None` when the session is archived or gone.
    pub fn interrupt_session(&self, id: &str, ended_at: i64) -> Result<Option<Session>> {
        self.update_session_row(
            id,
            "UPDATE sessions SET status = ?2, ended_at = ?3 WHERE id = ?1 AND status != ?4",
            params![
                id,
                enum_to_text(&SessionStatus::Interrupted),
                ended_at,
                enum_to_text(&SessionStatus::Archived),
            ],
        )
    }

    /// Records that the session's process ended at `ended_at`: an archived session stays archived,
    /// anything else becomes interrupted. Returns the status the session was at and the record as
    /// it now stands, or `None` when the session is gone; both are read under the write's lock.
    pub fn record_session_exit(
        &self,
        id: &str,
        ended_at: i64,
    ) -> Result<Option<(SessionStatus, Session)>> {
        let conn = self.lock();
        let select = format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1");
        let Some(before) = conn
            .query_row(&select, params![id], read_session)
            .optional()?
        else {
            return Ok(None);
        };
        conn.execute(
            "UPDATE sessions SET status = CASE WHEN status = ?2 THEN status ELSE ?3 END, ended_at = ?4
             WHERE id = ?1",
            params![
                id,
                enum_to_text(&SessionStatus::Archived),
                enum_to_text(&SessionStatus::Interrupted),
                ended_at,
            ],
        )?;
        let after = conn.query_row(&select, params![id], read_session)?;
        Ok(Some((before.status, after)))
    }

    /// Persists what the agent's records gave sessions, only while the source still belongs to the
    /// session's current agent identity; unrelated session writes never overwrite these columns.
    /// Returns the sessions that changed.
    pub fn set_record_data(&self, updates: &[RecordUpdate]) -> Result<Vec<Session>> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let mut changed = Vec::new();
        for update in updates {
            let source = &update.session;
            let session = tx
                .query_row(
                    &format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1"),
                    params![source.id],
                    read_session,
                )
                .optional()?;
            let Some(mut session) = session else { continue };
            if session.agent != source.agent
                || session.agent_session_id != source.agent_session_id
                || session.config_dir != source.config_dir
            {
                continue;
            }
            let context = update
                .context
                .as_ref()
                .filter(|context| &session.context != *context);
            let titles = update
                .titles
                .as_ref()
                .filter(|titles| &session.record_titles != *titles);
            let boundary: Option<String> = tx.query_row(
                "SELECT observed_boundary FROM sessions WHERE id = ?1",
                params![source.id],
                |row| row.get(0),
            )?;
            let current_observations = boundary == update.observed_boundary;
            let observed_model = update.observed_model.as_ref().filter(|model| {
                current_observations && session.observed_model.as_ref() != Some(*model)
            });
            let observed_effort = update
                .observed_effort
                .as_ref()
                .filter(|effort| current_observations && &session.observed_effort != *effort);
            if context.is_none()
                && titles.is_none()
                && observed_model.is_none()
                && observed_effort.is_none()
            {
                continue;
            }
            if let Some(context) = context {
                tx.execute(
                    "UPDATE sessions SET context = ?2 WHERE id = ?1",
                    params![
                        source.id,
                        context.as_ref().map(serde_json::to_string).transpose()?
                    ],
                )?;
                session.context = context.clone();
            }
            if let Some(titles) = titles {
                tx.execute(
                    "UPDATE sessions SET record_title = ?2, record_prompt = ?3 WHERE id = ?1",
                    params![source.id, titles.named, titles.prompt],
                )?;
                session.record_titles = titles.clone();
            }
            if let Some(model) = observed_model {
                tx.execute(
                    "UPDATE sessions SET observed_model = ?2 WHERE id = ?1",
                    params![source.id, model],
                )?;
                session.observed_model = Some(model.clone());
            }
            if let Some(effort) = observed_effort {
                tx.execute(
                    "UPDATE sessions SET observed_effort = ?2 WHERE id = ?1",
                    params![source.id, effort],
                )?;
                session.observed_effort = effort.clone();
            }
            changed.push(session);
        }
        tx.commit()?;
        Ok(changed)
    }

    /// Writes only the terminal title, and skips unchanged, dormant or deleted rows. Shutdown
    /// cleanup may still emit OSC titles after archiving or interruption; keep the last active
    /// title instead. The read and write share the store lock with status updates, so that
    /// boundary and concurrent renames are preserved.
    pub fn set_terminal_title(&self, id: &str, title: Option<&str>) -> Result<Option<Session>> {
        let conn = self.lock();
        let session = conn
            .query_row(
                &format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1"),
                params![id],
                read_session,
            )
            .optional()?;
        let Some(mut session) = session else {
            return Ok(None);
        };
        if session.status.is_dormant() || session.terminal_title.as_deref() == title {
            return Ok(None);
        }
        conn.execute(
            "UPDATE sessions SET terminal_title = ?2 WHERE id = ?1",
            params![id, title],
        )?;
        session.terminal_title = title.map(str::to_owned);
        Ok(Some(session))
    }

    /// Moves a session to `status`, but only while it is still `expected`, and returns the record
    /// as it now stands when it moved. Both statements run under one lock, so a caller whose write
    /// is only valid while the session has not moved on cannot be overtaken between deciding and
    /// writing — which a write after a separate [`Self::get_session`] can be.
    /// `None` means the session was something other than `expected`, or is gone. Asking for the
    /// status it already has counts as a move, so a caller that would gain nothing from that should
    /// not ask.
    pub fn update_session_status_if(
        &self,
        id: &str,
        expected: SessionStatus,
        status: SessionStatus,
    ) -> Result<Option<Session>> {
        let conn = self.lock();
        let moved = conn.execute(
            "UPDATE sessions SET status = ?3 WHERE id = ?1 AND status = ?2",
            params![id, enum_to_text(&expected), enum_to_text(&status)],
        )?;
        if moved == 0 {
            return Ok(None);
        }
        let session = conn
            .query_row(
                &format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1"),
                params![id],
                read_session,
            )
            .optional()?;
        Ok(session)
    }

    /// Changes a launch attempt's status and end time together, only while its expected status
    /// still holds. A later archive wins over an optimistic relaunch or its failure rollback.
    pub fn update_session_lifecycle_if(
        &self,
        id: &str,
        expected: SessionStatus,
        status: SessionStatus,
        ended_at: Option<i64>,
    ) -> Result<Option<Session>> {
        let conn = self.lock();
        let moved = conn.execute(
            "UPDATE sessions SET status = ?3, ended_at = ?4 WHERE id = ?1 AND status = ?2",
            params![id, enum_to_text(&expected), enum_to_text(&status), ended_at],
        )?;
        if moved == 0 {
            return Ok(None);
        }
        Ok(conn
            .query_row(
                &format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1"),
                params![id],
                read_session,
            )
            .optional()?)
    }

    /// Records the account a session runs under and its directory, together and by nothing else:
    /// the one place either is written after the session is created. Returns the record as it now
    /// stands, or `None` when the session is gone.
    pub fn set_session_account(
        &self,
        id: &str,
        account_id: Option<&str>,
        config_dir: Option<&str>,
    ) -> Result<Option<Session>> {
        let conn = self.lock();
        let changed = conn.execute(
            "UPDATE sessions SET account_id = ?2, config_dir = ?3 WHERE id = ?1",
            params![id, account_id, config_dir],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        Ok(conn
            .query_row(
                &format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1"),
                params![id],
                read_session,
            )
            .optional()?)
    }

    /// Binds the project session `id`, currently bound to `from` (`None` for unbound), to `owner`,
    /// with `lead` as its new lead flag, in one statement that matches only while that still holds,
    /// so of two changes racing for it only one lands. It also matches only while `owner` can own
    /// sessions — a console session, or a project session that is unbound or a lead session — and,
    /// with `childless`, only while no session is bound to `id`: the two conditions that keep
    /// orchestration two levels deep cannot be undone by a binding that lands between the
    /// caller's check and this write. Returns the record as it now stands, or `None` when nothing
    /// changed. What else `owner` may be is the caller's to check. With `owe_notice` the session is
    /// marked owed a notice of its new owner ([`Self::mark_owner_notice`]) in the same statement,
    /// so the change and the mark land together. Any parting note a release left owed is dropped:
    /// the release is no longer the latest change of owner.
    pub fn set_binding(
        &self,
        id: &str,
        from: Option<&str>,
        owner: &str,
        lead: bool,
        childless: bool,
        owe_notice: bool,
    ) -> Result<Option<Session>> {
        let conn = self.lock();
        let changed = conn.execute(
            "UPDATE sessions SET bound_to = ?3, lead = ?4,
                    owner_notice_pending = owner_notice_pending + ?8,
                    owner_notice_note = NULL
             WHERE id = ?1 AND role = ?5 AND bound_to IS ?2
               AND EXISTS (SELECT 1 FROM sessions AS owner WHERE owner.id = ?3
                             AND (owner.role = ?6 OR owner.bound_to IS NULL OR owner.lead = 1))
               AND (?7 = 0
                    OR NOT EXISTS (SELECT 1 FROM sessions AS bound WHERE bound.bound_to = ?1))",
            params![
                id,
                from,
                owner,
                lead,
                enum_to_text(&Role::Project),
                enum_to_text(&Role::Console),
                childless,
                owe_notice
            ],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        Ok(conn
            .query_row(
                &format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1"),
                params![id],
                read_session,
            )
            .optional()?)
    }

    /// How many sessions are bound to `owner`, in any status, counted in the store.
    pub fn count_bound_to(&self, owner: &str) -> Result<usize> {
        Ok(self.lock().query_row(
            "SELECT COUNT(*) FROM sessions WHERE bound_to = ?1",
            params![owner],
            |row| row.get(0),
        )?)
    }

    /// Unbinds the project session `id`, currently bound to `from`: `bound_to` cleared and `lead`
    /// off, in one statement that matches only while `id` is still bound to `from`, so of two
    /// changes racing for it only one lands. The sessions bound to `id` stay bound to it. Returns
    /// the record as it now stands, or `None` when nothing changed. With `owe_notice` the session
    /// is marked owed a notice ([`Self::mark_owner_notice`]) in the same statement, and `note`,
    /// the releasing owner's parting words, is kept with it ([`Self::owner_notice_note`]);
    /// without, there is nobody to keep it for and it is dropped.
    pub fn clear_binding(
        &self,
        id: &str,
        from: &str,
        owe_notice: bool,
        note: Option<&str>,
    ) -> Result<Option<Session>> {
        let conn = self.lock();
        let changed = conn.execute(
            "UPDATE sessions SET bound_to = NULL, lead = 0,
                    owner_notice_pending = owner_notice_pending + ?4,
                    owner_notice_note = CASE WHEN ?4 THEN ?5 END
             WHERE id = ?1 AND role = ?3 AND bound_to IS ?2",
            params![id, from, enum_to_text(&Role::Project), owe_notice, note],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        Ok(conn
            .query_row(
                &format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1"),
                params![id],
                read_session,
            )
            .optional()?)
    }

    /// Moves the sessions bound to `from` to `to`, and, with `lead_of`, binds `to`, an unbound
    /// project session, to that console session as a lead session, all in one transaction.
    /// Returns the ids of the sessions moved, or `None` when the lead binding did not apply (`to`
    /// was bound meanwhile, or `lead_of` is not a console session) and nothing changed. What may
    /// own what is the caller's to check.
    pub fn hand_over_binding(
        &self,
        from: &str,
        to: &str,
        lead_of: Option<&str>,
    ) -> Result<Option<Vec<String>>> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        if let Some(owner) = lead_of {
            let bound = tx.execute(
                "UPDATE sessions SET bound_to = ?2, lead = 1, owner_notice_note = NULL
                 WHERE id = ?1 AND role = ?3 AND bound_to IS NULL
                   AND EXISTS (SELECT 1 FROM sessions AS owner WHERE owner.id = ?2 AND owner.role = ?4)",
                params![
                    to,
                    owner,
                    enum_to_text(&Role::Project),
                    enum_to_text(&Role::Console)
                ],
            )?;
            if bound == 0 {
                return Ok(None);
            }
        }
        let moved = tx
            .prepare("SELECT id FROM sessions WHERE bound_to = ?1")?
            .query_map(params![from], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        tx.execute(
            "UPDATE sessions SET bound_to = ?2 WHERE bound_to = ?1",
            params![from, to],
        )?;
        tx.commit()?;
        Ok(Some(moved))
    }

    pub fn set_session_locked(&self, id: &str, locked: bool) -> Result<()> {
        self.lock().execute(
            "UPDATE sessions SET locked = ?2 WHERE id = ?1",
            params![id, locked],
        )?;
        Ok(())
    }

    /// Locks session `id` unless it is archived, deciding that and writing the lock in one
    /// statement, so a lock cannot land on a session archived a moment before. Returns false when
    /// it refused for that reason; an unknown session changes nothing and is not refused, as
    /// [`Self::set_session_locked`] does.
    pub fn set_session_locked_unless_archived(&self, id: &str) -> Result<bool> {
        let conn = self.lock();
        let changed = conn.execute(
            "UPDATE sessions SET locked = 1 WHERE id = ?1 AND status != 'archived'",
            params![id],
        )?;
        if changed != 0 {
            return Ok(true);
        }
        let archived = conn
            .query_row(
                "SELECT 1 FROM sessions WHERE id = ?1 AND status = 'archived'",
                params![id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        Ok(!archived)
    }

    /// Records that session `id` is owed a notice of a change of its owner: it was not running to
    /// be told, and its next launch delivers it. Kept apart from the session record, which clients
    /// read. The notice carries no text, since it is built from the binding at delivery, so a later
    /// change replaces an earlier pending one. Each call counts up, which is what lets a launch
    /// that delivered the notice clear only what it saw ([`Self::clear_owner_notice`]).
    pub fn mark_owner_notice(&self, id: &str) -> Result<()> {
        self.lock().execute(
            "UPDATE sessions SET owner_notice_pending = owner_notice_pending + 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// [`Self::mark_owner_notice`] for a release whose message could not be written to a running
    /// session: the former owner's parting `note` is kept with the mark, in the same statement,
    /// only while the session is still unbound, so a change of owner since leaves no stale note.
    pub fn mark_owner_notice_with_note(&self, id: &str, note: &str) -> Result<()> {
        self.lock().execute(
            "UPDATE sessions SET owner_notice_pending = owner_notice_pending + 1,
                    owner_notice_note = CASE WHEN bound_to IS NULL THEN ?2 END
             WHERE id = ?1",
            params![id, note],
        )?;
        Ok(())
    }

    /// The count of notices of a change of its owner session `id` is owed, 0 for none (see
    /// [`Self::mark_owner_notice`]).
    pub fn owner_notice_pending(&self, id: &str) -> Result<i64> {
        Ok(self
            .lock()
            .query_row(
                "SELECT owner_notice_pending FROM sessions WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }

    /// The parting note a release left with the notice session `id` is owed, if it left one and no
    /// change of owner has come since (see [`Self::clear_binding`]).
    pub fn owner_notice_note(&self, id: &str) -> Result<Option<String>> {
        Ok(self
            .lock()
            .query_row(
                "SELECT owner_notice_note FROM sessions WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?
            .flatten())
    }

    /// Clears the notice owed to session `id`, and the note kept with it, if its count is still
    /// `seen`, the one a launch read before it delivered the notice: a change marked since stays
    /// owed.
    pub fn clear_owner_notice(&self, id: &str, seen: i64) -> Result<()> {
        self.lock().execute(
            "UPDATE sessions SET owner_notice_pending = 0, owner_notice_note = NULL
             WHERE id = ?1 AND owner_notice_pending = ?2",
            params![id, seen],
        )?;
        Ok(())
    }

    pub fn set_session_pinned(&self, id: &str, pinned: bool) -> Result<()> {
        self.lock().execute(
            "UPDATE sessions SET pinned = ?2 WHERE id = ?1",
            params![id, pinned],
        )?;
        Ok(())
    }

    /// Replaces the session's model and effort overrides, each as `Session::model` states it: absent
    /// inherits, `Some(None)` leaves the choice to the agent, a value pins it. Nothing else writes
    /// these columns.
    pub fn set_session_launch_overrides(
        &self,
        id: &str,
        model: &Option<Option<String>>,
        effort: &Option<Option<String>>,
    ) -> Result<()> {
        self.write_session_launch_overrides(id, model, effort, false, None)
            .map(drop)
    }

    /// Records a session's model choice only while the user has not archived it. With `boundary`,
    /// a model change's observation boundary (`record_collector::ObservedBoundary`), it also stores
    /// that and clears the observed model and effort, in the same write.
    pub(crate) fn set_session_launch_overrides_unless_archived(
        &self,
        id: &str,
        model: &Option<Option<String>>,
        effort: &Option<Option<String>>,
        boundary: Option<&str>,
    ) -> Result<bool> {
        self.write_session_launch_overrides(id, model, effort, true, boundary)
    }

    fn write_session_launch_overrides(
        &self,
        id: &str,
        model: &Option<Option<String>>,
        effort: &Option<Option<String>>,
        unless_archived: bool,
        boundary: Option<&str>,
    ) -> Result<bool> {
        Ok(self.lock().execute(
            "UPDATE sessions SET model = ?2, effort = ?3, model_override = ?4, effort_override = ?5,
                observed_boundary = COALESCE(?7, observed_boundary),
                observed_model = CASE WHEN ?7 IS NULL THEN observed_model ELSE NULL END,
                observed_effort = CASE WHEN ?7 IS NULL THEN observed_effort ELSE NULL END
             WHERE id = ?1 AND (?6 = 0 OR status != 'archived')",
            params![
                id,
                model.as_ref().and_then(|value| value.as_deref()),
                effort.as_ref().and_then(|value| value.as_deref()),
                model.is_some(),
                effort.is_some(),
                unless_archived,
                boundary,
            ],
        )? > 0)
    }

    /// Every session's observation boundary that is set, by session id: the collector's private
    /// record of what the agent's records held when a model change last restarted the session
    /// (`record_collector::ObservedBoundary`), retained with this conversation across daemon
    /// restarts and archives. It is never part of a session's public record or a configuration.
    /// One query for a whole collector pass.
    pub(crate) fn observed_boundaries(&self) -> Result<std::collections::HashMap<String, String>> {
        let conn = self.lock();
        let mut statement = conn.prepare(
            "SELECT id, observed_boundary FROM sessions WHERE observed_boundary IS NOT NULL",
        )?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// One session's observation boundary (see [`Self::observed_boundaries`]).
    #[cfg(test)]
    pub(crate) fn observed_boundary(&self, id: &str) -> Result<Option<String>> {
        Ok(self
            .lock()
            .query_row(
                "SELECT observed_boundary FROM sessions WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?
            .flatten())
    }

    #[cfg(test)]
    pub(crate) fn reset_observed_model(&self, id: &str, boundary: &str) -> Result<()> {
        self.lock().execute(
            "UPDATE sessions SET observed_boundary = ?2,
            observed_model = NULL, observed_effort = NULL WHERE id = ?1",
            params![id, boundary],
        )?;
        Ok(())
    }

    /// Sets the colour of a console session. Like `pinned`, it is the user's own statement, which
    /// no other write touches.
    pub fn set_session_colour(&self, id: &str, colour: ConsoleSessionColour) -> Result<()> {
        self.lock().execute(
            "UPDATE sessions SET colour = ?2 WHERE id = ?1 AND role = ?3",
            params![id, enum_to_text(&colour), enum_to_text(&Role::Console)],
        )?;
        Ok(())
    }

    pub fn delete_session(&self, id: &str) -> Result<()> {
        self.lock()
            .execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Deletes the session only while it is archived and returns the record that was removed.
    pub fn delete_session_if_archived(&self, id: &str) -> Result<Option<Session>> {
        Ok(self
            .lock()
            .query_row(
                &format!("DELETE FROM sessions WHERE id = ?1 AND status = ?2 RETURNING {SESSION_COLUMNS}"),
                params![id, enum_to_text(&SessionStatus::Archived)],
                read_session,
            )
            .optional()?)
    }

    /// Deletes the session only while it is unused: it has had no turn, has no session bound to it
    /// and is either bound to no owner or was opened by the user (one an agent started was
    /// dispatched by its owner, which expects it to exist, so it is archived instead), whatever
    /// its status. The returned identity comes from the same statement as the delete, so
    /// native-record cleanup cannot use an earlier hook snapshot.
    pub fn delete_session_if_unused(&self, id: &str) -> Result<Option<Session>> {
        Ok(self
            .lock()
            .query_row(
                &format!(
                    "DELETE FROM sessions
                     WHERE id = ?1 AND has_conversation = 0
                       AND (bound_to IS NULL OR origin = ?2)
                       AND NOT EXISTS (SELECT 1 FROM sessions WHERE bound_to = ?1)
                     RETURNING {SESSION_COLUMNS}"
                ),
                params![id, enum_to_text(&Origin::User)],
                read_session,
            )
            .optional()?)
    }

    pub fn get_session(&self, id: &str) -> Result<Option<Session>> {
        let conn = self.lock();
        let session = conn
            .query_row(
                &format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1"),
                params![id],
                read_session,
            )
            .optional()?;
        Ok(session)
    }

    /// A session other than `except` that records `agent_session_id` as its agent's conversation
    /// id, live or archived and of any project, if there is one.
    pub fn session_by_agent_session_id(
        &self,
        agent_session_id: &str,
        except: Option<&str>,
    ) -> Result<Option<Session>> {
        let conn = self.lock();
        let session = conn
            .query_row(
                &format!(
                    "SELECT {SESSION_COLUMNS} FROM sessions
                     WHERE agent_session_id = ?1 AND id IS NOT ?2 LIMIT 1"
                ),
                params![agent_session_id, except],
                read_session,
            )
            .optional()?;
        Ok(session)
    }

    pub fn list_sessions(&self) -> Result<Vec<Session>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {SESSION_COLUMNS} FROM sessions ORDER BY started_at"
        ))?;
        let rows = stmt
            .query_map([], read_session)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Moves every session that a previous daemon left mid-flight to `interrupted`. The daemon
    /// owns the agent processes, so none of them survived its exit — whatever the stored status
    /// says. Archived sessions are already dormant on purpose and are left alone.
    pub fn mark_live_sessions_interrupted(&self) -> Result<usize> {
        let changed = self.lock().execute(
            "UPDATE sessions SET status = ?1, ended_at = ?2 WHERE status NOT IN (?1, ?3)",
            params![
                enum_to_text(&SessionStatus::Interrupted),
                crate::protocol::now_millis(),
                enum_to_text(&SessionStatus::Archived),
            ],
        )?;
        Ok(changed)
    }

    // -- branch origins ------------------------------------------------------

    /// Every origin recorded for the repository. A row is checked against its branch and dropped
    /// only when the Git mode lists the branches, so until then it can describe a branch deleted,
    /// or deleted and made again, since: a reader that acts on the rows holds each against the
    /// branch's first reflog entry itself.
    pub fn branch_origins(&self, repository: &str) -> Result<Vec<BranchOrigin>> {
        let conn = self.lock();
        read_branch_origins(&conn, repository)
    }

    /// Applies one reconcile of a repository's origins: removes the rows of `dropped`, writes
    /// `put` over whatever those branches had, and trims the repository to `keep` rows when
    /// anything was written. Returns what the repository holds afterwards.
    ///
    /// One transaction for the lot, and the read of the result inside it: every write here follows
    /// from a snapshot of the same rows, so a second reconcile landing between the delete and the
    /// insert of the first would delete a live branch's row or overwrite a fresh attribution with
    /// a stale one. It is also what keeps a repository with hundreds of branches to one journalled
    /// transaction on the first look at it, rather than one per branch.
    pub fn apply_branch_origins(
        &self,
        repository: &str,
        dropped: &[String],
        put: &[BranchOrigin],
        keep: usize,
    ) -> Result<Vec<BranchOrigin>> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        {
            let mut delete =
                tx.prepare("DELETE FROM branch_origins WHERE repository = ?1 AND branch = ?2")?;
            for branch in dropped {
                delete.execute(params![repository, branch])?;
            }
            let mut insert = tx.prepare(
                "INSERT OR REPLACE INTO branch_origins
                     (repository, branch, created_at, birth_commit, birth_time,
                      responsible_session_id, responsible_since, responsible_title,
                      target_branch)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;
            for origin in put {
                insert.execute(params![
                    repository,
                    origin.branch,
                    origin.created_at,
                    origin.birth_commit,
                    origin.birth_time,
                    origin.responsible_session,
                    origin.responsible_since,
                    origin.responsible_title,
                    origin.target_branch,
                ])?;
            }
        }
        if !put.is_empty() {
            // The oldest past the cap go, by the time their branch was created.
            tx.execute(
                "DELETE FROM branch_origins WHERE repository = ?1 AND branch NOT IN (
                     SELECT branch FROM branch_origins WHERE repository = ?1
                     ORDER BY created_at DESC, branch LIMIT ?2)",
                params![repository, keep as i64],
            )?;
        }
        let rows = read_branch_origins(&tx, repository)?;
        tx.commit()?;
        Ok(rows)
    }

    /// Makes `to`, called `to_title`, the responsible session of each of `branches` whose
    /// responsible session is `from`, as of `since` (milliseconds), and says which moved and which
    /// did not: a branch that has no row, or whose responsible session is another by now, stays as
    /// it is. One transaction for the lot, each branch compared and set in one statement, so of two
    /// handovers racing for a branch only one lands.
    pub fn move_branch_responsibility(
        &self,
        repository: &str,
        branches: &[String],
        from: &str,
        to: &str,
        to_title: &str,
        since: i64,
    ) -> Result<BranchMove> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let mut result = BranchMove::default();
        {
            let mut update = tx.prepare(
                "UPDATE branch_origins SET responsible_session_id = ?4, responsible_since = ?5,
                     responsible_title = ?6
                 WHERE repository = ?1 AND branch = ?2 AND responsible_session_id = ?3",
            )?;
            for branch in branches {
                if update.execute(params![repository, branch, from, to, since, to_title])? > 0 {
                    result.moved.push(branch.clone());
                } else {
                    result.unmoved.push(branch.clone());
                }
            }
        }
        tx.commit()?;
        Ok(result)
    }

    /// Removes every origin recorded for the repository.
    pub fn delete_repository_branch_origins(&self, repository: &str) -> Result<()> {
        self.lock().execute(
            "DELETE FROM branch_origins WHERE repository = ?1",
            params![repository],
        )?;
        Ok(())
    }
}

fn read_branch_origins(conn: &Connection, repository: &str) -> Result<Vec<BranchOrigin>> {
    let mut stmt = conn.prepare(
        "SELECT repository, branch, created_at, birth_commit, birth_time,
                responsible_session_id, responsible_since, responsible_title,
                target_branch
         FROM branch_origins WHERE repository = ?1 ORDER BY branch",
    )?;
    let rows = stmt
        .query_map(params![repository], |row| {
            Ok(BranchOrigin {
                repository: row.get(0)?,
                branch: row.get(1)?,
                created_at: row.get(2)?,
                birth_commit: row.get(3)?,
                birth_time: row.get(4)?,
                responsible_session: row.get(5)?,
                responsible_since: row.get(6)?,
                responsible_title: row.get(7)?,
                target_branch: row.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// The session working on one branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchOrigin {
    pub repository: String,
    /// The branch's name below `refs/heads/`, as a wire path.
    pub branch: String,
    /// When the branch was created, in milliseconds since the epoch.
    pub created_at: i64,
    /// The commit and the second the branch was created at: its first reflog entry, or in a
    /// reftable repository, which has no reflog files, the report's commit and time.
    pub birth_commit: String,
    pub birth_time: i64,
    /// The session working on the branch now, and when it became so (milliseconds since the
    /// epoch).
    pub responsible_session: String,
    pub responsible_since: i64,
    /// The responsible session's title when it became so, so the row outlives that session.
    pub responsible_title: String,
    /// The branch this one was created from, as a wire path; `None` when it was created from
    /// something that is no local branch.
    pub target_branch: Option<String>,
}

/// What [`Store::move_branch_responsibility`] did, by branch name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BranchMove {
    pub moved: Vec<String>,
    pub unmoved: Vec<String>,
}

/// The schema version the file at `conn` is at; 0 for a file never given a schema.
fn user_version(conn: &Connection) -> Result<i64> {
    Ok(conn.pragma_query_value(None, "user_version", |row| row.get(0))?)
}

/// Decides what `Store::open` does with a database file that already exists, before it opens it
/// for use:
///
/// - at [`SCHEMA_VERSION`], or holding no tables at all (an empty file, which `open` initializes):
///   left as it is;
/// - at versions 1 through 3: upgraded in place, adding the account display order (version 2),
///   the collector's private observation boundary (version 3), and delivery squash settings
///   (version 4) where needed;
/// - at an older version without an upgrade step, which includes every database from before
///   versions were recorded: moved aside, so `open` starts a fresh one. The old file is renamed
///   rather than deleted, so a user who needs what was in it still has it on disk; see "Files
///   Octoboard owns" in `docs/product/application-lifecycle.md`. A step from a recorded older
///   version to the current one belongs here, ahead of this case;
/// - at a newer version: refused. It was written by a newer build, whose data moving it aside
///   would lose, and this build cannot read it.
fn settle_existing_file(path: &Path) -> Result<()> {
    {
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        let version = user_version(&conn)
            .with_context(|| format!("reading the schema version of {}", path.display()))?;
        if version == SCHEMA_VERSION {
            return Ok(());
        }
        if (1..=3).contains(&version) {
            let tx = conn.unchecked_transaction()?;
            if version == 1 {
                tx.execute_batch("ALTER TABLE settings ADD COLUMN account_order TEXT")?;
            }
            if version <= 2 {
                tx.execute_batch("ALTER TABLE sessions ADD COLUMN observed_boundary TEXT")?;
            }
            tx.execute_batch(
                "ALTER TABLE settings ADD COLUMN squash_before_delivery INTEGER NOT NULL DEFAULT 0;
                 ALTER TABLE projects ADD COLUMN squash_before_delivery INTEGER",
            )?;
            tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            tx.commit()?;
            return Ok(());
        }
        if version > SCHEMA_VERSION {
            anyhow::bail!(
                "the database at {} is at schema version {version}, newer than the {SCHEMA_VERSION} \
                 this build reads; it was written by a newer build of Octoboard, so use that build \
                 or move the file aside",
                path.display()
            );
        }
        let tables: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'",
                [],
                |row| row.get(0),
            )
            .with_context(|| format!("listing the tables of {}", path.display()))?;
        if tables == 0 {
            return Ok(());
        }
    }

    let moved_aside = path.with_file_name(format!(
        "{}.superseded-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("octoboard.db"),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    ));
    std::fs::rename(path, &moved_aside).with_context(|| {
        format!(
            "moving the outdated database {} aside to {}",
            path.display(),
            moved_aside.display()
        )
    })?;
    tracing::warn!(
        old = %moved_aside.display(),
        new = %path.display(),
        "the database at this path is from before the current schema version and has no step to \
         it; moved it aside and starting a fresh one in its place"
    );
    Ok(())
}

const PROJECT_COLUMNS: &str = "id, console_id, host_id, name, path, default_agent, source,
                               remote_url, claude_account_id, codex_account_id, grok_account_id,
                               claude_account_default, codex_account_default, grok_account_default,
                               claude_session_model, claude_session_effort, codex_session_model,
                               codex_session_effort, grok_session_model, grok_session_effort,
                               trust_consent, pinned, tags, description, development,
                               rebase_before_merge, delivery, revert_gate, squash_before_delivery";

/// What the agent's records gave a session, to be stored if it differs from what is.
pub struct RecordUpdate {
    /// The session as it was read, which says whose records these are.
    pub session: Session,
    /// The context size the records show: `None` keeps what is stored, `Some(None)` clears it
    /// (the records say it is not known now, as after a compaction).
    pub context: Option<Option<crate::protocol::SessionContext>>,
    pub titles: Option<RecordTitles>,
    /// The model and effort the records show, where they show one; an absent value keeps what is
    /// stored. The effort can also be cleared, `Some(None)`, where the records say none is set.
    pub observed_model: Option<String>,
    pub observed_effort: Option<Option<String>>,
    /// The observation boundary the records were read against (see
    /// `record_collector::ObservedBoundary`): the model and effort are not written once a restart
    /// has stored another.
    pub observed_boundary: Option<String>,
}

const SESSION_COLUMNS: &str = "id, agent, agent_session_id, console_id, project_id, host_id,
                               role, origin, title, status, has_conversation, bound_to, colour,
                               ordinal, account_id, config_dir, pinned, started_at, ended_at,
                               lead, terminal_title, model, effort, model_override, effort_override, context,
                               record_title, record_prompt, locked, observed_model, observed_effort";

const CONSOLE_COLUMNS: &str = "id, name, workdir, default_agent,
                               claude_account_id, codex_account_id, grok_account_id,
                               claude_console_session_model, claude_console_session_effort,
                               codex_console_session_model, codex_console_session_effort,
                               grok_console_session_model, grok_console_session_effort, icon,
                               console_session_prompt, created_at, project_session_agent,
                               claude_project_session_model, claude_project_session_effort,
                               codex_project_session_model, codex_project_session_effort,
                               grok_project_session_model, grok_project_session_effort";

/// The colour for a new console session: the entry `roll` selects among the palette entries not
/// in `held` (the other non-archived console sessions of its console), or among the whole palette
/// once every entry is held. `roll` is reduced modulo the number of candidates, so any uniformly
/// random number gives a uniform pick; see `Store::insert_console_session`.
fn assign_console_session_colour(
    held: &[ConsoleSessionColour],
    roll: usize,
) -> ConsoleSessionColour {
    let palette = ConsoleSessionColour::PALETTE;
    let free: Vec<ConsoleSessionColour> = palette
        .iter()
        .copied()
        .filter(|candidate| !held.contains(candidate))
        .collect();
    let candidates: &[ConsoleSessionColour] = if free.is_empty() { &palette } else { &free };
    candidates[roll % candidates.len()]
}

/// Shared by [`Store::insert_session`] and [`Store::insert_console_session`], which differ only in
/// how `colour`, `ordinal` and `title` are decided before this runs.
fn insert_session_row(conn: &Connection, session: &Session) -> Result<()> {
    conn.execute(
        "INSERT INTO sessions
            (id, agent, agent_session_id, console_id, project_id, host_id, role, origin, title,
             status, has_conversation, bound_to, colour, ordinal, account_id, config_dir, pinned,
             started_at, ended_at, lead, terminal_title, model, effort, model_override, effort_override, context,
             record_title, record_prompt, locked, observed_model, observed_effort)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
                 ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31)",
        params![
            session.id,
            enum_to_text(&session.agent),
            session.agent_session_id,
            session.console_id,
            session.project_id,
            session.host_id,
            enum_to_text(&session.role),
            enum_to_text(&session.origin),
            session.title,
            enum_to_text(&session.status),
            session.has_conversation,
            session.bound_to,
            session.colour.as_ref().map(enum_to_text),
            session.ordinal,
            session.account_id,
            session.config_dir,
            session.pinned,
            session.started_at,
            session.ended_at,
            session.lead,
            session.terminal_title,
            session.model.as_ref().and_then(|value| value.as_deref()),
            session.effort.as_ref().and_then(|value| value.as_deref()),
            session.model.is_some(),
            session.effort.is_some(),
            session.context.as_ref().map(serde_json::to_string).transpose()?,
            session.record_titles.named,
            session.record_titles.prompt,
            session.locked,
            session.observed_model,
            session.observed_effort,
        ],
    )?;
    Ok(())
}

fn read_console(row: &Row<'_>) -> rusqlite::Result<Console> {
    Ok(Console {
        id: row.get(0)?,
        name: row.get(1)?,
        workdir: row.get(2)?,
        default_agent: enum_from_row(row, 3)?,
        claude_account_id: row.get(4)?,
        codex_account_id: row.get(5)?,
        grok_account_id: row.get(6)?,
        claude_console_session_model: row.get(7)?,
        claude_console_session_effort: row.get(8)?,
        codex_console_session_model: row.get(9)?,
        codex_console_session_effort: row.get(10)?,
        grok_console_session_model: row.get(11)?,
        grok_console_session_effort: row.get(12)?,
        icon: row.get(13)?,
        console_session_prompt: row.get(14)?,
        created_at: row.get(15)?,
        project_session_agent: enum_from_row_opt::<Agent>(row, 16)?,
        claude_project_session_model: row.get(17)?,
        claude_project_session_effort: row.get(18)?,
        codex_project_session_model: row.get(19)?,
        codex_project_session_effort: row.get(20)?,
        grok_project_session_model: row.get(21)?,
        grok_project_session_effort: row.get(22)?,
    })
}

/// Turns the `accounts_agent_name_unique` index tripping into the protocol's own
/// `account_name_taken` refusal, so an insert or update that reaches this far surfaces the same
/// way as the collision check `coordinator` already runs beforehand — this only fires when the
/// two disagree, which is two requests racing past that check. Any other error is passed through
/// unchanged.
fn account_write_error(err: rusqlite::Error, account: &Account) -> anyhow::Error {
    let is_name_collision = matches!(
        &err,
        rusqlite::Error::SqliteFailure(sqlite_err, _)
            if sqlite_err.code == rusqlite::ErrorCode::ConstraintViolation
    );
    if is_name_collision {
        return CodedError::raised(
            error_code::ACCOUNT_NAME_TAKEN,
            format!(
                "the name `{}` is already used for {}",
                account.name,
                account.agent.label()
            ),
            &[("agent", account.agent.label()), ("name", &account.name)],
        );
    }
    err.into()
}

/// What turning an account's sharing on recorded when it started; see
/// [`Store::account_sharing_start`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharingStart {
    pub stamp: String,
    pub default_dir: String,
    /// Turning sharing on has completed once since it started: what it carried over from the
    /// account's own Codex database is not carried again, since it may have been taken back since.
    pub completed: bool,
}

fn read_account(row: &Row<'_>) -> rusqlite::Result<Account> {
    Ok(Account {
        id: row.get(0)?,
        agent: enum_from_row(row, 1)?,
        name: row.get(2)?,
        config_dir: row.get(3)?,
        share_default_config: row.get(4)?,
        share_default_config_incomplete: row.get(5)?,
        disabled: row.get(6)?,
    })
}

fn read_git_workflow_settings(conn: &Connection) -> Result<GitWorkflowSettings> {
    Ok(conn.query_row(
        "SELECT development, rebase_before_merge, delivery, revert_gate, squash_before_delivery
         FROM settings WHERE id = ?1",
        params![SETTINGS_ID],
        |row| {
            Ok(GitWorkflowSettings {
                development: enum_from_row(row, 0)?,
                squash_before_delivery: row.get(4)?,
                rebase_before_merge: row.get(1)?,
                delivery: enum_from_row(row, 2)?,
                revert_gate: row.get(3)?,
            })
        },
    )?)
}

fn read_project(row: &Row<'_>) -> rusqlite::Result<Project> {
    let id: String = row.get(0)?;
    let tags = tags_from_text(&id, &row.get::<_, String>(22)?);
    Ok(Project {
        id,
        console_id: row.get(1)?,
        host_id: row.get(2)?,
        name: row.get(3)?,
        path: row.get(4)?,
        default_agent: enum_from_row_opt::<Agent>(row, 5)?,
        source: enum_from_row::<ProjectSource>(row, 6)?,
        remote_url: row.get(7)?,
        claude_account_id: project_account(row.get(8)?, row.get(11)?),
        codex_account_id: project_account(row.get(9)?, row.get(12)?),
        grok_account_id: project_account(row.get(10)?, row.get(13)?),
        claude_session_model: row.get(14)?,
        claude_session_effort: row.get(15)?,
        codex_session_model: row.get(16)?,
        codex_session_effort: row.get(17)?,
        grok_session_model: row.get(18)?,
        grok_session_effort: row.get(19)?,
        trust_consent: row.get(20)?,
        pinned: row.get(21)?,
        tags,
        description: row.get(23)?,
        git_workflow: GitWorkflowOverride {
            development: enum_from_row_opt(row, 24)?,
            squash_before_delivery: row.get(28)?,
            rebase_before_merge: row.get(25)?,
            delivery: enum_from_row_opt(row, 26)?,
            revert_gate: row.get(27)?,
        },
    })
}

/// A project's account setting from its id column and its pins-the-default flag. An id wins over
/// the flag, though nothing writes both.
fn project_account(id: Option<String>, pins_default: bool) -> ProjectAccount {
    match id {
        Some(id) => ProjectAccount::Account { id },
        None if pins_default => ProjectAccount::Default,
        None => ProjectAccount::Inherit,
    }
}

fn read_page(row: &Row<'_>) -> rusqlite::Result<Page> {
    Ok(Page {
        id: row.get(0)?,
        console_session_id: row.get(1)?,
        html: row.get(2)?,
        anchor_message_id: row.get(3)?,
        created_at: row.get(4)?,
    })
}

fn read_session(row: &Row<'_>) -> rusqlite::Result<Session> {
    Ok(Session {
        id: row.get(0)?,
        agent: enum_from_row::<Agent>(row, 1)?,
        agent_session_id: row.get(2)?,
        console_id: row.get(3)?,
        project_id: row.get(4)?,
        host_id: row.get(5)?,
        role: enum_from_row::<Role>(row, 6)?,
        origin: enum_from_row::<Origin>(row, 7)?,
        title: row.get(8)?,
        status: enum_from_row::<SessionStatus>(row, 9)?,
        background_tasks: Vec::new(),
        switching_account: false,
        default_effort: None,
        orchestration_role: None,
        has_conversation: row.get(10)?,
        bound_to: row.get(11)?,
        colour: enum_from_row_opt::<ConsoleSessionColour>(row, 12)?,
        ordinal: row.get(13)?,
        account_id: row.get(14)?,
        config_dir: row.get(15)?,
        pinned: row.get(16)?,
        locked: row.get(28)?,
        observed_model: row.get(29)?,
        observed_effort: row.get(30)?,
        started_at: row.get(17)?,
        ended_at: row.get(18)?,
        lead: row.get(19)?,
        terminal_title: row.get(20)?,
        record_titles: RecordTitles {
            named: row.get(26)?,
            prompt: row.get(27)?,
        },
        context: row
            .get::<_, Option<String>>(25)?
            .and_then(|text| serde_json::from_str(&text).ok()),
        model: row.get::<_, bool>(23)?.then(|| row.get(21)).transpose()?,
        effort: row.get::<_, bool>(24)?.then(|| row.get(22)).transpose()?,
    })
}

/// The text form of a protocol enum, taken from its serde representation so the stored value and
/// the wire value can never drift apart.
fn enum_to_text<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .expect("protocol enums serialize to a JSON string")
}

fn added_agents_to_text(agents: &[Agent]) -> String {
    serde_json::to_string(agents).expect("a list of agents serializes to JSON")
}

fn read_account_order(
    conn: &Connection,
) -> Result<std::collections::BTreeMap<String, Vec<Option<String>>>> {
    let text: Option<String> = conn.query_row(
        "SELECT account_order FROM settings WHERE id = ?1",
        params![SETTINGS_ID],
        |row| row.get(0),
    )?;
    Ok(text
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default())
}

/// The stored default account names; a value that is not a JSON object of strings reads as none.
fn read_default_account_names(
    conn: &Connection,
) -> Result<std::collections::BTreeMap<String, String>> {
    let text: Option<String> = conn.query_row(
        "SELECT default_account_names FROM settings WHERE id = ?1",
        params![SETTINGS_ID],
        |row| row.get(0),
    )?;
    Ok(text
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default())
}

/// The stored Claude compatibility choices; a value that is not a JSON object of booleans reads as
/// none.
fn read_claude_compatibility_choices(
    conn: &Connection,
) -> Result<std::collections::BTreeMap<String, bool>> {
    let text: Option<String> = conn.query_row(
        "SELECT claude_compatibility FROM settings WHERE id = ?1",
        params![SETTINGS_ID],
        |row| row.get(0),
    )?;
    Ok(text
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default())
}

/// The Claude compatibility setting in effect for `agent` given the user's `choices` and the
/// added agents (`None` while undecided, which counts as Claude Code not being among them).
fn effective_claude_compatibility(
    agent: Agent,
    choices: &std::collections::BTreeMap<String, bool>,
    added: Option<&[Agent]>,
) -> bool {
    if !agent.has_claude_compatibility() {
        return false;
    }
    choices
        .get(agent.wire_name())
        .copied()
        .unwrap_or_else(|| match agent {
            Agent::Codex => added.is_some_and(|added| added.contains(&Agent::Claude)),
            _ => true,
        })
}

/// The stored agents whose default account is disabled; a value that is not a JSON array of strings
/// reads as none.
fn read_disabled_default_accounts(conn: &Connection) -> Result<std::collections::BTreeSet<String>> {
    let text: Option<String> = conn.query_row(
        "SELECT disabled_default_accounts FROM settings WHERE id = ?1",
        params![SETTINGS_ID],
        |row| row.get(0),
    )?;
    Ok(text
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default())
}

/// A malformed value reads as an undecided list, with a warning, so the next fill replaces it. The
/// daemon is the only writer, so this can only mean a hand-edited database, or one written by a later version.
fn read_added_agents(conn: &Connection) -> Result<Option<Vec<Agent>>> {
    let text: Option<String> = conn.query_row(
        "SELECT added_agents FROM settings WHERE id = ?1",
        params![SETTINGS_ID],
        |row| row.get(0),
    )?;
    Ok(text.and_then(|text| match serde_json::from_str(&text) {
        Ok(agents) => Some(agents),
        Err(err) => {
            tracing::warn!(%err, "the added agents are not a JSON array of agents; reading the list as undecided");
            None
        }
    }))
}

fn write_added_agents(conn: &Connection, agents: &[Agent]) -> Result<()> {
    conn.execute(
        "UPDATE settings SET added_agents = ?2 WHERE id = ?1",
        params![SETTINGS_ID, added_agents_to_text(agents)],
    )?;
    Ok(())
}

fn tags_to_text(tags: &[String]) -> String {
    serde_json::to_string(tags).expect("a list of strings serializes to JSON")
}

/// A malformed value reads as no tags, with a warning, rather than failing the whole project read.
/// The daemon is the only writer, so this can only mean a hand-edited database, or one written by a later version.
fn tags_from_text(project_id: &str, text: &str) -> Vec<String> {
    serde_json::from_str(text).unwrap_or_else(|err| {
        tracing::warn!(project = %project_id, %err, "a project's tags are not a JSON array of strings; reading it as untagged");
        Vec::new()
    })
}

fn enum_from_row<T: DeserializeOwned>(row: &Row<'_>, index: usize) -> rusqlite::Result<T> {
    let text: String = row.get(index)?;
    enum_from_text(&text).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, err.into())
    })
}

fn enum_from_row_opt<T: DeserializeOwned>(
    row: &Row<'_>,
    index: usize,
) -> rusqlite::Result<Option<T>> {
    let text: Option<String> = row.get(index)?;
    match text {
        None => Ok(None),
        Some(text) => enum_from_text(&text).map(Some).map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(
                index,
                rusqlite::types::Type::Text,
                err.into(),
            )
        }),
    }
}

fn enum_from_text<T: DeserializeOwned>(text: &str) -> Result<T, serde_json::Error> {
    serde_json::from_value(serde_json::Value::String(text.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_workflow::{self, Delivery, Development};
    use crate::test_support::ScratchFile;

    fn temp_db(name: &str) -> ScratchFile {
        ScratchFile::new(&format!("store-{name}"), "octoboard.db")
    }

    fn console(claude: Option<&str>, codex: Option<&str>, grok: Option<&str>) -> Console {
        Console {
            id: "console-1".to_string(),
            name: "Console".to_string(),
            workdir: "/tmp/console-1".to_string(),
            default_agent: Agent::Claude,
            claude_account_id: claude.map(str::to_string),
            codex_account_id: codex.map(str::to_string),
            grok_account_id: grok.map(str::to_string),
            claude_console_session_model: None,
            claude_console_session_effort: None,
            codex_console_session_model: None,
            codex_console_session_effort: None,
            grok_console_session_model: None,
            grok_console_session_effort: None,
            icon: None,
            console_session_prompt: None,
            created_at: 0,
            project_session_agent: None,
            claude_project_session_model: None,
            claude_project_session_effort: None,
            codex_project_session_model: None,
            codex_project_session_effort: None,
            grok_project_session_model: None,
            grok_project_session_effort: None,
        }
    }

    fn account(id: &str, agent: Agent, config_dir: &str) -> Account {
        Account {
            id: id.to_string(),
            agent,
            name: format!("{id}-name"),
            config_dir: config_dir.to_string(),
            share_default_config: false,
            share_default_config_incomplete: false,
            disabled: false,
        }
    }

    fn session(config_dir: Option<&str>) -> Session {
        Session {
            id: "session-1".to_string(),
            agent: Agent::Claude,
            agent_session_id: None,
            console_id: "console-1".to_string(),
            project_id: None,
            host_id: LOCAL_HOST_ID.to_string(),
            role: Role::Console,
            origin: Origin::User,
            title: "Hub".to_string(),
            status: SessionStatus::Idle,
            terminal_title: None,
            record_titles: Default::default(),
            background_tasks: Vec::new(),
            switching_account: false,
            default_effort: None,
            orchestration_role: None,
            context: None,
            observed_model: None,
            observed_effort: None,
            model: None,
            effort: None,
            has_conversation: false,
            bound_to: None,
            lead: false,
            colour: None,
            ordinal: None,
            account_id: None,
            config_dir: config_dir.map(str::to_string),
            pinned: false,
            locked: false,
            started_at: 0,
            ended_at: None,
        }
    }

    /// The names of the files in `path`'s directory that are databases moved aside.
    fn moved_aside(path: &Path) -> Vec<String> {
        path.parent()
            .unwrap()
            .read_dir()
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("octoboard.db.superseded-"))
            .collect()
    }

    /// A new database is stamped with the schema version, and reopening it keeps its data and
    /// moves nothing aside.
    #[test]
    fn a_new_database_is_stamped_and_reopening_it_keeps_its_data() {
        let path = temp_db("fresh-version");
        {
            let store = Store::open(&path).expect("store");
            assert_eq!(user_version(&store.lock()).unwrap(), SCHEMA_VERSION);
            store
                .insert_console(&console(None, None, None))
                .expect("console");
        }
        let store = Store::open(&path).expect("reopened");
        assert_eq!(user_version(&store.lock()).unwrap(), SCHEMA_VERSION);
        assert!(store.get_console("console-1").unwrap().is_some());
        assert!(moved_aside(&path).is_empty());
    }

    #[test]
    fn delivery_squash_upgrade_preserves_version_three_settings_and_projects() {
        let path = temp_db("delivery-squash-upgrade");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                &SCHEMA
                    .replace("    squash_before_delivery INTEGER,\n", "")
                    .replace(
                        "    squash_before_delivery INTEGER NOT NULL DEFAULT 0,\n",
                        "",
                    ),
            )
            .unwrap();
            conn.pragma_update(None, "user_version", 3).unwrap();
            conn.execute(
                "INSERT INTO settings (id, rebase_before_merge, delivery) VALUES (?1, 0, 'merge')",
                params![SETTINGS_ID],
            )
            .unwrap();
            conn.execute_batch(
                "INSERT INTO consoles (id, name, workdir, default_agent, created_at)
                 VALUES ('kept-console', 'Kept', '/kept', 'claude', 1);
                 INSERT INTO projects (id, console_id, host_id, name, path, source,
                                       rebase_before_merge, delivery)
                 VALUES ('kept-project', 'kept-console', 'local', 'Kept', '/kept', 'local',
                         1, 'pull_request')",
            )
            .unwrap();
        }
        let store = Store::open(&path).unwrap();
        let global = store.git_workflow_settings().unwrap();
        assert!(!global.squash_before_delivery);
        assert!(!global.rebase_before_merge);
        assert_eq!(global.delivery, Delivery::Merge);
        let project = store.get_project("kept-project").unwrap().unwrap();
        assert_eq!(project.git_workflow.squash_before_delivery, None);
        assert_eq!(project.git_workflow.rebase_before_merge, Some(true));
        assert_eq!(project.git_workflow.delivery, Some(Delivery::PullRequest));
        assert_eq!(user_version(&store.lock()).unwrap(), SCHEMA_VERSION);
        assert!(moved_aside(&path).is_empty());
    }

    #[test]
    fn observation_boundary_upgrade_preserves_version_two_sessions() {
        let path = temp_db("model-boundary-upgrade");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                &SCHEMA
                    .replace("    observed_boundary TEXT,\n", "")
                    .replace("    squash_before_delivery INTEGER,\n", "")
                    .replace(
                        "    squash_before_delivery INTEGER NOT NULL DEFAULT 0,\n",
                        "",
                    ),
            )
            .unwrap();
            conn.pragma_update(None, "user_version", 2).unwrap();
            let store = Store {
                conn: Mutex::new(conn),
            };
            store.insert_console(&console(None, None, None)).unwrap();
            store.insert_session(&session(None)).unwrap();
        }
        let store = Store::open(&path).unwrap();
        let source = session(None);
        assert!(store.get_session(&source.id).unwrap().is_some());
        assert!(store.observed_boundary(&source.id).unwrap().is_none());
        assert_eq!(user_version(&store.lock()).unwrap(), SCHEMA_VERSION);
        assert!(moved_aside(&path).is_empty());
    }

    #[test]
    fn account_order_upgrade_keeps_existing_data() {
        let path = temp_db("account-order-upgrade");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                &SCHEMA
                    .replace("    account_order          TEXT,\n", "")
                    .replace("    squash_before_delivery INTEGER,\n", "")
                    .replace(
                        "    squash_before_delivery INTEGER NOT NULL DEFAULT 0,\n",
                        "",
                    )
                    .replace("    observed_boundary TEXT,\n", ""),
            )
            .unwrap();
            conn.pragma_update(None, "user_version", 1).unwrap();
            conn.execute(
                "INSERT INTO settings (id, auto_select_account) VALUES (?1, 1)",
                params![SETTINGS_ID],
            )
            .unwrap();
            conn.execute_batch(
                "INSERT INTO accounts (id, agent, name, config_dir, created_at)
                 VALUES ('kept', 'claude', 'Work', '/accounts/work', 1)",
            )
            .unwrap();
        }
        let store = Store::open(&path).unwrap();
        assert_eq!(store.list_accounts().unwrap()[0].id, "kept");
        assert!(store.get_settings().unwrap().auto_select_account);
        assert!(store.get_settings().unwrap().account_order.is_empty());
        assert_eq!(user_version(&store.lock()).unwrap(), SCHEMA_VERSION);
        assert!(moved_aside(&path).is_empty());
    }

    #[test]
    fn account_order_validates_current_membership_and_persists_per_agent() {
        let path = temp_db("account-order");
        let store = Store::open(&path).unwrap();
        assert!(store.get_settings().unwrap().account_order.is_empty());
        for (id, agent) in [
            ("one", Agent::Claude),
            ("two", Agent::Claude),
            ("other", Agent::Codex),
        ] {
            store
                .insert_account(&account(id, agent, &format!("/accounts/{id}")))
                .unwrap();
        }
        let stored_ids = || {
            store
                .list_accounts()
                .unwrap()
                .into_iter()
                .map(|account| account.id)
                .collect::<Vec<_>>()
        };
        let original_ids = stored_ids();
        let order = vec![Some("two".to_string()), None, Some("one".to_string())];
        assert!(store.reorder_accounts(Agent::Claude, &order).unwrap());
        assert!(!store.reorder_accounts(Agent::Claude, &order).unwrap());
        assert!(store
            .reorder_accounts(Agent::Codex, &[None, Some("other".to_string())])
            .unwrap());
        assert_eq!(stored_ids(), original_ids);
        let before = store.get_settings().unwrap().account_order;
        for invalid in [
            vec![None, Some("one".to_string())],
            vec![Some("one".to_string()), Some("two".to_string())],
            vec![None, None, Some("one".to_string())],
            vec![None, Some("one".to_string()), Some("one".to_string())],
            vec![None, Some("one".to_string()), Some("unknown".to_string())],
            vec![None, Some("one".to_string()), Some("other".to_string())],
            vec![],
        ] {
            let err = store.reorder_accounts(Agent::Claude, &invalid).unwrap_err();
            assert_eq!(
                crate::protocol::error_parts(&err).0,
                error_code::ACCOUNT_ORDER_INVALID
            );
            assert_eq!(store.get_settings().unwrap().account_order, before);
        }
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(store.get_settings().unwrap().account_order, before);
        store.delete_account("one").unwrap();
        assert!(store.reorder_accounts(Agent::Claude, &order).is_err());
        assert!(store
            .reorder_accounts(Agent::Claude, &[Some("two".to_string()), None])
            .unwrap());
        store
            .insert_account(&account("new", Agent::Claude, "/accounts/new"))
            .unwrap();
        assert!(store
            .reorder_accounts(Agent::Claude, &[Some("two".to_string()), None])
            .is_err());
        assert_eq!(
            store.get_settings().unwrap().account_order["codex"],
            before["codex"]
        );
    }

    /// A database from before versions were recorded (version 0, with tables in it) is moved
    /// aside rather than opened or deleted, and the daemon comes up on a fresh one at the original
    /// path.
    #[test]
    fn a_database_without_a_version_is_moved_aside_and_a_fresh_one_opened() {
        let path = temp_db("unversioned");
        {
            let conn = Connection::open(&path).expect("old database");
            conn.execute_batch(
                "CREATE TABLE consoles (id TEXT PRIMARY KEY, name TEXT NOT NULL);
                 INSERT INTO consoles VALUES ('console-1', 'Old');",
            )
            .expect("old schema");
        }

        let store = Store::open(&path).expect("opens a fresh database in its place");
        assert_eq!(user_version(&store.lock()).unwrap(), SCHEMA_VERSION);
        assert!(store.get_console("console-1").unwrap().is_none());
        assert_eq!(
            moved_aside(&path).len(),
            1,
            "the old file is kept, not deleted"
        );
        let kept = path.with_file_name(&moved_aside(&path)[0]);
        let old = Connection::open(&kept).expect("moved-aside file");
        let name: String = old
            .query_row(
                "SELECT name FROM consoles WHERE id = 'console-1'",
                [],
                |row| row.get(0),
            )
            .expect("the old row is still there");
        assert_eq!(name, "Old");
    }

    /// Until the user chooses, Grok Build's Claude compatibility is on and Codex's follows whether
    /// Claude Code is an added agent; a choice is stored per agent and wins from then on; Claude
    /// Code never has the setting.
    #[test]
    fn the_claude_compatibility_follows_added_agents_until_chosen_and_round_trips_per_agent() {
        let path = temp_db("claude-compatibility");
        let store = Store::open(&path).expect("store");
        let stored = |store: &Store| -> Option<String> {
            store
                .lock()
                .query_row(
                    "SELECT claude_compatibility FROM settings WHERE id = ?1",
                    params![SETTINGS_ID],
                    |row| row.get(0),
                )
                .unwrap()
        };
        let on = |store: &Store, agent| store.claude_compatibility_of(agent).unwrap();
        assert!(!on(&store, Agent::Codex));
        assert!(on(&store, Agent::Grok));
        assert!(!on(&store, Agent::Claude));

        store
            .set_added_agents_if_undecided(&[Agent::Codex])
            .unwrap();
        assert!(!on(&store, Agent::Codex));
        store.add_added_agent(Agent::Claude).unwrap();
        assert!(on(&store, Agent::Codex));
        assert_eq!(stored(&store), None);
        assert!(store.claude_compatibility_choices().unwrap().is_empty());

        assert!(store.set_claude_compatibility(Agent::Codex, false).unwrap());
        assert!(!store.set_claude_compatibility(Agent::Codex, false).unwrap());
        assert_eq!(stored(&store).as_deref(), Some(r#"{"codex":false}"#));
        assert!(!on(&store, Agent::Codex));
        assert!(on(&store, Agent::Grok));
        assert_eq!(
            store.get_settings().unwrap().claude_compatibility,
            [("codex".to_string(), false), ("grok".to_string(), true)].into()
        );

        // A choice outlives the added agents, whichever way it went.
        assert!(store.set_claude_compatibility(Agent::Codex, true).unwrap());
        store.remove_added_agent(Agent::Claude).unwrap();
        assert!(on(&store, Agent::Codex));
        assert!(store.set_claude_compatibility(Agent::Grok, false).unwrap());
        assert!(!on(&store, Agent::Grok));
    }

    /// A file that is not a SQLite database makes `open` fail and is left byte for byte as it was.
    #[test]
    fn a_file_that_is_not_a_database_is_refused_and_left_alone() {
        let path = temp_db("not-a-database");
        let bytes = b"this is not a sqlite database, only text padded out to more than a header's worth of bytes"
            .to_vec();
        std::fs::write(&path, &bytes).expect("file");

        let error = Store::open(&path).err().expect("refused");
        assert!(
            format!("{error:#}").contains("reading the schema version of"),
            "{error:#}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert!(moved_aside(&path).is_empty());
    }

    /// An existing file with no tables is initialized in place, not moved aside.
    #[test]
    fn an_empty_database_file_is_initialized_in_place() {
        let path = temp_db("empty-file");
        drop(Connection::open(&path).expect("empty database"));
        assert!(path.exists());

        let store = Store::open(&path).expect("initialized");
        assert_eq!(user_version(&store.lock()).unwrap(), SCHEMA_VERSION);
        assert!(moved_aside(&path).is_empty());
    }

    /// A database from a newer build is refused and left where it is: moving it aside would lose
    /// what that build wrote.
    #[test]
    fn a_database_from_a_newer_build_is_refused_and_left_alone() {
        let path = temp_db("newer-version");
        {
            let conn = Connection::open(&path).expect("database");
            conn.execute_batch("CREATE TABLE consoles (id TEXT PRIMARY KEY);")
                .expect("schema");
            conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
                .expect("version");
        }

        let error = Store::open(&path).err().expect("refused");
        assert!(error.to_string().contains("newer"), "{error}");
        assert!(path.exists());
        assert!(moved_aside(&path).is_empty());
    }

    /// A session without overrides inherits. Overrides, including explicit nulls, survive
    /// unrelated writes, account changes and opening the database again.
    #[test]
    fn session_launch_overrides_survive_reopening() {
        let path = temp_db("session-launch-overrides");
        {
            let store = Store::open(&path).unwrap();
            store.insert_console(&console(None, None, None)).unwrap();
            store.insert_session(&session(None)).unwrap();
        }
        let store = Store::open(&path).unwrap();
        let kept = store.get_session("session-1").unwrap().unwrap();
        assert_eq!((kept.model, kept.effort), (None, None));
        let cases = [
            (Some(Some("opus".to_string())), Some(None)),
            (Some(None), Some(Some("high".to_string()))),
            (None, Some(Some("medium".to_string()))),
            (Some(Some("sonnet".to_string())), None),
        ];
        for (index, (model, effort)) in cases.iter().enumerate() {
            let mut opened = session(None);
            opened.id = format!("override-{index}");
            opened.model = model.clone();
            opened.effort = effort.clone();
            let mut stored = store.insert_console_session(opened, None).unwrap();
            stored.status = SessionStatus::Archived;
            stored.model = None;
            stored.effort = None;
            store.update_session(&stored).unwrap();
            store
                .set_session_account(&stored.id, Some("other"), Some("/other"))
                .unwrap();
        }
        drop(store);
        let store = Store::open(&path).unwrap();
        for (index, expected) in cases.into_iter().enumerate() {
            let stored = store
                .get_session(&format!("override-{index}"))
                .unwrap()
                .unwrap();
            assert_eq!((stored.model, stored.effort), expected);
            assert_eq!(stored.status, SessionStatus::Archived);
            assert_eq!(stored.config_dir.as_deref(), Some("/other"));
        }
    }

    /// A console's console session model and effort round-trip through update, insert and a
    /// reopening, each of the six independent.
    #[test]
    fn the_console_session_launch_columns_round_trip() {
        let path = temp_db("console-session-launch-columns");
        {
            let store = Store::open(&path).expect("store");
            store
                .insert_console(&console(None, None, None))
                .expect("console");
        }

        let store = Store::open(&path).expect("reopened");
        let mut kept = store.get_console("console-1").unwrap().expect("kept");
        assert_eq!(kept.claude_console_session_model, None);
        // Six distinct values, so a pair of columns swapped between the SELECT list and the
        // positional reads fails here.
        kept.claude_console_session_model = Some("claude-model".to_string());
        kept.claude_console_session_effort = Some("claude-effort".to_string());
        kept.codex_console_session_model = Some("codex-model".to_string());
        kept.codex_console_session_effort = Some("codex-effort".to_string());
        kept.grok_console_session_model = Some("grok-model".to_string());
        kept.grok_console_session_effort = Some("grok-effort".to_string());
        store.update_console(&kept).expect("writes the columns");
        let reread = store.get_console("console-1").unwrap().expect("kept");
        assert_eq!(
            reread.claude_console_session_model.as_deref(),
            Some("claude-model")
        );
        assert_eq!(
            reread.claude_console_session_effort.as_deref(),
            Some("claude-effort")
        );
        assert_eq!(
            reread.codex_console_session_model.as_deref(),
            Some("codex-model")
        );
        assert_eq!(
            reread.codex_console_session_effort.as_deref(),
            Some("codex-effort")
        );
        assert_eq!(
            reread.grok_console_session_model.as_deref(),
            Some("grok-model")
        );
        assert_eq!(
            reread.grok_console_session_effort.as_deref(),
            Some("grok-effort")
        );

        // A console inserted with all six set reads them back the same way.
        let mut inserted = console(None, None, None);
        inserted.id = "console-2".to_string();
        inserted.workdir = "/tmp/console-2".to_string();
        inserted.claude_console_session_model = kept.claude_console_session_model.clone();
        inserted.claude_console_session_effort = kept.claude_console_session_effort.clone();
        inserted.codex_console_session_model = kept.codex_console_session_model.clone();
        inserted.codex_console_session_effort = kept.codex_console_session_effort.clone();
        inserted.grok_console_session_model = kept.grok_console_session_model.clone();
        inserted.grok_console_session_effort = kept.grok_console_session_effort.clone();
        store.insert_console(&inserted).expect("insert");
        let listed = store.get_console("console-2").unwrap().expect("listed");
        assert_eq!(
            [
                listed.claude_console_session_model,
                listed.claude_console_session_effort,
                listed.codex_console_session_model,
                listed.codex_console_session_effort,
                listed.grok_console_session_model,
                listed.grok_console_session_effort,
            ]
            .map(|field| field.expect("set")),
            [
                "claude-model",
                "claude-effort",
                "codex-model",
                "codex-effort",
                "grok-model",
                "grok-effort",
            ]
        );

        // Reopened once more, the same values are read back from the stored columns.
        drop(store);
        let store = Store::open(&path).expect("reopened again");
        let again = store.get_console("console-1").unwrap().expect("kept");
        assert_eq!(
            again.codex_console_session_effort.as_deref(),
            Some("codex-effort")
        );
    }

    /// A console follows `default_agent` until it names a project session agent, which then
    /// round-trips.
    #[test]
    fn the_project_session_agent_round_trips() {
        let path = temp_db("project-session-agent-column");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        let mut kept = store.get_console("console-1").unwrap().expect("kept");
        assert_eq!(kept.project_session_agent, None);
        kept.project_session_agent = Some(Agent::Grok);
        store.update_console(&kept).expect("writes the column");
        let reread = store.get_console("console-1").unwrap().expect("kept");
        assert_eq!(reread.project_session_agent, Some(Agent::Grok));
    }

    /// A console's project session model and effort start unset and round-trip, each of the six
    /// independent.
    #[test]
    fn the_console_project_session_launch_columns_round_trip() {
        let path = temp_db("console-project-session-launch-columns");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        let mut kept = store.get_console("console-1").unwrap().expect("kept");
        assert_eq!(kept.claude_project_session_model, None);
        assert_eq!(kept.grok_project_session_effort, None);
        // Six distinct values, so a pair of columns swapped between the SELECT list and the
        // positional reads fails here.
        kept.claude_project_session_model = Some("claude-model".to_string());
        kept.claude_project_session_effort = Some("claude-effort".to_string());
        kept.codex_project_session_model = Some("codex-model".to_string());
        kept.codex_project_session_effort = Some("codex-effort".to_string());
        kept.grok_project_session_model = Some("grok-model".to_string());
        kept.grok_project_session_effort = Some("grok-effort".to_string());
        store.update_console(&kept).expect("writes the columns");
        let reread = store.get_console("console-1").unwrap().expect("kept");
        assert_eq!(
            reread.claude_project_session_model.as_deref(),
            Some("claude-model")
        );
        assert_eq!(
            reread.claude_project_session_effort.as_deref(),
            Some("claude-effort")
        );
        assert_eq!(
            reread.codex_project_session_model.as_deref(),
            Some("codex-model")
        );
        assert_eq!(
            reread.codex_project_session_effort.as_deref(),
            Some("codex-effort")
        );
        assert_eq!(
            reread.grok_project_session_model.as_deref(),
            Some("grok-model")
        );
        assert_eq!(
            reread.grok_project_session_effort.as_deref(),
            Some("grok-effort")
        );
    }

    /// A console carries no console session instructions of the user's own until it is given
    /// some, which then round-trip.
    #[test]
    fn the_console_session_prompt_round_trips() {
        let path = temp_db("console-session-prompt-column");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        let mut kept = store.get_console("console-1").unwrap().expect("kept");
        assert_eq!(kept.console_session_prompt, None);
        kept.console_session_prompt = Some("Be brief.".to_string());
        store.update_console(&kept).expect("writes the column");
        let reread = store.get_console("console-1").unwrap().expect("kept");
        assert_eq!(reread.console_session_prompt.as_deref(), Some("Be brief."));
    }

    /// The three states of a project's account for an agent round-trip through insert, update and
    /// read, each agent's independent, and the id column never holds anything but an account id.
    #[test]
    fn a_projects_three_account_states_round_trip() {
        let path = temp_db("project-account-states");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        let mut stored = project(false);
        stored.claude_account_id = named("claude-acct");
        stored.codex_account_id = ProjectAccount::Default;
        store.insert_project(&stored).expect("insert");
        let read = || store.get_project("project-1").unwrap().unwrap();
        let states = |project: &Project| {
            [
                project.claude_account_id.clone(),
                project.codex_account_id.clone(),
                project.grok_account_id.clone(),
            ]
        };
        assert_eq!(
            states(&read()),
            [
                named("claude-acct"),
                ProjectAccount::Default,
                ProjectAccount::Inherit
            ]
        );

        // Every state can be moved to every other by an update.
        let all = [
            ProjectAccount::Inherit,
            ProjectAccount::Default,
            named("claude-acct"),
        ];
        for from in &all {
            for to in &all {
                stored.grok_account_id = from.clone();
                store.update_project(&stored).expect("update");
                assert_eq!(read().grok_account_id, *from);
                stored.grok_account_id = to.clone();
                store.update_project(&stored).expect("update");
                assert_eq!(read().grok_account_id, *to);
            }
        }

        // The default state stores no id at all, only its flag.
        stored.grok_account_id = ProjectAccount::Default;
        store.update_project(&stored).expect("update");
        let (id, flag): (Option<String>, bool) = store
            .lock()
            .query_row(
                "SELECT grok_account_id, grok_account_default FROM projects WHERE id = 'project-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((id, flag), (None, true));
    }

    /// A project's own accounts, models and efforts start unset; all nine are written and read back
    /// distinct.
    #[test]
    fn the_project_setting_columns_round_trip() {
        let path = temp_db("project-setting-columns");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store.insert_project(&project(true)).expect("project");
        let mut kept = store.get_project("project-1").unwrap().expect("kept");
        assert!(kept.trust_consent);
        assert_eq!(kept.claude_account_id, ProjectAccount::Inherit);
        assert_eq!(kept.grok_session_effort, None);
        kept.claude_account_id = named("claude-account");
        kept.codex_account_id = named("codex-account");
        kept.grok_account_id = named("grok-account");
        kept.claude_session_model = Some("claude-model".to_string());
        kept.claude_session_effort = Some("claude-effort".to_string());
        kept.codex_session_model = Some("codex-model".to_string());
        kept.codex_session_effort = Some("codex-effort".to_string());
        kept.grok_session_model = Some("grok-model".to_string());
        kept.grok_session_effort = Some("grok-effort".to_string());
        store.update_project(&kept).expect("writes the columns");

        let fields = |project: &Project| {
            [
                project.claude_account_id.account_id().map(str::to_string),
                project.codex_account_id.account_id().map(str::to_string),
                project.grok_account_id.account_id().map(str::to_string),
                project.claude_session_model.clone(),
                project.claude_session_effort.clone(),
                project.codex_session_model.clone(),
                project.codex_session_effort.clone(),
                project.grok_session_model.clone(),
                project.grok_session_effort.clone(),
            ]
            .map(|field| field.expect("set"))
        };
        let expected = [
            "claude-account",
            "codex-account",
            "grok-account",
            "claude-model",
            "claude-effort",
            "codex-model",
            "codex-effort",
            "grok-model",
            "grok-effort",
        ];
        let reread = store.get_project("project-1").unwrap().expect("kept");
        assert_eq!(fields(&reread), expected);
        // The trust permission and the pin sit after those columns in the row and are read by
        // position, so they are checked to have stayed where they were.
        assert!(reread.trust_consent);
        assert!(!reread.pinned);

        // A project inserted with all nine set reads them back the same way.
        let mut inserted = project(false);
        inserted.id = "project-2".to_string();
        inserted.path = "/tmp/project-2".to_string();
        inserted.claude_account_id = kept.claude_account_id.clone();
        inserted.codex_account_id = kept.codex_account_id.clone();
        inserted.grok_account_id = kept.grok_account_id.clone();
        inserted.claude_session_model = kept.claude_session_model.clone();
        inserted.claude_session_effort = kept.claude_session_effort.clone();
        inserted.codex_session_model = kept.codex_session_model.clone();
        inserted.codex_session_effort = kept.codex_session_effort.clone();
        inserted.grok_session_model = kept.grok_session_model.clone();
        inserted.grok_session_effort = kept.grok_session_effort.clone();
        store.insert_project(&inserted).expect("insert");
        let listed = store
            .list_projects()
            .unwrap()
            .into_iter()
            .find(|project| project.id == "project-2")
            .expect("listed");
        assert_eq!(fields(&listed), expected);
        assert!(!listed.trust_consent);
    }

    /// A reporter whose conclusion only holds while the session has not moved on writes through
    /// this, so the check and the write must be one step: it moves the session only from the status
    /// it expected, and says which happened.
    #[test]
    fn a_conditional_status_write_moves_the_session_only_from_the_status_it_expected() {
        let path = temp_db("conditional-status");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        let mut waiting = session(None);
        waiting.status = SessionStatus::WaitingUser;
        store.insert_session(&waiting).expect("insert");

        // From the expected status it moves, and hands back the record as it now stands.
        let moved = store
            .update_session_status_if("session-1", SessionStatus::WaitingUser, SessionStatus::Idle)
            .expect("conditional update");
        assert_eq!(moved.map(|s| s.status), Some(SessionStatus::Idle));

        // The same write a second time finds the session already moved on and leaves it alone —
        // this is the stale reporter whose conclusion no longer holds.
        let again = store
            .update_session_status_if("session-1", SessionStatus::WaitingUser, SessionStatus::Idle)
            .expect("conditional update");
        assert!(again.is_none());

        // And a status the session is not in cannot be overwritten by one that expects it.
        store
            .update_session_status_if("session-1", SessionStatus::Idle, SessionStatus::Working)
            .expect("conditional update")
            .expect("the session was idle, so this one moves it");
        let stale = store
            .update_session_status_if("session-1", SessionStatus::WaitingUser, SessionStatus::Idle)
            .expect("conditional update");
        assert!(stale.is_none());
        assert_eq!(
            store.get_session("session-1").unwrap().unwrap().status,
            SessionStatus::Working,
            "a write whose expected status did not hold must not have landed"
        );

        // An id that is not there is simply no move, not an error.
        assert!(store
            .update_session_status_if("nobody", SessionStatus::WaitingUser, SessionStatus::Idle)
            .expect("conditional update")
            .is_none());
    }

    fn named(id: &str) -> ProjectAccount {
        ProjectAccount::Account { id: id.to_string() }
    }

    fn project(consent: bool) -> Project {
        Project {
            id: "project-1".to_string(),
            console_id: "console-1".to_string(),
            host_id: LOCAL_HOST_ID.to_string(),
            name: "Project".to_string(),
            path: "/tmp/project-1".to_string(),
            default_agent: None,
            source: ProjectSource::Local,
            remote_url: None,
            claude_account_id: ProjectAccount::Inherit,
            codex_account_id: ProjectAccount::Inherit,
            grok_account_id: ProjectAccount::Inherit,
            claude_session_model: None,
            claude_session_effort: None,
            codex_session_model: None,
            codex_session_effort: None,
            grok_session_model: None,
            grok_session_effort: None,
            trust_consent: consent,
            pinned: false,
            tags: Vec::new(),
            description: None,
            git_workflow: Default::default(),
        }
    }

    /// The branch records are not the sessions' rows: deleting the project takes the sessions with
    /// it and leaves the attribution.
    #[test]
    fn branch_origins_outlive_their_session() {
        let path = temp_db("branch-origins");
        let origin = BranchOrigin {
            repository: "/r/.git".to_string(),
            branch: "feature".to_string(),
            created_at: 7,
            birth_commit: "abc".to_string(),
            birth_time: 7,
            responsible_session: "session-1".to_string(),
            responsible_since: 7,
            responsible_title: "Title".to_string(),
            target_branch: Some("main".to_string()),
        };
        let put = |store: &Store| {
            store
                .apply_branch_origins("/r/.git", &[], std::slice::from_ref(&origin), 10)
                .expect("origin");
        };
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store.insert_project(&project(false)).expect("project");
        assert!(store.branch_origins("/r/.git").unwrap().is_empty());

        put(&store);
        store.delete_project("project-1").expect("project deleted");
        assert_eq!(store.branch_origins("/r/.git").unwrap(), [origin]);
    }

    /// Each branch of a handover is compared and set on its own: the ones the expected session
    /// holds move, the rest are reported and stay.
    #[test]
    fn a_handover_moves_only_the_branches_the_expected_session_holds() {
        let path = temp_db("branch-handover");
        let store = Store::open(&path).expect("store");
        let origin = |branch: &str, session: &str| BranchOrigin {
            repository: "/r/.git".to_string(),
            branch: branch.to_string(),
            created_at: 7,
            birth_commit: "abc".to_string(),
            birth_time: 7,
            responsible_session: session.to_string(),
            responsible_since: 7,
            responsible_title: "Title".to_string(),
            target_branch: None,
        };
        store
            .apply_branch_origins("/r/.git", &[], &[origin("a", "s1"), origin("b", "s2")], 10)
            .unwrap();
        let names = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();

        let moved = store
            .move_branch_responsibility("/r/.git", &names(&["a", "b", "c"]), "s1", "s3", "Three", 9)
            .unwrap();
        assert_eq!(moved.moved, names(&["a"]));
        assert_eq!(moved.unmoved, names(&["b", "c"]));

        let rows = store.branch_origins("/r/.git").unwrap();
        let a = &rows[0];
        assert_eq!(
            (a.responsible_session.as_str(), a.responsible_since),
            ("s3", 9)
        );
        assert_eq!(a.responsible_title, "Three");
        assert_eq!(rows[1], origin("b", "s2"));
        let other = store
            .move_branch_responsibility("/elsewhere/.git", &names(&["a"]), "s3", "s4", "Four", 10)
            .unwrap();
        assert!(other.moved.is_empty());
    }

    /// The remote URL is written and cleared on its own, and an edit of the project leaves it be.
    #[test]
    fn a_projects_remote_url_is_set_and_cleared_apart_from_its_edits() {
        let path = temp_db("remote-url");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store.insert_project(&project(false)).expect("project");

        store
            .set_project_remote_url("project-1", Some("git@example.com:o/r.git"))
            .unwrap();
        let mut edited = store.get_project("project-1").unwrap().unwrap();
        assert_eq!(
            edited.remote_url.as_deref(),
            Some("git@example.com:o/r.git")
        );
        edited.name = "Renamed".to_string();
        edited.remote_url = None;
        store.update_project(&edited).unwrap();
        let stored = store.get_project("project-1").unwrap().unwrap();
        assert_eq!(stored.name, "Renamed");
        assert_eq!(
            stored.remote_url.as_deref(),
            Some("git@example.com:o/r.git")
        );

        store.set_project_remote_url("project-1", None).unwrap();
        assert_eq!(
            store.get_project("project-1").unwrap().unwrap().remote_url,
            None
        );
    }

    /// A project's pinned flag starts unset and round-trips through an update.
    #[test]
    fn a_projects_pinned_flag_round_trips() {
        let path = temp_db("project-pinned");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store.insert_project(&project(false)).expect("project");
        assert!(!store.get_project("project-1").unwrap().unwrap().pinned);

        let mut pinned = store.get_project("project-1").unwrap().expect("kept");
        pinned.pinned = true;
        store.update_project(&pinned).unwrap();
        assert!(store.get_project("project-1").unwrap().unwrap().pinned);
    }

    #[test]
    fn session_locks_persist_and_guard_every_store_removal() {
        let path = temp_db("session-locked");
        let store = Store::open(&path).unwrap();
        store.insert_console(&console(None, None, None)).unwrap();
        store.insert_project(&project(false)).unwrap();
        let mut record = session(None);
        record.project_id = Some("project-1".to_string());
        store.insert_session(&record).unwrap();
        assert!(!store.get_session("session-1").unwrap().unwrap().locked);
        store.set_session_locked("session-1", true).unwrap();
        assert!(store
            .set_session_title("session-1", "Renamed")
            .unwrap()
            .is_some());
        assert!(store.get_session("session-1").unwrap().unwrap().locked);
        assert!(store.archive_session("session-1", 1).is_err());
        assert!(store.delete_session("session-1").is_err());
        assert!(store.delete_session_if_unused("session-1").is_err());
        assert!(store.delete_project("project-1").is_err());
        assert!(store.delete_console("console-1").is_err());
        assert!(store.get_project("project-1").unwrap().is_some());
        store.set_session_locked("session-1", false).unwrap();
        store.archive_session("session-1", 1).unwrap();
        store.set_session_locked("session-1", true).unwrap();
        assert!(store.delete_session_if_archived("session-1").is_err());
        drop(store);
        let store = Store::open(&path).unwrap();
        assert!(store.get_session("session-1").unwrap().unwrap().locked);
        store.set_session_locked("session-1", false).unwrap();
        assert!(store
            .delete_session_if_archived("session-1")
            .unwrap()
            .is_some());
    }

    /// Each write of a session's changing fields writes its own columns, so writers on tasks of
    /// their own — a rename, the hook that names the conversation, the one that records its first
    /// turn — cannot undo each other or the user's pin; and a write to a deleted session reports
    /// that nothing was written.
    #[test]
    fn session_writes_leave_each_others_columns_alone() {
        let path = temp_db("session-columns");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store.insert_session(&session(None)).expect("insert");

        store.set_session_pinned("session-1", true).unwrap();
        assert!(store
            .set_agent_session_id("session-1", "conversation-1")
            .unwrap()
            .is_some());
        assert!(store
            .mark_conversation_started("session-1")
            .unwrap()
            .is_some());
        let renamed = store
            .set_session_title("session-1", "Renamed")
            .unwrap()
            .expect("renamed");
        assert_eq!(renamed.agent_session_id.as_deref(), Some("conversation-1"));
        assert!(renamed.has_conversation);
        assert!(renamed.pinned);
        let running = store
            .set_running_status("session-1", SessionStatus::Working)
            .unwrap()
            .expect("working");
        assert_eq!(running.title, "Renamed");
        assert_eq!(running.agent_session_id.as_deref(), Some("conversation-1"));

        // Nothing to change is no write.
        assert!(store
            .set_agent_session_id("session-1", "conversation-1")
            .unwrap()
            .is_none());
        assert!(store
            .mark_conversation_started("session-1")
            .unwrap()
            .is_none());
        assert!(store
            .set_running_status("session-1", SessionStatus::Working)
            .unwrap()
            .is_none());

        assert!(store.archive_session("session-1", 1).unwrap().is_some());
        assert!(store
            .delete_session_if_archived("session-1")
            .unwrap()
            .is_some());
        assert!(store
            .set_session_title("session-1", "Gone")
            .unwrap()
            .is_none());
        assert!(store.archive_session("session-1", 2).unwrap().is_none());
        assert!(store.record_session_exit("session-1", 2).unwrap().is_none());
        drop(store);
    }

    /// A running status reported after the session went dormant — a Stop hook landing after an
    /// archive, a late event after the process ended — leaves it dormant; and the process going
    /// away keeps an archive while interrupting anything else.
    #[test]
    fn a_dormant_session_stays_so_whatever_its_agent_reports() {
        let path = temp_db("session-dormant");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store.insert_session(&session(None)).expect("insert");
        store
            .set_running_status("session-1", SessionStatus::Working)
            .unwrap()
            .expect("working");

        let archived = store
            .archive_session("session-1", 10)
            .unwrap()
            .expect("archived");
        assert_eq!(archived.status, SessionStatus::Archived);
        assert!(store
            .set_running_status("session-1", SessionStatus::Idle)
            .unwrap()
            .is_none());
        assert!(store.interrupt_session("session-1", 20).unwrap().is_none());
        let (before, exited) = store
            .record_session_exit("session-1", 20)
            .unwrap()
            .expect("exited");
        assert_eq!(before, SessionStatus::Archived);
        assert_eq!(exited.status, SessionStatus::Archived);
        assert_eq!(exited.ended_at, Some(20));

        store
            .update_session_lifecycle_if(
                "session-1",
                SessionStatus::Archived,
                SessionStatus::Working,
                None,
            )
            .unwrap()
            .expect("relaunched");
        let (before, exited) = store
            .record_session_exit("session-1", 30)
            .unwrap()
            .expect("exited");
        assert_eq!(before, SessionStatus::Working);
        assert_eq!(exited.status, SessionStatus::Interrupted);
        assert_eq!(exited.ended_at, Some(30));
        assert!(store
            .set_running_status("session-1", SessionStatus::WaitingUser)
            .unwrap()
            .is_none());
        drop(store);
    }

    /// The consent is its own statement: a project starts without it, recording it survives a read
    /// back, and the user's own edit of the project neither grants nor withdraws it.
    #[test]
    fn the_trust_consent_is_recorded_on_its_own_and_an_edit_leaves_it_alone() {
        let path = temp_db("trust-consent");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store.insert_project(&project(false)).expect("project");
        let consented = |store: &Store| {
            store
                .get_project("project-1")
                .unwrap()
                .unwrap()
                .trust_consent
        };
        assert!(!consented(&store));

        store
            .set_project_trust_consent("project-1", true)
            .expect("recorded");
        assert!(consented(&store));
        assert!(store.list_projects().unwrap()[0].trust_consent);

        let mut edited = store.get_project("project-1").unwrap().unwrap();
        edited.name = "Renamed".to_string();
        edited.trust_consent = false;
        store.update_project(&edited).expect("edited");
        assert!(consented(&store), "an edit must not withdraw the consent");

        store
            .set_project_trust_consent("project-1", false)
            .expect("withdrawn");
        assert!(!consented(&store));
    }

    /// A console's project paths are listed as stored, and only its own.
    #[test]
    fn a_console_lists_only_its_own_project_paths() {
        let path = temp_db("project-paths");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        let mut stored = project(false);
        stored.path = "/work/project".to_string();
        store.insert_project(&stored).expect("project");

        assert_eq!(
            store.project_paths("console-1").unwrap(),
            vec!["/work/project".to_string()]
        );
        assert!(store.project_paths("console-2").unwrap().is_empty());
    }

    /// A fresh database starts with no trusted directories, and what is added survives opening it
    /// again.
    #[test]
    fn the_trusted_directories_start_empty_and_round_trip() {
        let path = temp_db("trusted-directories");
        let store = Store::open(&path).expect("store");
        assert!(store.trusted_directories().unwrap().is_empty());

        assert!(store.add_trusted_directory("/work").unwrap());
        assert!(
            !store.add_trusted_directory("/work").unwrap(),
            "a repeat adds nothing"
        );
        assert!(store.add_trusted_directory("/another").unwrap());
        drop(store);

        let store = Store::open(&path).expect("opens again");
        assert_eq!(store.trusted_directories().unwrap(), ["/another", "/work"]);
        assert!(store.remove_trusted_directory("/work").unwrap());
        assert!(!store.remove_trusted_directory("/work").unwrap());
        assert_eq!(store.trusted_directories().unwrap(), ["/another"]);
    }

    /// A console's references round-trip as written, and a session keeps its own copy of its
    /// account's directory regardless of what the console's reference is afterwards: only
    /// `set_session_account` changes it, and always the two fields together.
    #[test]
    fn a_consoles_account_references_round_trip_and_a_session_keeps_its_own_directory() {
        let path = temp_db("round-trip");
        let store = Store::open(&path).expect("store");

        store
            .insert_account(&account(
                "claude-acct",
                Agent::Claude,
                "/home/u/.claude-alt",
            ))
            .expect("account");
        store
            .insert_account(&account("codex-acct", Agent::Codex, "/home/u/.codex-alt"))
            .expect("account");
        store
            .insert_account(&account("grok-acct", Agent::Grok, "/home/u/.grok-alt"))
            .expect("account");
        store
            .insert_console(&console(
                Some("claude-acct"),
                Some("codex-acct"),
                Some("grok-acct"),
            ))
            .expect("insert");
        let stored = store.get_console("console-1").unwrap().unwrap();
        assert_eq!(stored.claude_account_id.as_deref(), Some("claude-acct"));
        assert_eq!(stored.codex_account_id.as_deref(), Some("codex-acct"));
        assert_eq!(stored.grok_account_id.as_deref(), Some("grok-acct"));

        let mut opened = session(Some("/home/u/.claude-alt"));
        opened.account_id = Some("claude-acct".to_string());
        store.insert_session(&opened).expect("insert");

        // Updating the session's mutable fields leaves the account and directory it has alone —
        // neither is in `update_session`'s write set.
        let mut live = store.get_session("session-1").unwrap().unwrap();
        live.account_id = Some("elsewhere".to_string());
        live.config_dir = Some("/elsewhere".to_string());
        live.title = "Renamed".to_string();
        store.update_session(&live).expect("update");
        let after = store.get_session("session-1").unwrap().unwrap();
        assert_eq!(after.title, "Renamed");
        assert_eq!(after.account_id.as_deref(), Some("claude-acct"));
        assert_eq!(after.config_dir.as_deref(), Some("/home/u/.claude-alt"));

        // The switch's write path moves both together, to the default account (nothing pinned)
        // included, and reports a session that is gone as `None`.
        let moved = store
            .set_session_account("session-1", None, None)
            .unwrap()
            .expect("the session");
        assert_eq!((moved.account_id, moved.config_dir), (None, None));
        assert!(store
            .set_session_account("missing", None, None)
            .unwrap()
            .is_none());
    }

    #[test]
    fn updating_a_console_persists_each_agents_account_reference() {
        let path = temp_db("update-console");
        let store = Store::open(&path).expect("store");
        store
            .insert_account(&account("alt", Agent::Claude, "/home/u/.alt"))
            .expect("account");
        store
            .insert_account(&account("other", Agent::Claude, "/home/u/other"))
            .expect("account");
        let mut stored = console(None, None, None);
        store.insert_console(&stored).expect("insert");
        let read = |store: &Store| {
            store
                .get_console("console-1")
                .unwrap()
                .unwrap()
                .claude_account_id
        };
        assert_eq!(read(&store), None);

        for account_id in [Some("alt"), Some("other"), None] {
            stored.claude_account_id = account_id.map(str::to_string);
            store.update_console(&stored).expect("update");
            assert_eq!(read(&store), account_id.map(str::to_string));
        }
    }

    /// Removing an account clears every console's and project's reference to it, in the same step,
    /// so no reader is ever left pointing at an id the `accounts` table no longer has.
    #[test]
    fn deleting_an_account_clears_every_consoles_and_projects_reference_to_it() {
        let path = temp_db("delete-account");
        let store = Store::open(&path).expect("store");
        store
            .insert_account(&account(
                "claude-acct",
                Agent::Claude,
                "/home/u/.claude-alt",
            ))
            .expect("account");
        store
            .insert_console(&console(Some("claude-acct"), None, None))
            .expect("insert");
        let mut other = console(Some("claude-acct"), None, None);
        other.id = "console-2".to_string();
        store.insert_console(&other).expect("insert");
        store
            .insert_account(&account(
                "claude-other",
                Agent::Claude,
                "/home/u/.claude-other",
            ))
            .expect("account");
        let mut untouched = console(Some("claude-other"), None, None);
        untouched.id = "console-3".to_string();
        store.insert_console(&untouched).expect("insert");
        let mut referring = project(false);
        referring.claude_account_id = named("claude-acct");
        // Another agent's setting of the same project is not touched by a Claude account going.
        referring.codex_account_id = ProjectAccount::Default;
        store.insert_project(&referring).expect("insert");
        // A project that pins the default account refers to no id, so it keeps saying so.
        let mut defaulting = project(false);
        defaulting.id = "project-2".to_string();
        defaulting.path = "/tmp/project-2".to_string();
        defaulting.claude_account_id = ProjectAccount::Default;
        store.insert_project(&defaulting).expect("insert");
        let mut inheriting = project(false);
        inheriting.id = "project-3".to_string();
        inheriting.path = "/tmp/project-3".to_string();
        store.insert_project(&inheriting).expect("insert");

        store.delete_account("claude-acct").expect("deleted");
        let claude_account = |id: &str| store.get_project(id).unwrap().unwrap().claude_account_id;
        assert_eq!(claude_account("project-1"), ProjectAccount::Inherit);
        assert_eq!(claude_account("project-2"), ProjectAccount::Default);
        assert_eq!(claude_account("project-3"), ProjectAccount::Inherit);
        assert_eq!(
            store
                .get_project("project-1")
                .unwrap()
                .unwrap()
                .codex_account_id,
            ProjectAccount::Default
        );
        assert!(store.get_account("claude-acct").unwrap().is_none());
        for id in ["console-1", "console-2"] {
            let console = store.get_console(id).unwrap().unwrap();
            assert_eq!(console.claude_account_id, None);
        }
        let untouched = store.get_console("console-3").unwrap().unwrap();
        assert_eq!(untouched.claude_account_id.as_deref(), Some("claude-other"));
    }

    /// The added agents are undecided on a fresh database, survive a restart once decided, and a
    /// list saved empty is still decided.
    #[test]
    fn added_agents_are_undecided_until_filled_and_survive_a_restart() {
        let path = temp_db("added-agents");
        let store = Store::open(&path).unwrap();
        assert_eq!(store.added_agents().unwrap(), None);
        assert_eq!(store.get_settings().unwrap().added_agents, None);

        assert!(store
            .set_added_agents_if_undecided(&[Agent::Codex, Agent::Claude])
            .unwrap());
        assert!(
            !store.set_added_agents_if_undecided(&[Agent::Grok]).unwrap(),
            "decided"
        );
        assert!(store.add_added_agent(Agent::Grok).unwrap());
        assert!(!store.add_added_agent(Agent::Grok).unwrap());
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(
            store.get_settings().unwrap().added_agents,
            Some(vec![Agent::Codex, Agent::Claude, Agent::Grok])
        );

        for agent in Agent::ALL {
            store.remove_added_agent(agent).unwrap();
        }
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(store.added_agents().unwrap(), Some(vec![]));
        assert!(!store.set_added_agents_if_undecided(&Agent::ALL).unwrap());
    }

    /// A stored value that does not read as a list counts as undecided, so the fill replaces it,
    /// and an append is refused rather than deciding the list as just that agent.
    #[test]
    fn a_malformed_added_agents_value_is_filled_and_not_appended_to() {
        let path = temp_db("added-agents-malformed");
        let store = Store::open(&path).unwrap();
        store
            .lock()
            .execute("UPDATE settings SET added_agents = 'not json'", [])
            .unwrap();
        assert_eq!(store.added_agents().unwrap(), None);
        assert!(store.add_added_agent(Agent::Claude).is_err());

        assert!(store
            .set_added_agents_if_undecided(&[Agent::Codex])
            .unwrap());
        assert_eq!(store.added_agents().unwrap(), Some(vec![Agent::Codex]));
    }

    /// The approval settings read as unset until chosen, and round-trip.
    #[test]
    fn approval_settings_round_trip() {
        let path = temp_db("approval-columns");
        let store = Store::open(&path).unwrap();
        assert_eq!(store.codex_approval_reviewer().unwrap(), None);
        assert_eq!(store.claude_approval_mode().unwrap(), None);

        store
            .set_codex_approval_reviewer(Some(ApprovalReviewer::AutoReview))
            .unwrap();
        store
            .set_claude_approval_mode(Some(ClaudeApprovalMode::Bypass))
            .unwrap();
        drop(store);
        let store = Store::open(&path).unwrap();
        let settings = store.get_settings().unwrap();
        assert_eq!(
            settings.codex_approval_reviewer,
            Some(ApprovalReviewer::AutoReview)
        );
        assert_eq!(
            settings.claude_approval_mode,
            Some(ClaudeApprovalMode::Bypass)
        );
        store.set_codex_approval_reviewer(None).unwrap();
        assert_eq!(store.codex_approval_reviewer().unwrap(), None);
        store.set_claude_approval_mode(None).unwrap();
        assert_eq!(store.claude_approval_mode().unwrap(), None);
    }

    /// An account's name and directory round-trip, and `update_account` can change either
    /// independently of the other.
    #[test]
    fn an_accounts_name_and_directory_round_trip_and_update_independently() {
        let path = temp_db("account-round-trip");
        let store = Store::open(&path).expect("store");
        store
            .insert_account(&account("acct-1", Agent::Codex, "/home/u/.codex-alt"))
            .expect("account");

        let mut renamed = store.get_account("acct-1").unwrap().unwrap();
        renamed.name = "Work".to_string();
        store.update_account(&renamed).expect("renamed");
        let after_rename = store.get_account("acct-1").unwrap().unwrap();
        assert_eq!(after_rename.name, "Work");
        assert_eq!(after_rename.config_dir, "/home/u/.codex-alt");

        let mut repointed = after_rename;
        repointed.config_dir = "/home/u/.codex-work".to_string();
        store.update_account(&repointed).expect("repointed");
        let after_repoint = store.get_account("acct-1").unwrap().unwrap();
        assert_eq!(after_repoint.name, "Work");
        assert_eq!(after_repoint.config_dir, "/home/u/.codex-work");

        assert_eq!(store.list_accounts().unwrap(), [after_repoint]);
    }

    /// Repointing an account starts the new directory out not sharing and forgets what turning
    /// sharing on recorded of the old one, and an outcome carried out on the old directory is not
    /// recorded for the new one; a rename changes none of it.
    #[test]
    fn repointing_an_account_starts_its_new_directory_out_not_sharing() {
        let path = temp_db("account-sharing-repoint");
        let store = Store::open(&path).expect("store");
        store
            .insert_account(&account("acct-1", Agent::Codex, "/home/u/.codex-alt"))
            .expect("account");
        let start = SharingStart {
            stamp: "20261020T000000Z".to_string(),
            default_dir: "/home/u/.codex".to_string(),
            completed: false,
        };
        store.set_account_sharing("acct-1", true, true).unwrap();
        assert!(store
            .start_account_sharing("acct-1", "/home/u/.codex-alt", &start)
            .unwrap());
        assert!(store
            .finish_account_sharing("acct-1", "/home/u/.codex-alt", true)
            .unwrap());

        let mut renamed = store.get_account("acct-1").unwrap().unwrap();
        renamed.name = "Work".to_string();
        store.update_account(&renamed).unwrap();
        let after_rename = store.get_account("acct-1").unwrap().unwrap();
        assert!(after_rename.share_default_config && !after_rename.share_default_config_incomplete);
        assert_eq!(
            store.account_sharing_start("acct-1").unwrap(),
            Some(SharingStart {
                completed: true,
                ..start.clone()
            })
        );

        let mut repointed = after_rename;
        repointed.config_dir = "/home/u/.codex-work".to_string();
        store.update_account(&repointed).unwrap();
        let after_repoint = store.get_account("acct-1").unwrap().unwrap();
        assert!(
            !after_repoint.share_default_config && !after_repoint.share_default_config_incomplete
        );
        assert_eq!(store.account_sharing_start("acct-1").unwrap(), None);
        store.set_account_sharing("acct-1", true, true).unwrap();
        assert!(!store
            .start_account_sharing("acct-1", "/home/u/.codex-alt", &start)
            .unwrap());
        assert!(!store
            .finish_account_sharing("acct-1", "/home/u/.codex-alt", true)
            .unwrap());
        assert!(
            store
                .get_account("acct-1")
                .unwrap()
                .unwrap()
                .share_default_config_incomplete
        );

        // Respelled only: the same directory where the platform compares it so, keeping what
        // sharing recorded of it.
        let start = SharingStart {
            stamp: "20261021T000000Z".to_string(),
            ..start
        };
        assert!(store
            .start_account_sharing("acct-1", "/home/u/.codex-work", &start)
            .unwrap());
        let mut respelled = store.get_account("acct-1").unwrap().unwrap();
        respelled.config_dir = "/home/u/.Codex-Work".to_string();
        store.update_account(&respelled).unwrap();
        let same = crate::account_dirs::account_dir_key("/home/u/.codex-work")
            == crate::account_dirs::account_dir_key("/home/u/.Codex-Work");
        assert_eq!(
            store.account_sharing_start("acct-1").unwrap().is_some(),
            same
        );
    }

    /// A freshly created project has no tags, and tags round-trip through an update.
    #[test]
    fn a_projects_tags_round_trip() {
        let path = temp_db("project-tags");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store.insert_project(&project(false)).expect("project");
        let mut project = store.get_project("project-1").unwrap().expect("kept");
        assert!(project.tags.is_empty());

        project.tags = vec!["backend".to_string(), "Rust".to_string()];
        store.update_project(&project).unwrap();
        assert_eq!(
            store.get_project("project-1").unwrap().unwrap().tags,
            ["backend", "Rust"]
        );
    }

    /// A freshly created project has no description, and a description round-trips through an
    /// update and is cleared by one.
    #[test]
    fn a_projects_description_round_trips() {
        let path = temp_db("project-description");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store.insert_project(&project(false)).expect("project");
        let mut project = store.get_project("project-1").unwrap().expect("kept");
        assert_eq!(project.description, None);

        project.description = Some("A billing service.".to_string());
        store.update_project(&project).unwrap();
        assert_eq!(
            store
                .get_project("project-1")
                .unwrap()
                .unwrap()
                .description
                .as_deref(),
            Some("A billing service.")
        );

        project.description = None;
        store.update_project(&project).unwrap();
        assert_eq!(
            store.get_project("project-1").unwrap().unwrap().description,
            None
        );
    }

    #[test]
    fn tags_that_are_not_a_json_array_of_strings_read_as_untagged() {
        assert!(tags_from_text("project-1", "not json").is_empty());
    }

    /// Each boolean setting starts at its default, round-trips, and a repeat write reports no
    /// change; the settings are independent of one another.
    #[test]
    fn the_boolean_settings_default_round_trip_and_a_repeat_write_changes_nothing() {
        struct Setting {
            column: &'static str,
            default: bool,
            read: fn(&Settings) -> bool,
            get: fn(&Store) -> Result<bool>,
            set: fn(&Store, bool) -> Result<bool>,
        }
        let settings = [
            Setting {
                column: "auto_pull_repositories",
                default: false,
                read: |settings| settings.auto_pull_repositories,
                get: Store::auto_pull_repositories,
                set: Store::set_auto_pull_repositories,
            },
            Setting {
                column: "auto_delete_unresumable_archives",
                default: true,
                read: |settings| settings.auto_delete_unresumable_archives,
                get: Store::auto_delete_unresumable_archives,
                set: Store::set_auto_delete_unresumable_archives,
            },
            Setting {
                column: "auto_approve_session_requests",
                default: false,
                read: |settings| settings.auto_approve_session_requests,
                get: Store::auto_approve_session_requests,
                set: Store::set_auto_approve_session_requests,
            },
            Setting {
                column: "auto_approve_resume_requests",
                default: false,
                read: |settings| settings.auto_approve_resume_requests,
                get: Store::auto_approve_resume_requests,
                set: Store::set_auto_approve_resume_requests,
            },
            Setting {
                column: "auto_approve_account_switches",
                default: false,
                read: |settings| settings.auto_approve_account_switches,
                get: Store::auto_approve_account_switches,
                set: Store::set_auto_approve_account_switches,
            },
            Setting {
                column: "auto_approve_handoffs",
                default: false,
                read: |settings| settings.auto_approve_handoffs,
                get: Store::auto_approve_handoffs,
                set: Store::set_auto_approve_handoffs,
            },
            Setting {
                column: "ask_questions_as_choices",
                default: true,
                read: |settings| settings.ask_questions_as_choices,
                get: Store::ask_questions_as_choices,
                set: Store::set_ask_questions_as_choices,
            },
            Setting {
                column: "auto_select_account",
                default: false,
                read: |settings| settings.auto_select_account,
                get: Store::auto_select_account,
                set: Store::set_auto_select_account,
            },
            Setting {
                column: "auto_switch_on_usage_limit",
                default: false,
                read: |settings| settings.auto_switch_on_usage_limit,
                get: Store::auto_switch_on_usage_limit,
                set: Store::set_auto_switch_on_usage_limit,
            },
            Setting {
                column: "sidebar_order_by_status",
                default: true,
                read: |settings| settings.sidebar_order_by_status,
                get: Store::sidebar_order_by_status,
                set: Store::set_sidebar_order_by_status,
            },
            Setting {
                column: "auto_start_quota_windows",
                default: false,
                read: |settings| settings.auto_start_quota_windows,
                get: Store::auto_start_quota_windows,
                set: Store::set_auto_start_quota_windows,
            },
        ];

        for setting in &settings {
            let column = setting.column;
            let changed = !setting.default;
            let path = temp_db(&format!("setting-{column}"));
            let store = Store::open(&path).expect("store");
            assert_eq!(
                (setting.read)(&store.get_settings().unwrap()),
                setting.default,
                "{column} starts at its default"
            );

            assert!((setting.set)(&store, changed).unwrap(), "{column}");
            assert!(
                !(setting.set)(&store, changed).unwrap(),
                "{column}: a repeat write changes nothing"
            );
            assert_eq!((setting.get)(&store).unwrap(), changed, "{column}");
            assert_eq!(
                (setting.read)(&store.get_settings().unwrap()),
                changed,
                "{column}"
            );
            for other in settings.iter().filter(|other| other.column != column) {
                assert_eq!(
                    (other.get)(&store).unwrap(),
                    other.default,
                    "writing {column} left {} alone",
                    other.column
                );
            }
            assert!((setting.set)(&store, setting.default).unwrap(), "{column}");
        }
    }

    /// The application-wide git workflow settings start at worktree, on, ask, off, change item by
    /// item with a repeat write reporting no change.
    #[test]
    fn the_git_workflow_settings_default_and_change_per_item() {
        let path = temp_db("git-workflow-settings");
        let store = Store::open(&path).expect("store");
        assert_eq!(
            store.git_workflow_settings().unwrap(),
            GitWorkflowSettings::default()
        );
        assert_eq!(
            store.get_settings().unwrap().git_workflow,
            GitWorkflowSettings::default()
        );

        let changes = GitWorkflowSettingsChanges {
            delivery: Some(Delivery::PullRequest),
            revert_gate: Some(true),
            ..GitWorkflowSettingsChanges::default()
        };
        assert!(store.change_git_workflow_settings(&changes).unwrap());
        assert!(!store.change_git_workflow_settings(&changes).unwrap());
        assert!(!store
            .change_git_workflow_settings(&GitWorkflowSettingsChanges::default())
            .unwrap());
        assert_eq!(
            store.git_workflow_settings().unwrap(),
            GitWorkflowSettings {
                development: Development::Worktree,
                squash_before_delivery: false,
                rebase_before_merge: true,
                delivery: Delivery::PullRequest,
                revert_gate: true,
            }
        );
    }

    /// A project's override is stored per item with NULL for following the application-wide
    /// value, and `effective` tracks a later change of the application-wide value for the items
    /// the project does not hold.
    #[test]
    fn a_project_override_falls_back_to_the_global_value_and_follows_its_changes() {
        let path = temp_db("git-workflow-override");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store.insert_project(&project(false)).expect("project");
        let stored = || store.get_project("project-1").unwrap().unwrap();
        assert_eq!(stored().git_workflow, GitWorkflowOverride::default());
        assert_eq!(
            git_workflow::effective(&store, &stored()).unwrap(),
            GitWorkflowSettings::default()
        );

        let mut edited = stored();
        edited.git_workflow = GitWorkflowOverride {
            development: Some(Development::InPlace),
            squash_before_delivery: Some(false),
            delivery: Some(Delivery::Merge),
            ..GitWorkflowOverride::default()
        };
        store.update_project(&edited).expect("update");
        assert_eq!(stored().git_workflow, edited.git_workflow);
        assert_eq!(
            store.list_projects().unwrap()[0].git_workflow,
            edited.git_workflow
        );

        store
            .change_git_workflow_settings(&GitWorkflowSettingsChanges {
                development: Some(Development::Branch),
                squash_before_delivery: Some(true),
                rebase_before_merge: Some(false),
                delivery: Some(Delivery::PullRequest),
                revert_gate: Some(true),
            })
            .unwrap();
        assert_eq!(
            git_workflow::effective(&store, &stored()).unwrap(),
            GitWorkflowSettings {
                development: Development::InPlace,
                squash_before_delivery: false,
                rebase_before_merge: false,
                delivery: Delivery::Merge,
                revert_gate: true,
            }
        );

        let mut cleared = stored();
        cleared.git_workflow = GitWorkflowOverride::default();
        store.update_project(&cleared).expect("update");
        assert_eq!(stored().git_workflow, GitWorkflowOverride::default());
        assert!(
            git_workflow::effective(&store, &stored())
                .unwrap()
                .squash_before_delivery
        );
    }

    /// Turning a default account off and on records it per agent, a repeat write reports no change,
    /// and the last one back on clears the column to NULL, as a database that never had one reads.
    #[test]
    fn the_disabled_default_accounts_round_trip_and_clear_to_null() {
        let path = temp_db("disabled-default-accounts");
        let store = Store::open(&path).expect("store");
        let stored = |store: &Store| -> Option<String> {
            store
                .lock()
                .query_row(
                    "SELECT disabled_default_accounts FROM settings WHERE id = ?1",
                    params![SETTINGS_ID],
                    |row| row.get(0),
                )
                .unwrap()
        };
        assert!(store.disabled_default_accounts().unwrap().is_empty());
        assert!(!store.account_disabled(Agent::Codex, None).unwrap());
        assert!(!store
            .set_default_account_disabled(Agent::Codex, false)
            .unwrap());

        assert!(store
            .set_default_account_disabled(Agent::Codex, true)
            .unwrap());
        assert!(!store
            .set_default_account_disabled(Agent::Codex, true)
            .unwrap());
        assert!(store
            .set_default_account_disabled(Agent::Claude, true)
            .unwrap());
        assert_eq!(stored(&store).as_deref(), Some(r#"["claude","codex"]"#));
        assert!(store.account_disabled(Agent::Codex, None).unwrap());
        assert!(!store.account_disabled(Agent::Grok, None).unwrap());
        assert_eq!(
            store.get_settings().unwrap().disabled_default_accounts,
            ["claude".to_string(), "codex".to_string()].into()
        );

        assert!(store
            .set_default_account_disabled(Agent::Codex, false)
            .unwrap());
        assert!(store
            .set_default_account_disabled(Agent::Claude, false)
            .unwrap());
        assert_eq!(stored(&store), None);
        assert!(store.disabled_default_accounts().unwrap().is_empty());
    }

    /// An account row's flag is written alone, a repeat write reports no change, and an id of
    /// another agent is not disabled for the agent asked about.
    #[test]
    fn an_accounts_disabled_flag_is_written_alone() {
        let path = temp_db("account-disabled-flag");
        let store = Store::open(&path).expect("store");
        store
            .insert_account(&Account {
                id: "work".to_string(),
                agent: Agent::Claude,
                name: "Work".to_string(),
                config_dir: "/home/u/.claude-work".to_string(),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
            })
            .unwrap();
        assert!(store.set_account_disabled("work", true).unwrap());
        assert!(!store.set_account_disabled("work", true).unwrap());
        assert!(!store.set_account_disabled("nope", true).unwrap());
        assert!(store.get_account("work").unwrap().unwrap().disabled);
        assert!(store.account_disabled(Agent::Claude, Some("work")).unwrap());
        assert!(!store.account_disabled(Agent::Codex, Some("work")).unwrap());
        assert!(store.set_account_disabled("work", false).unwrap());
        assert!(!store.account_disabled(Agent::Claude, Some("work")).unwrap());
    }

    /// The default clone directory is the built-in one until set, and a blank value goes back to
    /// it.
    #[test]
    fn the_default_clone_dir_falls_back_to_the_built_in_one() {
        let path = temp_db("clone-dir");
        let store = Store::open(&path).expect("store");
        let built_in = crate::hostfs::expand("~/Projects")
            .to_string_lossy()
            .into_owned();
        assert_eq!(store.default_clone_dir().unwrap(), built_in);

        assert!(store.set_default_clone_dir(Some("/work/repos")).unwrap());
        assert!(!store.set_default_clone_dir(Some("/work/repos")).unwrap());
        assert_eq!(
            store.get_settings().unwrap().default_clone_dir,
            "/work/repos"
        );
        assert!(store.set_default_clone_dir(None).unwrap());
        assert_eq!(store.default_clone_dir().unwrap(), built_in);
    }

    /// The interface language is unset until a client reports one, a repeat of the same tag
    /// changes nothing, and a later one replaces it.
    #[test]
    fn the_interface_language_is_unset_until_reported() {
        let path = temp_db("interface-language");
        let store = Store::open(&path).expect("store");
        assert_eq!(store.interface_language().unwrap(), None);

        assert!(store.set_interface_language("ja").unwrap());
        assert!(!store.set_interface_language("ja").unwrap());
        assert_eq!(
            store.get_settings().unwrap().interface_language.as_deref(),
            Some("ja")
        );
        assert!(store.set_interface_language("de").unwrap());
        assert_eq!(store.interface_language().unwrap().as_deref(), Some("de"));
    }

    /// A project session record for the binding tests: unbound unless `bound_to` says otherwise.
    fn project_session(id: &str, bound_to: Option<&str>) -> Session {
        let mut built = session(None);
        built.id = id.to_string();
        built.role = Role::Project;
        built.bound_to = bound_to.map(str::to_string);
        built
    }

    /// A session opened under an owner is inserted only while the owner can still own sessions.
    /// Whichever of the two lands first, a binding of the owner to a project session or the new
    /// session's insert, orchestration stays two levels deep: this is the insert's side of the
    /// race `set_binding`'s `childless` check covers from the other.
    #[test]
    fn a_session_is_inserted_under_an_owner_only_while_the_owner_can_own() {
        let path = temp_db("insert-under-owner");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store
            .insert_session(&session(None))
            .expect("console session");
        store
            .insert_session(&project_session("owner", None))
            .expect("owner");
        store
            .insert_session(&project_session("peer", None))
            .expect("peer");

        // The owner binds itself to a project session after the new session's owner was resolved.
        store
            .set_binding("owner", None, "peer", false, true, false)
            .expect("binding")
            .expect("the owner had no sessions of its own");
        assert!(!store
            .insert_session_under_owner(&project_session("child", Some("owner")), "owner")
            .expect("insert"));
        assert!(store.get_session("child").unwrap().is_none());

        // One with no sessions of its own and no project session to answer to can own sessions.
        store
            .insert_session(&project_session("free", None))
            .expect("free");
        assert!(store
            .insert_session_under_owner(&project_session("child", Some("free")), "free")
            .expect("insert"));

        // The other order: with the child in place, the owner can no longer be bound to a project
        // session.
        assert!(store
            .set_binding("free", None, "peer", false, true, false)
            .expect("binding")
            .is_none());

        // An owner that is gone owns nothing.
        assert!(!store
            .insert_session_under_owner(&project_session("orphan", Some("nobody")), "nobody")
            .expect("insert"));
    }

    /// Locking is refused only for an archived session, which stays unlocked; a live session is
    /// locked, and an unknown id is not refused.
    #[test]
    fn a_session_is_locked_unless_archived() {
        let path = temp_db("lock-unless-archived");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store
            .insert_session(&project_session("live", None))
            .expect("live");
        let mut archived = project_session("archived", None);
        archived.status = SessionStatus::Archived;
        store.insert_session(&archived).expect("archived");

        assert!(store.set_session_locked_unless_archived("live").unwrap());
        assert!(store.get_session("live").unwrap().unwrap().locked);

        assert!(!store
            .set_session_locked_unless_archived("archived")
            .unwrap());
        assert!(!store.get_session("archived").unwrap().unwrap().locked);

        assert!(store.set_session_locked_unless_archived("nobody").unwrap());
        assert!(store.get_session("nobody").unwrap().is_none());
    }

    /// The owner notice count is marked, read and cleared by session, a clear naming the count it
    /// saw.
    #[test]
    fn the_owner_notice_flag_follows_the_session() {
        let path = temp_db("owner-notice");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store.insert_session(&session(None)).expect("session");
        assert_eq!(store.owner_notice_pending("session-1").unwrap(), 0);
        assert_eq!(store.owner_notice_note("session-1").unwrap(), None);
        store.mark_owner_notice("session-1").unwrap();
        let seen = store.owner_notice_pending("session-1").unwrap();
        assert_eq!(seen, 1);
        // A change marked after the launch read the count is not cleared by that launch.
        store.mark_owner_notice("session-1").unwrap();
        store.clear_owner_notice("session-1", seen).unwrap();
        assert_eq!(store.owner_notice_pending("session-1").unwrap(), 2);
        store.clear_owner_notice("session-1", 2).unwrap();
        assert_eq!(store.owner_notice_pending("session-1").unwrap(), 0);
        assert_eq!(store.owner_notice_pending("nobody").unwrap(), 0);
    }

    /// Information held for an interrupted session is read oldest first, survives the store being
    /// opened again, is dropped by id or for the whole session, and goes with the session.
    #[test]
    fn information_held_for_a_session_survives_reopening_and_goes_with_the_session() {
        let path = temp_db("pending-info");
        {
            let store = Store::open(&path).expect("store");
            store
                .insert_console(&console(None, None, None))
                .expect("console");
            store.insert_session(&session(None)).expect("session");
            store.add_kept_message("session-1", "one").unwrap();
            store.add_kept_message("session-1", "two").unwrap();
        }
        let store = Store::open(&path).expect("reopened");
        let held = store.kept_messages("session-1").unwrap();
        assert_eq!(
            held.iter()
                .map(|(_, text)| text.as_str())
                .collect::<Vec<_>>(),
            ["one", "two"]
        );
        store.delete_kept_messages(&[held[0].0]).unwrap();
        assert_eq!(store.kept_messages("session-1").unwrap().len(), 1);
        store.add_kept_message("session-1", "three").unwrap();
        store.clear_kept_messages("session-1").unwrap();
        assert!(store.kept_messages("session-1").unwrap().is_empty());
        store.add_kept_message("session-1", "four").unwrap();
        store.delete_session("session-1").unwrap();
        assert!(store.kept_messages("session-1").unwrap().is_empty());
    }

    /// A binding is cleared only while the session is still bound to the owner the caller read,
    /// and the sessions bound to it keep it.
    #[test]
    fn a_binding_is_cleared_only_while_it_is_still_the_one_read() {
        let path = temp_db("clear-binding");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store
            .insert_session(&session(None))
            .expect("console session");
        store
            .insert_session(&project_session("lead", None))
            .expect("lead");
        store
            .insert_session(&project_session("child", Some("lead")))
            .expect("child");
        store
            .set_binding("lead", None, "session-1", true, false, false)
            .expect("binding")
            .expect("lead bound");

        assert!(store
            .clear_binding("lead", "elsewhere", false, None)
            .unwrap()
            .is_none());
        assert!(store
            .clear_binding("child", "elsewhere", false, None)
            .unwrap()
            .is_none());
        let cleared = store
            .clear_binding("lead", "session-1", true, Some("good luck"))
            .unwrap()
            .expect("cleared");
        assert_eq!((cleared.bound_to, cleared.lead), (None, false));
        // The mark lands with the change, and only with a change that landed.
        assert_eq!(store.owner_notice_pending("lead").unwrap(), 1);
        assert_eq!(store.owner_notice_pending("child").unwrap(), 0);
        // The parting note is kept with the mark, for the release that is still the latest change.
        assert_eq!(
            store.owner_notice_note("lead").unwrap().as_deref(),
            Some("good luck")
        );
        store
            .set_binding("lead", None, "session-1", false, false, true)
            .unwrap()
            .expect("bound again");
        assert_eq!(store.owner_notice_pending("lead").unwrap(), 2);
        assert_eq!(store.owner_notice_note("lead").unwrap(), None);
        // A release with nobody to be told keeps no note.
        assert!(store
            .clear_binding("lead", "session-1", false, Some("dropped"))
            .unwrap()
            .is_some());
        assert_eq!(store.owner_notice_note("lead").unwrap(), None);
        assert!(store
            .clear_binding("lead", "session-1", true, Some("good luck"))
            .unwrap()
            .is_none());
        assert_eq!(store.owner_notice_pending("lead").unwrap(), 2);
        // The note goes with the count that delivered it, and stays with a newer mark.
        store
            .set_binding("lead", None, "session-1", false, false, false)
            .unwrap()
            .expect("bound once more");
        store
            .clear_binding("lead", "session-1", true, Some("good luck"))
            .unwrap()
            .expect("released again");
        store.clear_owner_notice("lead", 2).unwrap();
        assert_eq!(store.owner_notice_pending("lead").unwrap(), 3);
        assert!(store.owner_notice_note("lead").unwrap().is_some());
        store.clear_owner_notice("lead", 3).unwrap();
        assert_eq!(store.owner_notice_note("lead").unwrap(), None);
        // A failed write to a running session marks the notice with the note, while unbound only.
        store.mark_owner_notice_with_note("lead", "kept").unwrap();
        assert_eq!(
            store.owner_notice_note("lead").unwrap().as_deref(),
            Some("kept")
        );
        store
            .set_binding("lead", None, "session-1", false, false, false)
            .unwrap()
            .expect("bound again");
        store.mark_owner_notice_with_note("lead", "stale").unwrap();
        assert_eq!(store.owner_notice_note("lead").unwrap(), None);
        let child = store.get_session("child").unwrap().unwrap();
        assert_eq!(child.bound_to.as_deref(), Some("lead"));
        // A console session is never bound, so nothing to clear.
        assert!(store
            .clear_binding("session-1", "lead", false, None)
            .unwrap()
            .is_none());
    }

    /// A console session to feed [`Store::insert_console_session`]: a plain `role: Console` record
    /// whose colour, ordinal and title that call is going to overwrite, distinguished only by `id`.
    fn console_session(id: &str) -> Session {
        let mut built = session(None);
        built.id = id.to_string();
        built
    }

    fn open_console_session(store: &Store, id: &str) -> ConsoleSessionColour {
        store
            .insert_console_session(console_session(id), None)
            .expect("inserted")
            .colour
            .expect("a console session always gets a colour")
    }

    /// While a free palette entry remains, a new console session never takes one a live console
    /// session of its console holds: opening as many as the palette has entries hands out each
    /// entry exactly once, whatever the random picks were.
    #[test]
    fn a_console_sessions_colour_is_never_one_already_held_while_one_is_free() {
        let path = temp_db("colour-assignment");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");

        let palette = ConsoleSessionColour::PALETTE;
        let mut assigned: Vec<ConsoleSessionColour> = (0..palette.len())
            .map(|i| open_console_session(&store, &format!("session-{i}")))
            .collect();
        assigned.sort_by_key(|colour| palette.iter().position(|entry| entry == colour));
        assert_eq!(assigned, palette);
    }

    /// The pick is uniform over the free entries: every roll lands on a free entry, and the rolls
    /// together reach each of them.
    #[test]
    fn assign_picks_only_among_the_free_entries() {
        let palette = ConsoleSessionColour::PALETTE;
        let held = [palette[0], palette[palette.len() - 1]];
        let picks: Vec<ConsoleSessionColour> = (0..palette.len() * 3)
            .map(|roll| assign_console_session_colour(&held, roll))
            .collect();
        assert!(picks.iter().all(|pick| !held.contains(pick)));
        for entry in palette {
            assert_eq!(
                picks.contains(&entry),
                !held.contains(&entry),
                "{entry:?} is reachable exactly when it is free"
            );
        }
    }

    /// Once every palette entry is held, assignment picks among all of them rather than refusing
    /// — the badge is a convenience, not a promise of uniqueness once the console runs more
    /// console sessions at once than the palette has room for.
    #[test]
    fn assign_picks_among_the_whole_palette_once_every_entry_is_held() {
        let held = ConsoleSessionColour::PALETTE;
        let picks: Vec<ConsoleSessionColour> = (0..held.len())
            .map(|roll| assign_console_session_colour(&held, roll))
            .collect();
        assert_eq!(picks, held);

        let path = temp_db("colour-wrap");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        for i in 0..held.len() {
            open_console_session(&store, &format!("session-{i}"));
        }
        // With every entry held, the next open is still not refused and still gets a colour.
        open_console_session(&store, "session-overflow");
    }

    /// Archiving a console session frees its colour for reuse — only a *non-archived* console
    /// session's colour counts as held, since the badge only ever needs to tell apart the console
    /// sessions currently showing side by side.
    #[test]
    fn an_archived_console_sessions_colour_is_free_for_reuse() {
        let path = temp_db("colour-archive-frees");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        let mut sessions: Vec<Session> = (0..ConsoleSessionColour::PALETTE.len())
            .map(|i| {
                store
                    .insert_console_session(console_session(&format!("session-{i}")), None)
                    .expect("inserted")
            })
            .collect();
        let mut archived = sessions.remove(0);
        let freed = archived.colour.expect("a colour");
        archived.status = SessionStatus::Archived;
        store.update_session(&archived).expect("archived");

        assert_eq!(
            open_console_session(&store, "session-next"),
            freed,
            "the archived console session's colour is the only free entry, so it is the pick"
        );
    }

    /// A colour the user chose replaces the assigned one, survives a stale write-back of the
    /// record, counts as held for the next console session, and is ignored for a project session.
    #[test]
    fn a_chosen_console_session_colour_is_kept_and_counts_as_held() {
        let path = temp_db("colour-chosen");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        let first = store
            .insert_console_session(console_session("session-0"), None)
            .expect("inserted");
        let chosen = ConsoleSessionColour::PALETTE
            .into_iter()
            .find(|entry| Some(*entry) != first.colour)
            .unwrap();

        store.set_session_colour("session-0", chosen).unwrap();
        assert!(store.update_session(&first).unwrap());
        let reread = store.get_session("session-0").unwrap().unwrap();
        assert_eq!(reread.colour, Some(chosen));

        // The other entries are all free but the chosen one: the next picks never take it.
        for i in 1..ConsoleSessionColour::PALETTE.len() {
            assert_ne!(
                open_console_session(&store, &format!("session-{i}")),
                chosen,
                "the chosen colour is held"
            );
        }

        let mut project_session = session(None);
        project_session.id = "session-project".to_string();
        project_session.role = Role::Project;
        store.insert_session(&project_session).expect("insert");
        store.set_session_colour("session-project", chosen).unwrap();
        assert_eq!(
            store
                .get_session("session-project")
                .unwrap()
                .unwrap()
                .colour,
            None
        );
    }

    /// The ordinal is one past the highest ever handed out in this console, kept on the console
    /// record itself — not derived from which console sessions still exist — so deleting the one
    /// that held the highest ordinal does not let a later console session reuse it.
    #[test]
    fn a_console_sessions_ordinal_is_never_reused_even_after_its_session_is_deleted() {
        let path = temp_db("ordinal-high-water-mark");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");

        let first = store
            .insert_console_session(console_session("session-0"), None)
            .expect("inserted");
        let second = store
            .insert_console_session(console_session("session-1"), None)
            .expect("inserted");
        assert_eq!(first.ordinal, Some(1));
        assert_eq!(second.ordinal, Some(2));

        let mut archived = second;
        archived.status = SessionStatus::Archived;
        store.update_session(&archived).expect("archived");
        assert!(store
            .delete_session_if_archived(&archived.id)
            .expect("delete")
            .is_some());

        let third = store
            .insert_console_session(console_session("session-2"), None)
            .expect("inserted");
        assert_eq!(
            third.ordinal,
            Some(3),
            "ordinal 2 was used and must not be handed out again, even though session-1 is gone"
        );
    }

    /// Defaults use the console name, including for later sessions; an explicit title is kept.
    #[test]
    fn a_console_sessions_default_title_uses_its_console_name() {
        let path = temp_db("console-session-title");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");

        let defaulted = store
            .insert_console_session(console_session("session-0"), None)
            .expect("inserted");
        assert_eq!(defaulted.title, "Console");

        let second = store
            .insert_console_session(console_session("session-0b"), None)
            .expect("inserted");
        assert_eq!(second.title, "Console");

        let mut renamed = console(None, None, None);
        renamed.name = "Renamed console".to_string();
        store.update_console(&renamed).expect("renamed");
        let after_rename = store
            .insert_console_session(console_session("session-0c"), None)
            .expect("inserted");
        assert_eq!(after_rename.title, "Renamed console");
        assert_eq!(
            store.get_session(&defaulted.id).unwrap().unwrap().title,
            "Console"
        );

        let named = store
            .insert_console_session(
                console_session("session-1"),
                Some("Investigate the outage".to_string()),
            )
            .expect("inserted");
        assert_eq!(named.title, "Investigate the outage");
    }

    /// Pages belong to the console session that pushed them: listing and "the newest" are per
    /// console session even within one console, a tie on `created_at` still resolves to the same
    /// page in both, and deleting a console session takes its pages and no one else's.
    #[test]
    fn pages_are_scoped_to_their_console_session_and_deleted_with_it() {
        let path = temp_db("pages-per-console-session");
        let store = Store::open(&path).expect("store");
        store.insert_console(&console(None, None, None)).unwrap();
        for id in ["session-a", "session-b"] {
            store.insert_session(&console_session(id)).unwrap();
        }
        for page in [
            crate::test_support::test_page("a-1", "session-a", 1),
            crate::test_support::test_page("b-1", "session-b", 2),
            crate::test_support::test_page("a-2", "session-a", 3),
            crate::test_support::test_page("a-3", "session-a", 3),
        ] {
            store.insert_page(&page).unwrap();
        }

        let ids = |owner: &str| -> Vec<String> {
            store
                .list_pages(owner)
                .unwrap()
                .into_iter()
                .map(|page| page.id)
                .collect()
        };
        assert_eq!(ids("session-a"), ["a-1", "a-2", "a-3"]);
        assert_eq!(ids("session-b"), ["b-1"]);
        assert_eq!(
            store.newest_page_id("session-a").unwrap().as_deref(),
            Some("a-3")
        );
        assert_eq!(
            store.newest_page_id("session-b").unwrap().as_deref(),
            Some("b-1")
        );

        store.delete_session("session-a").unwrap();
        assert!(ids("session-a").is_empty());
        assert_eq!(ids("session-b"), ["b-1"]);

        store.delete_console("console-1").unwrap();
        assert!(ids("session-b").is_empty());
        assert!(store.get_page("b-1").unwrap().is_none());
    }

    /// `delete_pages` checks and removes in one transaction: it reports only what it removed, a
    /// foreign or already-deleted id rolls the whole call back, and a repeated delete of the same
    /// page reports it once.
    #[test]
    fn delete_pages_reports_only_what_it_removed_and_rolls_back_on_a_foreign_id() {
        let path = temp_db("delete-pages");
        let store = Store::open(&path).expect("store");
        store.insert_console(&console(None, None, None)).unwrap();
        for id in ["session-a", "session-b"] {
            store.insert_session(&console_session(id)).unwrap();
        }
        for page in [
            crate::test_support::test_page("a-1", "session-a", 1),
            crate::test_support::test_page("a-2", "session-a", 2),
            crate::test_support::test_page("b-1", "session-b", 3),
        ] {
            store.insert_page(&page).unwrap();
        }
        let named = |ids: &[&str]| ids.iter().map(|id| id.to_string()).collect::<Vec<_>>();

        // A foreign id after an own one undoes the own one.
        let refused = store
            .delete_pages("session-a", Some(&named(&["a-1", "b-1"])))
            .unwrap();
        assert_eq!(refused, Err("b-1".to_string()));
        assert_eq!(store.list_pages("session-a").unwrap().len(), 2);
        assert_eq!(store.list_pages("session-b").unwrap().len(), 1);

        // A repeated id is removed, and reported, once.
        let done = store
            .delete_pages("session-a", Some(&named(&["a-2", "a-2"])))
            .unwrap();
        assert_eq!(done, Ok(named(&["a-2"])));
        // The same delete again finds it gone, so a second caller does not report it too.
        let again = store
            .delete_pages("session-a", Some(&named(&["a-2"])))
            .unwrap();
        assert_eq!(again, Err("a-2".to_string()));

        assert_eq!(
            store.delete_pages("session-a", None).unwrap(),
            Ok(named(&["a-1"]))
        );
        assert_eq!(store.delete_pages("session-a", None).unwrap(), Ok(vec![]));
        assert_eq!(store.list_pages("session-b").unwrap().len(), 1);
    }

    #[test]
    fn observation_boundaries_reject_stale_models_and_follow_the_retained_conversation() {
        let path = temp_db("model-boundary-source");
        let store = Store::open(&path).unwrap();
        store.insert_console(&console(None, None, None)).unwrap();
        let mut source = session(None);
        store.insert_session(&source).unwrap();
        let boundary = r#"{"files":{}}"#;
        store.reset_observed_model(&source.id, boundary).unwrap();
        let update = |boundary: Option<&str>| RecordUpdate {
            session: source.clone(),
            context: Some(Some(crate::protocol::SessionContext {
                tokens: 123,
                window: None,
                used_percent: None,
            })),
            titles: Some(RecordTitles {
                named: Some("Current title".to_string()),
                prompt: None,
            }),
            observed_model: Some("new-model".to_string()),
            observed_effort: Some(Some("high".to_string())),
            observed_boundary: boundary.map(str::to_string),
        };
        store.set_record_data(&[update(None)]).unwrap();
        let after = store.get_session(&source.id).unwrap().unwrap();
        assert!(after.observed_model.is_none());
        assert_eq!(after.context.unwrap().tokens, 123);
        assert_eq!(after.record_titles.named.as_deref(), Some("Current title"));
        store.set_record_data(&[update(Some(boundary))]).unwrap();
        assert_eq!(
            store
                .get_session(&source.id)
                .unwrap()
                .unwrap()
                .observed_model
                .as_deref(),
            Some("new-model")
        );
        source.status = SessionStatus::Archived;
        store.update_session(&source).unwrap();
        assert_eq!(
            store.observed_boundary(&source.id).unwrap().as_deref(),
            Some(boundary)
        );
        source.agent_session_id = Some("another-conversation".to_string());
        store.update_session(&source).unwrap();
        assert!(store.observed_boundary(&source.id).unwrap().is_none());
        store.reset_observed_model(&source.id, boundary).unwrap();
        store.delete_session(&source.id).unwrap();
        assert!(store.observed_boundary(&source.id).unwrap().is_none());
    }

    #[test]
    fn context_survives_unrelated_writes_and_rejects_stale_sources() {
        let path = temp_db("context");
        let store = Store::open(&path).unwrap();
        store.insert_console(&console(None, None, None)).unwrap();
        let mut source = session(Some("/account-a"));
        source.agent_session_id = Some("native-1".to_owned());
        store.insert_session(&source).unwrap();
        assert!(store
            .get_session(&source.id)
            .unwrap()
            .unwrap()
            .context
            .is_none());
        let context = crate::protocol::SessionContext {
            tokens: 120,
            window: Some(475_000),
            used_percent: Some(2),
        };
        let update = |session: &Session, context| RecordUpdate {
            observed_boundary: None,
            session: session.clone(),
            context,
            observed_model: None,
            observed_effort: None,
            titles: None,
        };
        assert_eq!(
            store
                .set_record_data(&[update(&source, Some(Some(context.clone())))])
                .unwrap()
                .len(),
            1
        );
        assert!(store
            .set_record_data(&[update(&source, Some(Some(context.clone())))])
            .unwrap()
            .is_empty());
        source.title = "Renamed".to_owned();
        source.status = SessionStatus::Archived;
        store.update_session(&source).unwrap();
        assert_eq!(
            store.get_session(&source.id).unwrap().unwrap().context,
            Some(context.clone())
        );
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(
            store.get_session(&source.id).unwrap().unwrap().context,
            Some(context.clone())
        );
        store
            .set_session_account(&source.id, None, Some("/account-b"))
            .unwrap();
        let changed = crate::protocol::SessionContext {
            tokens: 999,
            ..context.clone()
        };
        assert!(store
            .set_record_data(&[update(&source, Some(Some(changed)))])
            .unwrap()
            .is_empty());
        let mut current = store.get_session(&source.id).unwrap().unwrap();
        // The records can also say the size is not known now, which clears it.
        assert_eq!(
            store
                .set_record_data(&[update(&current, Some(None))])
                .unwrap()
                .len(),
            1
        );
        assert!(store
            .get_session(&source.id)
            .unwrap()
            .unwrap()
            .context
            .is_none());
        store
            .set_record_data(&[update(&current, Some(Some(context.clone())))])
            .unwrap();
        current.agent_session_id = Some("native-2".to_owned());
        store.update_session(&current).unwrap();
        assert!(store
            .get_session(&source.id)
            .unwrap()
            .unwrap()
            .context
            .is_none());
        assert!(store
            .set_record_data(&[update(&source, Some(Some(context)))])
            .unwrap()
            .is_empty());
    }

    #[test]
    fn record_titles_persist_and_follow_the_agent_session() {
        let path = temp_db("record-titles");
        let store = Store::open(&path).unwrap();
        store.insert_console(&console(None, None, None)).unwrap();
        store.insert_session(&session(None)).unwrap();
        let mut source = store.get_session("session-1").unwrap().unwrap();
        assert_eq!(source.record_titles, RecordTitles::default());
        let titles = RecordTitles {
            named: Some("Named".to_owned()),
            prompt: Some("Prompt".to_owned()),
        };
        let updates = [RecordUpdate {
            observed_boundary: None,
            session: source.clone(),
            context: None,
            observed_model: None,
            observed_effort: None,
            titles: Some(titles.clone()),
        }];
        assert_eq!(store.set_record_data(&updates).unwrap().len(), 1);
        assert!(store.set_record_data(&updates).unwrap().is_empty());
        source.title = "Renamed".to_owned();
        store.update_session(&source).unwrap();
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(
            store
                .get_session("session-1")
                .unwrap()
                .unwrap()
                .record_titles,
            titles
        );
        source.agent_session_id = Some("another".to_owned());
        store.update_session(&source).unwrap();
        assert_eq!(
            store
                .get_session("session-1")
                .unwrap()
                .unwrap()
                .record_titles,
            RecordTitles::default(),
            "another conversation does not inherit the titles"
        );
    }

    #[test]
    fn observed_model_and_effort_persist_and_follow_the_agent_session() {
        let path = temp_db("observed");
        let store = Store::open(&path).unwrap();
        store.insert_console(&console(None, None, None)).unwrap();
        store.insert_session(&session(None)).unwrap();
        let mut source = store.get_session("session-1").unwrap().unwrap();
        let updates = [RecordUpdate {
            observed_boundary: None,
            session: source.clone(),
            context: None,
            observed_model: Some("claude-opus-5-5".to_owned()),
            observed_effort: Some(Some("high".to_owned())),
            titles: None,
        }];
        assert_eq!(store.set_record_data(&updates).unwrap().len(), 1);
        assert!(store.set_record_data(&updates).unwrap().is_empty());
        source.title = "Renamed".to_owned();
        store.update_session(&source).unwrap();
        drop(store);
        let store = Store::open(&path).unwrap();
        let stored = store.get_session("session-1").unwrap().unwrap();
        assert_eq!(stored.observed_model.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(stored.observed_effort.as_deref(), Some("high"));
        let unset = [RecordUpdate {
            observed_boundary: None,
            session: stored,
            context: None,
            observed_model: None,
            observed_effort: Some(None),
            titles: None,
        }];
        assert_eq!(store.set_record_data(&unset).unwrap().len(), 1);
        let stored = store.get_session("session-1").unwrap().unwrap();
        assert_eq!(stored.observed_model.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(stored.observed_effort, None);
        assert!(store.set_record_data(&unset).unwrap().is_empty());
        source.agent_session_id = Some("another".to_owned());
        store.update_session(&source).unwrap();
        let stored = store.get_session("session-1").unwrap().unwrap();
        assert_eq!(
            (stored.observed_model, stored.observed_effort),
            (None, None)
        );
    }

    #[test]
    fn terminal_titles_survive_unrelated_writes_and_reopening() {
        let path = temp_db("terminal-title");
        let store = Store::open(&path).unwrap();
        store.insert_console(&console(None, None, None)).unwrap();
        store.insert_session(&session(None)).unwrap();
        let mut stale = store.get_session("session-1").unwrap().unwrap();
        assert!(stale.terminal_title.is_none());
        assert!(store
            .set_terminal_title("session-1", Some("Task title"))
            .unwrap()
            .is_some());
        assert!(store
            .set_terminal_title("session-1", Some("Task title"))
            .unwrap()
            .is_none());
        stale.title = "Renamed".to_string();
        stale.status = SessionStatus::Archived;
        store.update_session(&stale).unwrap();
        drop(store);
        let store = Store::open(&path).unwrap();
        let stored = store.get_session("session-1").unwrap().unwrap();
        assert_eq!(stored.terminal_title.as_deref(), Some("Task title"));
        assert_eq!(stored.title, "Renamed");
        assert_eq!(stored.status, SessionStatus::Archived);
        for status in [SessionStatus::Archived, SessionStatus::Interrupted] {
            let mut dormant = store.get_session("session-1").unwrap().unwrap();
            dormant.status = status;
            store.update_session(&dormant).unwrap();
            for title in [None, Some("Shutdown title")] {
                assert!(store
                    .set_terminal_title("session-1", title)
                    .unwrap()
                    .is_none());
                assert_eq!(
                    store
                        .get_session("session-1")
                        .unwrap()
                        .unwrap()
                        .terminal_title
                        .as_deref(),
                    Some("Task title"),
                    "cleanup cannot replace a dormant session's title"
                );
            }
        }
        let mut resumed = store.get_session("session-1").unwrap().unwrap();
        resumed.status = SessionStatus::Idle;
        store.update_session(&resumed).unwrap();
        assert_eq!(
            store
                .set_terminal_title("session-1", Some("Resumed task"))
                .unwrap()
                .unwrap()
                .terminal_title
                .as_deref(),
            Some("Resumed task")
        );
        assert!(store
            .set_terminal_title("session-1", None)
            .unwrap()
            .unwrap()
            .terminal_title
            .is_none());
        store.delete_session("session-1").unwrap();
        assert!(store
            .set_terminal_title("session-1", Some("Late title"))
            .unwrap()
            .is_none());
    }
}
