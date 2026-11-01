//! The processes the daemon starts and keeps track of: running a subprocess within bounds, a
//! process leading a session of its own, the agent-launch process ledger and the client requests on
//! it, listing every process id on the system, and ending every process on a PTY when the daemon
//! goes down on an internal failure.

pub(crate) mod crash_cleanup;
pub(crate) mod leader;
pub(crate) mod ledger;
pub(crate) mod pids;
pub(crate) mod requests;
pub(crate) mod subprocess;
