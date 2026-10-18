//! The coordinator's SQLite storage: consoles, projects, sessions, branch origins and the host
//! table.
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

use crate::protocol::{
    error_code, now_millis, Account, Agent, ApprovalReviewer, CodedError, Console,
    ConsoleSessionColour, Host, HostKind, Origin, Page, Project, ProjectAccount, ProjectSource,
    Role, Session, SessionStatus, Settings,
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
            supersede_if_outdated(path)?;
        }
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS hosts (
                id          TEXT PRIMARY KEY,
                name        TEXT NOT NULL,
                kind        TEXT NOT NULL,
                ssh_config  TEXT
            );
            CREATE TABLE IF NOT EXISTS accounts (
                id         TEXT PRIMARY KEY,
                agent      TEXT NOT NULL,
                name       TEXT NOT NULL,
                config_dir TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            -- Backstop for the rule that an account's name is required and unique within its
            -- agent: `coordinator` checks this before every write that can affect a name, but
            -- the invariant belongs on the table it protects, not only on the callers that happen
            -- to remember to check it. `LOWER` matches `coordinator::account_name_key`'s
            -- case-insensitive comparison; it is ASCII-only in SQLite, which is a narrower fold
            -- than `str::to_lowercase`, so this index is a backstop and not a full substitute for
            -- that check. A write that trips it surfaces through `Store::account_write_error` as
            -- the protocol's own `account_name_taken` rather than a raw SQLite error.
            CREATE UNIQUE INDEX IF NOT EXISTS accounts_agent_name_unique
                ON accounts (agent, LOWER(name));
            CREATE TABLE IF NOT EXISTS consoles (
                id            TEXT PRIMARY KEY,
                name          TEXT NOT NULL,
                workdir       TEXT NOT NULL,
                console_session_agent TEXT NOT NULL,
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
            CREATE TABLE IF NOT EXISTS projects (
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
                tags          TEXT NOT NULL DEFAULT '[]'
            );
            CREATE TABLE IF NOT EXISTS sessions (
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
                usage            TEXT,
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
                started_at       INTEGER NOT NULL,
                ended_at         INTEGER
            );
            CREATE TABLE IF NOT EXISTS trusted_directories (
                path TEXT PRIMARY KEY
            );
            CREATE TABLE IF NOT EXISTS pages (
                id                TEXT PRIMARY KEY,
                -- The console session that pushed the page; deleting it takes its pages, and
                -- deleting a console therefore takes all of them.
                console_session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                html              TEXT NOT NULL,
                anchor_message_id TEXT,
                created_at        INTEGER NOT NULL
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
            CREATE TABLE IF NOT EXISTS branch_origins (
                repository     TEXT NOT NULL,
                branch         TEXT NOT NULL,
                created_at     INTEGER NOT NULL,
                birth_commit   TEXT,
                birth_time     INTEGER,
                responsible_session_id TEXT,
                responsible_since      INTEGER,
                responsible_title      TEXT,
                PRIMARY KEY (repository, branch)
            );
            CREATE TABLE IF NOT EXISTS settings (
                id                     TEXT PRIMARY KEY,
                auto_pull_repositories INTEGER NOT NULL DEFAULT 0,
                -- NULL until the user sets one; see `Store::default_clone_dir`.
                default_clone_dir      TEXT
            );
            "#,
        )?;
        // Older milestone schemas are moved aside above. Compatible additions are migrated in
        // place so consoles, projects, accounts and sessions keep their stored state.
        //
        // Branch origins once recorded the creating session apart from the responsible one. The
        // creator is not kept: it becomes the responsible session, with its title, where none was
        // recorded; a session the branch was handed to takes its own title, cut as a record cuts
        // it, if it still exists, else the creator's. The creator's columns go.
        //
        // They once came from heuristics as well as from the session's own git, and a row of one
        // of those may name the wrong session; only the git's own report is kept, so that every
        // origin shown is one the session's git made, and the column saying where a row came from
        // goes with the rest.
        //
        // One transaction, so that an interrupted migration cannot leave a half-moved table.
        let tx = conn.unchecked_transaction()?;
        let creator_recorded = column_exists(&tx, "branch_origins", "session_id")?;
        if creator_recorded {
            tx.execute(
                "DELETE FROM branch_origins WHERE source <> 'ref_hook' OR session_id IS NULL
                     OR session_title IS NULL OR birth_commit IS NULL OR birth_time IS NULL",
                [],
            )?;
        }
        for (column, definition) in [
            ("responsible_session_id", "TEXT"),
            ("responsible_since", "INTEGER"),
            ("responsible_title", "TEXT"),
        ] {
            if !column_exists(&tx, "branch_origins", column)? {
                tx.execute(
                    &format!("ALTER TABLE branch_origins ADD COLUMN {column} {definition}"),
                    [],
                )?;
            }
        }
        if creator_recorded {
            tx.execute(
                "UPDATE branch_origins SET responsible_session_id = session_id,
                     responsible_since = created_at
                 WHERE responsible_session_id IS NULL OR responsible_since IS NULL",
                [],
            )?;
            tx.execute(
                &format!(
                    "UPDATE branch_origins SET responsible_title = COALESCE(
                         (SELECT substr(sessions.title, 1, {}) FROM sessions
                          WHERE sessions.id = responsible_session_id
                            AND responsible_session_id <> session_id),
                         session_title)
                     WHERE responsible_title IS NULL",
                    crate::browse::budget::MAX_ORIGIN_TITLE_CHARS
                ),
                [],
            )?;
            for column in ["session_id", "session_title"] {
                tx.execute(
                    &format!("ALTER TABLE branch_origins DROP COLUMN {column}"),
                    [],
                )?;
            }
        }
        if column_exists(&tx, "branch_origins", "source")? {
            tx.execute("ALTER TABLE branch_origins DROP COLUMN source", [])?;
        }
        tx.commit()?;
        if !column_exists(&conn, "settings", "default_clone_dir")? {
            conn.execute("ALTER TABLE settings ADD COLUMN default_clone_dir TEXT", [])?;
        }
        for (table, column) in [
            ("accounts", "approval_reviewer"),
            ("settings", "codex_default_approval_reviewer"),
        ] {
            if !column_exists(&conn, table, column)? {
                conn.execute(&format!("ALTER TABLE {table} ADD COLUMN {column} TEXT"), [])?;
            }
        }
        // The first palette had six labels; a branch's first later palette added plain labels (all
        // of them families now but `purple`), and its second wrote `<family>-<shade>`. Every label
        // of those is rewritten to the family it is nearest to (by hue) once, so a stored label
        // always reads.
        for (old, new) in LEGACY_CONSOLE_SESSION_COLOURS {
            conn.execute(
                "UPDATE sessions SET colour = ?2 WHERE colour = ?1",
                params![old, new],
            )?;
        }
        conn.execute(
            "UPDATE sessions SET colour = substr(colour, 1, length(colour) - 2)
             WHERE colour GLOB '*-[123]'",
            [],
        )?;
        if !column_exists(&conn, "sessions", "lead")? {
            conn.execute(
                "ALTER TABLE sessions ADD COLUMN lead INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        if !column_exists(&conn, "sessions", "usage")? {
            conn.execute("ALTER TABLE sessions ADD COLUMN usage TEXT", [])?;
        }
        if !column_exists(&conn, "sessions", "terminal_title")? {
            conn.execute("ALTER TABLE sessions ADD COLUMN terminal_title TEXT", [])?;
        }
        for column in ["record_title", "record_prompt"] {
            if !column_exists(&conn, "sessions", column)? {
                conn.execute(
                    &format!("ALTER TABLE sessions ADD COLUMN {column} TEXT"),
                    [],
                )?;
            }
        }
        for column in ["model", "effort"] {
            if !column_exists(&conn, "sessions", column)? {
                conn.execute(
                    &format!("ALTER TABLE sessions ADD COLUMN {column} TEXT"),
                    [],
                )?;
            }
        }
        for column in ["model_override", "effort_override", "locked"] {
            if !column_exists(&conn, "sessions", column)? {
                conn.execute(
                    &format!("ALTER TABLE sessions ADD COLUMN {column} INTEGER NOT NULL DEFAULT 0"),
                    [],
                )?;
            }
        }
        conn.execute_batch(
            "CREATE TRIGGER IF NOT EXISTS protect_locked_session_delete
             BEFORE DELETE ON sessions WHEN OLD.locked = 1
             BEGIN SELECT RAISE(ABORT, 'locked sessions cannot be deleted'); END;
             CREATE TRIGGER IF NOT EXISTS protect_locked_session_archive
             BEFORE UPDATE OF status ON sessions
             WHEN OLD.locked = 1 AND NEW.status = 'archived' AND OLD.status != 'archived'
             BEGIN SELECT RAISE(ABORT, 'locked sessions cannot be archived'); END;",
        )?;
        for column in CONSOLE_PROJECT_SESSION_LAUNCH_COLUMNS {
            if !column_exists(&conn, "consoles", column)? {
                conn.execute(
                    &format!("ALTER TABLE consoles ADD COLUMN {column} TEXT"),
                    [],
                )?;
            }
        }
        for column in CONSOLE_SESSION_LAUNCH_COLUMNS {
            if !column_exists(&conn, "consoles", column)? {
                conn.execute(
                    &format!("ALTER TABLE consoles ADD COLUMN {column} TEXT"),
                    [],
                )?;
            }
        }
        if !column_exists(&conn, "consoles", "project_session_agent")? {
            conn.execute(
                "ALTER TABLE consoles ADD COLUMN project_session_agent TEXT",
                [],
            )?;
        }
        if !column_exists(&conn, "consoles", "console_session_prompt")? {
            conn.execute(
                "ALTER TABLE consoles ADD COLUMN console_session_prompt TEXT",
                [],
            )?;
        }
        for column in PROJECT_SETTING_COLUMNS {
            if !column_exists(&conn, "projects", column)? {
                conn.execute(
                    &format!("ALTER TABLE projects ADD COLUMN {column} TEXT"),
                    [],
                )?;
            }
        }
        for column in PROJECT_ACCOUNT_DEFAULT_COLUMNS {
            if !column_exists(&conn, "projects", column)? {
                conn.execute(
                    &format!("ALTER TABLE projects ADD COLUMN {column} INTEGER NOT NULL DEFAULT 0"),
                    [],
                )?;
            }
        }
        // The legacy console session agent column mirrors the console's default and is no longer
        // read for agent selection. Rows from before that may differ; later opens are a no-op.
        conn.execute(
            "UPDATE consoles SET console_session_agent = default_agent
             WHERE console_session_agent <> default_agent",
            [],
        )?;
        if column_exists(&conn, "projects", "claude_trust_consent")? {
            conn.execute(
                "ALTER TABLE projects RENAME COLUMN claude_trust_consent TO trust_consent",
                [],
            )?;
        }
        // A database from an earlier build keeps this setting under its older name; renaming the
        // column in place keeps the saved value.
        if column_exists(&conn, "settings", "auto_sync_repositories")? {
            conn.execute(
                "ALTER TABLE settings \
                 RENAME COLUMN auto_sync_repositories TO auto_pull_repositories",
                [],
            )?;
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
            "INSERT INTO consoles (id, name, workdir, console_session_agent, default_agent, claude_account_id,
                                   codex_account_id, grok_account_id, claude_console_session_model,
                                   claude_console_session_effort, codex_console_session_model,
                                   codex_console_session_effort, grok_console_session_model,
                                   grok_console_session_effort, icon, console_session_prompt,
                                   created_at, project_session_agent,
                                   claude_project_session_model, claude_project_session_effort,
                                   codex_project_session_model, codex_project_session_effort,
                                   grok_project_session_model, grok_project_session_effort)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                     ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
            params![
                console.id,
                console.name,
                console.workdir,
                enum_to_text(&console.console_session_agent),
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
            "UPDATE consoles SET name = ?2, console_session_agent = ?3, default_agent = ?4,
                                 claude_account_id = ?5, codex_account_id = ?6,
                                 grok_account_id = ?7, claude_console_session_model = ?8,
                                 claude_console_session_effort = ?9,
                                 codex_console_session_model = ?10,
                                 codex_console_session_effort = ?11,
                                 grok_console_session_model = ?12,
                                 grok_console_session_effort = ?13, icon = ?14,
                                 console_session_prompt = ?15, project_session_agent = ?16,
                                 claude_project_session_model = ?17,
                                 claude_project_session_effort = ?18,
                                 codex_project_session_model = ?19,
                                 codex_project_session_effort = ?20,
                                 grok_project_session_model = ?21,
                                 grok_project_session_effort = ?22
             WHERE id = ?1",
            params![
                console.id,
                console.name,
                enum_to_text(&console.console_session_agent),
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
                "INSERT INTO accounts (id, agent, name, config_dir, created_at, approval_reviewer) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    account.id,
                    enum_to_text(&account.agent),
                    account.name,
                    account.config_dir,
                    now_millis(),
                    account.approval_reviewer.as_ref().map(enum_to_text),
                ],
            )
            .map_err(|err| account_write_error(err, account))?;
        Ok(())
    }

    /// Renames the account, repoints it, or both — whatever the caller already decided to write;
    /// this does not itself check the name for a collision, which is `coordinator`'s business. The
    /// table's own `accounts_agent_name_unique` index is the backstop for that check, not a
    /// replacement for it.
    pub fn update_account(&self, account: &Account) -> Result<()> {
        self.lock()
            .execute(
                "UPDATE accounts SET name = ?2, config_dir = ?3 WHERE id = ?1",
                params![account.id, account.name, account.config_dir],
            )
            .map_err(|err| account_write_error(err, account))?;
        Ok(())
    }

    pub fn set_account_approval_reviewer(
        &self,
        id: &str,
        reviewer: Option<ApprovalReviewer>,
    ) -> Result<()> {
        self.lock().execute(
            "UPDATE accounts SET approval_reviewer = ?2 WHERE id = ?1",
            params![id, reviewer.as_ref().map(enum_to_text)],
        )?;
        Ok(())
    }

    pub fn get_account(&self, id: &str) -> Result<Option<Account>> {
        let account = self
            .lock()
            .query_row(
                "SELECT id, agent, name, config_dir, approval_reviewer FROM accounts WHERE id = ?1",
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
            conn.prepare("SELECT id, agent, name, config_dir, approval_reviewer FROM accounts ORDER BY created_at")?;
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
                                   grok_session_effort, trust_consent, pinned, tags)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                     ?18, ?19, ?20, ?21, ?22, ?23)",
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
                                 pinned = ?16, tags = ?17
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

    /// Whether this console already has a project pointing at `path`. Associating a parent
    /// directory discovers repositories in bulk and is expected to be re-run as new ones appear,
    /// so the duplicates it would otherwise create are filtered on this.
    pub fn project_exists_at(&self, console_id: &str, path: &str) -> Result<bool> {
        // Compared written one way: a row stored before project paths were normalised, such as
        // `/work/project/`, is the same project as a new `/work/project`.
        let wanted = crate::hostfs::lexically_normalise(Path::new(path));
        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT path FROM projects WHERE console_id = ?1")?;
        let stored = stmt
            .query_map(params![console_id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(stored
            .iter()
            .any(|stored| crate::hostfs::lexically_normalise(Path::new(stored)) == wanted))
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
        Ok(Settings {
            auto_pull_repositories: self.auto_pull_repositories()?,
            default_clone_dir: self.default_clone_dir()?,
            accounts: self.list_accounts()?,
            codex_default_approval_reviewer: self.codex_default_approval_reviewer()?,
        })
    }

    pub fn codex_default_approval_reviewer(&self) -> Result<Option<ApprovalReviewer>> {
        Ok(self.lock().query_row(
            "SELECT codex_default_approval_reviewer FROM settings WHERE id = ?1",
            params![SETTINGS_ID],
            |row| enum_from_row_opt(row, 0),
        )?)
    }

    pub fn set_codex_default_approval_reviewer(
        &self,
        reviewer: Option<ApprovalReviewer>,
    ) -> Result<()> {
        self.lock().execute(
            "UPDATE settings SET codex_default_approval_reviewer = ?2 WHERE id = ?1",
            params![SETTINGS_ID, reviewer.as_ref().map(enum_to_text)],
        )?;
        Ok(())
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

    /// The directory a clone lands in when none is named: the stored one, else `~/Projects`
    /// expanded.
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
    pub fn list_pages(&self, console_session_id: &str) -> Result<Vec<Page>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, console_session_id, html, anchor_message_id, created_at
             FROM pages WHERE console_session_id = ?1 ORDER BY created_at, id",
        )?;
        let rows = stmt
            .query_map(params![console_session_id], read_page)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// The id of the console session's newest page, or `None` if it has none yet. The ordering is the exact
    /// reverse of [`Self::list_pages`]'s, so the two never disagree about which page is newest when
    /// `created_at` ties.
    pub fn newest_page_id(&self, console_session_id: &str) -> Result<Option<String>> {
        let id = self
            .lock()
            .query_row(
                "SELECT id FROM pages WHERE console_session_id = ?1 ORDER BY created_at DESC, id DESC LIMIT 1",
                params![console_session_id],
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

    /// Reads the current console name without allocating a session ordinal.
    pub fn preview_console_session_title(&self, console_id: &str) -> Result<String> {
        self.get_console(console_id)?
            .map(|console| console.name)
            .ok_or_else(|| CodedError::unknown_console(console_id))
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

    /// Writes back the fields that change over a session's life. Identity and placement
    /// (`console_id`, `project_id`, `role`, `origin`, `ordinal`, `model`, `effort`)
    /// never change; `bound_to` and `lead` change only through [`Self::set_binding`], and
    /// `account_id` and `config_dir` change only through [`Self::set_session_account`], the one
    /// write path a switch of the session's account uses, so a record read before a switch can
    /// never write the old account back over it. `pinned` and `locked` are the user's own statements
    /// ([`Self::set_session_pinned`] and [`Self::set_session_locked`]), which a record read earlier
    /// must not overwrite, and neither may it overwrite `colour` ([`Self::set_session_colour`]).
    /// Returns whether a row was written: `false` means the session is gone (deleted meanwhile).
    pub fn update_session(&self, session: &Session) -> Result<bool> {
        let changed = self.lock().execute(
            "UPDATE sessions SET agent = ?2, agent_session_id = ?3, title = ?4, status = ?5,
                                 has_conversation = ?6, started_at = ?7, ended_at = ?8,
                                 usage = CASE WHEN agent = ?2 AND agent_session_id IS ?3 THEN usage ELSE NULL END,
                                 record_title = CASE WHEN agent = ?2 AND agent_session_id IS ?3 THEN record_title ELSE NULL END,
                                 record_prompt = CASE WHEN agent = ?2 AND agent_session_id IS ?3 THEN record_prompt ELSE NULL END
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
            let usage = update
                .usage
                .as_ref()
                .filter(|usage| session.usage.as_ref() != Some(*usage));
            let titles = update
                .titles
                .as_ref()
                .filter(|titles| &session.record_titles != *titles);
            if usage.is_none() && titles.is_none() {
                continue;
            }
            if let Some(usage) = usage {
                tx.execute(
                    "UPDATE sessions SET usage = ?2 WHERE id = ?1",
                    params![source.id, serde_json::to_string(usage)?],
                )?;
                session.usage = Some(usage.clone());
            }
            if let Some(titles) = titles {
                tx.execute(
                    "UPDATE sessions SET record_title = ?2, record_prompt = ?3 WHERE id = ?1",
                    params![source.id, titles.named, titles.prompt],
                )?;
                session.record_titles = titles.clone();
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
    /// writing — which an [`Self::update_session`] after a separate [`Self::get_session`] can be.
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
    /// changed. What else `owner` may be is the caller's to check.
    pub fn set_binding(
        &self,
        id: &str,
        from: Option<&str>,
        owner: &str,
        lead: bool,
        childless: bool,
    ) -> Result<Option<Session>> {
        let conn = self.lock();
        let changed = conn.execute(
            "UPDATE sessions SET bound_to = ?3, lead = ?4
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
                childless
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

    pub fn set_session_locked(&self, id: &str, locked: bool) -> Result<()> {
        self.lock().execute(
            "UPDATE sessions SET locked = ?2 WHERE id = ?1",
            params![id, locked],
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

    /// Sets the colour of a console session. Like `pinned`, it is the user's own statement, which
    /// [`Self::update_session`] never writes back.
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

    /// Deletes the session only while it is unused: it has had no turn, is bound to no owner and
    /// has no session bound to it, whatever its status. The returned identity comes from the same
    /// statement as the delete, so native-record cleanup cannot use an earlier hook snapshot.
    pub fn delete_session_if_unused(&self, id: &str) -> Result<Option<Session>> {
        Ok(self
            .lock()
            .query_row(
                &format!(
                    "DELETE FROM sessions
                     WHERE id = ?1 AND has_conversation = 0 AND bound_to IS NULL
                       AND NOT EXISTS (SELECT 1 FROM sessions WHERE bound_to = ?1)
                     RETURNING {SESSION_COLUMNS}"
                ),
                params![id],
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

    /// Every origin recorded for the repository.
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
                      responsible_session_id, responsible_since, responsible_title)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
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
                responsible_session_id, responsible_since, responsible_title
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
}

/// What [`Store::move_branch_responsibility`] did, by branch name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BranchMove {
    pub moved: Vec<String>,
    pub unmoved: Vec<String>,
}

/// Whether a table has a column of this name.
fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let found: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2",
        params![table, column],
        |row| row.get(0),
    )?;
    Ok(found > 0)
}

/// A column each table must already have for a database to be in the current shape, one entry per
/// schema change that has no migration. A table that does not exist at all is not outdated — it is
/// what `CREATE TABLE IF NOT EXISTS` is about to create fresh — so only a table that is present
/// without its column counts against the file.
const CURRENT_SHAPE_COLUMNS: [(&str, &str); 3] = [
    ("sessions", "bound_to"),
    ("sessions", "account_id"),
    ("pages", "console_session_id"),
];

/// Moves `path` aside and out of the way when it is a database from before this schema, so that
/// `Store::open`'s own `CREATE TABLE IF NOT EXISTS` always lands on either an empty file or one
/// already in the current shape, and runs no migration of its own. "Before this schema" is
/// detected cheaply and specifically, rather than by letting a later query fail deep in a request
/// path: a `sessions` table that predates the binding milestone 2 added has no `bound_to` column,
/// and one that predates the accounts milestone (4) has no `account_id` column — every build that
/// has ever carried one of those also carries the rest of the shape as of its own milestone, so
/// the two columns together tell an outdated table apart from a current one. A `pages` table that
/// still names the console rather than the console session (milestone 9) is outdated the same way;
/// see [`CURRENT_SHAPE_COLUMNS`]. A `projects` table whose trust permission column still has its
/// older name is not: `Store::open` renames that column in place, as it does a `settings` table's
/// automatic-pull column. Nor is a `consoles` or `projects` table without its per-agent launch or
/// account columns: `Store::open` adds those in place
/// (see [`CONSOLE_SESSION_LAUNCH_COLUMNS`] and [`PROJECT_SETTING_COLUMNS`]).
///
/// The old file is renamed rather than deleted, so a user who needs what was in it still has it on
/// disk; see "the files Octoboard keeps under `~/.octoboard`" in
/// `docs/product/application-lifecycle.md`.
fn supersede_if_outdated(path: &Path) -> Result<()> {
    {
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        let mut current = true;
        for (table, column) in CURRENT_SHAPE_COLUMNS {
            let table_exists: i64 = conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                params![table],
                |row| row.get(0),
            )?;
            if table_exists > 0 && !column_exists(&conn, table, column)? {
                current = false;
            }
        }
        if current {
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
        "the database at this path predates the current schema and has no migration to it; \
         moved it aside and starting a fresh one in its place"
    );
    Ok(())
}

const PROJECT_COLUMNS: &str = "id, console_id, host_id, name, path, default_agent, source,
                               remote_url, claude_account_id, codex_account_id, grok_account_id,
                               claude_account_default, codex_account_default, grok_account_default,
                               claude_session_model, claude_session_effort, codex_session_model,
                               codex_session_effort, grok_session_model, grok_session_effort,
                               trust_consent, pinned, tags";

/// The projects columns flagging that a project pins an agent's default account. Added in place
/// like [`PROJECT_SETTING_COLUMNS`] and absent from `CURRENT_SHAPE_COLUMNS` for the same reason;
/// a row from before them reads as clear, which is inheriting.
const PROJECT_ACCOUNT_DEFAULT_COLUMNS: [&str; 3] = [
    "claude_account_default",
    "codex_account_default",
    "grok_account_default",
];

/// The projects columns holding a project's per-agent account, model and reasoning effort. Added
/// in place to a database that predates them, and deliberately absent from
/// `CURRENT_SHAPE_COLUMNS`: listing them there would move every existing database aside.
const PROJECT_SETTING_COLUMNS: [&str; 9] = [
    "claude_account_id",
    "codex_account_id",
    "grok_account_id",
    "claude_session_model",
    "claude_session_effort",
    "codex_session_model",
    "codex_session_effort",
    "grok_session_model",
    "grok_session_effort",
];

/// What the agent's records gave a session, to be stored if it differs from what is.
pub struct RecordUpdate {
    /// The session as it was read, which says whose records these are.
    pub session: Session,
    pub usage: Option<crate::protocol::SessionUsage>,
    pub titles: Option<RecordTitles>,
}

const SESSION_COLUMNS: &str = "id, agent, agent_session_id, console_id, project_id, host_id,
                               role, origin, title, status, has_conversation, bound_to, colour,
                               ordinal, account_id, config_dir, pinned, started_at, ended_at,
                               lead, terminal_title, model, effort, model_override, effort_override, usage,
                               record_title, record_prompt, locked";

const CONSOLE_COLUMNS: &str = "id, name, workdir, console_session_agent, default_agent,
                               claude_account_id, codex_account_id, grok_account_id,
                               claude_console_session_model, claude_console_session_effort,
                               codex_console_session_model, codex_console_session_effort,
                               grok_console_session_model, grok_console_session_effort, icon,
                               console_session_prompt, created_at, project_session_agent,
                               claude_project_session_model, claude_project_session_effort,
                               codex_project_session_model, codex_project_session_effort,
                               grok_project_session_model, grok_project_session_effort";

/// The consoles columns holding the model and reasoning effort a project session falls back to.
/// Added in place like [`CONSOLE_SESSION_LAUNCH_COLUMNS`], and absent from
/// `CURRENT_SHAPE_COLUMNS` for the same reason.
const CONSOLE_PROJECT_SESSION_LAUNCH_COLUMNS: [&str; 6] = [
    "claude_project_session_model",
    "claude_project_session_effort",
    "codex_project_session_model",
    "codex_project_session_effort",
    "grok_project_session_model",
    "grok_project_session_effort",
];

/// The consoles columns holding the model and reasoning effort a console session is launched
/// with. Added in place to a database that predates them, and deliberately absent from
/// `CURRENT_SHAPE_COLUMNS`: listing them there would move every existing database aside.
const CONSOLE_SESSION_LAUNCH_COLUMNS: [&str; 6] = [
    "claude_console_session_model",
    "claude_console_session_effort",
    "codex_console_session_model",
    "codex_console_session_effort",
    "grok_console_session_model",
    "grok_console_session_effort",
];

/// The labels of the first release's palette that no family is named after, with the family each
/// is rewritten to when the store opens; its `teal` and `violet` are families too. `purple` only
/// ever existed on a branch.
const LEGACY_CONSOLE_SESSION_COLOURS: [(&str, &str); 5] = [
    ("olive", "lime"),
    ("jade", "green"),
    ("azure", "sky"),
    ("rose", "pink"),
    ("purple", "fuchsia"),
];

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
             started_at, ended_at, lead, terminal_title, model, effort, model_override, effort_override, usage,
             record_title, record_prompt, locked)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
                 ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29)",
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
            session.usage.as_ref().map(serde_json::to_string).transpose()?,
            session.record_titles.named,
            session.record_titles.prompt,
            session.locked,
        ],
    )?;
    Ok(())
}

fn read_console(row: &Row<'_>) -> rusqlite::Result<Console> {
    Ok(Console {
        id: row.get(0)?,
        name: row.get(1)?,
        workdir: row.get(2)?,
        console_session_agent: enum_from_row(row, 3)?,
        default_agent: enum_from_row(row, 4)?,
        claude_account_id: row.get(5)?,
        codex_account_id: row.get(6)?,
        grok_account_id: row.get(7)?,
        claude_console_session_model: row.get(8)?,
        claude_console_session_effort: row.get(9)?,
        codex_console_session_model: row.get(10)?,
        codex_console_session_effort: row.get(11)?,
        grok_console_session_model: row.get(12)?,
        grok_console_session_effort: row.get(13)?,
        icon: row.get(14)?,
        console_session_prompt: row.get(15)?,
        created_at: row.get(16)?,
        project_session_agent: enum_from_row_opt::<Agent>(row, 17)?,
        claude_project_session_model: row.get(18)?,
        claude_project_session_effort: row.get(19)?,
        codex_project_session_model: row.get(20)?,
        codex_project_session_effort: row.get(21)?,
        grok_project_session_model: row.get(22)?,
        grok_project_session_effort: row.get(23)?,
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

fn read_account(row: &Row<'_>) -> rusqlite::Result<Account> {
    Ok(Account {
        id: row.get(0)?,
        agent: enum_from_row(row, 1)?,
        name: row.get(2)?,
        config_dir: row.get(3)?,
        approval_reviewer: enum_from_row_opt(row, 4)?,
    })
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
        has_conversation: row.get(10)?,
        bound_to: row.get(11)?,
        colour: enum_from_row_opt::<ConsoleSessionColour>(row, 12)?,
        ordinal: row.get(13)?,
        account_id: row.get(14)?,
        config_dir: row.get(15)?,
        pinned: row.get(16)?,
        locked: row.get(28)?,
        started_at: row.get(17)?,
        ended_at: row.get(18)?,
        lead: row.get(19)?,
        terminal_title: row.get(20)?,
        record_titles: RecordTitles {
            named: row.get(26)?,
            prompt: row.get(27)?,
        },
        usage: row
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

fn tags_to_text(tags: &[String]) -> String {
    serde_json::to_string(tags).expect("a list of strings serializes to JSON")
}

/// A malformed value reads as no tags, with a warning, rather than failing the whole project read.
/// The daemon is the only writer, so this can only mean a hand-edited database.
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
    use crate::test_support::ScratchFile;

    fn temp_db(name: &str) -> ScratchFile {
        ScratchFile::new(&format!("store-{name}"), "octoboard.db")
    }

    fn console(claude: Option<&str>, codex: Option<&str>, grok: Option<&str>) -> Console {
        Console {
            id: "console-1".to_string(),
            name: "Console".to_string(),
            workdir: "/tmp/console-1".to_string(),
            console_session_agent: Agent::Claude,
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
            approval_reviewer: None,
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
            usage: None,
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

    /// A database written by a build before the binding (one that still has `sessions` rows but
    /// no `bound_to` column, as every build before this milestone does) is moved aside rather than
    /// opened in place: the file gains a `.superseded-<timestamp>` sibling holding the old bytes,
    /// and the daemon comes up on an empty database at the original path instead of failing deep
    /// in a later query with "no such column: bound_to".
    #[test]
    fn an_old_shape_database_is_moved_aside_and_the_daemon_comes_up_on_an_empty_one() {
        let path = temp_db("old-shape");
        {
            let conn = Connection::open(&path).expect("old database");
            conn.execute_batch(
                r#"
                CREATE TABLE consoles (
                    id TEXT PRIMARY KEY, name TEXT NOT NULL, workdir TEXT NOT NULL,
                    console_session_agent TEXT NOT NULL, default_agent TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );
                INSERT INTO consoles VALUES ('console-1', 'Old', '/tmp/old', 'claude', 'claude', 5);
                CREATE TABLE sessions (
                    id TEXT PRIMARY KEY, agent TEXT NOT NULL, agent_session_id TEXT,
                    console_id TEXT NOT NULL, project_id TEXT, host_id TEXT NOT NULL,
                    role TEXT NOT NULL, origin TEXT NOT NULL, title TEXT NOT NULL,
                    status TEXT NOT NULL, has_conversation INTEGER NOT NULL DEFAULT 0,
                    include_in_hub INTEGER NOT NULL DEFAULT 0, pinned INTEGER NOT NULL DEFAULT 0,
                    started_at INTEGER NOT NULL, ended_at INTEGER
                );
                INSERT INTO sessions VALUES ('session-1', 'claude', 'agent-1', 'console-1', NULL,
                    'local', 'console', 'user', 'Hub', 'idle', 1, 0, 0, 6, NULL);
                "#,
            )
            .expect("old schema");
        }

        let store = Store::open(&path).expect("opens a fresh database in its place");
        assert!(store.list_sessions().unwrap().is_empty());
        assert!(store.get_console("console-1").unwrap().is_none());

        let siblings: Vec<_> = path
            .parent()
            .unwrap()
            .read_dir()
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            siblings
                .iter()
                .any(|name| name.starts_with("octoboard.db.superseded-")),
            "the old file should have been moved aside, not deleted: found {siblings:?}"
        );
    }

    /// A database from before the trust permission was shared keeps its projects, each with the
    /// permission it had, under the column's new name.
    #[test]
    fn the_older_trust_permission_column_is_renamed_in_place() {
        let path = temp_db("trust-consent-rename");
        {
            let store = Store::open(&path).expect("store");
            store
                .insert_console(&console(None, None, None))
                .expect("console");
            store.insert_project(&project(true)).expect("project");
        }
        {
            let conn = Connection::open(&path).expect("database");
            conn.execute(
                "ALTER TABLE projects RENAME COLUMN trust_consent TO claude_trust_consent",
                [],
            )
            .expect("the older name");
        }

        let store = Store::open(&path).expect("reopened in place");
        let project = store.get_project("project-1").unwrap().expect("kept");
        assert!(project.trust_consent);
    }

    /// A database from before a session could be a lead session keeps its sessions, none of them
    /// one, and gains the column in place.
    #[test]
    fn the_lead_column_is_added_in_place() {
        let path = temp_db("lead-column");
        {
            let store = Store::open(&path).expect("store");
            store
                .insert_console(&console(None, None, None))
                .expect("console");
            store.insert_session(&session(None)).expect("session");
        }
        {
            let conn = Connection::open(&path).expect("database");
            conn.execute("ALTER TABLE sessions DROP COLUMN lead", [])
                .expect("the older shape");
        }

        let store = Store::open(&path).expect("reopened in place");
        let session = store.get_session("session-1").unwrap().expect("kept");
        assert!(!session.lead);
    }

    /// Migration keeps older sessions inheriting. New overrides, including explicit nulls,
    /// survive unrelated writes, account changes and opening the database again.
    #[test]
    fn session_launch_overrides_migrate_and_survive_reopening() {
        let path = temp_db("session-launch-overrides");
        {
            let store = Store::open(&path).unwrap();
            store.insert_console(&console(None, None, None)).unwrap();
            store.insert_session(&session(None)).unwrap();
        }
        {
            let conn = Connection::open(&path).unwrap();
            for column in ["model", "effort", "model_override", "effort_override"] {
                conn.execute(&format!("ALTER TABLE sessions DROP COLUMN {column}"), [])
                    .unwrap();
            }
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

    /// A database from before a console carried a console session model and effort keeps its
    /// consoles, and gains the columns in place.
    #[test]
    fn the_console_session_launch_columns_are_added_in_place() {
        let path = temp_db("console-session-launch-columns");
        {
            let store = Store::open(&path).expect("store");
            store
                .insert_console(&console(None, None, None))
                .expect("console");
        }
        {
            let conn = Connection::open(&path).expect("database");
            for column in CONSOLE_SESSION_LAUNCH_COLUMNS {
                conn.execute(&format!("ALTER TABLE consoles DROP COLUMN {column}"), [])
                    .expect("the older shape");
            }
        }

        let store = Store::open(&path).expect("reopened in place");
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
        store.update_console(&kept).expect("writes the new columns");
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

    /// A database from before a console could name a project session agent keeps its consoles
    /// following `default_agent`, and gains the column in place.
    #[test]
    fn the_project_session_agent_column_is_added_in_place() {
        let path = temp_db("project-session-agent-column");
        {
            let store = Store::open(&path).expect("store");
            store
                .insert_console(&console(None, None, None))
                .expect("console");
        }
        {
            let conn = Connection::open(&path).expect("database");
            conn.execute("ALTER TABLE consoles DROP COLUMN project_session_agent", [])
                .expect("the older shape");
        }

        let store = Store::open(&path).expect("reopened in place");
        let mut kept = store.get_console("console-1").unwrap().expect("kept");
        assert_eq!(kept.project_session_agent, None);
        kept.project_session_agent = Some(Agent::Grok);
        store.update_console(&kept).expect("writes the new column");
        let reread = store.get_console("console-1").unwrap().expect("kept");
        assert_eq!(reread.project_session_agent, Some(Agent::Grok));
    }

    /// A database from before a console carried a project session model and effort keeps its
    /// consoles, with none set, and gains the columns in place.
    #[test]
    fn the_console_project_session_launch_columns_are_added_in_place() {
        let path = temp_db("console-project-session-launch-columns");
        {
            let store = Store::open(&path).expect("store");
            store
                .insert_console(&console(None, None, None))
                .expect("console");
        }
        {
            let conn = Connection::open(&path).expect("database");
            for column in CONSOLE_PROJECT_SESSION_LAUNCH_COLUMNS {
                conn.execute(&format!("ALTER TABLE consoles DROP COLUMN {column}"), [])
                    .expect("the older shape");
            }
        }

        let store = Store::open(&path).expect("reopened in place");
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
        store.update_console(&kept).expect("writes the new columns");
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

    /// A database from before a console could carry the user's own console session instructions
    /// keeps its consoles, and gains the column in place.
    #[test]
    fn the_console_session_prompt_column_is_added_in_place() {
        let path = temp_db("console-session-prompt-column");
        {
            let store = Store::open(&path).expect("store");
            store
                .insert_console(&console(None, None, None))
                .expect("console");
        }
        {
            let conn = Connection::open(&path).expect("database");
            conn.execute(
                "ALTER TABLE consoles DROP COLUMN console_session_prompt",
                [],
            )
            .expect("the older shape");
        }

        let store = Store::open(&path).expect("reopened in place");
        let mut kept = store.get_console("console-1").unwrap().expect("kept");
        assert_eq!(kept.console_session_prompt, None);
        kept.console_session_prompt = Some("Be brief.".to_string());
        store.update_console(&kept).expect("writes the new column");
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

    /// A database from before a project carried its own accounts, models and efforts keeps its
    /// projects, and gains the columns in place; all nine are written and read back distinct.
    #[test]
    fn the_project_setting_columns_are_added_in_place() {
        let path = temp_db("project-setting-columns");
        {
            let store = Store::open(&path).expect("store");
            store
                .insert_console(&console(None, None, None))
                .expect("console");
            store.insert_project(&project(true)).expect("project");
        }
        {
            let conn = Connection::open(&path).expect("database");
            for column in PROJECT_SETTING_COLUMNS
                .into_iter()
                .chain(PROJECT_ACCOUNT_DEFAULT_COLUMNS)
            {
                conn.execute(&format!("ALTER TABLE projects DROP COLUMN {column}"), [])
                    .expect("the older shape");
            }
        }

        let store = Store::open(&path).expect("reopened in place");
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
        store.update_project(&kept).expect("writes the new columns");

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
        // The trust permission and the pin sit after the new columns in the row and are read by
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

    /// A console stored when the console session's agent was its own setting may name an agent
    /// other than the default; opening the database brings it in line.
    #[test]
    fn a_divergent_console_session_agent_is_reconciled_on_open() {
        let path = temp_db("console-session-agent");
        {
            let store = Store::open(&path).expect("store");
            store
                .insert_console(&console(None, None, None))
                .expect("console");
        }
        {
            let conn = Connection::open(&path).expect("database");
            conn.execute("UPDATE consoles SET console_session_agent = 'grok'", [])
                .expect("the older data");
        }

        let store = Store::open(&path).expect("reopened");
        let reconciled = store.get_console("console-1").unwrap().expect("kept");
        assert_eq!(reconciled.console_session_agent, Agent::Claude);
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
        }
    }

    /// The table is created on a database that predates it, and its rows are not the sessions':
    /// deleting the project takes the sessions with it and leaves the attribution.
    #[test]
    fn branch_origins_are_added_in_place_and_outlive_their_session() {
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
        };
        let put = |store: &Store| {
            store
                .apply_branch_origins("/r/.git", &[], std::slice::from_ref(&origin), 10)
                .expect("origin");
        };
        {
            let store = Store::open(&path).expect("store");
            store
                .insert_console(&console(None, None, None))
                .expect("console");
            store.insert_project(&project(false)).expect("project");
            put(&store);
            let conn = store.lock();
            conn.execute_batch("DROP TABLE branch_origins").unwrap();
        }
        let store = Store::open(&path).expect("opens a database without the table");
        assert!(store.branch_origins("/r/.git").unwrap().is_empty());

        put(&store);
        store.delete_project("project-1").expect("project deleted");
        assert_eq!(store.branch_origins("/r/.git").unwrap(), [origin]);
    }

    /// A database from before the responsible session's title was recorded gets, for a branch
    /// handed to a session that still exists, that session's title cut as a record cuts it, and
    /// the recorded creator's otherwise; the creator columns are gone afterwards.
    #[test]
    fn branch_origins_without_a_responsible_title_get_the_creators_and_lose_the_creator() {
        let path = temp_db("branch-origins-responsible-title");
        let long_title = format!("é{}", "x".repeat(100));
        {
            let store = Store::open(&path).expect("store");
            store
                .insert_console(&console(None, None, None))
                .expect("console");
            let holder = Session {
                id: "session-3".to_string(),
                title: long_title.clone(),
                ..session(None)
            };
            store.insert_session(&holder).expect("session");
            let conn = store.lock();
            conn.execute_batch(
                "DROP TABLE branch_origins;
                 CREATE TABLE branch_origins (
                     repository TEXT NOT NULL, branch TEXT NOT NULL, session_id TEXT,
                     session_title TEXT, source TEXT NOT NULL, created_at INTEGER NOT NULL,
                     birth_commit TEXT, birth_time INTEGER, responsible_session_id TEXT,
                     responsible_since INTEGER, PRIMARY KEY (repository, branch));
                 INSERT INTO branch_origins VALUES
                     ('/r/.git', 'feature', 'session-1', 'Title', 'ref_hook', 7000, 'abc', 7,
                      'session-2', 8000),
                     ('/r/.git', 'handed', 'session-1', 'Title', 'ref_hook', 7000, 'abc', 7,
                      'session-3', 8000);",
            )
            .unwrap();
        }
        let store = Store::open(&path).expect("migrates the table in place");
        let rows = store.branch_origins("/r/.git").unwrap();
        assert_eq!(rows[0].branch, "feature");
        assert_eq!(rows[0].responsible_session, "session-2");
        assert_eq!(rows[0].responsible_title, "Title");
        assert_eq!(rows[1].branch, "handed");
        assert_eq!(
            rows[1].responsible_title,
            long_title.chars().take(80).collect::<String>()
        );
        let conn = store.lock();
        assert!(!column_exists(&conn, "branch_origins", "session_id").unwrap());
        assert!(!column_exists(&conn, "branch_origins", "session_title").unwrap());
        assert!(!column_exists(&conn, "branch_origins", "source").unwrap());
    }

    /// A database from before the responsible session was recorded gets each branch's recorded
    /// creator as its responsible session, from when the branch was created, with its title; a row
    /// no git of a session reported, or one that names no session, is dropped.
    #[test]
    fn branch_origins_without_a_responsible_session_get_the_recorded_creator() {
        let path = temp_db("branch-origins-responsible");
        {
            let store = Store::open(&path).expect("store");
            let conn = store.lock();
            conn.execute_batch(
                "DROP TABLE branch_origins;
                 CREATE TABLE branch_origins (
                     repository TEXT NOT NULL, branch TEXT NOT NULL, session_id TEXT,
                     session_title TEXT, source TEXT NOT NULL, created_at INTEGER NOT NULL,
                     birth_commit TEXT, birth_time INTEGER, PRIMARY KEY (repository, branch));
                 INSERT INTO branch_origins VALUES
                     ('/r/.git', 'feature', 'session-1', 'Title', 'ref_hook', 7000, 'abc', 7),
                     ('/r/.git', 'no-session', NULL, 'Title', 'ref_hook', 7000, 'abc', 7),
                     ('/r/.git', 'guessed', 'session-1', 'Title', 'heuristic', 7000, 'abc', 7);",
            )
            .unwrap();
        }
        let store = Store::open(&path).expect("migrates the table in place");
        let rows = store.branch_origins("/r/.git").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].branch, "feature");
        assert_eq!(rows[0].responsible_session, "session-1");
        assert_eq!(rows[0].responsible_since, 7000);
        assert_eq!(rows[0].responsible_title, "Title");
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
    fn session_locks_migrate_persist_and_guard_every_store_removal() {
        let path = temp_db("session-locked");
        {
            let store = Store::open(&path).unwrap();
            store.insert_console(&console(None, None, None)).unwrap();
            store.insert_project(&project(false)).unwrap();
            let mut record = session(None);
            record.project_id = Some("project-1".to_string());
            store.insert_session(&record).unwrap();
            store
                .lock()
                .execute_batch(
                    "DROP TRIGGER protect_locked_session_delete;
                 DROP TRIGGER protect_locked_session_archive;
                 ALTER TABLE sessions DROP COLUMN locked;",
                )
                .unwrap();
        }
        let store = Store::open(&path).unwrap();
        let mut stale = store.get_session("session-1").unwrap().unwrap();
        assert!(!stale.locked);
        store.set_session_locked("session-1", true).unwrap();
        assert!(store.update_session(&stale).unwrap());
        assert!(store.get_session("session-1").unwrap().unwrap().locked);
        stale.status = SessionStatus::Archived;
        assert!(store.update_session(&stale).is_err());
        assert!(store.delete_session("session-1").is_err());
        assert!(store.delete_session_if_unused("session-1").is_err());
        assert!(store.delete_project("project-1").is_err());
        assert!(store.delete_console("console-1").is_err());
        assert!(store.get_project("project-1").unwrap().is_some());
        store.set_session_locked("session-1", false).unwrap();
        store.update_session(&stale).unwrap();
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

    /// A session record read before the user pinned it, written back afterwards, does not undo
    /// the pin; and writing back a deleted session reports that nothing was written.
    #[test]
    fn a_stale_session_write_leaves_the_pin_alone_and_a_deleted_one_is_not_written() {
        let path = temp_db("stale-session");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        let mut archived = session(None);
        archived.status = SessionStatus::Archived;
        store.insert_session(&archived).expect("insert");

        let stale = store.get_session("session-1").unwrap().unwrap();
        store.set_session_pinned("session-1", true).unwrap();
        assert!(store.update_session(&stale).unwrap());
        assert!(store.get_session("session-1").unwrap().unwrap().pinned);

        assert!(store
            .delete_session_if_archived("session-1")
            .unwrap()
            .is_some());
        assert!(!store.update_session(&stale).unwrap());
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

    /// A project stored before paths were normalised is the same project as the normalised one.
    #[test]
    fn a_project_stored_with_a_trailing_slash_is_found_by_its_normalised_path() {
        let path = temp_db("project-exists");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        let mut old = project(false);
        old.path = "/work/project/".to_string();
        store.insert_project(&old).expect("project");

        assert!(store
            .project_exists_at("console-1", "/work/project")
            .unwrap());
        assert!(store
            .project_exists_at("console-1", "/work/./x/../project/")
            .unwrap());
        assert!(!store
            .project_exists_at("console-1", "/work/project2")
            .unwrap());
        assert!(!store
            .project_exists_at("console-2", "/work/project")
            .unwrap());
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
        let mut referring = project(false);
        referring.claude_account_id = named("claude-acct");
        store.insert_project(&referring).expect("insert");
        // A project that pins the default account refers to no id, so it keeps saying so.
        let mut defaulting = project(false);
        defaulting.id = "project-2".to_string();
        defaulting.path = "/tmp/project-2".to_string();
        defaulting.claude_account_id = ProjectAccount::Default;
        store.insert_project(&defaulting).expect("insert");

        store.delete_account("claude-acct").expect("deleted");
        let claude_account = |id: &str| store.get_project(id).unwrap().unwrap().claude_account_id;
        assert_eq!(claude_account("project-1"), ProjectAccount::Inherit);
        assert_eq!(claude_account("project-2"), ProjectAccount::Default);
        assert!(store.get_account("claude-acct").unwrap().is_none());
        for id in ["console-1", "console-2"] {
            let console = store.get_console(id).unwrap().unwrap();
            assert_eq!(console.claude_account_id, None);
        }
    }

    #[test]
    fn approval_reviewer_columns_migrate_and_settings_round_trip() {
        let path = temp_db("approval-reviewer-migration");
        let store = Store::open(&path).unwrap();
        store
            .insert_account(&account("acct-1", Agent::Codex, "/tmp/codex"))
            .unwrap();
        {
            let conn = store.lock();
            conn.execute("ALTER TABLE accounts DROP COLUMN approval_reviewer", [])
                .unwrap();
            conn.execute(
                "ALTER TABLE settings DROP COLUMN codex_default_approval_reviewer",
                [],
            )
            .unwrap();
        }
        drop(store);
        let store = Store::open(&path).unwrap();
        let mut account = store.get_account("acct-1").unwrap().unwrap();
        assert_eq!(account.approval_reviewer, None);
        assert_eq!(store.codex_default_approval_reviewer().unwrap(), None);
        account.approval_reviewer = Some(ApprovalReviewer::User);
        store
            .set_account_approval_reviewer(&account.id, account.approval_reviewer)
            .unwrap();
        store
            .set_codex_default_approval_reviewer(Some(ApprovalReviewer::AutoReview))
            .unwrap();
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(store.get_account("acct-1").unwrap().unwrap(), account);
        assert_eq!(
            store
                .get_settings()
                .unwrap()
                .codex_default_approval_reviewer,
            Some(ApprovalReviewer::AutoReview)
        );
        store.set_codex_default_approval_reviewer(None).unwrap();
        assert_eq!(store.codex_default_approval_reviewer().unwrap(), None);
    }

    #[test]
    fn reviewer_changes_and_account_edits_do_not_overwrite_each_other() {
        let path = temp_db("account-reviewer-independent");
        let store = Store::open(&path).unwrap();
        let mut stale = account("acct-1", Agent::Codex, "/tmp/codex");
        store.insert_account(&stale).unwrap();
        store
            .set_account_approval_reviewer(&stale.id, Some(ApprovalReviewer::AutoReview))
            .unwrap();
        stale.name = "Renamed".into();
        stale.config_dir = "/tmp/codex-moved".into();
        store.update_account(&stale).unwrap();
        let saved = store.get_account(&stale.id).unwrap().unwrap();
        assert_eq!(saved.approval_reviewer, Some(ApprovalReviewer::AutoReview));
        store
            .set_account_approval_reviewer(&stale.id, Some(ApprovalReviewer::User))
            .unwrap();
        let saved = store.get_account(&stale.id).unwrap().unwrap();
        assert_eq!(saved.name, stale.name);
        assert_eq!(saved.config_dir, stale.config_dir);
        assert_eq!(saved.approval_reviewer, Some(ApprovalReviewer::User));
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

    #[test]
    fn tags_that_are_not_a_json_array_of_strings_read_as_untagged() {
        assert!(tags_from_text("project-1", "not json").is_empty());
    }

    /// A fresh database's settings start at their default, round-trip, and a repeat write reports
    /// no change.
    #[test]
    fn the_settings_round_trip_and_a_repeat_write_changes_nothing() {
        let path = temp_db("settings");
        let store = Store::open(&path).expect("store");
        assert!(!store.get_settings().unwrap().auto_pull_repositories);

        assert!(store.set_auto_pull_repositories(true).unwrap());
        assert!(
            !store.set_auto_pull_repositories(true).unwrap(),
            "a repeat write changes nothing"
        );
        assert!(store.get_settings().unwrap().auto_pull_repositories);
    }

    /// The default clone directory is the built-in one until set, a blank value goes back to it,
    /// and a database created before the setting existed gains the column and keeps its data.
    #[test]
    fn the_default_clone_dir_falls_back_and_a_database_without_the_column_still_opens() {
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
        drop(store);

        {
            let conn = Connection::open(&path).expect("conn");
            conn.execute("ALTER TABLE settings DROP COLUMN default_clone_dir", [])
                .expect("drop column");
        }
        let store = Store::open(&path).expect("reopened");
        assert_eq!(store.default_clone_dir().unwrap(), built_in);
    }

    /// A database whose setting column still has its older name keeps the saved value under the
    /// new one.
    #[test]
    fn the_older_auto_sync_column_is_renamed_in_place() {
        let path = temp_db("auto-pull-rename");
        {
            let store = Store::open(&path).expect("store");
            assert!(store.set_auto_pull_repositories(true).unwrap());
        }
        {
            let conn = Connection::open(&path).expect("database");
            conn.execute(
                "ALTER TABLE settings \
                 RENAME COLUMN auto_pull_repositories TO auto_sync_repositories",
                [],
            )
            .expect("the older name");
        }

        let store = Store::open(&path).expect("reopened in place");
        assert!(store.auto_pull_repositories().unwrap());
    }

    /// A project stored with the source's old name still reads, and is written back as `git`.
    #[test]
    fn a_stored_github_source_reads_as_git() {
        assert_eq!(
            enum_from_text::<ProjectSource>("github").unwrap(),
            ProjectSource::Git
        );
        assert_eq!(enum_to_text(&ProjectSource::Git), "git");
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
            .set_binding("owner", None, "peer", false, true)
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
            .set_binding("free", None, "peer", false, true)
            .expect("binding")
            .is_none());

        // An owner that is gone owns nothing.
        assert!(!store
            .insert_session_under_owner(&project_session("orphan", Some("nobody")), "nobody")
            .expect("insert"));
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

    /// A label of an earlier palette is rewritten to its nearest family when the store opens, and
    /// the store then reads it; a family name is left alone, and opening again changes nothing.
    #[test]
    fn colours_of_earlier_palettes_are_rewritten_on_opening() {
        let path = temp_db("colour-legacy");
        let earlier = [
            ("olive", "lime"),
            ("jade", "green"),
            ("azure", "sky"),
            ("rose", "pink"),
            ("purple", "fuchsia"),
            ("yellow-1", "yellow"),
            ("gray-3", "gray"),
            ("teal-2", "teal"),
            ("violet", "violet"),
            ("red", "red"),
        ];
        {
            let store = Store::open(&path).expect("store");
            store
                .insert_console(&console(None, None, None))
                .expect("console");
            for (i, (old, _)) in earlier.iter().enumerate() {
                store
                    .insert_console_session(console_session(&format!("session-{i}")), None)
                    .expect("inserted");
                let conn = Connection::open(&path).expect("database");
                conn.execute(
                    "UPDATE sessions SET colour = ?2 WHERE id = ?1",
                    params![format!("session-{i}"), old],
                )
                .expect("the earlier palette");
            }
        }

        for _ in 0..2 {
            let store = Store::open(&path).expect("reopened");
            for (i, (_, family)) in earlier.iter().enumerate() {
                let colour = store
                    .get_session(&format!("session-{i}"))
                    .unwrap()
                    .unwrap()
                    .colour;
                assert_eq!(enum_to_text(&colour.unwrap()), *family);
            }
        }
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

        assert_eq!(
            store.preview_console_session_title("console-1").unwrap(),
            "Console"
        );
        assert_eq!(
            store.preview_console_session_title("console-1").unwrap(),
            "Console"
        );
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

        assert_eq!(
            store.preview_console_session_title("console-1").unwrap(),
            "Console"
        );
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
        assert_eq!(
            store.preview_console_session_title("console-1").unwrap(),
            "Renamed console"
        );
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

    fn page(id: &str, console_session_id: &str, created_at: i64) -> Page {
        Page {
            id: id.to_string(),
            console_session_id: console_session_id.to_string(),
            html: String::new(),
            anchor_message_id: None,
            created_at,
        }
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
            page("a-1", "session-a", 1),
            page("b-1", "session-b", 2),
            page("a-2", "session-a", 3),
            page("a-3", "session-a", 3),
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

    #[test]
    fn usage_migrates_survives_unrelated_writes_and_rejects_stale_sources() {
        let path = temp_db("usage");
        let store = Store::open(&path).unwrap();
        store.insert_console(&console(None, None, None)).unwrap();
        let mut source = session(Some("/account-a"));
        source.agent_session_id = Some("native-1".to_owned());
        store.insert_session(&source).unwrap();
        drop(store);
        Connection::open(&path)
            .unwrap()
            .execute("ALTER TABLE sessions DROP COLUMN usage", [])
            .unwrap();
        let store = Store::open(&path).unwrap();
        let usage = crate::protocol::SessionUsage {
            input_tokens: 100,
            output_tokens: 20,
            total_tokens: 120,
            cached_input_tokens: Some(80),
            cache_write_input_tokens: None,
        };
        assert_eq!(
            store
                .set_record_data(&[RecordUpdate {
                    session: source.clone(),
                    usage: Some(usage.clone()),
                    titles: None
                }])
                .unwrap()
                .len(),
            1
        );
        assert!(store
            .set_record_data(&[RecordUpdate {
                session: source.clone(),
                usage: Some(usage.clone()),
                titles: None
            }])
            .unwrap()
            .is_empty());
        source.title = "Renamed".to_owned();
        source.status = SessionStatus::Archived;
        store.update_session(&source).unwrap();
        assert_eq!(
            store.get_session(&source.id).unwrap().unwrap().usage,
            Some(usage.clone())
        );
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(
            store.get_session(&source.id).unwrap().unwrap().usage,
            Some(usage.clone())
        );
        store
            .set_session_account(&source.id, None, Some("/account-b"))
            .unwrap();
        let changed = crate::protocol::SessionUsage {
            total_tokens: 999,
            ..usage.clone()
        };
        assert!(store
            .set_record_data(&[RecordUpdate {
                session: source.clone(),
                usage: Some(changed),
                titles: None
            }])
            .unwrap()
            .is_empty());
        let mut current = store.get_session(&source.id).unwrap().unwrap();
        current.agent_session_id = Some("native-2".to_owned());
        store.update_session(&current).unwrap();
        assert!(store
            .get_session(&source.id)
            .unwrap()
            .unwrap()
            .usage
            .is_none());
        assert!(store
            .set_record_data(&[RecordUpdate {
                session: source,
                usage: Some(usage),
                titles: None
            }])
            .unwrap()
            .is_empty());
    }

    #[test]
    fn record_titles_migrate_persist_and_follow_the_agent_session() {
        let path = temp_db("record-titles");
        let store = Store::open(&path).unwrap();
        store.insert_console(&console(None, None, None)).unwrap();
        store.insert_session(&session(None)).unwrap();
        drop(store);
        let conn = Connection::open(&path).unwrap();
        for column in ["record_title", "record_prompt"] {
            conn.execute(&format!("ALTER TABLE sessions DROP COLUMN {column}"), [])
                .unwrap();
        }
        drop(conn);
        let store = Store::open(&path).unwrap();
        let mut source = store.get_session("session-1").unwrap().unwrap();
        assert_eq!(source.record_titles, RecordTitles::default());
        let titles = RecordTitles {
            named: Some("Named".to_owned()),
            prompt: Some("Prompt".to_owned()),
        };
        let updates = [RecordUpdate {
            session: source.clone(),
            usage: None,
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
    fn terminal_titles_migrate_and_survive_unrelated_writes_and_reopening() {
        let path = temp_db("terminal-title");
        let store = Store::open(&path).unwrap();
        store.insert_console(&console(None, None, None)).unwrap();
        store.insert_session(&session(None)).unwrap();
        drop(store);
        Connection::open(&path)
            .unwrap()
            .execute("ALTER TABLE sessions DROP COLUMN terminal_title", [])
            .unwrap();
        let store = Store::open(&path).unwrap();
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

    /// A database already in the current shape is opened in place, and one whose `pages` table
    /// still names the console rather than the console session — the only difference from the
    /// current shape — is moved aside, though its `sessions` table is current.
    #[test]
    fn a_pages_table_naming_the_console_marks_the_database_outdated() {
        let moved_aside = |old_pages: bool| -> bool {
            let path = temp_db(if old_pages {
                "old-pages"
            } else {
                "current-pages"
            });
            drop(Store::open(&path).expect("current shape"));
            if old_pages {
                let conn = Connection::open(&path).expect("reopened");
                conn.execute_batch(
                    "DROP TABLE pages;
                     CREATE TABLE pages (id TEXT PRIMARY KEY, console_id TEXT NOT NULL,
                         html TEXT NOT NULL, anchor_message_id TEXT, created_at INTEGER NOT NULL);",
                )
                .expect("old pages table");
            }
            Store::open(&path).expect("opens");
            let moved = path.parent().unwrap().read_dir().unwrap().any(|entry| {
                entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("octoboard.db.superseded-")
            });
            moved
        };
        assert!(!moved_aside(false));
        assert!(moved_aside(true));
    }
}
