//! The processes the daemon starts and keeps track of: running a subprocess within bounds, the
//! agent-launch process ledger, and which ports of a listed tree serve web pages. What the host
//! does to a process (leading a session, signals, the sweep of a session, ending every process on
//! a PTY when the daemon goes down on an internal failure) is `crate::platform::proc`'s, and the
//! system's process table itself is read by `crate::platform::proc_table`.

pub(crate) mod ledger;
pub(crate) mod subprocess;
pub(crate) mod web_ports;
