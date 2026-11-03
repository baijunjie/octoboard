//! Codex's thread registry: its thread state database (`state_<n>.sqlite`), where a home keeps it,
//! and the rows of its `threads` table that name where a thread's rollouts are. Codex resumes and
//! lists a thread by the row its database keeps (`threads.rollout_path`, an absolute path recorded
//! through whichever home wrote it), falls back to the rollouts' names only for a thread with no
//! row, and backfills a database from `sessions/` only once (measured with 0.162.1). So rollouts
//! copied or moved to another place are neither listed nor resumed by their rows unless the rows
//! name where the rollouts now are. Only `threads` is written to, and of a row that is already
//! there only its `rollout_path`: the other tables Codex keeps of a thread are its own business,
//! bar the rows Codex's own schema removes with a thread that is forgotten.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension};

use crate::accounts::thread_registry::{Repoint, RowCause, RowError, ThreadRegistry};
use crate::adapter::files::{LARGE_FILE_LIMIT, SMALL_FILE_LIMIT};
use crate::protocol::SharingCause;
use crate::providers::codex::{
    CODEX_ARCHIVED_DIR, CODEX_CONFIG_FILE, CODEX_RECORDS_DIR, CODEX_STATE_DATABASE,
    CODEX_TITLE_INDEX,
};

/// How long a database may be busy before a write to it gives up.
const PATIENCE: Duration = Duration::from_secs(5);

pub(crate) struct CodexThreads;

impl ThreadRegistry for CodexThreads {
    fn is_database(&self, name: &str) -> bool {
        is_state_database(name)
    }

    fn record_dirs(&self) -> &'static [&'static str] {
        &[CODEX_RECORDS_DIR, CODEX_ARCHIVED_DIR]
    }

    fn patience(&self) -> Duration {
        PATIENCE
    }

    fn repoint_rows(&self, database: &Path, repoints: &[Repoint]) -> rusqlite::Result<()> {
        repoint_rows(database, repoints)
    }

    fn bring_rows(
        &self,
        shared: &Path,
        set_aside: &Path,
        repoints: &[Repoint],
    ) -> rusqlite::Result<usize> {
        bring_rows(shared, set_aside, repoints)
    }

    fn forget_threads(&self, database: &Path, threads: &[&str]) -> rusqlite::Result<()> {
        forget_threads(database, threads)
    }

    fn point_thread_at(
        &self,
        home: &Path,
        head: &Path,
        thread: &str,
        launch_env: &HashMap<String, String>,
        patience: Duration,
    ) -> Result<(), RowError> {
        point_thread_at(home, head, thread, launch_env, patience)
    }

    fn after_take_back(&self, default_dir: &Path, account_dir: &Path, thread: &str) {
        copy_thread_names(default_dir, account_dir, thread);
    }
}

/// Whether `name` is a Codex thread state database, `state_<n>.sqlite`: the number is Codex's
/// schema generation, which a newer Codex may raise.
fn is_state_database(name: &str) -> bool {
    let (prefix, suffix) = CODEX_STATE_DATABASE;
    name.strip_prefix(prefix)
        .and_then(|rest| rest.strip_suffix(suffix))
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|byte| byte.is_ascii_digit()))
}

/// Opens a database for writing, waiting up to `patience` while another process has it busy.
/// Never creates one: a file that is not there, or is not a database, is an error. Foreign keys
/// are off on it, as SQLite leaves them: a caller deleting `threads` rows turns them on first, or
/// the child rows stay and attach to a row later inserted with the same id.
fn open_existing(database: &Path, patience: Duration) -> rusqlite::Result<Connection> {
    let connection = Connection::open_with_flags(
        database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.busy_timeout(patience)?;
    Ok(connection)
}

/// Whether the database attached as `schema` (`main` for the one opened) has a `threads` table.
fn has_threads(connection: &Connection, schema: &str) -> rusqlite::Result<bool> {
    Ok(connection
        .query_row(
            &format!(
                "SELECT 1 FROM {schema}.sqlite_master WHERE type = 'table' AND name = 'threads'"
            ),
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// Points the row a Codex home's databases keep of `thread` at `head`, the rollout at the head of
/// the thread's chain in that home: Codex follows the row, so a home that held the thread before it
/// went on elsewhere would otherwise resume the stale head however whole the chain beside it is.
/// The path written is `head` without any `.zst`, the form Codex keeps for a rollout it has
/// compressed. A home with no row of the thread has nothing to point, and neither has one whose row
/// names no path or already names that same file: the path it names and `head` resolve to one file
/// when two homes share `sessions/` and the database, and a row that names another file is pointed
/// even if the database is shared (two homes may keep one database without sharing their
/// rollouts).
///
/// `launch_env` is the environment the home's sessions launch with, which with its `config.toml`
/// says where the home keeps its databases ([`database_directory`]). Where that cannot be told, a
/// directory that cannot be listed, or a database that cannot be written to or stays busy past
/// `patience` is an error: a row left standing would resume the stale head with no sign of it.
fn point_thread_at(
    home: &Path,
    head: &Path,
    thread: &str,
    launch_env: &HashMap<String, String>,
    patience: Duration,
) -> Result<(), RowError> {
    let directory = database_directory(home, launch_env)?;
    let mut rollout = head.to_path_buf();
    if rollout.extension().is_some_and(|ext| ext == "zst") {
        rollout.set_extension("");
    }
    let listing = |err: io::Error| {
        RowError::new(
            RowCause::Registry(SharingCause::of(&err)),
            format!("listing `{}` failed: {err}", directory.display()),
        )
    };
    let entries = match std::fs::read_dir(&directory) {
        Ok(entries) => entries,
        // No directory, no database: the home has not run Codex yet.
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(listing(err)),
    };
    for entry in entries {
        let entry = entry.map_err(listing)?;
        let path = entry.path();
        if !path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(is_state_database)
        {
            continue;
        }
        // A link counts when it leads to a file (an account sharing its database with the default
        // account links it), and a dangling one is no database. A link that cannot be followed
        // for another reason is an error: it is the database, and its row cannot be checked.
        match std::fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => {}
            Ok(_) => continue,
            Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
            Err(err) => {
                return Err(RowError::new(
                    RowCause::Registry(SharingCause::of(&err)),
                    format!("inspecting `{}` failed: {err}", path.display()),
                ))
            }
        }
        point_row(&path, thread, &rollout, patience).map_err(|err| {
            RowError::new(
                RowCause::Registry(SharingCause::of_database(&err)),
                format!("updating `{}` failed: {err}", path.display()),
            )
        })?;
    }
    Ok(())
}

/// Where a Codex home keeps its databases: its own directory, unless its `config.toml` sets
/// `sqlite_home`, which wins over the `CODEX_SQLITE_HOME` environment variable (measured with
/// 0.162.1). A relative `sqlite_home` is taken against the home and a leading `~` is the launch
/// environment's `HOME`; a relative `CODEX_SQLITE_HOME` is taken against the directory Codex is
/// started in, which is not known here, so it is refused. Codex also lets an enterprise
/// requirement override both, which is not looked for here.
fn database_directory(
    home: &Path,
    launch_env: &HashMap<String, String>,
) -> Result<PathBuf, RowError> {
    let configured =
        match crate::hostfs::read_regular_text(&home.join(CODEX_CONFIG_FILE), SMALL_FILE_LIMIT) {
            Ok(text) => toml::from_str::<toml::Table>(&text)
                .map_err(|err| {
                    RowError::new(
                        RowCause::Configuration(SharingCause::Other),
                        format!("`config.toml` is not valid: {err}"),
                    )
                })?
                .get("sqlite_home")
                .and_then(toml::Value::as_str)
                .map(str::to_owned),
            Err(err) if err.kind() == io::ErrorKind::NotFound => None,
            Err(err) => {
                return Err(RowError::new(
                    RowCause::Configuration(SharingCause::of(&err)),
                    err.to_string(),
                ))
            }
        };
    if let Some(dir) = configured {
        let user_home = launch_env.get("HOME").filter(|home| !home.is_empty());
        return match (dir.strip_prefix('~'), user_home) {
            (Some(rest), Some(user_home)) if rest.is_empty() || rest.starts_with('/') => {
                Ok(PathBuf::from(user_home).join(rest.trim_start_matches('/')))
            }
            (Some(_), None) => Err(RowError::new(
                RowCause::Unlocatable,
                "`sqlite_home` starts with `~` and the launch environment has no HOME",
            )),
            _ => Ok(home.join(dir)),
        };
    }
    match launch_env
        .get("CODEX_SQLITE_HOME")
        .filter(|dir| !dir.is_empty())
    {
        Some(dir) if Path::new(dir).is_absolute() => Ok(PathBuf::from(dir)),
        Some(dir) => Err(RowError::new(
            RowCause::Unlocatable,
            format!(
                "`CODEX_SQLITE_HOME` is the relative path `{dir}`, which depends on the directory \
                 the session starts in"
            ),
        )),
        None => Ok(home.to_path_buf()),
    }
}

/// Points the row of `thread` in `database` at `rollout`, unless it names no path or that same
/// file already.
fn point_row(
    database: &Path,
    thread: &str,
    rollout: &Path,
    patience: Duration,
) -> rusqlite::Result<()> {
    let connection = open_existing(database, patience)?;
    if !has_threads(&connection, "main")? {
        return Ok(());
    }
    let named: Option<Option<String>> = connection
        .query_row(
            "SELECT rollout_path FROM threads WHERE id = ?1",
            [thread],
            |row| row.get(0),
        )
        .optional()?;
    // A row that already names the rollout, by whatever route, is left as it is.
    // A row with no path names no rollout to compare, so it is left alone.
    if named
        .flatten()
        .is_some_and(|named| resolved(Path::new(&named)) != resolved(rollout))
    {
        connection.execute(
            "UPDATE threads SET rollout_path = ?1 WHERE id = ?2",
            rusqlite::params![rollout.to_string_lossy(), thread],
        )?;
    }
    Ok(())
}

/// The path a rollout is found at, with the links of its directory resolved. The file itself is
/// not looked up, as the path written for a compressed rollout names one that need not exist; a
/// directory that cannot be resolved leaves the path as written.
fn resolved(path: &Path) -> PathBuf {
    match (path.parent().map(std::fs::canonicalize), path.file_name()) {
        (Some(Ok(parent)), Some(name)) => parent.join(name),
        _ => path.to_path_buf(),
    }
}

/// Points every row of `database` that names a rollout under one of `repoints`' `from`
/// directories at the same rollout under its `to`. A database with no `threads` table has
/// nothing to point.
fn repoint_rows(database: &Path, repoints: &[Repoint]) -> rusqlite::Result<()> {
    let mut connection = open_existing(database, PATIENCE)?;
    if !has_threads(&connection, "main")? {
        return Ok(());
    }
    let transaction = connection.transaction()?;
    repoint_in(&transaction, repoints)?;
    transaction.commit()
}

/// Removes the rows of `threads` from `database`: their rollouts have left the directory it
/// covers. Foreign keys are enforced on the connection, so the rows other tables keep of a thread
/// go or are unset as Codex's own schema declares.
fn forget_threads(database: &Path, threads: &[&str]) -> rusqlite::Result<()> {
    let mut connection = open_existing(database, PATIENCE)?;
    if !has_threads(&connection, "main")? {
        return Ok(());
    }
    connection.pragma_update(None, "foreign_keys", true)?;
    let transaction = connection.transaction()?;
    for thread in threads {
        transaction.execute("DELETE FROM threads WHERE id = ?1", [thread])?;
    }
    transaction.commit()
}

fn repoint_in(connection: &Connection, repoints: &[Repoint]) -> rusqlite::Result<()> {
    for Repoint { from, to } in repoints {
        // Compared as directories, so `…/sessions` does not take `…/sessions-old/…` with it.
        let (from, to) = (directory_prefix(from), directory_prefix(to));
        if from == to {
            continue;
        }
        connection.execute(
            "UPDATE threads SET rollout_path = ?2 || substr(rollout_path, length(?1) + 1)
             WHERE substr(rollout_path, 1, length(?1)) = ?1",
            rusqlite::params![from, to],
        )?;
    }
    Ok(())
}

fn directory_prefix(path: &Path) -> String {
    let mut text = path.to_string_lossy().into_owned();
    if !text.ends_with('/') {
        text.push('/');
    }
    text
}

/// Brings the rows of `set_aside` (an account's own database, moved out of the way) into
/// `shared`, then applies `repoints` to `shared`. A thread `shared` does not know is added with
/// the columns both databases have, its section and project left unset since those name rows of
/// tables that are not carried. A thread both know keeps `shared`'s row, which is only pointed at
/// the account's rollout when the account's row was updated later: after a conversation went on
/// in the account, its head is the account's. Answers how many threads were added.
fn bring_rows(shared: &Path, set_aside: &Path, repoints: &[Repoint]) -> rusqlite::Result<usize> {
    let mut connection = open_existing(shared, PATIENCE)?;
    connection.execute(
        "ATTACH DATABASE ?1 AS account",
        [set_aside.to_string_lossy()],
    )?;
    let result = bring_attached(&mut connection, repoints);
    let detached = connection.execute("DETACH DATABASE account", []);
    let added = result?;
    detached?;
    Ok(added)
}

fn bring_attached(connection: &mut Connection, repoints: &[Repoint]) -> rusqlite::Result<usize> {
    if !has_threads(connection, "main")? || !has_threads(connection, "account")? {
        return Ok(0);
    }
    let shared_columns = columns(connection, "main")?;
    let account_columns = columns(connection, "account")?;
    let unset = foreign_key_columns(connection, "account")?;
    let common: Vec<&String> = shared_columns
        .iter()
        .filter(|column| account_columns.contains(column))
        .collect();
    let names = common
        .iter()
        .map(|column| quoted(column))
        .collect::<Vec<_>>()
        .join(", ");
    let values = common
        .iter()
        .map(|column| {
            if unset.contains(column) {
                "NULL".to_string()
            } else {
                format!("a.{}", quoted(column))
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let transaction = connection.transaction()?;
    let added = transaction.execute(
        &format!(
            "INSERT INTO main.threads ({names}) SELECT {values} FROM account.threads a
             WHERE NOT EXISTS (SELECT 1 FROM main.threads m WHERE m.id = a.id)"
        ),
        [],
    )?;
    let has = |column: &str| common.iter().any(|name| name.as_str() == column);
    if has("updated_at") && has("rollout_path") {
        transaction.execute(
            "UPDATE main.threads SET rollout_path =
                 (SELECT a.rollout_path FROM account.threads a WHERE a.id = main.threads.id)
             WHERE id IN (SELECT a.id FROM account.threads a
                 JOIN main.threads m ON m.id = a.id WHERE a.updated_at > m.updated_at)",
            [],
        )?;
    }
    repoint_in(&transaction, repoints)?;
    transaction.commit()?;
    Ok(added)
}

fn columns(connection: &Connection, schema: &str) -> rusqlite::Result<Vec<String>> {
    let mut statement = connection.prepare(&format!("PRAGMA {schema}.table_info(threads)"))?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(names)
}

fn foreign_key_columns(connection: &Connection, schema: &str) -> rusqlite::Result<Vec<String>> {
    let mut statement =
        connection.prepare(&format!("PRAGMA {schema}.foreign_key_list(threads)"))?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(3))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(names)
}

fn quoted(column: &str) -> String {
    format!("\"{}\"", column.replace('"', "\"\""))
}

/// Copies the default's thread-name entries of Codex thread `id` into the account's own index, so
/// a resume of the thread by name still finds it there. Best effort: a name is not needed to
/// resume by id, which is how a session is resumed.
fn copy_thread_names(default_dir: &Path, account_dir: &Path, id: &str) {
    use std::io::Write;
    let Ok(index) =
        crate::hostfs::read_regular_text(&default_dir.join(CODEX_TITLE_INDEX), LARGE_FILE_LIMIT)
    else {
        return;
    };
    let lines: String = index
        .lines()
        .filter(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .is_ok_and(|entry| entry.get("id").and_then(|value| value.as_str()) == Some(id))
        })
        .map(|line| format!("{line}\n"))
        .collect();
    if lines.is_empty() {
        return;
    }
    let target = account_dir.join(CODEX_TITLE_INDEX);
    // An index that is there but cannot be read is not appended to: it may hold the names already,
    // or be no file to write to.
    let already = match crate::hostfs::read_regular_text(&target, LARGE_FILE_LIMIT) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => {
            tracing::warn!(thread = %id, %err, "reading the account's Codex thread names failed");
            return;
        }
    };
    if already.contains(&lines) {
        return;
    }
    let written = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&target)
        .and_then(|mut file| file.write_all(lines.as_bytes()));
    if let Err(err) = written {
        tracing::warn!(thread = %id, %err, "copying a Codex thread's name into the account failed");
    }
}

#[cfg(test)]
mod tests;
