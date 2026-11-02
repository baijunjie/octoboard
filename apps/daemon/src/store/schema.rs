//! The schema text, its version, and what opening an existing database file does with each version.

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::Connection;

/// The schema version a database is created at, recorded in SQLite's `user_version`. A database
/// at any other version is not opened as it stands; see [`settle_existing_file`].
///
/// `SCHEMA` only ever runs on an empty file, so an existing database at an unchanged version keeps
/// its old shape and fails at query time — while every test, starting from a fresh file, still
/// passes. No build has been released yet: the schema is edited in place and stays at version 1,
/// with no upgrade steps, and a database an earlier development build created is moved aside by
/// hand. From the first release on, every change to `SCHEMA` raises this by one and adds the step
/// from the previous version, with a test that opens a database at it.
pub(super) const SCHEMA_VERSION: i64 = 1;

/// Every table, index and trigger of the current schema.
pub(super) const SCHEMA: &str = r#"
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
-- to remember to check it. `LOWER` matches `accounts::account_name_key`'s
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
    -- 1 while the session's last turn ended abnormally: cancelled, declined, stopped on a
    -- usage limit, failed, or cut short by its process ending mid-turn. Cleared by a
    -- status off the prompt and by a turn that ends normally (see `Session::paused`).
    last_turn_abnormal INTEGER NOT NULL DEFAULT 0,
    -- The console session this (project) session reports to, or NULL outside the
    -- orchestration. Always NULL for a console session itself. No `REFERENCES`, because
    -- what follows a console session is decided by the coordinator, not by the
    -- database: archiving one archives its interrupted bound sessions, and deleting an
    -- archived one deletes its archived bound sessions. An `ON DELETE CASCADE` would be
    -- wrong twice over: it would take a bound session that is not archived, and it
    -- would remove rows behind the daemon's back, so no `session_deleted` would be
    -- broadcast and every client would keep a ghost row.
    bound_to         TEXT,
    -- `coordinator` or `worker` (see `Session::orchestration_role`). Written with
    -- `bound_to`: a console session and a session with no parent are always coordinators.
    orchestration_role TEXT NOT NULL,
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
    session_id        TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    html              TEXT NOT NULL,
    anchor_message_id TEXT,
    created_at        INTEGER NOT NULL
);
-- Messages for a session that was interrupted (shared information, and the reports and
-- notices for an owner), held to be written into it once it is resumed
-- (`Store::add_kept_message`), oldest first by `id`. Deleting the session takes its rows.
CREATE TABLE kept_messages (
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
    -- NULL until the user puts a tag on an agent; see `Store::agent_tags`.
    agent_tags             TEXT,
    development         TEXT NOT NULL DEFAULT 'worktree',
    squash_before_delivery INTEGER NOT NULL DEFAULT 0,
    rebase_before_merge INTEGER NOT NULL DEFAULT 1,
    delivery            TEXT NOT NULL DEFAULT 'ask',
    revert_gate         INTEGER NOT NULL DEFAULT 0
);
"#;

/// The schema version the file at `conn` is at; 0 for a file never given a schema.
pub(super) fn user_version(conn: &Connection) -> Result<i64> {
    Ok(conn.pragma_query_value(None, "user_version", |row| row.get(0))?)
}

/// Decides what `Store::open` does with a database file that already exists, before it opens it
/// for use:
///
/// - at [`SCHEMA_VERSION`], or holding no tables at all (an empty file, which `open` initializes):
///   left as it is;
/// - at an older version, which today is only 0, a database from before versions were recorded:
///   moved aside, so `open` starts a fresh one. The old file is renamed rather than deleted, so a
///   user who needs what was in it still has it on disk; see "Files Octoboard owns" in
///   `docs/product/application-lifecycle.md`;
/// - at a newer version: refused and left in place. This build cannot read it, and the user may
///   still have a build that does, so the file is not moved aside for them; the message says how
///   to do that by hand.
pub(super) fn settle_existing_file(path: &Path) -> Result<()> {
    {
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        let version = user_version(&conn)
            .with_context(|| format!("reading the schema version of {}", path.display()))?;
        if version == SCHEMA_VERSION {
            return Ok(());
        }
        if version > SCHEMA_VERSION {
            anyhow::bail!(
                "the database at {} is at schema version {version}, which this build does not read \
                 (it reads version {SCHEMA_VERSION}); rename that file to move it aside (nothing \
                 is deleted) and this build starts a fresh database in its place",
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
        "the database at this path is from before the current schema version; moved it aside and \
         starting a fresh one in its place"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    use crate::store::test_support::*;

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

    /// A database at a schema version this build does not read is refused and left where it is,
    /// with the way to move it aside by hand.
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
        let message = error.to_string();
        assert!(message.contains(&path.display().to_string()), "{message}");
        assert!(message.contains("rename that file"), "{message}");
        assert!(message.contains("nothing is deleted"), "{message}");
        assert!(path.exists());
        assert!(moved_aside(&path).is_empty());
    }
}
