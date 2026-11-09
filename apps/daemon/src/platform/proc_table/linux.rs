//! The process table on Linux, from `/proc`.

use std::collections::{HashMap, HashSet};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use super::{Cwd, EnvRead, ProcInfo};
use crate::platform::proc::ContainerId;

/// What `/proc/<pid>/stat` says of a process.
struct Stat {
    comm: String,
    ppid: i32,
    sid: i32,
    /// Clock ticks since boot.
    start_ticks: u64,
    zombie: bool,
}

/// Reads `/proc/<pid>/stat`. The command name in parentheses may hold spaces and parentheses
/// itself, so the fields are counted from the last `)`.
fn stat(pid: i32) -> Option<Stat> {
    let text = std::fs::read(format!("/proc/{pid}/stat")).ok()?;
    let before = text.iter().position(|&byte| byte == b'(')?;
    let after = text.iter().rposition(|&byte| byte == b')')?;
    let rest = std::str::from_utf8(&text[after + 1..]).ok()?;
    // From field 3 (`state`) on: state, ppid, pgrp, session, … with `starttime` field 22.
    let fields: Vec<&str> = rest.split_ascii_whitespace().collect();
    Some(Stat {
        comm: String::from_utf8_lossy(text.get(before + 1..after)?).into_owned(),
        zombie: matches!(fields.first(), Some(&"Z") | Some(&"X")),
        ppid: fields.get(1)?.parse().ok()?,
        sid: fields.get(3)?.parse().ok()?,
        start_ticks: fields.get(19)?.parse().ok()?,
    })
}

/// Clock ticks per second, what a start time in ticks since boot is turned into microseconds
/// with. Read once: it does not change while the system runs.
fn ticks_per_second() -> Option<u64> {
    static TICKS: OnceLock<Option<u64>> = OnceLock::new();
    *TICKS.get_or_init(|| {
        // SAFETY: `sysconf` only reads.
        let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
        (ticks > 0).then_some(ticks as u64)
    })
}

/// A start time in ticks since boot, in microseconds since boot: the identity's start. Not turned
/// into one since the epoch, since the boot time the system gives (`btime` in `/proc/stat`) is
/// worked out from the wall clock and moves whenever that is stepped, which would change every
/// identity from one run of the daemon to the next.
fn start_micros(start_ticks: u64) -> Option<u64> {
    Some(start_ticks * 1_000_000 / ticks_per_second()?)
}

/// `clock` now, in microseconds, or 0 when it cannot be read.
fn clock_micros(clock: libc::clockid_t) -> u64 {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `clock_gettime` writes only the `timespec` it is given.
    if unsafe { libc::clock_gettime(clock, &mut now) } != 0 {
        return 0;
    }
    now.tv_sec as u64 * 1_000_000 + now.tv_nsec as u64 / 1_000
}

/// Now on the clock start times are on: since boot, the time asleep included, as the kernel
/// counts a process's start.
pub(crate) fn now() -> u64 {
    clock_micros(libc::CLOCK_BOOTTIME)
}

/// A start time in microseconds since the epoch, for showing: the wall clock now, less how long
/// ago the process started.
pub(crate) fn epoch_micros(start: u64) -> u64 {
    clock_micros(libc::CLOCK_REALTIME).saturating_sub(now().saturating_sub(start))
}

/// What tells this boot of the system from any other, since start times count from boot.
pub(crate) fn boot_id() -> Option<String> {
    let id = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").ok()?;
    let id = id.trim();
    (!id.is_empty()).then(|| id.to_string())
}

/// The running processes of the daemon's own effective user, zombies left out, or `None` when
/// the pids cannot be listed.
pub(crate) fn list() -> Option<Vec<ProcInfo>> {
    let pids = super::pids::all()?;
    // SAFETY: `geteuid` only reads.
    let uid = unsafe { libc::geteuid() };
    Some(
        pids.into_iter()
            .filter_map(|pid| {
                // `/proc/<pid>` belongs to the process's effective user.
                if std::fs::symlink_metadata(format!("/proc/{pid}"))
                    .ok()?
                    .uid()
                    != uid
                {
                    return None;
                }
                let stat = stat(pid)?;
                if stat.zombie {
                    return None;
                }
                Some(ProcInfo {
                    pid,
                    start: start_micros(stat.start_ticks)?,
                    ppid: stat.ppid,
                    sid: Some(ContainerId(stat.sid as u32)),
                    comm: stat.comm,
                })
            })
            .collect(),
    )
}

/// When the process `pid` started, or `None` when there is none or it is a zombie.
pub(crate) fn start_of(pid: i32) -> Option<u64> {
    let stat = stat(pid)?;
    if stat.zombie {
        return None;
    }
    start_micros(stat.start_ticks)
}

/// The pid of the parent of the process `pid`, or `None` when there is no such process or it is a
/// zombie.
pub(crate) fn parent_of(pid: i32) -> Option<i32> {
    let stat = stat(pid)?;
    (!stat.zombie).then_some(stat.ppid)
}

/// Whether a process that is not a zombie has the pid `pid`, whichever user it runs as: every
/// user can read any process's `stat`.
pub(crate) fn runs_as_anyone(pid: i32) -> bool {
    stat(pid).is_some_and(|stat| !stat.zombie)
}

/// `pid`'s environment block, `KEY=value` entries each NUL-terminated, or unreadable when it
/// cannot be read.
pub(crate) fn environment(pid: i32) -> EnvRead {
    match std::fs::read(format!("/proc/{pid}/environ")) {
        Ok(block) => EnvRead::Readable(block),
        Err(_) => EnvRead::Unreadable,
    }
}

/// `pid`'s arguments, or `None` when they cannot be read.
pub(crate) fn argv(pid: i32) -> Option<Vec<String>> {
    let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    Some(
        cmdline
            .split(|&byte| byte == 0)
            .filter(|arg| !arg.is_empty())
            .map(|arg| String::from_utf8_lossy(arg).into_owned())
            .collect(),
    )
}

/// What the kernel reports of a process's working directory, read without looking at the
/// directory itself: what its `/proc` link reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CwdReport {
    pid: i32,
    read: PathBuf,
}

/// Reading a [`CwdReport`] does not block on the directory's file system: the kernel writes the
/// link's text from what it holds of the path, without asking that file system anything.
pub const CWD_REPORT_MAY_BLOCK: bool = false;

/// The suffix the `cwd` link reads after the path of a removed directory.
const DELETED: &[u8] = b" (deleted)";

/// What the kernel reports of `pid`'s working directory, or `None` when it cannot be read.
pub(crate) fn cwd_report(pid: i32) -> Option<CwdReport> {
    let read = std::fs::read_link(format!("/proc/{pid}/cwd")).ok()?;
    Some(CwdReport { pid, read })
}

impl CwdReport {
    /// A report of a directory at `path`, for a test.
    #[cfg(test)]
    pub(crate) fn at(path: impl Into<PathBuf>) -> Self {
        Self {
            pid: 0,
            read: path.into(),
        }
    }

    /// The path the link reads, which is where the directory is unless it was removed.
    pub fn path(&self) -> &Path {
        &self.read
    }

    /// The paths the directory may be at, by the report alone: what the link reads, and that
    /// without the suffix of a removed directory, which a directory could also be named.
    pub(super) fn candidates(&self) -> impl Iterator<Item = &Path> {
        let stripped = self
            .read
            .as_os_str()
            .as_bytes()
            .strip_suffix(DELETED)
            .map(|stripped| Path::new(std::ffi::OsStr::from_bytes(stripped)));
        std::iter::once(self.read.as_path()).chain(stripped)
    }

    /// The directory, looked at through the file system to tell whether it was removed.
    /// **Blocks** on the file system the directory is on, which may not answer (a dead network
    /// mount).
    ///
    /// Removed or not is told from the directory itself, reached through the link, which has no
    /// link left once removed: the path the link reads is the one in the process's own mount
    /// namespace, which may lead elsewhere, or nowhere, in the daemon's. The link reads
    /// ` (deleted)` after the path of a removed directory, which a directory could also be named;
    /// so the suffix is taken off only for a removed one.
    pub fn looked(&self) -> Option<Cwd> {
        let link = PathBuf::from(format!("/proc/{}/cwd", self.pid));
        let missing = std::fs::metadata(&link).ok()?.nlink() == 0;
        let path = match self.read.as_os_str().as_bytes().strip_suffix(DELETED) {
            Some(stripped) if missing => PathBuf::from(std::ffi::OsStr::from_bytes(stripped)),
            _ => self.read.clone(),
        };
        Some(Cwd { path, missing })
    }
}

/// How much memory `pid` holds resident, in bytes, or `None` when it cannot be read.
pub(crate) fn memory(pid: i32) -> Option<u64> {
    let statm = std::fs::read_to_string(format!("/proc/{pid}/statm")).ok()?;
    let resident: u64 = statm.split_ascii_whitespace().nth(1)?.parse().ok()?;
    // SAFETY: `sysconf` only reads.
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    (page > 0).then_some(resident * page as u64)
}

/// A reader of each process's children, for a walk down the tree from `top`: from the kernel's
/// list when this kernel keeps one, decided once from `top`'s own entry, and from a single pass
/// over `/proc` otherwise, which answers `None` when the pass failed.
pub(crate) fn children_lister(top: i32) -> impl FnMut(i32) -> Option<Vec<i32>> {
    // The list is absent on a kernel built without `CONFIG_PROC_CHILDREN`.
    let listed = std::fs::read_to_string(format!("/proc/{top}/task/{top}/children")).is_ok();
    let mut by_parent = None;
    move |pid| {
        if listed {
            Some(listed_children(pid))
        } else {
            by_parent
                .get_or_insert_with(children_by_parent)
                .as_ref()
                .map(|map| map.get(&pid).cloned().unwrap_or_default())
        }
    }
}

/// The kernel's own list of a process's children, read for each of its threads, since a program
/// may start a child from any of them. A thread that exited since the listing is skipped.
fn listed_children(pid: i32) -> Vec<i32> {
    let Ok(tasks) = std::fs::read_dir(format!("/proc/{pid}/task")) else {
        return Vec::new();
    };
    tasks
        .flatten()
        .filter_map(|task| std::fs::read_to_string(task.path().join("children")).ok())
        .flat_map(|children| {
            children
                .split_whitespace()
                .filter_map(|child| child.parse::<i32>().ok())
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Every process's children, zombies among them, from the parent each one's `stat` names; `None`
/// when the processes could not be listed.
fn children_by_parent() -> Option<HashMap<i32, Vec<i32>>> {
    let mut by_parent = HashMap::<i32, Vec<i32>>::new();
    for child in super::pids::all()? {
        if let Some(stat) = stat(child) {
            by_parent.entry(stat.ppid).or_default().push(child);
        }
    }
    Some(by_parent)
}

/// The path of the file `pid` has open as descriptor `fd`, as its `/proc` link reads (`socket:[N]`
/// for a socket), and the system's error when it could not be read (no such process or
/// descriptor, or a refusal for another user's process).
pub(crate) fn fd_path(pid: i32, fd: i32) -> std::io::Result<Option<PathBuf>> {
    std::fs::read_link(format!("/proc/{pid}/fd/{fd}")).map(Some)
}

/// The TCP ports any of `pids` listens on, in no order; a process whose descriptors cannot be read
/// adds nothing. The listening TCP sockets of the system, by inode, from `/proc/net/tcp` and
/// `tcp6`; then the processes' descriptors that are one of them.
pub(crate) fn listening_ports(pids: &[i32]) -> Vec<u16> {
    let mut listening = HashMap::new();
    for table in ["/proc/net/tcp", "/proc/net/tcp6"] {
        if let Ok(text) = std::fs::read_to_string(table) {
            listening.extend(listening_sockets(&text));
        }
    }
    if listening.is_empty() {
        return Vec::new();
    }
    let mut ports = HashSet::new();
    for &pid in pids {
        for inode in socket_inodes(Path::new(&format!("/proc/{pid}/fd"))) {
            if let Some(&port) = listening.get(&inode) {
                ports.insert(port);
            }
        }
    }
    ports.into_iter().collect()
}

/// The inode and local port of each listening socket in one of `/proc/net/tcp*`: rows of
/// `sl local_address rem_address st … inode`, the port in hex after the address's colon and
/// `0A` the listening state.
fn listening_sockets(text: &str) -> impl Iterator<Item = (u64, u16)> + '_ {
    text.lines().skip(1).filter_map(|line| {
        let fields: Vec<&str> = line.split_ascii_whitespace().collect();
        if fields.get(3)? != &"0A" {
            return None;
        }
        let port = u16::from_str_radix(fields.get(1)?.rsplit_once(':')?.1, 16).ok()?;
        let inode = fields.get(9)?.parse().ok()?;
        Some((inode, port))
    })
}

/// The inodes of the sockets among the descriptors in `fd_dir`, whose links read `socket:[N]`.
fn socket_inodes(fd_dir: &Path) -> Vec<u64> {
    let Ok(entries) = std::fs::read_dir(fd_dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let target = std::fs::read_link(entry.path()).ok()?;
            let target = target.to_str()?;
            target
                .strip_prefix("socket:[")?
                .strip_suffix(']')?
                .parse()
                .ok()
        })
        .collect()
}
