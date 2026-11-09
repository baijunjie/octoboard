//! The system's process table as the ledger reads it: which processes of the daemon's own user
//! there are, each told by its pid and start time, and on request what one of them runs, where,
//! with how much memory and on which listening ports. The reads themselves are
//! `crate::platform::proc_table`'s.

use std::collections::{HashMap, HashSet};

use crate::platform::proc::{self, ContainerId, ProcessId, ProcessIdentity};
use crate::platform::proc_table::{self as platform, EnvRead};

pub use platform::{Cwd, CwdReport, CWD_REPORT_MAY_BLOCK};
pub use proc::TAG_VARIABLE;

/// One process, told from any other that has had its pid before or after it by its start time:
/// a pid is reused, a pid with a start time is not. Its start is on the clock [`now`] reads: since
/// the Unix epoch on macOS, where the system records it so, and since boot on Linux, where only
/// that stays the same across a step of the wall clock. So a start compares only with another
/// read in the same boot of the system ([`boot_id`]).
pub type Identity = ProcessIdentity;

/// One process as a scan of the table found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proc {
    pub id: Identity,
    pub ppid: ProcessId,
    /// The terminal session it is in, where the host has them.
    pub sid: Option<ContainerId>,
    /// The name of the program it runs, as the system keeps it (cut to 15 or 16 bytes): what
    /// changes when it execs another.
    pub comm: String,
}

/// Every running process of the daemon's own user at one moment, zombies left out, by pid.
#[derive(Debug, Default, Clone)]
pub struct Table {
    procs: HashMap<ProcessId, Proc>,
    /// The terminal sessions of these processes whose leader is running as another user, and so
    /// is not in the table without being gone.
    foreign_leaders: HashSet<ContainerId>,
    /// The running holder processes (see [`Self::with_holders`]).
    holders: HashSet<ProcessId>,
}

impl Table {
    /// Reads the system's table, or `None` when it cannot be listed.
    pub fn read() -> Option<Self> {
        let mut table = Self::from_procs(platform::list()?.into_iter().map(|info| Proc {
            id: Identity {
                pid: ProcessId(info.pid as u32),
                start_micros: info.start,
            },
            ppid: ProcessId(info.ppid as u32),
            sid: info.sid,
            comm: info.comm,
        }));
        let unlisted: HashSet<ContainerId> = table
            .procs
            .values()
            .filter_map(|proc| proc.sid)
            .filter(|&sid| sid.0 > 1 && !table.procs.contains_key(&sid.leader()))
            .collect();
        table.foreign_leaders = unlisted
            .into_iter()
            .filter(|&sid| platform::runs_as_anyone(sid.0 as i32))
            .collect();
        Some(table)
    }

    pub fn from_procs(procs: impl IntoIterator<Item = Proc>) -> Self {
        Self {
            procs: procs.into_iter().map(|proc| (proc.id.pid, proc)).collect(),
            foreign_leaders: HashSet::new(),
            holders: HashSet::new(),
        }
    }

    /// The table, with the holder processes `holders` names that run in it. A holder runs the
    /// programs the daemon starts, so what the daemon's direct children are to the ledger, its
    /// children are too ([`Self::protected`]).
    pub fn with_holders(mut self, holders: impl IntoIterator<Item = Identity>) -> Self {
        let running: Vec<ProcessId> = holders
            .into_iter()
            .filter(|id| self.has(id))
            .map(|id| id.pid)
            .collect();
        self.holders.extend(running);
        self
    }

    /// The table, with the sessions `sids` led by processes of another user.
    #[cfg(test)]
    pub fn with_foreign_leaders(mut self, sids: impl IntoIterator<Item = ContainerId>) -> Self {
        self.foreign_leaders.extend(sids);
        self
    }

    /// Whether the session `sid` is led by a running process of another user: one the table
    /// leaves out, but that is not gone.
    pub fn led_by_another_user(&self, sid: ContainerId) -> bool {
        self.foreign_leaders.contains(&sid)
    }

    /// The pids that are never any launch's: `own` — the daemon — and its ancestors, the holder
    /// processes, the direct children of the daemon and of the holders (the agents themselves,
    /// and what else they run, a shell terminal's shell among it), the terminal sessions those
    /// children lead, and pid 1.
    pub fn protected(&self, own: ProcessId) -> Protected {
        let mut ancestors: HashSet<ProcessId> = HashSet::from([ProcessId(1), own]);
        if let Some(me) = self.get(own) {
            ancestors.extend(self.ancestors(me).map(|ancestor| ancestor.id.pid));
        }
        ancestors.extend(&self.holders);
        let mut managers = self.holders.clone();
        managers.insert(own);
        let sessions = self
            .procs()
            .filter(|proc| {
                managers.contains(&proc.ppid)
                    && proc.sid.is_some_and(|sid| sid.leader() == proc.id.pid)
            })
            .filter_map(|proc| proc.sid)
            .collect();
        Protected {
            managers,
            ancestors,
            sessions,
        }
    }

    pub fn get(&self, pid: ProcessId) -> Option<&Proc> {
        self.procs.get(&pid)
    }

    /// Whether `id` is running in this table: its pid is here with the same start time.
    pub fn has(&self, id: &Identity) -> bool {
        self.get(id.pid).is_some_and(|proc| proc.id == *id)
    }

    pub fn procs(&self) -> impl Iterator<Item = &Proc> {
        self.procs.values()
    }

    /// Every process, oldest first, so a parent comes before its children unless both started
    /// within the clock's resolution (10 ms on Linux).
    pub fn by_start(&self) -> Vec<&Proc> {
        let mut procs: Vec<&Proc> = self.procs.values().collect();
        procs.sort_by_key(|proc| (proc.id.start_micros, proc.id.pid));
        procs
    }

    /// The process whose child `proc` is, when it is in the table.
    pub fn parent(&self, proc: &Proc) -> Option<&Proc> {
        if proc.ppid.0 <= 1 {
            return None;
        }
        // A process whose parent exited keeps that parent's pid as its parent on a host that
        // never reparents (Windows), and the pid may since have passed to a later process.
        self.get(proc.ppid)
            .filter(|parent| parent.id.start_micros <= proc.id.start_micros)
    }

    /// `proc`'s ancestors in the table, nearest first. Bounded, so a table read while pids were
    /// reused cannot loop.
    pub fn ancestors<'a>(&'a self, proc: &'a Proc) -> impl Iterator<Item = &'a Proc> + 'a {
        std::iter::successors(self.parent(proc), |proc| self.parent(proc)).take(256)
    }
}

/// What [`Table::protected`] answers.
#[derive(Debug, Clone)]
pub struct Protected {
    /// The daemon and the holders, whose direct children are protected.
    managers: HashSet<ProcessId>,
    ancestors: HashSet<ProcessId>,
    sessions: HashSet<ContainerId>,
}

impl Protected {
    /// Whether `proc` is managed by the daemon or a holder itself, including internals in a
    /// terminal session led by a direct child of either. Commands that start their own session
    /// are not covered.
    pub fn covers(&self, proc: &Proc) -> bool {
        self.managers.contains(&proc.ppid)
            || self.ancestors.contains(&proc.id.pid)
            || proc.sid.is_some_and(|sid| self.covers_session(sid))
    }

    pub fn covers_session(&self, sid: ContainerId) -> bool {
        self.sessions.contains(&sid)
    }
}

/// When the process `pid` started, or `None` when there is none or it is a zombie: what a signal
/// is checked against just before it is sent.
pub fn start_of(pid: ProcessId) -> Option<u64> {
    platform::start_of(pid.0 as i32)
}

/// Now, on the clock of [`Identity::start`].
pub fn now() -> u64 {
    platform::now()
}

/// What tells this boot of the system from any other, where start times need it to be told
/// apart (Linux); `None` where they do not, or it cannot be read.
pub fn boot_id() -> Option<String> {
    platform::boot_id()
}

/// What reading a process's environment for [`TAG_VARIABLE`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvTag {
    /// The environment could not be read, or the host does not give one. On macOS with SIP on
    /// that is every Apple platform binary (`/bin/zsh`, `/bin/sleep`, …), whose environment the
    /// system withholds from other processes.
    Unreadable,
    /// The environment was read and carries no tag.
    Untagged,
    /// The environment carries a tag, split into the instance id and the launch id.
    Tagged { instance: String, launch: String },
}

/// Reads `id`'s environment for the tag. A process that is not `id` any more (its pid reused, or
/// gone) reads as unreadable.
pub fn env_tag(id: Identity) -> EnvTag {
    let tag = match platform::environment(id.pid.0 as i32) {
        EnvRead::Unreadable | EnvRead::Unsupported => EnvTag::Unreadable,
        EnvRead::Readable(environment) => tag_in(&environment),
    };
    // Read first and checked after, so what was read is the process's that had the identity.
    if !id.is_running() {
        return EnvTag::Unreadable;
    }
    tag
}

/// The tag in a process's environment block: `KEY=value` entries separated by NULs. An empty
/// block counts as unreadable, since the system hands one back for a process whose environment it
/// withholds.
fn tag_in(environment: &[u8]) -> EnvTag {
    let prefix = format!("{TAG_VARIABLE}=");
    let mut any = false;
    for entry in environment.split(|&byte| byte == 0) {
        if entry.is_empty() {
            continue;
        }
        any = true;
        if let Some(value) = entry.strip_prefix(prefix.as_bytes()) {
            let value = String::from_utf8_lossy(value);
            return match proc::split_tag(&value) {
                Some((instance, launch)) => EnvTag::Tagged {
                    instance: instance.to_string(),
                    launch: launch.to_string(),
                },
                None => EnvTag::Untagged,
            };
        }
    }
    if any {
        EnvTag::Untagged
    } else {
        EnvTag::Unreadable
    }
}

/// `id`'s arguments, or `None` when they cannot be read or it is not running any more.
pub fn argv(id: Identity) -> Option<Vec<String>> {
    let argv = platform::argv(id.pid.0 as i32)?;
    id.is_running().then_some(argv)
}

/// What the system reports of `id`'s working directory, without the daemon looking at the
/// directory, or `None` when it cannot be read; where [`CWD_REPORT_MAY_BLOCK`], the system's own
/// answer can block as a look would. Whether it is `id`'s, and not that of a process the pid has
/// passed to since, is for the caller to check once it is done with it.
pub fn cwd_report(id: Identity) -> Option<CwdReport> {
    platform::cwd_report(id.pid.0 as i32)
}

/// How much memory `id` holds resident, in bytes, or `None` when it cannot be read.
pub fn memory(id: Identity) -> Option<u64> {
    platform::memory(id.pid.0 as i32)
}

/// The TCP ports any of `pids` listens on, sorted and without repeats. Native reads only; a
/// process whose sockets cannot be read adds nothing.
pub fn listening_ports(pids: &[ProcessId]) -> Vec<u16> {
    let pids: Vec<i32> = pids.iter().map(|pid| pid.0 as i32).collect();
    let mut ports = platform::listening_ports(&pids);
    ports.sort_unstable();
    ports.dedup();
    ports
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tag_is_read_from_an_environment_block_and_an_empty_one_is_unreadable() {
        let tagged = format!("A=1\0{TAG_VARIABLE}=inst:launch\0B=2\0");
        assert_eq!(
            tag_in(tagged.as_bytes()),
            EnvTag::Tagged {
                instance: "inst".to_string(),
                launch: "launch".to_string()
            }
        );
        assert_eq!(tag_in(b"A=1\0B=2\0"), EnvTag::Untagged);
        assert_eq!(tag_in(b""), EnvTag::Unreadable);
        assert_eq!(tag_in(b"\0\0"), EnvTag::Unreadable);
    }

    /// The table is read with this process in it, with its own start time, and a listener it
    /// holds is found among its listening ports.
    #[test]
    fn the_table_holds_this_process_and_its_listening_port() {
        let pid = ProcessId(std::process::id());
        let table = Table::read().expect("the table");
        let me = table.get(pid).expect("this process is in the table");
        assert_eq!(start_of(pid), Some(me.id.start_micros));
        // On the clock `now` reads, and shown on the wall clock.
        assert!(me.id.start_micros <= now());
        let wall_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let age_ms = (now() - me.id.start_micros) as i64 / 1000;
        assert!((me.id.started_at_ms() - (wall_ms - age_ms)).abs() < 1000);
        // SAFETY: `getsid(0)` only reads this process's session.
        #[cfg(unix)]
        assert_eq!(me.sid, Some(ContainerId(unsafe { libc::getsid(0) } as u32)));
        assert!(argv(me.id).is_some_and(|argv| !argv.is_empty()));
        let cwd = cwd_report(me.id)
            .and_then(|report| report.looked())
            .expect("this process's cwd");
        // The system reports the directory with its links resolved (`/private/var` for `/var`
        // on macOS).
        assert_eq!(
            crate::hostfs::canonical(&cwd.path).unwrap(),
            crate::hostfs::canonical(std::env::current_dir().unwrap()).unwrap()
        );
        assert!(!cwd.missing);
        assert!(memory(me.id).is_some_and(|bytes| bytes > 0));

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(listening_ports(&[pid]).contains(&port));
        drop(listener);
        assert!(!listening_ports(&[pid]).contains(&port));
    }
}
