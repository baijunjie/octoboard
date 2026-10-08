//! Fixtures the daemon's tests share: a scratch directory that removes itself, an `AppState` over
//! a store in one, and a stand-in live session on a PTY that ends itself. Each is a guard, so a
//! test that fails part-way cleans up as surely as one that passes. Compiled only for tests, and
//! one module rather than a `tests/` directory because the fixtures reach private items
//! (`LiveSession::new`, `AppState::new`).

use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::protocol::Agent;
use crate::session::{LiveSession, NewSession};
use crate::state::AppState;
use crate::store::Store;

/// How long a test waits for something a spawned shell does: a snapshot it should produce, a file
/// it should write, a process it should end. Not a measure of speed: on a machine whose
/// endpoint-security software stalls every `exec` for seconds at a time under a parallel burst, a
/// shell can take well over ten seconds to start, and a wait returns as soon as its condition
/// holds, so a generous value costs a passing test nothing.
pub(crate) const PATIENCE: Duration = Duration::from_secs(30);

/// A scratch directory for one test, removed when whatever holds it is dropped. Dereferences to
/// the path, so it is passed as one.
///
/// The name is `octoboardd-<label>-<pid>-<counter>`: the label attributes a leftover directory to
/// the test that made it, and the counter (not a thread id, which is reused within a process)
/// keeps two directories of one run apart. Across runs a pid can be recycled, which is why the
/// directory is also removed before being created: a run killed outright leaves its directory
/// behind, and a leftover file in it (a SQLite database, say) would be picked up by the next run
/// that reuses the name.
///
/// Bind it to a named local: a temporary guard, such as one passed straight into `Store::open`,
/// removes the directory at the end of that statement.
#[derive(Debug)]
pub(crate) struct ScratchDir(PathBuf);

impl ScratchDir {
    pub(crate) fn new(label: &str) -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "octoboardd-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("temporary directory");
        Self(dir)
    }
}

impl Deref for ScratchDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for ScratchDir {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

/// The path of one file inside a scratch directory that goes away with it. Dereferences to the
/// path, so it is passed as one; the file itself is not created.
#[derive(Debug)]
pub(crate) struct ScratchFile {
    _dir: ScratchDir,
    path: PathBuf,
}

impl ScratchFile {
    pub(crate) fn new(label: &str, file_name: &str) -> Self {
        let dir = ScratchDir::new(label);
        Self {
            path: dir.join(file_name),
            _dir: dir,
        }
    }
}

impl Deref for ScratchFile {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for ScratchFile {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

/// An `AppState` over a fresh store in a scratch directory labelled `label`, with no console in
/// it. The caller keeps the directory alive for as long as the state is used. A console added to
/// the state needs a workdir of its own rather than this directory, because deleting a console
/// removes its workdir.
pub(crate) fn app_state(label: &str) -> (Arc<AppState>, ScratchDir) {
    let dir = ScratchDir::new(label);
    let store = Store::open(&dir.join("octoboard.db")).expect("store");
    (
        Arc::new(AppState::new(store, 1234, "/opt/octoboardd".to_string())),
        dir,
    )
}

/// A stand-in live session that stays alive on a PTY (`sleep 30`) with nothing reading it.
pub(crate) fn idle_stand_in(id: &str) -> StandIn {
    fake_live_session(id, Agent::Claude, 80, 24, "sleep 30")
}

/// A stand-in live session on a real PTY, with `/bin/sh` running `script` behind it instead of a
/// real agent: enough for whatever a test needs a `LiveSession` to write into, read from, or just
/// stay alive on. `LiveSession` has no `Drop` that ends its process, so this guard does: the
/// process is terminated when the guard drops, which a test that fails part-way reaches too.
/// Dereferences to the `Arc<LiveSession>`, so `live.clone()` registers it with an `AppState`.
/// Declare it after any `ScratchDir` its script uses, so the process ends before the directory
/// goes.
pub(crate) struct StandIn(Arc<LiveSession>);

impl Deref for StandIn {
    type Target = Arc<LiveSession>;

    fn deref(&self) -> &Arc<LiveSession> {
        &self.0
    }
}

impl Drop for StandIn {
    fn drop(&mut self) {
        self.0.terminate();
    }
}

pub(crate) fn fake_live_session(
    id: &str,
    agent: Agent,
    cols: u16,
    rows: u16,
    script: &str,
) -> StandIn {
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};

    let pty = native_pty_system()
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("a PTY");
    let mut cmd = CommandBuilder::new("/bin/sh");
    cmd.args(["-c", script]);
    let child = pty.slave.spawn_command(cmd).expect("the stand-in");
    let pid = child.process_id().expect("a pid");
    let fd = pty.master.as_raw_fd().expect("a descriptor");
    crate::ptyio::set_nonblocking(fd).expect("non-blocking");
    StandIn(Arc::new(LiveSession::new(NewSession {
        id: id.to_string(),
        agent,
        pid,
        fd,
        master: pty.master,
        child,
        scratch_dir: None,
        resolves_approvals_itself: false,
    })))
}
