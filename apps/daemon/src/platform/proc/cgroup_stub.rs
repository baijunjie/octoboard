//! What stands for cgroup v2 groups on a Unix host that has none: nothing, as no value of
//! [`Group`] or [`Subtree`] can be made. The same surface as `cgroup.rs`, so that `unix.rs` is the
//! same on every Unix host.

use std::io;

use super::{ProcessId, ProcessIdentity};

/// Where groups would be made; there is no such place here.
#[derive(Debug, Clone)]
pub(super) enum Subtree {}

impl Subtree {
    pub(super) fn below_own() -> Option<Self> {
        None
    }
}

#[derive(Debug)]
pub(super) enum Group {}

impl Group {
    pub(super) fn create(base: &Subtree, _instance: &str, _launch: &str) -> Option<Self> {
        match *base {}
    }

    pub(super) fn open(base: &Subtree, _instance: &str, _launch: &str) -> Option<Self> {
        match *base {}
    }

    pub(super) fn join(base: &Subtree, _instance: &str, _launch: &str) -> Option<Self> {
        match *base {}
    }

    pub(super) fn leftovers(base: &Subtree, _instance: &str) -> Vec<String> {
        match *base {}
    }

    pub(super) fn members(&self) -> Option<Vec<ProcessId>> {
        match *self {}
    }

    pub(super) fn adopt_child(&self, _pid: ProcessId) -> io::Result<()> {
        match *self {}
    }

    pub(super) fn adopt_running(&self, _ids: &[ProcessIdentity]) {
        match *self {}
    }

    pub(super) fn release(&self, _ids: &[ProcessIdentity], _into: Option<&Group>) -> bool {
        match *self {}
    }

    pub(super) fn end(&self, _spared: &[(ProcessIdentity, Option<&Group>)]) -> bool {
        match *self {}
    }
}
