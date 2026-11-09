//! One daemon at a time per data directory.
//!
//! Two daemons would own two sets of agent processes while sharing one database, and the second
//! one's startup would mark the first one's live sessions interrupted — the session list of a
//! perfectly healthy application would go stale in front of the user. Refusing to start is the
//! legible failure; the application surfaces the message below.
//!
//! A user-run command finds a running daemon without talking to it: [`is_running`] says whether one
//! runs, which only the lock can say, and [`Owner`] says which process it is and where it listens.

use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::hostfs::replace::{replace_beside, Permissions};
use crate::platform::fs::Busy;
use crate::platform::proc::{ProcessId, ProcessIdentity};
use crate::platform::proc_table;

/// Holds the lock for as long as it is alive. The lock is released by the OS when the process
/// exits, so a crashed daemon leaves no stale lock behind.
pub struct InstanceLock {
    _file: File,
}

pub fn acquire() -> Result<InstanceLock> {
    let dir = crate::paths::data_dir();
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    // The files written below hold keys and session data; where the host makes them private
    // through the directory, an unprivate one is still written, so the daemon runs on.
    if let Err(err) = crate::platform::fs::keep_private_below(&dir) {
        tracing::warn!(path = %dir.display(), %err, "making the data directory private failed");
    }
    let path = lock_path();
    match crate::platform::fs::try_lock_file(&path) {
        Ok(file) => Ok(InstanceLock { _file: file }),
        Err(Busy::Held) => bail!(
            "another octoboardd is already running against {}",
            dir.display()
        ),
        Err(Busy::Open(err)) => Err(err).with_context(|| format!("opening {}", path.display())),
        Err(Busy::Lock(err)) => Err(err).with_context(|| format!("locking {}", path.display())),
    }
}

fn lock_path() -> PathBuf {
    crate::paths::data_dir().join("daemon.lock")
}

/// Whether a daemon runs against this data directory, which the lock alone says: a crashed daemon
/// leaves no lock behind, and a record of its own could outlive it. The lock is taken to find out
/// and released at once, so a daemon starting in that instant finds it held and refuses to start.
pub fn is_running() -> Result<bool> {
    is_running_at(&lock_path())
}

fn is_running_at(path: &Path) -> Result<bool> {
    match crate::platform::fs::try_lock_file(path) {
        Ok(_file) => Ok(false),
        Err(Busy::Held) => Ok(true),
        // No data directory yet: no daemon has ever run here, and a status check creates nothing.
        Err(Busy::Open(err)) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(Busy::Open(err)) => Err(err).with_context(|| format!("opening {}", path.display())),
        Err(Busy::Lock(err)) => Err(err).with_context(|| format!("locking {}", path.display())),
    }
}

/// Who holds the lock and where it listens, which the daemon records once it is bound, in a file
/// beside the lock rather than in it: on Windows a locked file cannot be read by another process.
/// Written and read only through the process's identity, so a pid reused by another process is
/// never taken for the daemon.
///
/// The record is one line: the identity, the port, and whether the daemon follows an application
/// (`application`) or runs on its own (`alone`). A daemon that predates the third token writes the
/// first two only, and takes no planned-restart request; so does one that could not install the
/// handler of that request, which `restart` then refuses. The other way round, the command line of
/// a build that predates the third token cannot read a record that has it, so it finds no owner for
/// the daemon of this build; that is accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Owner {
    pub identity: ProcessIdentity,
    pub port: u16,
    /// Whether the daemon follows an application (`--parent-pid`), which restarts it itself;
    /// `None` for a record without the third token, or with one this build does not know.
    pub follows_application: Option<bool>,
}

/// The third token of a record of a daemon that follows an application.
const FOLLOWS_APPLICATION: &str = "application";

/// The third token of a record of a daemon that runs on its own.
const ALONE: &str = "alone";

impl Owner {
    /// This process, listening on `port`, following an application or not; `None` when the host
    /// cannot tell when it started. Without `takes_restart`, the record has no third token, since
    /// a request for a planned restart would end this process rather than restart it.
    pub fn current(port: u16, follows_application: bool, takes_restart: bool) -> Option<Self> {
        let pid = std::process::id();
        let start_micros = proc_table::start_of(i32::try_from(pid).ok()?)?;
        Some(Self {
            identity: ProcessIdentity {
                pid: ProcessId(pid),
                start_micros,
            },
            port,
            follows_application: takes_restart.then_some(follows_application),
        })
    }

    /// Records this owner, replacing any earlier record whole and owner-only.
    pub fn record(&self) -> std::io::Result<()> {
        self.write_to(&record_path())
    }

    /// The recorded owner, or `None` when there is no record, it is unreadable, or the process it
    /// names is gone — a daemon killed before it could remove it. A daemon that holds the lock may
    /// still have none: it may still be starting or stopping.
    pub fn recorded() -> Option<Self> {
        Self::read_from(&record_path()).filter(|owner| owner.identity.is_running())
    }

    /// Removes the record on the way out, unless it is another daemon's by now.
    pub fn forget(&self) {
        if let Err(err) = self.remove_from(&record_path()) {
            tracing::warn!(%err, "removing the owner record failed");
        }
    }

    fn write_to(&self, path: &Path) -> std::io::Result<()> {
        let mut content = format!("{} {}", self.identity.key(), self.port);
        match self.follows_application {
            Some(true) => content.push_str(&format!(" {FOLLOWS_APPLICATION}")),
            Some(false) => content.push_str(&format!(" {ALONE}")),
            None => {}
        }
        content.push('\n');
        replace_beside(path, content.as_bytes(), Permissions::Private)
    }

    fn read_from(path: &Path) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        let mut tokens = content.split_whitespace();
        let identity = ProcessIdentity::parse(tokens.next()?)?;
        let port = tokens.next()?.parse().ok()?;
        let follows_application = match tokens.next() {
            Some(FOLLOWS_APPLICATION) => Some(true),
            Some(ALONE) => Some(false),
            _ => None,
        };
        Some(Self {
            identity,
            port,
            follows_application,
        })
    }

    fn remove_from(&self, path: &Path) -> std::io::Result<()> {
        if Self::read_from(path) != Some(*self) {
            return Ok(());
        }
        match std::fs::remove_file(path) {
            Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(err),
            _ => Ok(()),
        }
    }
}

fn record_path() -> PathBuf {
    crate::paths::data_dir().join("daemon.owner")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;

    fn owner(pid: u32, port: u16) -> Owner {
        Owner {
            identity: ProcessIdentity {
                pid: ProcessId(pid),
                start_micros: 123_456,
            },
            port,
            follows_application: Some(false),
        }
    }

    #[test]
    fn the_record_reads_back_and_is_removed_only_by_the_daemon_that_wrote_it() {
        let dir = ScratchDir::new("owner-record");
        let path = dir.join("daemon.owner");
        assert_eq!(Owner::read_from(&path), None);

        let ours = owner(10, 7531);
        ours.write_to(&path).expect("the record was written");
        assert_eq!(Owner::read_from(&path), Some(ours));

        owner(11, 7531).remove_from(&path).unwrap();
        owner(10, 7532).remove_from(&path).unwrap();
        assert!(path.exists(), "another daemon's record was removed");
        ours.remove_from(&path).unwrap();
        assert!(!path.exists());
        // Already gone: nothing to do.
        ours.remove_from(&path).unwrap();
    }

    #[test]
    fn whether_the_daemon_follows_an_application_reads_back() {
        let dir = ScratchDir::new("owner-record-application");
        let path = dir.join("daemon.owner");
        let following = Owner {
            follows_application: Some(true),
            ..owner(10, 7531)
        };
        following.write_to(&path).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "10:123456 7531 application\n"
        );
        assert_eq!(Owner::read_from(&path), Some(following));
        owner(10, 7531).write_to(&path).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "10:123456 7531 alone\n"
        );
        assert_eq!(Owner::read_from(&path), Some(owner(10, 7531)));
    }

    /// A record an earlier build wrote, with no third token, still names the daemon and its port;
    /// whether it follows an application is not known, and neither is a third token of a later
    /// build's.
    #[test]
    fn a_record_of_two_tokens_or_an_unknown_third_still_reads() {
        let dir = ScratchDir::new("owner-record-old");
        let path = dir.join("daemon.owner");
        let unknown = Owner {
            follows_application: None,
            ..owner(10, 7531)
        };
        for content in ["10:123456 7531\n", "10:123456 7531 elsewhere\n"] {
            std::fs::write(&path, content).unwrap();
            assert_eq!(Owner::read_from(&path), Some(unknown), "{content:?}");
        }
        // The daemon that wrote such a record removes it on its way out.
        std::fs::write(&path, "10:123456 7531\n").unwrap();
        unknown.remove_from(&path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn a_later_record_replaces_the_earlier_one() {
        let dir = ScratchDir::new("owner-record-replaced");
        let path = dir.join("daemon.owner");
        owner(10, 7531).write_to(&path).unwrap();
        owner(12, 7540).write_to(&path).unwrap();
        assert_eq!(Owner::read_from(&path), Some(owner(12, 7540)));
    }

    #[test]
    fn an_unreadable_record_is_none() {
        let dir = ScratchDir::new("owner-record-garbage");
        let path = dir.join("daemon.owner");
        for content in [
            "",
            "garbage",
            "10:5",
            "10:5 notaport",
            "0:5 7531",
            "10:5 70000",
        ] {
            std::fs::write(&path, content).unwrap();
            assert_eq!(Owner::read_from(&path), None, "{content:?}");
        }
    }

    #[test]
    fn a_held_lock_is_a_running_daemon_and_a_free_or_missing_one_is_not() {
        let dir = ScratchDir::new("is-running");
        let path = dir.join("daemon.lock");
        assert!(!is_running_at(&path).unwrap(), "no daemon has run here yet");

        let held = crate::platform::fs::try_lock_file(&path).expect("the lock was taken");
        assert!(is_running_at(&path).unwrap());
        drop(held);
        assert!(!is_running_at(&path).unwrap(), "the lock was released");

        let missing = dir.join("absent").join("daemon.lock");
        assert!(!is_running_at(&missing).unwrap());
        assert!(
            !missing.parent().unwrap().exists(),
            "a check creates nothing"
        );
    }

    #[test]
    fn this_process_is_a_running_owner() {
        let ours = Owner::current(7531, false, true).expect("this process's start time");
        assert_eq!(ours.identity.pid.0, std::process::id());
        assert!(ours.identity.is_running());
    }
}
