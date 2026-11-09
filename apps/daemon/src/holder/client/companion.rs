//! A terminal's companion in a holder, as the daemon holds it: whether it still runs, as the link
//! learns it, and its end while no terminal has bound it. The daemon reaches what the companion
//! serves at its endpoint directly, not through this.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use anyhow::Result;

use super::link::{CompanionRoute, Link};
use crate::platform::proc::{ProcessId, ProcessIdentity};

/// A companion a holder runs for a terminal of a launch (one this daemon started, or one it found
/// running). It reads as exited once the holder reports that it exited by itself
/// ([`crate::holder::protocol::FromHolder::CompanionEnded`]), once a welcome lists it exited or
/// no longer lists it, once it was ended through [`Self::end`] or its terminal was released, and
/// once the link to its holder is lost for good (the holder is gone). A link that is only pending
/// leaves it as it was: the holder enforces the companion's lifetime by itself meanwhile. A
/// companion handed over to the next daemon, or left to it by a planned parting, never reads as
/// exited here. Dropping this does nothing to the companion.
pub struct HeldCompanion {
    link: Arc<Link>,
    /// The terminal id the companion runs under.
    id: String,
    route: Arc<CompanionRoute>,
    pid: ProcessId,
    identity: Option<ProcessIdentity>,
}

impl HeldCompanion {
    pub(super) fn new(
        link: Arc<Link>,
        id: String,
        route: Arc<CompanionRoute>,
        pid: u32,
        identity: Option<&str>,
    ) -> Self {
        Self {
            link,
            id,
            route,
            pid: ProcessId(pid),
            identity: identity.and_then(ProcessIdentity::parse),
        }
    }

    /// The companion's process, in its holder's view of the system.
    #[allow(
        dead_code,
        reason = "nothing in the daemon names a companion's process yet"
    )]
    pub fn pid(&self) -> ProcessId {
        self.pid
    }

    /// The companion's process identity, when the holder could read its start time.
    #[allow(
        dead_code,
        reason = "nothing in the daemon names a companion's process yet"
    )]
    pub fn identity(&self) -> Option<ProcessIdentity> {
        self.identity
    }

    /// Whether the companion is known to no longer run (see the type's rules). Never waits.
    pub fn exited(&self) -> bool {
        self.route.exited.load(Ordering::Acquire)
    }

    /// Ends the companion while no terminal has bound it (a resume's first server, a launch that
    /// falls back to a terminal of its own), removing its endpoint, and waits for the holder to
    /// have done so. A companion its terminal has bound ends with the terminal: the holder refuses
    /// and this fails, the companion left running. Fails too when the holder cannot be reached;
    /// the holder ends an unbound companion with the connection it was asked over in any case.
    /// **Blocks**, for up to the companion's grace.
    pub fn end(&self) -> Result<()> {
        self.link.end_companion(&self.id)
    }
}
