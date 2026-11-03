use super::*;
use crate::records::carried::companions;
use crate::test_support::{code_of, param_of, ScratchDir};

fn temp_dir(name: &str) -> ScratchDir {
    ScratchDir::new(&format!("relocate-{name}"))
}

fn write(root: &Path, relative: &str, content: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

/// The record keeps the path it has relative to the config directory, whatever the agent put
/// in the middle of it, with the directories in between created; the original stays; and a
/// neighbour that is not this session's is not copied. Nothing in it derives a slug or an
/// encoding.
#[test]
fn the_record_keeps_its_path_relative_to_the_config_directory_and_is_copied_not_moved() {
    // The record, a file inside it (the record itself unless it is a directory) and a
    // neighbour belonging to another session.
    let cases = [
        (
            Agent::Claude,
            CLAUDE_ID,
            format!("projects/-Users-me-repo/{CLAUDE_ID}.jsonl"),
            format!("projects/-Users-me-repo/{CLAUDE_ID}.jsonl"),
            format!("projects/-Users-me-repo/{OTHER_ID}.jsonl"),
        ),
        (
            Agent::Codex,
            CODEX_ID,
            format!("sessions/2026/10/08/rollout-2026-10-08T01-02-03-{CODEX_ID}.jsonl"),
            format!("sessions/2026/10/08/rollout-2026-10-08T01-02-03-{CODEX_ID}.jsonl"),
            format!("sessions/2026/10/08/rollout-2026-10-08T01-02-04-{OTHER_ID}.jsonl"),
        ),
        (
            Agent::Grok,
            GROK_ID,
            format!("sessions/%2FUsers%2Fme%2Frepo/{GROK_ID}"),
            format!("sessions/%2FUsers%2Fme%2Frepo/{GROK_ID}/messages.jsonl"),
            format!("sessions/%2FUsers%2Fme%2Frepo/{OTHER_ID}/messages.jsonl"),
        ),
    ];
    for (agent, id, record, file, neighbour) in cases {
        let source = temp_dir(&format!("{}-source", agent.label()));
        let target = temp_dir(&format!("{}-target", agent.label()));
        write(&source, &file, "conversation");
        write(&source, &neighbour, "someone else's");

        let relative = locate(agent, &source, id).expect("found");
        assert_eq!(relative, Path::new(&record), "{agent:?}");

        copy(&source, &target, &relative, &[]).expect("copied");
        assert_eq!(
            std::fs::read_to_string(target.join(&file)).unwrap(),
            "conversation",
            "{agent:?}"
        );
        assert!(source.join(&file).exists(), "{agent:?} was moved");
        assert!(
            !target.join(&neighbour).exists(),
            "{agent:?} copied a neighbour"
        );
    }
}

const CLAUDE_ID: &str = "11111111-2222-3333-4444-555555555555";
const CODEX_ID: &str = "019a0000-0000-7000-8000-000000000001";
const GROK_ID: &str = "22222222-3333-4444-5555-666666666666";
const OTHER_ID: &str = "99999999-8888-7777-6666-555555555555";

/// A record is found by the rules every reader of an agent's records keeps: a session id that is
/// a canonical UUID, and an entry at the agent's own depth under the records directory, so a name
/// that merely matches one level up or down is not the record.
#[test]
fn a_record_is_taken_only_by_a_canonical_id_at_the_agents_own_depth() {
    let source = temp_dir("own-depth");
    write(
        &source,
        "projects/slug/claude-id.jsonl",
        "not a canonical id",
    );
    write(
        &source,
        &format!("projects/slug/deeper/{CLAUDE_ID}.jsonl"),
        "one level too deep",
    );
    write(
        &source,
        &format!("sessions/2026/10/08/extra/rollout-2026-10-08T01-02-03-{CODEX_ID}.jsonl"),
        "one level too deep",
    );
    write(
        &source,
        &format!("sessions/{GROK_ID}/messages.jsonl"),
        "one level too shallow",
    );
    for (agent, id) in [
        (Agent::Claude, "claude-id"),
        (Agent::Claude, CLAUDE_ID),
        (Agent::Codex, CODEX_ID),
        (Agent::Grok, GROK_ID),
    ] {
        let err = locate(agent, &source, id).expect_err(id);
        assert_eq!(code_of(&err), error_code::CONVERSATION_NOT_FOUND, "{id}");
    }
}

/// A Codex thread that had a turn reverted is carried whole: the head is the record, the
/// first rollout and the middle segment travel with it as companions, compressed ones as they
/// are (the bytes stay compressed). A subagent's rollout, named by its own thread id, stays
/// behind.
#[test]
fn a_reverted_codex_thread_is_carried_whole() {
    for compressed in [false, true] {
        let source = temp_dir("codex-source");
        let target = temp_dir("codex-target");
        let chain = crate::test_support::codex_chain(&source, CODEX_ID, compressed);
        let relative = |path: &Path| path.strip_prefix(&*source).unwrap().to_path_buf();
        let child = "019a0000-0000-7000-8000-00000000000a";
        let subagent = format!("sessions/2026/10/01/rollout-2026-10-01T01-05-00-{child}.jsonl");
        write(&source, &subagent, "subagent");

        let located = locate(Agent::Codex, &source, CODEX_ID).expect("found");
        assert_eq!(located, relative(&chain.head), "compressed: {compressed}");
        let mut relatives = companions(Agent::Codex, &source, &located, CODEX_ID);
        relatives.sort();
        assert_eq!(
            relatives,
            [relative(&chain.first), relative(&chain.middle)],
            "compressed: {compressed}"
        );

        copy(&source, &target, &located, &relatives).expect("copied");
        for file in chain.all() {
            assert_eq!(
                std::fs::read(target.join(relative(&file))).unwrap(),
                std::fs::read(&file).unwrap(),
                "{file:?}"
            );
        }
        assert!(
            !target.join(&subagent).exists(),
            "the subagent's was carried"
        );
        assert!(source.join(relative(&chain.head)).is_file(), "moved");
    }
}

/// The target's database row of the thread is pointed at the carried head and nothing else of
/// it or of the database changes; the database is found where the home's `config.toml` or the
/// environment puts it; a home with no database, or one with no `threads` table, has nothing to
/// point; a database that stays busy or is no database fails the switch, saying the
/// conversation was copied.
#[test]
fn a_codex_homes_row_of_the_thread_is_pointed_at_the_head_and_nothing_else() {
    use rusqlite::Connection;
    let target = temp_dir("point-row");
    let other = "019a0000-0000-7000-8000-0000000000bb";
    let head = Path::new("sessions/2026/10/03/rollout-2026-10-03T00-00-00-x_y.jsonl.zst");
    let database = target.join("state_5.sqlite");
    let connection = Connection::open(&database).unwrap();
    connection
        .execute_batch(&format!(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, rollout_path TEXT, title TEXT);
             CREATE TABLE thread_attachments (thread_id TEXT NOT NULL
                 REFERENCES threads(id) ON DELETE CASCADE);
             INSERT INTO threads VALUES ('{CODEX_ID}', 'old', 'kept'), ('{other}', 'other', 'x');
             INSERT INTO thread_attachments VALUES ('{CODEX_ID}');"
        ))
        .unwrap();
    let path_of = |id: &str, database: &Path| -> String {
        Connection::open(database)
            .unwrap()
            .query_row(
                "SELECT rollout_path FROM threads WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .unwrap()
    };
    let patience = std::time::Duration::from_millis(50);
    let point = |target: &Path, thread: &str, env: &[(&str, &str)]| {
        let env = env
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        repoint_within(Agent::Codex, target, head, thread, &env, patience)
    };

    let held = Connection::open(&database).unwrap();
    held.execute_batch("BEGIN EXCLUSIVE").unwrap();
    let err = point(&target, CODEX_ID, &[]).expect_err("a busy database");
    assert_eq!(code_of(&err), error_code::RELOCATION_FAILED);
    assert_eq!(param_of(&err, "stage"), "thread_record");
    assert_eq!(param_of(&err, "cause"), "busy");
    assert!(
        err.to_string().contains("conversation was copied"),
        "{err:#}"
    );
    held.execute_batch("ROLLBACK").unwrap();
    assert_eq!(path_of(CODEX_ID, &database), "old");

    point(&target, CODEX_ID, &[]).unwrap();
    // The head's path in the target, without the `.zst` Codex has compressed it to.
    let expected = target
        .join(head)
        .with_extension("")
        .to_string_lossy()
        .into_owned();
    assert_eq!(path_of(CODEX_ID, &database), expected);
    assert_eq!(path_of(other, &database), "other");
    let conn = Connection::open(&database).unwrap();
    let kept: (String, i64) = conn
        .query_row(
            "SELECT (SELECT title FROM threads WHERE id = ?1),
                    (SELECT count(*) FROM thread_attachments)",
            [CODEX_ID],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(kept, ("kept".to_owned(), 1), "more than the path changed");

    // A thread with no row, another agent, no database, no table, no database file.
    point(&target, other.replace("bb", "cc").as_str(), &[]).unwrap();
    repoint_within(
        Agent::Claude,
        &target,
        head,
        CODEX_ID,
        &HashMap::new(),
        patience,
    )
    .unwrap();
    point(&temp_dir("point-none"), CODEX_ID, &[]).unwrap();
    let bare = temp_dir("point-bare");
    Connection::open(bare.join("state_9.sqlite"))
        .unwrap()
        .execute_batch("CREATE TABLE unrelated (id TEXT)")
        .unwrap();
    point(&bare, CODEX_ID, &[]).unwrap();
    let broken = temp_dir("point-broken");
    write(
        &broken,
        "state_5.sqlite",
        "not a database, but long enough to be read as one",
    );
    point(&broken, CODEX_ID, &[]).expect_err("a file that is no database");

    // The database lives where `config.toml` says, else where the environment says; the
    // configuration wins. `~` is the launch environment's HOME.
    let moved = temp_dir("point-moved");
    let elsewhere = moved.join("elsewhere");
    let (elsewhere_text, user_home) = (elsewhere.to_string_lossy(), moved.join("user"));
    let user_home_text = user_home.to_string_lossy();
    // (name, config.toml, launch environment, where the database is)
    type Case<'a> = (&'a str, Option<&'a str>, Vec<(&'a str, &'a str)>, PathBuf);
    let cases: [Case; 4] = [
        (
            "config",
            Some("sqlite_home = \"db\""),
            vec![],
            moved.join("config/db"),
        ),
        (
            "env",
            None,
            vec![("CODEX_SQLITE_HOME", &elsewhere_text)],
            elsewhere.clone(),
        ),
        (
            "both",
            Some("sqlite_home = \"db\""),
            vec![("CODEX_SQLITE_HOME", &elsewhere_text)],
            moved.join("both/db"),
        ),
        (
            "tilde",
            Some("sqlite_home = \"~/tdb\""),
            vec![("HOME", &user_home_text)],
            user_home.join("tdb"),
        ),
    ];
    for (name, config, env, directory) in cases {
        let home = moved.join(name);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        if let Some(config) = config {
            write(&home, "config.toml", config);
        }
        let moved_database = directory.join("state_5.sqlite");
        std::fs::copy(&database, &moved_database).unwrap();
        Connection::open(&moved_database)
            .unwrap()
            .execute("UPDATE threads SET rollout_path = 'old'", [])
            .unwrap();
        point(&home, CODEX_ID, &env).unwrap();
        let expected = home
            .join(head)
            .with_extension("")
            .to_string_lossy()
            .into_owned();
        assert_eq!(path_of(CODEX_ID, &moved_database), expected, "{name}");
    }

    // Where the database is cannot be told: not guessed past. Nor can an unlistable
    // directory (the setting names a file) be taken for "no database"; a missing one can.
    for (name, config, env) in [
        ("unreadable", "sqlite_home = [", vec![]),
        ("no-home", "sqlite_home = \"~/db\"", vec![]),
        ("relative-env", "", vec![("CODEX_SQLITE_HOME", "db")]),
        (
            "not-a-directory",
            "sqlite_home = \"state_5.sqlite\"",
            vec![],
        ),
    ] {
        let home = moved.join(name);
        write(&home, "config.toml", config);
        let in_home = home.join("state_5.sqlite");
        if name == "not-a-directory" {
            write(&home, "state_5.sqlite", "a file where a directory is named");
        } else {
            // A valid database in the home itself: a fallback to it would be caught.
            std::fs::copy(&database, &in_home).unwrap();
            Connection::open(&in_home)
                .unwrap()
                .execute("UPDATE threads SET rollout_path = 'old'", [])
                .unwrap();
        }
        point(&home, CODEX_ID, &env).expect_err(name);
        if name != "not-a-directory" {
            assert_eq!(
                path_of(CODEX_ID, &in_home),
                "old",
                "{name} fell back to the home"
            );
        }
    }
    let absent = moved.join("absent");
    write(&absent, "config.toml", "sqlite_home = \"nowhere\"");
    point(&absent, CODEX_ID, &[]).unwrap();
}

/// Claude Code's directory beside the transcript and the rewind checkpoints travel with it
/// when they exist, and are not invented when they do not.
#[test]
fn a_claude_transcripts_sidecar_directory_and_rewind_checkpoints_are_carried() {
    let source = temp_dir("claude-sidecar-source");
    let target = temp_dir("claude-sidecar-target");
    let record = format!("projects/-Users-me-repo/{CLAUDE_ID}.jsonl");
    write(&source, &record, "conversation");
    write(
        &source,
        &format!("projects/-Users-me-repo/{CLAUDE_ID}/subagents/agent-1.jsonl"),
        "subagent",
    );
    write(
        &source,
        &format!("projects/-Users-me-repo/{CLAUDE_ID}/tool-results/r.txt"),
        "result",
    );
    write(
        &source,
        &format!("file-history/{CLAUDE_ID}/0123@v1"),
        "checkpoint",
    );
    write(&source, "file-history/another/0123@v1", "someone else's");

    let located = locate(Agent::Claude, &source, CLAUDE_ID).expect("found");
    let relatives = companions(Agent::Claude, &source, &located, CLAUDE_ID);
    assert_eq!(
        relatives,
        [
            PathBuf::from(format!("projects/-Users-me-repo/{CLAUDE_ID}")),
            PathBuf::from(format!("file-history/{CLAUDE_ID}")),
        ]
    );
    copy(&source, &target, &located, &relatives).expect("copied");

    for file in [
        record.clone(),
        format!("projects/-Users-me-repo/{CLAUDE_ID}/subagents/agent-1.jsonl"),
        format!("projects/-Users-me-repo/{CLAUDE_ID}/tool-results/r.txt"),
        format!("file-history/{CLAUDE_ID}/0123@v1"),
    ] {
        assert!(target.join(&file).is_file(), "{file} was not carried");
        assert!(source.join(&file).is_file(), "{file} was moved");
    }
    assert!(!target.join("file-history/another").exists());

    // A session with neither has only its transcript, and an id that is no UUID names no path.
    let bare = temp_dir("claude-sidecar-bare");
    write(&bare, &record, "conversation");
    assert!(companions(Agent::Claude, &bare, Path::new(&record), CLAUDE_ID).is_empty());
    assert!(companions(Agent::Claude, &source, Path::new(&record), "..").is_empty());
}

/// The items of one switch are replaced and rolled back together: a copy that cannot complete
/// on any of them leaves the target as it was, and so does a swap that fails part-way, after
/// the earlier items were already in place.
#[test]
fn the_carried_items_complete_or_roll_back_together() {
    let source = temp_dir("together-source");
    let record = format!("projects/slug/{CLAUDE_ID}.jsonl");
    let sidecar = format!("file-history/{CLAUDE_ID}");
    write(&source, &record, "new conversation");
    write(&source, &format!("{sidecar}/a@v1"), "new checkpoint");
    let (record_path, companions) = (PathBuf::from(&record), [PathBuf::from(&sidecar)]);

    // The second item cannot be copied: the first is not left in the target.
    let target = temp_dir("together-target");
    std::os::unix::fs::symlink("/etc/hosts", source.join(&sidecar).join("link")).unwrap();
    let err = copy(&source, &target, &record_path, &companions).expect_err("a link in the sidecar");
    assert_eq!(code_of(&err), error_code::RELOCATION_FAILED);
    assert_eq!(param_of(&err, "stage"), "copy");
    assert_eq!(param_of(&err, "cause"), "symlink");
    assert!(!target.join(&record).exists());
    assert!(!target.join(STAGING).exists());
    std::fs::remove_file(source.join(&sidecar).join("link")).unwrap();

    // The second item cannot be put in place: the first is put back as it was.
    let blocked = temp_dir("together-blocked");
    write(&blocked, &record, "older conversation");
    write(&blocked, "file-history", "a file where a directory must be");
    let err = copy(&source, &blocked, &record_path, &companions).expect_err("blocked");
    assert_eq!(code_of(&err), error_code::RELOCATION_FAILED);
    assert_eq!(param_of(&err, "cause"), "other");
    assert_eq!(
        std::fs::read_to_string(blocked.join(&record)).unwrap(),
        "older conversation"
    );
    assert!(!blocked.join(STAGING).exists());

    // And with nothing in the way both are in place.
    copy(&source, &target, &record_path, &companions).expect("copied");
    assert!(target.join(&record).is_file() && target.join(&sidecar).join("a@v1").is_file());
}

/// Switching back finds an older copy in the account it returns to, and the newer record
/// replaces it whole — a stale file of the older copy does not survive in a directory record.
#[test]
fn a_record_already_in_the_target_is_replaced() {
    let source = temp_dir("replace-source");
    let target = temp_dir("replace-target");
    write(&source, "sessions/enc/grok-id/messages.jsonl", "new");
    write(&target, "sessions/enc/grok-id/messages.jsonl", "old");
    write(&target, "sessions/enc/grok-id/stale.json", "stale");

    copy(&source, &target, Path::new("sessions/enc/grok-id"), &[]).expect("copied");

    assert_eq!(
        std::fs::read_to_string(target.join("sessions/enc/grok-id/messages.jsonl")).unwrap(),
        "new"
    );
    assert!(!target.join("sessions/enc/grok-id/stale.json").exists());

    // A file record is replaced by the rename itself, and nothing of the staging stays.
    write(&source, "projects/slug/claude-id.jsonl", "new");
    write(&target, "projects/slug/claude-id.jsonl", "old");
    copy(
        &source,
        &target,
        Path::new("projects/slug/claude-id.jsonl"),
        &[],
    )
    .expect("copied");
    assert_eq!(
        std::fs::read_to_string(target.join("projects/slug/claude-id.jsonl")).unwrap(),
        "new"
    );
    assert!(!target.join(STAGING).exists());
}

#[test]
fn a_session_with_no_record_in_the_account_is_not_found() {
    let source = temp_dir("not-found");
    write(&source, "projects/slug/another.jsonl", "x");

    let err = locate(Agent::Claude, &source, CLAUDE_ID).expect_err("nothing there");
    assert_eq!(code_of(&err), error_code::CONVERSATION_NOT_FOUND);
    let err = locate(Agent::Claude, &source.join("missing"), CLAUDE_ID).expect_err("no dir");
    assert_eq!(code_of(&err), error_code::CONVERSATION_NOT_FOUND);
    // A link named like the record is not the record.
    std::os::unix::fs::symlink(
        "/etc/hosts",
        source.join(format!("projects/slug/{CLAUDE_ID}.jsonl")),
    )
    .unwrap();
    let err = locate(Agent::Claude, &source, CLAUDE_ID).expect_err("a link");
    assert_eq!(code_of(&err), error_code::CONVERSATION_NOT_FOUND);
}

/// Two accounts that share one directory have nothing to copy, and copying onto itself must
/// not delete the record on the way.
#[test]
fn a_shared_directory_is_left_alone() {
    let dir = temp_dir("shared");
    write(&dir, "projects/slug/claude-id.jsonl", "conversation");
    let before = inode(&dir, "projects/slug/claude-id.jsonl");

    copy(&dir, &dir, Path::new("projects/slug/claude-id.jsonl"), &[]).expect("nothing to do");

    assert_eq!(inode(&dir, "projects/slug/claude-id.jsonl"), before);
    assert_eq!(
        std::fs::read_to_string(dir.join("projects/slug/claude-id.jsonl")).unwrap(),
        "conversation"
    );
}

/// Makes each of `names` in `account` a link to a directory of the same name in `default`, the
/// way an account shares its conversation directories with the agent's default account.
fn share(account: &Path, default: &Path, names: &[&str]) {
    for name in names {
        std::fs::create_dir_all(default.join(name)).unwrap();
        std::os::unix::fs::symlink(default.join(name), account.join(name)).unwrap();
    }
}

/// What a switch of the session does to the files: locate, collect the companions, copy.
fn carry(agent: Agent, source: &Path, target: &Path, id: &str) -> PathBuf {
    let record = locate(agent, source, id).expect("found");
    let companions = companions(agent, source, &record, id);
    copy(source, target, &record, &companions).expect("copied");
    record
}

/// The inode a path resolves to: a replaced item, even with the same content, has another.
fn inode(root: &Path, relative: &str) -> u64 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(root.join(relative)).unwrap().ino()
}

fn read(root: &Path, relative: &str) -> String {
    std::fs::read_to_string(root.join(relative)).unwrap()
}

/// Claude Code accounts that share `projects/` and `file-history/` hold the one conversation
/// already: a switch copies nothing and the transcript, sidecar directory and checkpoints stay
/// as they were. Sharing only `projects/` copies the checkpoints alone.
#[test]
fn claude_accounts_sharing_their_directories_copy_only_what_is_not_shared() {
    let record = format!("projects/slug/{CLAUDE_ID}.jsonl");
    let sidecar = format!("projects/slug/{CLAUDE_ID}/tool-results/r.txt");
    let checkpoint = format!("file-history/{CLAUDE_ID}/0123@v1");
    for (shared, checkpoints_copied) in [
        (&["projects", "file-history"][..], false),
        (&["projects"][..], true),
    ] {
        let default = temp_dir("claude-shared-default");
        let source = temp_dir("claude-shared-source");
        let target = temp_dir("claude-shared-target");
        share(&source, &default, shared);
        share(&target, &default, shared);
        write(&source, &record, "conversation");
        write(&source, &sidecar, "result");
        write(&source, &checkpoint, "checkpoint");
        let sidecar_dir = format!("projects/slug/{CLAUDE_ID}");
        let checkpoint_dir = format!("file-history/{CLAUDE_ID}");
        let items = [&record, &sidecar_dir, &checkpoint_dir];
        let before = items.map(|item| inode(&source, item));

        carry(Agent::Claude, &source, &target, CLAUDE_ID);

        assert_eq!(items.map(|item| inode(&source, item)), before, "{shared:?}");
        for (file, content) in [
            (&record, "conversation"),
            (&sidecar, "result"),
            (&checkpoint, "checkpoint"),
        ] {
            assert_eq!(read(&source, file), content, "{shared:?} {file}");
        }
        // Only an unshared account ends up with checkpoints in a directory of its own.
        let own_checkpoints = std::fs::symlink_metadata(target.join("file-history"))
            .is_ok_and(|metadata| metadata.is_dir());
        assert_eq!(own_checkpoints, checkpoints_copied, "{shared:?}");
        if checkpoints_copied {
            assert_eq!(read(&target, &checkpoint), "checkpoint");
        }
        assert!(!target.join(STAGING).exists(), "{shared:?} left staging");
    }
}

/// A switch from a sharing account to one that does not share puts the conversation in the
/// target's own real directories; the other way, it is copied through the target's links into
/// the default account's directories.
#[test]
fn a_switch_between_a_sharing_and_a_non_sharing_account_copies_either_way() {
    let record = format!("projects/slug/{CLAUDE_ID}.jsonl");
    let checkpoint = format!("file-history/{CLAUDE_ID}/0123@v1");
    let names = ["projects", "file-history"];

    let default = temp_dir("mixed-default");
    let sharing = temp_dir("mixed-sharing");
    let own = temp_dir("mixed-own");
    share(&sharing, &default, &names);
    write(&sharing, &record, "conversation");
    write(&sharing, &checkpoint, "checkpoint");
    carry(Agent::Claude, &sharing, &own, CLAUDE_ID);
    for file in [&record, &checkpoint] {
        assert!(own.join(file).is_file(), "{file}");
    }

    let default = temp_dir("mixed-default-back");
    let sharing = temp_dir("mixed-sharing-back");
    share(&sharing, &default, &names);
    carry(Agent::Claude, &own, &sharing, CLAUDE_ID);
    for (file, content) in [(&record, "conversation"), (&checkpoint, "checkpoint")] {
        assert_eq!(read(&default, file), content, "{file}");
    }
    assert!(!sharing.join(STAGING).exists());
}

/// Codex accounts that share `sessions/` hold the whole chain of rollouts already, compressed
/// segment included: nothing is copied and every rollout is as it was.
#[test]
fn codex_accounts_sharing_their_sessions_copy_nothing() {
    use std::os::unix::fs::MetadataExt;
    let stamp = |file: &PathBuf| {
        (
            std::fs::read(file).unwrap(),
            std::fs::metadata(file).unwrap().ino(),
        )
    };
    let default = temp_dir("codex-shared-default");
    let source = temp_dir("codex-shared-source");
    let target = temp_dir("codex-shared-target");
    share(&source, &default, &["sessions"]);
    share(&target, &default, &["sessions"]);
    let chain = crate::test_support::codex_chain(&source, CODEX_ID, true);
    let before: Vec<(Vec<u8>, u64)> = chain.all().iter().map(stamp).collect();

    carry(Agent::Codex, &source, &target, CODEX_ID);

    let after: Vec<(Vec<u8>, u64)> = chain.all().iter().map(stamp).collect();
    assert_eq!(before, after);
    assert!(!target.join(STAGING).exists());
}

/// Grok Build accounts that share `sessions/` copy nothing and the session directory stays.
#[test]
fn grok_accounts_sharing_their_sessions_copy_nothing() {
    let default = temp_dir("grok-shared-default");
    let source = temp_dir("grok-shared-source");
    let target = temp_dir("grok-shared-target");
    share(&source, &default, &["sessions"]);
    share(&target, &default, &["sessions"]);
    let record = format!("sessions/enc/{GROK_ID}");
    write(&source, &format!("{record}/messages.jsonl"), "conversation");

    let before = inode(&source, &record);

    carry(Agent::Grok, &source, &target, GROK_ID);

    assert_eq!(inode(&source, &record), before);
    assert_eq!(
        read(&source, &format!("{record}/messages.jsonl")),
        "conversation"
    );
    assert!(!target.join(STAGING).exists());
}

/// A copy that cannot complete is a failure with nothing half-written left behind, whether the
/// target cannot be created or the record holds something that is not copied.
#[test]
fn an_incomplete_copy_fails_and_leaves_nothing_in_the_target() {
    let source = temp_dir("incomplete-source");
    write(&source, "sessions/enc/grok-id/messages.jsonl", "x");
    std::os::unix::fs::symlink("/etc/hosts", source.join("sessions/enc/grok-id/link")).unwrap();
    let target = temp_dir("incomplete-target");
    let blocked = temp_dir("incomplete-blocked");
    std::fs::write(blocked.join("sessions"), "a file where a directory must be").unwrap();

    for (target, why) in [(&target, "symlink"), (&blocked, "blocked")] {
        let err = copy(&source, target, Path::new("sessions/enc/grok-id"), &[]).expect_err(why);
        assert_eq!(code_of(&err), error_code::RELOCATION_FAILED, "{why}");
    }
    assert!(!target.join("sessions/enc/grok-id").exists());
    for dir in [&target, &blocked] {
        assert!(!dir.join(STAGING).exists(), "staging left in {dir:?}");
    }

    // An older copy already in the target survives a copy that fails, and a staging directory
    // an earlier crash left behind for this record is cleared rather than trusted.
    write(&target, "sessions/enc/grok-id/messages.jsonl", "older");
    write(
        &target,
        ".octoboard-switch/grok-id/leftover",
        "from a crash",
    );
    copy(&source, &target, Path::new("sessions/enc/grok-id"), &[]).expect_err("symlink");
    assert_eq!(
        std::fs::read_to_string(target.join("sessions/enc/grok-id/messages.jsonl")).unwrap(),
        "older"
    );
    assert!(!target.join(STAGING).exists());
}

/// Copies into one account at the same moment each keep to their own stage: a copy that starts
/// while another is part-way, clears nothing of the other's, and both records are in place
/// afterwards.
#[test]
fn copies_into_one_account_do_not_clear_each_others_staging() {
    let source = temp_dir("concurrent-source");
    let target = temp_dir("concurrent-target");
    write(&source, "projects/slug/a.jsonl", "a");
    write(&source, "sessions/enc/b/messages.jsonl", "b");
    write(&target, "sessions/enc/b/messages.jsonl", "older b");

    // Copy A is part-way: its stage exists. Copy B (a directory over an older directory)
    // runs its whole course meanwhile.
    let a_stage = target.join(STAGING).join("a.jsonl");
    std::fs::create_dir_all(&a_stage).unwrap();
    std::fs::write(a_stage.join("partial"), "a in progress").unwrap();

    copy(&source, &target, Path::new("sessions/enc/b"), &[]).expect("b copied");
    assert_eq!(
        std::fs::read_to_string(target.join("sessions/enc/b/messages.jsonl")).unwrap(),
        "b"
    );
    assert!(
        a_stage.join("partial").exists(),
        "A's stage was cleared by B"
    );
    assert!(!target.join(STAGING).join("b").exists());

    // A then completes its own copy over what is left of its stage.
    copy(&source, &target, Path::new("projects/slug/a.jsonl"), &[]).expect("a copied");
    assert_eq!(
        std::fs::read_to_string(target.join("projects/slug/a.jsonl")).unwrap(),
        "a"
    );
    assert!(!target.join(STAGING).exists());
}

/// A swap that fails puts the older record back, and a sweep at start removes whatever a
/// crash left.
#[test]
fn a_failed_swap_restores_the_older_record_and_a_sweep_clears_a_crashs_leftovers() {
    let source = temp_dir("aside-source");
    let target = temp_dir("aside-target");
    write(&source, "sessions/enc/b/messages.jsonl", "b");
    write(&target, "sessions/enc/b/messages.jsonl", "older b");
    let to = target.join("sessions/enc/b");
    let staging = target.join(STAGING).join("b");
    std::fs::create_dir_all(&staging).unwrap();
    // The older record is set aside and the rename into place cannot succeed, as `stage`
    // does not exist.
    let items = [(staging.join("b"), &*to, staging.join("previous"))];
    assert!(swap_into_place(&items).is_err());
    assert_eq!(
        std::fs::read_to_string(to.join("messages.jsonl")).unwrap(),
        "older b"
    );

    write(&target, ".octoboard-switch/b/leftover", "from a crash");
    sweep_staging([&*target]);
    assert!(!target.join(STAGING).exists());
}
/// Where a Codex home keeps its thread database is told wrongly in a way the user can fix:
/// the cause says which, so a client can word the remedy.
#[test]
fn a_thread_record_failure_names_a_cause_the_user_can_fix() {
    let head = Path::new("sessions/2026/10/03/rollout-2026-10-03T00-00-00-x_y.jsonl");
    let point = |target: &Path, env: &[(&str, &str)]| {
        let env = env
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        repoint_within(
            Agent::Codex,
            target,
            head,
            CODEX_ID,
            &env,
            std::time::Duration::from_millis(50),
        )
    };

    let invalid = temp_dir("cause-invalid");
    write(&invalid, "config.toml", "sqlite_home = [unterminated");
    let err = point(&invalid, &[]).expect_err("an invalid config.toml");
    assert_eq!(
        (
            param_of(&err, "stage").as_str(),
            param_of(&err, "cause").as_str()
        ),
        ("thread_record", "config_unreadable")
    );

    let unreadable = temp_dir("cause-config-dir");
    std::fs::create_dir(unreadable.join("config.toml")).unwrap();
    let err = point(&unreadable, &[]).expect_err("a config.toml that is a directory");
    assert_eq!(param_of(&err, "cause"), "config_unreadable");

    let relative = temp_dir("cause-relative");
    let err = point(&relative, &[("CODEX_SQLITE_HOME", "state")]).expect_err("relative");
    assert_eq!(param_of(&err, "cause"), "sqlite_home");

    let tilde = temp_dir("cause-tilde");
    write(&tilde, "config.toml", "sqlite_home = \"~/db\"");
    let err = point(&tilde, &[]).expect_err("a ~ with no HOME");
    assert_eq!(param_of(&err, "cause"), "sqlite_home");

    let broken = temp_dir("cause-broken");
    write(
        &broken,
        "state_5.sqlite",
        "not a database, but long enough to be read as one",
    );
    let err = point(&broken, &[]).expect_err("not a database");
    assert_eq!(param_of(&err, "cause"), "other");
}

/// A copy the file system refuses names why, when it is one a user can act on.
#[test]
fn a_copy_refused_for_permissions_says_so() {
    use std::os::unix::fs::PermissionsExt;
    let source = temp_dir("cause-perm-source");
    write(&source, "sessions/enc/grok-id/messages.jsonl", "x");
    let target = temp_dir("cause-perm-target");
    std::fs::set_permissions(&*target, std::fs::Permissions::from_mode(0o555)).unwrap();
    // A superuser writes anyway, and then there is nothing to refuse.
    if std::fs::create_dir(target.join("probe")).is_ok() {
        std::fs::set_permissions(&*target, std::fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }
    let err =
        copy(&source, &target, Path::new("sessions/enc/grok-id"), &[]).expect_err("read-only");
    std::fs::set_permissions(&*target, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        (
            param_of(&err, "stage").as_str(),
            param_of(&err, "cause").as_str()
        ),
        ("copy", "permission_denied")
    );
}
