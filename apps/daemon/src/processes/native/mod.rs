//! Native reads of the system's process table, without starting a process: which processes there
//! are and how they descend from one another, and on request what one of them runs, where, with
//! how much memory, on which descriptors and on which listening ports. Each platform's reader is a
//! module of its own under the same names; the types here are shared.
//!
//! What a read means — which process is which across reads, which belong to whom — is for the
//! callers to decide: these readers only report what the system says at the moment of the call.

use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "linux")]
pub use linux::*;
#[cfg(target_os = "macos")]
pub use macos::*;

/// One process as a listing of the table found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcInfo {
    pub pid: i32,
    /// When the process started, in microseconds on the clock [`now`] reads.
    pub start: u64,
    pub ppid: i32,
    pub sid: i32,
    /// The name of the program it runs, as the system keeps it (cut to 15 or 16 bytes).
    pub comm: String,
}

/// A process's working directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cwd {
    pub path: PathBuf,
    /// The directory was removed. `false` also when that could not be told.
    pub missing: bool,
}

impl CwdReport {
    /// Whether the directory may lie under `root`, from the reported path alone: what decides
    /// whether it is worth looking at the directory itself ([`CwdReport::looked`]).
    pub fn may_lie_under(&self, root: &Path) -> bool {
        self.candidates().any(|path| path.starts_with(root))
    }
}
