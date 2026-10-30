//! The process table on Linux, from `/proc`.

use std::collections::HashSet;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use super::{Cwd, Identity, Proc};

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

/// Microseconds since the epoch at boot, and clock ticks per second: what a start time in ticks
/// since boot is turned into one since the epoch with. Read once: neither changes while the
/// system runs.
fn clock() -> Option<(u64, u64)> {
    static CLOCK: OnceLock<Option<(u64, u64)>> = OnceLock::new();
    *CLOCK.get_or_init(|| {
        let stat = std::fs::read_to_string("/proc/stat").ok()?;
        let boot: u64 = stat
            .lines()
            .find_map(|line| line.strip_prefix("btime "))?
            .trim()
            .parse()
            .ok()?;
        // SAFETY: `sysconf` only reads.
        let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
        (ticks > 0).then_some((boot * 1_000_000, ticks as u64))
    })
}

fn start_micros(start_ticks: u64) -> Option<u64> {
    let (boot, ticks) = clock()?;
    Some(boot + start_ticks * 1_000_000 / ticks)
}

pub(super) fn list() -> Option<Vec<Proc>> {
    let entries = std::fs::read_dir("/proc").ok()?;
    // SAFETY: `geteuid` only reads.
    let uid = unsafe { libc::geteuid() };
    Some(
        entries
            .flatten()
            .filter_map(|entry| {
                let pid: i32 = entry.file_name().to_str()?.parse().ok()?;
                // `/proc/<pid>` belongs to the process's effective user.
                if entry.metadata().ok()?.uid() != uid {
                    return None;
                }
                let stat = stat(pid)?;
                if stat.zombie {
                    return None;
                }
                Some(Proc {
                    id: Identity {
                        pid,
                        start: start_micros(stat.start_ticks)?,
                    },
                    ppid: stat.ppid,
                    sid: stat.sid,
                    comm: stat.comm,
                })
            })
            .collect(),
    )
}

pub(super) fn start_of(pid: i32) -> Option<u64> {
    let stat = stat(pid)?;
    if stat.zombie {
        return None;
    }
    start_micros(stat.start_ticks)
}

/// Whether a process that is not a zombie has the pid `pid`, whichever user it runs as: every
/// user can read any process's `stat`.
pub(super) fn runs_as_anyone(pid: i32) -> bool {
    stat(pid).is_some_and(|stat| !stat.zombie)
}

pub(super) fn environment(pid: i32) -> Option<Vec<u8>> {
    std::fs::read(format!("/proc/{pid}/environ")).ok()
}

pub(super) fn argv(pid: i32) -> Option<Vec<String>> {
    let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    Some(
        cmdline
            .split(|&byte| byte == 0)
            .filter(|arg| !arg.is_empty())
            .map(|arg| String::from_utf8_lossy(arg).into_owned())
            .collect(),
    )
}

/// Removed or not is told from the directory itself, reached through the link, which has no
/// link left once removed: the path the link reads is the one in the process's own mount
/// namespace, which may lead elsewhere, or nowhere, in the daemon's. The link reads
/// ` (deleted)` after the path of a removed directory, which a directory could also be named; so
/// the suffix is taken off only for a removed one.
pub(super) fn cwd(pid: i32) -> Option<Cwd> {
    let link = PathBuf::from(format!("/proc/{pid}/cwd"));
    let read = std::fs::read_link(&link).ok()?;
    let missing = std::fs::metadata(&link).ok()?.nlink() == 0;
    let path = match read.as_os_str().as_bytes().strip_suffix(b" (deleted)") {
        Some(stripped) if missing => PathBuf::from(std::ffi::OsStr::from_bytes(stripped)),
        _ => read,
    };
    Some(Cwd { path, missing })
}

pub(super) fn memory(pid: i32) -> Option<u64> {
    let statm = std::fs::read_to_string(format!("/proc/{pid}/statm")).ok()?;
    let resident: u64 = statm.split_ascii_whitespace().nth(1)?.parse().ok()?;
    // SAFETY: `sysconf` only reads.
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    (page > 0).then_some(resident * page as u64)
}

/// The listening TCP sockets of the system, by inode, from `/proc/net/tcp` and `tcp6`; then the
/// processes' descriptors that are one of them.
pub(super) fn listening_ports(pids: &[i32]) -> Vec<u16> {
    let mut listening = std::collections::HashMap::new();
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
