//! Reading the system's process table without starting a process: which processes of the daemon's
//! own user there are, and on request what one of them runs, where, with how much memory and on
//! which listening ports. Each platform's reader is in a module of its own; what is here is
//! shared.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;

/// One process, told from any other that has had its pid before or after it by its start time:
/// a pid is reused, a pid with a start time is not.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct Identity {
    pub pid: i32,
    /// When the process started, in microseconds since the Unix epoch.
    pub start: u64,
}

impl Identity {
    /// The form a client names the process by: `<pid>:<start>`.
    pub fn key(&self) -> String {
        format!("{}:{}", self.pid, self.start)
    }

    /// The inverse of [`Self::key`].
    pub fn parse(key: &str) -> Option<Self> {
        let (pid, start) = key.split_once(':')?;
        Some(Self {
            pid: pid.parse().ok().filter(|pid| *pid > 0)?,
            start: start.parse().ok()?,
        })
    }

    /// When the process started, in milliseconds since the Unix epoch.
    pub fn started_at_ms(&self) -> i64 {
        (self.start / 1000) as i64
    }

    /// Whether this process is still running: its pid is in use, by a process that started when
    /// this one did, and is not a zombie.
    pub fn is_running(&self) -> bool {
        start_of(self.pid) == Some(self.start)
    }
}

/// One process as a scan of the table found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proc {
    pub id: Identity,
    pub ppid: i32,
    pub sid: i32,
    /// The name of the program it runs, as the system keeps it (cut to 15 or 16 bytes): what
    /// changes when it execs another.
    pub comm: String,
}

/// Every running process of the daemon's own user at one moment, zombies left out, by pid.
#[derive(Debug, Default, Clone)]
pub struct Table {
    procs: HashMap<i32, Proc>,
    /// The terminal sessions of these processes whose leader is running as another user, and so
    /// is not in the table without being gone.
    foreign_leaders: HashSet<i32>,
}

impl Table {
    /// Reads the system's table, or `None` when it cannot be listed.
    pub fn read() -> Option<Self> {
        let mut table = Self::from_procs(platform::list()?);
        let unlisted: HashSet<i32> = table
            .procs
            .values()
            .map(|proc| proc.sid)
            .filter(|&sid| sid > 1 && !table.procs.contains_key(&sid))
            .collect();
        table.foreign_leaders = unlisted
            .into_iter()
            .filter(|&sid| platform::runs_as_anyone(sid))
            .collect();
        Some(table)
    }

    pub fn from_procs(procs: impl IntoIterator<Item = Proc>) -> Self {
        Self {
            procs: procs.into_iter().map(|proc| (proc.id.pid, proc)).collect(),
            foreign_leaders: HashSet::new(),
        }
    }

    /// The table, with the sessions `sids` led by processes of another user.
    #[cfg(test)]
    pub fn with_foreign_leaders(mut self, sids: impl IntoIterator<Item = i32>) -> Self {
        self.foreign_leaders.extend(sids);
        self
    }

    /// Whether the session `sid` is led by a running process of another user: one the table
    /// leaves out, but that is not gone.
    pub fn led_by_another_user(&self, sid: i32) -> bool {
        self.foreign_leaders.contains(&sid)
    }

    /// The pids that are never any launch's: `own` — the daemon — and its ancestors, its
    /// direct children (the agents themselves, and what else the daemon runs), and pid 1.
    pub fn protected(&self, own: i32) -> Protected {
        let mut ancestors: HashSet<i32> = HashSet::from([1, own]);
        if let Some(me) = self.get(own) {
            ancestors.extend(self.ancestors(me).map(|ancestor| ancestor.id.pid));
        }
        Protected { own, ancestors }
    }

    pub fn get(&self, pid: i32) -> Option<&Proc> {
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
        procs.sort_by_key(|proc| (proc.id.start, proc.id.pid));
        procs
    }

    /// The process whose child `proc` is, when it is in the table.
    pub fn parent(&self, proc: &Proc) -> Option<&Proc> {
        if proc.ppid <= 1 {
            return None;
        }
        self.get(proc.ppid)
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
    own: i32,
    ancestors: HashSet<i32>,
}

impl Protected {
    /// Whether `proc` is the daemon, one of its ancestors, or one of its direct children.
    pub fn covers(&self, proc: &Proc) -> bool {
        proc.ppid == self.own || self.ancestors.contains(&proc.id.pid)
    }
}

/// When the process `pid` started, or `None` when there is none or it is a zombie: what a signal
/// is checked against just before it is sent.
pub fn start_of(pid: i32) -> Option<u64> {
    platform::start_of(pid)
}

/// The environment variable that tags every process an agent launch started; its value is
/// `<instance id>:<launch id>`.
pub const TAG_VARIABLE: &str = "OCTOBOARD_PROCESS_TAG";

/// What reading a process's environment for [`TAG_VARIABLE`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvTag {
    /// The environment could not be read. On macOS that is every Apple platform binary
    /// (`/bin/zsh`, `/bin/sleep`, …), whose environment the system withholds from other processes.
    Unreadable,
    /// The environment was read and carries no tag.
    Untagged,
    /// The environment carries a tag, split into the instance id and the launch id.
    Tagged { instance: String, launch: String },
}

/// Reads `id`'s environment for the tag. A process that is not `id` any more (its pid reused, or
/// gone) reads as unreadable.
pub fn env_tag(id: Identity) -> EnvTag {
    let tag = match platform::environment(id.pid) {
        None => EnvTag::Unreadable,
        Some(environment) => tag_in(&environment),
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
            return match value.split_once(':') {
                Some((instance, launch)) if !instance.is_empty() && !launch.is_empty() => {
                    EnvTag::Tagged {
                        instance: instance.to_string(),
                        launch: launch.to_string(),
                    }
                }
                _ => EnvTag::Untagged,
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
    let argv = platform::argv(id.pid)?;
    id.is_running().then_some(argv)
}

/// A process's working directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cwd {
    pub path: PathBuf,
    /// The directory was removed. `false` also when that could not be told.
    pub missing: bool,
}

/// `id`'s working directory, or `None` when it cannot be read or it is not running any more.
pub fn cwd(id: Identity) -> Option<Cwd> {
    let cwd = platform::cwd(id.pid)?;
    id.is_running().then_some(cwd)
}

/// How much memory `id` holds resident, in bytes, or `None` when it cannot be read.
pub fn memory(id: Identity) -> Option<u64> {
    platform::memory(id.pid)
}

/// The TCP ports any of `pids` listens on, sorted and without repeats. Native reads only; a
/// process whose sockets cannot be read adds nothing.
pub fn listening_ports(pids: &[i32]) -> Vec<u16> {
    let mut ports = platform::listening_ports(pids);
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
        let pid = std::process::id() as i32;
        let table = Table::read().expect("the table");
        let me = table.get(pid).expect("this process is in the table");
        assert_eq!(start_of(pid), Some(me.id.start));
        // SAFETY: `getsid(0)` only reads this process's session.
        assert_eq!(me.sid, unsafe { libc::getsid(0) });
        assert!(argv(me.id).is_some_and(|argv| !argv.is_empty()));
        let cwd = cwd(me.id).expect("this process's cwd");
        // The system reports the directory with its links resolved (`/private/var` for `/var`
        // on macOS).
        assert_eq!(
            std::fs::canonicalize(&cwd.path).unwrap(),
            std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap()
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
