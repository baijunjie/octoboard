//! Capturing the user's real shell environment, which every agent is then launched with, and
//! resolving a binary on its `PATH`. Running a subprocess the daemon spawns directly, within a
//! deadline and output bounds, is `crate::processes::subprocess`'s.
//!
//! - `snapshot.rs` takes one: the environment the host's login shell gives (`platform::shell`), less
//!   what is not the user's. Slow, so nothing here takes one per call that can avoid it.
//! - `cache.rs` keeps the last one and decides who may be served it, under one policy for an agent
//!   launch and another for callers on someone else's schedule.
//! - `resolve.rs` finds a binary on a snapshot's `PATH`, as what starts it.
//!
//! This file is the module's face: callers name these functions, not the files.

mod cache;
mod resolve;
mod snapshot;

use std::collections::HashMap;

use anyhow::Result;

use crate::protocol::{error_code, CodedError};

pub use resolve::{resolve_binary, resolve_program};
pub use snapshot::is_filtered;

static CACHE: cache::Cache = cache::Cache::new();

/// A fresh snapshot of the user's login shell, waited for, which also becomes what the cached
/// readers below serve; see [`cache::Cache::refresh`]. The daemon's own start-up snapshot goes
/// through this, so the first launch after it does not pay for a login shell either.
pub fn snapshot() -> Result<HashMap<String, String>> {
    CACHE.refresh(snapshot::take)
}

/// The environment an agent launch runs with; see [`cache::Cache::launch`].
pub fn launch_snapshot() -> Result<HashMap<String, String>> {
    CACHE.launch(snapshot::take)
}

/// The environment for a caller paced by something other than a person, such as a periodic sweep;
/// see [`cache::Cache::scheduled`].
pub fn cached_snapshot() -> Result<HashMap<String, String>> {
    CACHE.scheduled(snapshot::take)
}

/// The refusal for a request that needs the shell environment and could not take it:
/// `shell_environment_timeout`, the cause in `detail`.
pub fn capture_failed(err: anyhow::Error) -> anyhow::Error {
    let detail = format!("{err:#}");
    CodedError::raised(
        error_code::SHELL_ENVIRONMENT_TIMEOUT,
        format!("The shell environment could not be captured: {detail}"),
        &[("detail", &detail)],
    )
}
