//! The process table on macOS, from `libproc` and the `KERN_PROCARGS2` sysctl.

use std::ffi::CStr;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

use super::{Cwd, Identity, Proc};

pub(super) fn list() -> Option<Vec<Proc>> {
    let pids = all_pids()?;
    // SAFETY: `geteuid` only reads.
    let uid = unsafe { libc::geteuid() };
    Some(
        pids.into_iter()
            // Pid 0 is the kernel, which `getsid(0)` would answer for the caller instead.
            .filter(|&pid| pid > 0)
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
                Some(Proc {
                    id: Identity {
                        pid,
                        start: start_micros(&info),
                    },
                    ppid: info.pbi_ppid as i32,
                    sid,
                    comm: comm_of(&info),
                })
            })
            .collect(),
    )
}

/// Every pid on the system. A listing that fills the buffer may have been cut short, since a
/// process missing from it reads as gone, so it is taken again with more room.
fn all_pids() -> Option<Vec<libc::pid_t>> {
    // SAFETY: a null buffer of size 0 only asks how many processes there are.
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    if count <= 0 {
        return None;
    }
    let mut room = count as usize * 2 + 256;
    for _ in 0..4 {
        let mut pids = vec![0 as libc::pid_t; room];
        let size = (pids.len() * std::mem::size_of::<libc::pid_t>()) as libc::c_int;
        // SAFETY: `pids` is writable for `size` bytes.
        let filled = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), size) };
        if filled <= 0 {
            return None;
        }
        if (filled as usize) < pids.len() {
            pids.truncate(filled as usize);
            return Some(pids);
        }
        room *= 2;
    }
    None
}

pub(super) fn start_of(pid: i32) -> Option<u64> {
    bsd_info(pid).map(|info| start_micros(&info))
}

fn start_micros(info: &libc::proc_bsdinfo) -> u64 {
    info.pbi_start_tvsec * 1_000_000 + info.pbi_start_tvusec
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
pub(super) fn runs_as_anyone(pid: i32) -> bool {
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
/// for an Apple platform binary.
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

pub(super) fn environment(pid: i32) -> Option<Vec<u8>> {
    let buffer = procargs(pid)?;
    let (_, environment) = split_procargs(&buffer)?;
    Some(environment.to_vec())
}

pub(super) fn argv(pid: i32) -> Option<Vec<String>> {
    let buffer = procargs(pid)?;
    split_procargs(&buffer).map(|(argv, _)| argv)
}

/// The kernel keeps reporting a removed directory's last path, while a renamed one is reported
/// at its new path; so a path with nothing at it, or with another directory at it, is one the
/// process's directory was removed from.
pub(super) fn cwd(pid: i32) -> Option<Cwd> {
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
    let path = PathBuf::from(std::ffi::OsStr::from_bytes(&path));
    // A removed directory keeps a link count of 2 here as long as it is open, so the count says
    // nothing; only `NotFound` or another directory at the path does, and any other failure to
    // look says nothing.
    let missing = match std::fs::metadata(&path) {
        Ok(found) => {
            use std::os::unix::fs::MetadataExt;
            // `st_dev` is a 32-bit `dev_t` here, as `vst_dev` is.
            (found.dev() as u32 as u64, found.ino()) != (dev, ino)
        }
        Err(err) => err.kind() == std::io::ErrorKind::NotFound,
    };
    Some(Cwd { path, missing })
}

pub(super) fn memory(pid: i32) -> Option<u64> {
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

/// `PROC_PIDFDSOCKETINFO`, and where the fields read in the `struct socket_fdinfo` it fills sit,
/// from `<sys/proc_info.h>`: the `socket_info` after a 24-byte `proc_fileinfo`; in it `soi_kind`
/// and the protocol union; in that union's `tcp_sockinfo` the `in_sockinfo`'s local port and the
/// TCP state.
const PROC_PIDFDSOCKETINFO: libc::c_int = 3;
const SOCKET_FDINFO_SIZE: usize = 792;
const SOCKET_INFO: usize = 24;
const SOI_KIND: usize = SOCKET_INFO + 232;
const SOI_PROTO: usize = SOCKET_INFO + 240;
const INSI_LPORT: usize = SOI_PROTO + 4;
const TCPSI_STATE: usize = SOI_PROTO + 80;
const SOCKINFO_TCP: i32 = 2;
const TSI_S_LISTEN: i32 = 1;

pub(super) fn listening_ports(pids: &[i32]) -> Vec<u16> {
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
