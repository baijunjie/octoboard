//! A session's launch and end leave the user's home as it was outside Octoboard's own directory
//! (P9 in `docs/conventions/principles.md`): the dynamic half of the registered writes check
//! (`scripts/verify/README.md`, "Registered writes"), for what a search of the source cannot see.
//!
//! Octoboard's own paths follow the process's home (`crate::paths`), so the launch runs in this
//! test binary started again with a scratch home, as the platform's helper tests are
//! (`crate::platform::test_helper`). It covers the test provider on the host it runs on, and the
//! launch's and the end's own writes: not what a real agent's adapter writes, nor the login
//! shell's snapshot, which the launch is handed here, nor the daemon's own start (`main`), in whose
//! place the helper builds the application state a test runs over.

use std::collections::{BTreeMap, HashMap};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::launch::relaunch_session_with;
use super::lifecycle::archive_session;
use crate::platform::test_helper;
use crate::protocol::{AgentId, Session, SessionStatus};
use crate::session::term;
use crate::test_support::{app_state, checked_project, fake_agent, project_session, ScratchDir};

/// What [`test_helper::ROLE`] is set to for [`helper_launches_and_ends_a_session_in_this_home`].
const ROLE: &str = "launch-in-scratch-home";
/// The helper test's full path, as `--exact` takes it.
const HELPER: &str =
    "coordinator::own_directories_tests::helper_launches_and_ends_a_session_in_this_home";
/// Octoboard's own directory in a home, which the comparison leaves out.
const OWN_DIRECTORY: &str = ".octoboard";
const SESSION: &str = "s1";

/// One entry of a home, as far as a write would change it.
#[derive(Debug, PartialEq, Eq)]
enum Entry {
    Directory {
        mode: u32,
        modified: SystemTime,
    },
    File {
        mode: u32,
        modified: SystemTime,
        content: String,
    },
    Link {
        target: PathBuf,
    },
}

/// Everything under `home` but Octoboard's own directory, by its path below `home`. A link is
/// listed by its target and not followed.
fn listing(home: &Path) -> BTreeMap<PathBuf, Entry> {
    fn walk(home: &Path, dir: &Path, found: &mut BTreeMap<PathBuf, Entry>) {
        for entry in std::fs::read_dir(dir).expect("the directory is listed") {
            let path = entry.expect("an entry is read").path();
            let below = path.strip_prefix(home).expect("below home").to_path_buf();
            if below == Path::new(OWN_DIRECTORY) {
                continue;
            }
            let metadata = std::fs::symlink_metadata(&path).expect("the entry's metadata");
            let modified = || metadata.modified().expect("a modification time");
            let mode = metadata.permissions().mode();
            let listed = if metadata.is_symlink() {
                Entry::Link {
                    target: std::fs::read_link(&path).expect("the link's target"),
                }
            } else if metadata.is_dir() {
                walk(home, &path, found);
                Entry::Directory {
                    mode,
                    modified: modified(),
                }
            } else {
                Entry::File {
                    mode,
                    modified: modified(),
                    content: String::from_utf8_lossy(&std::fs::read(&path).expect("the file"))
                        .into_owned(),
                }
            };
            found.insert(below, listed);
        }
    }
    let mut found = BTreeMap::new();
    walk(home, home, &mut found);
    found
}

/// The environment the helper's launch runs in: the home it was started with, and the fake agent
/// in that home's `bin`.
fn home_env() -> HashMap<String, String> {
    let home = crate::paths::home_dir();
    HashMap::from([
        (
            "PATH".to_string(),
            format!("{}:/usr/bin:/bin", home.join("bin").display()),
        ),
        ("HOME".to_string(), home.to_string_lossy().into_owned()),
    ])
}

/// What a user's home holds that a launch could reach, written into `home`: shell and git
/// configuration, each agent's own directory, the test provider's default one with a record, a
/// link, and a project.
fn seed(home: &Path) {
    for (file, text) in [
        (".zshrc", "export EDITOR=vi\n"),
        (".gitconfig", "[user]\n\tname = Someone\n"),
        (".claude.json", "{}\n"),
        (".claude/settings.json", "{}\n"),
        (".codex/config.toml", "model = \"some-model\"\n"),
        (".grok/config.toml", "[ui]\ntheme = \"dark\"\n"),
        (".octoboard-test-agent/sessions/kept.jsonl", "{}\n"),
        ("Projects/app/README.md", "# App\n"),
    ] {
        let path = home.join(file);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the directory");
        std::fs::write(&path, text).expect("the file");
    }
    crate::test_support::symlink(".zshrc", home.join(".profile")).expect("the link");
}

/// A way a write changes a home, by what it is called in a failure.
type Change = (&'static str, fn(&Path));

/// The comparison sees each way a write changes a home: without this, an unchanged listing after
/// the launch would prove nothing.
#[test]
fn the_listing_differs_after_each_kind_of_change_outside_octoboards_own_directory() {
    let changes: [Change; 7] = [
        ("a file added", |home| {
            std::fs::write(home.join(".claude/added.json"), "{}").unwrap()
        }),
        ("a directory added", |home| {
            std::fs::create_dir(home.join(".config")).unwrap()
        }),
        ("a file removed", |home| {
            std::fs::remove_file(home.join(".gitconfig")).unwrap()
        }),
        ("a file's content", |home| {
            std::fs::write(home.join(".codex/config.toml"), "model = \"other-model\"\n").unwrap()
        }),
        // The same content written again: only the modification time tells.
        ("a file rewritten as it was", |home| {
            let file = std::fs::File::options()
                .write(true)
                .open(home.join(".zshrc"))
                .unwrap();
            file.set_modified(SystemTime::UNIX_EPOCH).unwrap();
        }),
        // One bit the other way, whatever the file was created with.
        ("a file's permissions", |home| {
            let file = home.join(".zshrc");
            let mode = std::fs::metadata(&file).unwrap().permissions().mode();
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode ^ 0o100)).unwrap();
        }),
        ("a link's target", |home| {
            std::fs::remove_file(home.join(".profile")).unwrap();
            crate::test_support::symlink(".gitconfig", home.join(".profile")).unwrap();
        }),
    ];
    for (what, change) in changes {
        let home = ScratchDir::new("listed-home");
        seed(&home);
        let before = listing(&home);
        change(&home);
        assert_ne!(listing(&home), before, "{what}");
    }
    // What is written in Octoboard's own directory is not listed.
    let home = ScratchDir::new("listed-home");
    seed(&home);
    let before = listing(&home);
    std::fs::create_dir_all(home.join(OWN_DIRECTORY).join("run")).unwrap();
    std::fs::write(home.join(OWN_DIRECTORY).join("run/key"), "k").unwrap();
    // Creating it changes the home directory itself, which the listing leaves out with it.
    assert_eq!(listing(&home), before);
}

#[test]
fn a_sessions_launch_and_end_change_nothing_in_the_home_outside_octoboards_own_directory() {
    let home = ScratchDir::new("untouched-home");
    seed(&home);
    std::fs::create_dir_all(home.join("bin")).expect("the fake agent's directory");
    fake_agent(
        &home.join("bin"),
        AgentId::TEST.descriptor().binary,
        "sleep 30",
    );
    let before = listing(&home);

    let output = std::process::Command::new(test_helper::program())
        .args(test_helper::args(HELPER))
        .env(test_helper::ROLE, ROLE)
        .env("HOME", &*home)
        .output()
        .expect("the helper runs");
    assert!(
        output.status.success(),
        "the helper failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    // Without it the helper launched nothing, and an unchanged home would prove nothing.
    assert!(
        home.join(OWN_DIRECTORY).join("launched").is_file(),
        "the helper did not launch a session in {}",
        home.display()
    );
    assert_eq!(listing(&home), before);
}

/// The helper of the test above: in a process whose home is that test's scratch home, starts the
/// application state, launches a project session of the test provider through the launch a
/// running daemon uses, and archives it. A no-op in an ordinary test run.
#[tokio::test(flavor = "multi_thread")]
async fn helper_launches_and_ends_a_session_in_this_home() {
    if std::env::var(test_helper::ROLE).as_deref() != Ok(ROLE) {
        return;
    }
    let home = crate::paths::home_dir();
    let (state, dir) = app_state("launch-in-scratch-home");
    let project = checked_project(&state, &dir, &home.join("Projects/app"));
    state
        .store
        .insert_session(&Session {
            project_id: Some(project.id),
            ..project_session(SESSION, AgentId::TEST, SessionStatus::Interrupted)
        })
        .unwrap();

    let claim = state.begin_launch(SESSION).unwrap();
    relaunch_session_with(
        &state,
        &claim,
        SESSION,
        None,
        false,
        Some(crate::env_shell::LaunchEnv::captured(home_env())),
        None,
        term::launch,
    )
    .await
    .unwrap();
    drop(claim);
    // The launch wrote the session's scratch directory under this home, not the developer's.
    let scratch = crate::paths::session_scratch(SESSION);
    assert!(scratch.starts_with(home.join(OWN_DIRECTORY)));
    assert!(scratch.is_dir(), "{} is missing", scratch.display());
    let live = state
        .live_session(SESSION)
        .expect("the launch registered the session");

    archive_session(&state, SESSION).unwrap();
    // The end is over when the exit watcher is: it records the exit and runs what follows one,
    // all of which has to have happened before the home is compared.
    live.await_exit_watch().await;
    assert!(state.live_session(SESSION).is_none());
    assert_eq!(
        state.session_record(SESSION).unwrap().status,
        SessionStatus::Archived
    );
    // Tells the test that started this helper that a session was launched and ended here.
    std::fs::write(home.join(OWN_DIRECTORY).join("launched"), "").unwrap();
}
