//! What a daemon records of itself for the commands a user runs against it: who it is and where it
//! listens ([`Owner`]), in a file beside the instance lock, and the key it requires of every
//! connection ([`Owner::recorded_key`]), in an owner-only file of its own. Whether a daemon runs at
//! all is the lock's to say ([`crate::instance_lock::is_running`]); a record may outlive a daemon
//! killed before it could remove it. A daemon that releases the lock to the daemon that takes over
//! after a planned restart also leaves a note of that ([`Owner::note_handover`]), which shows that
//! one on its way.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::hostfs::replace::{remove_replaced, replace_beside, Permissions};
use crate::platform::proc::{ProcessId, ProcessIdentity};
use crate::platform::proc_table;

/// Who holds the lock and where it listens, which the daemon records once it is bound, in a file
/// beside the lock rather than in it: on Windows a locked file cannot be read by another process.
/// Written and read only through the process's identity, so a pid reused by another process is
/// never taken for the daemon.
///
/// The record is one line: the identity, the port, and whether the daemon follows an application
/// (`application`) or runs on its own (`alone`). A daemon that could not install the handler of
/// the planned-restart request writes no third token, and `restart` refuses it.
///
/// Last comes whether a service manager starts the daemon's successor after a planned restart
/// (`service`) or the daemon starts it itself (`no-service`), which tells the command line how to
/// recognise that successor. A build reads the third token alone and ignores what follows, and
/// takes a word it does not know in its place as no third token, so this token changes nothing it
/// reads, whether it comes fourth or, without a third, in its place. It is read wherever it
/// stands after the port; a record without it, as an earlier build writes, does not say.
///
/// A daemon that accepts the owner key ([`crate::owner_key`]) adds `owner-key` after the tokens
/// above, and is read wherever it stands after the port. It changes nothing an earlier build reads
/// for the same reason: it is the third token only in a record that has none of the others, and
/// that build takes an unknown word there as no third token. A record without it is of a daemon
/// that takes no owner key, as one that follows the application or an earlier build.
///
/// Two more tokens follow for the same reason, read wherever they stand after the port:
/// `unit=<name>`, the systemd unit a service-managed daemon runs as, so that a command can ask
/// systemd whether it is restarting the daemon; and `after=<pid:start>` or `after=none`, which
/// daemon this one took over from after a planned restart (or after a crash, in a service), so that
/// a command following a daemon through a planned restart compares identities. A record without
/// `after=`, as a build before it writes or a daemon that cannot tell writes, does not say
/// ([`Predecessor::Unsaid`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Owner {
    pub identity: ProcessIdentity,
    pub port: u16,
    /// Whether the daemon follows an application (`--parent-pid`), which restarts it itself;
    /// `None` for a record without the third token, or with one this build does not know.
    pub follows_application: Option<bool>,
    /// Whether a service manager starts the daemon that takes over after a planned restart, which
    /// makes a key of its own, rather than this daemon, which hands over its key; `None` for a
    /// record without the token.
    pub service_managed: Option<bool>,
    /// Whether the daemon accepts the owner key, which `address` then prints in place of the
    /// launch key.
    pub takes_owner_key: bool,
    /// The systemd unit the daemon runs as, where a service manager restarts it and it could name
    /// the unit (a systemd user service, systemd 248 or later); `None` otherwise.
    pub unit: Option<String>,
    /// The daemon this one took over from.
    pub predecessor: Predecessor,
}

/// Which daemon a daemon took over from, as its record says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Predecessor {
    /// The record does not say: a build before the token wrote it; a daemon whose predecessor
    /// handed it no identity (one of such a build, or one that could not tell its own start time);
    /// or a service-managed daemon that found no note or leftover record of a daemon before it
    /// (see [`Owner::service_predecessor`]).
    Unsaid,
    /// None: a daemon a user or the desktop application started.
    None,
    /// The daemon with this identity, which handed over to it in a planned restart, or which a
    /// service manager restarted it after.
    Daemon(ProcessIdentity),
}

/// A daemon that has released the instance lock, or is about to, to the daemon that takes over
/// from it after a planned restart, as its note says ([`Owner::note_handover`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandoverNote {
    /// The daemon, as its owner record named it, whether or not its process still runs.
    pub from: Owner,
    /// When the note was written, which is when the daemon released the lock, give or take its
    /// teardown.
    pub noted: SystemTime,
}

/// The third token of a record of a daemon that follows an application.
const FOLLOWS_APPLICATION: &str = "application";

/// The third token of a record of a daemon that runs on its own.
const ALONE: &str = "alone";

/// The token of a record of a daemon whose successor a service manager starts.
const SERVICE: &str = "service";

/// The token of a record of a daemon that starts its successor itself.
const NO_SERVICE: &str = "no-service";

/// The token of a record of a daemon that accepts the owner key.
const OWNER_KEY: &str = "owner-key";

/// What leads the token naming the daemon's systemd unit.
const UNIT: &str = "unit=";

/// What leads the token naming the daemon this one took over from.
const AFTER: &str = "after=";

/// What follows [`AFTER`] for a daemon that took over from none.
const AFTER_NONE: &str = "none";

/// The identity of this process; `None` when the host cannot tell when it started.
pub fn own_identity() -> Option<ProcessIdentity> {
    let pid = std::process::id();
    let start_micros = proc_table::start_of(i32::try_from(pid).ok()?)?;
    Some(ProcessIdentity {
        pid: ProcessId(pid),
        start_micros,
    })
}

/// Whether `unit` has the form of a systemd unit name, which the record can hold as one token.
fn is_unit_name(unit: &str) -> bool {
    !unit.is_empty()
        && unit.len() <= 256
        && unit
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b":-_.\\@".contains(&byte))
}

impl Owner {
    /// This process, listening on `port`, following an application or not; `None` when the host
    /// cannot tell when it started. Without `takes_restart`, the record has no third token, since
    /// a request for a planned restart would end this process rather than restart it. Whether a
    /// service manager starts its successor is not said until [`Owner::with_service_manager`].
    pub fn current(port: u16, follows_application: bool, takes_restart: bool) -> Option<Self> {
        Some(Self {
            identity: own_identity()?,
            port,
            follows_application: takes_restart.then_some(follows_application),
            service_managed: None,
            takes_owner_key: false,
            unit: None,
            predecessor: Predecessor::Unsaid,
        })
    }

    /// This owner, naming the systemd unit it runs as, where it is one the record can hold.
    pub fn with_unit(self, unit: Option<String>) -> Self {
        Self {
            unit: unit.filter(|unit| is_unit_name(unit)),
            ..self
        }
    }

    /// This owner, saying which daemon it took over from.
    pub fn with_predecessor(self, predecessor: Predecessor) -> Self {
        Self {
            predecessor,
            ..self
        }
    }

    /// This owner, saying whether the daemon accepts the owner key.
    pub fn with_owner_key(self, takes: bool) -> Self {
        Self {
            takes_owner_key: takes,
            ..self
        }
    }

    /// This owner, saying whether a service manager starts its successor after a planned restart.
    pub fn with_service_manager(self, managed: bool) -> Self {
        Self {
            service_managed: Some(managed),
            ..self
        }
    }

    /// Records this owner, replacing any earlier record whole and owner-only.
    pub fn record(&self) -> std::io::Result<()> {
        self.write_to(&record_path())
    }

    /// The recorded owner, or `None` when there is no record, it is unreadable, or the process it
    /// names is gone — a daemon killed before it could remove it. A daemon that holds the lock may
    /// still have none: it may still be starting or stopping.
    pub fn recorded() -> Option<Self> {
        Self::recorded_in(&crate::paths::data_dir())
    }

    /// [`Owner::recorded`] for the daemon whose data directory is `data`.
    pub fn recorded_in(data: &Path) -> Option<Self> {
        Self::read_from(&record_path_in(data)).filter(|owner| owner.identity.is_running())
    }

    /// What the owner record names, whether or not its process still runs: a daemon killed or
    /// crashed before it could remove its record leaves it there.
    pub fn last_recorded() -> Option<Self> {
        Self::read_from(&record_path())
    }

    /// The daemon a service-managed daemon starting in the unit `unit` takes over from: the one
    /// whose note says it handed over, else the one the owner record still names, having ended
    /// without removing it (it crashed), where that daemon was service-managed too, has ended, as
    /// a service manager waits for, and ran in the same unit where both name one. Read before this
    /// daemon writes its own record and settles the note. Where neither qualifies it does not say
    /// ([`Predecessor::Unsaid`]) rather than name none: a daemon of an earlier build that exited for
    /// the service manager leaves neither a note nor a record, and the commands then compare keys,
    /// which a start by a service manager never shares with the daemon before it.
    pub fn service_predecessor(unit: Option<&str>) -> Predecessor {
        Self::service_predecessor_in(&crate::paths::data_dir(), unit)
    }

    /// [`Owner::service_predecessor`] in the data directory `data`.
    pub fn service_predecessor_in(data: &Path, unit: Option<&str>) -> Predecessor {
        let qualifies = |from: &Owner| restarted_in_service(from, unit, from.identity.is_running());
        Self::handover_noted_in(data)
            .map(|noted| noted.from)
            .filter(qualifies)
            .or_else(|| Self::read_from(&record_path_in(data)).filter(qualifies))
            .map_or(Predecessor::Unsaid, |from| {
                Predecessor::Daemon(from.identity)
            })
    }

    /// Records `key`, the key this daemon requires of every connection, for the owner's `address`
    /// command: owner-only, in a file of its own beside the owner record, which stays free of the
    /// key. Written before the owner record, so that a daemon found through that record has its
    /// key recorded already.
    pub fn record_key(&self, key: &str) -> std::io::Result<()> {
        self.write_key_to(&key_path(), key)
    }

    /// The key this daemon recorded, or `None` when there is none, it is unreadable, or it is
    /// another daemon's — one killed before it could remove its own, or one starting.
    pub fn recorded_key(&self) -> Option<String> {
        self.recorded_key_in(&crate::paths::data_dir())
    }

    /// [`Owner::recorded_key`] in the data directory `data`.
    pub fn recorded_key_in(&self, data: &Path) -> Option<String> {
        self.read_key_from(&key_path_in(data))
    }

    /// Records this owner and `key` in the data directory `data`, as a daemon does in its own:
    /// for a test standing in for a running daemon.
    #[cfg(test)]
    pub(crate) fn record_in(&self, data: &Path, key: &str) -> std::io::Result<()> {
        std::fs::create_dir_all(data)?;
        self.write_key_to(&key_path_in(data), key)?;
        self.write_to(&record_path_in(data))
    }

    /// Notes that this daemon is about to release the instance lock to the daemon that takes over
    /// from it after a planned restart, in a file beside the owner record, owner-only. The owner
    /// record goes before the lock does and the next daemon records itself only once it has the
    /// lock, so without the note nothing would show, while the lock is free or held by a daemon
    /// not yet recorded, that a daemon is on its way. Written before the owner record is removed;
    /// the next daemon removes it once it has recorded itself ([`settle_handover`]).
    pub fn note_handover(&self) -> std::io::Result<()> {
        self.note_handover_in(&crate::paths::data_dir())
    }

    /// [`Owner::note_handover`] in the data directory `data`.
    pub(crate) fn note_handover_in(&self, data: &Path) -> std::io::Result<()> {
        self.write_to(&handover_path_in(data))
    }

    /// The note of the daemon that last released the lock to its successor, while no daemon has
    /// recorded itself since; `None` when there is none, or it is unreadable.
    pub fn handover_noted() -> Option<HandoverNote> {
        Self::handover_noted_in(&crate::paths::data_dir())
    }

    /// [`Owner::handover_noted`] in the data directory `data`.
    pub fn handover_noted_in(data: &Path) -> Option<HandoverNote> {
        let path = handover_path_in(data);
        let noted = std::fs::metadata(&path).ok()?.modified().ok()?;
        Some(HandoverNote {
            from: Self::read_from(&path)?,
            noted,
        })
    }

    /// Removes the note of this daemon's handover when its successor did not start, unless the
    /// note is another daemon's by now.
    pub fn withdraw_handover(&self) {
        self.withdraw_handover_in(&crate::paths::data_dir());
    }

    fn withdraw_handover_in(&self, data: &Path) {
        if let Err(err) = self.remove_from(&handover_path_in(data)) {
            tracing::warn!(%err, "removing the handover note failed");
        }
    }

    /// Removes the owner and key records on the way out, unless they are another daemon's by now.
    pub fn forget(&self) {
        if let Err(err) = self.remove_from(&record_path()) {
            tracing::warn!(%err, "removing the owner record failed");
        }
        if let Err(err) = self.remove_key_from(&key_path()) {
            tracing::warn!(%err, "removing the key record failed");
        }
    }

    /// The key record is one line: this daemon's identity, so that a stale record is never taken
    /// for the running daemon's, and the key.
    fn write_key_to(&self, path: &Path, key: &str) -> std::io::Result<()> {
        let content = format!("{} {key}\n", self.identity.key());
        replace_beside(path, content.as_bytes(), Permissions::Private)
    }

    fn read_key_from(&self, path: &Path) -> Option<String> {
        let content = std::fs::read_to_string(path).ok()?;
        let (identity, rest) = split_key_record(&content)?;
        let mut tokens = rest.split_whitespace();
        let key = tokens.next()?;
        (identity == self.identity && crate::request_guard::is_key(key) && tokens.next().is_none())
            .then(|| key.to_owned())
    }

    /// Removes the key record when its identity is this daemon's, whatever follows it: a record
    /// of ours with a malformed key is ours to remove all the same.
    fn remove_key_from(&self, path: &Path) -> std::io::Result<()> {
        let ours = std::fs::read_to_string(path)
            .ok()
            .and_then(|content| Some(split_key_record(&content)?.0))
            == Some(self.identity);
        if !ours {
            return Ok(());
        }
        remove_replaced(path)
    }

    fn write_to(&self, path: &Path) -> std::io::Result<()> {
        let mut content = format!("{} {}", self.identity.key(), self.port);
        match self.follows_application {
            Some(true) => content.push_str(&format!(" {FOLLOWS_APPLICATION}")),
            Some(false) => content.push_str(&format!(" {ALONE}")),
            None => {}
        }
        match self.service_managed {
            Some(true) => content.push_str(&format!(" {SERVICE}")),
            Some(false) => content.push_str(&format!(" {NO_SERVICE}")),
            None => {}
        }
        if self.takes_owner_key {
            content.push_str(&format!(" {OWNER_KEY}"));
        }
        if let Some(unit) = &self.unit {
            content.push_str(&format!(" {UNIT}{unit}"));
        }
        match self.predecessor {
            Predecessor::Unsaid => {}
            Predecessor::None => content.push_str(&format!(" {AFTER}{AFTER_NONE}")),
            Predecessor::Daemon(from) => content.push_str(&format!(" {AFTER}{}", from.key())),
        }
        content.push('\n');
        replace_beside(path, content.as_bytes(), Permissions::Private)
    }

    fn read_from(path: &Path) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        let mut tokens = content.split_whitespace();
        let identity = ProcessIdentity::parse(tokens.next()?)?;
        let port = tokens.next()?.parse().ok()?;
        let rest: Vec<&str> = tokens.collect();
        let follows_application = match rest.first().copied() {
            Some(FOLLOWS_APPLICATION) => Some(true),
            Some(ALONE) => Some(false),
            _ => None,
        };
        let service_managed = rest.iter().find_map(|token| match *token {
            SERVICE => Some(true),
            NO_SERVICE => Some(false),
            _ => None,
        });
        let takes_owner_key = rest.contains(&OWNER_KEY);
        let unit = rest
            .iter()
            .find_map(|token| token.strip_prefix(UNIT))
            .filter(|unit| is_unit_name(unit))
            .map(str::to_owned);
        // A value that is neither reads as a record that does not say.
        let predecessor = rest
            .iter()
            .find_map(|token| token.strip_prefix(AFTER))
            .and_then(|after| match after {
                AFTER_NONE => Some(Predecessor::None),
                identity => ProcessIdentity::parse(identity).map(Predecessor::Daemon),
            })
            .unwrap_or(Predecessor::Unsaid);
        Some(Self {
            identity,
            port,
            follows_application,
            service_managed,
            takes_owner_key,
            unit,
            predecessor,
        })
    }

    fn remove_from(&self, path: &Path) -> std::io::Result<()> {
        if Self::read_from(path).as_ref() != Some(self) {
            return Ok(());
        }
        remove_replaced(path)
    }
}

/// Whether a service-managed daemon starting in the unit `unit` takes over from `from`, which
/// still `running` or not (see [`Owner::service_predecessor`]).
fn restarted_in_service(from: &Owner, unit: Option<&str>, running: bool) -> bool {
    let same_unit = match (from.unit.as_deref(), unit) {
        (Some(from), Some(unit)) => from == unit,
        _ => true,
    };
    !running && from.service_managed == Some(true) && same_unit
}

fn record_path() -> PathBuf {
    record_path_in(&crate::paths::data_dir())
}

fn record_path_in(data: &Path) -> PathBuf {
    data.join("daemon.owner")
}

fn handover_path_in(data: &Path) -> PathBuf {
    data.join("daemon.handover")
}

/// Removes the note of a handover ([`Owner::note_handover`]): a daemon that has recorded itself, or
/// would have if it could, does, whichever daemon's note it is, since the commands then find a
/// daemon through the lock and its record. A daemon whose successor did not start removes only its
/// own ([`Owner::withdraw_handover`]).
pub fn settle_handover() {
    settle_handover_in(&crate::paths::data_dir());
}

fn settle_handover_in(data: &Path) {
    if let Err(err) = remove_replaced(&handover_path_in(data)) {
        tracing::warn!(%err, "removing the handover note failed");
    }
}

fn key_path() -> PathBuf {
    key_path_in(&crate::paths::data_dir())
}

fn key_path_in(data: &Path) -> PathBuf {
    data.join("daemon.key")
}

/// The identity a key record leads with, and what follows it.
fn split_key_record(content: &str) -> Option<(ProcessIdentity, &str)> {
    let content = content.trim_start();
    let (identity, rest) = content
        .split_once(char::is_whitespace)
        .unwrap_or((content, ""));
    Some((ProcessIdentity::parse(identity)?, rest))
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
            service_managed: None,
            takes_owner_key: false,
            unit: None,
            predecessor: Predecessor::Unsaid,
        }
    }

    #[test]
    fn the_record_reads_back_and_is_removed_only_by_the_daemon_that_wrote_it() {
        let dir = ScratchDir::new("owner-record");
        let path = dir.join("daemon.owner");
        assert_eq!(Owner::read_from(&path), None);

        let ours = owner(10, 7531);
        ours.write_to(&path).expect("the record was written");
        assert_eq!(Owner::read_from(&path), Some(ours.clone()));

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

    /// A record with no third token, as a daemon that could not install its restart handler
    /// writes, still names the daemon and its port; whether it follows an application is not
    /// known, and neither is a third token of a later build's.
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
            assert_eq!(
                Owner::read_from(&path),
                Some(unknown.clone()),
                "{content:?}"
            );
        }
        // The daemon that wrote such a record removes it on its way out.
        std::fs::write(&path, "10:123456 7531\n").unwrap();
        unknown.remove_from(&path).unwrap();
        assert!(!path.exists());
    }

    /// Whether a service manager starts the successor follows whatever else the record says, and
    /// reads back; so does a daemon's own record, which it then removes.
    #[test]
    fn whether_a_service_manager_starts_the_successor_reads_back() {
        let dir = ScratchDir::new("owner-record-service");
        let path = dir.join("daemon.owner");
        let cases = [
            (Some(false), true, "10:123456 7531 alone service\n"),
            (Some(false), false, "10:123456 7531 alone no-service\n"),
            (Some(true), false, "10:123456 7531 application no-service\n"),
            (None, true, "10:123456 7531 service\n"),
            (None, false, "10:123456 7531 no-service\n"),
        ];
        for (follows_application, managed, expected) in cases {
            let ours = Owner {
                follows_application,
                ..owner(10, 7531)
            }
            .with_service_manager(managed);
            ours.write_to(&path).unwrap();
            assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
            assert_eq!(Owner::read_from(&path), Some(ours.clone()), "{expected:?}");
            ours.remove_from(&path).unwrap();
            assert!(!path.exists(), "{expected:?}");
        }
    }

    /// A record an earlier build wrote, without the token, does not say whether a service manager
    /// starts the successor, and reads otherwise as it always did.
    #[test]
    fn a_record_without_the_service_token_does_not_say() {
        let dir = ScratchDir::new("owner-record-service-old");
        let path = dir.join("daemon.owner");
        for (content, follows_application) in [
            ("10:123456 7531\n", None),
            ("10:123456 7531 alone\n", Some(false)),
            ("10:123456 7531 application\n", Some(true)),
            ("10:123456 7531 alone elsewhere\n", Some(false)),
        ] {
            std::fs::write(&path, content).unwrap();
            let read = Owner::read_from(&path).expect("the record reads");
            assert_eq!(read.service_managed, None, "{content:?}");
            assert_eq!(read.follows_application, follows_application, "{content:?}");
            assert_eq!((read.identity.pid.0, read.port), (10, 7531), "{content:?}");
        }
    }

    /// The owner-key token comes last, reads back wherever it stands after the port, and a record
    /// without it is of a daemon that takes no owner key.
    #[test]
    fn whether_the_daemon_takes_the_owner_key_reads_back() {
        let dir = ScratchDir::new("owner-record-owner-key");
        let path = dir.join("daemon.owner");
        let cases = [
            (
                Some(false),
                Some(true),
                "10:123456 7531 alone service owner-key\n",
            ),
            (
                Some(false),
                Some(false),
                "10:123456 7531 alone no-service owner-key\n",
            ),
            (Some(false), None, "10:123456 7531 alone owner-key\n"),
            (None, None, "10:123456 7531 owner-key\n"),
        ];
        for (follows_application, managed, expected) in cases {
            let mut ours = Owner {
                follows_application,
                ..owner(10, 7531)
            }
            .with_owner_key(true);
            ours.service_managed = managed;
            ours.write_to(&path).unwrap();
            assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
            assert_eq!(Owner::read_from(&path), Some(ours.clone()), "{expected:?}");
            ours.remove_from(&path).unwrap();
            assert!(!path.exists(), "{expected:?}");
        }
        for content in ["10:123456 7531\n", "10:123456 7531 alone service\n"] {
            std::fs::write(&path, content).unwrap();
            assert!(
                !Owner::read_from(&path).unwrap().takes_owner_key,
                "{content:?}"
            );
        }
        assert!(!owner(10, 7531).takes_owner_key);
        assert!(owner(10, 7531).with_owner_key(true).takes_owner_key);
    }

    /// An earlier build reads the identity, the port and the third token alone, so the token
    /// changes nothing it reads, even in place of a missing third token.
    #[test]
    fn an_earlier_build_reads_a_record_with_the_owner_key_token_as_one_without_it() {
        let dir = ScratchDir::new("owner-record-owner-key-earlier");
        let path = dir.join("daemon.owner");
        for follows_application in [Some(true), Some(false), None] {
            for managed in [None, Some(true), Some(false)] {
                let mut ours = Owner {
                    follows_application,
                    ..owner(10, 7531)
                }
                .with_owner_key(true);
                ours.service_managed = managed;
                ours.write_to(&path).unwrap();
                let read = read_as_an_earlier_build(&std::fs::read_to_string(&path).unwrap());
                assert_eq!(read, Some((ours.identity, 7531, follows_application)));
            }
        }
    }

    /// What an earlier build makes of a record: the identity, the port and the third token alone,
    /// a word it does not know there being no third token.
    fn read_as_an_earlier_build(content: &str) -> Option<(ProcessIdentity, u16, Option<bool>)> {
        let mut tokens = content.split_whitespace();
        let identity = ProcessIdentity::parse(tokens.next()?)?;
        let port = tokens.next()?.parse().ok()?;
        let follows_application = match tokens.next() {
            Some("application") => Some(true),
            Some("alone") => Some(false),
            _ => None,
        };
        Some((identity, port, follows_application))
    }

    /// The token changes nothing an earlier build reads of a record, wherever it stands.
    #[test]
    fn an_earlier_build_reads_a_record_with_the_service_token_as_one_without_it() {
        let dir = ScratchDir::new("owner-record-service-earlier");
        let path = dir.join("daemon.owner");
        for follows_application in [Some(true), Some(false), None] {
            let without = Owner {
                follows_application,
                ..owner(10, 7531)
            };
            without.write_to(&path).unwrap();
            let expected = read_as_an_earlier_build(&std::fs::read_to_string(&path).unwrap());
            assert_eq!(
                expected,
                Some((without.identity, 7531, follows_application))
            );
            for managed in [true, false] {
                without
                    .clone()
                    .with_service_manager(managed)
                    .write_to(&path)
                    .unwrap();
                let content = std::fs::read_to_string(&path).unwrap();
                assert_eq!(read_as_an_earlier_build(&content), expected, "{content:?}");
            }
        }
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

    const KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn the_key_record_is_one_owner_only_line_and_reads_back_for_its_daemon() {
        let dir = ScratchDir::new("key-record");
        let path = dir.join("daemon.key");
        let ours = owner(10, 7531);
        assert_eq!(ours.read_key_from(&path), None);

        ours.write_key_to(&path, KEY).expect("the key was recorded");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            format!("10:123456 {KEY}\n")
        );
        assert_eq!(ours.read_key_from(&path).as_deref(), Some(KEY));
        // The port is not part of who the daemon is.
        assert_eq!(owner(10, 7999).read_key_from(&path).as_deref(), Some(KEY));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0, "the key record is readable by others");
        }
    }

    #[test]
    fn a_later_key_record_replaces_the_earlier_one() {
        let dir = ScratchDir::new("key-record-replaced");
        let path = dir.join("daemon.key");
        let other_key = "f".repeat(64);
        owner(10, 7531).write_key_to(&path, KEY).unwrap();
        owner(12, 7540).write_key_to(&path, &other_key).unwrap();
        assert_eq!(
            owner(12, 7540).read_key_from(&path).as_deref(),
            Some(other_key.as_str())
        );
        assert_eq!(owner(10, 7531).read_key_from(&path), None);
    }

    #[test]
    fn a_key_record_of_another_daemon_is_not_this_ones() {
        let dir = ScratchDir::new("key-record-stale");
        let path = dir.join("daemon.key");
        owner(11, 7531).write_key_to(&path, KEY).unwrap();
        assert_eq!(owner(10, 7531).read_key_from(&path), None, "another pid");

        // The same pid started at another time is another process.
        let reused = Owner {
            identity: ProcessIdentity {
                pid: ProcessId(11),
                start_micros: 999,
            },
            ..owner(11, 7531)
        };
        assert_eq!(reused.read_key_from(&path), None, "a reused pid");
    }

    #[test]
    fn an_unusable_key_record_reads_as_no_key() {
        let dir = ScratchDir::new("key-record-garbage");
        let path = dir.join("daemon.key");
        let ours = owner(10, 7531);
        let upper = KEY.to_uppercase();
        let short = &KEY[..63];
        let long = format!("{KEY}0");
        let non_hex = format!("{}g", &KEY[..63]);
        let contents = [
            String::new(),
            "garbage".to_owned(),
            "10:123456".to_owned(),
            "10:123456\n".to_owned(),
            format!("{KEY}\n"),
            format!("10:123456 {upper}\n"),
            format!("10:123456 {short}\n"),
            format!("10:123456 {long}\n"),
            format!("10:123456 {non_hex}\n"),
            format!("10:123456 {KEY} extra\n"),
            format!("10:123456 {KEY} {KEY}\n"),
            format!("notanidentity {KEY}\n"),
        ];
        for content in contents {
            std::fs::write(&path, &content).unwrap();
            assert_eq!(ours.read_key_from(&path), None, "{content:?}");
        }
    }

    #[test]
    fn the_key_record_is_removed_only_by_the_daemon_that_wrote_it() {
        let dir = ScratchDir::new("key-record-removed");
        let path = dir.join("daemon.key");
        let ours = owner(10, 7531);
        // Nothing recorded: nothing to do.
        ours.remove_key_from(&path).unwrap();

        ours.write_key_to(&path, KEY).unwrap();
        owner(11, 7531).remove_key_from(&path).unwrap();
        assert!(path.exists(), "another daemon's record was removed");
        assert_eq!(ours.read_key_from(&path).as_deref(), Some(KEY));

        ours.remove_key_from(&path).unwrap();
        assert!(!path.exists());
        // Already gone: nothing to do.
        ours.remove_key_from(&path).unwrap();

        // A record that is not one is left alone.
        std::fs::write(&path, "garbage").unwrap();
        ours.remove_key_from(&path).unwrap();
        assert!(path.exists());

        // A record of ours whose key is malformed is ours to remove all the same.
        std::fs::write(&path, "10:123456 notakey\n").unwrap();
        ours.remove_key_from(&path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn the_owner_record_never_holds_the_key() {
        let dir = ScratchDir::new("key-record-apart");
        let owner_path = dir.join("daemon.owner");
        let key_file = dir.join("daemon.key");
        let ours = owner(10, 7531);
        ours.write_key_to(&key_file, KEY).unwrap();
        ours.write_to(&owner_path).unwrap();

        let record = std::fs::read_to_string(&owner_path).unwrap();
        assert_eq!(record, "10:123456 7531 alone\n");
        assert_ne!(record_path().file_name(), key_path().file_name());
        assert_eq!(Owner::read_from(&owner_path), Some(ours));
    }

    /// A daemon's note of a handover reads back as its record, with when it was written, whether
    /// or not its process still runs; a note is no owner record, and goes once settled.
    #[test]
    fn a_handover_note_reads_back_until_it_is_settled() {
        let dir = ScratchDir::new("handover-note");
        let data = dir.join(".octoboard");
        std::fs::create_dir_all(&data).unwrap();
        assert_eq!(Owner::handover_noted_in(&data), None);
        // Nothing noted: nothing to do.
        settle_handover_in(&data);

        let before = SystemTime::now() - std::time::Duration::from_secs(2);
        let gone = owner(10, 7531).with_service_manager(true);
        gone.note_handover_in(&data).unwrap();
        let noted = Owner::handover_noted_in(&data).expect("the note");
        assert_eq!(noted.from, gone);
        assert!(noted.noted >= before, "{:?}", noted.noted);
        assert_eq!(Owner::recorded_in(&data), None, "a note is no owner record");

        settle_handover_in(&data);
        assert_eq!(Owner::handover_noted_in(&data), None);
    }

    /// A daemon whose successor did not start withdraws its own note, and leaves another daemon's.
    #[test]
    fn a_daemon_withdraws_only_its_own_handover_note() {
        let dir = ScratchDir::new("handover-withdraw");
        let data = dir.join(".octoboard");
        std::fs::create_dir_all(&data).unwrap();
        let ours = owner(10, 7531);
        let theirs = owner(11, 7532);
        theirs.note_handover_in(&data).unwrap();
        ours.withdraw_handover_in(&data);
        assert_eq!(
            Owner::handover_noted_in(&data).map(|note| note.from),
            Some(theirs.clone())
        );
        theirs.withdraw_handover_in(&data);
        assert_eq!(Owner::handover_noted_in(&data), None);
    }

    /// What another process reads of a data directory it is handed: the owner and its key while
    /// the process the record names runs, and nothing once it is gone.
    #[test]
    fn the_records_read_back_from_the_data_directory_they_are_in() {
        let dir = ScratchDir::new("owner-record-in");
        let data = dir.join(".octoboard");
        assert_eq!(Owner::recorded_in(&data), None);
        let ours = Owner::current(7531, false, true).expect("this process's start time");
        ours.record_in(&data, KEY).unwrap();
        assert_eq!(Owner::recorded_in(&data), Some(ours.clone()));
        assert_eq!(ours.recorded_key_in(&data).as_deref(), Some(KEY));

        let gone = Owner {
            identity: ProcessIdentity {
                start_micros: ours.identity.start_micros.wrapping_add(1),
                ..ours.identity
            },
            ..ours
        };
        gone.record_in(&data, KEY).unwrap();
        assert_eq!(Owner::recorded_in(&data), None, "a process that is gone");
    }

    #[test]
    fn this_process_is_a_running_owner() {
        let ours = Owner::current(7531, false, true).expect("this process's start time");
        assert_eq!(ours.identity.pid.0, std::process::id());
        assert!(ours.identity.is_running());
    }

    /// The process says whether a service manager starts its successor only when told, and the
    /// rest of what it records stays as it was.
    #[test]
    fn whether_a_service_manager_starts_the_successor_is_said_only_when_told() {
        let ours = Owner::current(7531, true, true).expect("this process's start time");
        assert_eq!(ours.service_managed, None);
        for managed in [true, false] {
            let told = ours.clone().with_service_manager(managed);
            assert_eq!(told.service_managed, Some(managed));
            assert_eq!(
                (told.identity, told.port, told.follows_application),
                (ours.identity, 7531, Some(true))
            );
        }
    }

    fn identity(pid: u32, start_micros: u64) -> ProcessIdentity {
        ProcessIdentity {
            pid: ProcessId(pid),
            start_micros,
        }
    }

    /// An identity no process has: this process's pid, started at another time.
    fn gone_identity(offset: u64) -> ProcessIdentity {
        let this = own_identity().expect("this process's start time");
        identity(this.pid.0, this.start_micros.wrapping_add(offset))
    }

    /// The unit and predecessor tokens come after everything else, read back whatever else the
    /// record says, in every combination, and each daemon removes its own record.
    #[test]
    fn the_unit_and_predecessor_tokens_read_back_with_everything_else() {
        let dir = ScratchDir::new("owner-record-succession");
        let path = dir.join("daemon.owner");
        let predecessors = [
            Predecessor::Unsaid,
            Predecessor::None,
            Predecessor::Daemon(identity(9, 77)),
        ];
        for follows_application in [Some(true), Some(false), None] {
            for managed in [None, Some(true), Some(false)] {
                for takes_owner_key in [false, true] {
                    for unit in [None, Some("octoboard.service".to_owned())] {
                        for predecessor in predecessors {
                            let ours = Owner {
                                follows_application,
                                service_managed: managed,
                                takes_owner_key,
                                unit: unit.clone(),
                                predecessor,
                                ..owner(10, 7531)
                            };
                            ours.write_to(&path).unwrap();
                            let content = std::fs::read_to_string(&path).unwrap();
                            assert_eq!(Owner::read_from(&path), Some(ours.clone()), "{content:?}");
                            ours.remove_from(&path).unwrap();
                            assert!(!path.exists(), "{content:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_unit_and_predecessor_are_written_as_tokens_after_the_others() {
        let dir = ScratchDir::new("owner-record-succession-text");
        let path = dir.join("daemon.owner");
        let cases = [
            (
                owner(10, 7531)
                    .with_service_manager(true)
                    .with_owner_key(true)
                    .with_unit(Some("octoboard.service".to_owned()))
                    .with_predecessor(Predecessor::Daemon(identity(9, 77))),
                "10:123456 7531 alone service owner-key unit=octoboard.service after=9:77\n",
            ),
            (
                owner(10, 7531).with_predecessor(Predecessor::None),
                "10:123456 7531 alone after=none\n",
            ),
            (
                owner(10, 7531).with_unit(Some("a@b.service".to_owned())),
                "10:123456 7531 alone unit=a@b.service\n",
            ),
            (owner(10, 7531), "10:123456 7531 alone\n"),
        ];
        for (ours, expected) in cases {
            ours.write_to(&path).unwrap();
            assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
        }
    }

    /// The tokens are read wherever they stand after the port, in either order, and with no third
    /// token they stand third.
    #[test]
    fn the_unit_and_predecessor_are_read_wherever_they_stand_after_the_port() {
        let dir = ScratchDir::new("owner-record-succession-order");
        let path = dir.join("daemon.owner");
        let cases = [
            // `alone` counts only as the third token, as every build reads it.
            ("10:123456 7531 after=9:77 unit=u.service alone", None),
            (
                "10:123456 7531 alone after=9:77 service unit=u.service",
                Some(false),
            ),
            ("10:123456 7531 unit=u.service after=9:77", None),
            ("10:123456 7531 after=9:77 unit=u.service\n", None),
        ];
        for (content, follows_application) in cases {
            std::fs::write(&path, content).unwrap();
            let read = Owner::read_from(&path).expect("the record reads");
            assert_eq!(read.unit.as_deref(), Some("u.service"), "{content:?}");
            assert_eq!(
                read.predecessor,
                Predecessor::Daemon(identity(9, 77)),
                "{content:?}"
            );
            assert_eq!(read.follows_application, follows_application, "{content:?}");
            assert_eq!((read.identity, read.port), (identity(10, 123_456), 7531));
        }
        std::fs::write(&path, "10:123456 7531 after=none").unwrap();
        let read = Owner::read_from(&path).unwrap();
        assert_eq!(
            (read.follows_application, read.predecessor, read.unit),
            (None, Predecessor::None, None)
        );
    }

    /// A record written without the tokens, in any earlier format, has no unit and does not say
    /// which daemon it followed.
    #[test]
    fn a_record_without_the_tokens_has_no_unit_and_an_unsaid_predecessor() {
        let dir = ScratchDir::new("owner-record-succession-old");
        let path = dir.join("daemon.owner");
        for content in [
            "10:123456 7531\n",
            "10:123456 7531 elsewhere\n",
            "10:123456 7531 alone\n",
            "10:123456 7531 application no-service\n",
            "10:123456 7531 alone service owner-key\n",
            "10:123456 7531 service\n",
        ] {
            std::fs::write(&path, content).unwrap();
            let read = Owner::read_from(&path).expect("the record reads");
            assert_eq!(read.unit, None, "{content:?}");
            assert_eq!(read.predecessor, Predecessor::Unsaid, "{content:?}");
        }
    }

    /// An earlier build reads the identity, the port and the third token alone, so neither token
    /// changes what it reads, whether it stands fourth or, with no third token, third.
    #[test]
    fn an_earlier_build_reads_a_record_with_the_unit_and_predecessor_as_one_without_them() {
        let dir = ScratchDir::new("owner-record-succession-earlier");
        let path = dir.join("daemon.owner");
        let predecessors = [
            Predecessor::Unsaid,
            Predecessor::None,
            Predecessor::Daemon(identity(9, 77)),
        ];
        for follows_application in [Some(true), Some(false), None] {
            for managed in [None, Some(true), Some(false)] {
                for takes_owner_key in [false, true] {
                    for unit in [None, Some("octoboard.service".to_owned())] {
                        for predecessor in predecessors {
                            let ours = Owner {
                                follows_application,
                                service_managed: managed,
                                takes_owner_key,
                                unit: unit.clone(),
                                predecessor,
                                ..owner(10, 7531)
                            };
                            ours.write_to(&path).unwrap();
                            let content = std::fs::read_to_string(&path).unwrap();
                            assert_eq!(
                                read_as_an_earlier_build(&content),
                                Some((ours.identity, 7531, follows_application)),
                                "{content:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    /// A unit name that is not one is dropped, on write and on read, and the rest of the record is
    /// kept.
    #[test]
    fn a_unit_name_that_is_not_one_is_dropped_on_write_and_on_read() {
        let dir = ScratchDir::new("owner-record-unit-name");
        let path = dir.join("daemon.owner");
        let longest = "a".repeat(256);
        let too_long = "a".repeat(257);
        for unit in [
            "",
            "has/slash",
            "has$dollar",
            "has=equals",
            "ünit.service",
            too_long.as_str(),
        ] {
            let ours = owner(10, 7531).with_unit(Some(unit.to_owned()));
            assert_eq!(ours.unit, None, "{unit:?}");
            ours.write_to(&path).unwrap();
            assert_eq!(
                std::fs::read_to_string(&path).unwrap(),
                "10:123456 7531 alone\n",
                "{unit:?}"
            );

            std::fs::write(
                &path,
                format!("10:123456 7531 alone service unit={unit} after=none\n"),
            )
            .unwrap();
            let read = Owner::read_from(&path).expect("the rest of the record reads");
            assert_eq!(read.unit, None, "{unit:?}");
            assert_eq!(read.service_managed, Some(true), "{unit:?}");
            assert_eq!(read.predecessor, Predecessor::None, "{unit:?}");
        }
        for unit in ["octoboard.service", "a:b-c_d.e\\f@g", longest.as_str()] {
            let ours = owner(10, 7531).with_unit(Some(unit.to_owned()));
            assert_eq!(ours.unit.as_deref(), Some(unit));
            ours.write_to(&path).unwrap();
            assert_eq!(Owner::read_from(&path), Some(ours), "{unit:?}");
        }
        assert_eq!(owner(10, 7531).with_unit(None).unit, None);
    }

    /// A predecessor value that is neither `none` nor an identity reads as not said, and the rest
    /// of the record is kept.
    #[test]
    fn a_malformed_predecessor_reads_as_unsaid() {
        let dir = ScratchDir::new("owner-record-after-malformed");
        let path = dir.join("daemon.owner");
        for value in [
            "", "garbage", "NONE", "0:5", "9", "9:", "9:x", ":5", "-1:5", "9:77:3",
        ] {
            std::fs::write(
                &path,
                format!("10:123456 7531 alone unit=u.service after={value}\n"),
            )
            .unwrap();
            let read = Owner::read_from(&path).expect("the rest of the record reads");
            assert_eq!(read.predecessor, Predecessor::Unsaid, "{value:?}");
            assert_eq!(read.unit.as_deref(), Some("u.service"), "{value:?}");
            assert_eq!(read.follows_application, Some(false), "{value:?}");
        }
    }

    #[test]
    fn a_predecessor_is_set_without_changing_the_rest() {
        let ours = owner(10, 7531).with_service_manager(true);
        let followed = ours
            .clone()
            .with_predecessor(Predecessor::Daemon(identity(9, 77)));
        assert_eq!(followed.predecessor, Predecessor::Daemon(identity(9, 77)));
        assert_eq!(
            Owner {
                predecessor: Predecessor::Unsaid,
                ..followed
            },
            ours
        );
    }

    #[test]
    fn this_process_has_an_identity_that_runs() {
        let ours = own_identity().expect("this process's start time");
        assert_eq!(ours.pid.0, std::process::id());
        assert!(ours.is_running());
    }

    /// A daemon is taken over from in a service only if it was service-managed, has ended, and, where
    /// both name a unit, ran in the same one.
    #[test]
    fn a_service_restarts_only_a_service_managed_daemon_that_has_ended_in_its_unit() {
        let managed = owner(10, 7531).with_service_manager(true);
        let unmanaged = owner(10, 7531).with_service_manager(false);
        let unsaid = owner(10, 7531);
        let in_unit = |owner: &Owner, unit: &str| owner.clone().with_unit(Some(unit.to_owned()));
        assert!(restarted_in_service(&managed, None, false));
        assert!(restarted_in_service(&managed, Some("a.service"), false));
        assert!(restarted_in_service(
            &in_unit(&managed, "a.service"),
            None,
            false
        ));
        assert!(restarted_in_service(
            &in_unit(&managed, "a.service"),
            Some("a.service"),
            false
        ));
        assert!(!restarted_in_service(
            &in_unit(&managed, "a.service"),
            Some("b.service"),
            false
        ));
        assert!(!restarted_in_service(&managed, None, true));
        assert!(!restarted_in_service(
            &in_unit(&managed, "a.service"),
            Some("a.service"),
            true
        ));
        assert!(!restarted_in_service(&unmanaged, None, false));
        assert!(!restarted_in_service(&unsaid, None, false));
    }

    /// A data directory holding the note of a handover and the owner record, as given.
    fn data_with(note: Option<&Owner>, record: Option<&Owner>) -> (ScratchDir, PathBuf) {
        let dir = ScratchDir::new("service-predecessor");
        let data = dir.join(".octoboard");
        std::fs::create_dir_all(&data).unwrap();
        if let Some(note) = note {
            note.note_handover_in(&data).unwrap();
        }
        if let Some(record) = record {
            record.write_to(&record_path_in(&data)).unwrap();
        }
        (dir, data)
    }

    fn serviced(identity: ProcessIdentity) -> Owner {
        Owner {
            identity,
            ..owner(10, 7531)
        }
        .with_service_manager(true)
    }

    /// The daemon a note names, which handed over and has ended, is the predecessor of the daemon
    /// a service manager starts.
    #[test]
    fn a_service_managed_daemon_that_handed_over_and_ended_is_the_predecessor() {
        let gone = serviced(gone_identity(1));
        let (_dir, data) = data_with(Some(&gone), None);
        assert_eq!(
            Owner::service_predecessor_in(&data, Some("a.service")),
            Predecessor::Daemon(gone.identity)
        );
        assert_eq!(
            Owner::service_predecessor_in(&data, None),
            Predecessor::Daemon(gone.identity)
        );
    }

    /// A daemon that crashed leaves its record and no note: it is the predecessor all the same.
    #[test]
    fn a_service_managed_daemon_that_left_its_record_is_the_predecessor() {
        let crashed = serviced(gone_identity(1));
        let (_dir, data) = data_with(None, Some(&crashed));
        assert_eq!(
            Owner::service_predecessor_in(&data, None),
            Predecessor::Daemon(crashed.identity)
        );
    }

    #[test]
    fn with_nothing_recorded_a_service_managed_daemon_does_not_say() {
        let (_dir, data) = data_with(None, None);
        assert_eq!(
            Owner::service_predecessor_in(&data, Some("a.service")),
            Predecessor::Unsaid
        );
        // Not even a data directory.
        let dir = ScratchDir::new("service-predecessor-none");
        assert_eq!(
            Owner::service_predecessor_in(&dir.join("absent"), None),
            Predecessor::Unsaid
        );
    }

    /// A daemon still running, one that was not service-managed (or does not say), and one of
    /// another unit do not qualify, whether the note or the record names them.
    #[test]
    fn a_daemon_that_still_runs_or_is_not_service_managed_or_is_of_another_unit_is_not_named() {
        let own = own_identity().expect("this process's start time");
        let in_unit = |owner: Owner, unit: &str| owner.with_unit(Some(unit.to_owned()));
        let disqualified = [
            ("running", serviced(own)),
            (
                "not service-managed",
                serviced(gone_identity(1)).with_service_manager(false),
            ),
            (
                "unsaid",
                Owner {
                    service_managed: None,
                    ..serviced(gone_identity(1))
                },
            ),
            (
                "another unit",
                in_unit(serviced(gone_identity(1)), "other.service"),
            ),
        ];
        for (why, from) in disqualified {
            let (_dir, data) = data_with(Some(&from), None);
            assert_eq!(
                Owner::service_predecessor_in(&data, Some("a.service")),
                Predecessor::Unsaid,
                "note: {why}"
            );
            let (_dir, data) = data_with(None, Some(&from));
            assert_eq!(
                Owner::service_predecessor_in(&data, Some("a.service")),
                Predecessor::Unsaid,
                "record: {why}"
            );
        }
    }

    /// Where only one of the two names a unit, or both name the same, the unit does not disqualify.
    #[test]
    fn a_unit_disqualifies_only_where_both_name_one_and_they_differ() {
        let gone = serviced(gone_identity(1));
        let in_unit = gone.clone().with_unit(Some("a.service".to_owned()));
        for (from, unit) in [
            (&in_unit, Some("a.service")),
            (&in_unit, None),
            (&gone, Some("a.service")),
        ] {
            let (_dir, data) = data_with(Some(from), None);
            assert_eq!(
                Owner::service_predecessor_in(&data, unit),
                Predecessor::Daemon(gone.identity),
                "{from:?} {unit:?}"
            );
        }
    }

    /// The note is preferred over the record: both name an ended service-managed daemon, and the
    /// note's is the predecessor.
    #[test]
    fn the_note_is_preferred_over_the_record() {
        let noted = serviced(gone_identity(1));
        let recorded = serviced(gone_identity(2));
        let (_dir, data) = data_with(Some(&noted), Some(&recorded));
        assert_eq!(
            Owner::service_predecessor_in(&data, None),
            Predecessor::Daemon(noted.identity)
        );
    }

    /// A note that does not qualify falls through to the record, which does.
    #[test]
    fn a_note_that_does_not_qualify_falls_through_to_the_record() {
        let own = own_identity().expect("this process's start time");
        let recorded = serviced(gone_identity(2));
        let unqualified = [
            serviced(own),
            serviced(gone_identity(1)).with_service_manager(false),
            serviced(gone_identity(1)).with_unit(Some("other.service".to_owned())),
        ];
        for noted in unqualified {
            let (_dir, data) = data_with(Some(&noted), Some(&recorded));
            assert_eq!(
                Owner::service_predecessor_in(&data, Some("a.service")),
                Predecessor::Daemon(recorded.identity),
                "{noted:?}"
            );
        }
        // And when neither qualifies, it does not say.
        let (_dir, data) = data_with(Some(&serviced(own)), Some(&serviced(own)));
        assert_eq!(
            Owner::service_predecessor_in(&data, None),
            Predecessor::Unsaid
        );
    }
}
