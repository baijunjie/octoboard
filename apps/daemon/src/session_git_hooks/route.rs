//! Where the two modes the dispatchers start ([`super::ref_mode`], [`super::gate_mode`]) send
//! their request: the launch's own route first, and the daemon's own route where that one is gone.
//!
//! The launch's route is the port the session's environment names, with the launch key from the
//! file the environment names: in a launch a holder runs, the holder, which relays to whichever
//! daemon is linked and holds a question while none is; in one the daemon runs itself, the daemon.
//! A job the session leaves running after it has ended outlives that route: the key file goes with
//! the session's scratch directory once its process is gone, the holder revokes the launch's key a
//! few seconds later and exits once it runs nothing, and a daemon that restarts may listen on
//! another port. So the launch's route counts as gone when the key file is missing, unreadable or
//! empty, when the port refuses the key (401, 403), when nothing listens on it, or when the
//! environment names no port at all; the request then goes to the daemon that runs against the
//! data directory, found through its owner record and its key record
//! ([`crate::owner_record::Owner::recorded_in`]), the same as `octoboardd address` finds it. The
//! daemon answers for any session whose record it still has, ended ones included.
//!
//! A holder saying it has no daemon (503) or lost it (502), a timeout, or any other answer is the
//! daemon not being there, or a fault, and is not tried again elsewhere. The key read from the
//! daemon's key record is sent only to the port its owner record names, never to the one the
//! environment names, which may be anyone's by now.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The routes a mode can take: the launch's, where the environment names a port and a usable key
/// file, and the daemon's, through the records in `data`.
pub(crate) struct Routes {
    /// The launch's port and the key in its key file.
    pub(crate) launch: Option<(u16, String)>,
    /// The data directory of the daemon the daemon's route goes to.
    pub(crate) data: PathBuf,
}

/// What came of a request.
#[derive(Debug)]
pub(crate) enum Outcome {
    /// The body of a success.
    Answered(Vec<u8>),
    /// The route taken failed in a way that is not the launch's route being gone: an error status,
    /// a timeout, a connection that broke.
    Failed(std::io::Error),
    /// Neither route could be used, and nothing answered the request.
    Unrouted,
}

/// How long each route's round trip may take.
#[derive(Clone, Copy)]
pub(crate) struct Deadlines {
    pub(crate) launch: Duration,
    pub(crate) daemon: Duration,
}

impl Routes {
    /// The routes of the session whose environment this process inherited, with `port` from the
    /// dispatcher's arguments and the daemon's route through `data`.
    pub(crate) fn from_environment(port: Option<u16>, data: &Path) -> Self {
        Self {
            launch: port.zip(super::key_from_environment()),
            data: data.to_path_buf(),
        }
    }

    /// Posts `body` to `path` (a daemon helper route, the same on the holder's port and on the
    /// daemon's) by the first route that is there.
    pub(crate) fn post(&self, path: &str, body: &[u8], deadlines: Deadlines) -> Outcome {
        if let Some((port, key)) = &self.launch {
            match crate::loopback::post(*port, key, path, body, deadlines.launch) {
                Ok(reply) => return Outcome::Answered(reply),
                Err(err) if !launch_route_gone(&err) => return Outcome::Failed(err),
                Err(_) => {}
            }
        }
        let Some((port, key)) = self.daemon() else {
            return Outcome::Unrouted;
        };
        match crate::loopback::post(port, &key, path, body, deadlines.daemon) {
            Ok(reply) => Outcome::Answered(reply),
            Err(err) => Outcome::Failed(err),
        }
    }

    /// The port and key of the daemon running against the data directory, when its records name a
    /// process that is still running and hold its key.
    fn daemon(&self) -> Option<(u16, String)> {
        let owner = crate::owner_record::Owner::recorded_in(&self.data)?;
        let key = owner.recorded_key_in(&self.data)?;
        Some((owner.port, key))
    }
}

/// Whether `err`, from the launch's route, says that route is gone rather than the daemon: the key
/// refused, or nothing listening on the port.
fn launch_route_gone(err: &std::io::Error) -> bool {
    matches!(crate::loopback::error_status(err), Some(401 | 403))
        || err.kind() == ErrorKind::ConnectionRefused
}
