//! What tells the daemon which session's git created a branch: a directory of hook dispatchers
//! that a project session's agent points `core.hooksPath` at through its environment, and the
//! variables that do the pointing.
//!
//! The variables go on the agent's process alone, so they reach whatever that process starts —
//! the agent's shell, a script, a background job — and nothing else: no repository, no
//! configuration file and no other program is touched, and the daemon's own `git` drops every
//! `GIT_CONFIG_*` it could inherit (`git_env::user_configuration_only`). They take the next free
//! slot of the user's own `GIT_CONFIG_COUNT`, so an entry the user exported keeps its place and
//! its effect.
//!
//! The directory holds one copy of `dispatcher.sh` per hook name. With `core.hooksPath` pointed
//! at it, git looks for every hook there and nowhere else, so a hook name missing from it is a
//! user hook git would silently skip: the names are the ones this daemon knows, together with every
//! one the installed git's own documentation lists. Each copy runs the hook the user's own
//! configuration names (see the script), and `reference-transaction` reports the branches a
//! committed transaction created by starting `octoboardd git-ref` in the background. When the
//! user's hook has passed, a prepared transaction that moves an existing branch and a push of
//! branches (`pre-push`) are run past the revert gate by starting `octoboardd gate-hook` in the
//! foreground; only a status of 3 from it refuses the update.
//!
//! It is shared by every session and outlives them, so a background job a session leaves running
//! still finds it: it lives under the data directory, named by a hash of its content, written once
//! and left read-only. A tool that installs hooks wherever `git rev-parse --git-path hooks` points
//! is refused there rather than writing into every session's hooks. Beside it, a writable state
//! directory carries what a `reference-transaction` dispatcher learns while a transaction is
//! prepared over to the same git's committed call, and spools a long list of updates or refs.
//!
//! The script is written for one git build: the system-wide configuration files it scans for a
//! `core.hooksPath` are that build's, and a hook run by another build, which git tells apart by the
//! `GIT_EXEC_PATH` it exports, has git asked where the user's hook is instead.
//!
//! Nothing is injected — and nothing is attributed — when the user's `git` is older than
//! [`MIN_GIT`], when `GIT_CONFIG_COUNT` is already unusable, or when the directory cannot be
//! written: a session without the variables works exactly as it would without Octoboard.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::Write as _;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{anyhow, bail, Context, Result};

use crate::adapter::shell_quote;
use crate::env_shell;
use crate::git_env;
use crate::paths;
use crate::subprocess;

/// The oldest git the variables are given to: `GIT_CONFIG_COUNT` needs 2.31 and the
/// `reference-transaction` hook 2.28, and 2.36 is the floor the rest of the daemon holds git to.
const MIN_GIT: (u32, u32) = (2, 36);

/// The variable naming the session whose git reports a creation.
const SESSION_VARIABLE: &str = "OCTOBOARD_SESSION_ID";
/// The variable naming the port the report goes to.
const PORT_VARIABLE: &str = "OCTOBOARD_DAEMON_PORT";
/// The variable carrying the daemon's key, which the report has to present.
const KEY_VARIABLE: &str = "OCTOBOARD_DAEMON_KEY";

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
pub(super) const BUILT_IN_HOOKS: &[&str] = &[
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

/// How old a directory of an earlier content has to be before it is removed. A session started by
/// an earlier daemon, or a background job of one, may still be pointed at it, and a directory that
/// is gone makes git run no hook at all; a month leaves only what nothing plausibly still uses.
const STALE_AFTER: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// What making the dispatchers ready came to, by the `git` and the daemon executable they were made
/// for, so that only the first launch with each pays for the probes: the directory, or when the
/// attempt failed.
type Ready = Result<PathBuf, Instant>;
static READY: Mutex<Option<HashMap<(String, String), Ready>>> = Mutex::new(None);

/// The variables a project session's agent is started with so that its git reports the branches
/// it creates: the next `GIT_CONFIG_COUNT` slot set to `core.hooksPath` = the dispatchers, and
/// what a report needs. Empty when the session cannot be given them (see the module header).
pub fn session_env(
    shell_env: &HashMap<String, String>,
    self_exe: &str,
    session_id: &str,
    port: u16,
    daemon_key: &str,
) -> Vec<(String, String)> {
    let root = paths::data_dir().join("githooks");
    let Some(user) = user_entries(shell_env, &root) else {
        return Vec::new();
    };
    let dir = match ready_dir(&root, shell_env, self_exe) {
        Ok(dir) => dir,
        Err(err) => {
            tracing::debug!(%err, "branch origins: no hook dispatchers for this session");
            return Vec::new();
        }
    };
    let mut env: Vec<(String, String)> = user
        .iter()
        .enumerate()
        .flat_map(|(slot, (key, value))| {
            [
                (format!("GIT_CONFIG_KEY_{slot}"), key.clone()),
                (format!("GIT_CONFIG_VALUE_{slot}"), value.clone()),
            ]
        })
        .collect();
    env.extend(injected(user.len(), &dir, session_id, port, daemon_key));
    env
}

/// The user's own `GIT_CONFIG_*` entries, in order, which keep their effect ahead of the
/// injected one. An entry pointing `core.hooksPath` under `root` is not the user's: it is what a
/// daemon started from inside another session inherited, and kept, its sessions would run both
/// daemons' dispatchers and report to both. `None` for a `GIT_CONFIG_COUNT` that is not a count,
/// which already fails every git the user runs.
fn user_entries(shell_env: &HashMap<String, String>, root: &Path) -> Option<Vec<(String, String)>> {
    let count = match shell_env.get("GIT_CONFIG_COUNT") {
        None => 0,
        Some(count) if count.is_empty() => 0,
        Some(count) => count.parse::<usize>().ok()?,
    };
    let entry = |name: &str, slot: usize| {
        shell_env
            .get(&format!("GIT_CONFIG_{name}_{slot}"))
            .cloned()
            .unwrap_or_default()
    };
    Some(
        (0..count)
            .map(|slot| (entry("KEY", slot), entry("VALUE", slot)))
            .filter(|(key, value)| {
                !(key.eq_ignore_ascii_case("core.hooksPath") && Path::new(value).starts_with(root))
            })
            .collect(),
    )
}

/// The variables for slot `slot` pointing at `dir`.
pub(super) fn injected(
    slot: usize,
    dir: &Path,
    session_id: &str,
    port: u16,
    key: &str,
) -> Vec<(String, String)> {
    vec![
        ("GIT_CONFIG_COUNT".to_string(), (slot + 1).to_string()),
        (
            format!("GIT_CONFIG_KEY_{slot}"),
            "core.hooksPath".to_string(),
        ),
        (
            format!("GIT_CONFIG_VALUE_{slot}"),
            dir.to_string_lossy().into_owned(),
        ),
        (SESSION_VARIABLE.to_string(), session_id.to_string()),
        (PORT_VARIABLE.to_string(), port.to_string()),
        (KEY_VARIABLE.to_string(), key.to_string()),
    ]
}

/// The dispatchers under `root` for the user's `git` and this daemon executable, made ready on the
/// first call.
fn ready_dir(root: &Path, shell_env: &HashMap<String, String>, self_exe: &str) -> Result<PathBuf> {
    let exe = Path::new(self_exe);
    if !exe.is_absolute() || !exe.is_file() {
        bail!("the daemon executable `{self_exe}` cannot be found for the report");
    }
    let git = env_shell::resolve_binary("git", shell_env)?;
    let key = (git.clone(), self_exe.to_string());
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
pub(super) fn prepare(
    root: &Path,
    git: &str,
    shell_env: &HashMap<String, String>,
    self_exe: &str,
) -> Result<PathBuf> {
    let version = git_version(git, shell_env)?;
    if version < MIN_GIT {
        bail!(
            "git {}.{} is older than the hooks need",
            version.0,
            version.1
        );
    }
    let exec_path = exec_path(git, shell_env)?;
    let mut names: BTreeSet<String> = BUILT_IN_HOOKS.iter().map(|name| name.to_string()).collect();
    names.extend(documented_hook_names(git, shell_env));
    let state = root.join(STATE_DIR);
    fs::create_dir_all(&state).with_context(|| format!("creating {}", state.display()))?;
    fs::set_permissions(&state, fs::Permissions::from_mode(0o700))?;
    remove_stale_state(&state);
    let script = render(&Rendering {
        interpreter: interpreter(),
        self_exe,
        exec_path: &exec_path,
        state: &state,
        system_files: &system_config_files(git, shell_env),
    });
    write_dir(root, &names, &script)
}

/// The values a dispatcher is written with.
pub(super) struct Rendering<'a> {
    pub(super) interpreter: &'a str,
    pub(super) self_exe: &'a str,
    /// What `git --exec-path` says for the git whose configuration files were found, which git
    /// exports to every hook it runs as `GIT_EXEC_PATH`.
    pub(super) exec_path: &'a str,
    pub(super) state: &'a Path,
    pub(super) system_files: &'a [PathBuf],
}

/// The shell the dispatchers run under. Every hook git runs starts one, several per commit, so
/// `dash` is taken where the system has it: it starts in well under half the time of the `bash`
/// behind macOS's `/bin/sh`. The script is POSIX either way.
pub(super) fn interpreter() -> &'static str {
    let dash = Path::new("/bin/dash");
    let usable = dash
        .metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0);
    if usable {
        "/bin/dash"
    } else {
        "/bin/sh"
    }
}

/// The dispatcher with this daemon's values in it.
pub(super) fn render(values: &Rendering<'_>) -> String {
    let scans: String = values
        .system_files
        .iter()
        .map(|file| {
            format!(
                "\tocto_mentions {} && return 1\n",
                shell_quote(&file.to_string_lossy())
            )
        })
        .collect();
    DISPATCHER
        .replacen("#!/bin/sh", &format!("#!{}", values.interpreter), 1)
        // The placeholder holds a line of its own.
        .replace("@SYSTEM_SCANS@\n", &scans)
        .replace("@EXE@", &shell_quote(values.self_exe))
        .replace("@EXEC_PATH@", &shell_quote(values.exec_path))
        .replace("@STATE@", &shell_quote(&values.state.to_string_lossy()))
        .replace("@SESSION_VARIABLE@", SESSION_VARIABLE)
        .replace("@PORT_VARIABLE@", PORT_VARIABLE)
        .replace("@KEY_VARIABLE@", KEY_VARIABLE)
}

/// A `git` run to learn about the installed git, in the user's environment less anything that
/// could point it at a repository or a configuration item, and with its system-wide configuration
/// read where the build reads it: the dispatcher scans a `GIT_CONFIG_SYSTEM` of the hook's own.
fn probe(git: &str, shell_env: &HashMap<String, String>) -> Command {
    let mut command = git_env::non_interactive(git, git_env::user_configuration_only(shell_env));
    command
        .current_dir("/")
        .env_remove("GIT_CONFIG_SYSTEM")
        .env_remove("GIT_CONFIG_NOSYSTEM");
    command
}

fn exec_path(git: &str, shell_env: &HashMap<String, String>) -> Result<String> {
    let mut command = probe(git, shell_env);
    command.arg("--exec-path");
    let output = subprocess::run_with_timeout(&mut command, PROBE_TIMEOUT)?;
    let path = String::from_utf8(output.stdout)?;
    let path = path.trim();
    if !output.status.success() || !path.starts_with('/') {
        bail!("`git --exec-path` gave no path");
    }
    Ok(path.to_string())
}

fn git_version(git: &str, shell_env: &HashMap<String, String>) -> Result<(u32, u32)> {
    let mut command = probe(git, shell_env);
    command.arg("--version");
    let output = subprocess::run_with_timeout(&mut command, PROBE_TIMEOUT)?;
    if !output.status.success() {
        bail!("`git --version` failed");
    }
    parse_version(&String::from_utf8_lossy(&output.stdout))
        .ok_or_else(|| anyhow!("unrecognised `git --version` output"))
}

/// `git version 2.50.1 (Apple Git-155)` as `(2, 50)`.
pub(super) fn parse_version(output: &str) -> Option<(u32, u32)> {
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
fn documented_hook_names(git: &str, shell_env: &HashMap<String, String>) -> Vec<String> {
    let path_of = |flag: &str| -> Option<PathBuf> {
        let mut command = probe(git, shell_env);
        command.arg(flag);
        let output = subprocess::run_with_timeout(&mut command, PROBE_TIMEOUT).ok()?;
        let path = String::from_utf8(output.stdout).ok()?;
        let path = path.trim();
        (output.status.success() && !path.is_empty()).then(|| PathBuf::from(path))
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
pub(super) fn hook_names_in_man(text: &str) -> Vec<String> {
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
pub(super) fn hook_names_in_html(text: &str) -> Vec<String> {
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
fn system_config_files(git: &str, shell_env: &HashMap<String, String>) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut command = probe(git, shell_env);
    command
        .env("GIT_EDITOR", "echo")
        .args(["config", "--system", "--edit"]);
    if let Ok(output) = subprocess::run_with_timeout(&mut command, PROBE_TIMEOUT) {
        let path = String::from_utf8_lossy(&output.stdout);
        let path = path.trim();
        if output.status.success() && path.starts_with('/') {
            files.push(PathBuf::from(path));
        }
    }
    let mut command = probe(git, shell_env);
    command.args(["config", "-z", "--list", "--show-origin", "--show-scope"]);
    if let Ok(output) = subprocess::run_with_timeout(&mut command, PROBE_TIMEOUT) {
        if output.status.success() {
            files.extend(other_scope_files(&output.stdout));
        }
    }
    files.sort();
    files.dedup();
    files
}

/// The files of `git config -z --list --show-origin --show-scope` output whose scope is neither
/// the user's, the repository's nor the command line's: `<scope>\0file:<path>\0<entry>\0`, again
/// and again.
pub(super) fn other_scope_files(output: &[u8]) -> Vec<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
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
            path.starts_with(b"/")
                .then(|| PathBuf::from(std::ffi::OsStr::from_bytes(path)))
        })
        .collect()
}

/// Writes the directory for `names` and `script` under `root`, named by their hash, unless it is
/// already there as it should be, and removes directories of other content that have gone stale.
pub(super) fn write_dir(root: &Path, names: &BTreeSet<String>, script: &str) -> Result<PathBuf> {
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
            let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o755));
            fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))?;
        }
        write_new(root, &dir, names, script)?;
    }
    remove_stale(root, &dir);
    Ok(dir)
}

/// Whether `dir` holds exactly an executable copy of `script` under each of `names`.
fn holds(dir: &Path, names: &BTreeSet<String>, script: &str) -> bool {
    names.iter().all(|name| {
        let path = dir.join(name);
        let executable = path
            .metadata()
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0);
        executable && fs::read(&path).is_ok_and(|body| body == script.as_bytes())
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
            fs::set_permissions(&path, fs::Permissions::from_mode(0o555))?;
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
    fs::set_permissions(dir, fs::Permissions::from_mode(0o555))?;
    Ok(())
}

/// Moves the directory's modification time to now, which is what [`remove_stale`] goes by: a
/// directory a daemon still starts sessions with is never stale, however long ago it was written.
fn mark_in_use(dir: &Path) {
    use std::os::unix::ffi::OsStrExt;
    let Ok(path) = std::ffi::CString::new(dir.as_os_str().as_bytes()) else {
        return;
    };
    // SAFETY: a valid C string, and a null `times` sets both times to now, which the owner may do
    // whatever the directory's mode is.
    unsafe {
        libc::utimes(path.as_ptr(), std::ptr::null());
    }
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
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o755));
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let man =
            ".SH \"DESCRIPTION\"\n.SS \"not\\-a\\-hook\"\n.SH \"HOOKS\"\n.SS \"pre\\-commit\"\n\
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
}
