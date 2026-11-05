//! Processes on a PTY and what clients see of them: the PTY core an agent session and a shell
//! terminal share, its replay ring, terminal snapshots and saved previews, the
//! OSC title observer, escape sequences stripped from output (`plain_text`), which client's answer
//! to a terminal query reaches the program (`reply_route`) and how long the queries an agent's host
//! waits on took to be answered (`query_log`), a chunk of input that is only a
//! terminal's own protocol traffic (`input_class`), and the shell terminals themselves.

pub(crate) mod fan;
pub(crate) mod input_class;
pub(crate) mod plain_text;
pub(crate) mod pty;
pub(crate) mod query_log;
pub(crate) mod reply_route;
pub(crate) mod ringbuf;
pub(crate) mod saved_output;
pub(crate) mod shell;
pub(crate) mod snapshot;
pub(crate) mod title;
