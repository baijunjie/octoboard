//! Capturing the user's real shell environment, which every agent is then launched with.
//!
//! `$SHELL -l -c ...` is not enough: tool paths such as `grok`, `codex` and a node-version
//! manager's shims are set up in `~/.zshrc`, which a login-only non-interactive zsh never sources.
//! The working form is `$SHELL -l -i -c 'env -0 && printf <marker>'`, where `<marker>` is a fresh
//! random token generated per call (`snapshot_marker`) and searched for rather than inferred from
//! the shape of a trailing byte — see `wait_with_timeout` for why. Forcing `TERM=dumb` and feeding
//! `/dev/null` on stdin keeps that interactive shell from printing a prompt or reading a stray tty.
//!
//! That `TERM` is for the snapshot shell only and must not reach the agent: `env -0` dumps it like
//! any other variable, and an agent inheriting `TERM=dumb` downgrades its own renderer, colour
//! handling and mouse reporting. `SNAPSHOT_ONLY_VARS` is dropped from the snapshot and the PTY
//! command sets its own `TERM` instead.
//!
//! The snapshot is taken per launch rather than cached: a node-version manager's `PATH` entry can
//! point at a per-shell-instance directory, and a long-running daemon would otherwise never pick
//! up an edit the user makes to their shell configuration. Agent launches are human-paced, so the
//! extra `fork+exec` is immaterial.

use std::collections::HashMap;
use std::io::{self, Read};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

/// Upper bound on how long the login-shell snapshot may run. An rc file waiting on a version
/// manager or a network call is routinely a second or two; this leaves a wide margin above that
/// while still turning a wedged rc file (one blocked on something other than stdin, which never
/// resolves on its own) into a reported failure within a single human-noticeable wait instead of
/// a launch that hangs forever.
const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(10);

/// Upper bound on the bounded reap attempted after a timeout's `SIGKILL`. A process wedged in an
/// uninterruptible kernel wait (a dead network mount under `/Volumes` is the realistic trigger
/// here) can leave the signal pending indefinitely, and nothing short of the kernel itself can
/// force that reap to finish — so this is a best effort, not a guarantee, and giving up after it
/// elapses risks at most one leaked zombie.
const REAP_GRACE: Duration = Duration::from_millis(500);

/// Size of each chunk a pipe reader thread forwards over its channel. Arbitrary beyond "comfortably
/// smaller than the pipe's own OS buffer", since the channel is unbounded and nothing here needs
/// the chunking to line up with any particular boundary in the data.
const PIPE_CHUNK_SIZE: usize = 8192;

/// Variables that belong to the snapshot shell rather than to the agent, and are stripped from the
/// snapshot before it is handed to a PTY command.
pub const SNAPSHOT_ONLY_VARS: &[&str] = &[
    "TERM",
    "TERMINFO",
    "COLORTERM",
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
    "TERM_SESSION_ID",
];

/// Variables that mark *this process* as running inside an agent session. A daemon started from
/// within such a session would otherwise hand them to every agent it spawns, and the agent takes
/// them at face value: Claude Code turns transcript saving off and stops applying
/// `--permission-mode`, with no error anywhere.
///
/// Enumerated, because prefix matching is the wrong axis and fails in both directions. It strips
/// too much — `GROK_CODE_XAI_API_KEY`, `GROK_HOME`, `CODEX_HOME` and `CLAUDE_CODE_USE_BEDROCK` are
/// user settings, and dropping the first of those stops a key-authenticated session from starting
/// at all, with a failure that looks like a login problem. And it strips too little — a Claude Code
/// session also exports `CLAUDE_PID` and `CLAUDE_EFFORT`, which match neither `CLAUDECODE` nor
/// `CLAUDE_CODE_`. The two sets are not separable by name, so this list is what was observed inside
/// live sessions; extend it the same way, by dumping `env` inside a session rather than by guessing
/// a pattern.
///
/// Stripping these does not disturb authentication: credentials live in the macOS Keychain, so
/// `claude auth status` still reports a logged-in account with the markers removed. A login failure
/// is the obvious thing to blame this filter for, and re-adding the variables to "fix" it would
/// reintroduce the original bug.
pub const INHERITED_SESSION_VARS: &[&str] = &[
    "AI_AGENT",
    "CLAUDECODE",
    "CLAUDE_CODE_BRIDGE_MCP_CARRIER",
    "CLAUDE_CODE_BRIDGE_OWNER_ACCOUNT_UUID",
    "CLAUDE_CODE_BRIDGE_OWNER_ORG_UUID",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_ENVIRONMENT_KIND",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_SSE_PORT",
    "CLAUDE_CODE_WORKER_EPOCH",
    "CLAUDE_EFFORT",
    "CLAUDE_PID",
    "CLAUDE_PROJECT_DIR",
    "GROK_HOOK_EVENT",
    "GROK_HOOK_NAME",
    "GROK_SESSION_ID",
    "GROK_WORKSPACE_ROOT",
];

/// True when a variable marks the daemon's own agent session rather than the user's environment.
pub fn is_filtered(key: &str) -> bool {
    SNAPSHOT_ONLY_VARS.contains(&key) || INHERITED_SESSION_VARS.contains(&key)
}

pub fn shell_path() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string())
}

/// A fresh completion marker for one snapshot call. Generated from a v4 UUID rather than a fixed
/// string: an environment variable's value is arbitrary text the user's own shell configuration
/// controls, and a hardcoded sentinel could in principle appear inside one and be mistaken for the
/// real end of the dump. A random 122-bit value makes that collision not worth guarding against
/// further. See `wait_with_timeout` for what the marker is used for.
fn snapshot_marker() -> String {
    format!("OCTOBOARD-ENV-SNAPSHOT-END-{}", uuid::Uuid::new_v4())
}

/// Runs `$SHELL -l -i -c 'env -0 && printf <marker>'` and returns the parsed environment with the
/// filtered variables already removed.
pub fn snapshot() -> Result<HashMap<String, String>> {
    snapshot_with(&shell_path(), SNAPSHOT_TIMEOUT)
}

/// `snapshot()`'s body, with the shell and the timeout as parameters so a test can point both at
/// something under its control instead of the user's real login shell and a ten-second wait.
fn snapshot_with(shell: &str, timeout: Duration) -> Result<HashMap<String, String>> {
    let marker = snapshot_marker();
    // `&&` keeps a failing `env -0` on the non-zero-exit branch below: there is then no marker to
    // search for, and `wait_with_timeout` must not wait around for one.
    let command = format!("env -0 && printf '%s' '{marker}'");
    let mut child = Command::new(shell)
        .args(["-l", "-i", "-c", &command])
        .env("TERM", "dumb")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // A new process group with this process as its own leader (`setpgid(0, 0)`), so the
        // timeout below can kill everything the shell started — including a background process
        // an rc file left running — rather than only the shell itself: such a process lives in
        // the shell's group, not under the shell as a reapable child, so signalling the leader
        // alone would leave it running.
        .process_group(0)
        .spawn()
        .with_context(|| {
            format!("spawning `{shell} -l -i -c '{command}'` to snapshot the environment")
        })?;

    // Piped output has to be drained on its own threads rather than read after `wait()`: the
    // child can fill the pipe buffer and block on a write long before the timeout below ever
    // notices it, which is exactly the kind of hang this function exists to avoid. Each thread
    // forwards what it reads in chunks over an unbounded channel instead of being `join()`ed or
    // read to completion in one shot, so the consumer can inspect what has arrived so far without
    // waiting for the pipe to close — see `wait_with_timeout` for why that matters. On a timeout
    // the threads are left to finish on their own once the killed process group closes the pipes,
    // which happens promptly enough that nothing meaningful leaks.
    //
    // A successful snapshot can leak one of these threads instead: when a backgrounded process
    // is still holding a pipe open, `wait_with_timeout` deliberately returns without killing the
    // group (see there), so that thread stays parked in `read()` — and leaked with it, one pipe
    // read-end — until whatever it backgrounded writes again or dies. This is accepted rather
    // than fixed: the only portable way to unstick a thread blocked in `read()` is to close its
    // fd out from under it, and doing that from a different thread while the read is in flight is
    // its own hazard (the closed descriptor number can be reused for an unrelated file before the
    // blocked call notices, misdirecting it). A thread and an fd per long-lived backgrounded
    // process over the daemon's own lifetime is the cheaper failure mode.
    let mut stdout_pipe = child.stdout.take().expect("stdout was piped");
    let mut stderr_pipe = child.stderr.take().expect("stderr was piped");
    let (stdout_tx, stdout_rx) = mpsc::channel();
    let (stderr_tx, stderr_rx) = mpsc::channel();
    thread::spawn(move || pipe_reader(&mut stdout_pipe, &stdout_tx));
    thread::spawn(move || pipe_reader(&mut stderr_pipe, &stderr_tx));

    let (status, stdout, stderr) = wait_with_timeout(
        &mut child, stdout_rx, stderr_rx, shell, &command, &marker, timeout,
    )?;

    if !status.success() {
        bail!(
            "`{shell} -l -i -c '{command}'` exited with {:?}; stderr: {}",
            status.code(),
            String::from_utf8_lossy(&stderr)
        );
    }

    let mut env = HashMap::new();
    for pair in stdout.split(|&b| b == 0) {
        if pair.is_empty() {
            continue;
        }
        let text = String::from_utf8_lossy(pair);
        if let Some((key, value)) = text.split_once('=') {
            if is_filtered(key) {
                continue;
            }
            env.insert(key.to_string(), value.to_string());
        }
    }
    Ok(env)
}

/// Reads `pipe` in chunks until EOF or an error, forwarding each over `tx`. Run on its own thread
/// so the pipe keeps draining regardless of whether or how fast the consumer end catches up: the
/// channel is unbounded, so a send never blocks the read loop the way a bounded hand-off (or a
/// single `read_to_end` held until the whole message is assembled) would. An `Err` is forwarded
/// like any other message rather than swallowed, so a mid-read io error reaches the consumer
/// instead of silently truncating what came before it; either outcome ends the loop.
fn pipe_reader(pipe: &mut impl Read, tx: &mpsc::Sender<io::Result<Vec<u8>>>) {
    let mut chunk = vec![0u8; PIPE_CHUNK_SIZE];
    loop {
        match pipe.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                if tx.send(Ok(chunk[..n].to_vec())).is_err() {
                    break;
                }
            }
            // A read the kernel interrupted to deliver a signal has not failed and has consumed
            // nothing; retrying is what the caller means by "read this pipe". Ending the thread
            // here instead would refuse the launch over stdout, and on the pre-exit path would
            // leave the shell neither killed nor reaped.
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => {
                let _ = tx.send(Err(err));
                break;
            }
        }
    }
}

/// Whether a reader channel still has a live sender on the other end. `Disconnected` means the
/// pipe behind it has already hit EOF and `pipe_reader`'s thread has exited — the bytes already
/// drained are final for that pipe, and nothing will ever add to them again.
#[derive(Debug)]
enum DrainOutcome {
    Open,
    Disconnected,
}

/// Appends every chunk currently waiting on `rx` to `buf`, without blocking for more to arrive,
/// and reports whether the channel is still open. Surfaces a reader thread's io error instead of
/// discarding the bytes already appended ahead of it — unlike `read_to_end`, which on error still
/// leaves its partial buffer looking complete to the caller.
fn drain_available(
    rx: &mpsc::Receiver<io::Result<Vec<u8>>>,
    buf: &mut Vec<u8>,
    which: &str,
) -> Result<DrainOutcome> {
    loop {
        match rx.try_recv() {
            Ok(chunk) => {
                let bytes =
                    chunk.with_context(|| format!("reading {which} from the snapshot shell"))?;
                buf.extend_from_slice(&bytes);
            }
            Err(mpsc::TryRecvError::Empty) => return Ok(DrainOutcome::Open),
            Err(mpsc::TryRecvError::Disconnected) => return Ok(DrainOutcome::Disconnected),
        }
    }
}

/// Drains whatever has arrived on `rx` into `buf`, discarding a read error instead of surfacing
/// it. Stderr is read only for its contents (the non-zero-exit error message in `snapshot_with`),
/// never awaited for completeness, so an io error on it must not cost a good launch its result.
fn drain_stderr_best_effort(rx: &mpsc::Receiver<io::Result<Vec<u8>>>, buf: &mut Vec<u8>) {
    let _ = drain_available(rx, buf, "stderr");
}

/// Byte offset where `marker` begins in `haystack`, or `None` if it has not arrived (in full) yet.
/// A plain substring scan rather than anything tuned for it: `stdout` here is at most a few KiB.
fn find_marker(haystack: &[u8], marker: &[u8]) -> Option<usize> {
    if marker.is_empty() || haystack.len() < marker.len() {
        return None;
    }
    haystack.windows(marker.len()).position(|w| w == marker)
}

/// Waits for `child` to exit, then drains both output pipes, all within `timeout`.
///
/// The two phases need different completion signals. Up to exit, `try_wait` is polled rather than
/// blocking on `wait()` in another thread, since there is no portable way to cancel the latter —
/// the child would still have to be killed through `child` itself, from this thread, once the
/// deadline passed.
///
/// After exit, EOF on the pipes is not a safe signal to wait for: an rc file that starts a
/// background process (a version manager's update check, `direnv`, `brew` autoupdate) hands it
/// the inherited stdout, and the shell can exit while that process keeps the pipe open — nothing
/// here is waiting on that process, so EOF on stdout can lag arbitrarily far behind the shell's
/// own exit. `env -0` emits a NUL after every variable, though, including the last, so a drain
/// landing right after an early one is indistinguishable from one landing after the real end —
/// `command` therefore appends `&& printf '<marker>'`, and completeness is tested by finding that
/// marker in what has arrived so far, not by the shape of a trailing byte. Stderr is drained
/// alongside on a best-effort basis only; it is never awaited for completeness, so a process
/// backgrounding only stderr does not cost a good launch its result.
///
/// A disconnected stdout reader (its pipe hit EOF) with no marker yet means the shell has already
/// exited without producing a complete dump and nothing is left holding stdout open — there is
/// nothing to wait for, so this fails immediately instead of spinning to the deadline, and skips
/// the group kill below: an empty group has no member for `-pid` to still validly reach (see
/// `kill_group_after_timeout`).
///
/// On either phase's timeout the whole process group is killed (not just the shell, for the same
/// reason `process_group(0)` was set above), so anything still holding a pipe open is stopped.
fn wait_with_timeout(
    child: &mut Child,
    stdout_rx: mpsc::Receiver<io::Result<Vec<u8>>>,
    stderr_rx: mpsc::Receiver<io::Result<Vec<u8>>>,
    shell: &str,
    command: &str,
    marker: &str,
    timeout: Duration,
) -> Result<(ExitStatus, Vec<u8>, Vec<u8>)> {
    let deadline = Instant::now() + timeout;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let status = loop {
        drain_available(&stdout_rx, &mut stdout, "stdout")?;
        drain_stderr_best_effort(&stderr_rx, &mut stderr);
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            return Err(kill_group_after_timeout(child, shell, command, timeout));
        }
        thread::sleep(Duration::from_millis(20));
    };

    // A non-zero exit never ran the command to completion (the `&&` short-circuits before
    // `printf` runs), so there is no marker to wait for and the dump is discarded either way —
    // which makes `stderr` the only thing worth having here, since it carries the whole of what
    // the user will be told went wrong. So stdout is drained best-effort too (a read error on a
    // buffer nobody will parse must not replace that message), and stderr is given a short
    // settle rather than one non-blocking grab, so a diagnostic longer than a single chunk does
    // not reach the user cut off at a chunk boundary.
    if !status.success() {
        let _ = drain_available(&stdout_rx, &mut stdout, "stdout");
        let settle = Instant::now() + REAP_GRACE;
        while Instant::now() < settle {
            if matches!(
                drain_available(&stderr_rx, &mut stderr, "stderr"),
                Ok(DrainOutcome::Disconnected) | Err(_)
            ) {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        return Ok((status, stdout, stderr));
    }

    let marker = marker.as_bytes();
    loop {
        let stdout_state = drain_available(&stdout_rx, &mut stdout, "stdout")?;
        drain_stderr_best_effort(&stderr_rx, &mut stderr);
        if let Some(end) = find_marker(&stdout, marker) {
            stdout.truncate(end);
            break;
        }
        if matches!(stdout_state, DrainOutcome::Disconnected) {
            bail!(
                "`{shell} -l -i -c '{command}'` exited cleanly but closed its output before \
                 writing a complete environment snapshot, and nothing is left holding that pipe \
                 open for it to ever complete; stderr: {}",
                String::from_utf8_lossy(&stderr)
            );
        }
        if Instant::now() >= deadline {
            return Err(kill_group_after_timeout(child, shell, command, timeout));
        }
        thread::sleep(Duration::from_millis(20));
    }
    Ok((status, stdout, stderr))
}

/// Kills the whole process group behind `child` (including anything an rc file backgrounded that
/// is still holding a pipe open) and returns the user-facing timeout error.
fn kill_group_after_timeout(
    child: &mut Child,
    shell: &str,
    command: &str,
    timeout: Duration,
) -> anyhow::Error {
    let pid = child.id();
    // SAFETY: a plain signal to a process group number. `-pid` stays valid for it as long as
    // some member of the group is still alive, and that is exactly the condition each of
    // `wait_with_timeout`'s two timeout call sites diagnoses before calling here: the shell
    // itself hasn't exited, or its stdout reader is still connected (something still holds the
    // pipe open) with the marker not yet found. The narrow residual risk is the group emptying
    // out in the gap between that diagnosis and this call, in which case the id could have been
    // recycled and the signal reaches an unrelated group.
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
    // A bounded reap, not `child.wait()`: on the drain-timeout call site `child` is already
    // reaped (the exit-wait loop above got `Some` from `try_wait`), so this is a no-op there, but
    // on the exit-timeout call site it still needs reaping — and a shell wedged in an
    // uninterruptible kernel wait (a dead network mount under `/Volumes` is the realistic
    // trigger) can leave the `SIGKILL` pending indefinitely, which would turn this error path
    // into the same unbounded hang it exists to report. A zombie left behind on that rare timeout
    // is far cheaper than a second wedged launch.
    let reap_deadline = Instant::now() + REAP_GRACE;
    while Instant::now() < reap_deadline {
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => break,
            Ok(None) => thread::sleep(Duration::from_millis(20)),
        }
    }
    anyhow::anyhow!(
        "`{shell} -l -i -c '{command}'` did not finish within {timeout:?}; a shell startup file \
         is probably blocked on something other than stdin, or it left a background process \
         holding the shell's output open. Octoboard refuses to launch with an unknown environment \
         rather than guess at one — fix or skip the slow step in the shell's rc files."
    )
}

/// Resolves a bare binary name to an absolute path by searching the snapshot's `PATH`, mirroring
/// what the login shell would have found. Deliberately does not leave resolution to
/// `portable_pty`, whose own search tries `cwd.join(exe)` first and accepts it on `.exists()`
/// alone — so a project file named `claude` would silently shadow the real CLI.
pub fn resolve_binary(name: &str, env: &HashMap<String, String>) -> Result<String> {
    use std::os::unix::fs::PermissionsExt;

    let path_var = env.get("PATH").map(String::as_str).unwrap_or_default();
    for dir in path_var.split(':') {
        if dir.is_empty() {
            continue;
        }
        let candidate = std::path::Path::new(dir).join(name);
        let executable = candidate
            .metadata()
            .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false);
        if executable {
            return Ok(candidate.to_string_lossy().into_owned());
        }
    }
    bail!("`{name}` was not found on PATH in the snapshotted shell environment")
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::OnceLock;

    use super::*;

    static TEMP_PATH_COUNTER: AtomicUsize = AtomicUsize::new(0);

    /// Builds a process-unique temp-file path labeled `label`. Shared by `FakeShell` and the
    /// tests that hand the fixture shell a path to write a pid into, so the naming scheme lives
    /// in one place. Uniqueness comes from an `AtomicUsize` counter rather than `ThreadId`, which
    /// collapses to the same `ThreadId(1)` for every test under `--test-threads=1` and, formatted
    /// with `{:?}`, contains parentheses that would otherwise have to be worked around wherever
    /// the path is substituted into shell script text.
    fn temp_path(label: &str) -> std::path::PathBuf {
        let n = TEMP_PATH_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "octoboardd-env-shell-test-{label}-{}-{n}",
            std::process::id()
        ))
    }

    /// A path reserved for a fixture shell script to write into, removed on drop regardless of
    /// whether the script ever created it — including when a test panics before reaching its own
    /// cleanup.
    struct TempFile(std::path::PathBuf);

    impl TempFile {
        fn new(label: &str) -> Self {
            Self(temp_path(label))
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            std::fs::remove_file(&self.0).ok();
        }
    }

    /// Polls for `path` to exist and parse as the pid it is expected to hold, rather than
    /// asserting it is already there: the fixture shell writes it concurrently, and reading too
    /// early is this suite's one real wall-clock race. Panics with a message naming that race
    /// specifically, instead of the generic message a bare `.expect()` on a missing file would
    /// give, if `deadline` passes first.
    fn read_pid_file_when_ready(path: &std::path::Path, deadline: Instant) -> i32 {
        loop {
            if let Ok(text) = std::fs::read_to_string(path) {
                if let Ok(pid) = text.trim().parse() {
                    return pid;
                }
            }
            if Instant::now() >= deadline {
                panic!("{path:?} was not written with a pid before the test's own deadline");
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn errno_is_esrch() -> bool {
        std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
    }

    /// Waits for `pid` to no longer exist, failing the test if it outlives `deadline`. Used to
    /// confirm a process the fixture backgrounded was actually reaped rather than leaked, whether
    /// by the production code's own group kill or by a test cleaning up after a successful
    /// snapshot that (correctly) never touched it.
    fn assert_gone_by(pid: i32, deadline: Instant) {
        loop {
            if unsafe { libc::kill(pid, 0) } == -1 && errno_is_esrch() {
                return;
            }
            if Instant::now() >= deadline {
                panic!("pid {pid} outlived the test's deadline");
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    /// The one executable every `FakeShell` ultimately execs, written once and reused for the
    /// rest of the process's tests. macOS serializes code-signing evaluation across concurrent
    /// first execs of a freshly written executable — measured at roughly 80ms apart per
    /// concurrent exec, against 34-39ms once a script has already been exec'd — so a fixture that
    /// wrote and immediately exec'd a fresh script per call could burn most of a short test
    /// timeout before its own simulated behaviour ever ran. A `FakeShell` therefore never writes
    /// a fresh executable: it is a symlink to this one already-validated binary (symlink creation
    /// and resolution carry none of that cost), and the per-test behaviour lives in a plain,
    /// non-executable companion file next to the symlink that this dispatcher `eval`s.
    ///
    /// `snapshot_with` bakes a fresh completion marker into the `-c` argument it passes
    /// (`env -0 && printf '<marker>'`); this dispatcher pulls that marker back out of `$4` and
    /// re-exports it as `$OCTOBOARD_TEST_MARKER` so the companion script can print it wherever it
    /// wants to signal that its simulated dump is complete, without needing to know the value in
    /// advance.
    fn fake_shell_dispatcher() -> &'static std::path::Path {
        static DISPATCHER: OnceLock<std::path::PathBuf> = OnceLock::new();
        DISPATCHER.get_or_init(|| {
            // One fixed name rather than a per-run unique one: nothing removes this file (the
            // `OnceLock` holds a path, not a guard, and a `FakeShell` only cleans up its own
            // symlink and companion), so a unique name would leave one behind per test-binary
            // invocation. The content is byte-identical every time, which makes a second test
            // binary writing it concurrently harmless.
            let path = std::env::temp_dir().join("octoboardd-env-shell-test-shell-dispatcher");
            std::fs::write(
                &path,
                "#!/bin/sh\n\
                 marker=${4%\\'}\n\
                 export OCTOBOARD_TEST_MARKER=${marker##*\\'}\n\
                 eval \"$(cat \"$0.body\")\"\n",
            )
            .expect("write fake shell dispatcher");
            let mut perms = std::fs::metadata(&path).unwrap().permissions();
            std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o700);
            std::fs::set_permissions(&path, perms).expect("chmod fake shell dispatcher");
            path
        })
    }

    /// A standalone shell standing in for `$SHELL`, cleaned up on drop. It ignores the
    /// `-l -i -c` part of the arguments `snapshot_with` passes — a real shell would act on them,
    /// but the fixture only needs to control how long the process runs and what it prints — and
    /// runs `body` instead, with `$OCTOBOARD_TEST_MARKER` available for it to print when it wants
    /// to simulate a complete dump (see `fake_shell_dispatcher`).
    struct FakeShell {
        path: std::path::PathBuf,
    }

    impl FakeShell {
        fn new(body: &str) -> Self {
            let path = temp_path("shell");
            std::fs::write(format!("{}.body", path.display()), body)
                .expect("write fake shell body");
            std::os::unix::fs::symlink(fake_shell_dispatcher(), &path).expect("symlink fake shell");
            Self { path }
        }

        fn path(&self) -> &str {
            self.path.to_str().expect("utf8 path")
        }
    }

    impl Drop for FakeShell {
        fn drop(&mut self) {
            std::fs::remove_file(&self.path).ok();
            std::fs::remove_file(format!("{}.body", self.path.display())).ok();
        }
    }

    #[test]
    fn parses_and_filters_a_normal_snapshot() {
        let shell = FakeShell::new(
            "printf 'FOO=bar\\0TERM=dumb\\0CLAUDE_PID=1\\0'\nprintf '%s' \"$OCTOBOARD_TEST_MARKER\"",
        );
        let env = snapshot_with(shell.path(), Duration::from_secs(5)).expect("snapshot");
        assert_eq!(env.get("FOO"), Some(&"bar".to_string()));
        // `TERM` is snapshot-only and `CLAUDE_PID` marks the daemon's own session; both must be
        // stripped before the caller ever sees them.
        assert!(!env.contains_key("TERM"));
        assert!(!env.contains_key("CLAUDE_PID"));
    }

    #[test]
    fn a_shell_exiting_nonzero_is_reported_with_its_stderr() {
        let shell = FakeShell::new("echo 'boom' >&2; exit 7");
        let err = snapshot_with(shell.path(), Duration::from_secs(5))
            .expect_err("nonzero exit must fail");
        let message = err.to_string();
        assert!(message.contains("boom"));
        assert!(
            message.contains("Some(7)"),
            "exit code must reach the message: {message}"
        );
    }

    #[test]
    fn a_shell_emitting_more_than_a_pipe_buffer_does_not_deadlock() {
        // Several hundred KiB — comfortably more than a pipe's OS buffer (64 KiB on both Linux and
        // macOS). If the pipes were read only after `wait()` instead of concurrently by the two
        // reader threads, the shell would block on a full pipe forever while this thread blocked
        // on `wait()` for the shell to exit — each waiting on the other. A regression back to that
        // shape makes this test hang rather than quietly pass.
        let shell = FakeShell::new(
            "i=0\nwhile [ \"$i\" -lt 20000 ]; do\n  printf 'K%d=v%d\\0' \"$i\" \"$i\"\n  \
             i=$((i+1))\ndone\nprintf '%s' \"$OCTOBOARD_TEST_MARKER\"",
        );
        let env = snapshot_with(shell.path(), Duration::from_secs(5)).expect("snapshot");
        assert_eq!(env.get("K0"), Some(&"v0".to_string()));
        assert_eq!(env.get("K19999"), Some(&"v19999".to_string()));
    }

    #[test]
    fn a_backgrounded_process_holding_the_pipe_open_does_not_fail_a_complete_snapshot() {
        // The shell itself exits almost immediately once its output (ending in the completion
        // marker it prints) is already sitting in the pipe; a backgrounded process it leaves
        // behind keeps that pipe's write end open for far longer (the shape of hang
        // `/bin/sh -c '(sleep 3 &) ; echo FOO=bar' | cat` demonstrates: it returns after 3s, not
        // immediately). EOF is therefore not a safe completeness signal, and the data being
        // already complete means this must succeed quickly rather than wait out the backgrounded
        // process or time out.
        let shell = FakeShell::new(
            "sleep 300 & printf 'BGPID=%s\\0FOO=bar\\0' \"$!\"\nprintf '%s' \"$OCTOBOARD_TEST_MARKER\"",
        );
        let timeout = Duration::from_secs(1);
        let started = Instant::now();
        let env = snapshot_with(shell.path(), timeout)
            .expect("a pipe held open by a backgrounded process must not fail a complete snapshot");
        assert_eq!(env.get("FOO"), Some(&"bar".to_string()));
        // A generous margin over `timeout` for scheduling jitter — nowhere near the `sleep 300`
        // waiting out the backgrounded process would take.
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "snapshot_with took {:?}, far longer than its {timeout:?} timeout",
            started.elapsed(),
        );
        // A successful snapshot has no reason to touch the process group, so the backgrounded
        // sleep is still alive here; clean it up ourselves rather than leaking it for its full
        // 300s.
        let bg_pid: i32 = env.get("BGPID").expect("BGPID").parse().expect("bg pid");
        unsafe { libc::kill(bg_pid, libc::SIGKILL) };
        assert_gone_by(bg_pid, Instant::now() + Duration::from_secs(2));
    }

    #[test]
    fn a_backgrounded_process_holding_only_stderr_open_does_not_fail_a_complete_snapshot() {
        // The backgrounded process redirects its own stdout to `/dev/null` but still inherits
        // stderr, so it holds only the stderr pipe open. stdout is read for the environment and
        // stderr only for the non-zero-exit error message below, so this must succeed exactly
        // like a clean exit would — coupling success to stderr's completeness would fail a good
        // launch over a pipe this function never needed to wait on.
        let shell = FakeShell::new(
            "sleep 300 >/dev/null & printf 'BGPID=%s\\0FOO=bar\\0' \"$!\"\nprintf '%s' \"$OCTOBOARD_TEST_MARKER\"",
        );
        let timeout = Duration::from_secs(1);
        let started = Instant::now();
        let env = snapshot_with(shell.path(), timeout).expect(
            "a backgrounded process holding only stderr open must not fail a complete snapshot",
        );
        assert_eq!(env.get("FOO"), Some(&"bar".to_string()));
        assert!(started.elapsed() < Duration::from_secs(5));
        let bg_pid: i32 = env.get("BGPID").expect("BGPID").parse().expect("bg pid");
        unsafe { libc::kill(bg_pid, libc::SIGKILL) };
        assert_gone_by(bg_pid, Instant::now() + Duration::from_secs(2));
    }

    #[test]
    fn a_backgrounded_process_holding_an_incomplete_snapshot_open_still_times_out() {
        // The shell exits immediately after a partial write with no completion marker — the
        // simulated dump never got to finish — while a backgrounded process keeps the pipe open
        // indefinitely, so the stdout reader stays connected and the drain-timeout branch cannot
        // take the disconnected-reader fast-fail path; only the deadline can end this. This is
        // what distinguishes it from
        // `a_shell_exiting_cleanly_without_a_complete_snapshot_and_nothing_holding_the_pipes_fails_fast`
        // below, and from the exit-timeout path
        // `a_shell_that_hangs_is_killed_along_with_its_backgrounded_child` exercises.
        let child_pid_path = TempFile::new("drain-timeout-bg-pid");
        let shell = FakeShell::new(&format!(
            "sleep 300 & echo $! > '{path}'\nprintf 'FOO=ba'",
            path = child_pid_path.path().display(),
        ));
        let timeout = Duration::from_secs(1);
        let started = Instant::now();
        let err = snapshot_with(shell.path(), timeout)
            .expect_err("an incomplete snapshot behind a held-open pipe must still time out");
        assert!(err.to_string().contains("did not finish within"));
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "snapshot_with took {:?}, far longer than its {timeout:?} timeout",
            started.elapsed(),
        );

        // Confirms the drain-timeout branch's group kill reached the backgrounded process too —
        // this is exactly the path whose `-pid` validity is subtlest (the group is diagnosed as
        // still populated by this very process holding the pipe open).
        let bg_pid = read_pid_file_when_ready(
            child_pid_path.path(),
            Instant::now() + Duration::from_secs(2),
        );
        assert_gone_by(bg_pid, Instant::now() + Duration::from_secs(2));
    }

    #[test]
    fn a_shell_exiting_cleanly_without_a_complete_snapshot_and_nothing_holding_the_pipes_fails_fast(
    ) {
        // Nothing is backgrounded and no marker is printed: the shell's own exit closes both
        // pipes immediately, so the stdout reader thread disconnects right away with an
        // incomplete buffer. This is the empty process group `kill_group_after_timeout`'s `-pid`
        // reasoning depends on never reaching — waiting out the deadline here would signal a
        // process-group id that may already have been recycled for something unrelated — so it
        // must be reported, fast, without going anywhere near the kill.
        let shell = FakeShell::new("printf 'FOO=ba'");
        let timeout = Duration::from_secs(5);
        let started = Instant::now();
        let err = snapshot_with(shell.path(), timeout).expect_err(
            "an incomplete snapshot with nothing left holding the pipe open must fail fast",
        );
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "should fail as soon as the disconnected reader is noticed, not wait out the \
             {timeout:?} deadline: took {:?}",
            started.elapsed(),
        );
        assert!(
            !err.to_string().contains("did not finish within"),
            "a disconnected reader is a clean-exit truncation, not a timeout: {err}"
        );
    }

    #[test]
    fn a_nul_mid_stream_is_not_mistaken_for_the_end_of_the_dump() {
        // `env -0` emits a NUL after every variable, not only the last one, so a drain landing
        // right after an early one looks exactly like a complete frame to anything that tests
        // only for a trailing NUL. Delaying the rest of the output long enough for an eager drain
        // to land on that first NUL, then finishing afterwards, catches a regression back to
        // treating any trailing NUL as the marker for completion.
        let shell = FakeShell::new(
            "printf 'FOO=bar\\0'\nsleep 1\nprintf 'BAZ=qux\\0'\nprintf '%s' \"$OCTOBOARD_TEST_MARKER\"",
        );
        let env = snapshot_with(shell.path(), Duration::from_secs(5)).expect("snapshot");
        assert_eq!(env.get("FOO"), Some(&"bar".to_string()));
        assert_eq!(env.get("BAZ"), Some(&"qux".to_string()));
    }

    #[test]
    fn a_shell_that_hangs_is_killed_along_with_its_backgrounded_child() {
        // The shell backgrounds a long sleep and records both pids before waiting on it, so the
        // test can confirm after the timeout that killing the shell did not leave the
        // backgrounded process behind — the exact failure mode the process-group kill exists to
        // prevent.
        let shell_pid_path = TempFile::new("shell-pid");
        let child_pid_path = TempFile::new("child-pid");
        let shell = FakeShell::new(&format!(
            "echo $$ > '{shell_pid}'\nsleep 300 &\necho $! > '{child_pid}'\nwait",
            shell_pid = shell_pid_path.path().display(),
            child_pid = child_pid_path.path().display(),
        ));

        // Generous enough for the shell to reliably write both pid files before the timeout
        // fires even under a loaded, fully parallel `cargo test` run (the point of this test is
        // the kill reaching the whole group, not how fast it fires), yet far shorter than the
        // `sleep 300` a bug that ignores the timeout would leave this test hanging on.
        let timeout = Duration::from_secs(2);
        let err = snapshot_with(shell.path(), timeout)
            .expect_err("a wedged shell must be reported, not waited out");
        assert!(err.to_string().contains("did not finish within"));

        // Polled rather than read once: the files are written concurrently with the timeout
        // above firing, and an unflushed file on a loaded machine is this suite's one real
        // wall-clock race.
        let poll_deadline = Instant::now() + timeout;
        let shell_pid = read_pid_file_when_ready(shell_pid_path.path(), poll_deadline);
        let child_pid = read_pid_file_when_ready(child_pid_path.path(), poll_deadline);

        // SIGKILL is near-instant; this is a generous margin on a loaded CI box, not evidence the
        // kill is slow.
        let kill_deadline = Instant::now() + Duration::from_secs(2);
        assert_gone_by(shell_pid, kill_deadline);
        assert_gone_by(child_pid, kill_deadline);
    }

    /// A `Read` impl that returns one real chunk and then an io error, standing in for a pipe
    /// that fails mid-read — a shape a real pipe practically never produces, but `pipe_reader`
    /// is generic over `Read` precisely so this can be exercised without a subprocess at all.
    struct ErroringReader {
        failed: bool,
    }

    impl Read for ErroringReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.failed {
                return Err(io::Error::other("boom"));
            }
            self.failed = true;
            buf[..5].copy_from_slice(b"FOO=1");
            Ok(5)
        }
    }

    #[test]
    fn a_mid_read_error_is_not_swallowed_into_a_truncated_buffer() {
        let (tx, rx) = mpsc::channel();
        pipe_reader(&mut ErroringReader { failed: false }, &tx);

        let mut buf = Vec::new();
        // The real chunk is still appended to `buf` — `drain_available` does not discard data
        // read before the error — but the error itself has to surface rather than be swallowed:
        // silently dropping it would leave `buf` holding only the partial data with nothing to
        // say it stopped short of a complete snapshot.
        let err =
            drain_available(&rx, &mut buf, "stdout").expect_err("a mid-read error must surface");
        // `{:#}` rather than `to_string()`: anyhow's default `Display` prints only the
        // outermost context message, and the `boom` io error is its source.
        assert!(format!("{err:#}").contains("boom"));
        assert_eq!(buf, b"FOO=1");
    }
}
