//! What the daemon asks of the host about the terminals it runs processes on: starting a process
//! on a pseudo-terminal of its own, reading what it prints, writing into its input, resizing it,
//! hanging it up, and whose job its foreground is. The daemon's own writes are semantic inputs
//! ([`Input`]: a paste, a key) that this module encodes the way the host's terminal takes them; a
//! client's keystrokes and its replies to the program's queries are written as they came. Where a
//! backend behaves differently in a way business code must branch on, it says so in
//! [`TermFacts`].
//!
//! What the process is, who reads its output, when it is stopped and with what grace, and how its
//! input is serialised against the screen it draws are for the callers to decide; this module only
//! does what it is asked. The types it hands out are plain values, a command, a size and a list of
//! inputs, so that the terminal can later be held by another process and driven over a channel.

use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::proc::{ChildHandle, ProcessId, SetEnv};

mod vt;

#[cfg(test)]
mod contract_tests;

#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub(crate) use unix::*;

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub(crate) use windows::*;

/// The size of a terminal, in character cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub rows: u16,
    pub cols: u16,
}

/// How the terminal backend that starts this host's pseudoconsoles behaves where business code has
/// to know. Where the OS ships more than one host for them, these are the facts of the one in use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TermFacts {
    /// What the program prints reaches the reader as a host's repaint of the screen rather than as
    /// the program's own bytes, so only the parsed screen says what is drawn, not the order of the
    /// output, and the host may set titles and drop attributes and modes of its own accord. The
    /// trust screen, the titles, the input box and the agent's readiness are then read off the
    /// parsed screen, its title changes and the agent's hooks.
    pub host_renders: bool,
    /// The host answers the program's terminal queries (device attributes, status and cursor
    /// reports) itself, before any client could.
    // TODO(multi-client-terminal 04): the reply routing consults this once the daemon answers
    // every query.
    #[allow(dead_code, reason = "no consumer until the daemon answers every query")]
    pub host_answers_queries: bool,
    /// The host forwards the program's terminal queries to the output and holds the console until
    /// they are answered, about half a second for each, so a query nobody answers stalls the
    /// program and what it was about to read. The daemon then answers the device attributes and
    /// the cursor position itself while no client is attached to do it.
    pub host_waits_on_queries: bool,
    /// The host takes a bracketed paste's markers itself and hands a program that reads its input
    /// as key presses (a console's input records) the pasted text as typed keys, and an Enter
    /// written right behind it as one more key in the same burst. A program that tells a paste by
    /// how fast its keys arrive then reads that Enter as a line break inside the paste.
    pub paste_as_keys: bool,
    /// [`Pty::foreground_job`] can tell whose job the terminal's foreground is.
    #[allow(
        dead_code,
        reason = "the busy check acts on `Foreground::Unsupported` itself; the contract tests read this"
    )]
    pub foreground_job: bool,
    /// How long the output has to stay silent for the program to count as having finished
    /// drawing.
    pub settle_window: Duration,
}

/// The facts of the terminal backend this host's pseudoconsoles are started by.
pub fn facts() -> TermFacts {
    host_facts()
}

/// What the host writes to a terminal's output of its own accord before anything the process
/// prints, so that a test can read what the process printed from its start.
#[cfg(test)]
pub(crate) fn host_preamble() -> &'static [u8] {
    host_output_preamble()
}

/// A program to start on a terminal, with its arguments, environment and working directory. Plain
/// data: it says what to start, and [`spawn`] starts it.
#[derive(Debug, Clone)]
pub struct Command {
    program: OsString,
    args: Vec<OsString>,
    /// Whether the process starts over the daemon's own environment, with `env` on top of it, or
    /// over `env` alone.
    inherits_env: bool,
    env: Vec<(OsString, OsString)>,
    cwd: Option<PathBuf>,
    #[cfg_attr(
        unix,
        allow(
            dead_code,
            reason = "a Unix host's polite request is a signal the program takes"
        )
    )]
    quit_keys: Option<QuitKeys>,
}

impl Command {
    /// `program`, with no arguments, over the daemon's environment, in the daemon's directory.
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        Self {
            program: program.as_ref().to_os_string(),
            args: Vec::new(),
            inherits_env: true,
            env: Vec::new(),
            cwd: None,
            quit_keys: None,
        }
    }

    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.args
            .extend(args.into_iter().map(|arg| arg.as_ref().to_os_string()));
        self
    }

    /// Starts the process over no environment but what [`Self::env`] sets afterwards.
    pub fn env_clear(&mut self) -> &mut Self {
        self.inherits_env = false;
        self.env.clear();
        self
    }

    pub fn env(&mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> &mut Self {
        self.env
            .push((key.as_ref().to_os_string(), value.as_ref().to_os_string()));
        self
    }

    pub fn cwd(&mut self, dir: impl AsRef<Path>) -> &mut Self {
        self.cwd = Some(dir.as_ref().to_path_buf());
        self
    }

    /// Has a polite request to terminate the process press `keys` first, where the host's own
    /// request is one the program would die on without its shutdown work. A host whose request
    /// the program takes as a request to finish (a signal, on Unix) does without them.
    pub fn quit_keys(&mut self, keys: QuitKeys) -> &mut Self {
        self.quit_keys = Some(keys);
        self
    }
}

/// The keys a program takes as a request to quit: `key` pressed `presses` times, `apart` from each
/// other, after which the program has `wait` to finish before the host's own request to end
/// follows. A program that quits on the first press is not waited on any further.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuitKeys {
    pub key: &'static [Input],
    pub presses: u32,
    pub apart: Duration,
    pub wait: Duration,
}

impl QuitKeys {
    /// How long pressing the keys and waiting on the program takes at most.
    #[cfg_attr(unix, allow(dead_code, reason = "a Unix host presses no quit keys"))]
    pub(crate) fn lead(&self) -> Duration {
        self.apart * self.presses.saturating_sub(1) + self.wait
    }
}

impl SetEnv for Command {
    fn set_env(&mut self, key: &str, value: &str) {
        self.env(key, value);
    }
}

/// A process just started on a terminal of its own, by [`spawn`].
pub struct Spawned {
    /// The process, which the caller now holds unreaped.
    pub child: ChildHandle,
    pub pid: ProcessId,
    pub pty: Pty,
}

/// What the daemon itself writes into a terminal, by meaning; [`Pty::write_input`] encodes it as
/// the host's terminal takes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// Text pasted in one piece, which a program that asked for bracketed paste reads as a paste
    /// rather than as typing. The caller strips what must not reach the program, the paste's own
    /// end marker included.
    Paste(String),
    /// The Enter key: what submits a line.
    Enter,
    /// The Escape key.
    Escape,
    /// A letter (or one of `@[\]^_`) pressed with Control.
    Ctrl(char),
    /// The Down arrow key.
    Down,
    /// Text typed as it is.
    Text(String),
}

/// The bytes `input` is written as on this host.
pub(crate) fn encode(input: &[Input]) -> Vec<u8> {
    vt::encode(input)
}

/// A write that did not finish, and how far it got. The count matters to a caller deciding whether
/// to retry: the bytes already accepted are in the child's input buffer, so writing the whole
/// message again would deliver that prefix twice. It counts the bytes written, which for
/// [`Pty::write_input`] are those of the encoded inputs.
#[derive(Debug)]
pub struct PartialWrite {
    pub written: usize,
    pub error: io::Error,
}

impl std::fmt::Display for PartialWrite {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{} ({} bytes written)", self.error, self.written)
    }
}

impl std::error::Error for PartialWrite {}

/// The error of a write into a terminal that has been hung up.
fn hung_up() -> PartialWrite {
    PartialWrite {
        written: 0,
        error: io::Error::new(io::ErrorKind::BrokenPipe, "the terminal has been hung up"),
    }
}

/// Whose job the terminal's foreground is, as [`Pty::foreground_job`] finds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    windows,
    allow(
        dead_code,
        reason = "a Windows console has no foreground job to tell apart from its leader"
    )
)]
pub enum Foreground {
    /// The process that leads the terminal's session: the program is at its own prompt.
    Own,
    /// A job the process started in the foreground.
    Other,
    /// The host cannot say.
    Unsupported,
}
