//! The next daemon of a planned restart that no service manager starts, and what happens when it
//! does not start after an update.
//!
//! By then this daemon has handed its sessions over, let go of its lock and stopped serving its
//! port (which it holds for the next daemon where it can hand it over), so it cannot
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

/// How starting the next daemon went ([`start_successor`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SuccessorStart {
    /// It reported listening.
    Started,
    /// It ended at once refusing the argument that hands it the listener: a build from before
    /// listeners were handed over, which would come up binding the port itself.
    RefusedListener,
    /// It did not come up for any other reason.
    Failed,
}

/// Starts the next daemon with `start` (handed `listener`, the duplicate of this daemon's
/// listening socket, or none), the command line's start of the daemon at `launch`, and falls back
/// to the build that ran when that fails after an update (see the module). Whether a daemon
/// started. Blocking.
pub(crate) fn start_successor(
    launch: &Path,
    listener: Option<crate::platform::listener_handover::Kept>,
    start: impl FnMut(Option<&crate::platform::listener_handover::Kept>) -> SuccessorStart,
) -> bool {
    start_with(listener, start, || {
        let put_back = cfg!(feature = "self-update")
            && proc::self_update_host()
            && fall_back(
                launch,
                &Files::new(&crate::paths::data_dir()),
                &crate::paths::db_path(),
                proc::names_running_image,
            );
        if put_back {
            tracing::warn!(
                launch = %launch.display(),
                "the next daemon did not start; the build that ran was put back and is started \
                 instead, and the one that did not start is kept as the previous build"
            );
        }
        put_back
    })
}

/// [`start_successor`] over the start of one daemon (`start`, handed the listener or none) and the
/// fallback that puts the build that ran back (`fall_back`, whether it did).
///
/// A first start that refused the listener as an argument it does not know is of a build from
/// before listeners were handed over, put at the launch path on purpose (a rollback, or by hand):
/// it is started once more with the listener let go first, binding the port itself, the port free
/// for that moment, and the build that ran is put back only should that fail too. Any other failure
/// keeps the port held: the build that ran, put back, is started with the listener; with nothing
/// put back the start is not repeated, and the port goes only as this daemon ends.
pub(super) fn start_with(
    listener: Option<crate::platform::listener_handover::Kept>,
    mut start: impl FnMut(Option<&crate::platform::listener_handover::Kept>) -> SuccessorStart,
    fall_back: impl FnOnce() -> bool,
) -> bool {
    match start(listener.as_ref()) {
        SuccessorStart::Started => true,
        SuccessorStart::RefusedListener => {
            drop(listener);
            tracing::warn!("the next daemon does not take the listener handed over; starting it once more to bind the port itself");
            start(None) == SuccessorStart::Started
                || (fall_back() && start(None) == SuccessorStart::Started)
        }
        SuccessorStart::Failed => {
            fall_back() && start(listener.as_ref()) == SuccessorStart::Started
        }
    }
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
