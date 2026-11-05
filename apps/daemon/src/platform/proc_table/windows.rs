//! The process table on Windows: a toolhelp snapshot for the processes and their parents, each
//! process's own handle for its user, start, memory and command line, its process environment block
//! for its working directory, and the TCP table for listening ports. Start times are the system's
//! creation times, in microseconds since the Unix epoch.

use std::collections::HashMap;
use std::ffi::{c_void, OsString};
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::OsStringExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use windows_sys::Wdk::System::Threading::{
    NtQueryInformationProcess, ProcessBasicInformation, ProcessCommandLineInformation,
    ProcessWow64Information,
};
use windows_sys::Win32::Foundation::{
    LocalFree, FILETIME, HANDLE, INVALID_HANDLE_VALUE, STILL_ACTIVE, UNICODE_STRING, WAIT_OBJECT_0,
    WAIT_TIMEOUT,
};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCP6TABLE_OWNER_PID, MIB_TCPROW_OWNER_PID,
    MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_LISTENER,
};
use windows_sys::Win32::Security::{
    EqualSid, GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER,
};
use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetExitCodeProcess, GetProcessTimes, IsWow64Process, OpenProcess,
    OpenProcessToken, WaitForSingleObject, PROCESS_BASIC_INFORMATION,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_VM_READ,
};
use windows_sys::Win32::UI::Shell::CommandLineToArgvW;

use super::{Cwd, EnvRead, ProcInfo};

/// Microseconds from the `FILETIME` epoch (1601-01-01) to the Unix epoch.
const UNIX_EPOCH_IN_FILETIME_MICROS: u64 = 11_644_473_600_000_000;

/// `AF_INET` and `AF_INET6`, the address families the TCP table is read for.
const AF_INET: u32 = 2;
const AF_INET6: u32 = 23;

/// `STATUS_INFO_LENGTH_MISMATCH`: the buffer given to `NtQueryInformationProcess` was too small.
const STATUS_INFO_LENGTH_MISMATCH: i32 = 0xC000_0004_u32 as i32;

/// One process as the toolhelp snapshot lists it.
pub(crate) struct Listed {
    pub pid: u32,
    pub ppid: u32,
    /// The executable's file name.
    pub name: String,
}

/// Every process the system runs, from one toolhelp snapshot, or `None` when it cannot be taken.
/// The parent is the pid the process was started by, which may since have passed to another
/// process: Windows never reparents.
pub(crate) fn snapshot() -> Option<Vec<Listed>> {
    // SAFETY: a plain call; the handle is owned below.
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if raw == INVALID_HANDLE_VALUE {
        return None;
    }
    // SAFETY: a valid handle the call above returned, owned from here.
    let snapshot = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut listed = Vec::new();
    // SAFETY: `entry` is a `PROCESSENTRY32W` with its size set, alive for each call.
    let mut more = unsafe { Process32FirstW(snapshot.as_raw_handle(), &mut entry) } != 0;
    while more {
        let name = &entry.szExeFile;
        let end = name
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(name.len());
        listed.push(Listed {
            pid: entry.th32ProcessID,
            ppid: entry.th32ParentProcessID,
            name: String::from_utf16_lossy(&name[..end]),
        });
        // SAFETY: as above.
        more = unsafe { Process32NextW(snapshot.as_raw_handle(), &mut entry) } != 0;
    }
    Some(listed)
}

/// An open handle on a process.
pub(crate) struct Process(OwnedHandle);

impl Process {
    /// Opens `pid` with `access`, or `None` when there is no such process or it is refused.
    pub(crate) fn open(pid: u32, access: u32) -> Option<Self> {
        // Pid 0 is the idle process, which `OpenProcess` refuses with an error that reads as a
        // bad argument rather than as a refusal.
        if pid == 0 {
            return None;
        }
        // SAFETY: a plain call; the handle is owned below.
        let raw = unsafe { OpenProcess(access, 0, pid) };
        // SAFETY: a valid handle `OpenProcess` returned, owned from here.
        (!raw.is_null()).then(|| Self(unsafe { OwnedHandle::from_raw_handle(raw) }))
    }

    /// Opens `pid` for queries, and for waiting on where that is allowed, which tells a running
    /// process from an exited one more surely than its exit code.
    pub(crate) fn query(pid: u32) -> Option<Self> {
        Self::open(pid, PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE)
            .or_else(|| Self::open(pid, PROCESS_QUERY_LIMITED_INFORMATION))
    }

    pub(crate) fn raw(&self) -> HANDLE {
        self.0.as_raw_handle()
    }

    pub(crate) fn into_handle(self) -> OwnedHandle {
        self.0
    }

    /// Whether the process has not exited. A handle keeps an exited process's pid reserved, so
    /// one may be open on a process that is no longer running.
    pub(crate) fn is_running(&self) -> bool {
        is_running(self.raw())
    }

    /// When the process started, in microseconds since the Unix epoch.
    pub(crate) fn start(&self) -> Option<u64> {
        start_of_handle(self.raw())
    }
}

/// Whether the process `process` is a handle on has not exited.
pub(crate) fn is_running(process: HANDLE) -> bool {
    // SAFETY: a plain wait that does not block, on a handle the caller holds open.
    match unsafe { WaitForSingleObject(process, 0) } {
        WAIT_TIMEOUT => true,
        WAIT_OBJECT_0 => false,
        // Opened without `SYNCHRONIZE`: the exit code says it, unless the process exited with
        // the code that means "still active", which no program the daemon cares about does.
        _ => {
            let mut code = 0;
            // SAFETY: `code` is a valid place for the code.
            let read = unsafe { GetExitCodeProcess(process, &mut code) };
            read != 0 && code == STILL_ACTIVE as u32
        }
    }
}

/// When the process `process` is a handle on started, in microseconds since the Unix epoch.
pub(crate) fn start_of_handle(process: HANDLE) -> Option<u64> {
    // SAFETY: four `FILETIME`s are valid places for the times, and the handle is open.
    unsafe {
        let (mut created, mut exited, mut kernel, mut user): (
            FILETIME,
            FILETIME,
            FILETIME,
            FILETIME,
        ) = (zeroed(), zeroed(), zeroed(), zeroed());
        if GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user) == 0 {
            return None;
        }
        let ticks = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
        (ticks / 10).checked_sub(UNIX_EPOCH_IN_FILETIME_MICROS)
    }
}

/// The `TOKEN_USER` of the token of `process`, in a buffer aligned for it.
fn token_user(process: HANDLE) -> Option<Vec<u64>> {
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: `token` is a valid place for the handle, which is owned below.
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return None;
    }
    // SAFETY: a valid handle the call above returned, owned from here.
    let token = unsafe { OwnedHandle::from_raw_handle(token) };
    // A `TOKEN_USER` and the SID it points into: well within 128 bytes.
    let mut buffer = vec![0u64; 16];
    let mut needed = 0;
    // SAFETY: the buffer is writable for the size given.
    let read = unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            buffer.as_mut_ptr().cast(),
            (buffer.len() * 8) as u32,
            &mut needed,
        )
    };
    (read != 0).then_some(buffer)
}

/// The daemon's own user, read once.
fn own_user() -> Option<&'static [u64]> {
    static OWN: OnceLock<Option<Vec<u64>>> = OnceLock::new();
    // SAFETY: the pseudo handle of this process needs no closing.
    OWN.get_or_init(|| token_user(unsafe { GetCurrentProcess() }))
        .as_deref()
}

/// Whether the process `process` is a handle on runs as the daemon's own user.
fn runs_as_own_user(process: HANDLE) -> bool {
    let (Some(own), Some(theirs)) = (own_user(), token_user(process)) else {
        return false;
    };
    // SAFETY: both buffers hold a `TOKEN_USER` whose SID points into the same buffer, alive for
    // the call.
    unsafe {
        let own = &*own.as_ptr().cast::<TOKEN_USER>();
        let theirs = &*theirs.as_ptr().cast::<TOKEN_USER>();
        EqualSid(own.User.Sid, theirs.User.Sid) != 0
    }
}

/// The running processes of the daemon's own user, or `None` when the snapshot cannot be taken.
pub(crate) fn list() -> Option<Vec<ProcInfo>> {
    Some(
        snapshot()?
            .into_iter()
            .filter_map(|listed| {
                let process = Process::query(listed.pid)?;
                if !runs_as_own_user(process.raw()) || !process.is_running() {
                    return None;
                }
                Some(ProcInfo {
                    pid: listed.pid as i32,
                    start: process.start()?,
                    ppid: listed.ppid as i32,
                    sid: None,
                    comm: listed.name,
                })
            })
            .collect(),
    )
}

/// When the process `pid` started, or `None` when there is none or it has exited.
pub(crate) fn start_of(pid: i32) -> Option<u64> {
    let process = Process::query(pid as u32)?;
    process.is_running().then(|| process.start()).flatten()
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

/// Not needed: start times are on the wall clock, and a process that started before a reboot
/// did so earlier than any after it.
pub(crate) fn boot_id() -> Option<String> {
    None
}

/// Whether a running process has the pid `pid`, whichever user it runs as.
pub(crate) fn runs_as_anyone(pid: i32) -> bool {
    Process::query(pid as u32).is_some_and(|process| process.is_running())
}

/// Not read: another process's environment is only in its memory, and nothing here needs it.
pub(crate) fn environment(_pid: i32) -> EnvRead {
    EnvRead::Unsupported
}

/// `pid`'s arguments, split from its command line as the C runtime splits it, or `None` when it
/// cannot be read. Read through the system's query for it, which needs no access to the process's
/// memory, so it works for an elevated process and one of another bitness.
pub(crate) fn argv(pid: i32) -> Option<Vec<String>> {
    let process = Process::query(pid as u32)?;
    let line = command_line(process.raw())?;
    if line.is_empty() {
        return Some(Vec::new());
    }
    let mut wide: Vec<u16> = line;
    wide.push(0);
    let mut count = 0;
    // SAFETY: `wide` is NUL-terminated and alive for the call; the array it returns is freed
    // below, after its strings are copied.
    unsafe {
        let args = CommandLineToArgvW(wide.as_ptr(), &mut count);
        if args.is_null() {
            return None;
        }
        let argv = std::slice::from_raw_parts(args, count.max(0) as usize)
            .iter()
            .map(|&arg| {
                let len = (0..).take_while(|&at| *arg.add(at) != 0).count();
                String::from_utf16_lossy(std::slice::from_raw_parts(arg, len))
            })
            .collect();
        LocalFree(args.cast());
        Some(argv)
    }
}

/// The command line of the process `process` is a handle on, as UTF-16 without its NUL.
fn command_line(process: HANDLE) -> Option<Vec<u16>> {
    // Aligned for the `UNICODE_STRING` the query writes at its start.
    let mut buffer = vec![0u64; 512];
    for _ in 0..3 {
        let mut needed = 0u32;
        // SAFETY: the buffer is writable for the size given.
        let status = unsafe {
            NtQueryInformationProcess(
                process,
                ProcessCommandLineInformation,
                buffer.as_mut_ptr().cast(),
                (buffer.len() * 8) as u32,
                &mut needed,
            )
        };
        if status == STATUS_INFO_LENGTH_MISMATCH && needed as usize > buffer.len() * 8 {
            buffer = vec![0u64; needed as usize / 8 + 1];
            continue;
        }
        if status != 0 {
            return None;
        }
        // SAFETY: the query wrote a `UNICODE_STRING` at the start of the buffer whose characters
        // follow it in the same buffer.
        unsafe {
            let line = &*buffer.as_ptr().cast::<UNICODE_STRING>();
            if line.Buffer.is_null() {
                return Some(Vec::new());
            }
            return Some(
                std::slice::from_raw_parts(line.Buffer, usize::from(line.Length) / 2).to_vec(),
            );
        }
    }
    None
}

/// Reads a `T` at `address` in the memory of the process `process` is a handle on.
fn read<T: Copy>(process: HANDLE, address: usize) -> Option<T> {
    // SAFETY: a `T` of plain integers, for which all zeroes is valid; the read fills all of it or
    // fails.
    unsafe {
        let mut value: T = zeroed();
        let mut read = 0;
        (ReadProcessMemory(
            process,
            address as *const c_void,
            (&mut value as *mut T).cast(),
            size_of::<T>(),
            &mut read,
        ) != 0
            && read == size_of::<T>())
        .then_some(value)
    }
}

/// Reads `bytes` bytes of UTF-16 at `address` in the memory of the process `process` is a handle
/// on.
fn read_wide(process: HANDLE, address: usize, bytes: usize) -> Option<Vec<u16>> {
    // A Windows path is at most 32767 characters.
    if !bytes.is_multiple_of(2) || bytes > 2 * 32_767 {
        return None;
    }
    let mut wide = vec![0u16; bytes / 2];
    let mut read = 0;
    // SAFETY: `wide` is writable for `bytes` bytes.
    let done = unsafe {
        ReadProcessMemory(
            process,
            address as *const c_void,
            wide.as_mut_ptr().cast(),
            bytes,
            &mut read,
        )
    };
    (done != 0 && read == bytes).then_some(wide)
}

/// The working directory of the process `process` is a handle on, from its process parameters:
/// the 64-bit layout, or for a 32-bit process under WOW64 the 32-bit one. The daemon is itself a
/// 64-bit process, and an x64 process emulated on ARM64 has the 64-bit layout. The process may
/// change its directory while it is read; a read that fails then is `None`.
fn current_directory(process: HANDLE) -> Option<Vec<u16>> {
    let mut wow64 = 0;
    // SAFETY: `wow64` is a valid place for the answer.
    if unsafe { IsWow64Process(process, &mut wow64) } == 0 {
        return None;
    }
    if wow64 == 0 {
        // SAFETY: a zeroed `PROCESS_BASIC_INFORMATION` is a valid buffer of the size given.
        let mut basic: PROCESS_BASIC_INFORMATION = unsafe { zeroed() };
        // SAFETY: as above.
        let status = unsafe {
            NtQueryInformationProcess(
                process,
                ProcessBasicInformation,
                (&mut basic as *mut PROCESS_BASIC_INFORMATION).cast(),
                size_of::<PROCESS_BASIC_INFORMATION>() as u32,
                std::ptr::null_mut(),
            )
        };
        if status != 0 || basic.PebBaseAddress.is_null() {
            return None;
        }
        // `PEB.ProcessParameters`, then `RTL_USER_PROCESS_PARAMETERS.CurrentDirectory.DosPath`: a
        // `UNICODE_STRING`, its length first and its buffer 8 bytes on.
        let parameters: usize = read(process, basic.PebBaseAddress as usize + 0x20)?;
        let length: u16 = read(process, parameters + 0x38)?;
        let buffer: usize = read(process, parameters + 0x40)?;
        read_wide(process, buffer, usize::from(length))
    } else {
        let mut peb: usize = 0;
        // SAFETY: `peb` is a valid place for the address of the 32-bit PEB.
        let status = unsafe {
            NtQueryInformationProcess(
                process,
                ProcessWow64Information,
                (&mut peb as *mut usize).cast(),
                size_of::<usize>() as u32,
                std::ptr::null_mut(),
            )
        };
        if status != 0 || peb == 0 {
            return None;
        }
        // The same fields in the 32-bit layout.
        let parameters: u32 = read(process, peb + 0x10)?;
        let length: u16 = read(process, parameters as usize + 0x24)?;
        let buffer: u32 = read(process, parameters as usize + 0x28)?;
        read_wide(process, buffer as usize, usize::from(length))
    }
}

/// What the system reports of a process's working directory: its path. Windows keeps a process's
/// working directory open without letting it be removed, so the report is the directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CwdReport {
    read: PathBuf,
}

/// Reading the report reads the process's memory, which never waits on a file system.
pub const CWD_REPORT_MAY_BLOCK: bool = false;

/// `pid`'s working directory, or `None` when it cannot be read: the process is gone, or its memory
/// cannot be read, as a process running elevated cannot be from a daemon that is not.
pub(crate) fn cwd_report(pid: i32) -> Option<CwdReport> {
    let process = Process::open(
        pid as u32,
        PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
    )?;
    let wide = current_directory(process.raw())?;
    // The system keeps the directory with a trailing separator, which only a root (`C:\\`) keeps
    // in a path.
    let end = if wide.len() > 3 && wide.last() == Some(&u16::from(b'\\')) {
        wide.len() - 1
    } else {
        wide.len()
    };
    (end > 0).then(|| CwdReport {
        read: OsString::from_wide(&wide[..end]).into(),
    })
}

impl CwdReport {
    #[cfg(test)]
    pub(crate) fn at(path: impl Into<PathBuf>) -> Self {
        Self { read: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.read
    }

    pub(super) fn candidates(&self) -> impl Iterator<Item = &Path> {
        std::iter::once(self.read.as_path())
    }

    /// The directory as reported: a directory a process works in cannot be removed on Windows,
    /// so it is never missing.
    pub fn looked(&self) -> Option<Cwd> {
        Some(Cwd {
            path: self.read.clone(),
            missing: false,
        })
    }
}

/// How much memory `pid` holds resident (its working set), in bytes, or `None` when it cannot be
/// read.
pub(crate) fn memory(pid: i32) -> Option<u64> {
    let process = Process::query(pid as u32)?;
    // SAFETY: a zeroed `PROCESS_MEMORY_COUNTERS` is a valid buffer of the size given.
    unsafe {
        let mut counters: PROCESS_MEMORY_COUNTERS = zeroed();
        let size = size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        (K32GetProcessMemoryInfo(process.raw(), &mut counters, size) != 0)
            .then_some(counters.WorkingSetSize as u64)
    }
}

/// A reader of each process's children, for a walk down the tree from `_top`, from one snapshot
/// taken at the first call. A process listed under a parent's pid is that parent's child only when
/// it did not start before the parent: Windows never reparents, so the pid of a parent that has
/// exited may have passed to a later process. A child whose start cannot be read is gone.
pub(crate) fn children_lister(_top: i32) -> impl FnMut(i32) -> Option<Vec<i32>> {
    let mut by_parent: Option<Option<HashMap<u32, Vec<u32>>>> = None;
    move |pid| {
        let by_parent = by_parent
            .get_or_insert_with(|| {
                snapshot().map(|listed| {
                    let mut by_parent: HashMap<u32, Vec<u32>> = HashMap::new();
                    for process in listed {
                        by_parent.entry(process.ppid).or_default().push(process.pid);
                    }
                    by_parent
                })
            })
            .as_ref()?;
        let Some(parent_start) = start_of(pid) else {
            return Some(Vec::new());
        };
        Some(
            by_parent
                .get(&(pid as u32))
                .into_iter()
                .flatten()
                .filter(|&&child| child != pid as u32)
                .filter(|&&child| start_of(child as i32).is_some_and(|start| start >= parent_start))
                .map(|&child| child as i32)
                .collect(),
        )
    }
}

/// Not available: the system gives no other process's descriptors.
pub(crate) fn fd_path(_pid: i32, _fd: i32) -> std::io::Result<Option<PathBuf>> {
    Err(std::io::ErrorKind::Unsupported.into())
}

/// The TCP ports any of `pids` listens on, from the system's TCP tables for IPv4 and IPv6, in no
/// order and possibly repeated.
pub(crate) fn listening_ports(pids: &[i32]) -> Vec<u16> {
    let owned = |owner: u32| pids.contains(&(owner as i32));
    let mut ports = Vec::new();
    if let Some(table) = tcp_listeners(AF_INET, size_of::<MIB_TCPROW_OWNER_PID>()) {
        // SAFETY: the system filled the buffer with a `MIB_TCPTABLE_OWNER_PID` of that many rows,
        // which `tcp_listeners` checked fit in it.
        let rows = unsafe {
            let table = &*table.as_ptr().cast::<MIB_TCPTABLE_OWNER_PID>();
            std::slice::from_raw_parts(table.table.as_ptr(), table.dwNumEntries as usize)
        };
        ports.extend(
            rows.iter()
                .filter(|row| owned(row.dwOwningPid))
                .map(|row| u16::from_be(row.dwLocalPort as u16)),
        );
    }
    if let Some(table) = tcp_listeners(AF_INET6, size_of::<MIB_TCP6ROW_OWNER_PID>()) {
        // SAFETY: as above, a `MIB_TCP6TABLE_OWNER_PID`.
        let rows = unsafe {
            let table = &*table.as_ptr().cast::<MIB_TCP6TABLE_OWNER_PID>();
            std::slice::from_raw_parts(table.table.as_ptr(), table.dwNumEntries as usize)
        };
        ports.extend(
            rows.iter()
                .filter(|row| owned(row.dwOwningPid))
                .map(|row| u16::from_be(row.dwLocalPort as u16)),
        );
    }
    ports.retain(|&port| port != 0);
    ports
}

/// The listening sockets of the address family `family` with their owners' pids, as the system's
/// table for it with rows of `row_size` bytes, or `None` when it cannot be read.
fn tcp_listeners(family: u32, row_size: usize) -> Option<Vec<u64>> {
    let mut size = 0u32;
    // SAFETY: a null table only asks for the size.
    unsafe {
        GetExtendedTcpTable(
            std::ptr::null_mut(),
            &mut size,
            0,
            family,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        )
    };
    // Sockets opened between the two calls make the table larger than the size asked for.
    for _ in 0..4 {
        let mut table = vec![0u64; size as usize / 8 + 64];
        let mut room = (table.len() * 8) as u32;
        // SAFETY: the table is writable for the size given.
        let status = unsafe {
            GetExtendedTcpTable(
                table.as_mut_ptr().cast(),
                &mut room,
                0,
                family,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            )
        };
        if status == 0 {
            // The count, then that many rows, must fit in the buffer.
            let count = table[0] as u32 as usize;
            return (4 + count * row_size <= table.len() * 8).then_some(table);
        }
        size = room;
    }
    None
}
