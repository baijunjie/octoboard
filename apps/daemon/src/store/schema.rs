//! The schema text, its version, and what opening an existing database file does with each version.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::Connection;

/// The schema version a database is created at, recorded in SQLite's `user_version`. A database
/// at another version is not opened as it stands; see [`settle_existing_file`].
///
/// `SCHEMA` only ever runs on an empty file, so an existing database keeps the shape it was
/// created with and fails at query time unless it is brought up to date. Every change to `SCHEMA`
/// therefore raises this by one and appends the step from the previous version to [`MIGRATIONS`];
/// the schema of every past version stays as `schema/v{N}.sql` and is listed in the test-only
/// `PAST_SCHEMAS`, for the test that migrates a database built from each and compares the result
/// with a fresh one.
pub(super) const SCHEMA_VERSION: i64 = 3;

/// The `application_id` of an Octoboard database, "OCTB" in ASCII, written into the file's header
/// when it is created, so that a SQLite file of another program is told apart from one of ours.
/// Databases of version 1 were created before it existed and carry 0; the first migration stamps
/// them.
pub(super) const APPLICATION_ID: i64 = 0x4F43_5442;

/// The first schema version whose files carry [`APPLICATION_ID`].
const APPLICATION_ID_SINCE: i64 = 2;

/// The steps that bring a database up to [`SCHEMA_VERSION`]: `MIGRATIONS[n - 1]` takes a database
/// at version `n` to version `n + 1`. Each is a batch of SQL that runs in the one transaction of
/// the whole migration, with foreign keys off, and leaves exactly the shape `SCHEMA` creates (a
/// test compares the two). A step that rebuilds a table renames the old one aside and creates the
/// new one under its final name: a table renamed into place is stored with its name quoted in
/// `sqlite_master`, which no longer matches `SCHEMA`.
pub(super) const MIGRATIONS: &[&str] = &[MIGRATE_V1_TO_V2, MIGRATE_V2_TO_V3];

/// Version 1 to 2: a project's path is unique within its console, the child-table lookups have
/// indexes, and the `branch_origins` columns the code reads as present are `NOT NULL`.
const MIGRATE_V1_TO_V2: &str = r#"
-- Two projects at one path of one console become one: the oldest (lowest rowid) stays, and the
-- sessions of the others move to it before the others go. Nothing else refers to a project.
UPDATE sessions SET project_id = (
    SELECT kept.id FROM projects AS dup JOIN projects AS kept
        ON kept.console_id = dup.console_id AND kept.path = dup.path
    WHERE dup.id = sessions.project_id
    ORDER BY kept.rowid LIMIT 1)
WHERE project_id IN (
    SELECT dup.id FROM projects AS dup WHERE EXISTS (
        SELECT 1 FROM projects AS older
        WHERE older.console_id = dup.console_id AND older.path = dup.path
          AND older.rowid < dup.rowid));
DELETE FROM projects WHERE EXISTS (
    SELECT 1 FROM projects AS older
    WHERE older.console_id = projects.console_id AND older.path = projects.path
      AND older.rowid < projects.rowid);
CREATE UNIQUE INDEX projects_console_path_unique
    ON projects (console_id, path);
CREATE INDEX sessions_console_id ON sessions (console_id);
CREATE INDEX sessions_project_id ON sessions (project_id);
CREATE INDEX sessions_bound_to ON sessions (bound_to);
CREATE INDEX sessions_agent_session_id ON sessions (agent_session_id);
CREATE INDEX pages_session_id ON pages (session_id);
CREATE INDEX kept_messages_session_id ON kept_messages (session_id);
-- A row missing one of the columns made `BranchOrigin` fail the whole read of its repository, so
-- it is not carried over.
ALTER TABLE branch_origins RENAME TO branch_origins_v1;
CREATE TABLE branch_origins (
    repository     TEXT NOT NULL,
    branch         TEXT NOT NULL,
    created_at     INTEGER NOT NULL,
    birth_commit   TEXT NOT NULL,
    birth_time     INTEGER NOT NULL,
    responsible_session_id TEXT NOT NULL,
    responsible_since      INTEGER NOT NULL,
    responsible_title      TEXT NOT NULL,
    target_branch          TEXT,
    PRIMARY KEY (repository, branch)
);
INSERT INTO branch_origins
    SELECT repository, branch, created_at, birth_commit, birth_time, responsible_session_id,
           responsible_since, responsible_title, target_branch
    FROM branch_origins_v1
    WHERE birth_commit IS NOT NULL AND birth_time IS NOT NULL
      AND responsible_session_id IS NOT NULL AND responsible_since IS NOT NULL
      AND responsible_title IS NOT NULL;
DROP TABLE branch_origins_v1;
"#;

/// Version 2 to 3: pages no longer record the conversation position they were pushed at, which
/// nothing read.
const MIGRATE_V2_TO_V3: &str = r#"
ALTER TABLE pages DROP COLUMN anchor_message_id;
"#;

/// Every table, index and trigger of the current schema.
///
/// The `rebase_before_merge` columns of `projects` and `settings` hold the `rebase_before_delivery`
/// item of the git workflow settings; the column keeps its first name until a schema bump has a
/// migration step to rename it in.
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
-- A path is one project of its console. `coordinator` checks this before it adds a project,
-- by what the path is on the file system; the index is the backstop for two requests racing
-- past that check, and compares the stored text. Its leading column also serves the lookup
-- of a console's projects.
CREATE UNIQUE INDEX projects_console_path_unique
    ON projects (console_id, path);
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
-- The lookups by the columns that other rows and the daemon's queries hold sessions by.
CREATE INDEX sessions_console_id ON sessions (console_id);
CREATE INDEX sessions_project_id ON sessions (project_id);
CREATE INDEX sessions_bound_to ON sessions (bound_to);
CREATE INDEX sessions_agent_session_id ON sessions (agent_session_id);
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
    id         TEXT PRIMARY KEY,
    -- The session that pushed the page, a console session or a project session that
    -- faces the user; deleting it takes its pages, and deleting a console or a project
    -- therefore takes all of theirs.
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    html       TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX pages_session_id ON pages (session_id);
-- Messages for a session that was interrupted (shared information, and the reports and
-- notices for an owner), held to be written into it once it is resumed
-- (`Store::add_kept_message`), oldest first by `id`. Deleting the session takes its rows.
CREATE TABLE kept_messages (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    text       TEXT NOT NULL
);
CREATE INDEX kept_messages_session_id ON kept_messages (session_id);
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
    birth_commit   TEXT NOT NULL,
    birth_time     INTEGER NOT NULL,
    responsible_session_id TEXT NOT NULL,
    responsible_since      INTEGER NOT NULL,
    responsible_title      TEXT NOT NULL,
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

/// The `application_id` in the header of the file at `conn`; 0 for a file never given one.
fn application_id(conn: &Connection) -> Result<i64> {
    Ok(conn.pragma_query_value(None, "application_id", |row| row.get(0))?)
}

/// Decides what `Store::open` does with a database file that already exists, before it opens it
/// for use:
///
/// - holding no tables at all (an empty file, which `open` initializes): left as it is;
/// - with an `application_id` that is not [`APPLICATION_ID`] (0 counts as ours only for the
///   versions before the id existed): refused and left in place, since it is not an Octoboard
///   database and must not be moved, migrated or written to. This is judged before the version,
///   so a foreign file is never taken for a newer Octoboard one;
/// - at a negative version or one newer than [`SCHEMA_VERSION`]: refused and left in place. This
///   build cannot read it, and the user may still have a build that does, so the file is not moved
///   aside for them; the message says how to do that by hand;
/// - at [`SCHEMA_VERSION`]: left as it is;
/// - at version 0, a database from before versions were recorded: moved aside, so `open` starts a
///   fresh one. The old file is renamed rather than deleted, so a user who needs what was in it
///   still has it on disk; see "The database's schema version" in
///   `docs/product/application-lifecycle.md`;
/// - at any version in between: copied aside as a backup, then brought up to date by
///   [`MIGRATIONS`] in one transaction, which a failure rolls back, leaving the file as it was.
pub(super) fn settle_existing_file(path: &Path) -> Result<()> {
    let version = {
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        let version = user_version(&conn)
            .with_context(|| format!("reading the schema version of {}", path.display()))?;
        let tables: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'",
                [],
                |row| row.get(0),
            )
            .with_context(|| format!("listing the tables of {}", path.display()))?;
        if tables == 0 {
            // An empty file is initialized by `Store::open` only at version 0; at any other
            // version it would be left without a schema.
            if version != 0 {
                anyhow::bail!(
                    "the database at {} has no tables but is stamped with schema version \
                     {version}; rename that file to move it aside (nothing is deleted) and this \
                     build starts a fresh database in its place",
                    path.display()
                );
            }
            return Ok(());
        }
        let id = application_id(&conn)
            .with_context(|| format!("reading the application id of {}", path.display()))?;
        if id != APPLICATION_ID && !(id == 0 && version < APPLICATION_ID_SINCE) {
            anyhow::bail!(
                "the file at {} is not an Octoboard database (its application id is {id}); it is \
                 left as it is, so choose another data folder or move that file away",
                path.display()
            );
        }
        version
    };

    match version {
        0 => move_aside(path),
        1..SCHEMA_VERSION => migrate(path, version),
        SCHEMA_VERSION => Ok(()),
        _ => anyhow::bail!(
            "the database at {} is at schema version {version}, which this build does not read \
             (it reads version {SCHEMA_VERSION}); rename that file to move it aside (nothing \
             is deleted) and this build starts a fresh database in its place",
            path.display()
        ),
    }
}

/// Renames the outdated database at `path`, so `Store::open` starts a fresh one in its place.
fn move_aside(path: &Path) -> Result<()> {
    let moved_aside = path.with_file_name(format!(
        "{}.superseded-{}",
        file_name(path),
        crate::time::now_secs(),
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

/// Brings the database at `path`, at schema version `version`, up to [`SCHEMA_VERSION`]: backs
/// the file up first, with no connection open on it, then runs the pending [`MIGRATIONS`] and
/// the stamping in one transaction. Foreign keys are off for it, as a rebuilt table needs, and
/// the rows still have to satisfy them when it ends. A step that fails rolls the transaction back,
/// leaving the file as it was, and the error names the backup. A backup already identical to the
/// file is reused, so retrying after a failure does not pile up copies.
fn migrate(path: &Path, version: i64) -> Result<()> {
    let backup = match identical_backup(path, version)
        .with_context(|| format!("looking for an earlier backup of {}", path.display()))?
    {
        Some(existing) => existing,
        None => {
            let backup = backup_path(path, version);
            write_backup(path, &backup).with_context(|| {
                format!(
                    "backing up the database {} to {} before migrating it",
                    path.display(),
                    backup.display()
                )
            })?;
            backup
        }
    };
    run_migrations(path, version).with_context(|| {
        format!(
            "migrating the database at {} from schema version {version} to {SCHEMA_VERSION} failed \
             and left it as it was; a copy from before is at {}",
            path.display(),
            backup.display()
        )
    })?;
    tracing::info!(
        database = %path.display(),
        backup = %backup.display(),
        from = version,
        to = SCHEMA_VERSION,
        "migrated the database to the current schema version"
    );
    Ok(())
}

fn run_migrations(path: &Path, version: i64) -> Result<()> {
    let mut conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
    // The pragma has no effect inside a transaction.
    conn.pragma_update(None, "foreign_keys", "OFF")?;
    let tx = conn.transaction()?;
    for migration in &MIGRATIONS[usize::try_from(version - 1)?..] {
        tx.execute_batch(migration)?;
    }
    let violations: i64 =
        tx.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })?;
    if violations > 0 {
        anyhow::bail!("{violations} rows refer to a row that does not exist");
    }
    tx.pragma_update(None, "application_id", APPLICATION_ID)?;
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()?;
    // `foreign_keys` stays off: the connection is dropped on return, and `Store::open` opens a
    // fresh one with it on.
    Ok(())
}

/// The existing backup of the database at `path`, at schema version `version`, whose content is
/// the file's current content: `<name>.pre-v<version>` or one of its timestamped siblings.
fn identical_backup(path: &Path, version: i64) -> Result<Option<PathBuf>> {
    let name = format!("{}.pre-v{version}", file_name(path));
    let size = path.metadata()?.len();
    let current = std::fs::read(path)?;
    for entry in path.parent().unwrap_or_else(|| Path::new(".")).read_dir()? {
        let candidate = entry?.path();
        let candidate_name = candidate.file_name().and_then(|n| n.to_str()).unwrap_or("");
        // A `.partial` file is a copy that may never have finished, whatever its bytes.
        let is_backup = !candidate_name.ends_with(".partial")
            && (candidate_name == name
                || candidate_name
                    .strip_prefix(&name)
                    .is_some_and(|rest| rest.starts_with('-')));
        if is_backup
            && candidate.metadata().is_ok_and(|m| m.len() == size)
            && std::fs::read(&candidate).is_ok_and(|bytes| bytes == current)
        {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

/// Copies `path` to `backup` so that `backup` is either absent or complete: the copy is written
/// and synced under a `.partial` name, then renamed into place.
fn write_backup(path: &Path, backup: &Path) -> Result<()> {
    let partial = backup.with_file_name(format!("{}.partial", file_name(backup)));
    match std::fs::remove_file(&partial) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
        _ => {}
    }
    std::fs::copy(path, &partial)?;
    std::fs::OpenOptions::new()
        .write(true)
        .open(&partial)?
        .sync_all()?;
    std::fs::rename(&partial, backup)?;
    Ok(())
}

/// Where the backup of the database at `path`, at schema version `version`, goes: next to it as
/// `<name>.pre-v<version>`, with the time added when an earlier backup already has that name, so
/// no backup is overwritten.
fn backup_path(path: &Path, version: i64) -> PathBuf {
    let name = format!("{}.pre-v{version}", file_name(path));
    let mut candidate = path.with_file_name(&name);
    let mut attempt = 0;
    while candidate.exists() {
        attempt += 1;
        let suffix = if attempt == 1 {
            crate::time::now_secs().to_string()
        } else {
            format!("{}-{attempt}", crate::time::now_secs())
        };
        candidate = path.with_file_name(format!("{name}-{suffix}"));
    }
    candidate
}

fn file_name(path: &Path) -> &str {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("octoboard.db")
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

    /// A file with no tables but a version stamped on it is refused rather than opened without a
    /// schema.
    #[test]
    fn an_empty_file_with_a_version_is_refused() {
        let path = temp_db("empty-stamped");
        {
            let conn = Connection::open(&path).expect("empty database");
            conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
                .expect("version");
        }

        let error = Store::open(&path).err().expect("refused");
        assert!(error.to_string().contains("has no tables"), "{error:#}");
        assert!(path.exists());
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
            conn.pragma_update(None, "application_id", APPLICATION_ID)
                .expect("application id");
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

    /// The schema of version 1, as it was before the first migration.
    const V1_SCHEMA: &str = include_str!("schema/v1.sql");

    /// The schema of version 2, as it was before pages lost `anchor_message_id`.
    const V2_SCHEMA: &str = include_str!("schema/v2.sql");

    /// The schema of every version before the current one: `PAST_SCHEMAS[n - 1]` is version `n`.
    const PAST_SCHEMAS: [&str; (SCHEMA_VERSION - 1) as usize] = [V1_SCHEMA, V2_SCHEMA];

    /// The files next to `path` that are backups from before a migration.
    fn backups(path: &Path) -> Vec<PathBuf> {
        path.parent()
            .unwrap()
            .read_dir()
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|file| {
                file.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("octoboard.db.pre-v")
            })
            .collect()
    }

    /// A version 1 database with representative rows: two consoles, a project with three copies at
    /// one path (`kept` is inserted first, so it is the oldest, though its id sorts last), a
    /// session on each copy, a project at the same path of the other console, a branch record,
    /// and one missing a column.
    fn write_v1_database(path: &Path) {
        let conn = Connection::open(path).expect("v1 database");
        conn.execute_batch(V1_SCHEMA).expect("v1 schema");
        conn.execute_batch(
            "INSERT INTO consoles (id, name, workdir, default_agent, created_at)
                 VALUES ('console-1', 'One', '/w/1', 'claude', 0),
                        ('console-2', 'Two', '/w/2', 'claude', 0);
             INSERT INTO projects (id, console_id, host_id, name, path, source) VALUES
                 ('kept', 'console-1', 'local', 'A', '/p/a', 'local'),
                 ('dup-1', 'console-1', 'local', 'A', '/p/a', 'local'),
                 ('dup-2', 'console-1', 'local', 'A', '/p/a', 'local'),
                 ('single', 'console-1', 'local', 'B', '/p/b', 'local'),
                 ('other', 'console-2', 'local', 'A', '/p/a', 'local');
             INSERT INTO sessions (id, agent, console_id, project_id, host_id, role, origin, title,
                                   status, orchestration_role, started_at, locked) VALUES
                 ('s-kept', 'claude', 'console-1', 'kept', 'local', 'project', 'user', 't', 'idle', 'coordinator', 0, 0),
                 ('s-dup-1', 'claude', 'console-1', 'dup-1', 'local', 'project', 'user', 't', 'idle', 'coordinator', 0, 1),
                 ('s-dup-2', 'claude', 'console-1', 'dup-2', 'local', 'project', 'user', 't', 'idle', 'coordinator', 0, 0),
                 ('s-single', 'claude', 'console-1', 'single', 'local', 'project', 'user', 't', 'idle', 'coordinator', 0, 0),
                 ('s-other', 'claude', 'console-2', 'other', 'local', 'project', 'user', 't', 'idle', 'coordinator', 0, 0),
                 ('s-console', 'claude', 'console-1', NULL, 'local', 'console', 'user', 't', 'idle', 'coordinator', 0, 0);
             INSERT INTO pages (id, session_id, html, created_at) VALUES ('page-1', 's-dup-2', '', 0);
             INSERT INTO kept_messages (session_id, text) VALUES ('s-dup-1', 'hello');
             INSERT INTO branch_origins VALUES
                 ('/r/.git', 'whole', 1, 'abc', 1, 's-dup-1', 1, 'T', NULL),
                 ('/r/.git', 'broken', 1, NULL, 1, 's-dup-1', 1, 'T', NULL);",
        )
        .expect("v1 rows");
        conn.pragma_update(None, "user_version", 1)
            .expect("version");
    }

    /// The `sqlite_master` entries of the database at `path`, with each statement's whitespace
    /// made uniform.
    fn master(path: &Path) -> Vec<(String, String, String, Option<String>)> {
        let conn = Connection::open(path).expect("database");
        let mut stmt = conn
            .prepare("SELECT type, name, tbl_name, sql FROM sqlite_master ORDER BY type, name")
            .unwrap();
        let rows = stmt
            .query_map([], |row| {
                let sql: Option<String> = row.get(3)?;
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    sql.map(|sql| sql.split_whitespace().collect::<Vec<_>>().join(" ")),
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        rows
    }

    /// Every migration is one step from a version to the next, so there is one fewer than there
    /// are versions.
    #[test]
    fn there_is_one_migration_for_each_version_but_the_first() {
        assert_eq!(MIGRATIONS.len() as i64, SCHEMA_VERSION - 1);
    }

    /// A new database carries Octoboard's application id.
    #[test]
    fn a_new_database_carries_the_application_id() {
        let path = temp_db("fresh-application-id");
        let store = Store::open(&path).expect("store");
        assert_eq!(application_id(&store.lock()).unwrap(), APPLICATION_ID);
    }

    /// A version 1 database is backed up untouched, brought to the current version with its data
    /// (the sessions of duplicate projects moved to the oldest copy), and left with exactly the
    /// shape a new database has.
    #[test]
    fn a_version_1_database_is_migrated_to_the_shape_of_a_new_one() {
        let path = temp_db("migrate-v1");
        write_v1_database(&path);
        let original = std::fs::read(&path).unwrap();

        let store = Store::open(&path).expect("migrated");
        assert_eq!(user_version(&store.lock()).unwrap(), SCHEMA_VERSION);
        assert_eq!(application_id(&store.lock()).unwrap(), APPLICATION_ID);

        let backups = backups(&path);
        assert_eq!(backups.len(), 1);
        assert_eq!(
            backups[0].file_name().unwrap().to_string_lossy(),
            "octoboard.db.pre-v1"
        );
        assert_eq!(std::fs::read(&backups[0]).unwrap(), original);
        assert!(moved_aside(&path).is_empty());

        let project_ids = |console: &str| {
            let mut ids = store
                .list_projects()
                .unwrap()
                .into_iter()
                .filter(|project| project.console_id == console)
                .map(|project| project.id)
                .collect::<Vec<_>>();
            ids.sort();
            ids
        };
        assert_eq!(project_ids("console-1"), ["kept", "single"]);
        assert_eq!(project_ids("console-2"), ["other"]);
        let project_of = |session: &str| store.get_session(session).unwrap().unwrap().project_id;
        for session in ["s-kept", "s-dup-1", "s-dup-2"] {
            assert_eq!(project_of(session).as_deref(), Some("kept"), "{session}");
        }
        assert_eq!(project_of("s-single").as_deref(), Some("single"));
        assert_eq!(project_of("s-other").as_deref(), Some("other"));
        assert_eq!(project_of("s-console"), None);
        assert!(store.get_session("s-dup-1").unwrap().unwrap().locked);
        assert_eq!(store.list_pages("s-dup-2").unwrap().len(), 1);
        assert_eq!(store.kept_messages("s-dup-1").unwrap().len(), 1);
        let origins = store.branch_origins("/r/.git").unwrap();
        assert_eq!(origins.len(), 1);
        assert_eq!(origins[0].branch, "whole");

        let fresh = temp_db("migrate-v1-fresh");
        drop(Store::open(&fresh).expect("fresh"));
        assert_eq!(master(&path), master(&fresh));
    }

    /// A version 2 database keeps its pages, without the anchor they no longer carry, and has
    /// exactly the shape a new one has.
    #[test]
    fn a_version_2_database_is_migrated_to_the_shape_of_a_new_one() {
        let path = temp_db("migrate-v2");
        {
            let conn = Connection::open(&path).expect("v2 database");
            conn.execute_batch(V2_SCHEMA).expect("v2 schema");
            conn.execute_batch(
                "INSERT INTO consoles (id, name, workdir, default_agent, created_at)
                     VALUES ('console-1', 'One', '/w/1', 'claude', 0);
                 INSERT INTO sessions (id, agent, console_id, project_id, host_id, role, origin,
                                       title, status, orchestration_role, started_at, locked)
                     VALUES ('s', 'claude', 'console-1', NULL, 'local', 'console', 'user', 't',
                             'idle', 'coordinator', 0, 0);
                 INSERT INTO pages (id, session_id, html, anchor_message_id, created_at)
                     VALUES ('page-1', 's', '<p>kept</p>', 'message-1', 7);",
            )
            .expect("v2 rows");
            conn.pragma_update(None, "user_version", 2)
                .expect("version");
            conn.pragma_update(None, "application_id", APPLICATION_ID)
                .expect("application id");
        }

        let store = Store::open(&path).expect("migrated");
        assert_eq!(user_version(&store.lock()).unwrap(), SCHEMA_VERSION);
        let pages = store.list_pages("s").unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(
            (pages[0].html.as_str(), pages[0].created_at),
            ("<p>kept</p>", 7)
        );
        drop(store);

        let fresh = temp_db("migrate-v2-fresh");
        drop(Store::open(&fresh).expect("fresh"));
        assert_eq!(master(&path), master(&fresh));
    }

    /// An empty database at every past version, once migrated, has exactly the shape a new one has.
    #[test]
    fn every_past_schema_migrates_to_the_shape_of_a_new_database() {
        let fresh = temp_db("past-fresh");
        drop(Store::open(&fresh).expect("fresh"));
        for (index, schema) in PAST_SCHEMAS.iter().enumerate() {
            let version = index as i64 + 1;
            let path = temp_db(&format!("past-v{version}"));
            {
                let conn = Connection::open(&path).expect("past database");
                conn.execute_batch(schema).expect("past schema");
                conn.pragma_update(None, "user_version", version)
                    .expect("version");
                if version >= APPLICATION_ID_SINCE {
                    conn.pragma_update(None, "application_id", APPLICATION_ID)
                        .expect("application id");
                }
            }
            drop(Store::open(&path).unwrap_or_else(|e| panic!("v{version}: {e:#}")));
            assert_eq!(master(&path), master(&fresh), "v{version}");
        }
    }

    /// A second project at a path its console already has is refused by the store, not applied.
    #[test]
    fn a_second_project_at_one_path_of_a_console_is_refused() {
        let path = temp_db("project-path-unique");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store.insert_project(&project(false)).expect("project");
        let mut twin = project(false);
        twin.id = "project-2".to_string();
        let error = store.insert_project(&twin).expect_err("refused");
        assert!(error.to_string().contains("/tmp/project-1"), "{error}");
        assert_eq!(store.list_projects().unwrap().len(), 1);
    }

    /// A migration that fails leaves the database as it was, and the error names the backup.
    #[test]
    fn a_failed_migration_leaves_the_database_as_it_was() {
        let path = temp_db("migrate-fails");
        write_v1_database(&path);
        {
            // A session whose project is gone.
            let conn = Connection::open(&path).unwrap();
            conn.pragma_update(None, "foreign_keys", "OFF").unwrap();
            conn.execute_batch(
                "INSERT INTO sessions (id, agent, console_id, project_id, host_id, role, origin,
                                       title, status, orchestration_role, started_at)
                 VALUES ('orphan', 'claude', 'console-1', 'gone', 'local', 'project', 'user', 't',
                         'idle', 'coordinator', 0);",
            )
            .unwrap();
        }
        let original = std::fs::read(&path).unwrap();

        let error = Store::open(&path).err().expect("refused");
        let message = format!("{error:#}");
        assert!(message.contains("octoboard.db.pre-v1"), "{message}");
        assert!(
            message.contains("refer to a row that does not exist"),
            "{message}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert_eq!(user_version(&Connection::open(&path).unwrap()).unwrap(), 1);
        assert_eq!(
            std::fs::read(path.with_file_name("octoboard.db.pre-v1")).unwrap(),
            original
        );
    }

    /// Opening again after a failed migration reuses the backup the first try took.
    #[test]
    fn retrying_a_failed_migration_does_not_take_another_backup() {
        let path = temp_db("migrate-retry");
        write_v1_database(&path);
        {
            let conn = Connection::open(&path).unwrap();
            conn.pragma_update(None, "foreign_keys", "OFF").unwrap();
            conn.execute_batch(
                "INSERT INTO sessions (id, agent, console_id, project_id, host_id, role, origin,
                                       title, status, orchestration_role, started_at)
                 VALUES ('orphan', 'claude', 'console-1', 'gone', 'local', 'project', 'user', 't',
                         'idle', 'coordinator', 0);",
            )
            .unwrap();
        }

        for _ in 0..3 {
            assert!(Store::open(&path).is_err());
        }
        assert_eq!(backups(&path).len(), 1);
    }

    /// A migrated database reopens as it is, with no further backup.
    #[test]
    fn reopening_a_migrated_database_takes_no_backup() {
        let path = temp_db("migrate-reopen");
        write_v1_database(&path);
        drop(Store::open(&path).expect("migrated"));

        let store = Store::open(&path).expect("reopened");
        assert_eq!(user_version(&store.lock()).unwrap(), SCHEMA_VERSION);
        assert_eq!(backups(&path).len(), 1);
    }

    /// A backup that already exists is not overwritten by the next one.
    #[test]
    fn an_earlier_backup_is_not_overwritten() {
        let path = temp_db("backup-names");
        write_v1_database(&path);
        let earlier = path.with_file_name("octoboard.db.pre-v1");
        std::fs::write(&earlier, b"earlier").unwrap();

        drop(Store::open(&path).expect("migrated"));
        assert_eq!(std::fs::read(&earlier).unwrap(), b"earlier");
        assert_eq!(backups(&path).len(), 2);
    }

    /// A file that has tables and another program's application id is refused and left alone,
    /// at any version, and so is a current-version file without the id.
    #[test]
    fn a_database_of_another_program_is_refused_and_left_alone() {
        for (name, version, id) in [
            ("foreign-v0", 0, 0x1234_i64),
            ("foreign-v1", 1, 0x1234),
            ("foreign-current", SCHEMA_VERSION, 0x1234),
            ("no-id-current", SCHEMA_VERSION, 0),
        ] {
            let path = temp_db(name);
            {
                let conn = Connection::open(&path).expect("database");
                conn.execute_batch("CREATE TABLE things (id TEXT PRIMARY KEY);")
                    .expect("schema");
                conn.pragma_update(None, "user_version", version).unwrap();
                conn.pragma_update(None, "application_id", id).unwrap();
            }
            let original = std::fs::read(&path).unwrap();

            let error = Store::open(&path).err().expect("refused");
            assert!(
                format!("{error:#}").contains("is not an Octoboard database"),
                "{name}: {error:#}"
            );
            assert_eq!(std::fs::read(&path).unwrap(), original, "{name}");
            assert!(backups(&path).is_empty(), "{name}");
            assert!(moved_aside(&path).is_empty(), "{name}");
        }
    }
}
