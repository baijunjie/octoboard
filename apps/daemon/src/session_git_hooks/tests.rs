mod gate;
mod reread;

use std::process::Command;

use super::fixture::{
    dispatchers, dispatchers_for, dispatchers_with, installed, rev_parses, succeeds,
    system_exec_path, write_hook, Fixture,
};
use super::*;
use crate::test_support::{
    git, isolated_git_env, pinned_shell_env, CommandLine, ScratchDir, TEST_KEY,
};

/// The command lines of the probes, pinned whole: no leading option or index guard, at the
/// filesystem root, and with the system-wide configuration where the build reads it — the
/// user's own `GIT_CONFIG_NOSYSTEM` removed.
#[test]
fn the_probes_build_their_command_lines() {
    let git = UserGit::new("/opt/git", pinned_shell_env());
    let expected = |args: &[&str], extra: &[(&str, &str)]| {
        let mut line = CommandLine::pinned("/opt/git", args, extra, Some("/"));
        line.env.remove("GIT_CONFIG_NOSYSTEM");
        line
    };
    for args in [
        &["--exec-path"][..],
        &["--version"],
        &["--man-path"],
        &["--html-path"],
        &["config", "-z", "--list", "--show-origin", "--show-scope"],
    ] {
        assert_eq!(CommandLine::of(&probe(&git, args)), expected(args, &[]));
    }
    assert_eq!(
        CommandLine::of(&system_file_probe(&git)),
        expected(&["config", "--system", "--edit"], &[("GIT_EDITOR", "echo")])
    );
}

/// The user's own entries keep their slots and their effect; one a daemon started inside
/// another session inherited is dropped.
#[test]
fn the_report_takes_the_slot_after_the_users_own_entries() {
    let root = Path::new("/data/githooks");
    let env = |pairs: &[(&str, &str)]| -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    };
    assert_eq!(user_entries(&HashMap::new(), root), Some(Vec::new()));
    assert_eq!(
        user_entries(&env(&[("GIT_CONFIG_COUNT", "two")]), root),
        None,
        "git refuses it already"
    );
    let inherited = env(&[
        ("GIT_CONFIG_COUNT", "2"),
        ("GIT_CONFIG_KEY_0", "core.hooksPath"),
        ("GIT_CONFIG_VALUE_0", "/data/githooks/0123"),
        ("GIT_CONFIG_KEY_1", "user.name"),
        ("GIT_CONFIG_VALUE_1", "u"),
    ]);
    assert_eq!(
        user_entries(&inherited, root),
        Some(vec![("user.name".to_string(), "u".to_string())])
    );

    let vars = injected(1, Path::new("/hooks"), "s", 7, "k");
    assert!(vars.contains(&("GIT_CONFIG_COUNT".to_string(), "2".to_string())));
    assert!(vars.contains(&("GIT_CONFIG_KEY_1".to_string(), "core.hooksPath".to_string())));
    assert!(vars.contains(&("GIT_CONFIG_VALUE_1".to_string(), "/hooks".to_string())));
    assert!(vars.contains(&("OCTOBOARD_DAEMON_KEY".to_string(), "k".to_string())));
}

/// What a launch runs against the installed git: the version, the documented names and the
/// system-wide files, then the directory, written once and found again as it is.
#[test]
fn the_dispatchers_are_written_for_the_installed_git_and_found_again() {
    use crate::test_support::{isolated_git_env, ScratchDir, SYSTEM_GIT};
    let root = ScratchDir::new("githooks-prepare");
    let env = isolated_git_env();
    let dir = prepare(&root, SYSTEM_GIT, &env, "/opt/octo'boardd").unwrap();

    for name in BUILT_IN_HOOKS {
        let meta = fs::metadata(dir.join(name)).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o555, "{name}");
    }
    let script = fs::read_to_string(dir.join("pre-commit")).unwrap();
    assert!(script.contains("'/opt/octo'\\''boardd' git-ref"));
    for placeholder in [
        "@EXE@",
        "@EXEC_PATH@",
        "@STATE@",
        "@SYSTEM_SCANS@",
        "_VARIABLE@",
    ] {
        assert!(!script.contains(placeholder), "{placeholder}");
    }
    assert_eq!(
        prepare(&root, SYSTEM_GIT, &env, "/opt/octo'boardd").unwrap(),
        dir
    );
    assert!(root.join(STATE_DIR).is_dir());
    // Read-only, so the scratch directory can go.
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
}

/// A git newer than the built-in list documents hooks the list lacks, and a user's hook under
/// such a name would be skipped without a dispatcher of that name.
#[test]
fn the_hook_names_a_git_documents_are_read_from_its_manual_or_html_page() {
    let man = ".SH \"DESCRIPTION\"\n.SS \"not\\-a\\-hook\"\n.SH \"HOOKS\"\n.SS \"pre\\-commit\"\n\
               text\n.SS \"post\\-frobnicate\"\n.SH \"SEE ALSO\"\n.SS \"git\\-hook\"\n";
    assert_eq!(hook_names_in_man(man), ["pre-commit", "post-frobnicate"]);
    let html = "<h2 id=\"_hooks\">HOOKS</h2><h3 id=\"_pre_commit\">pre-commit</h3>\
                <p>x</p><h3 id=\"_post_frobnicate\">post-frobnicate</h3>";
    assert_eq!(hook_names_in_html(html), ["pre-commit", "post-frobnicate"]);
}

/// Apple's git reads a file inside Xcode besides the system and user ones, which a
/// `core.hooksPath` could sit in like any other.
#[test]
fn a_configuration_file_outside_the_users_and_repositorys_scopes_is_scanned() {
    let listing = b"unknown\0file:/Xcode/gitconfig\0credential.helper\nosxkeychain\0\
                    system\0file:/etc/gitconfig\0a.b\nc\0\
                    global\0file:/home/u/.gitconfig\0user.name\nu\0\
                    command\0command line:\0x.y\nz\0";
    assert_eq!(
        other_scope_files(listing),
        [
            PathBuf::from("/Xcode/gitconfig"),
            PathBuf::from("/etc/gitconfig")
        ]
    );
}

#[test]
fn the_version_is_read_from_a_vendors_version_line() {
    assert_eq!(
        parse_version("git version 2.50.1 (Apple Git-155)\n"),
        Some((2, 50))
    );
}

/// A user's `reference-transaction` hook can refuse a transaction while it is being prepared.
#[test]
fn a_users_reference_transaction_hook_still_aborts_and_nothing_is_reported() {
    let fixture = Fixture::new();
    let hook = fixture.common_dir.join("hooks/reference-transaction");
    let seen = fixture.dir.join("seen");
    write_hook(
        &hook,
        &format!(
            "[ \"$1\" = prepared ] && grep -q refused && exit 1\n\
             [ \"$1\" = committed ] && cat >>'{}'\nexit 0",
            seen.display()
        ),
    );

    let refused = fixture
        .session_git("a", &fixture.repo, &[])
        .args(["branch", "refused"])
        .output()
        .unwrap();
    assert!(!succeeds(refused), "the user's hook refused it");
    fixture.as_session("a", &["branch", "allowed"]);
    fixture.deliver(1);

    assert_eq!(fixture.responsible("refused"), None);
    assert_eq!(fixture.responsible("allowed").as_deref(), Some("a"));
    assert!(git(&fixture.repo, &["branch", "--list", "refused"]).is_empty());
    assert_eq!(
        fixture.reports(1).len(),
        1,
        "the refused branch was never reported"
    );
    let seen = std::fs::read_to_string(&seen).unwrap();
    assert!(seen.contains(" refs/heads/allowed\n"), "{seen}");
}

/// A sandbox may refuse every write outside the workspace: the dispatcher then reports no
/// creation, says nothing, and leaves the user's hook to run as it would.
#[test]
fn a_state_directory_that_cannot_be_written_costs_only_the_report() {
    let fixture = Fixture::new();
    let seen = fixture.dir.join("seen");
    write_hook(
        &fixture.common_dir.join("hooks/reference-transaction"),
        &format!("echo \"$1\" >>'{}'\ncat >/dev/null", seen.display()),
    );
    let hooks = dispatchers_with(
        &system_exec_path(),
        &[],
        Path::new("/nonexistent/octoboard-state"),
    );

    let output = fixture
        .session_git_with(&hooks, "a", &fixture.repo, &[])
        .args(["branch", "sandboxed"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    assert_eq!(
        std::fs::read_to_string(&seen).unwrap(),
        "prepared\ncommitted\n"
    );
}

/// A transaction of hundreds of references is spooled rather than read line by line, and the
/// user's hook still gets every update.
#[test]
fn a_long_transaction_is_reported_and_handed_to_the_users_hook_whole() {
    let fixture = Fixture::new();
    let seen = fixture.dir.join("seen");
    write_hook(
        &fixture.common_dir.join("hooks/reference-transaction"),
        &format!(
            "[ \"$1\" = committed ] && cat >>'{}'\nexit 0",
            seen.display()
        ),
    );
    let head = String::from_utf8(git(&fixture.repo, &["rev-parse", "HEAD"])).unwrap();
    let updates: String = (0..300)
        .map(|n| format!("create refs/heads/bulk-{n:03} {}\n", head.trim()))
        .collect();
    let mut child = fixture
        .session_git("a", &fixture.repo, &[])
        .args(["update-ref", "--stdin"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    std::io::Write::write_all(&mut child.stdin.take().unwrap(), updates.as_bytes()).unwrap();
    assert!(child.wait().unwrap().success());

    fixture.deliver(1);

    assert_eq!(fixture.rows().len(), 300);
    assert_eq!(std::fs::read_to_string(&seen).unwrap().lines().count(), 300);
}

/// Husky and its like keep their hooks under the worktree and point `core.hooksPath` at them by a
/// relative path; a user may also export an entry of their own.
#[test]
fn a_hooks_path_of_the_users_own_still_runs_its_hooks() {
    let fixture = Fixture::new();
    let marker = fixture.repo.join("ran");
    write_hook(
        &fixture.repo.join(".husky/_/pre-commit"),
        &format!("echo husky >>'{}'", marker.display()),
    );
    git(&fixture.repo, &["config", "core.hooksPath", ".husky/_"]);
    fixture.as_session("a", &["commit", "-q", "--allow-empty", "-m", "one"]);
    assert_eq!(std::fs::read_to_string(&marker).unwrap(), "husky\n");

    git(&fixture.repo, &["config", "--unset", "core.hooksPath"]);
    let exported = fixture.dir.join("exported");
    write_hook(
        &exported.join("pre-commit"),
        &format!("echo exported >>'{}'", marker.display()),
    );
    let exported = exported.to_string_lossy().into_owned();
    let user = [
        ("GIT_CONFIG_COUNT", "1"),
        ("GIT_CONFIG_KEY_0", "core.hooksPath"),
        ("GIT_CONFIG_VALUE_0", exported.as_str()),
    ];
    let output = fixture
        .session_git("a", &fixture.repo, &user)
        .args(["commit", "-q", "--allow-empty", "-m", "two"])
        .output()
        .unwrap();
    assert!(succeeds(output));
    assert_eq!(
        std::fs::read_to_string(&marker).unwrap(),
        "husky\nexported\n"
    );
}

/// Every hook git looks for starts a dispatcher, several per commit, so one with nothing of the
/// user's to run must not start a `git` of its own to find that out — and must still find a hook
/// the user does have.
#[test]
fn the_fast_path_starts_no_git_and_still_runs_a_hook_the_user_has() {
    let fixture = Fixture::new();
    let trace = fixture.dir.join("trace");
    let commit = |message: &str| {
        let output = fixture
            .session_git("a", &fixture.repo, &[])
            .env("GIT_TRACE2_EVENT", &trace)
            .args(["commit", "-q", "--allow-empty", "-m", message])
            .output()
            .unwrap();
        assert!(succeeds(output));
    };

    commit("none");
    assert_eq!(rev_parses(&trace), 0, "no hook of the user's");

    let marker = fixture.repo.join("ran");
    write_hook(
        &fixture.common_dir.join("hooks/pre-commit"),
        &format!("echo ran >>'{}'", marker.display()),
    );
    commit("own");
    assert_eq!(std::fs::read_to_string(&marker).unwrap(), "ran\n");
    assert_eq!(rev_parses(&trace), 0, "found without git");

    // The control: configuration that could move the hooks has git asked.
    git(&fixture.repo, &["config", "core.hooksPath", ".git/hooks"]);
    commit("asked");
    assert_eq!(std::fs::read_to_string(&marker).unwrap(), "ran\nran\n");
    assert!(rev_parses(&trace) > 0);
}

/// Each place a `core.hooksPath` could come from that the dispatcher cannot read for itself sends
/// it to git, the slow path, rather than to the repository's own hooks directory.
#[test]
fn anything_that_could_move_the_hooks_has_git_asked() {
    let system = installed(
        "octoboardd-test-system-gitconfig",
        "[core]\n\thooksPath = .git/hooks\n",
        0o644,
    );
    let other_build = dispatchers_for("/elsewhere/libexec/git-core", &[]);
    let with_system = dispatchers_for(&system_exec_path(), std::slice::from_ref(&system));
    type Setup = fn(&Fixture, &Path, &Path, &mut Command);
    let cases: [(&str, Option<&Path>, Setup); 9] = [
        ("an exported entry of the user's", None, |_, _, hooks, c| {
            // The user's entry first, the injected one after it, as a launch lays them out.
            c.env("GIT_CONFIG_COUNT", "2")
                .env("GIT_CONFIG_KEY_0", "core.hooksPath")
                .env("GIT_CONFIG_VALUE_0", ".git/hooks")
                .env("GIT_CONFIG_KEY_1", "core.hooksPath")
                .env("GIT_CONFIG_VALUE_1", hooks);
        }),
        ("git -c", None, |_, _, _, c| {
            c.args(["-c", "include.path=/dev/null"]);
        }),
        ("GIT_CONFIG_GLOBAL", None, |_, moved, _, c| {
            c.env("GIT_CONFIG_GLOBAL", moved);
        }),
        ("~/.gitconfig", None, |f, moved, _, c| {
            std::fs::copy(moved, f.dir.join(".gitconfig")).unwrap();
            c.env_remove("GIT_CONFIG_GLOBAL").env("HOME", &*f.dir);
        }),
        (
            "an include in the repository's configuration",
            None,
            |f, _, _, _| {
                git(&f.repo, &["config", "include.path", "/dev/null"]);
            },
        ),
        ("config.worktree", None, |f, moved, _, _| {
            std::fs::copy(moved, f.common_dir.join("config.worktree")).unwrap();
        }),
        ("GIT_CONFIG_SYSTEM", None, |_, moved, _, c| {
            c.env_remove("GIT_CONFIG_NOSYSTEM")
                .env("GIT_CONFIG_SYSTEM", moved);
        }),
        (
            "a system file this git build reads",
            Some(&with_system),
            |_, _, _, _| {},
        ),
        ("another git build", Some(&other_build), |_, _, _, _| {}),
    ];
    for (source, hooks, setup) in cases {
        let fixture = Fixture::new();
        let moved = fixture.dir.join("moved");
        std::fs::write(&moved, "[core]\n\thooksPath = .git/hooks\n").unwrap();
        let trace = fixture.dir.join("trace");
        let hooks = hooks.unwrap_or(dispatchers());
        let mut command = fixture.session_git_with(hooks, "a", &fixture.repo, &[]);
        setup(&fixture, &moved, hooks, &mut command);
        let output = command
            .env("GIT_TRACE2_EVENT", &trace)
            .args(["commit", "-q", "--allow-empty", "-m", "m"])
            .output()
            .unwrap();
        assert!(succeeds(output), "{source}");
        assert!(rev_parses(&trace) > 0, "{source}");
    }
}

/// A push into a non-bare repository's checked-out branch updates its worktree unless a
/// push-to-checkout hook does, and the dispatcher's presence must not count as one. git clears
/// `GIT_CONFIG_*` for the receiving end of a push it makes itself, so the dispatcher only meets
/// this behind a server a session starts (`git daemon`, `git http-backend`); it is run here the
/// way `receive-pack` runs it.
#[test]
fn a_push_to_checkout_with_no_hook_of_the_users_updates_the_worktree() {
    let fixture = Fixture::new();
    let theirs = ScratchDir::new("branch-origin-push-target");
    git(
        &fixture.repo,
        &[
            "clone",
            "-q",
            &fixture.repo.to_string_lossy(),
            &theirs.to_string_lossy(),
        ],
    );
    std::fs::write(fixture.repo.join("pushed"), "pushed").unwrap();
    git(&fixture.repo, &["add", "pushed"]);
    git(&fixture.repo, &["commit", "-q", "-m", "push"]);
    let pushed = String::from_utf8(git(&fixture.repo, &["rev-parse", "HEAD"])).unwrap();
    git(&theirs, &["fetch", "-q", "origin"]);

    let git_dir = theirs.join(".git");
    let output = Command::new(dispatchers().join("push-to-checkout"))
        .arg(pushed.trim())
        .current_dir(&git_dir)
        .env_clear()
        .envs(isolated_git_env())
        .envs(injected(0, dispatchers(), "a", 1, TEST_KEY))
        .env("GIT_DIR", &git_dir)
        .env("GIT_WORK_TREE", &*theirs)
        .output()
        .unwrap();
    assert!(succeeds(output));

    assert_eq!(
        std::fs::read_to_string(theirs.join("pushed")).unwrap(),
        "pushed"
    );
}
