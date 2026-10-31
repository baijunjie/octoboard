//! The text frames of `/ws/term/:session`. See "`GET /ws/term/:session` — terminal stream" in
//! `apps/daemon/PROTOCOL.md`.

use serde::{Deserialize, Serialize};

/// Client-sent text frame on `/ws/term/:session`. Binary frames on that socket are raw PTY input
/// and never reach this type.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TermControl {
    Resize {
        cols: u16,
        rows: u16,
    },
    /// The client's terminal answering a query the program wrote (the cursor position, a colour,
    /// the device attributes), as opposed to the user's input. `offset` is the end of the output
    /// frame whose parsing produced it, counted in bytes of the program's output (see
    /// [`TermEvent::Replay`]); only the first client's answers to a point are written, and answers
    /// to an earlier point are dropped.
    Reply {
        data: String,
        offset: u64,
    },
}

/// Daemon-sent text frame on `/ws/term/:session`. Binary frames on that socket are raw PTY output.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TermEvent {
    /// Sent first on attaching to a live process, before the replay: the replay's length in bytes
    /// (no replay frame follows when it is 0), the count of bytes of output the program had written
    /// when it was taken, which live output counts on from, and how many of its leading bytes hold
    /// only output whose queries have already been answered.
    Replay {
        length: usize,
        offset: u64,
        answered: usize,
    },
}
