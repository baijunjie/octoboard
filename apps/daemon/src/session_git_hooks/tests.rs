mod gate;
mod reread;
mod route;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use super::fixture::{
    dispatchers, dispatchers_for, dispatchers_in, dispatchers_with, installed, rev_parses,
    succeeds, system_exec_path, test_key_file, write_hook, Fixture,
};
use super::*;
use crate::test_support::{git, isolated_git_env, pinned_shell_env, CommandLine, ScratchDir};

/// The command lines of the probes, pinned whole: no leading option beyond the base ones and no
/// index guard, at the filesystem root, and with the system-wide configuration where the build
/// reads it — the user's own `GIT_CONFIG_NOSYSTEM` removed.
#[test]
fn the_probes_build_their_command_lines() {
    let git = Runner::new(
        UserGit::new("/opt/git", pinned_shell_env()),
        Policy::PLAIN,
        PROBE_LIMITS,
    );
    let expected = |args: &[&str], extra: &[(&str, &str)]| {
        let mut all = crate::git::env::BASE_OPTIONS.to_vec();
        all.extend_from_slice(args);
        let mut line = CommandLine::pinned("/opt/git", &all, extra, Some("/"));
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

/// The user's own `GIT_CONFIG_PARAMETERS` entries stay, in order and as written, ahead of the
/// injected one; one a daemon started inside another session inherited is dropped, and a value git
/// would refuse gives the session no variables.
#[test]
fn the_hooks_path_entry_goes_after_the_users_own_parameters() {
    let root = Path::new("/data/githooks");
    let env = |parameters: &str| -> HashMap<String, String> {
        HashMap::from([("GIT_CONFIG_PARAMETERS".to_string(), parameters.to_string())])
    };
    assert_eq!(user_parameters(&HashMap::new(), root).as_deref(), Some(""));
    assert_eq!(user_parameters(&env(""), root).as_deref(), Some(""));
    // Both forms git reads, an implicit `true`, an escaped quote and runs of whitespace.
    let own = "'user.name'='U '\\''s' 'color.ui=auto'  'core.bare'= 'a.b'='x=y'";
    assert_eq!(
        user_parameters(&env(own), root).as_deref(),
        Some(own.replace("  ", " ").as_str())
    );
    let inherited = "'core.hooksPath'='/data/githooks/0123' 'user.name'='u' \
                     'core.hookspath=/data/githooks/4567' 'core.hookspath'='/elsewhere'";
    assert_eq!(
        user_parameters(&env(inherited), root).as_deref(),
        Some("'user.name'='u' 'core.hookspath'='/elsewhere'"),
        "only the entries under this daemon's directory are dropped"
    );
    for bogus in [
        " 'user.name'='u'",
        "user.name=u",
        "'user.name'='u",
        "'user.name'='u'x",
        "'user.name'=u",
        "'user.name'x",
        // A key git refuses: no section, an empty section or variable, a variable that starts
        // with a digit, a character a name cannot hold, and a newline in a subsection.
        "'name'='u'",
        "''=''",
        "'.name'='u'",
        "'user.'='u'",
        "'user.1x'='u'",
        "'us er.name'='u'",
        "'user.na_me'='u'",
        "'user.a\nb.name'='u'",
        "'user.name=u' 'name=u'",
    ] {
        assert_eq!(user_parameters(&env(bogus), root), None, "{bogus:?}");
    }
    // A subsection holds anything but a newline, and a variable may hold `-` and digits.
    assert!(parse_parameters("'remote.my origin.url'='u' 'a-b.c-1'='v' 'x.y.z.w'='v'").is_some());
    assert_eq!(
        parse_parameters("'a.b'='it'\\''s' 'c.d'= 'e.f=g'").unwrap(),
        [
            Parameter {
                text: "'a.b'='it'\\''s'",
                key: "a.b".to_string(),
                value: Some("it's".to_string()),
            },
            Parameter {
                text: "'c.d'=",
                key: "c.d".to_string(),
                value: None,
            },
            Parameter {
                text: "'e.f=g'",
                key: "e.f".to_string(),
                value: Some("g".to_string()),
            },
        ]
    );

    let vars = injected(
        "'user.name'='u'",
        Path::new("/hooks"),
        "s",
        7,
        Path::new("/run/s/key"),
    );
    assert_eq!(
        vars,
        [
            (
                "GIT_CONFIG_PARAMETERS".to_string(),
                "'user.name'='u' 'core.hookspath'='/hooks'".to_string()
            ),
            ("OCTOBOARD_SESSION_ID".to_string(), "s".to_string()),
            ("OCTOBOARD_DAEMON_PORT".to_string(), "7".to_string()),
            (
                "OCTOBOARD_DAEMON_CREDENTIAL_PATH".to_string(),
                "/run/s/key".to_string()
            ),
        ]
    );
    let alone = injected("", Path::new("/my hooks/it's"), "s", 7, Path::new("/k"));
    assert_eq!(
        alone[0].1, "'core.hookspath'='/my hooks/it'\\''s'",
        "no whitespace ahead of the first entry, which git refuses"
    );
}

/// No variable of a session's git hooks has a name an agent's filter of secrets strips — Codex's
/// shell environment policy drops, where its default excludes are turned on, every name holding
/// `KEY`, `SECRET` or `TOKEN`, whatever the case, from the commands it runs. Scoped to what this
/// module sets; the agents' adapters set variables of their own.
#[test]
fn no_variable_a_session_git_hooks_set_is_named_like_a_secret() {
    let dir = ScratchDir::new("githooks-names");
    let env = session_env_under(
        &dir.join("githooks"),
        &isolated_git_env(),
        &std::env::current_exe().unwrap().to_string_lossy(),
        "session-1",
        7,
        &dir.join("key"),
    );
    assert!(!env.is_empty(), "the installed git is given the variables");
    for (name, _) in &env {
        let name = name.to_ascii_lowercase();
        assert!(
            !["key", "secret", "token"]
                .iter()
                .any(|word| name.contains(word)),
            "{name}"
        );
    }
}

/// The session works exactly as it would without Octoboard, which is to say it is given no
/// variable at all, when the user's git is older than the hooks need, when the user's own
/// `GIT_CONFIG_PARAMETERS` is already unreadable to git, and when the directory the dispatchers go
/// in cannot be written. The same arguments with none of those hold give the variables.
#[test]
fn nothing_is_injected_where_the_session_would_work_better_without() {
    let dir = ScratchDir::new("githooks-skipped");
    let exe = std::env::current_exe()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let key_file = dir.join("key");
    let session = |root: &Path, env: &HashMap<String, String>| {
        session_env_under(root, env, &exe, "session-1", 7, &key_file)
    };
    let env = isolated_git_env();
    let control = session(&dir.join("githooks"), &env);
    assert!(
        !control.is_empty(),
        "the installed git is given the variables"
    );

    let mut bogus = env.clone();
    bogus.insert(
        "GIT_CONFIG_PARAMETERS".to_string(),
        "'user.name'='unterminated".to_string(),
    );
    assert!(session(&dir.join("githooks-bogus"), &bogus).is_empty());

    // A file stands where the directory's parent would be.
    let blocker = dir.join("blocker");
    std::fs::write(&blocker, "a file").unwrap();
    assert!(session(&blocker.join("githooks"), &env).is_empty());

    #[cfg(unix)]
    {
        let old_git = dir.join("old-git");
        std::fs::create_dir(&old_git).unwrap();
        let script = old_git.join("git");
        std::fs::write(&script, "#!/bin/sh\necho 'git version 2.30.2'\n").unwrap();
        crate::test_support::make_executable(&script);
        let mut old = env.clone();
        old.insert("PATH".to_string(), old_git.to_string_lossy().into_owned());
        assert!(session(&dir.join("githooks-old"), &old).is_empty());
    }
}

/// What a launch adds to the user's own `GIT_CONFIG_*` variables: the hooks path goes into
/// `GIT_CONFIG_PARAMETERS` after the user's entries there, and the user's `GIT_CONFIG_COUNT` and
/// its keys and values are neither set nor named, so that an agent stripping `GIT_CONFIG_KEY_<n>`
/// from its commands has nothing of Octoboard's to leave a count behind for.
#[test]
fn a_launch_adds_to_git_config_parameters_and_leaves_the_count_to_the_user() {
    let dir = ScratchDir::new("githooks-user-config");
    let mut env = isolated_git_env();
    env.insert("GIT_CONFIG_COUNT".to_string(), "1".to_string());
    env.insert("GIT_CONFIG_KEY_0".to_string(), "user.name".to_string());
    env.insert("GIT_CONFIG_VALUE_0".to_string(), "from-count".to_string());
    env.insert(
        "GIT_CONFIG_PARAMETERS".to_string(),
        "'user.email'='u@example.com'".to_string(),
    );
    let vars = session_env_under(
        &dir.join("githooks"),
        &env,
        &std::env::current_exe().unwrap().to_string_lossy(),
        "session-1",
        7,
        &dir.join("key"),
    );
    assert!(!vars.is_empty());
    let named = |name: &str| vars.iter().filter(|(n, _)| n == name).count();
    for name in ["GIT_CONFIG_COUNT", "GIT_CONFIG_KEY_0", "GIT_CONFIG_VALUE_0"] {
        assert_eq!(named(name), 0, "{name}");
    }
    assert_eq!(named("GIT_CONFIG_PARAMETERS"), 1);
    let parameters = &vars
        .iter()
        .find(|(n, _)| n == "GIT_CONFIG_PARAMETERS")
        .unwrap()
        .1;
    assert!(
        parameters.starts_with("'user.email'='u@example.com' 'core.hookspath'="),
        "{parameters}"
    );
}

/// What git makes of the injected entry, by real runs: it sets `core.hooksPath` to the directory,
/// spaces and quotes in its path and all; it wins over a `core.hooksPath` the user exported through
/// `GIT_CONFIG_COUNT`, which keeps its other entries; the user's own `GIT_CONFIG_PARAMETERS` keep
/// their effect; and the user's `git -c core.hooksPath=…` wins over it.
#[test]
fn git_reads_the_injected_hooks_path_beside_the_users_own_configuration() {
    let dir = ScratchDir::new("githooks-parameters");
    let hooks = dir.join("my hooks").join("it's");
    let config = |extra: &[(&str, &str)], args: &[&str]| -> String {
        let user = extra
            .iter()
            .find(|(name, _)| *name == "GIT_CONFIG_PARAMETERS")
            .map_or("", |(_, value)| value);
        let output = Command::new(crate::test_support::SYSTEM_GIT)
            .current_dir(&*dir)
            .env_clear()
            .envs(isolated_git_env())
            .envs(extra.iter().copied())
            .envs(injected(user, &hooks, "s", 7, Path::new("/k")))
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    };
    let ours = shell::posix_path(&hooks);
    assert_eq!(config(&[], &["config", "core.hooksPath"]), ours);
    let count = [
        ("GIT_CONFIG_COUNT", "2"),
        ("GIT_CONFIG_KEY_0", "core.hooksPath"),
        ("GIT_CONFIG_VALUE_0", "/user/hooks"),
        ("GIT_CONFIG_KEY_1", "user.name"),
        ("GIT_CONFIG_VALUE_1", "from-count"),
    ];
    assert_eq!(config(&count, &["config", "core.hooksPath"]), ours);
    assert_eq!(config(&count, &["config", "user.name"]), "from-count");
    let parameters = [(
        "GIT_CONFIG_PARAMETERS",
        "'user.name'='from parameters' 'core.hooksPath'='/user/hooks'",
    )];
    assert_eq!(config(&parameters, &["config", "core.hooksPath"]), ours);
    assert_eq!(
        config(&parameters, &["config", "user.name"]),
        "from parameters"
    );
    assert_eq!(
        config(
            &[],
            &["-c", "core.hooksPath=/mine", "config", "core.hooksPath"]
        ),
        "/mine"
    );
    // A `git -c` nested inside a command that has one already.
    assert_eq!(
        config(
            &[],
            &[
                "-c",
                "user.name=outer",
                "-c",
                "user.name=inner",
                "config",
                "user.name"
            ]
        ),
        "inner"
    );
}

/// The key file variable is read by a native program, so a Windows path stays the native path it
/// was given and is not turned into the POSIX form the hooks path takes.
#[cfg(windows)]
#[test]
fn the_key_file_variable_is_a_native_path_on_windows() {
    let key_file = Path::new(r"C:\Users\u\run\s\key");
    let vars = injected("", Path::new(r"C:\hooks"), "s", 7, key_file);
    assert!(vars.contains(&(
        "OCTOBOARD_DAEMON_CREDENTIAL_PATH".to_string(),
        r"C:\Users\u\run\s\key".to_string()
    )));
}

/// What a project session's environment carries of the launch key is the path of its key file: no
/// variable's value is the key, and the file the path names holds it, for its owner alone.
#[test]
fn the_session_environment_names_the_key_file_and_holds_no_key() {
    let dir = ScratchDir::new("githooks-key-file");
    let key = "0123456789abcdef-launch-key";
    let key_file = crate::key_file::write(&dir, key).unwrap();
    let env = session_env_under(
        &dir.join("githooks"),
        &isolated_git_env(),
        &std::env::current_exe().unwrap().to_string_lossy(),
        "session-1",
        7,
        &key_file,
    );
    assert!(!env.is_empty(), "the installed git is given the variables");
    assert!(
        env.iter()
            .all(|(name, value)| !name.contains(key) && !value.contains(key)),
        "{env:?}"
    );
    assert!(env.contains(&(
        "OCTOBOARD_DAEMON_CREDENTIAL_PATH".to_string(),
        key_file.to_string_lossy().into_owned()
    )));
    assert_eq!(std::fs::read_to_string(&key_file).unwrap(), key);
    #[cfg(unix)]
    {
        let mode = fs::metadata(&key_file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

/// What a launch runs against the installed git: the version, the documented names and the
/// system-wide files, then the directory, written once and found again as it is.
#[test]
fn the_dispatchers_are_written_for_the_installed_git_and_found_again() {
    use crate::test_support::{isolated_git_env, ScratchDir, SYSTEM_GIT};
    let root = ScratchDir::new("githooks-prepare");
    let env = isolated_git_env();
    let dir = prepare(&root, SYSTEM_GIT, &env, "/opt/octo'boardd").unwrap();

    #[cfg(unix)]
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
        "@HOOKS_PATH_KEY@",
        "_VARIABLE@",
    ] {
        assert!(!script.contains(placeholder), "{placeholder}");
    }
    // Back-dated, so a directory written again would show a time of now.
    let written = SystemTime::now() - Duration::from_secs(3600);
    crate::platform::fs::set_dir_modified(&dir, written).unwrap();
    let identity = crate::platform::fs::identity(&dir, crate::platform::fs::Links::Keep).unwrap();
    assert_eq!(
        prepare(&root, SYSTEM_GIT, &env, "/opt/octo'boardd").unwrap(),
        dir
    );
    assert_eq!(
        crate::platform::fs::identity(&dir, crate::platform::fs::Links::Keep).unwrap(),
        identity
    );
    assert_eq!(fs::metadata(&dir).unwrap().modified().unwrap(), written);
    assert!(root.join(STATE_DIR).is_dir());
    // Read-only, so the scratch directory can go.
    #[cfg(unix)]
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
    // A path is absolute with a drive letter on Windows.
    let root = if cfg!(windows) { "C:" } else { "" };
    let listing = format!(
        "unknown\0file:{root}/Xcode/gitconfig\0credential.helper\nosxkeychain\0\
         system\0file:{root}/etc/gitconfig\0a.b\nc\0\
         global\0file:{root}/home/u/.gitconfig\0user.name\nu\0\
         command\0command line:\0x.y\nz\0"
    );
    assert_eq!(
        other_scope_files(listing.as_bytes()),
        [
            PathBuf::from(format!("{root}/Xcode/gitconfig")),
            PathBuf::from(format!("{root}/etc/gitconfig"))
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
    // A git that has a `preparing` state of its own runs the hook for it too.
    let seen = std::fs::read_to_string(&seen).unwrap();
    let states: Vec<&str> = seen.lines().filter(|state| *state != "preparing").collect();
    assert_eq!(states, ["prepared", "committed"]);
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

/// The user's hook, and every git it runs, sees the user's configuration alone: the dispatcher
/// takes its own entry out of `GIT_CONFIG_PARAMETERS` wherever `git -c` put entries around it, its
/// directory's path quoted as git quotes it, and an entry for another directory beside its own out
/// of `GIT_CONFIG_COUNT`, renumbering the rest; with nothing left that could move the hooks, it
/// still finds the user's hook without asking git.
#[test]
fn the_users_hook_sees_the_users_configuration_alone() {
    let fixture = Fixture::new();
    let root = fixture.dir.join("hooks root 'q'");
    let hooks = dispatchers_in(&root);
    let seen = fixture.dir.join("seen");
    write_hook(
        &fixture.common_dir.join("hooks/pre-commit"),
        &format!(
            "printf '%s\\n' \"${{GIT_CONFIG_PARAMETERS-}}\" \"${{GIT_CONFIG_COUNT-}}\" \
             \"${{GIT_CONFIG_KEY_0-}}\" \"${{GIT_CONFIG_VALUE_0-}}\" \"${{GIT_CONFIG_KEY_1-}}\" \
             >'{}'",
            seen.display()
        ),
    );
    let older = shell::posix_path(&root.join("0123456789abcdef"));
    let user = [
        ("GIT_CONFIG_PARAMETERS", "'user.x'='1'"),
        ("GIT_CONFIG_COUNT", "2"),
        ("GIT_CONFIG_KEY_0", "core.hooksPath"),
        ("GIT_CONFIG_VALUE_0", older.as_str()),
        ("GIT_CONFIG_KEY_1", "user.y"),
        ("GIT_CONFIG_VALUE_1", "z"),
    ];
    let trace = fixture.dir.join("trace");
    let output = fixture
        .session_git_with(&hooks, "a", &fixture.repo, &user)
        .env("GIT_TRACE2_EVENT", &trace)
        .args(["-c", "user.w=2", "commit", "-q", "--allow-empty", "-m", "m"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let seen = std::fs::read_to_string(&seen).expect("the user's hook ran");
    let lines: Vec<&str> = seen.lines().collect();
    let parameters = lines[0];
    assert!(
        parameters.starts_with("'user.x'='1' "),
        "the user's own entry first, with no whitespace ahead of it: {parameters}"
    );
    assert!(parameters.contains("'user.w'='2'"), "{parameters}");
    assert!(
        !parameters.to_ascii_lowercase().contains("hookspath"),
        "{parameters}"
    );
    assert_eq!(lines[1..], ["1", "user.y", "z", ""]);
    assert_eq!(rev_parses(&trace), 0, "found without git");
}

/// Codex, with its shell environment policy's default excludes applied, strips every variable
/// whose name holds `KEY`, `SECRET` or `TOKEN` from the commands its model runs. With all of them
/// gone from a session's environment, git still works and the dispatchers still report.
#[test]
fn git_works_and_reports_with_every_variable_named_like_a_secret_stripped() {
    let fixture = Fixture::new();
    let mut command = fixture.session_git("a", &fixture.repo, &[]);
    command
        .env("GITHUB_TOKEN", "t")
        .env("AWS_SECRET_ACCESS_KEY", "s")
        .env("openai_api_key", "k");
    let names: Vec<String> = command
        .get_envs()
        .filter(|(_, value)| value.is_some())
        .map(|(name, _)| name.to_string_lossy().into_owned())
        .collect();
    let mut stripped = Vec::new();
    for name in names {
        let lower = name.to_ascii_lowercase();
        if ["key", "secret", "token"]
            .iter()
            .any(|word| lower.contains(word))
        {
            command.env_remove(&name);
            stripped.push(name);
        }
    }
    for name in ["GITHUB_TOKEN", "AWS_SECRET_ACCESS_KEY", "openai_api_key"] {
        assert!(
            stripped.iter().any(|stripped| stripped == name),
            "{stripped:?}"
        );
    }
    let output = command.args(["branch", "stripped"]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fixture.deliver(1);
    assert_eq!(fixture.responsible("stripped").as_deref(), Some("a"));
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
        false,
    );
    let other_build = dispatchers_for("/elsewhere/libexec/git-core", &[]);
    let with_system = dispatchers_for(&system_exec_path(), std::slice::from_ref(&system));
    type Setup = fn(&Fixture, &Path, &Path, &mut Command);
    let cases: [(&str, Option<&Path>, Setup); 9] = [
        ("an exported entry of the user's", None, |_, _, _, c| {
            c.env("GIT_CONFIG_COUNT", "1")
                .env("GIT_CONFIG_KEY_0", "core.hooksPath")
                .env("GIT_CONFIG_VALUE_0", ".git/hooks");
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
    let runner = shell::command_runner(shell::Dialect::Posix);
    let output = Command::new(&runner.program)
        .arg(shell::posix_path(&dispatchers().join("push-to-checkout")))
        .arg(pushed.trim())
        .current_dir(&git_dir)
        .env_clear()
        .envs(isolated_git_env())
        .envs(injected("", dispatchers(), "a", 1, &test_key_file()))
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

/// `git --exec-path` prints a drive-letter path under git for Windows, and the dispatcher compares
/// what is kept with the `GIT_EXEC_PATH` it is given, so the text stays as printed.
#[test]
fn an_exec_path_is_a_posix_or_a_drive_letter_absolute_path_kept_as_printed() {
    for path in [
        "/usr/libexec/git-core",
        "C:/Program Files/Git/mingw64/libexec/git-core",
        "z:/git",
    ] {
        assert_eq!(parse_exec_path(&format!("{path}\n")).unwrap(), path);
    }
    for path in [
        "",
        "git-core",
        "./git-core",
        "C:",
        "C:git",
        r"C:\git",
        "1:/git",
    ] {
        assert!(parse_exec_path(path).is_err(), "{path:?}");
    }
}

/// A drive-letter path from `git rev-parse --git-path` is absolute: it is run as it is, not as
/// `./C:/…`, which names another file.
#[cfg(unix)]
#[test]
fn a_drive_letter_hooks_path_is_run_as_it_is() {
    let fixture = Fixture::new();
    let hooks = fixture.repo.join("C:/hooks");
    let seen = fixture.dir.join("seen");
    write_hook(
        &hooks.join("pre-commit"),
        &format!("echo \"$0\" >>'{}'", seen.display()),
    );
    git(&fixture.repo, &["config", "core.hooksPath", "C:/hooks"]);

    fixture.as_session("a", &["commit", "-q", "--allow-empty", "-m", "one"]);

    assert_eq!(
        std::fs::read_to_string(&seen).unwrap(),
        "C:/hooks/pre-commit\n"
    );
}

/// Runs the dispatcher of the hook `hook` from this very process, as the session `a`'s git would,
/// from `fixture`'s repository: with `args` and `input` on its stdin, and `hooks` its directory.
fn run_dispatcher(fixture: &Fixture, hooks: &Path, hook: &str, args: &[&str], input: &str) {
    run_dispatcher_in(fixture, &fixture.repo, hooks, hook, args, input);
}

/// [`run_dispatcher`] from the repository `dir` rather than `fixture`'s, its reports still going to
/// `fixture`.
fn run_dispatcher_in(
    fixture: &Fixture,
    dir: &Path,
    hooks: &Path,
    hook: &str,
    args: &[&str],
    input: &str,
) {
    let runner = shell::command_runner(shell::Dialect::Posix);
    let mut command = Command::new(&runner.program);
    command
        .arg(shell::posix_path(&hooks.join(hook)))
        .args(args)
        .current_dir(dir)
        .env_clear()
        .envs(isolated_git_env())
        .envs(injected("", hooks, "a", 1, &test_key_file()))
        .env("OCTOBOARD_TEST_REPORTS", &*fixture.reports)
        .env("OCTOBOARD_TEST_GATES", &*fixture.gates)
        .stdin(std::process::Stdio::piped());
    let mut child = command.spawn().unwrap();
    std::io::Write::write_all(&mut child.stdin.take().unwrap(), input.as_bytes()).unwrap();
    assert!(child.wait().unwrap().success());
}

/// One state of a transaction creating `branch` at `oid` in `fixture`'s repository.
fn run_transaction_state(fixture: &Fixture, state: &str, branch: &str, oid: &str) {
    run_transaction_state_in(fixture, &fixture.repo, state, branch, oid);
}

/// [`run_transaction_state`] in the repository `dir`.
fn run_transaction_state_in(fixture: &Fixture, dir: &Path, state: &str, branch: &str, oid: &str) {
    let line = format!("{} {oid} refs/heads/{branch}\n", "0".repeat(oid.len()));
    run_dispatcher_in(
        fixture,
        dir,
        dispatchers(),
        "reference-transaction",
        &[state],
        &line,
    );
}

/// Two transactions in one repository that run at once, from one parent process (every hook of a
/// git for Windows has the same one), each report the branch they created: what a prepared call
/// leaves for its committed call is not shared between them.
#[test]
fn transactions_in_one_repository_at_once_do_not_share_state() {
    let fixture = Fixture::new();
    let head = String::from_utf8(git(&fixture.repo, &["rev-parse", "HEAD"])).unwrap();
    let head = head.trim();

    run_transaction_state(&fixture, "prepared", "one", head);
    run_transaction_state(&fixture, "prepared", "two", head);
    run_transaction_state(&fixture, "committed", "one", head);
    run_transaction_state(&fixture, "committed", "two", head);

    let reports = fixture.reports(2);
    let mut created: Vec<_> = reports
        .iter()
        .flat_map(|(_, report)| report.refs.iter().map(|created| created.name.clone()))
        .collect();
    created.sort();
    assert_eq!(created, ["refs/heads/one", "refs/heads/two"]);
}

/// Transactions of two repositories at once that create the same branch at the same commit each
/// report it, however long the start their paths share: repositories made alike, as every test's
/// and many a user's worktrees are, differ in their paths' last characters alone.
#[test]
fn transactions_in_repositories_of_alike_paths_do_not_share_state() {
    let fixture = Fixture::new();
    let head = String::from_utf8(git(&fixture.repo, &["rev-parse", "HEAD"])).unwrap();
    let head = head.trim();
    let shared = fixture.dir.join(format!("worktrees-{}", "w".repeat(70)));
    let repos = [shared.join("one"), shared.join("two")];
    for repo in &repos {
        std::fs::create_dir_all(repo).unwrap();
        git(repo, &["init", "-q"]);
    }

    for state in ["prepared", "committed"] {
        for repo in &repos {
            run_transaction_state_in(&fixture, repo, state, "mine", head);
        }
    }

    let reports = fixture.reports(2);
    let mut created: Vec<_> = reports
        .iter()
        .flat_map(|(_, report)| {
            let repo = Path::new(&report.cwd).file_name().unwrap().to_owned();
            report
                .refs
                .iter()
                .map(move |created| (repo.clone(), created.name.clone()))
        })
        .collect();
    created.sort();
    assert_eq!(
        created,
        [
            ("one".into(), "refs/heads/mine".to_string()),
            ("two".into(), "refs/heads/mine".to_string()),
        ]
    );
}

/// A report the stand-in has not finished writing is not read: its line has no newline yet.
#[test]
fn a_report_still_being_written_is_not_read() {
    let fixture = Fixture::new();
    std::fs::write(&*fixture.reports, "whole\tline\n/repo\t\t--session a --key").unwrap();
    assert_eq!(fixture.report_lines(), ["whole\tline"]);
}

/// A long list of updates or pushed refs is spooled to a file of the run's own, not of its
/// parent's, which a git for Windows gives every hook alike.
#[test]
fn runs_from_one_parent_spool_to_files_of_their_own() {
    let fixture = Fixture::new();
    let state = fixture.dir.join("state");
    std::fs::create_dir(&state).unwrap();
    let hooks = dispatchers_with(&system_exec_path(), &[], &state);
    let head = String::from_utf8(git(&fixture.repo, &["rev-parse", "HEAD"])).unwrap();
    let head = head.trim();
    let updates: String = (0..300)
        .map(|n| format!("{} {head} refs/heads/bulk-{n:03}\n", "0".repeat(head.len())))
        .collect();
    let pushes: String = (0..300)
        .map(|n| {
            format!(
                "refs/heads/bulk-{n:03} {head} refs/heads/bulk-{n:03} {}\n",
                "0".repeat(head.len())
            )
        })
        .collect();

    for _ in 0..2 {
        run_dispatcher(
            &fixture,
            &hooks,
            "reference-transaction",
            &["committed"],
            &updates,
        );
        run_dispatcher(&fixture, &hooks, "pre-push", &["origin", "url"], &pushes);
    }

    let mut spools: Vec<String> = std::fs::read_dir(&state)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".in") || name.ends_with(".push"))
        .collect();
    spools.sort();
    assert_eq!(spools.len(), 4, "{spools:?}");
}
