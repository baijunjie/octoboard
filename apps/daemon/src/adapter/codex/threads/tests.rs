use super::*;
use crate::test_support::ScratchDir;

fn temp_dir(name: &str) -> ScratchDir {
    ScratchDir::new(&format!("codex-threads-{name}"))
}

const CODEX_ID: &str = "019a0000-0000-7000-8000-000000000001";

/// Makes each of `names` in `account` a link to a directory of the same name in `default`, the
/// way an account shares its conversation directories with the agent's default account.
fn share(account: &Path, default: &Path, names: &[&str]) {
    for name in names {
        std::fs::create_dir_all(default.join(name)).unwrap();
        crate::test_support::symlink(default.join(name), account.join(name)).unwrap();
    }
}

fn head_of(home: &Path) -> (crate::test_support::CodexChain, PathBuf) {
    let chain = crate::test_support::codex_chain(home, CODEX_ID, true);
    let relative = chain.head.strip_prefix(home).unwrap().to_path_buf();
    (chain, relative)
}

fn row_of(database: &Path) -> Option<String> {
    rusqlite::Connection::open(database)
        .unwrap()
        .query_row(
            "SELECT rollout_path FROM threads WHERE id = ?1",
            [CODEX_ID],
            |row| row.get(0),
        )
        .unwrap()
}

/// Makes `path` a database whose row of the thread names `rollout` (no path when `None`).
fn database_naming(path: &Path, rollout: Option<&str>) {
    let connection = rusqlite::Connection::open(path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS threads (id TEXT PRIMARY KEY, rollout_path TEXT)",
        )
        .unwrap();
    connection
        .execute(
            "INSERT OR REPLACE INTO threads VALUES (?1, ?2)",
            rusqlite::params![CODEX_ID, rollout],
        )
        .unwrap();
}

fn point_head(target: &Path, head: &Path, env: &[(&str, &Path)]) {
    let env = env
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string_lossy().into_owned()))
        .collect();
    point_thread_at(
        target,
        &target.join(head),
        CODEX_ID,
        &env,
        std::time::Duration::from_millis(50),
    )
    .unwrap();
}

fn uncompressed(path: &Path) -> String {
    path.with_extension("").to_string_lossy().into_owned()
}

/// A Codex row is rewritten only when the rollout it names is another file than the head:
/// left alone when it names the head by another route (both accounts link `sessions/` and the
/// database file into the default's) or names no path, pointed at the head when it names an
/// older segment of a shared `sessions/`, and when the database is shared but `sessions/` is
/// not (two homes given one `CODEX_SQLITE_HOME`).
#[test]
fn a_row_is_rewritten_only_when_it_names_another_file_than_the_head() {
    // Both link `sessions/` and the database file: the row names the source's head, which is
    // the target's head.
    let default = temp_dir("row-linked-default");
    let (source, target) = (temp_dir("row-linked-source"), temp_dir("row-linked-target"));
    share(&source, &default, &["sessions"]);
    share(&target, &default, &["sessions"]);
    let (chain, head) = head_of(&source);
    let linked = default.join("state_5.sqlite");
    let named = uncompressed(&chain.head);
    database_naming(&linked, Some(&named));
    for home in [&source, &target] {
        crate::test_support::symlink(&linked, home.join("state_5.sqlite")).unwrap();
    }
    point_head(&target, &head, &[]);
    assert_eq!(row_of(&linked), Some(named));

    // A row with no path is left alone.
    database_naming(&linked, None);
    point_head(&target, &head, &[]);
    assert_eq!(row_of(&linked), None);

    // One database, `sessions/` not shared: the row names the source's file, another file.
    let (source, target) = (temp_dir("row-own-source"), temp_dir("row-own-target"));
    let (chain, head) = head_of(&source);
    head_of(&target);
    let shared_database = temp_dir("row-shared-database");
    let database = shared_database.join("state_5.sqlite");
    database_naming(&database, Some(&uncompressed(&chain.head)));
    point_head(&target, &head, &[("CODEX_SQLITE_HOME", &shared_database)]);
    assert_eq!(row_of(&database), Some(uncompressed(&target.join(&head))));

    // `sessions/` shared, the target's own database names an older segment of the chain.
    let default = temp_dir("row-older-default");
    let (source, target) = (temp_dir("row-older-source"), temp_dir("row-older-target"));
    share(&source, &default, &["sessions"]);
    share(&target, &default, &["sessions"]);
    let (chain, head) = head_of(&source);
    let database = target.join("state_5.sqlite");
    database_naming(&database, Some(&uncompressed(&chain.middle)));
    point_head(&target, &head, &[]);
    assert_eq!(row_of(&database), Some(uncompressed(&target.join(&head))));
}

/// Of what a home's database directory holds, a `state_<n>.sqlite` that is a link to a file is
/// a database (an account sharing its database with the default account links it), and a link
/// to nothing is not; anything else is never looked at, a link that cannot be followed
/// included.
#[test]
fn the_database_scan_follows_a_linked_file_and_ignores_what_is_not_one() {
    // A target linking `sessions/` and the database file: the chain is reached through the link,
    // and the row, naming an older segment, is pointed at it.
    let default = temp_dir("scan-default");
    let target = temp_dir("scan-target");
    share(&target, &default, &["sessions"]);
    let (chain, head) = head_of(&default);
    let linked = default.join("state_5.sqlite");
    database_naming(&linked, Some(&uncompressed(&chain.middle)));
    crate::test_support::symlink(&linked, target.join("state_5.sqlite")).unwrap();
    // A link that loops, and is no database, does not fail the switch.
    crate::test_support::symlink("loop", target.join("loop")).unwrap();
    point_head(&target, &head, &[]);
    assert_eq!(row_of(&linked), Some(uncompressed(&target.join(&head))));

    // A link to nothing is no database.
    let dangling = temp_dir("scan-dangling");
    crate::test_support::symlink(
        dangling.join("gone.sqlite"),
        dangling.join("state_5.sqlite"),
    )
    .unwrap();
    point_head(&dangling, &head, &[]);
}

/// Codex's thread state databases are `state_<generation>.sqlite`, whatever the generation.
#[test]
fn a_state_database_is_named_by_its_generation() {
    for name in ["state_5.sqlite", "state_12.sqlite"] {
        assert!(is_state_database(name), "{name}");
    }
    for name in [
        "state_.sqlite",
        "state_5.sqlite-wal",
        "state_x.sqlite",
        "logs_2.sqlite",
        "state_5.db",
    ] {
        assert!(!is_state_database(name), "{name}");
    }
}
