//! The text frames of `/ws/term/:session`. See "`GET /ws/term/:session` — terminal stream" in
//! `apps/daemon/PROTOCOL.md`.

use serde::{Deserialize, Serialize};

/// Client-sent text frame on `/ws/term/:session`. Binary frames on that socket are raw PTY input
/// and never reach this type.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TermControl {
    Resize {
        cols: u16,
        rows: u16,
    },
    /// The client's terminal answering a query the program wrote that the daemon passed through,
    /// as opposed to the user's input. `offset` is the end of the output frame whose parsing
    /// produced it, counted in bytes of the output the client received (see
    /// [`TermEvent::Replay`]); only the first client's answers to a point are written, and answers
    /// to an earlier point are dropped.
    Reply {
        data: String,
        offset: u64,
    },
    /// What the client's terminal shows, which the daemon answers the program's colour and size
    /// queries with while this client is the most recently active one: colours as `#rrggbb`, and
    /// a cell's size in whole pixels where the client knows it. Sent on attaching and whenever it
    /// changes.
    Display {
        foreground: String,
        background: String,
        cursor: String,
        #[cfg_attr(test, ts(optional))]
        cell_width: Option<u16>,
        #[cfg_attr(test, ts(optional))]
        cell_height: Option<u16>,
    },
    /// The client's terminal has consumed `bytes` more of the binary frames it was sent, which
    /// gives the daemon that much credit to send more; see the terminal stream's flow control.
    Ack {
        bytes: u64,
    },
    /// The person turned to this client's terminal: selected it, pressed on it, came back to its
    /// window while it was shown, or resized the window they work in. Makes the client the most
    /// recently active one, as input does, so the terminal takes its size and the program's queries
    /// its display. Not sent for a reconnect in the background or a size change in a window in the
    /// background.
    Activate,
}

/// Daemon-sent text frame on `/ws/term/:session`. Binary frames on that socket are raw PTY output.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TermEvent {
    /// Sent first on attaching to a live process, before the first paint, and again before each
    /// fresh paint that replaces output the client fell too far behind on: the paint's length in
    /// bytes, the count of bytes of output handed to the clients when it was taken, which live
    /// output counts on from, and how many of its leading bytes hold only output whose queries are
    /// not the client's to answer: all of them, since the paint asks nothing.
    Replay {
        length: usize,
        offset: u64,
        answered: usize,
    },
}
