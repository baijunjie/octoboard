//! Fixtures the daemon's tests share, each a guard or a plain builder, so a test that fails
//! part-way cleans up as surely as one that passes. Compiled only for tests, and one module
//! rather than a `tests/` directory because some of the fixtures reach private items
//! (`LiveSession::new`, `AppState::new`). The submodules are re-exported here, so a caller names
//! a fixture as `test_support::X`:
//!
//! - [`scratch`]: a scratch directory that removes itself, and the files and links made in one.
//! - [`app`]: an `AppState` over a store in one, the host tools and environment, and the records
//!   it seeds.
//! - [`stand_in`]: a stand-in live session on a PTY that ends itself, the fake agent CLIs, and
//!   the reads of what reached them.
//! - [`background`]: the commands a stand-in agent starts in the background, spelled for each host.
//! - [`processes`]: the waits on a fixture process a test spawned (the pid it recorded, running a
//!   scenario again when it was killed before recording one, and its being gone) and the wait for
//!   a condition that runs off the async runtime.
//! - [`clock`]: the wait for every task on the runtime to be idle through tokio's paused clock,
//!   `runtime_settled` and `runtime_advanced`, which pause a real-clock test's clock for the wait, the
//!   latter moving it on by a given duration, and `next_millisecond`, which waits for the wall-clock
//!   millisecond to change.
//! - `cgroups` (Linux): a cgroup of a test's own below the test process's, for the tests of what
//!   runs in a cgroup other than the daemon's, and the launch group and cgroup reads of the tests of
//!   what runs in a launch's group; named as `cgroups::ChildCgroup`, not re-exported.
//! - `fixture` (Windows): the processes a test starts where a Unix test has perl start them.
//! - [`closed_port`]: a loopback port that refuses connections.
//! - [`git`]: the environment a test's `git` runs in together with the repositories it builds
//!   with it, the state a read must leave untouched and the stale stat that makes a read want to
//!   write.
//! - [`programs`]: the small programs a test starts (sleep, echo, hold stdin, ignore the hang-up,
//!   print a variable), spelled for each host; named as `programs::sleep`, not re-exported.
//! - [`sessions`]: session records, Codex rollout chains and report pages.
//! - [`owner_team`]: an owner with sessions bound to it, for the tests of what is written into
//!   an owner.

use std::time::Duration;

pub(crate) mod app;
pub(crate) mod background;
#[cfg(target_os = "linux")]
pub(crate) mod cgroups;
pub(crate) mod clock;
pub(crate) mod closed_port;
#[cfg(windows)]
pub(crate) mod fixture;
pub(crate) mod git;
pub(crate) mod owner_team;
pub(crate) mod processes;
pub(crate) mod programs;
pub(crate) mod scratch;
pub(crate) mod sessions;
pub(crate) mod stand_in;

pub(crate) use app::*;
pub(crate) use background::Background;
pub(crate) use clock::*;
pub(crate) use closed_port::ClosedPort;
pub(crate) use git::*;
pub(crate) use processes::*;
pub(crate) use scratch::*;
pub(crate) use sessions::*;
pub(crate) use stand_in::*;

/// `unix_path`, an absolute path spelled the Unix way (`/home/u/x`) that a test only names and
/// never touches, as an absolute path of this host: on Windows on drive `C:`, with backslashes.
pub(crate) fn host_path(unix_path: &str) -> String {
    if cfg!(windows) {
        format!("C:{}", unix_path.replace('/', "\\"))
    } else {
        unix_path.to_string()
    }
}

/// The root of the disk the temporary directory is on: `/`, or a drive's root such as `C:\`.
pub(crate) fn disk_root() -> std::path::PathBuf {
    let temp = std::env::temp_dir();
    temp.ancestors().last().unwrap_or(&temp).to_path_buf()
}

/// Whether this host can create the relative path `path` (segments separated by `/`): Windows
/// forbids names a Unix file system takes, such as ones holding `:`, `*`, `"` or a newline.
pub(crate) fn creatable(path: &str) -> bool {
    path.split('/')
        .all(|segment| crate::platform::names::component_is_valid(segment.as_bytes()))
}

/// A shell environment whose `PATH`, under `dir`, finds Codex only as an npm shim (`codex.cmd`)
/// whose package ships no native program.
#[cfg(windows)]
pub(crate) fn codex_shim_without_native_program(
    dir: &std::path::Path,
) -> std::collections::HashMap<String, String> {
    let bin = dir.join("bin");
    std::fs::create_dir_all(bin.join("node_modules/@openai/codex")).unwrap();
    std::fs::write(bin.join("codex.cmd"), "").unwrap();
    std::fs::write(bin.join("node_modules/@openai/codex/package.json"), "{}").unwrap();
    std::collections::HashMap::from([
        ("PATH".to_owned(), bin.display().to_string()),
        ("PATHEXT".to_owned(), ".CMD".to_owned()),
    ])
}

/// The code of the coded error `err` is, which a test that something is refused asserts beside the
/// message.
pub(crate) fn code_of(err: &anyhow::Error) -> crate::protocol::Code {
    err.downcast_ref::<crate::protocol::CodedError>()
        .unwrap_or_else(|| panic!("a coded error: {err:#}"))
        .code
}

/// The value of param `name` of the coded error `err` is, such as the `stage` or `cause` of a
/// failed relocation.
pub(crate) fn param_of(err: &anyhow::Error, name: &str) -> String {
    err.downcast_ref::<crate::protocol::CodedError>()
        .unwrap_or_else(|| panic!("a coded error: {err:#}"))
        .params
        .get(name)
        .cloned()
        .unwrap_or_else(|| panic!("a `{name}` param: {err:#}"))
}

/// How long a test waits for something a spawned shell does: a snapshot it should produce, a file
/// it should write, a process it should end. Not a measure of speed: on a machine whose
/// endpoint-security software stalls every `exec` for seconds at a time under a parallel burst, a
/// shell can take well over ten seconds to start, and a wait returns as soon as its condition
/// holds, so a generous value costs a passing test nothing.
pub(crate) const PATIENCE: Duration = Duration::from_secs(30);
