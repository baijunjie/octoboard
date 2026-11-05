//! What the daemon asks of the host about processes it starts: leading a session or a process
//! group, telling how a child stands without reaping it, signalling a process or a group by what
//! is meant (a polite request to end, or a kill) rather than by a signal number, listing what a
//! session left behind, the table that ends every process the daemon runs when it goes down on an
//! internal failure ([`crash`]), and reading a child's pipes within bounds. It also holds what the
//! daemon asks of the host about itself: the host asking it to end ([`shutdown_requested`]) and
//! the application that started it going away ([`watch_parent`]). Which processes to end, when,
//! and with what grace are for the callers to decide; this module only does what it is asked.
//!
//! A group or a session is told by its id, which is the leader's pid, so it is signalled or swept
//! only while the leader is unreaped: until then its pid, and with it that id, cannot pass to
//! another process. A [`ChildHandle`] owns that rule: it is made for a child the caller holds
//! unreaped, right after starting a command [`Reach::lead`] prepared ([`ChildHandle::started`],
//! without which such a child may never run), and the caller keeps it only until it reaps. What one
//! launch's processes are told apart by, wherever they went, is its [`Containment`].

use std::collections::HashMap;
use std::io::{self, Read};
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use super::proc_table;

pub(crate) mod crash;

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

/// A process told by the number the host gives it, valid only while that process runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProcessId(pub u32);

/// What the host groups processes by where it lets them be listed by group: on Unix the id of a
/// terminal session, which is its leader's pid. A host that tells a launch's processes by the
/// [`Containment`] alone has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContainerId(pub u32);

impl ContainerId {
    /// The process whose id names the container, which leads it.
    pub fn leader(self) -> ProcessId {
        ProcessId(self.0)
    }
}

/// One process, told from any other that has had its pid before or after it by its start time: a
/// pid is reused, a pid with a start time is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProcessIdentity {
    pub pid: ProcessId,
    /// When the process started, in microseconds on a clock that compares only with another read
    /// of the same boot of the system.
    pub start_micros: u64,
}

impl ProcessIdentity {
    /// When the process started, in milliseconds since the Unix epoch.
    pub fn started_at_ms(&self) -> i64 {
        (proc_table::epoch_micros(self.start_micros) / 1000) as i64
    }

    /// Whether this process is still running: its pid is in use, by a process that started when
    /// this one did, and is not a zombie.
    pub fn is_running(&self) -> bool {
        proc_table::start_of(self.pid.0 as i32) == Some(self.start_micros)
    }

    /// The form a client names the process by: `<pid>:<start>`.
    pub fn key(&self) -> String {
        format!("{}:{}", self.pid.0, self.start_micros)
    }

    /// The inverse of [`Self::key`]; a pid of 0 is none.
    pub fn parse(key: &str) -> Option<Self> {
        let (pid, start) = key.split_once(':')?;
        Some(Self {
            pid: ProcessId(pid.parse().ok().filter(|pid| *pid > 0)?),
            start_micros: start.parse().ok()?,
        })
    }
}

/// The environment variable that tags every process a launch started; its value is
/// `<instance id>:<launch id>`.
pub const TAG_VARIABLE: &str = "OCTOBOARD_PROCESS_TAG";

/// The value of [`TAG_VARIABLE`] for a launch of the daemon instance `instance`.
pub fn tag_value(instance: &str, launch: &str) -> String {
    format!("{instance}:{launch}")
}

/// The instance id and the launch id a value of [`TAG_VARIABLE`] names, or `None` when it names
/// no launch.
pub fn split_tag(value: &str) -> Option<(&str, &str)> {
    value
        .split_once(':')
        .filter(|(instance, launch)| !instance.is_empty() && !launch.is_empty())
}

/// Where a launch's environment is set: a map the process is started over, or the command that
/// starts it.
pub trait SetEnv {
    fn set_env(&mut self, key: &str, value: &str);
}

impl SetEnv for HashMap<String, String> {
    fn set_env(&mut self, key: &str, value: &str) {
        self.insert(key.to_string(), value.to_string());
    }
}

impl SetEnv for Command {
    fn set_env(&mut self, key: &str, value: &str) {
        self.env(key, value);
    }
}

/// How a process is asked to end before it is killed, by what it means on every host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Polite {
    /// To finish what it is doing and exit: what an agent is asked, since its own shutdown work
    /// (a hook that fires on a clean teardown, a state file written through a rename) runs only
    /// then.
    Terminate,
    /// To behave as if its terminal had closed: what a shell and its jobs are asked.
    HangUp,
}

/// What is sent to a process or a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Polite(Polite),
    /// Ends it outright, with nothing of its own left to run.
    Kill,
}

/// What came of sending a [`Signal`].
#[derive(Debug)]
pub enum Sent {
    Delivered,
    /// There was no such process or group any more: it exited in the meantime.
    Gone,
    Failed(io::Error),
}

impl Sent {
    pub fn delivered(&self) -> bool {
        matches!(self, Self::Delivered)
    }
}

/// How an unreaped child stands, as [`unreaped_state`] finds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    windows,
    allow(
        dead_code,
        reason = "Windows tells a child by its handle, not by its pid"
    )
)]
pub enum Unreaped {
    Running,
    /// Exited, with status 0 when `success`.
    Exited {
        success: bool,
    },
}

/// What a stop of a command kills besides the command itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// The process group the command leads: what it forked without moving it elsewhere, such as
    /// the `ssh` or askpass helper of a `git`. Left alone when the command ends by itself.
    Group,
    /// The session the command leads, for an agent CLI: it moves some helpers into process groups
    /// of their own, which a signal to its group misses. Killed when the command ends by itself
    /// too, as nothing is left to stop a helper of a CLI that has gone.
    Session,
}

impl Reach {
    /// Has `command` lead what this reach names: its own process group, or its own session (and so
    /// a group too).
    pub fn lead(self, command: &mut Command) {
        match self {
            Self::Group => lead_group(command),
            Self::Session => lead_session(command),
        }
    }
}

/// Asks every member of the group `leader` leads to end as `how` says.
pub fn stop_group(leader: ProcessId, how: Polite) -> Sent {
    signal_group(leader, Signal::Polite(how))
}

/// Kills every member of the group `leader` leads.
#[cfg_attr(
    windows,
    allow(
        dead_code,
        reason = "only the Unix shell snapshot kills a group by its id"
    )
)]
pub fn kill_group(leader: ProcessId) -> Sent {
    signal_group(leader, Signal::Kill)
}

/// What [`ChildHandle::kill_leftovers`] did.
pub struct Swept {
    pub killed: usize,
    /// `false` when listing the processes failed, so some may be left.
    pub complete: bool,
}

impl Swept {
    /// Logs the sweep of the session `leader` leads, named `process` in the log, when it killed
    /// anything or could not finish.
    pub fn log(&self, leader: ProcessId, process: &str) {
        if !self.complete {
            tracing::warn!(
                process,
                pid = leader.0,
                killed = self.killed,
                "listing processes failed; the sweep of the process's session may be incomplete"
            );
        } else if self.killed > 0 {
            tracing::info!(
                process,
                pid = leader.0,
                killed = self.killed,
                "killed processes left in the process's session"
            );
        }
    }
}

/// Upper bound on the bounded reap attempted after a kill. A process wedged in an uninterruptible
/// kernel wait (a dead network mount under `/Volumes` is the realistic trigger here) can leave the
/// signal pending indefinitely, and nothing short of the kernel itself can force that reap to
/// finish — so this is a best effort, not a guarantee, and giving up after it elapses risks at
/// most one leaked zombie.
pub const REAP_GRACE: Duration = Duration::from_millis(500);

/// Size of each chunk a pipe reader thread forwards over its channel. Arbitrary beyond "comfortably
/// smaller than the pipe's own OS buffer", since the channel is unbounded and nothing here needs
/// the chunking to line up with any particular boundary in the data.
const PIPE_CHUNK_SIZE: usize = 8192;

/// What a bounded pipe reader forwards: a chunk, the news that the pipe went past its limit (after
/// which the reader has stopped reading), or a read error.
pub enum BoundedChunk {
    Data(Vec<u8>),
    Exceeded,
    Failed(io::Error),
}

/// Reads `pipe` until EOF, forwarding at most `limit` bytes over `tx`. Past the limit it either
/// stops reading altogether and reports [`BoundedChunk::Exceeded`] (`stop_at_limit`), which drops
/// the read end so the writer gets `EPIPE` instead of a pipe that never drains, or keeps reading
/// and discards the rest, so a chatty writer is never blocked on a full pipe. Either way at most
/// `limit` bytes are ever held, here or in the channel.
pub fn bounded_pipe_reader(
    mut pipe: impl Read,
    tx: &mpsc::Sender<BoundedChunk>,
    limit: usize,
    stop_at_limit: bool,
) {
    let mut chunk = vec![0u8; PIPE_CHUNK_SIZE];
    let mut forwarded = 0usize;
    loop {
        match pipe.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                let room = limit - forwarded;
                if n > room {
                    if room > 0 && tx.send(BoundedChunk::Data(chunk[..room].to_vec())).is_err() {
                        break;
                    }
                    forwarded = limit;
                    if stop_at_limit {
                        let _ = tx.send(BoundedChunk::Exceeded);
                        break;
                    }
                    continue;
                }
                forwarded += n;
                if tx.send(BoundedChunk::Data(chunk[..n].to_vec())).is_err() {
                    break;
                }
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => {
                let _ = tx.send(BoundedChunk::Failed(err));
                break;
            }
        }
    }
}
