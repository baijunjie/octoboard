//! What sharing does to Codex's thread state database (`state_<n>.sqlite`) beyond linking it.
//! Codex resumes and lists a thread by the row its database keeps (`threads.rollout_path`, an
//! absolute path recorded through whichever home wrote it), and backfills a database from
//! `sessions/` only once, so rollouts carried into a shared directory are neither listed nor
//! resumed by their rows unless the rows come along and name where the rollouts now are. Only
//! `threads` is touched, and of a row that is already there only its `rollout_path`: the other
//! tables Codex keeps of a thread are its own business.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension};

/// How long a database may be busy before a write to it gives up.
const PATIENCE: Duration = Duration::from_secs(5);

/// A rewrite of the paths rows record: every `rollout_path` that starts with `from` (a directory)
/// is made to start with `to` instead.
#[derive(Debug, Clone)]
pub(super) struct Repoint {
    pub from: PathBuf,
    pub to: PathBuf,
}

fn open(database: &Path) -> rusqlite::Result<Connection> {
    crate::relocate::open_existing_database(database, PATIENCE)
}

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

/// Points every row of `database` that names a rollout under one of `repoints`' `from`
/// directories at the same rollout under its `to`. A database with no `threads` table has
/// nothing to point.
pub(super) fn repoint_rows(database: &Path, repoints: &[Repoint]) -> rusqlite::Result<()> {
    let mut connection = open(database)?;
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
pub(super) fn forget_threads(database: &Path, threads: &[&str]) -> rusqlite::Result<()> {
    let mut connection = open(database)?;
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
pub(super) fn bring_rows(
    shared: &Path,
    set_aside: &Path,
    repoints: &[Repoint],
) -> rusqlite::Result<usize> {
    let mut connection = open(shared)?;
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
