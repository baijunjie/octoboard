//! The schema text, its version, and what opening an existing database file does with each version.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::Connection;

/// The schema version a database is created at, recorded in SQLite's `user_version`. A database
/// at another version is not opened as it stands; see [`settle_existing_file`].
///
/// `SCHEMA` only ever runs on an empty file, so an existing database keeps the shape it was
/// created with and fails at query time unless it is brought up to date. Version changes and
/// migration code are written only between released versions: a change to `SCHEMA` that no
/// release has carried yet is made in place, at this version, with no new step (when this version
/// already has a step from a released one, that step is amended instead); the first change to the
/// schema of a released version raises this by one and appends the step from that version to
/// [`MIGRATIONS`], and the released version's schema is kept as `schema/v{N}.sql` and listed in the
/// test-only `PAST_SCHEMAS`, for the test that migrates a database built from each and compares the
/// result with a fresh one.
pub(super) const SCHEMA_VERSION: i64 = 1;

/// The `application_id` of an Octoboard database, "OCTB" in ASCII, written into the file's header
/// when it is created, so that a SQLite file of another program is told apart from one of ours.
pub(super) const APPLICATION_ID: i64 = 0x4F43_5442;

/// The steps that bring a database up to [`SCHEMA_VERSION`]: `MIGRATIONS[n - 1]` takes a database
/// at version `n` to version `n + 1`. Each is a batch of SQL that runs in the one transaction of
/// the whole migration, with foreign keys off, and leaves exactly the shape `SCHEMA` creates (a
/// test compares the two). A step that rebuilds a table renames the old one aside and creates the
/// new one under its final name: a table renamed into place is stored with its name quoted in
/// `sqlite_master`, which no longer matches `SCHEMA`.
pub(super) const MIGRATIONS: &[&str] = &[];

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
    trust_consent INTEGER NOT NULL DEFAULT 0,
    pinned        INTEGER NOT NULL DEFAULT 0,
    tags          TEXT NOT NULL DEFAULT '[]',
    description   TEXT,
    -- NULL follows the application-wide value; see `Store::git_workflow_settings`.
    development         TEXT,
    squash_before_delivery INTEGER,
    rebase_before_delivery INTEGER,
    delivery            TEXT,
    revert_gate         INTEGER
);
-- A path is one project of its console. `coordinator` checks this before it adds a project,
-- by what the path is on the file system; the index is the backstop for two requests racing
-- past that check, and compares the stored text. Its leading column also serves the lookup
-- of a console's projects.
CREATE UNIQUE INDEX projects_console_path_unique
    ON projects (console_id, path);
-- The account each provider's sessions in a console read, by id; a provider with no row
-- uses its default account. No `REFERENCES` on `account_id`: `Store::delete_account` removes
-- these rows itself, in the same locked step as the account's removal, rather than relying on
-- a cascade. A row exists only while it names an account. Rows naming a provider this build
-- does not register are kept and ignored.
CREATE TABLE console_providers (
    console_id TEXT NOT NULL REFERENCES consoles(id) ON DELETE CASCADE,
    provider   TEXT NOT NULL,
    account_id TEXT NOT NULL,
    PRIMARY KEY (console_id, provider)
);
-- The model and reasoning effort sessions in a console are launched with, per provider because
-- each has its own model names; NULL means the provider's own default. `session_kind` says which
-- sessions the pair launches: `console` for a console session of the console, `project` for a
-- project session there when neither it nor its project names one. Read at launch, so nothing
-- is copied onto the session. A row exists only while it names a model or an effort.
CREATE TABLE console_launches (
    console_id   TEXT NOT NULL REFERENCES consoles(id) ON DELETE CASCADE,
    provider     TEXT NOT NULL,
    session_kind TEXT NOT NULL CHECK (session_kind IN ('console', 'project')),
    model        TEXT,
    effort       TEXT,
    PRIMARY KEY (console_id, provider, session_kind)
);
-- What a project sets for each provider; a provider with no row inherits everything from its
-- console. `account_id` is the account the provider's sessions here read, by id; NULL with
-- `account_default` clear inherits the console's reference. The flag, set only where the id is
-- NULL, says the project pins the provider's default account instead, even where the console
-- names an account: a separate column so that no stored value in the id column can be mistaken
-- for an account id. The id is cleared by `Store::delete_account`, like the console's; the flag
-- names no account. `model` and `effort` are what a session here is launched with; NULL means
-- the provider's own default. Read at launch, so nothing is copied onto the session. A row
-- exists only while it holds a value; rows naming a provider this build does not register are
-- kept and ignored.
CREATE TABLE project_providers (
    project_id      TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    provider        TEXT NOT NULL,
    account_id      TEXT,
    account_default INTEGER NOT NULL DEFAULT 0,
    model           TEXT,
    effort          TEXT,
    PRIMARY KEY (project_id, provider)
);
CREATE TABLE sessions (
    id               TEXT PRIMARY KEY,
    agent            TEXT NOT NULL,
    agent_session_id TEXT,
    console_id       TEXT NOT NULL REFERENCES consoles(id) ON DELETE CASCADE,
    project_id       TEXT REFERENCES projects(id) ON DELETE CASCADE,
    host_id          TEXT NOT NULL,
    kind             TEXT NOT NULL,
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
    -- `coordinator` or `worker` (see `Session::role`). Written with `bound_to`: a console session,
    -- a session with no parent and one opened under a console session are always coordinators.
    role             TEXT NOT NULL,
    -- Above 0 while the session has a change of its owner it was not running to be
    -- told of (one count per change): its next launch tells it, from the binding as
    -- it then stands, and clears what it saw.
    owner_notice_pending INTEGER NOT NULL DEFAULT 0,
    -- The parting note of the owner that released the session while it was not running,
    -- told with the notice while the session is still unbound; NULL for none. Written
    -- with the release, or with the mark a failed write to the running session left,
    -- and cleared by the next change of owner and by the delivery.
    owner_notice_note TEXT,
    -- Set only for a console session (`kind = 'console'`); NULL for a project session.
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
-- reported creating the branch and moves only by a transfer. No `REFERENCES` on
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
    -- NULL until the user puts a tag on an agent; see `Store::agent_tags`.
    agent_tags             TEXT,
    development         TEXT NOT NULL DEFAULT 'worktree',
    squash_before_delivery INTEGER NOT NULL DEFAULT 0,
    rebase_before_delivery INTEGER NOT NULL DEFAULT 1,
    delivery            TEXT NOT NULL DEFAULT 'ask',
    revert_gate         INTEGER NOT NULL DEFAULT 0
);
-- The user's explicit choice for a setting of a provider, as JSON (`true`, `false` or a quoted
-- choice); a setting with no row follows its default. Rows naming a provider or setting this
-- build does not declare are kept and ignored. See `Store::provider_settings`.
CREATE TABLE provider_settings (
    provider TEXT NOT NULL,
    setting  TEXT NOT NULL,
    value    TEXT NOT NULL,
    PRIMARY KEY (provider, setting)
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
/// - with no `application_id` (0), a database written by a build from before the first release:
///   moved aside, so `open` starts a fresh one. The old file is renamed rather than deleted, so a
///   user who needs what was in it still has it on disk; see "The database's schema version" in
///   `docs/product/application-lifecycle.md`;
/// - with an `application_id` that is not [`APPLICATION_ID`]: refused and left in place, since it
///   is not an Octoboard database and must not be moved, migrated or written to. This is judged
///   before the version, so a foreign file is never taken for a newer Octoboard one;
/// - at a version below 1 or newer than [`SCHEMA_VERSION`]: refused and left in place. This build
///   cannot read it, and the user may still have a build that does, so the file is not moved aside
///   for them; the message says how to do that by hand;
/// - at [`SCHEMA_VERSION`]: left as it is;
/// - at any version in between: copied aside as a backup, then brought up to date by
///   [`MIGRATIONS`] in one transaction, which a failure rolls back, leaving the file as it was.
pub(super) fn settle_existing_file(path: &Path) -> Result<()> {
    let (version, id) = {
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
        (version, id)
    };

    if id == 0 {
        return move_aside(path);
    }
    if id != APPLICATION_ID {
        anyhow::bail!(
            "the file at {} is not an Octoboard database (its application id is {id}); it is \
             left as it is, so choose another data folder or move that file away",
            path.display()
        );
    }
    match version {
        SCHEMA_VERSION => Ok(()),
        1.. if version < SCHEMA_VERSION => migrate(path, version, MIGRATIONS),
        _ => anyhow::bail!(
            "the database at {} is at schema version {version}, which this build does not read \
             (it reads version {SCHEMA_VERSION}); rename that file to move it aside (nothing \
             is deleted) and this build starts a fresh database in its place",
            path.display()
        ),
    }
}

/// The files SQLite keeps beside a database while it is in use, by the suffix added to its name:
/// the write-ahead log, its index, and the rollback journal.
const SIDECARS: [&str; 3] = ["-wal", "-shm", "-journal"];

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    path.with_file_name(format!("{}{suffix}", file_name(path)))
}

/// Writes what the write-ahead log of the database at `path` holds into the file itself, so the
/// file alone has every committed row. A database not in WAL mode has nothing to write. Refused
/// while another connection keeps the log from being written back, since then the file alone is
/// not the whole database.
fn checkpoint(path: &Path) -> Result<()> {
    let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
    let busy: i64 = conn
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))
        .with_context(|| format!("writing the log of {} into it", path.display()))?;
    if busy != 0 {
        anyhow::bail!(
            "the database at {} is in use by another process; quit it and start again",
            path.display()
        );
    }
    Ok(())
}

/// `<name>` next to `path` if neither it nor any of its [`SIDECARS`] exists, otherwise the first
/// free `<name>-<n>`, counting from 2, so nothing already there is overwritten.
fn unused_name(path: &Path, name: &str) -> PathBuf {
    let taken = |candidate: &Path| {
        candidate.exists()
            || SIDECARS
                .iter()
                .any(|suffix| sidecar(candidate, suffix).exists())
    };
    let mut candidate = path.with_file_name(name);
    let mut attempt = 1;
    while taken(&candidate) {
        attempt += 1;
        candidate = path.with_file_name(format!("{name}-{attempt}"));
    }
    candidate
}

/// Renames the pre-release database at `path` to `<name>.superseded-<Unix seconds>` beside it
/// (see [`unused_name`] for a name already taken), so `Store::open` starts a fresh one in its
/// place. The log is written back into the file first, so the renamed file is a complete copy on
/// its own; any [`SIDECARS`] still there move with it under the new name, before the file itself,
/// so none is left for the fresh database to pick up. Nothing is deleted, and nothing in the file
/// changes beyond its log being written back.
fn move_aside(path: &Path) -> Result<()> {
    checkpoint(path)?;
    let moved_aside = unused_name(
        path,
        &format!("{}.superseded-{}", file_name(path), crate::time::now_secs()),
    );
    let moving = |from: &Path, to: &Path| {
        std::fs::rename(from, to).with_context(|| {
            format!(
                "moving the pre-release database {} aside to {}",
                from.display(),
                to.display()
            )
        })
    };
    for suffix in SIDECARS {
        let from = sidecar(path, suffix);
        if from.exists() {
            moving(&from, &sidecar(&moved_aside, suffix))?;
        }
    }
    moving(path, &moved_aside)?;
    tracing::warn!(
        old = %moved_aside.display(),
        new = %path.display(),
        "the database at this path is from a build before the first release; moved it aside and \
         starting a fresh one in its place"
    );
    Ok(())
}

/// Brings the database at `path`, at schema version `version`, up to the version `steps` lead to
/// (`steps[n - 1]` takes version `n` to `n + 1`, as in [`MIGRATIONS`]): backs the file up first,
/// with no connection open on it and its log written into it, then runs the pending steps and the
/// stamping in one transaction. Foreign keys are off for it, as a rebuilt table needs, and the rows
/// still have to satisfy them when it ends. A step that fails rolls the transaction back, leaving
/// the file as it was, and the error names the backup. A backup already identical to the file is
/// reused, so retrying after a failure does not pile up copies.
fn migrate(path: &Path, version: i64, steps: &[&str]) -> Result<()> {
    let target = i64::try_from(steps.len())? + 1;
    checkpoint(path)?;
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
    run_migrations(path, version, steps, target).with_context(|| {
        format!(
            "migrating the database at {} from schema version {version} to {target} failed and \
             left it as it was; a copy from before is at {}",
            path.display(),
            backup.display()
        )
    })?;
    tracing::info!(
        database = %path.display(),
        backup = %backup.display(),
        from = version,
        to = target,
        "migrated the database to the current schema version"
    );
    Ok(())
}

fn run_migrations(path: &Path, version: i64, steps: &[&str], target: i64) -> Result<()> {
    let mut conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
    // The pragma has no effect inside a transaction.
    conn.pragma_update(None, "foreign_keys", "OFF")?;
    let tx = conn.transaction()?;
    for step in &steps[usize::try_from(version - 1)?..] {
        tx.execute_batch(step)?;
    }
    let violations: i64 =
        tx.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })?;
    if violations > 0 {
        anyhow::bail!("{violations} rows refer to a row that does not exist");
    }
    tx.pragma_update(None, "application_id", APPLICATION_ID)?;
    tx.pragma_update(None, "user_version", target)?;
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
    let candidate = path.with_file_name(&name);
    if candidate.exists() {
        unused_name(path, &format!("{name}-{}", crate::time::now_secs()))
    } else {
        candidate
    }
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
            .filter(|name| {
                name.starts_with("octoboard.db.superseded-")
                    && !SIDECARS.iter().any(|suffix| name.ends_with(suffix))
            })
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

    /// A database from a build before the first release (tables in it, no application id) is
    /// moved aside rather than opened or deleted, at whatever version it records, and the daemon
    /// comes up on a fresh one at the original path.
    #[test]
    fn a_pre_release_database_is_moved_aside_and_a_fresh_one_opened() {
        for version in [0, 1, 2, 3] {
            let path = temp_db(&format!("pre-release-v{version}"));
            {
                let conn = Connection::open(&path).expect("old database");
                conn.execute_batch(
                    "CREATE TABLE consoles (id TEXT PRIMARY KEY, name TEXT NOT NULL);
                     INSERT INTO consoles VALUES ('console-1', 'Old');",
                )
                .expect("old schema");
                conn.pragma_update(None, "user_version", version)
                    .expect("version");
            }

            let store = Store::open(&path).expect("opens a fresh database in its place");
            assert_eq!(user_version(&store.lock()).unwrap(), SCHEMA_VERSION);
            assert_eq!(application_id(&store.lock()).unwrap(), APPLICATION_ID);
            assert!(store.get_console("console-1").unwrap().is_none());
            assert_eq!(
                moved_aside(&path).len(),
                1,
                "v{version}: the old file is kept, not deleted"
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
            assert_eq!(user_version(&old).unwrap(), version);
        }
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

    /// The schema of every version before the current one, each kept as `schema/v{N}.sql`:
    /// `PAST_SCHEMAS[n - 1]` is version `n`.
    const PAST_SCHEMAS: [&str; (SCHEMA_VERSION - 1) as usize] = [];

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
                conn.pragma_update(None, "application_id", APPLICATION_ID)
                    .expect("application id");
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

    /// A test-only step from version 1 to 2, standing in for the first real migration: the
    /// machinery is exercised through it while [`MIGRATIONS`] is empty.
    const TEST_STEPS: &[&str] = &["ALTER TABLE consoles ADD COLUMN extra TEXT;"];

    /// A database at version 1 holding a console, written by the current build.
    fn write_version_1_database(path: &Path) {
        let store = Store::open(path).expect("version 1 database");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
    }

    /// Adds a session whose project does not exist, which the check after the steps refuses.
    fn add_orphan_session(path: &Path) {
        let conn = Connection::open(path).unwrap();
        conn.pragma_update(None, "foreign_keys", "OFF").unwrap();
        conn.execute_batch(
            "INSERT INTO sessions (id, agent, console_id, project_id, host_id, kind, origin,
                                   title, status, role, started_at)
             VALUES ('orphan', 'claude', 'console-1', 'gone', 'local', 'project', 'user', 't',
                     'idle', 'coordinator', 0);",
        )
        .unwrap();
    }

    /// A migration backs the file up untouched, runs the pending steps, keeps the rows, and stamps
    /// the version the steps lead to.
    #[test]
    fn a_migration_backs_up_then_brings_the_database_up_to_date() {
        let path = temp_db("migrate");
        write_version_1_database(&path);
        let original = std::fs::read(&path).unwrap();

        migrate(&path, 1, TEST_STEPS).expect("migrated");
        let conn = Connection::open(&path).unwrap();
        assert_eq!(user_version(&conn).unwrap(), 2);
        assert_eq!(application_id(&conn).unwrap(), APPLICATION_ID);
        let extra: Option<String> = conn
            .query_row(
                "SELECT extra FROM consoles WHERE id = 'console-1'",
                [],
                |row| row.get(0),
            )
            .expect("the row is kept, with the new column");
        assert_eq!(extra, None);

        let backups = backups(&path);
        assert_eq!(backups.len(), 1);
        assert_eq!(
            backups[0].file_name().unwrap().to_string_lossy(),
            "octoboard.db.pre-v1"
        );
        assert_eq!(std::fs::read(&backups[0]).unwrap(), original);
        assert!(moved_aside(&path).is_empty());
    }

    /// A migration that fails leaves the database as it was, and the error names the backup.
    #[test]
    fn a_failed_migration_leaves_the_database_as_it_was() {
        let path = temp_db("migrate-fails");
        write_version_1_database(&path);
        add_orphan_session(&path);
        let original = std::fs::read(&path).unwrap();

        let error = migrate(&path, 1, TEST_STEPS).expect_err("refused");
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

    /// Migrating again after a failure reuses the backup the first try took.
    #[test]
    fn retrying_a_failed_migration_does_not_take_another_backup() {
        let path = temp_db("migrate-retry");
        write_version_1_database(&path);
        add_orphan_session(&path);

        for _ in 0..3 {
            assert!(migrate(&path, 1, TEST_STEPS).is_err());
        }
        assert_eq!(backups(&path).len(), 1);
    }

    /// A backup that already exists is not overwritten by the next one.
    #[test]
    fn an_earlier_backup_is_not_overwritten() {
        let path = temp_db("backup-names");
        write_version_1_database(&path);
        let earlier = path.with_file_name("octoboard.db.pre-v1");
        std::fs::write(&earlier, b"earlier").unwrap();

        migrate(&path, 1, TEST_STEPS).expect("migrated");
        assert_eq!(std::fs::read(&earlier).unwrap(), b"earlier");
        assert_eq!(backups(&path).len(), 2);
    }

    /// A file that has tables and another program's application id is refused and left alone,
    /// at any version.
    #[test]
    fn a_database_of_another_program_is_refused_and_left_alone() {
        for (name, version) in [
            ("foreign-v0", 0),
            ("foreign-current", SCHEMA_VERSION),
            ("foreign-newer", SCHEMA_VERSION + 1),
        ] {
            let path = temp_db(name);
            {
                let conn = Connection::open(&path).expect("database");
                conn.execute_batch("CREATE TABLE things (id TEXT PRIMARY KEY);")
                    .expect("schema");
                conn.pragma_update(None, "user_version", version).unwrap();
                conn.pragma_update(None, "application_id", 0x1234).unwrap();
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

    /// An Octoboard database at a version below the first is refused and left alone.
    #[test]
    fn an_octoboard_database_below_the_first_version_is_refused() {
        let path = temp_db("octoboard-v0");
        {
            let conn = Connection::open(&path).expect("database");
            conn.execute_batch("CREATE TABLE consoles (id TEXT PRIMARY KEY);")
                .expect("schema");
            conn.pragma_update(None, "application_id", APPLICATION_ID)
                .unwrap();
        }
        let original = std::fs::read(&path).unwrap();

        let error = Store::open(&path).err().expect("refused");
        assert!(error.to_string().contains("schema version 0"), "{error:#}");
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert!(moved_aside(&path).is_empty());
    }

    /// Rows a pre-release build left in its write-ahead log are in the moved-aside file itself,
    /// while that build's connection is still open, and the log moves with it.
    #[test]
    fn a_pre_release_database_moved_aside_keeps_the_rows_in_its_log() {
        let path = temp_db("pre-release-wal");
        let writer = Connection::open(&path).expect("old database");
        let mode: String = writer
            .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
        writer.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
        writer
            .execute_batch(
                "CREATE TABLE consoles (id TEXT PRIMARY KEY, name TEXT NOT NULL);
                 INSERT INTO consoles VALUES ('console-1', 'Old'), ('console-2', 'Older');",
            )
            .unwrap();
        assert!(sidecar(&path, "-wal").metadata().unwrap().len() > 0);

        drop(Store::open(&path).expect("opens a fresh database in its place"));
        let moved = moved_aside(&path);
        assert_eq!(moved.len(), 1);
        let kept = path.with_file_name(&moved[0]);
        assert!(
            !sidecar(&path, "-wal").exists(),
            "no old log is left for the fresh one"
        );
        assert!(
            sidecar(&kept, "-wal").exists(),
            "the log moved with the file"
        );

        // The renamed file alone, without any log beside it, holds every committed row.
        let alone = temp_db("pre-release-wal-alone");
        std::fs::copy(&kept, &alone).unwrap();
        let old = Connection::open(&alone).unwrap();
        let count: i64 = old
            .query_row("SELECT COUNT(*) FROM consoles", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
        drop(writer);
    }

    /// A name already taken by an earlier moved-aside copy, or by one of its sidecars, is not
    /// reused: the next free counter is.
    #[test]
    fn a_taken_name_is_not_overwritten() {
        let path = temp_db("unused-name");
        let name = "octoboard.db.superseded-7";
        assert_eq!(unused_name(&path, name), path.with_file_name(name));
        std::fs::write(path.with_file_name(name), b"earlier").unwrap();
        std::fs::write(path.with_file_name("octoboard.db.superseded-7-2-wal"), b"").unwrap();
        assert_eq!(
            unused_name(&path, name),
            path.with_file_name("octoboard.db.superseded-7-3")
        );
        assert_eq!(
            std::fs::read(path.with_file_name(name)).unwrap(),
            b"earlier"
        );
    }

    /// Moving aside a second pre-release database keeps the first copy as it was.
    #[test]
    fn moving_aside_again_keeps_the_earlier_copy() {
        let path = temp_db("pre-release-twice");
        let write_old = |name: &str| {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(&format!(
                "CREATE TABLE consoles (id TEXT PRIMARY KEY, name TEXT NOT NULL);
                 INSERT INTO consoles VALUES ('console-1', '{name}');"
            ))
            .unwrap();
        };
        write_old("First");
        drop(Store::open(&path).unwrap());
        let first = path.with_file_name(&moved_aside(&path)[0]);
        let first_bytes = std::fs::read(&first).unwrap();
        std::fs::remove_file(&path).unwrap();
        write_old("Second");
        drop(Store::open(&path).unwrap());

        assert_eq!(moved_aside(&path).len(), 2);
        assert_eq!(std::fs::read(&first).unwrap(), first_bytes);
    }
}
