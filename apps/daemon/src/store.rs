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
    error_code, now_millis, Account, Agent, BranchOriginSource, CodedError, Console,
    ConsoleSessionColour, Host, HostKind, Origin, Page, Project, ProjectAccount, ProjectSource,
    Role, Session, SessionStatus, Settings,
};

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
                icon          TEXT,
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
                -- 1 for a lead session: a project session bound to a console session after it
                -- was opened (see `Session::lead`).
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
            -- Which session created a branch, keyed on the branch because one session creates many
            -- and a branch has one creator. No `REFERENCES` on `session_id`: a project's deletion
            -- cascades its sessions away and the attribution outlives them, which is what the
            -- title snapshot is for. `repository` is the repository's canonical common directory,
            -- so a row belongs to the repository and not to any project.
            -- `birth_commit` and `birth_time` are the first entry of the branch's reflog, which
            -- tells a branch deleted and made again under the same name from the one the row was
            -- made for; both are NULL for a branch with no reflog.
            CREATE TABLE IF NOT EXISTS branch_origins (
                repository     TEXT NOT NULL,
                branch         TEXT NOT NULL,
                session_id     TEXT,
                session_title  TEXT,
                source         TEXT NOT NULL,
                created_at     INTEGER NOT NULL,
                birth_commit   TEXT,
                birth_time     INTEGER,
                PRIMARY KEY (repository, branch)
            );
            CREATE TABLE IF NOT EXISTS settings (
                id                     TEXT PRIMARY KEY,
                auto_sync_repositories INTEGER NOT NULL DEFAULT 0,
                -- NULL until the user sets one; see `Store::default_clone_dir`.
                default_clone_dir      TEXT
            );
            "#,
        )?;
        // No migration runs here: the application has not shipped, so a database whose schema is
        // not this one has already been moved aside by `supersede_if_outdated`, above, rather than
        // upgraded in place. `CREATE TABLE IF NOT EXISTS` therefore only ever meets either a brand
        // new file or one already in this shape. Moving the file aside would cost the user every
        // console, project and account, so six changes that lose nothing are made in place
        // instead. Five add a column: an optional setting gained after a database was created, a
        // session's lead flag (which no session stored before it can have), a console's per-agent
        // console session model and effort, a project's per-agent account, model and effort, and
        // a project's per-agent flag for pinning the default account.
        // The sixth renames the project's trust permission, once Claude Code's alone and now shared
        // by every agent, keeping its values. A seventh, a data change rather than a column, brings
        // each console's console session agent in line with its default agent; it is explained
        // where it runs, below.
        if !column_exists(&conn, "settings", "default_clone_dir")? {
            conn.execute("ALTER TABLE settings ADD COLUMN default_clone_dir TEXT", [])?;
        }
        if !column_exists(&conn, "sessions", "lead")? {
            conn.execute(
                "ALTER TABLE sessions ADD COLUMN lead INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        for column in CONSOLE_SESSION_LAUNCH_COLUMNS {
            if !column_exists(&conn, "consoles", column)? {
                conn.execute(
                    &format!("ALTER TABLE consoles ADD COLUMN {column} TEXT"),
                    [],
                )?;
            }
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
        // A console session always runs the console's default agent now; the column is kept, but
        // no longer a setting of its own. Rows from before that may differ, and this is a no-op
        // on every open after the first.
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
            "INSERT OR IGNORE INTO settings (id, auto_sync_repositories) VALUES (?1, 0)",
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
                                   grok_console_session_effort, icon, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
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
                console.created_at,
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
                                 grok_console_session_effort = ?13, icon = ?14
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
                "INSERT INTO accounts (id, agent, name, config_dir, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    account.id,
                    enum_to_text(&account.agent),
                    account.name,
                    account.config_dir,
                    now_millis(),
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

    pub fn get_account(&self, id: &str) -> Result<Option<Account>> {
        let account = self
            .lock()
            .query_row(
                "SELECT id, agent, name, config_dir FROM accounts WHERE id = ?1",
                params![id],
                read_account,
            )
            .optional()?;
        Ok(account)
    }

    /// Every account of every agent, oldest first.
    pub fn list_accounts(&self) -> Result<Vec<Account>> {
        let conn = self.lock();
        let mut stmt =
            conn.prepare("SELECT id, agent, name, config_dir FROM accounts ORDER BY created_at")?;
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
            auto_sync_repositories: self.auto_sync_repositories()?,
            default_clone_dir: self.default_clone_dir()?,
            accounts: self.list_accounts()?,
        })
    }

    pub fn auto_sync_repositories(&self) -> Result<bool> {
        Ok(self.lock().query_row(
            "SELECT auto_sync_repositories FROM settings WHERE id = ?1",
            params![SETTINGS_ID],
            |row| row.get(0),
        )?)
    }

    /// Whether the setting actually changed.
    pub fn set_auto_sync_repositories(&self, value: bool) -> Result<bool> {
        let conn = self.lock();
        let current: bool = conn.query_row(
            "SELECT auto_sync_repositories FROM settings WHERE id = ?1",
            params![SETTINGS_ID],
            |row| row.get(0),
        )?;
        if current == value {
            return Ok(false);
        }
        conn.execute(
            "UPDATE settings SET auto_sync_repositories = ?2 WHERE id = ?1",
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

    /// Inserts a freshly-opened console session, assigning its colour and its ordinal — and, with
    /// no `title` given, the default title that ordinal spells out ("Hub", then "Hub 2", "Hub 3",
    /// …) — in the same locked step as the row insert itself.
    ///
    /// Fused into one critical section because both assignments are a read followed by a decision
    /// that the insert must not be allowed to go stale between: the colour is the first palette
    /// entry not already held by one of the console's other non-archived console sessions, and the
    /// ordinal is a read-then-increment counter kept on the console record. Two concurrent opens
    /// for the same console must not both read the same snapshot and pick the same colour, or both
    /// consume the same ordinal — the shape of race the deleted one-live-console-session claim used
    /// to guard against by a different means.
    ///
    /// `session.colour`, `session.ordinal` and `session.title` (when `title` is `None`) are
    /// overwritten; every other field of `session` is inserted as given.
    ///
    /// The ordinal is consumed here, before the caller has launched the agent process, and is not
    /// given back if that launch then fails: a console whose agent binary is missing hands out
    /// "Hub 3" to its next successful open after a failed one at "Hub 2", with a gap in between.
    /// This is the intended reading of "one past the highest ever used", not a bug to fix by
    /// moving the increment into the same transaction as the launch: a title is never reused once
    /// handed out, whether or not the session behind it ever came up.
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
        let colour = ConsoleSessionColour::PALETTE
            .iter()
            .find(|candidate| !used_colours.contains(&enum_to_text(*candidate)))
            .copied()
            // Every entry is already in use: wrap back to the start rather than refuse, since the
            // badge is a convenience, not a uniqueness guarantee the user was ever promised.
            .unwrap_or(ConsoleSessionColour::PALETTE[0]);
        let ordinal: i64 = conn.query_row(
            "UPDATE consoles SET next_console_session_ordinal = next_console_session_ordinal + 1
             WHERE id = ?1
             RETURNING next_console_session_ordinal - 1",
            params![session.console_id],
            |row| row.get(0),
        )?;
        session.colour = Some(colour);
        session.ordinal = Some(ordinal);
        session.title = title.unwrap_or_else(|| match ordinal {
            // The first one needs no number to tell it apart.
            1 => "Hub".to_string(),
            _ => format!("Hub {ordinal}"),
        });
        insert_session_row(&conn, &session)?;
        Ok(session)
    }

    /// Writes back the fields that change over a session's life. Identity and placement
    /// (`console_id`, `project_id`, `role`, `origin`, `colour`, `ordinal`) never change, `bound_to`
    /// and `lead` change only through [`Self::bind_session`], once, and `account_id` and
    /// `config_dir` change only through [`Self::set_session_account`], the one write path a switch
    /// of the session's account uses, so a record read before a switch can never write the old
    /// account back over it. `pinned` is the user's own statement ([`Self::set_session_pinned`]),
    /// which a record read earlier must not overwrite.
    /// Returns whether a row was written: `false` means the session is gone (deleted meanwhile).
    pub fn update_session(&self, session: &Session) -> Result<bool> {
        let changed = self.lock().execute(
            "UPDATE sessions SET agent = ?2, agent_session_id = ?3, title = ?4, status = ?5,
                                 has_conversation = ?6, started_at = ?7, ended_at = ?8
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

    /// Binds the unbound project session `id` to `owner` and marks it a lead session, in one
    /// statement that only matches while it is still an unbound project session, so of two
    /// bindings racing for it only one lands. Returns the record as it now stands, or `None` when
    /// nothing was bound: the session is gone, is a console session, or is already bound. What
    /// `owner` may be is the caller's to check.
    pub fn bind_session(&self, id: &str, owner: &str) -> Result<Option<Session>> {
        let conn = self.lock();
        let changed = conn.execute(
            "UPDATE sessions SET bound_to = ?2, lead = 1
             WHERE id = ?1 AND role = ?3 AND bound_to IS NULL",
            params![id, owner, enum_to_text(&Role::Project)],
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

    pub fn set_session_pinned(&self, id: &str, pinned: bool) -> Result<()> {
        self.lock().execute(
            "UPDATE sessions SET pinned = ?2 WHERE id = ?1",
            params![id, pinned],
        )?;
        Ok(())
    }

    pub fn delete_session(&self, id: &str) -> Result<()> {
        self.lock()
            .execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Deletes the session only while it is archived, and says whether it did.
    pub fn delete_session_if_archived(&self, id: &str) -> Result<bool> {
        let deleted = self.lock().execute(
            "DELETE FROM sessions WHERE id = ?1 AND status = ?2",
            params![id, enum_to_text(&SessionStatus::Archived)],
        )?;
        Ok(deleted > 0)
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
                     (repository, branch, session_id, session_title, source,
                      created_at, birth_commit, birth_time)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for origin in put {
                insert.execute(params![
                    repository,
                    origin.branch,
                    origin.session_id,
                    origin.session_title,
                    enum_to_text(&origin.source),
                    origin.created_at,
                    origin.birth_commit,
                    origin.birth_time,
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

    /// Removes every origin recorded for the repository.
    pub fn delete_repository_branch_origins(&self, repository: &str) -> Result<()> {
        self.lock().execute(
            "DELETE FROM branch_origins WHERE repository = ?1",
            params![repository],
        )?;
        Ok(())
    }

    /// The connection itself, for a test that has to make a read of the store fail.
    #[cfg(test)]
    pub(crate) fn connection(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.lock()
    }
}

fn read_branch_origins(conn: &Connection, repository: &str) -> Result<Vec<BranchOrigin>> {
    let mut stmt = conn.prepare(
        "SELECT repository, branch, session_id, session_title, source,
                created_at, birth_commit, birth_time
         FROM branch_origins WHERE repository = ?1 ORDER BY branch",
    )?;
    let rows = stmt
        .query_map(params![repository], |row| {
            Ok(BranchOrigin {
                repository: row.get(0)?,
                branch: row.get(1)?,
                session_id: row.get(2)?,
                session_title: row.get(3)?,
                source: enum_from_row::<BranchOriginSource>(row, 4)?,
                created_at: row.get(5)?,
                birth_commit: row.get(6)?,
                birth_time: row.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// What the daemon knows of who created one branch. `session_id` and the title beside it are
/// `None` exactly when `source` is [`BranchOriginSource::Unknown`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchOrigin {
    pub repository: String,
    /// The branch's name below `refs/heads/`, as a wire path.
    pub branch: String,
    pub session_id: Option<String>,
    pub session_title: Option<String>,
    pub source: BranchOriginSource,
    /// Milliseconds since the epoch.
    pub created_at: i64,
    /// The commit and the second of the branch's first reflog entry.
    pub birth_commit: Option<String>,
    pub birth_time: Option<i64>,
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
/// older name is not: `Store::open` renames that column in place. Nor is a `consoles` or `projects`
/// table without its per-agent launch or account columns: `Store::open` adds those in place
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

const SESSION_COLUMNS: &str = "id, agent, agent_session_id, console_id, project_id, host_id,
                               role, origin, title, status, has_conversation, bound_to, colour,
                               ordinal, account_id, config_dir, pinned, started_at, ended_at,
                               lead";

const CONSOLE_COLUMNS: &str = "id, name, workdir, console_session_agent, default_agent,
                               claude_account_id, codex_account_id, grok_account_id,
                               claude_console_session_model, claude_console_session_effort,
                               codex_console_session_model, codex_console_session_effort,
                               grok_console_session_model, grok_console_session_effort, icon,
                               created_at";

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

/// Shared by [`Store::insert_session`] and [`Store::insert_console_session`], which differ only in
/// how `colour`, `ordinal` and `title` are decided before this runs.
fn insert_session_row(conn: &Connection, session: &Session) -> Result<()> {
    conn.execute(
        "INSERT INTO sessions
            (id, agent, agent_session_id, console_id, project_id, host_id, role, origin, title,
             status, has_conversation, bound_to, colour, ordinal, account_id, config_dir, pinned,
             started_at, ended_at, lead)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
                 ?19, ?20)",
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
        created_at: row.get(15)?,
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
        started_at: row.get(17)?,
        ended_at: row.get(18)?,
        lead: row.get(19)?,
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
            created_at: 0,
        }
    }

    fn account(id: &str, agent: Agent, config_dir: &str) -> Account {
        Account {
            id: id.to_string(),
            agent,
            name: format!("{id}-name"),
            config_dir: config_dir.to_string(),
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
            has_conversation: false,
            bound_to: None,
            lead: false,
            colour: None,
            ordinal: None,
            account_id: None,
            config_dir: config_dir.map(str::to_string),
            pinned: false,
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
            session_id: Some("session-1".to_string()),
            session_title: Some("Title".to_string()),
            source: BranchOriginSource::HookDiff,
            created_at: 7,
            birth_commit: Some("abc".to_string()),
            birth_time: Some(7),
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

        assert!(store.delete_session_if_archived("session-1").unwrap());
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
        assert!(!store.get_settings().unwrap().auto_sync_repositories);

        assert!(store.set_auto_sync_repositories(true).unwrap());
        assert!(
            !store.set_auto_sync_repositories(true).unwrap(),
            "a repeat write changes nothing"
        );
        assert!(store.get_settings().unwrap().auto_sync_repositories);
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

    /// A project stored with the source's old name still reads, and is written back as `git`.
    #[test]
    fn a_stored_github_source_reads_as_git() {
        assert_eq!(
            enum_from_text::<ProjectSource>("github").unwrap(),
            ProjectSource::Git
        );
        assert_eq!(enum_to_text(&ProjectSource::Git), "git");
    }

    /// A console session to feed [`Store::insert_console_session`]: a plain `role: Console` record
    /// whose colour, ordinal and title that call is going to overwrite, distinguished only by `id`.
    fn console_session(id: &str) -> Session {
        let mut built = session(None);
        built.id = id.to_string();
        built
    }

    /// Each of a console's console sessions takes the first palette entry none of the others
    /// already holds, in palette order — so with none yet live, six opens in a row take the whole
    /// palette in sequence.
    #[test]
    fn a_console_sessions_colour_is_the_first_palette_entry_not_already_held() {
        let path = temp_db("colour-assignment");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");

        let assigned: Vec<ConsoleSessionColour> = (0..ConsoleSessionColour::PALETTE.len())
            .map(|i| {
                store
                    .insert_console_session(console_session(&format!("session-{i}")), None)
                    .expect("inserted")
                    .colour
                    .expect("a console session always gets a colour")
            })
            .collect();
        assert_eq!(assigned, ConsoleSessionColour::PALETTE);
    }

    /// Once every palette entry is already held by a live console session, assignment wraps back
    /// to the first entry rather than refusing — the badge is a convenience, not a promise of
    /// uniqueness once the console runs more console sessions at once than the palette has room for.
    #[test]
    fn a_console_sessions_colour_wraps_back_to_the_start_once_every_entry_is_taken() {
        let path = temp_db("colour-wrap");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        for i in 0..ConsoleSessionColour::PALETTE.len() {
            store
                .insert_console_session(console_session(&format!("session-{i}")), None)
                .expect("inserted");
        }

        let wrapped = store
            .insert_console_session(console_session("session-overflow"), None)
            .expect("inserted");
        assert_eq!(wrapped.colour, Some(ConsoleSessionColour::PALETTE[0]));
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
        let first = store
            .insert_console_session(console_session("session-0"), None)
            .expect("inserted");
        assert_eq!(first.colour, Some(ConsoleSessionColour::PALETTE[0]));

        let mut archived = first.clone();
        archived.status = SessionStatus::Archived;
        store.update_session(&archived).expect("archived");

        let next = store
            .insert_console_session(console_session("session-1"), None)
            .expect("inserted");
        assert_eq!(
            next.colour,
            Some(ConsoleSessionColour::PALETTE[0]),
            "the archived console session's colour is free again, and is the first in palette order"
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
            .expect("delete"));

        let third = store
            .insert_console_session(console_session("session-2"), None)
            .expect("inserted");
        assert_eq!(
            third.ordinal,
            Some(3),
            "ordinal 2 was used and must not be handed out again, even though session-1 is gone"
        );
    }

    /// With no title given, a console session's default title spells out its ordinal, bare for the
    /// first; an explicit title is kept instead.
    #[test]
    fn a_console_sessions_default_title_names_its_ordinal() {
        let path = temp_db("console-session-title");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");

        let defaulted = store
            .insert_console_session(console_session("session-0"), None)
            .expect("inserted");
        assert_eq!(defaulted.title, "Hub");

        let second = store
            .insert_console_session(console_session("session-0b"), None)
            .expect("inserted");
        assert_eq!(second.title, "Hub 2");

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
