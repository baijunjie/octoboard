//! Processes on a PTY and what clients see of them: the PTY core an agent session and a shell
//! terminal share, its replay ring and non-blocking I/O, terminal snapshots and saved previews, the
//! OSC title observer, which client's answer to a terminal query reaches the program, and the
//! shell terminals themselves.

pub(crate) mod fan;
pub(crate) mod pty;
pub(crate) mod ptyio;
pub(crate) mod reply_route;
pub(crate) mod ringbuf;
pub(crate) mod saved_output;
pub(crate) mod shell;
pub(crate) mod snapshot;
pub(crate) mod title;
