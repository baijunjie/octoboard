//! Where a terminal's PTY and the program on it live: in this process, as `platform::term`
//! starts it ([`Backend::Local`], what the tests' stand-ins use), or in a holder
//! ([`Backend::Held`], what the daemon's own terminals use once [`crate::holder::client::install`]
//! has run). [`PtyCore`](super::pty::PtyCore) drives either through the same calls.

use std::io;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::holder::client::{self, HeldAt, HeldReader, HeldTerminal, Issued};
use crate::holder::protocol::{ContainmentSpec, SpawnSpec};
use crate::platform::proc::{
    crash, ChildHandle, Containment, Polite, ProcessId, ProcessIdentity, Sent, Swept,
};
use crate::platform::term::{
    self, Command, Foreground, Input, PartialWrite, Pty, PtyReader, Size, Spawned,
};

/// A terminal just started, before a [`PtyCore`](super::pty::PtyCore) is built on it.
pub enum Started {
    Local(Spawned),
    Held(HeldTerminal),
}

impl From<Spawned> for Started {
    fn from(spawned: Spawned) -> Self {
        Self::Local(spawned)
    }
}

/// What the helpers of a launch (the hook script, the MCP server, the status line, the git hooks)
/// are given to call: where, with which key, and the executable they run.
///
/// A launch in a holder is given the holder's port, a key the holder issued for this launch and
/// the executable the holder runs from, and the program has to be started through the same
/// `Helpers` ([`Launching::helpers`]). Where the terminals run in this process, it is the daemon's
/// own port, key and executable.
pub struct Helpers {
    pub port: u16,
    pub key: String,
    pub exe: String,
    issued: Option<Issued>,
}

impl Helpers {
    /// The credentials for a launch of the session `session_id` with the MCP token `mcp_token`:
    /// the holder's, which binds the key to the token, when the daemon runs its terminals in one,
    /// and `local` otherwise. **Blocks**, as [`start`] does.
    pub fn issue(session_id: &str, mcp_token: &str, local: LocalHelpers<'_>) -> Result<Self> {
        let Some(holders) = client::current() else {
            return Ok(Self {
                port: local.port,
                key: local.key.to_string(),
                exe: local.exe.to_string(),
                issued: None,
            });
        };
        let issued = holders
            .issue_launch(session_id, Some(mcp_token))
            .context("getting the launch's credentials from the holder")?;
        Ok(Self {
            port: issued.port,
            key: issued.key.clone(),
            exe: issued.exe.clone(),
            issued: Some(issued),
        })
    }
}

/// The daemon's own address, key and executable, which the helpers of a program that runs in this
/// process call.
#[derive(Clone, Copy)]
pub struct LocalHelpers<'a> {
    pub port: u16,
    pub key: &'a str,
    pub exe: &'a str,
}

/// A program to start on a terminal of its own, with what a holder needs to start it.
pub struct Launching<'a> {
    pub command: Command,
    pub size: Size,
    /// The launch's containment, which the program is started into.
    pub containment: &'a Containment,
    /// The names `containment` was made from, which a holder opens it again by.
    pub instance: &'a str,
    pub launch: &'a str,
    /// See [`NewPty::kill_session_leftovers`](super::pty::NewPty::kill_session_leftovers).
    pub kill_session_leftovers: bool,
    /// What the daemon records of the terminal, which the holder keeps and hands back.
    pub meta: serde_json::Value,
    /// The credentials the program's helpers were given, for a program that has helpers.
    pub helpers: Option<Helpers>,
}

/// Starts `launching` on a terminal of its own: in the holder, when the daemon runs its terminals
/// in one, and in this process otherwise. **Blocks** on the start, and on the holder's when there
/// is none yet.
pub fn start(launching: Launching<'_>) -> Result<Started> {
    let Launching {
        command,
        size,
        containment,
        instance,
        launch,
        kill_session_leftovers,
        meta,
        helpers,
    } = launching;
    let Some(holders) = client::current() else {
        return Ok(term::spawn(command, size, Some(containment))?.into());
    };
    let spec = SpawnSpec {
        id: uuid::Uuid::new_v4().to_string(),
        command,
        size,
        containment: Some(ContainmentSpec {
            instance: instance.to_string(),
            launch: launch.to_string(),
        }),
        kill_session_leftovers,
        meta,
    };
    let held = match helpers.and_then(|helpers| helpers.issued) {
        // The key was issued by one holder, which has to be the one that runs the program.
        Some(issued) => issued.spawn(spec),
        None => holders.spawn(spec),
    }
    .context("starting the program in the holder")?;
    Ok(Started::Held(held))
}

pub(super) enum Backend {
    Local { pty: Pty, handle: ChildHandle },
    Held(HeldTerminal),
}

/// Reads a terminal's output, from one thread at a time.
pub(super) enum Reader {
    Local(PtyReader),
    Held(HeldReader),
}

impl Reader {
    /// See `PtyReader::read_chunk`.
    pub(super) fn read_chunk(&self, buf: &mut [u8]) -> io::Result<Option<usize>> {
        match self {
            Self::Local(reader) => reader.read_chunk(buf),
            Self::Held(reader) => reader.read_chunk(buf),
        }
    }
}

impl From<Started> for Backend {
    fn from(started: Started) -> Self {
        match started {
            Started::Local(spawned) => Self::Local {
                pty: spawned.pty,
                handle: spawned.child,
            },
            Started::Held(held) => Self::Held(held),
        }
    }
}

impl Backend {
    pub(super) fn pid(&self) -> ProcessId {
        match self {
            Self::Local { handle, .. } => handle.pid(),
            Self::Held(held) => held.pid(),
        }
    }

    /// See [`HeldTerminal::on_resumed`]; a terminal the daemon owns is never out of reach, so it
    /// never runs the hook.
    pub(super) fn on_resumed(&self, hook: impl Fn() + Send + Sync + 'static) {
        if let Self::Held(held) = self {
            held.on_resumed(hook);
        }
    }

    /// See [`HeldTerminal::on_link_changed`]; a terminal the daemon owns has no link to lose.
    pub(super) fn on_link_changed(&self, hook: impl Fn() + Send + Sync + 'static) {
        if let Self::Held(held) = self {
            held.on_link_changed(hook);
        }
    }

    /// See [`HeldTerminal::unlinked`]; never for a terminal the daemon owns.
    pub(super) fn unlinked(&self) -> bool {
        matches!(self, Self::Held(held) if held.unlinked())
    }

    /// See [`HeldTerminal::held_at`]; `None` for a terminal the daemon owns.
    pub(super) fn held_at(&self) -> Option<HeldAt> {
        match self {
            Self::Local { .. } => None,
            Self::Held(held) => Some(held.held_at()),
        }
    }

    pub(super) fn input_lock(&self) -> &Mutex<()> {
        match self {
            Self::Local { pty, .. } => pty.input_lock(),
            Self::Held(held) => held.input_lock(),
        }
    }

    pub(super) fn reader(&self) -> Reader {
        match self {
            Self::Local { pty, .. } => Reader::Local(pty.reader()),
            Self::Held(held) => Reader::Held(held.reader()),
        }
    }

    pub(super) fn stop_reader(&self) {
        match self {
            Self::Local { pty, .. } => pty.stop_reader(),
            Self::Held(held) => held.stop_reader(),
        }
    }

    pub(super) fn write_raw(&self, bytes: &[u8]) -> Result<(), PartialWrite> {
        match self {
            Self::Local { pty, .. } => pty.write_raw(bytes),
            Self::Held(held) => held.write_raw(bytes),
        }
    }

    /// See [`HeldTerminal::write_answers`]; a terminal the daemon owns is never handed over, so
    /// its answers are written as any raw input is.
    pub(super) fn write_answers(&self, bytes: &[u8]) -> Result<(), PartialWrite> {
        match self {
            Self::Local { pty, .. } => pty.write_raw(bytes),
            Self::Held(held) => held.write_answers(bytes),
        }
    }

    pub(super) fn write_input(&self, input: &[Input]) -> Result<(), PartialWrite> {
        match self {
            Self::Local { pty, .. } => pty.write_input(input),
            Self::Held(held) => held.write_input(input),
        }
    }

    pub(super) fn size(&self) -> Result<Size> {
        match self {
            Self::Local { pty, .. } => pty.size(),
            Self::Held(held) => held.size(),
        }
    }

    pub(super) fn resize(&self, size: Size) -> Result<()> {
        match self {
            Self::Local { pty, .. } => pty.resize(size),
            Self::Held(held) => held.resize(size),
        }
    }

    /// Whether a resize waits on the host's answer, which may come behind this terminal's own
    /// output: a held terminal's does, so it must not be made under the fan's lock.
    pub(super) fn resize_waits_on_reply(&self) -> bool {
        matches!(self, Self::Held(_))
    }

    pub(super) fn hang_up(&self) {
        match self {
            Self::Local { pty, .. } => pty.hang_up(),
            Self::Held(held) => held.hang_up(),
        }
    }

    pub(super) fn polite_stop_lead(&self, how: Polite) -> Duration {
        match self {
            Self::Local { pty, .. } => pty.polite_stop_lead(how),
            Self::Held(held) => held.polite_stop_lead(how),
        }
    }

    pub(super) fn foreground_job(&self) -> Foreground {
        match self {
            Self::Local { pty, .. } => pty.foreground_job(),
            Self::Held(held) => held.foreground_job(),
        }
    }

    pub(super) fn slave_path(&self) -> Option<PathBuf> {
        match self {
            Self::Local { pty, .. } => pty.slave_path(),
            Self::Held(held) => held.slave_path(),
        }
    }

    #[cfg(test)]
    pub(super) fn is_hung_up(&self) -> bool {
        match self {
            Self::Local { pty, .. } => pty.is_hung_up(),
            Self::Held(held) => held.is_hung_up(),
        }
    }

    pub(super) fn exited_unreaped(&self) -> Option<bool> {
        match self {
            Self::Local { handle, .. } => handle.exited_unreaped(),
            Self::Held(held) => held.exited_unreaped(),
        }
    }

    pub(super) fn stop(&self, how: Polite) -> Sent {
        match self {
            Self::Local { handle, .. } => handle.stop(how),
            Self::Held(held) => held.stop(how),
        }
    }

    pub(super) fn kill(&self) -> Sent {
        match self {
            Self::Local { handle, .. } => handle.kill(),
            Self::Held(held) => held.kill(),
        }
    }

    pub(super) fn kill_leftovers(&self, keep: &[ProcessIdentity]) -> Swept {
        match self {
            Self::Local { handle, .. } => handle.kill_leftovers(keep),
            Self::Held(held) => held.kill_leftovers(keep),
        }
    }

    pub(super) fn reap_if_exited(&self) -> io::Result<bool> {
        match self {
            Self::Local { handle, .. } => handle.reap_if_exited(),
            Self::Held(held) => held.reap_if_exited(),
        }
    }

    /// The program's place in this process's crash cleanup. A held program is never in it: the
    /// holder has its own, which ends the programs it runs when the holder goes down.
    pub(super) fn enter_crash_cleanup(&self, sweep_session: bool) -> Option<Option<crash::Entry>> {
        match self {
            Self::Local { handle, .. } => Some(crash::register(handle, sweep_session)),
            Self::Held(_) => None,
        }
    }

    /// The local child, for the tests that put it in the crash cleanup by hand.
    #[cfg(test)]
    pub(super) fn local_handle(&self) -> &ChildHandle {
        match self {
            Self::Local { handle, .. } => handle,
            Self::Held(_) => panic!("the terminal is held, not local"),
        }
    }
}
