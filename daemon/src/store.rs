//! The coordinator's SQLite storage: consoles, projects, sessions and the host table.
//!
//! Enum columns hold the same text the protocol puts on the wire — the conversion goes through
//! serde (`enum_to_text` / `enum_from_text`) rather than a second hand-written mapping, so a
//! renamed variant cannot mean one thing in the database and another on the socket.
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
    SessionStatus,
};

/// The single local host record every project and session points at. The MVP has no other host,
/// but the column exists so that adding remote hosts needs no data migration (`docs/mvp.md`
/// section 10).
pub const LOCAL_HOST_ID: &str = "local";

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
                remote_url    TEXT
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
                started_at       INTEGER NOT NULL,
                ended_at         INTEGER
            );
            CREATE TABLE IF NOT EXISTS pages (
                id                TEXT PRIMARY KEY,
                console_id        TEXT NOT NULL REFERENCES consoles(id) ON DELETE CASCADE,
                html              TEXT NOT NULL,
                anchor_message_id TEXT,
                created_at        INTEGER NOT NULL
            );
            "#,
        )?;
        // Databases written before orchestration existed have no `include_in_hub`. `CREATE TABLE
        // IF NOT EXISTS` leaves those alone, so the column is added separately and the duplicate
        // error ignored — there is no other way to ask SQLite for "add it if it is missing".
        if let Err(err) = conn.execute(
            "ALTER TABLE sessions ADD COLUMN include_in_hub INTEGER NOT NULL DEFAULT 0",
            [],
        ) {
            let duplicate = err.to_string().contains("duplicate column name");
            if !duplicate {
                return Err(err).context("adding the sessions.include_in_hub column");
            }
        }
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.ensure_local_host()?;
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
            "INSERT INTO consoles (id, name, workdir, hub_agent, default_agent, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                console.id,
                console.name,
                console.workdir,
                enum_to_text(&console.hub_agent),
                enum_to_text(&console.default_agent),
                console.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn update_console(&self, console: &Console) -> Result<()> {
        self.lock().execute(
            "UPDATE consoles SET name = ?2, hub_agent = ?3, default_agent = ?4 WHERE id = ?1",
            params![
                console.id,
                console.name,
                enum_to_text(&console.hub_agent),
                enum_to_text(&console.default_agent),
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
                "SELECT id, name, workdir, hub_agent, default_agent, created_at
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
            "SELECT id, name, workdir, hub_agent, default_agent, created_at
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
            "INSERT INTO projects (id, console_id, host_id, name, path, default_agent, source, remote_url)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                project.id,
                project.console_id,
                project.host_id,
                project.name,
                project.path,
                project.default_agent.as_ref().map(enum_to_text),
                enum_to_text(&project.source),
                project.remote_url,
            ],
        )?;
        Ok(())
    }

    pub fn update_project(&self, project: &Project) -> Result<()> {
        self.lock().execute(
            "UPDATE projects SET name = ?2, default_agent = ?3 WHERE id = ?1",
            params![
                project.id,
                project.name,
                project.default_agent.as_ref().map(enum_to_text),
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
                "SELECT id, console_id, host_id, name, path, default_agent, source, remote_url
                 FROM projects WHERE id = ?1",
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
        let conn = self.lock();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM projects WHERE console_id = ?1 AND path = ?2",
            params![console_id, path],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn list_projects(&self) -> Result<Vec<Project>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, console_id, host_id, name, path, default_agent, source, remote_url
             FROM projects ORDER BY name",
        )?;
        let rows = stmt
            .query_map([], read_project)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
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
                                   started_at, ended_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
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
                session.started_at,
                session.ended_at,
            ],
        )?;
        Ok(())
    }

    /// Writes back the fields that change over a session's life. Identity and placement
    /// (`console_id`, `project_id`, `role`, `origin`, `include_in_hub`) never change, so they are not
    /// touched.
    pub fn update_session(&self, session: &Session) -> Result<()> {
        self.lock().execute(
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
        Ok(())
    }

    pub fn delete_session(&self, id: &str) -> Result<()> {
        self.lock()
            .execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        Ok(())
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

const SESSION_COLUMNS: &str = "id, agent, agent_session_id, console_id, project_id, host_id,
                               role, origin, title, status, has_conversation, include_in_hub,
                               started_at, ended_at";

fn read_console(row: &Row<'_>) -> rusqlite::Result<Console> {
    Ok(Console {
        id: row.get(0)?,
        name: row.get(1)?,
        workdir: row.get(2)?,
        hub_agent: enum_from_row(row, 3)?,
        default_agent: enum_from_row(row, 4)?,
        created_at: row.get(5)?,
    })
}

fn read_project(row: &Row<'_>) -> rusqlite::Result<Project> {
    Ok(Project {
        id: row.get(0)?,
        console_id: row.get(1)?,
        host_id: row.get(2)?,
        name: row.get(3)?,
        path: row.get(4)?,
        default_agent: enum_from_row_opt::<Agent>(row, 5)?,
        source: enum_from_row::<ProjectSource>(row, 6)?,
        remote_url: row.get(7)?,
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
        started_at: row.get(12)?,
        ended_at: row.get(13)?,
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
