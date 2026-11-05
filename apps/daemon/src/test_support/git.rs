//! The environment a test's `git` runs in, and the repositories and branches built with it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use super::ScratchDir;
use crate::state::AppState;

/// The environment a test's `git` runs in: this process's own, without any `GIT_*` variable, plus
/// no global or system configuration. A variable of the developer's shell (`GIT_DIR`, say) cannot
/// redirect a command, and their own configuration, hooks or signing settings cannot decide what a
/// test observes.
pub(crate) fn isolated_git_env() -> HashMap<String, String> {
    let mut env: HashMap<String, String> = std::env::vars().collect();
    env.retain(|key, _| !key.starts_with("GIT_"));
    env.insert("GIT_CONFIG_GLOBAL".to_string(), "/dev/null".to_string());
    env.insert("GIT_CONFIG_NOSYSTEM".to_string(), "1".to_string());
    env
}

/// The `git` a test runs: the system one, already exec'd countless times, so no
/// fresh-executable cost. On Windows, Git for Windows where its installer puts it, as on the CI
/// runners.
#[cfg(unix)]
pub(crate) const SYSTEM_GIT: &str = "/usr/bin/git";
#[cfg(windows)]
pub(crate) const SYSTEM_GIT: &str = r"C:\Program Files\Git\cmd\git.exe";

/// The `PATH` of [`pinned_shell_env`]: one that holds a git on this host.
fn pinned_path() -> String {
    if cfg!(windows) {
        std::env::var("PATH").unwrap_or_default()
    } else {
        "/usr/bin:/bin".to_owned()
    }
}

/// A user's shell environment for pinning the command a `git` runner builds: a variable that is
/// kept, the configuration location the user's own material is named by, a `GIT_*` that would
/// redirect the command, and the prompt settings the daemon overrides. The values are never run.
pub(crate) fn pinned_shell_env() -> HashMap<String, String> {
    let path = pinned_path();
    [
        ("HOME", "/home/u"),
        ("PATH", path.as_str()),
        ("GIT_CONFIG_GLOBAL", "/home/u/.gitconfig"),
        ("GIT_CONFIG_NOSYSTEM", "1"),
        ("GIT_DIR", "/elsewhere/.git"),
        ("GIT_LITERAL_PATHSPECS", "0"),
        ("GIT_TERMINAL_PROMPT", "1"),
        ("GIT_ASKPASS", "/askpass"),
        ("SSH_ASKPASS", "/askpass"),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_string(), value.to_string()))
    .collect()
}

/// What a built `Command` would run: the program, every argument in order, each variable it sets
/// (`None` for one it marks as removed) and the directory it runs in.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct CommandLine {
    pub(crate) program: String,
    pub(crate) args: Vec<String>,
    pub(crate) env: std::collections::BTreeMap<String, Option<String>>,
    pub(crate) dir: Option<PathBuf>,
}

impl CommandLine {
    /// What `command` would run.
    pub(crate) fn of(command: &Command) -> Self {
        let text = |value: &std::ffi::OsStr| value.to_string_lossy().into_owned();
        Self {
            program: text(command.get_program()),
            args: command.get_args().map(text).collect(),
            env: command
                .get_envs()
                .map(|(key, value)| (text(key), value.map(text)))
                .collect(),
            dir: command.get_current_dir().map(Path::to_path_buf),
        }
    }

    /// The command line a runner over [`pinned_shell_env`] is expected to build: `program` with
    /// `args` in `dir`, in the environment every daemon `git` gets from that shell environment
    /// (its `GIT_*` variables cut down to the user's configuration, unable to prompt) plus `extra`.
    pub(crate) fn pinned(
        program: &str,
        args: &[&str],
        extra: &[(&str, &str)],
        dir: Option<&str>,
    ) -> Self {
        let path = pinned_path();
        let env: std::collections::BTreeMap<String, Option<String>> = [
            ("HOME", "/home/u"),
            ("PATH", path.as_str()),
            ("GIT_CONFIG_GLOBAL", "/home/u/.gitconfig"),
            ("GIT_CONFIG_NOSYSTEM", "1"),
            ("GIT_TERMINAL_PROMPT", "0"),
            ("GIT_SSH_COMMAND", "ssh -oBatchMode=yes"),
        ]
        .into_iter()
        .chain(extra.iter().copied())
        .map(|(key, value)| (key.to_string(), Some(value.to_string())))
        .collect();
        Self {
            program: program.to_string(),
            args: args.iter().map(|arg| arg.to_string()).collect(),
            env,
            dir: dir.map(PathBuf::from),
        }
    }
}

/// A `git` command in `dir` for fixture setup, over exactly [`isolated_git_env`] and nothing
/// more. The caller adds the subcommand, and runs it where it needs its output, its exit status
/// or a pipe on its stdin.
///
/// It arrives with `-c` options already queued, so a caller adding one of its own adds it before
/// the subcommand: a `-c` after the subcommand is an argument to that subcommand instead.
///
/// The identity and the default branch name are passed per command because that environment
/// leaves no configuration to take them from: without them a commit fails for want of an author
/// and the first branch is whatever `git`'s own built-in default is. Automatic maintenance is off:
/// a commit past `gc.auto` loose objects starts it detached, and it outlives the test, writing
/// into the repository after its scratch directory has been removed.
pub(crate) fn fixture_git(dir: &Path) -> Command {
    let mut command = Command::new(SYSTEM_GIT);
    command
        .current_dir(dir)
        .env_clear()
        .envs(isolated_git_env())
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(["-c", "init.defaultBranch=main"])
        .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"]);
    command
}

/// Runs plain `git` in `dir` through [`fixture_git`], failing the test on a non-zero exit.
/// Returns what it printed.
pub(crate) fn git(dir: &Path, args: &[&str]) -> Vec<u8> {
    let args: Vec<&[u8]> = args.iter().map(|arg| arg.as_bytes()).collect();
    git_bytes(dir, &args)
}

/// Whether the `git` on this machine knows `show-ref --exists` (Git 2.43), which is what tells a
/// branch that is broken from one not born yet; before it a broken branch is taken for an unborn
/// one. `dir` must be inside a repository, since outside one `git` exits 128 and this panics.
pub(crate) fn git_has_show_ref_exists(dir: &Path) -> bool {
    let status = fixture_git(dir)
        .args(["show-ref", "--exists", "refs/heads/none"])
        .output()
        .expect("git runs")
        .status;
    // Exit 2 is "no such reference"; an older Git rejects the option itself with 129.
    match status.code() {
        Some(0 | 2) => true,
        Some(129) => false,
        other => panic!("git show-ref --exists exited {other:?}"),
    }
}

/// [`git`] with arguments that need not be UTF-8.
pub(crate) fn git_bytes(dir: &Path, args: &[&[u8]]) -> Vec<u8> {
    let args: Vec<std::ffi::OsString> = args
        .iter()
        .map(|arg| {
            crate::platform::names::name_from_bytes(arg).expect("an argument this host spells")
        })
        .collect();
    let output = fixture_git(dir).args(&args).output().expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

/// [`git`] with `input` on its stdin, for a subcommand that reads one. Its own output is left to
/// the test's, so a failure reports the arguments alone rather than `git`'s complaint.
pub(crate) fn git_stdin(dir: &Path, args: &[&str], input: &[u8]) {
    git_stdin_env(dir, args, &[], input);
}

/// [`git_stdin`] with `env` set for that one `git`, for a fixture that needs a variable the
/// isolated environment leaves out: a committer date, which is the time a reflog entry `git`
/// writes carries.
pub(crate) fn git_stdin_env(dir: &Path, args: &[&str], env: &[(&str, &str)], input: &[u8]) {
    use std::io::Write as _;
    let mut child = fixture_git(dir)
        .args(args)
        .envs(env.iter().copied())
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    assert!(child.wait().unwrap().success(), "git {args:?}");
}

/// `/System/Volumes/Data` followed by `real`, where that is the same directory as `real` under
/// another real path: on macOS the data volume's firmlinks give everything under `/private` and
/// `/Users` one, as a bind mount does on Linux.
pub(crate) fn second_real_path(real: &Path) -> Option<PathBuf> {
    let second = Path::new("/System/Volumes/Data").join(real.strip_prefix("/").ok()?);
    let identity =
        |path: &Path| crate::platform::fs::identity(path, crate::platform::fs::Links::Follow).ok();
    let distinct =
        crate::hostfs::canonical(&second).ok()? != crate::hostfs::canonical(real).ok()?;
    (distinct && identity(&second).is_some() && identity(&second) == identity(real))
        .then_some(second)
}

/// A repository in a scratch directory with one commit holding `files`.
pub(crate) fn repo_with(label: &str, files: &[(&[u8], &[u8])]) -> ScratchDir {
    let dir = ScratchDir::new(label);
    git(&dir, &["init", "-q"]);
    for (name, body) in files {
        let path = dir.join(
            crate::platform::names::path_from_name_bytes(name).expect("a name this host spells"),
        );
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "fixture"]);
    dir
}

/// Makes `branch` in the repository at `repo` and records `session` as its responsible
/// session, as the hook's report would have; the branch is the one the record was made for, which
/// is what a transfer asks.
pub(crate) fn branch_made_by(state: &AppState, repo: &Path, branch: &str, session: &str) {
    branch_made_from(state, repo, branch, session, None);
}

/// [`branch_made_by`], with the branch `target` recorded as the one it was created from.
pub(crate) fn branch_made_from(
    state: &AppState,
    repo: &Path,
    branch: &str,
    session: &str,
    target: Option<&str>,
) {
    git(repo, &["branch", branch]);
    let reflog = std::fs::read_to_string(repo.join(".git/logs/refs/heads").join(branch)).unwrap();
    let head = reflog.lines().next().unwrap().split('\t').next().unwrap();
    let mut fields = head.split(' ');
    let commit = fields.nth(1).unwrap().to_string();
    let time: i64 = head.rsplit(' ').nth(1).unwrap().parse().unwrap();
    let key = crate::branch_origin::repo_key(&crate::git::repo_files::repository_of(repo).unwrap());
    state
        .store
        .apply_branch_origins(
            &key,
            &[],
            &[crate::store::BranchOrigin {
                repository: key.clone(),
                branch: branch.to_string(),
                created_at: time * 1000,
                birth_commit: commit,
                birth_time: time,
                responsible_session: session.to_string(),
                responsible_since: time * 1000,
                responsible_title: format!("Work {session}"),
                target_branch: target.map(str::to_string),
            }],
            100,
        )
        .unwrap();
}

/// Two repositories whose `a.txt` differs, `(ours, theirs)`: a command that reads `a.txt` says
/// which of them it ran against.
pub(crate) fn repo_pair(label: &str) -> (ScratchDir, ScratchDir) {
    (
        repo_with(&format!("{label}-ours"), &[(b"a.txt", b"ours\n")]),
        repo_with(&format!("{label}-theirs"), &[(b"a.txt", b"theirs\n")]),
    )
}

/// The variables a login shell could export that send a `git` to `theirs` instead of the
/// repository its working directory names: the repository, index and common directory, and a
/// `remote.origin.url` passed through the environment by both of its channels. The tests that run
/// a command over these (`project_git`, `browse`) prove the filter is wired into each
/// path, each by what its own read can see — a file's content sees the first three, a remote's
/// URL the last two, and `GIT_WORK_TREE` none. Which names stay kept is pinned by the unit test
/// in `crate::git::env`, which takes its dropped names from here.
pub(crate) fn redirecting_overrides(theirs: &Path) -> Vec<(&'static str, String)> {
    let git_dir = theirs.join(".git");
    vec![
        ("GIT_DIR", git_dir.to_string_lossy().into_owned()),
        ("GIT_WORK_TREE", theirs.to_string_lossy().into_owned()),
        (
            "GIT_INDEX_FILE",
            git_dir.join("index").to_string_lossy().into_owned(),
        ),
        ("GIT_COMMON_DIR", git_dir.to_string_lossy().into_owned()),
        (
            "GIT_CONFIG_PARAMETERS",
            "'remote.origin.url'='https://example.com/other.git'".to_string(),
        ),
        ("GIT_CONFIG_COUNT", "1".to_string()),
        ("GIT_CONFIG_KEY_0", "remote.origin.url".to_string()),
        (
            "GIT_CONFIG_VALUE_0",
            "https://example.com/other.git".to_string(),
        ),
    ]
}

/// Every file under `.git`, by path, with its length, a digest of its bytes and its modification
/// time: what a read that must leave the repository alone is checked against. The modification
/// time is part of it because an index rewritten with the same stat information it already held
/// would compare equal by content alone; the digest stands in for the bytes so that a failure
/// prints an entry per file rather than tens of kilobytes of byte literals.
pub(crate) fn git_dir_state(repo: &Path) -> Vec<(PathBuf, u64, u64, std::time::SystemTime)> {
    use std::hash::{Hash, Hasher};

    let mut state = Vec::new();
    let mut stack = vec![repo.join(".git")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("the .git directory is readable") {
            let path = entry.expect("its entries are readable").path();
            let meta = std::fs::symlink_metadata(&path).expect("an entry has metadata");
            if meta.is_dir() {
                stack.push(path);
                continue;
            }
            let body = std::fs::read(&path).expect("an entry is readable");
            let mut digest = std::hash::DefaultHasher::new();
            body.hash(&mut digest);
            let modified = meta.modified().expect("an entry has a modification time");
            state.push((path, body.len() as u64, digest.finish(), modified));
        }
    }
    state.sort_by(|a, b| a.0.cmp(&b.0));
    state
}

/// Moves `path`'s modification time a minute back, leaving its content alone: a tracked file
/// whose stat information is stale, which is what makes a `git` read want to refresh the index.
///
/// Call it after every `git` the fixture runs. `commit` and `checkout` refresh the whole index,
/// so a file backdated before them has the moved time recorded as the index entry's own stat
/// data and the fixture is not stale at all.
pub(crate) fn move_modification_time_back(path: &Path) {
    let file = std::fs::File::options()
        .write(true)
        .open(path)
        .expect("the tracked file is writable");
    file.set_modified(std::time::SystemTime::now() - Duration::from_secs(60))
        .expect("its modification time is settable");
}
