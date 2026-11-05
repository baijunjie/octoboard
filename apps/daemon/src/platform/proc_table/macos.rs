//! The process table on macOS, from `libproc` and the `KERN_PROCARGS2` sysctl.

use std::ffi::CStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use super::{Cwd, EnvRead, ProcInfo};
use crate::platform::proc::ContainerId;

/// The running processes of the daemon's own effective user, zombies left out, or `None` when
/// the pids cannot be listed.
pub(crate) fn list() -> Option<Vec<ProcInfo>> {
    let pids = super::pids::all()?;
    // SAFETY: `geteuid` only reads.
    let uid = unsafe { libc::geteuid() };
    Some(
        pids.into_iter()
            .filter_map(|pid| {
                let info = bsd_info(pid)?;
                if info.pbi_uid != uid {
                    return None;
                }
                // SAFETY: `getsid` only reads; a pid gone in the meantime reports an error.
                let sid = unsafe { libc::getsid(pid) };
                if sid < 0 {
                    return None;
                }
                Some(ProcInfo {
                    pid,
                    start: start_micros(&info),
                    ppid: info.pbi_ppid as i32,
                    sid: Some(ContainerId(sid as u32)),
                    comm: comm_of(&info),
                })
            })
            .collect(),
    )
}

/// When the process `pid` started, or `None` when there is none or it is a zombie.
pub(crate) fn start_of(pid: i32) -> Option<u64> {
    bsd_info(pid).map(|info| start_micros(&info))
}

/// A process's start in microseconds since the epoch, as the system recorded it when the process
/// started: a step of the wall clock since does not change it.
fn start_micros(info: &libc::proc_bsdinfo) -> u64 {
    info.pbi_start_tvsec * 1_000_000 + info.pbi_start_tvusec
}

/// Now on the clock start times are on: the wall clock.
pub(crate) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_micros() as u64)
}

/// A start time in microseconds since the epoch, for showing: what it is already.
pub(crate) fn epoch_micros(start: u64) -> u64 {
    start
}

/// What tells this boot of the system from any other: not needed, since start times are on the
/// wall clock and a process that started before a reboot did so earlier than any after it.
pub(crate) fn boot_id() -> Option<String> {
    None
}

fn comm_of(info: &libc::proc_bsdinfo) -> String {
    let bytes: Vec<u8> = info
        .pbi_comm
        .iter()
        .take_while(|&&byte| byte != 0)
        .map(|&byte| byte as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Whether a process that is not a zombie has the pid `pid`, whichever user it runs as. The short
/// form of the BSD information is the one the system gives out for another user's process.
pub(crate) fn runs_as_anyone(pid: i32) -> bool {
    // SAFETY: a zeroed `proc_bsdshortinfo` is a valid buffer of the size given; it is only read
    // once the call filled all of it.
    unsafe {
        let mut info: libc::proc_bsdshortinfo = std::mem::zeroed();
        let size = std::mem::size_of::<libc::proc_bsdshortinfo>() as libc::c_int;
        let filled = libc::proc_pidinfo(
            pid,
            libc::PROC_PIDT_SHORTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdshortinfo).cast(),
            size,
        );
        filled == size && info.pbsi_status != libc::SZOMB
    }
}

/// The process's BSD information, or `None` when there is no such process or it is a zombie.
fn bsd_info(pid: i32) -> Option<libc::proc_bsdinfo> {
    // SAFETY: a zeroed `proc_bsdinfo` is a valid buffer of the size given; it is only read once
    // the call filled all of it.
    unsafe {
        let mut info: libc::proc_bsdinfo = std::mem::zeroed();
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
        let filled = libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        );
        (filled == size && info.pbi_status != libc::SZOMB).then_some(info)
    }
}

/// What `KERN_PROCARGS2` holds for the process: the argument count, the executable's path, then
/// the arguments and the environment, each NUL-terminated. The system leaves the environment out
/// for an Apple platform binary with SIP on.
fn procargs(pid: i32) -> Option<Vec<u8>> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
    let mut size: libc::size_t = 0;
    // SAFETY: a null buffer only asks for the size.
    let asked = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if asked != 0 || size == 0 {
        return None;
    }
    let mut buffer = vec![0u8; size];
    // SAFETY: `buffer` is writable for `size` bytes, and the call writes back how many it filled.
    let read = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            buffer.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if read != 0 {
        return None;
    }
    buffer.truncate(size);
    Some(buffer)
}

/// [`procargs`] split into the arguments and what follows them, the environment block.
fn split_procargs(buffer: &[u8]) -> Option<(Vec<String>, &[u8])> {
    let argc = i32::from_ne_bytes(buffer.get(..4)?.try_into().ok()?);
    let mut rest = &buffer[4..];
    // The executable's path, then the NULs that pad it.
    let path_end = rest.iter().position(|&byte| byte == 0)?;
    rest = &rest[path_end..];
    let first = rest
        .iter()
        .position(|&byte| byte != 0)
        .unwrap_or(rest.len());
    rest = &rest[first..];
    let mut argv = Vec::new();
    for _ in 0..argc.max(0) {
        let end = rest
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(rest.len());
        argv.push(String::from_utf8_lossy(&rest[..end]).into_owned());
        rest = rest.get(end + 1..).unwrap_or_default();
    }
    Some((argv, rest))
}

/// `pid`'s environment block, `KEY=value` entries each NUL-terminated; empty or unreadable where
/// the system withholds it.
pub(crate) fn environment(pid: i32) -> EnvRead {
    let Some(buffer) = procargs(pid) else {
        return EnvRead::Unreadable;
    };
    match split_procargs(&buffer) {
        Some((_, environment)) => EnvRead::Readable(environment.to_vec()),
        None => EnvRead::Unreadable,
    }
}

/// `pid`'s arguments, or `None` when they cannot be read.
pub(crate) fn argv(pid: i32) -> Option<Vec<String>> {
    let buffer = procargs(pid)?;
    split_procargs(&buffer).map(|(argv, _)| argv)
}

/// What the kernel reports of a process's working directory, read without the daemon looking the
/// directory up by its path: the path the kernel last knew the directory at, and the directory's
/// identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CwdReport {
    path: PathBuf,
    dev: u64,
    ino: u64,
}

/// Reading a [`CwdReport`] can block on the file system the directory is on: to give its identity,
/// the kernel asks that file system for the directory's attributes, which on a network mount that
/// stopped answering waits as long as a look at the directory would.
pub const CWD_REPORT_MAY_BLOCK: bool = true;

/// What the kernel reports of `pid`'s working directory, or `None` when it cannot be read.
/// **Blocks** on the file system the directory is on ([`CWD_REPORT_MAY_BLOCK`]).
pub(crate) fn cwd_report(pid: i32) -> Option<CwdReport> {
    // SAFETY: a zeroed `proc_vnodepathinfo` is a valid buffer of the size given; it is only read
    // once the call filled all of it, and `vip_path` is a NUL-terminated string the kernel wrote.
    let (path, dev, ino) = unsafe {
        let mut info: libc::proc_vnodepathinfo = std::mem::zeroed();
        let size = std::mem::size_of::<libc::proc_vnodepathinfo>() as libc::c_int;
        let filled = libc::proc_pidinfo(
            pid,
            libc::PROC_PIDVNODEPATHINFO,
            0,
            (&mut info as *mut libc::proc_vnodepathinfo).cast(),
            size,
        );
        if filled != size {
            return None;
        }
        let dir = &info.pvi_cdir;
        // Room for one NUL past the path in any case, so the read below stays in the buffer.
        let raw: &[libc::c_char] = std::slice::from_raw_parts(
            dir.vip_path.as_ptr().cast(),
            std::mem::size_of_val(&dir.vip_path),
        );
        if !raw.contains(&0) {
            return None;
        }
        let path = CStr::from_ptr(raw.as_ptr()).to_bytes().to_vec();
        let stat = &dir.vip_vi.vi_stat;
        (path, u64::from(stat.vst_dev), stat.vst_ino)
    };
    if path.is_empty() {
        return None;
    }
    Some(CwdReport {
        path: PathBuf::from(std::ffi::OsStr::from_bytes(&path)),
        dev,
        ino,
    })
}

impl CwdReport {
    /// A report of a directory at `path`, for a test.
    #[cfg(test)]
    pub(crate) fn at(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            dev: 0,
            ino: 0,
        }
    }

    /// The path the directory was reported at, which is where it is unless it was removed.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The paths the directory may be at, by the report alone.
    pub(super) fn candidates(&self) -> impl Iterator<Item = &Path> {
        std::iter::once(self.path.as_path())
    }

    /// The directory, looked at through the file system to tell whether it was removed.
    /// **Blocks** on the file system the path is on, which may not answer (a dead network mount).
    ///
    /// The kernel keeps reporting a removed directory's last path, while a renamed one is reported
    /// at its new path; so a path with nothing at it, or with another directory at it, is one the
    /// process's directory was removed from.
    pub fn looked(&self) -> Option<Cwd> {
        // A removed directory keeps a link count of 2 here as long as it is open, so the count
        // says nothing; only `NotFound` or another directory at the path does, and any other
        // failure to look says nothing.
        let missing = match std::fs::metadata(&self.path) {
            Ok(found) => {
                use std::os::unix::fs::MetadataExt;
                // `st_dev` is a 32-bit `dev_t` here, as `vst_dev` is.
                (found.dev() as u32 as u64, found.ino()) != (self.dev, self.ino)
            }
            Err(err) => err.kind() == std::io::ErrorKind::NotFound,
        };
        Some(Cwd {
            path: self.path.clone(),
            missing,
        })
    }
}

/// How much memory `pid` holds resident, in bytes, or `None` when it cannot be read.
pub(crate) fn memory(pid: i32) -> Option<u64> {
    // SAFETY: a zeroed `proc_taskinfo` is a valid buffer of the size given; it is only read once
    // the call filled all of it.
    unsafe {
        let mut info: libc::proc_taskinfo = std::mem::zeroed();
        let size = std::mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
        let filled = libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTASKINFO,
            0,
            (&mut info as *mut libc::proc_taskinfo).cast(),
            size,
        );
        (filled == size).then_some(info.pti_resident_size)
    }
}

/// A reader of each process's children, for a walk down the tree from `_top`: the kernel keeps
/// each process's list. A process that is gone has none.
pub(crate) fn children_lister(_top: i32) -> impl FnMut(i32) -> Option<Vec<i32>> {
    |pid| Some(children(pid))
}

/// The most entries [`children`] grows its buffer to for one process.
const MAX_CHILD_BUFFER: usize = 1 << 16;

fn children(pid: i32) -> Vec<i32> {
    let mut capacity = 64;
    loop {
        let mut children = vec![0 as libc::pid_t; capacity];
        // SAFETY: the buffer and the size passed describe the same vector, which outlives the call.
        let found = unsafe {
            libc::proc_listchildpids(
                pid,
                children.as_mut_ptr().cast(),
                std::mem::size_of_val(children.as_slice()) as libc::c_int,
            )
        };
        let found = found.max(0) as usize;
        // A buffer that came back full may have cut the list short, so it is asked again larger.
        if found >= capacity && capacity < MAX_CHILD_BUFFER {
            capacity *= 4;
            continue;
        }
        children.truncate(found);
        return children;
    }
}

/// The head of what a `proc_pidfdinfo` flavour fills, `struct proc_fileinfo`, which the libc
/// crate does not carry.
#[repr(C)]
struct ProcFileInfo {
    fi_openflags: u32,
    fi_status: u32,
    fi_offset: i64,
    fi_type: i32,
    fi_guardflags: u32,
}

/// `struct vnode_fdinfowithpath`, which `PROC_PIDFDVNODEPATHINFO` fills.
#[repr(C)]
struct VnodeFdInfoWithPath {
    file: ProcFileInfo,
    vnode: libc::vnode_info_path,
}

/// The path of the file `pid` has open as descriptor `fd`: `Ok(None)` when the descriptor is not
/// a file with a path (a socket, a pipe), and the system's error when it could not be looked at
/// (no such process or descriptor, or a refusal for another user's process).
pub(crate) fn fd_path(pid: i32, fd: i32) -> std::io::Result<Option<PathBuf>> {
    const PROC_PIDFDVNODEPATHINFO: libc::c_int = 2;
    let size = std::mem::size_of::<VnodeFdInfoWithPath>();
    // SAFETY: the structure is plain integers and characters, for which all zeroes is valid.
    let mut info: VnodeFdInfoWithPath = unsafe { std::mem::zeroed() };
    // SAFETY: the buffer and the size passed describe the same structure, which outlives the
    // call.
    let filled = unsafe {
        libc::proc_pidfdinfo(
            pid,
            fd,
            PROC_PIDFDVNODEPATHINFO,
            (&raw mut info).cast(),
            size as libc::c_int,
        )
    };
    if filled <= 0 {
        return Err(std::io::Error::last_os_error());
    }
    if (filled as usize) < size {
        return Ok(None);
    }
    // SAFETY: the array of arrays of characters is one run of bytes.
    let path = unsafe {
        std::slice::from_raw_parts(
            (&raw const info.vnode.vip_path).cast::<u8>(),
            std::mem::size_of_val(&info.vnode.vip_path),
        )
    };
    let path = &path[..path
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(path.len())];
    Ok(Some(PathBuf::from(std::ffi::OsStr::from_bytes(path))))
}

/// `PROC_PIDFDSOCKETINFO`, and where the fields read in the `struct socket_fdinfo` it fills sit,
/// from `<sys/proc_info.h>`: the `socket_info` after a `proc_fileinfo`; in it `soi_kind`
/// and the protocol union; in that union's `tcp_sockinfo` the `in_sockinfo`'s local port and the
/// TCP state.
const PROC_PIDFDSOCKETINFO: libc::c_int = 3;
const SOCKET_FDINFO_SIZE: usize = 792;
const SOCKET_INFO: usize = std::mem::size_of::<ProcFileInfo>();
const SOI_KIND: usize = SOCKET_INFO + 232;
const SOI_PROTO: usize = SOCKET_INFO + 240;
const INSI_LPORT: usize = SOI_PROTO + 4;
const TCPSI_STATE: usize = SOI_PROTO + 80;
const SOCKINFO_TCP: i32 = 2;
const TSI_S_LISTEN: i32 = 1;

/// The TCP ports any of `pids` listens on, in no order and possibly repeated; a process whose
/// sockets cannot be read adds nothing.
pub(crate) fn listening_ports(pids: &[i32]) -> Vec<u16> {
    pids.iter().flat_map(|&pid| ports_of(pid)).collect()
}

fn ports_of(pid: i32) -> Vec<u16> {
    let fd_size = std::mem::size_of::<libc::proc_fdinfo>();
    // SAFETY: a null buffer only asks how many bytes the descriptor list needs.
    let needed =
        unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, std::ptr::null_mut(), 0) };
    if needed <= 0 {
        return Vec::new();
    }
    // Room for descriptors opened since.
    let mut fds = vec![
        libc::proc_fdinfo {
            proc_fd: 0,
            proc_fdtype: 0
        };
        needed as usize / fd_size + 32
    ];
    // SAFETY: `fds` is writable for the size given.
    let filled = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDLISTFDS,
            0,
            fds.as_mut_ptr().cast(),
            (fds.len() * fd_size) as libc::c_int,
        )
    };
    if filled <= 0 {
        return Vec::new();
    }
    fds.truncate(filled as usize / fd_size);
    let read_i32 = |buffer: &[u8], at: usize| {
        i32::from_ne_bytes(buffer[at..at + 4].try_into().expect("four bytes"))
    };
    let mut ports = Vec::new();
    let mut buffer = vec![0u8; SOCKET_FDINFO_SIZE];
    for fd in fds
        .iter()
        .filter(|fd| fd.proc_fdtype == libc::PROX_FDTYPE_SOCKET as u32)
    {
        // SAFETY: `buffer` is writable for the size given; it is only read once the call filled
        // all of it.
        let got = unsafe {
            libc::proc_pidfdinfo(
                pid,
                fd.proc_fd,
                PROC_PIDFDSOCKETINFO,
                buffer.as_mut_ptr().cast(),
                SOCKET_FDINFO_SIZE as libc::c_int,
            )
        };
        if got as usize != SOCKET_FDINFO_SIZE {
            continue;
        }
        if read_i32(&buffer, SOI_KIND) == SOCKINFO_TCP
            && read_i32(&buffer, TCPSI_STATE) == TSI_S_LISTEN
        {
            // The port is in network byte order in the field's low 16 bits.
            let port = u16::from_be(read_i32(&buffer, INSI_LPORT) as u16);
            if port != 0 {
                ports.push(port);
            }
        }
    }
    ports
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_descriptor_structure_has_the_size_the_system_fills() {
        assert_eq!(std::mem::size_of::<ProcFileInfo>(), 24);
        assert_eq!(std::mem::size_of::<VnodeFdInfoWithPath>(), 24 + 152 + 1024);
    }
}
