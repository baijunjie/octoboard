//! What tells the daemon which session's git created a branch, and when it moved the refs a
//! project's branch header reads: a directory of hook dispatchers that a project session's agent
//! points `core.hooksPath` at through its environment, and the variables that do the pointing.
//!
//! The variables go on the agent's process alone, so they reach whatever that process starts —
//! the agent's shell, a script, a background job — and nothing else: no repository, no
//! configuration file and no other program is touched, and the daemon's own `git` drops every
//! `GIT_CONFIG_*` it could inherit (`crate::git::env::user_configuration_only`). `core.hooksPath`
//! goes in `GIT_CONFIG_PARAMETERS`, after whatever the user's own shell exports there, the way
//! `git -c` adds an entry: it takes precedence over the user's `GIT_CONFIG_COUNT` entries, which
//! are left as they are, and a `git -c core.hooksPath=…` of the user's takes precedence over it.
//! Not `GIT_CONFIG_COUNT`: an agent that strips the variables whose names hold `KEY` from the
//! commands it runs (Codex's shell environment policy, where its default excludes are turned on
//! with `ignore_default_excludes = false`) would take `GIT_CONFIG_KEY_<n>` and leave the count,
//! which fails every git that reads configuration. No variable this module sets has `KEY`,
//! `SECRET` or `TOKEN` in its name.
//!
//! The directory holds one copy of `dispatcher.sh` per hook name. With `core.hooksPath` pointed
//! at it, git looks for every hook there and nowhere else, so a hook name missing from it is a
//! user hook git would silently skip: the names are the ones this daemon knows, together with every
//! one the installed git's own documentation lists. Each copy runs the hook the user's own
//! configuration names (see the script), and `reference-transaction` reports the branches a
//! committed transaction created, and whether it touched `HEAD`, a local branch or a
//! remote-tracking branch (which has the repository's projects re-read,
//! `crate::project_git::ref_moves`), by starting `octoboardd git-ref` in the background. When the
//! user's hook has passed, a prepared transaction that moves an existing branch and a push of
//! branches (`pre-push`) are run past the revert gate by starting `octoboardd gate-hook` in the
//! foreground; only a status of 3 from it refuses the update.
//!
//! It is shared by every session and outlives them: it lives under the data directory, named by a
//! hash of its content, written once and left read-only. A background job a session leaves running
//! after it ended still finds it, and still reports and is gated: the two modes go to the daemon
//! through the owner record it keeps when the launch's own route has gone with the session's
//! process ([`route`]), found in the data directory the dispatchers are under. The executable the
//! dispatchers start is the installation's own entry point, which an update replaces in place, not
//! the copy of the build a holder runs from, which is removed once no holder runs from it; so a
//! newer build's modes may serve the dispatchers an older one wrote, and their arguments and
//! variables stay accepted across versions. A tool that installs hooks wherever
//! `git rev-parse --git-path hooks` points is refused there rather than writing into every
//! session's hooks. Beside it, a writable state directory carries what a `reference-transaction`
//! dispatcher learns while a transaction is prepared over to the same git's committed call, and
//! spools a long list of updates or refs.
//!
//! The script is written for one git build: the system-wide configuration files it scans for a
//! `core.hooksPath` are that build's, and a hook run by another build, which git tells apart by the
//! `GIT_EXEC_PATH` it exports, has git asked where the user's hook is instead.
//!
//! Nothing is injected — and nothing is attributed — when the user's `git` is older than
//! [`MIN_GIT`], when the user's `GIT_CONFIG_PARAMETERS` is one git rejects, or when the
//! directory cannot be written: a session without the variables works exactly as it would without
//! Octoboard.
//!
//! The dispatchers start two modes of this executable ([`ref_mode`], `octoboardd git-ref`, and
//! [`gate_mode`], `octoboardd gate-hook`), and the second asks the daemon what the revert gate is
//! to run with, which [`gate_inputs`] answers.

#[cfg(test)]
pub(crate) mod fixture;
pub mod gate_inputs;
pub mod gate_mode;
pub mod ref_mode;
pub(crate) mod route;
#[cfg(test)]
mod tests;

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{anyhow, bail, Context, Result};

use crate::env_shell;
use crate::git::run::{Access, GitError, Limits, Policy, Runner, UserGit, WorkDir};
use crate::paths;
use crate::platform::fs as platform;
use crate::platform::names;
use crate::platform::shell::{self, posix_quote};
use crate::processes::subprocess;

/// The oldest git the variables are given to: the `'key'='value'` form of a
/// `GIT_CONFIG_PARAMETERS` entry needs 2.31 (the variable itself, with `'key=value'` entries, is as
/// old as 1.7) and the `reference-transaction` hook 2.28, and 2.36 is the floor the rest of the
/// daemon holds git to.
const MIN_GIT: (u32, u32) = (2, 36);

/// The variable `core.hooksPath` goes in, as `git -c` passes its entries to the commands and hooks
/// a git starts.
const PARAMETERS_VARIABLE: &str = "GIT_CONFIG_PARAMETERS";

/// The key of the entry that points git at the dispatchers, as git itself writes it.
const HOOKS_PATH_KEY: &str = "core.hookspath";

/// The variable naming the session whose git reports a creation.
const SESSION_VARIABLE: &str = "OCTOBOARD_SESSION_ID";
/// The variable naming the port the report goes to.
const PORT_VARIABLE: &str = "OCTOBOARD_DAEMON_PORT";
/// The variable naming the launch's key file, whose key a request by the launch's route presents.
/// The two modes the dispatchers start read the path from the environment they inherit through
/// git from the agent; the key itself is in neither their arguments, which every user of the
/// machine can list, nor the environment, which an agent writes into files of its own. Its name
/// holds no `KEY`, which an agent's filter would strip.
const CREDENTIAL_PATH_VARIABLE: &str = "OCTOBOARD_DAEMON_CREDENTIAL_PATH";

/// The directory beside the dispatchers, under the same root, where a `reference-transaction`
/// dispatcher leaves what it learned while a transaction was prepared for the same git's committed
/// call, and spools a long list of updates or pushed refs. Writable, unlike the dispatchers.
const STATE_DIR: &str = "state";

/// How long a file in [`STATE_DIR`] is kept. Each is read back within the one git command that
/// wrote it, and one left behind by a git that died between its two calls is of no use to anyone.
const STATE_KEPT: Duration = Duration::from_secs(24 * 60 * 60);

/// How long a failure to make the dispatchers ready is remembered before a launch tries again: the
/// probes run on the launch path, and a git too old stays too old until the user changes it.
const FAILURE_KEPT: Duration = Duration::from_secs(10 * 60);

/// Every hook name `githooks(5)` documented when this was written. The installed git's own list is
/// added to it at run time (see [`documented_hook_names`]), so a git that knows more still finds
/// the user's hook under each of them.
const BUILT_IN_HOOKS: &[&str] = &[
    "applypatch-msg",
    "pre-applypatch",
    "post-applypatch",
    "pre-commit",
    "pre-merge-commit",
    "prepare-commit-msg",
    "commit-msg",
    "post-commit",
    "pre-rebase",
    "post-checkout",
    "post-merge",
    "pre-push",
    "pre-receive",
    "update",
    "proc-receive",
    "post-receive",
    "post-update",
    "reference-transaction",
    "push-to-checkout",
    "pre-auto-gc",
    "post-rewrite",
    "sendemail-validate",
    "fsmonitor-watchman",
    "p4-changelist",
    "p4-prepare-changelist",
    "p4-post-changelist",
    "p4-pre-submit",
    "post-index-change",
];

/// The dispatcher's source; see the script's own header.
const DISPATCHER: &str = include_str!("dispatcher.sh");

/// How long each `git` this module runs to learn about the installed git may take.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// What each of those `git` runs is held to.
const PROBE_LIMITS: Limits = Limits::deadline(PROBE_TIMEOUT);

/// How old a directory of an earlier content has to be before it is removed. A session started by
/// an earlier daemon, or a background job of one, may still be pointed at it, and a directory that
/// is gone makes git run no hook at all; a month leaves only what nothing plausibly still uses.
const STALE_AFTER: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// What making the dispatchers ready came to, by the directory they are made under, the `git` and
/// the daemon executable they were made for, so that only the first launch with each pays for the
/// probes: the directory, or when the attempt failed.
type Ready = Result<PathBuf, Instant>;
type ReadyByLaunch = HashMap<(PathBuf, String, String), Ready>;
static READY: Mutex<Option<ReadyByLaunch>> = Mutex::new(None);

/// The variables a project session's agent is started with so that its git reports the branches
/// it creates: `GIT_CONFIG_PARAMETERS` with `core.hooksPath` = the dispatchers after the user's own
/// entries, and what a report needs, which includes `key_file`, the launch's key file. `exe` is the
/// executable the dispatchers start: the installation's entry point (see the module header). Empty
/// when the session cannot be given them (see the module header).
pub fn session_env(
    shell_env: &HashMap<String, String>,
    exe: &str,
    session_id: &str,
    port: u16,
    key_file: &Path,
) -> Vec<(String, String)> {
    session_env_under(
        &paths::data_dir().join("githooks"),
        shell_env,
        exe,
        session_id,
        port,
        key_file,
    )
}

/// [`session_env`] with the dispatchers under `root`, which a test points into a directory of its
/// own.
pub(crate) fn session_env_under(
    root: &Path,
    shell_env: &HashMap<String, String>,
    exe: &str,
    session_id: &str,
    port: u16,
    key_file: &Path,
) -> Vec<(String, String)> {
    let Some(user) = user_parameters(shell_env, root) else {
        return Vec::new();
    };
    let dir = match ready_dir(root, shell_env, exe) {
        Ok(dir) => dir,
        Err(err) => {
            tracing::debug!(%err, "branch origins: no hook dispatchers for this session");
            return Vec::new();
        }
    };
    injected(&user, &dir, session_id, port, key_file)
}

/// One entry of a `GIT_CONFIG_PARAMETERS` value: its text as it stands, and the key and value it
/// sets (no value for an implicit `true`).
#[derive(Debug, PartialEq, Eq)]
struct Parameter<'a> {
    text: &'a str,
    key: String,
    value: Option<String>,
}

/// The user's own `GIT_CONFIG_PARAMETERS` entries, as text to put ours after: empty when there are
/// none. An entry pointing `core.hooksPath` under `root` is not the user's: it is what a daemon
/// started from inside another session inherited, and kept, its sessions would run both daemons'
/// dispatchers and report to both. `None` for a value git cannot read, which already fails every
/// git the user runs.
fn user_parameters(shell_env: &HashMap<String, String>, root: &Path) -> Option<String> {
    let value = shell_env
        .get(PARAMETERS_VARIABLE)
        .map(String::as_str)
        .unwrap_or("");
    let kept: Vec<&str> = parse_parameters(value)?
        .into_iter()
        .filter(|entry| {
            !(entry.key.eq_ignore_ascii_case(HOOKS_PATH_KEY)
                && entry
                    .value
                    .as_deref()
                    .is_some_and(|path| Path::new(path).starts_with(root)))
        })
        .map(|entry| entry.text)
        .collect();
    Some(kept.join(" "))
}

/// The entries of a `GIT_CONFIG_PARAMETERS` value, read as git reads it (`parse_config_env_list`
/// in its `config.c`): each a single-quoted key and value, `'key'='value'`, or the older
/// `'key=value'`, separated by whitespace, and a key that names a section and a variable. `None`
/// where git reports the value bogus, leading whitespace included.
fn parse_parameters(value: &str) -> Option<Vec<Parameter<'_>>> {
    let mut entries = Vec::new();
    let mut rest = value;
    while !rest.is_empty() {
        let start = rest;
        let (key, after) = sq_dequote_step(rest)?;
        let (key, value, after) = match after.strip_prefix('=') {
            // The newer form: a value of its own, or none for an implicit `true`.
            Some(after) if after.starts_with('\'') => {
                let (value, after) = sq_dequote_step(after)?;
                if after.starts_with(|c: char| !c.is_ascii_whitespace()) {
                    return None;
                }
                (key, Some(value), after)
            }
            Some(after)
                if after.is_empty() || after.starts_with(|c: char| c.is_ascii_whitespace()) =>
            {
                (key, None, after)
            }
            Some(_) => return None,
            None if after.is_empty() || after.starts_with(|c: char| c.is_ascii_whitespace()) => {
                // The older form, split at its first `=`.
                match key.split_once('=') {
                    Some((key, value)) => (key.to_string(), Some(value.to_string()), after),
                    None => (key, None, after),
                }
            }
            None => return None,
        };
        if !valid_key(&key) {
            return None;
        }
        entries.push(Parameter {
            text: &start[..start.len() - after.len()],
            key,
            value,
        });
        rest = after.trim_start_matches(|c: char| c.is_ascii_whitespace());
    }
    Some(entries)
}

/// Whether git takes `key` as a configuration key (`git_config_parse_key`): a section and a
/// variable name of letters, digits and `-`, the variable starting with a letter, with a
/// subsection of anything but a newline between them where there is one.
fn valid_key(key: &str) -> bool {
    let (Some(first), Some(last)) = (key.find('.'), key.rfind('.')) else {
        return false;
    };
    let (section, variable) = (&key[..first], &key[last + 1..]);
    let name = |word: &str| {
        !word.is_empty()
            && word
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    };
    name(section)
        && name(variable)
        && variable.starts_with(|c: char| c.is_ascii_alphabetic())
        && (first == last || !key[first..last].contains('\n'))
}

/// The single-quoted word at the start of `text`, unquoted, and what follows it: git's
/// `sq_dequote_step`, which takes `'\''` and `'\!'` between quoted parts as the character itself.
fn sq_dequote_step(text: &str) -> Option<(String, &str)> {
    let mut rest = text.strip_prefix('\'')?;
    let mut word = String::new();
    loop {
        let close = rest.find('\'')?;
        word.push_str(&rest[..close]);
        rest = &rest[close + 1..];
        let mut escaped = rest.chars();
        match (escaped.next(), escaped.next(), escaped.next()) {
            (Some('\\'), Some(c @ ('\'' | '!')), Some('\'')) => {
                word.push(c);
                rest = &rest[3..];
            }
            _ => return Some((word, rest)),
        }
    }
}

/// `value` single-quoted as git reads a `GIT_CONFIG_PARAMETERS` word: each `'` closed, escaped and
/// reopened. The dispatcher builds the same text to take the entry out again (see the script), so
/// the two quote alike.
fn sq_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// The key in the key file the session's environment names, for the helpers the dispatchers start;
/// `None` when there is no such variable, or its file is missing, unreadable or empty, which sends
/// those helpers to the daemon's own route.
pub(crate) fn key_from_environment() -> Option<String> {
    let path = std::env::var_os(CREDENTIAL_PATH_VARIABLE).filter(|path| !path.is_empty())?;
    crate::key_file::read(Path::new(&path))
}

/// The variables pointing at `dir` after the user's own `GIT_CONFIG_PARAMETERS` entries, `user`.
fn injected(
    user: &str,
    dir: &Path,
    session_id: &str,
    port: u16,
    key_file: &Path,
) -> Vec<(String, String)> {
    let ours = format!("'{HOOKS_PATH_KEY}'={}", sq_quote(&shell::posix_path(dir)));
    let parameters = if user.is_empty() {
        ours
    } else {
        format!("{user} {ours}")
    };
    vec![
        (PARAMETERS_VARIABLE.to_string(), parameters),
        (SESSION_VARIABLE.to_string(), session_id.to_string()),
        (PORT_VARIABLE.to_string(), port.to_string()),
        // The path as the helper, a native program, opens it; not the POSIX form of the hooks path.
        (
            CREDENTIAL_PATH_VARIABLE.to_string(),
            key_file.to_string_lossy().into_owned(),
        ),
    ]
}

/// The dispatchers under `root` for the user's `git` and this daemon executable, made ready on the
/// first call.
fn ready_dir(root: &Path, shell_env: &HashMap<String, String>, self_exe: &str) -> Result<PathBuf> {
    let exe = Path::new(self_exe);
    if !exe.is_absolute() || !exe.is_file() {
        bail!("the daemon executable `{self_exe}` cannot be found for the report");
    }
    let git = env_shell::resolve_program("git", shell_env)?;
    let key = (root.to_path_buf(), git.clone(), self_exe.to_string());
    let cached = READY
        .lock()
        .expect("hook dispatcher cache poisoned")
        .get_or_insert_with(HashMap::new)
        .get(&key)
        .cloned();
    match cached {
        // A directory removed behind the daemon's back is written again.
        Some(Ok(dir))
            if dir.join("reference-transaction").is_file() && root.join(STATE_DIR).is_dir() =>
        {
            mark_in_use(&dir);
            return Ok(dir);
        }
        Some(Err(failed)) if failed.elapsed() < FAILURE_KEPT => {
            bail!("making the hook dispatchers ready failed a moment ago");
        }
        _ => {}
    }
    let prepared = prepare(root, &git, shell_env, self_exe);
    if let Ok(dir) = &prepared {
        mark_in_use(dir);
    }
    READY
        .lock()
        .expect("hook dispatcher cache poisoned")
        .get_or_insert_with(HashMap::new)
        .insert(key, prepared.as_ref().cloned().map_err(|_| Instant::now()));
    prepared
}

/// Makes the dispatchers for `git` ready under `root` and returns their directory: checks the
/// version, learns the hook names and the system-wide configuration files of that git, and writes
/// the directory unless an identical one is already there.
fn prepare(
    root: &Path,
    git: &str,
    shell_env: &HashMap<String, String>,
    self_exe: &str,
) -> Result<PathBuf> {
    let git = Runner::new(
        UserGit::new(git, shell_env.clone()),
        Policy::PLAIN,
        PROBE_LIMITS,
    );
    let version = git_version(&git)?;
    if version < MIN_GIT {
        bail!(
            "git {}.{} is older than the hooks need",
            version.0,
            version.1
        );
    }
    let exec_path = exec_path(&git)?;
    let mut names: BTreeSet<String> = BUILT_IN_HOOKS.iter().map(|name| name.to_string()).collect();
    names.extend(documented_hook_names(&git));
    let state = root.join(STATE_DIR);
    fs::create_dir_all(&state).with_context(|| format!("creating {}", state.display()))?;
    platform::restrict_to_owner(&state)?;
    remove_stale_state(&state);
    let script = render(&Rendering {
        interpreter: shell::posix_interpreter(),
        self_exe,
        exec_path: &exec_path,
        state: &state,
        system_files: &system_config_files(&git),
    });
    write_dir(root, &names, &script)
}

/// The values a dispatcher is written with.
struct Rendering<'a> {
    /// The shell the dispatcher is run by; `None` leaves the one it names itself.
    interpreter: Option<PathBuf>,
    self_exe: &'a str,
    /// What `git --exec-path` says for the git whose configuration files were found, which git
    /// exports to every hook it runs as `GIT_EXEC_PATH`.
    exec_path: &'a str,
    state: &'a Path,
    system_files: &'a [PathBuf],
}

/// The dispatcher with this daemon's values in it.
fn render(values: &Rendering<'_>) -> String {
    let scans: String = values
        .system_files
        .iter()
        .map(|file| {
            format!(
                "\tocto_mentions {} && return 1\n",
                posix_quote(&shell::posix_path(file))
            )
        })
        .collect();
    let dispatcher = match (&values.interpreter, DISPATCHER.split_once('\n')) {
        // The first line names the interpreter.
        (Some(interpreter), Some((_, rest))) => {
            format!("#!{}\n{rest}", interpreter.display())
        }
        _ => DISPATCHER.to_string(),
    };
    dispatcher
        // The placeholder holds a line of its own.
        .replace("@SYSTEM_SCANS@\n", &scans)
        .replace(
            "@EXE@",
            &posix_quote(&shell::posix_path(Path::new(values.self_exe))),
        )
        .replace("@EXEC_PATH@", &posix_quote(values.exec_path))
        .replace("@STATE@", &posix_quote(&shell::posix_path(values.state)))
        .replace("@SESSION_VARIABLE@", SESSION_VARIABLE)
        .replace("@PORT_VARIABLE@", PORT_VARIABLE)
        .replace("@HOOKS_PATH_KEY@", HOOKS_PATH_KEY)
}

/// A `git` run to learn about the installed git, in the user's environment less anything that
/// could point it at a repository or a configuration item, and with its system-wide configuration
/// read where the build reads it: the dispatcher scans a `GIT_CONFIG_SYSTEM` of the hook's own.
fn probe(git: &Runner, args: &[&str]) -> Command {
    let mut command = git.command(Access::Write, WorkDir::At(Path::new("/")), args);
    command
        .env_remove("GIT_CONFIG_SYSTEM")
        .env_remove("GIT_CONFIG_NOSYSTEM");
    command
}

/// The [`probe`] that names the system-wide configuration file: git is asked to edit it, with
/// `echo` as the editor.
fn system_file_probe(git: &Runner) -> Command {
    let mut command = probe(git, &["config", "--system", "--edit"]);
    command.env("GIT_EDITOR", "echo");
    command
}

/// [`probe`] run with `args`: its stdout where `git` succeeded, `None` where it ran and failed,
/// and the stop as an error where it did not run to an exit.
fn probe_run(git: &Runner, args: &[&str]) -> Result<Option<Vec<u8>>> {
    run_probe(git, &mut probe(git, args))
}

/// Runs `command`, built by [`probe`], as [`probe_run`] does.
fn run_probe(git: &Runner, command: &mut Command) -> Result<Option<Vec<u8>>> {
    match git.run(command) {
        Ok(stdout) => Ok(Some(stdout)),
        Err(GitError::Exited { .. }) => Ok(None),
        Err(GitError::Stopped(stopped)) => Err(stopped.into_error()),
    }
}

fn exec_path(git: &Runner) -> Result<String> {
    let Some(stdout) = probe_run(git, &["--exec-path"])? else {
        bail!("`git --exec-path` gave no path");
    };
    parse_exec_path(&String::from_utf8(stdout)?)
}

/// The exec path `git --exec-path` printed, which the dispatcher compares with the
/// `GIT_EXEC_PATH` git exports, so it is kept as printed: an absolute path of a POSIX shell, or
/// with a drive letter, as git for Windows prints one.
fn parse_exec_path(output: &str) -> Result<String> {
    let path = output.trim();
    let bytes = path.as_bytes();
    let drive =
        bytes.len() > 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'/';
    if !path.starts_with('/') && !drive {
        bail!("`git --exec-path` gave no path");
    }
    Ok(path.to_string())
}

fn git_version(git: &Runner) -> Result<(u32, u32)> {
    let Some(stdout) = probe_run(git, &["--version"])? else {
        bail!("`git --version` failed");
    };
    parse_version(&String::from_utf8_lossy(&stdout))
        .ok_or_else(|| anyhow!("unrecognised `git --version` output"))
}

/// `git version 2.50.1 (Apple Git-155)` as `(2, 50)`.
fn parse_version(output: &str) -> Option<(u32, u32)> {
    let version = output.trim().strip_prefix("git version ")?;
    let mut parts = version.split(|c: char| !c.is_ascii_digit());
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

/// The hook names the installed git's `githooks(5)` documents: the section headings of its
/// manual page, or of its HTML page where no manual page is installed. Read from the files
/// themselves rather than through `git help`, which needs `man` and a pager. Whatever cannot be
/// read adds nothing, and the built-in list stands alone.
fn documented_hook_names(git: &Runner) -> Vec<String> {
    let path_of = |flag: &str| -> Option<PathBuf> {
        let stdout = probe_run(git, &[flag]).ok()??;
        let path = String::from_utf8(stdout).ok()?;
        let path = path.trim();
        (!path.is_empty()).then(|| PathBuf::from(path))
    };
    if let Some(man) = path_of("--man-path") {
        let page = man.join("man5/githooks.5");
        if let Ok(text) = fs::read_to_string(&page) {
            return hook_names_in_man(&text);
        }
        if let Some(text) = gunzip(&man.join("man5/githooks.5.gz")) {
            return hook_names_in_man(&text);
        }
    }
    if let Some(html) = path_of("--html-path") {
        if let Ok(text) = fs::read_to_string(html.join("githooks.html")) {
            return hook_names_in_html(&text);
        }
    }
    Vec::new()
}

/// A gzipped manual page, as most Linux distributions install them, through the system's `gzip`.
fn gunzip(path: &Path) -> Option<String> {
    if !path.is_file() {
        return None;
    }
    let mut command = Command::new("gzip");
    command.arg("-dc").arg(path).env_clear();
    let output = subprocess::run_with_timeout(&mut command, PROBE_TIMEOUT).ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8(output.stdout).ok())
        .flatten()
}

/// The `.SS "pre\-commit"` headings of the manual page's `HOOKS` section.
fn hook_names_in_man(text: &str) -> Vec<String> {
    let mut in_hooks = false;
    let mut names = Vec::new();
    for line in text.lines() {
        if let Some(section) = line.strip_prefix(".SH ") {
            in_hooks = section.trim().trim_matches('"') == "HOOKS";
            continue;
        }
        if !in_hooks {
            continue;
        }
        if let Some(heading) = line.strip_prefix(".SS ") {
            let name = heading.trim().trim_matches('"').replace("\\-", "-");
            if is_hook_name(&name) {
                names.push(name);
            }
        }
    }
    names
}

/// The `<h3 id="_pre_commit">pre-commit</h3>` headings of the HTML page.
fn hook_names_in_html(text: &str) -> Vec<String> {
    text.split("<h3 id=\"")
        .skip(1)
        .filter_map(|rest| {
            let (_, rest) = rest.split_once('>')?;
            let (name, _) = rest.split_once("</h3>")?;
            let name = name.trim();
            is_hook_name(name).then(|| name.to_string())
        })
        .collect()
}

/// A name that can be a file in the directory and a hook git asks for.
fn is_hook_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && name.as_bytes()[0].is_ascii_lowercase()
}

/// The system-wide configuration files this git reads, existing or not: the one `--system` names,
/// found by asking git to edit it with `echo` as the editor, and any other file it reports a value
/// from outside the user's and the repository's scopes — Apple's git reads one inside Xcode, for
/// one. The dispatcher scans each for a `core.hooksPath` it would otherwise not know about.
fn system_config_files(git: &Runner) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(Some(stdout)) = run_probe(git, &mut system_file_probe(git)) {
        let path = String::from_utf8_lossy(&stdout);
        let path = PathBuf::from(path.trim());
        if path.is_absolute() {
            files.push(path);
        }
    }
    let listing = probe_run(
        git,
        &["config", "-z", "--list", "--show-origin", "--show-scope"],
    );
    if let Ok(Some(stdout)) = listing {
        files.extend(other_scope_files(&stdout));
    }
    files.sort();
    files.dedup();
    files
}

/// The files of `git config -z --list --show-origin --show-scope` output whose scope is neither
/// the user's, the repository's nor the command line's: `<scope>\0file:<path>\0<entry>\0`, again
/// and again.
fn other_scope_files(output: &[u8]) -> Vec<PathBuf> {
    let fields: Vec<&[u8]> = output.split(|b| *b == 0).collect();
    fields
        .chunks(3)
        .filter_map(|entry| {
            let [scope, origin, _] = entry else {
                return None;
            };
            if matches!(*scope, b"global" | b"local" | b"worktree" | b"command") {
                return None;
            }
            let path = origin.strip_prefix(b"file:")?;
            names::path_from_name_bytes(path).filter(|path| path.is_absolute())
        })
        .collect()
}

/// Writes the directory for `names` and `script` under `root`, named by their hash, unless it is
/// already there as it should be, and removes directories of other content that have gone stale.
fn write_dir(root: &Path, names: &BTreeSet<String>, script: &str) -> Result<PathBuf> {
    let mut content = script.as_bytes().to_vec();
    for name in names {
        content.push(0);
        content.extend_from_slice(name.as_bytes());
    }
    let dir = root.join(format!("{:016x}", fnv1a(&content)));
    if !holds(&dir, names, script) {
        fs::create_dir_all(root).with_context(|| format!("creating {}", root.display()))?;
        if dir.exists() {
            // Something else changed it: made writable again, it is written anew.
            let _ = platform::set_read_only(&dir, false);
            fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))?;
        }
        write_new(root, &dir, names, script)?;
    }
    remove_stale(root, &dir);
    Ok(dir)
}

/// Whether `dir` holds exactly a copy of `script` git would run under each of `names`.
fn holds(dir: &Path, names: &BTreeSet<String>, script: &str) -> bool {
    names.iter().all(|name| {
        let path = dir.join(name);
        platform::is_hook_script(&path)
            && fs::read(&path).is_ok_and(|body| body == script.as_bytes())
    })
}

/// Writes the directory under a temporary name and renames it into place, so a session never sees
/// it half written. Another daemon writing the same content at the same moment is no failure.
fn write_new(root: &Path, dir: &Path, names: &BTreeSet<String>, script: &str) -> Result<()> {
    let staging = root.join(format!(
        ".staging-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    ));
    fs::create_dir(&staging).with_context(|| format!("creating {}", staging.display()))?;
    let written = (|| -> Result<()> {
        for name in names {
            let path = staging.join(name);
            let mut file = fs::File::create(&path)?;
            file.write_all(script.as_bytes())?;
            file.sync_all()?;
            platform::set_read_only(&path, true)?;
        }
        Ok(())
    })();
    let placed = written.and_then(|()| {
        fs::rename(&staging, dir).or_else(|err| {
            if holds(dir, names, script) {
                Ok(())
            } else {
                Err(err.into())
            }
        })
    });
    if staging.exists() {
        let _ = fs::remove_dir_all(&staging);
    }
    placed.with_context(|| format!("writing {}", dir.display()))?;
    platform::set_read_only(dir, true)?;
    Ok(())
}

/// Moves the directory's modification time to now, which is what [`remove_stale`] goes by: a
/// directory a daemon still starts sessions with is never stale, however long ago it was written.
fn mark_in_use(dir: &Path) {
    let _ = platform::set_dir_modified(dir, SystemTime::now());
}

/// Removes the directories under `root` other than `keep` — another content's, or a staging
/// directory a daemon left behind — that no daemon has used for [`STALE_AFTER`].
fn remove_stale(root: &Path, keep: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        if path == keep || entry.file_name() == STATE_DIR {
            continue;
        }
        if older_than(&entry, now, STALE_AFTER) {
            let _ = platform::set_read_only(&path, false);
            let _ = fs::remove_dir_all(&path);
        }
    }
}

/// Removes the files in the state directory older than [`STATE_KEPT`].
fn remove_stale_state(state: &Path) {
    let Ok(entries) = fs::read_dir(state) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        if older_than(&entry, now, STATE_KEPT) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

fn older_than(entry: &fs::DirEntry, now: SystemTime, age: Duration) -> bool {
    entry
        .metadata()
        .and_then(|meta| meta.modified())
        .is_ok_and(|modified| {
            now.duration_since(modified)
                .is_ok_and(|elapsed| elapsed > age)
        })
}

/// FNV-1a, 64 bits: a name for the content that stays the same across Rust releases, which the
/// standard hasher does not promise.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}
