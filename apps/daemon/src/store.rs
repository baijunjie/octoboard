//! The coordinator's SQLite storage: consoles, projects, sessions and the host table.
//!
//! Enum columns hold the same text the protocol puts on the wire — the conversion goes through
//! serde (`enum_to_text` / `enum_from_text`) rather than a second hand-written mapping, so a
//! renamed variant cannot mean one thing in the database and another on the socket.
//!
//! A list-valued field is one JSON text column rather than a join table: a project's tags are
//! always read and written whole, so `read_project` stays a single-row read.
//!
//! `Report` is not among these tables: it is an in-flight struct passed between the worker's
//! `report` tool and the hub's session, never stored of its own accord (see `reporting.rs`).

use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::protocol::{
    Agent, Console, Host, HostKind, Origin, Page, Project, ProjectSource, Role, Session,
    SessionStatus, Settings,
};

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
            CREATE TABLE IF NOT EXISTS consoles (
                id            TEXT PRIMARY KEY,
                name          TEXT NOT NULL,
                workdir       TEXT NOT NULL,
                hub_agent     TEXT NOT NULL,
                default_agent TEXT NOT NULL,
                claude_config_dir TEXT,
                codex_config_dir  TEXT,
                grok_config_dir   TEXT,
                icon          TEXT,
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
                claude_trust_consent INTEGER NOT NULL DEFAULT 0,
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
                include_in_hub   INTEGER NOT NULL DEFAULT 0,
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
                console_id        TEXT NOT NULL REFERENCES consoles(id) ON DELETE CASCADE,
                html              TEXT NOT NULL,
                anchor_message_id TEXT,
                created_at        INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS settings (
                id                     TEXT PRIMARY KEY,
                auto_sync_repositories INTEGER NOT NULL DEFAULT 0
            );
            "#,
        )?;
        // `CREATE TABLE IF NOT EXISTS` leaves a database written by an older version alone, so a
        // column added since is added here separately. Existing rows read back as `NULL`, which
        // for the config directory columns is the right history: no directory was pinned for those
        // sessions.
        add_column_if_missing(
            &conn,
            "sessions",
            "include_in_hub",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        add_column_if_missing(&conn, "consoles", "claude_config_dir", "TEXT")?;
        add_column_if_missing(&conn, "consoles", "codex_config_dir", "TEXT")?;
        add_column_if_missing(&conn, "consoles", "grok_config_dir", "TEXT")?;
        add_column_if_missing(&conn, "consoles", "icon", "TEXT")?;
        // The session's pinned directory began as a Claude-only column. Renamed rather than copied,
        // so a session that already pinned one keeps it: every such session is a Claude Code one.
        if column_exists(&conn, "sessions", "claude_config_dir")? {
            conn.execute(
                "ALTER TABLE sessions RENAME COLUMN claude_config_dir TO config_dir",
                [],
            )
            .context("renaming sessions.claude_config_dir")?;
        }
        add_column_if_missing(&conn, "sessions", "config_dir", "TEXT")?;
        // Existing projects read back as not consented, which is the right history: nobody was ever
        // asked.
        add_column_if_missing(
            &conn,
            "projects",
            "claude_trust_consent",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        // Nothing was pinned before pinning existed.
        add_column_if_missing(&conn, "projects", "pinned", "INTEGER NOT NULL DEFAULT 0")?;
        add_column_if_missing(&conn, "sessions", "pinned", "INTEGER NOT NULL DEFAULT 0")?;
        // Projects from before the column read back as having no tags.
        add_column_if_missing(&conn, "projects", "tags", "TEXT NOT NULL DEFAULT '[]'")?;
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

    /// The settings row, at its defaults, for a database that has never had one: freshly created,
    /// or migrated from before the table existed.
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
            "INSERT INTO consoles (id, name, workdir, hub_agent, default_agent, claude_config_dir,
                                   codex_config_dir, grok_config_dir, icon, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                console.id,
                console.name,
                console.workdir,
                enum_to_text(&console.hub_agent),
                enum_to_text(&console.default_agent),
                console.claude_config_dir,
                console.codex_config_dir,
                console.grok_config_dir,
                console.icon,
                console.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn update_console(&self, console: &Console) -> Result<()> {
        self.lock().execute(
            "UPDATE consoles SET name = ?2, hub_agent = ?3, default_agent = ?4,
                                 claude_config_dir = ?5, codex_config_dir = ?6,
                                 grok_config_dir = ?7, icon = ?8
             WHERE id = ?1",
            params![
                console.id,
                console.name,
                enum_to_text(&console.hub_agent),
                enum_to_text(&console.default_agent),
                console.claude_config_dir,
                console.codex_config_dir,
                console.grok_config_dir,
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
        let conn = self.lock();
        let console = conn
            .query_row(
                "SELECT id, name, workdir, hub_agent, default_agent, claude_config_dir,
                        codex_config_dir, grok_config_dir, icon, created_at
                 FROM consoles WHERE id = ?1",
                params![id],
                read_console,
            )
            .optional()?;
        Ok(console)
    }

    pub fn list_consoles(&self) -> Result<Vec<Console>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, workdir, hub_agent, default_agent, claude_config_dir,
                    codex_config_dir, grok_config_dir, icon, created_at
             FROM consoles ORDER BY created_at",
        )?;
        let rows = stmt
            .query_map([], read_console)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    // -- projects ------------------------------------------------------------

    pub fn insert_project(&self, project: &Project) -> Result<()> {
        self.lock().execute(
            "INSERT INTO projects (id, console_id, host_id, name, path, default_agent, source,
                                   remote_url, claude_trust_consent, pinned, tags)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                project.id,
                project.console_id,
                project.host_id,
                project.name,
                project.path,
                project.default_agent.as_ref().map(enum_to_text),
                enum_to_text(&project.source),
                project.remote_url,
                project.claude_trust_consent,
                project.pinned,
                tags_to_text(&project.tags),
            ],
        )?;
        Ok(())
    }

    /// Records the user's agreement (or its withdrawal) that Octoboard may answer Claude Code's
    /// trust screen for this project. Its own statement rather than a field of `update_project`,
    /// which is the user's edit of the project and never reaches this decision.
    pub fn set_project_claude_trust_consent(&self, id: &str, consent: bool) -> Result<()> {
        self.lock().execute(
            "UPDATE projects SET claude_trust_consent = ?2 WHERE id = ?1",
            params![id, consent],
        )?;
        Ok(())
    }

    pub fn update_project(&self, project: &Project) -> Result<()> {
        self.lock().execute(
            "UPDATE projects SET name = ?2, default_agent = ?3, pinned = ?4, tags = ?5
             WHERE id = ?1",
            params![
                project.id,
                project.name,
                project.default_agent.as_ref().map(enum_to_text),
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

    /// The directories the user has agreed Octoboard may answer Claude Code's trust screen under,
    /// as stored: absolute and lexically normalised by whoever wrote them.
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
        let auto_sync_repositories = self.lock().query_row(
            "SELECT auto_sync_repositories FROM settings WHERE id = ?1",
            params![SETTINGS_ID],
            |row| row.get(0),
        )?;
        Ok(Settings {
            auto_sync_repositories,
        })
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

    // -- pages ---------------------------------------------------------------

    pub fn get_page(&self, id: &str) -> Result<Option<Page>> {
        let conn = self.lock();
        let page = conn
            .query_row(
                "SELECT id, console_id, html, anchor_message_id, created_at
                 FROM pages WHERE id = ?1",
                params![id],
                read_page,
            )
            .optional()?;
        Ok(page)
    }

    pub fn insert_page(&self, page: &Page) -> Result<()> {
        self.lock().execute(
            "INSERT INTO pages (id, console_id, html, anchor_message_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                page.id,
                page.console_id,
                page.html,
                page.anchor_message_id,
                page.created_at,
            ],
        )?;
        Ok(())
    }

    /// Oldest first: `created_at` alone can tie at millisecond resolution, so `id` breaks the tie
    /// deterministically rather than leaving the order to SQLite's whim.
    pub fn list_pages(&self, console_id: &str) -> Result<Vec<Page>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, console_id, html, anchor_message_id, created_at
             FROM pages WHERE console_id = ?1 ORDER BY created_at, id",
        )?;
        let rows = stmt
            .query_map(params![console_id], read_page)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// The id of the console's newest page, or `None` if it has none yet. The ordering is the exact
    /// reverse of [`Self::list_pages`]'s, so the two never disagree about which page is newest when
    /// `created_at` ties.
    pub fn newest_page_id(&self, console_id: &str) -> Result<Option<String>> {
        let id = self
            .lock()
            .query_row(
                "SELECT id FROM pages WHERE console_id = ?1 ORDER BY created_at DESC, id DESC LIMIT 1",
                params![console_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(id)
    }

    // -- sessions ------------------------------------------------------------

    pub fn insert_session(&self, session: &Session) -> Result<()> {
        self.lock().execute(
            "INSERT INTO sessions (id, agent, agent_session_id, console_id, project_id, host_id,
                                   role, origin, title, status, has_conversation, include_in_hub,
                                   config_dir, pinned, started_at, ended_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
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
                session.include_in_hub,
                session.config_dir,
                session.pinned,
                session.started_at,
                session.ended_at,
            ],
        )?;
        Ok(())
    }

    /// Writes back the fields that change over a session's life. Identity and placement
    /// (`console_id`, `project_id`, `role`, `origin`, `include_in_hub`) never change, and neither does
    /// `config_dir`, so they are not touched. `pinned` is the user's own statement
    /// ([`Self::set_session_pinned`]), which a record read earlier must not overwrite.
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

/// Adds a column to a table that may predate it. SQLite has no "add it if it is missing", so the
/// duplicate-column error is the signal that it is already there.
fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<()> {
    if let Err(err) = conn.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        [],
    ) {
        let duplicate = err.to_string().contains("duplicate column name");
        if !duplicate {
            return Err(err).with_context(|| format!("adding the {table}.{column} column"));
        }
    }
    Ok(())
}

const PROJECT_COLUMNS: &str = "id, console_id, host_id, name, path, default_agent, source,
                               remote_url, claude_trust_consent, pinned, tags";

const SESSION_COLUMNS: &str = "id, agent, agent_session_id, console_id, project_id, host_id,
                               role, origin, title, status, has_conversation, include_in_hub,
                               config_dir, pinned, started_at, ended_at";

fn read_console(row: &Row<'_>) -> rusqlite::Result<Console> {
    Ok(Console {
        id: row.get(0)?,
        name: row.get(1)?,
        workdir: row.get(2)?,
        hub_agent: enum_from_row(row, 3)?,
        default_agent: enum_from_row(row, 4)?,
        claude_config_dir: row.get(5)?,
        codex_config_dir: row.get(6)?,
        grok_config_dir: row.get(7)?,
        icon: row.get(8)?,
        created_at: row.get(9)?,
    })
}

fn read_project(row: &Row<'_>) -> rusqlite::Result<Project> {
    let id: String = row.get(0)?;
    let tags = tags_from_text(&id, &row.get::<_, String>(10)?);
    Ok(Project {
        id,
        console_id: row.get(1)?,
        host_id: row.get(2)?,
        name: row.get(3)?,
        path: row.get(4)?,
        default_agent: enum_from_row_opt::<Agent>(row, 5)?,
        source: enum_from_row::<ProjectSource>(row, 6)?,
        remote_url: row.get(7)?,
        claude_trust_consent: row.get(8)?,
        pinned: row.get(9)?,
        tags,
    })
}

fn read_page(row: &Row<'_>) -> rusqlite::Result<Page> {
    Ok(Page {
        id: row.get(0)?,
        console_id: row.get(1)?,
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
        include_in_hub: row.get(11)?,
        config_dir: row.get(12)?,
        pinned: row.get(13)?,
        started_at: row.get(14)?,
        ended_at: row.get(15)?,
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

    fn temp_db(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "octoboardd-store-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("temporary directory");
        dir.join("octoboard.db")
    }

    fn console(claude: Option<&str>, codex: Option<&str>, grok: Option<&str>) -> Console {
        Console {
            id: "console-1".to_string(),
            name: "Console".to_string(),
            workdir: "/tmp/console-1".to_string(),
            hub_agent: Agent::Claude,
            default_agent: Agent::Claude,
            claude_config_dir: claude.map(str::to_string),
            codex_config_dir: codex.map(str::to_string),
            grok_config_dir: grok.map(str::to_string),
            icon: None,
            created_at: 0,
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
            role: Role::Hub,
            origin: Origin::User,
            title: "Hub".to_string(),
            status: SessionStatus::Idle,
            has_conversation: false,
            include_in_hub: false,
            config_dir: config_dir.map(str::to_string),
            pinned: false,
            started_at: 0,
            ended_at: None,
        }
    }

    /// A reporter whose conclusion only holds while the session has not moved on writes through
    /// this, so the check and the write must be one step: it moves the session only from the status
    /// it expected, and says which happened.
    #[test]
    fn a_conditional_status_write_moves_the_session_only_from_the_status_it_expected() {
        let store = Store::open(&temp_db("conditional-status")).expect("store");
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

    const OLD_SESSIONS: &str = r#"
        CREATE TABLE sessions (
            id TEXT PRIMARY KEY, agent TEXT NOT NULL, agent_session_id TEXT,
            console_id TEXT NOT NULL, project_id TEXT, host_id TEXT NOT NULL,
            role TEXT NOT NULL, origin TEXT NOT NULL, title TEXT NOT NULL,
            status TEXT NOT NULL, has_conversation INTEGER NOT NULL DEFAULT 0,
            include_in_hub INTEGER NOT NULL DEFAULT 0, %COLUMN%started_at INTEGER NOT NULL,
            ended_at INTEGER
        );
    "#;

    /// A database written before the config directories existed is opened in place: its consoles
    /// and sessions read back with the setting unset — which is what they were started with — and
    /// opening it again is a no-op.
    #[test]
    fn a_database_from_before_the_config_dirs_is_migrated_in_place() {
        let path = temp_db("migration");
        {
            let conn = Connection::open(&path).expect("old database");
            conn.execute_batch(
                r#"
                CREATE TABLE consoles (
                    id TEXT PRIMARY KEY, name TEXT NOT NULL, workdir TEXT NOT NULL,
                    hub_agent TEXT NOT NULL, default_agent TEXT NOT NULL, created_at INTEGER NOT NULL
                );
                INSERT INTO consoles VALUES ('console-1', 'Old', '/tmp/old', 'claude', 'claude', 5);
                "#,
            )
            .expect("old schema");
            conn.execute_batch(&OLD_SESSIONS.replace("%COLUMN%", ""))
                .expect("old sessions");
            conn.execute_batch(
                "INSERT INTO sessions VALUES ('session-1', 'claude', 'agent-1', 'console-1', NULL,
                    'local', 'hub', 'user', 'Hub', 'archived', 1, 0, 6, 7);",
            )
            .expect("old session");
        }

        let store = Store::open(&path).expect("migrates");
        let consoles = store.list_consoles().expect("consoles");
        assert_eq!(consoles.len(), 1);
        assert_eq!(consoles[0].claude_config_dir, None);
        assert_eq!(consoles[0].codex_config_dir, None);
        assert_eq!(consoles[0].grok_config_dir, None);
        let migrated = store.get_session("session-1").expect("read").expect("kept");
        assert_eq!(migrated.config_dir, None);
        drop(store);

        Store::open(&path).expect("opens again");
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    /// The previous schema had the Claude-only columns. A session that pinned a directory keeps it
    /// under the generic name, and the console's Claude setting is kept as it was.
    #[test]
    fn a_database_with_the_claude_only_columns_keeps_what_they_held() {
        let path = temp_db("claude-only-migration");
        {
            let conn = Connection::open(&path).expect("old database");
            conn.execute_batch(
                r#"
                CREATE TABLE consoles (
                    id TEXT PRIMARY KEY, name TEXT NOT NULL, workdir TEXT NOT NULL,
                    hub_agent TEXT NOT NULL, default_agent TEXT NOT NULL,
                    claude_config_dir TEXT, created_at INTEGER NOT NULL
                );
                INSERT INTO consoles VALUES ('console-1', 'Old', '/tmp/old', 'claude', 'claude',
                    '/home/u/.claude-alt', 5);
                "#,
            )
            .expect("old schema");
            conn.execute_batch(&OLD_SESSIONS.replace("%COLUMN%", "claude_config_dir TEXT, "))
                .expect("old sessions");
            conn.execute_batch(
                "INSERT INTO sessions VALUES ('session-1', 'claude', 'agent-1', 'console-1', NULL,
                    'local', 'hub', 'user', 'Hub', 'archived', 1, 0, '/home/u/.claude-alt', 6, 7);",
            )
            .expect("old session");
        }

        let store = Store::open(&path).expect("migrates");
        let console = store.get_console("console-1").unwrap().unwrap();
        assert_eq!(
            console.claude_config_dir.as_deref(),
            Some("/home/u/.claude-alt")
        );
        assert_eq!(console.codex_config_dir, None);
        let migrated = store.get_session("session-1").unwrap().unwrap();
        assert_eq!(migrated.config_dir.as_deref(), Some("/home/u/.claude-alt"));
        drop(store);

        Store::open(&path).expect("opens again");
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
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
            claude_trust_consent: consent,
            pinned: false,
            tags: Vec::new(),
        }
    }

    /// A database written before the trust consent existed is opened in place, and its projects
    /// read back as not consented: nobody was ever asked. Opening it again is a no-op.
    #[test]
    fn a_database_from_before_the_trust_consent_reads_every_project_as_not_consented() {
        let path = temp_db("trust-migration");
        {
            let conn = Connection::open(&path).expect("old database");
            conn.execute_batch(
                r#"
                CREATE TABLE consoles (
                    id TEXT PRIMARY KEY, name TEXT NOT NULL, workdir TEXT NOT NULL,
                    hub_agent TEXT NOT NULL, default_agent TEXT NOT NULL, created_at INTEGER NOT NULL
                );
                INSERT INTO consoles VALUES ('console-1', 'Old', '/tmp/old', 'claude', 'claude', 5);
                CREATE TABLE projects (
                    id TEXT PRIMARY KEY, console_id TEXT NOT NULL, host_id TEXT NOT NULL,
                    name TEXT NOT NULL, path TEXT NOT NULL, default_agent TEXT,
                    source TEXT NOT NULL, remote_url TEXT
                );
                INSERT INTO projects VALUES ('project-1', 'console-1', 'local', 'Old', '/tmp/p',
                    NULL, 'local', NULL);
                "#,
            )
            .expect("old schema");
        }

        let store = Store::open(&path).expect("migrates");
        let migrated = store.get_project("project-1").unwrap().expect("kept");
        assert!(!migrated.claude_trust_consent);
        drop(store);

        Store::open(&path).expect("opens again");
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    /// A database written before pinning existed is opened in place, and nothing in it is pinned.
    /// A pin then round-trips on a project and a session.
    #[test]
    fn a_database_from_before_pinning_reads_nothing_as_pinned() {
        let path = temp_db("pinned-migration");
        {
            let conn = Connection::open(&path).expect("old database");
            conn.execute_batch(
                r#"
                CREATE TABLE consoles (
                    id TEXT PRIMARY KEY, name TEXT NOT NULL, workdir TEXT NOT NULL,
                    hub_agent TEXT NOT NULL, default_agent TEXT NOT NULL, created_at INTEGER NOT NULL
                );
                INSERT INTO consoles VALUES ('console-1', 'Old', '/tmp/old', 'claude', 'claude', 5);
                CREATE TABLE projects (
                    id TEXT PRIMARY KEY, console_id TEXT NOT NULL, host_id TEXT NOT NULL,
                    name TEXT NOT NULL, path TEXT NOT NULL, default_agent TEXT,
                    source TEXT NOT NULL, remote_url TEXT, claude_trust_consent INTEGER NOT NULL DEFAULT 0
                );
                INSERT INTO projects VALUES ('project-1', 'console-1', 'local', 'Old', '/tmp/p',
                    NULL, 'local', NULL, 0);
                CREATE TABLE sessions (
                    id TEXT PRIMARY KEY, agent TEXT NOT NULL, agent_session_id TEXT,
                    console_id TEXT NOT NULL, project_id TEXT, host_id TEXT NOT NULL,
                    role TEXT NOT NULL, origin TEXT NOT NULL, title TEXT NOT NULL,
                    status TEXT NOT NULL, has_conversation INTEGER NOT NULL DEFAULT 0,
                    include_in_hub INTEGER NOT NULL DEFAULT 0, config_dir TEXT,
                    started_at INTEGER NOT NULL, ended_at INTEGER
                );
                INSERT INTO sessions VALUES ('session-1', 'claude', NULL, 'console-1', NULL, 'local',
                    'hub', 'user', 'Old', 'idle', 0, 0, NULL, 1, NULL);
                "#,
            )
            .expect("old schema");
        }

        let store = Store::open(&path).expect("migrates");
        assert!(
            !store
                .get_project("project-1")
                .unwrap()
                .expect("kept")
                .pinned
        );
        assert!(
            !store
                .get_session("session-1")
                .unwrap()
                .expect("kept")
                .pinned
        );

        let mut pinned = store.get_project("project-1").unwrap().expect("kept");
        pinned.pinned = true;
        store.update_project(&pinned).unwrap();
        assert!(store.get_project("project-1").unwrap().unwrap().pinned);

        store.set_session_pinned("session-1", true).unwrap();
        assert!(store.get_session("session-1").unwrap().unwrap().pinned);
        drop(store);

        Store::open(&path).expect("opens again");
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
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
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
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
                .claude_trust_consent
        };
        assert!(!consented(&store));

        store
            .set_project_claude_trust_consent("project-1", true)
            .expect("recorded");
        assert!(consented(&store));
        assert!(store.list_projects().unwrap()[0].claude_trust_consent);

        let mut edited = store.get_project("project-1").unwrap().unwrap();
        edited.name = "Renamed".to_string();
        edited.claude_trust_consent = false;
        store.update_project(&edited).expect("edited");
        assert!(consented(&store), "an edit must not withdraw the consent");

        store
            .set_project_claude_trust_consent("project-1", false)
            .expect("withdrawn");
        assert!(!consented(&store));

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
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
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    /// A database from before the trusted directories opens in place with none, and what is written
    /// survives opening it again.
    #[test]
    fn the_trusted_directories_start_empty_after_a_migration_and_round_trip() {
        let path = temp_db("trusted-directories");
        {
            let conn = Connection::open(&path).expect("old database");
            conn.execute_batch(
                "CREATE TABLE consoles (
                    id TEXT PRIMARY KEY, name TEXT NOT NULL, workdir TEXT NOT NULL,
                    hub_agent TEXT NOT NULL, default_agent TEXT NOT NULL, created_at INTEGER NOT NULL
                );",
            )
            .expect("old schema");
        }
        let store = Store::open(&path).expect("migrates");
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

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn the_config_dirs_round_trip_and_a_session_keeps_its_own() {
        let path = temp_db("round-trip");
        let store = Store::open(&path).expect("store");

        store
            .insert_console(&console(
                Some("/home/u/.claude-alt"),
                Some("/home/u/.codex-alt"),
                Some("/home/u/.grok-alt"),
            ))
            .expect("insert");
        let stored = store.get_console("console-1").unwrap().unwrap();
        assert_eq!(
            stored.claude_config_dir.as_deref(),
            Some("/home/u/.claude-alt")
        );
        assert_eq!(
            stored.codex_config_dir.as_deref(),
            Some("/home/u/.codex-alt")
        );
        assert_eq!(stored.grok_config_dir.as_deref(), Some("/home/u/.grok-alt"));

        store
            .insert_session(&session(Some("/home/u/.claude-alt")))
            .expect("insert");

        // Updating the session's mutable fields leaves the directory it was started with alone.
        let mut live = store.get_session("session-1").unwrap().unwrap();
        live.config_dir = Some("/elsewhere".to_string());
        live.title = "Renamed".to_string();
        store.update_session(&live).expect("update");
        let after = store.get_session("session-1").unwrap().unwrap();
        assert_eq!(after.title, "Renamed");
        assert_eq!(after.config_dir.as_deref(), Some("/home/u/.claude-alt"));

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn updating_a_console_persists_each_agents_config_dir() {
        let path = temp_db("update-console");
        let store = Store::open(&path).expect("store");
        let mut stored = console(None, None, None);
        store.insert_console(&stored).expect("insert");
        let read = |store: &Store| {
            let console = store.get_console("console-1").unwrap().unwrap();
            (
                console.claude_config_dir,
                console.codex_config_dir,
                console.grok_config_dir,
            )
        };
        assert_eq!(read(&store), (None, None, None));

        for next in [Some("/home/u/.alt"), Some("/home/u/other"), None] {
            stored.claude_config_dir = next.map(str::to_string);
            stored.codex_config_dir = next.map(|dir| format!("{dir}-codex"));
            stored.grok_config_dir = next.map(|dir| format!("{dir}-grok"));
            store.update_console(&stored).expect("update");
            assert_eq!(
                read(&store),
                (
                    next.map(str::to_string),
                    next.map(|dir| format!("{dir}-codex")),
                    next.map(|dir| format!("{dir}-grok")),
                )
            );
        }

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    /// A database written before tags existed is opened in place, and no project in it has any.
    /// Tags then round-trip.
    #[test]
    fn a_database_from_before_tags_reads_every_project_as_untagged() {
        let path = temp_db("tags-migration");
        {
            let conn = Connection::open(&path).expect("old database");
            conn.execute_batch(
                r#"
                CREATE TABLE consoles (
                    id TEXT PRIMARY KEY, name TEXT NOT NULL, workdir TEXT NOT NULL,
                    hub_agent TEXT NOT NULL, default_agent TEXT NOT NULL, created_at INTEGER NOT NULL
                );
                INSERT INTO consoles VALUES ('console-1', 'Old', '/tmp/old', 'claude', 'claude', 5);
                CREATE TABLE projects (
                    id TEXT PRIMARY KEY, console_id TEXT NOT NULL, host_id TEXT NOT NULL,
                    name TEXT NOT NULL, path TEXT NOT NULL, default_agent TEXT,
                    source TEXT NOT NULL, remote_url TEXT,
                    claude_trust_consent INTEGER NOT NULL DEFAULT 0,
                    pinned INTEGER NOT NULL DEFAULT 0
                );
                INSERT INTO projects VALUES ('project-1', 'console-1', 'local', 'Old', '/tmp/p',
                    NULL, 'local', NULL, 0, 0);
                "#,
            )
            .expect("old schema");
        }

        let store = Store::open(&path).expect("migrates");
        let mut migrated = store.get_project("project-1").unwrap().expect("kept");
        assert!(migrated.tags.is_empty());

        migrated.tags = vec!["backend".to_string(), "Rust".to_string()];
        store.update_project(&migrated).unwrap();
        drop(store);

        let store = Store::open(&path).expect("opens again");
        assert_eq!(
            store.get_project("project-1").unwrap().unwrap().tags,
            ["backend", "Rust"]
        );
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn tags_that_are_not_a_json_array_of_strings_read_as_untagged() {
        assert!(tags_from_text("project-1", "not json").is_empty());
    }

    /// A database from before the settings table existed is opened in place, with the setting at
    /// its default; the setting then round-trips and a repeat write reports no change.
    #[test]
    fn the_settings_table_starts_at_its_default_after_a_migration_and_round_trips() {
        let path = temp_db("settings-migration");
        {
            let conn = Connection::open(&path).expect("old database");
            conn.execute_batch(
                "CREATE TABLE consoles (
                    id TEXT PRIMARY KEY, name TEXT NOT NULL, workdir TEXT NOT NULL,
                    hub_agent TEXT NOT NULL, default_agent TEXT NOT NULL, created_at INTEGER NOT NULL
                );",
            )
            .expect("old schema");
        }

        let store = Store::open(&path).expect("migrates");
        assert!(!store.get_settings().unwrap().auto_sync_repositories);

        assert!(store.set_auto_sync_repositories(true).unwrap());
        assert!(
            !store.set_auto_sync_repositories(true).unwrap(),
            "a repeat write changes nothing"
        );
        assert!(store.get_settings().unwrap().auto_sync_repositories);
        drop(store);

        let store = Store::open(&path).expect("opens again");
        assert!(store.get_settings().unwrap().auto_sync_repositories);
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }
}
