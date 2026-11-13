//! The next daemon of a planned restart that no service manager starts, and what happens when it
//! does not start after an update.
//!
//! By then this daemon has handed its sessions over and let go of its lock and port, so it cannot
//! simply carry on: the next daemon is all there is. Where the file at the launch path is another
//! build than this daemon's (an update, or a rollback made under it) and it does not start, while
//! this daemon's build is the one kept beside it as `<name>.previous`, the two are swapped back, as
//! `octoboardd update --rollback` does, and this daemon's build is started instead: the restart
//! ends with a running daemon of the build that ran, and the other is kept as the previous one.
//! Not where the database is at a schema version above this build's (a build that did not start
//! may have migrated it, and this one would stop on it too), nor while another process holds the
//! update lock.

use std::path::Path;

use super::record::Files;
use super::replace::{previous_path, swap_with_previous};
use crate::platform::{fs, proc};

/// Starts the next daemon from `launch` on `port` with `key` (`crate::cli::start_successor`), and
/// falls back to the build that ran when that fails after an update (see the module). Whether a
/// daemon started. Blocking.
pub(crate) fn start_successor(port: u16, key: &str, launch: &Path) -> bool {
    if crate::cli::start_successor(port, key, launch) {
        return true;
    }
    if !cfg!(feature = "self-update") || !proc::self_update_host() {
        return false;
    }
    let files = Files::new(&crate::paths::data_dir());
    if !fall_back(
        launch,
        &files,
        &crate::paths::db_path(),
        proc::names_running_image,
    ) {
        return false;
    }
    tracing::warn!(
        launch = %launch.display(),
        "the next daemon did not start; the build that ran was put back and is started instead, and \
         the one that did not start is kept as the previous build"
    );
    crate::cli::start_successor(port, key, launch)
}

/// Puts the build that runs (`is_running_image`) back at `launch` when it was kept as the
/// previous build of another file there, and the database at `database` is one it reads. Whether
/// it was put back. Blocking.
pub(super) fn fall_back(
    launch: &Path,
    files: &Files,
    database: &Path,
    is_running_image: fn(&Path) -> bool,
) -> bool {
    let Ok(real) = crate::hostfs::canonical(launch) else {
        return false;
    };
    // The build that ran is the one that did not start: there is nothing better to go back to.
    if is_running_image(&real) {
        return false;
    }
    if !previous_path(&real).is_some_and(|previous| is_running_image(&previous)) {
        tracing::warn!(launch = %real.display(), "the next daemon did not start, and the build that ran is not kept beside it to go back to");
        return false;
    }
    // The build to go back to is this one, whose schema version is known exactly.
    let reads = crate::store::Store::SCHEMA_VERSION;
    if let Some(at) = crate::store::Store::peek_schema_version(database).filter(|at| *at > reads) {
        tracing::error!(
            database = at,
            reads,
            "the next daemon did not start, and the database is at a schema version the build that ran does not read, so it is not put back"
        );
        return false;
    }
    let Ok(_lock) = fs::try_lock_file(&files.lock()) else {
        tracing::warn!("the next daemon did not start, and the update lock is held, so the build that ran is not put back");
        return false;
    };
    match swap_with_previous(&real) {
        Ok(()) => {
            files.forget_installed().ok();
            true
        }
        Err(err) => {
            tracing::error!(?err, "putting the build that ran back failed");
            false
        }
    }
}
