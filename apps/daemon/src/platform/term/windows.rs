//! The Windows terminal backend: a pseudoconsole (ConPTY) of the daemon's own, with the process
//! started on it by `CreateProcessW` and placed in its jobs before it runs its first instruction
//! ([`Joining`]).
//!
//! The pseudoconsole's host (`conhost.exe`, or the `OpenConsole.exe` a bundled `conpty.dll` starts)
//! is a child of the daemon that reads the input pipe and writes the output pipe. Three of its ways
//! shape this module:
//!
//! - Its output pipe never ends while the console is open, even after every process on it has
//!   exited, so the console is closed once the process exits, and the reader then reads to the end.
//! - Closing the console blocks (inbox host) or keeps the host and its processes alive (bundled
//!   host) until what it has written is read, so the close runs on a thread of its own, and while
//!   no reader drains the output a drainer reads and drops it.
//! - Closing the console is the host's request to end: every process attached to it gets
//!   `CTRL_CLOSE_EVENT`, which an agent may die on without its shutdown work. A process that
//!   declared [`QuitKeys`] is asked by its keys first.
//!
//! Both pipes are opened for overlapped I/O on the daemon's side, so a read can be stopped without
//! closing anything and a write can give up at its deadline.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Once, OnceLock};
use std::time::Duration;

use anyhow::{Context, Result};
use windows_sys::Win32::Foundation::{
    GetLastError, ERROR_BROKEN_PIPE, ERROR_IO_PENDING, ERROR_PIPE_NOT_CONNECTED, HANDLE,
    INVALID_HANDLE_VALUE, WAIT_FAILED, WAIT_OBJECT_0,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED,
    FILE_READ_ATTRIBUTES, FILE_WRITE_ATTRIBUTES, OPEN_EXISTING, PIPE_ACCESS_INBOUND,
    PIPE_ACCESS_OUTBOUND,
};
use windows_sys::Win32::System::Console::{SetConsoleCtrlHandler, COORD, HPCON};
use windows_sys::Win32::System::LibraryLoader::{
    GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32,
};
use windows_sys::Win32::System::Pipes::{
    CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, CreateProcessW, DeleteProcThreadAttributeList, InitializeProcThreadAttributeList,
    ResumeThread, SetEvent, TerminateProcess, UpdateProcThreadAttribute, WaitForMultipleObjects,
    WaitForSingleObject, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT,
    EXTENDED_STARTUPINFO_PRESENT, INFINITE, LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION,
    PROC_THREAD_ATTRIBUTE_DESKTOP_APP_POLICY, PROC_THREAD_ATTRIBUTE_JOB_LIST,
    PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, STARTF_USESTDHANDLES, STARTUPINFOEXW,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};

use super::{
    encode, hung_up, Command, Foreground, Input, PartialWrite, QuitKeys, Size, Spawned, TermFacts,
};
use crate::platform::fs;
use crate::platform::proc::{Containment, Placement, Polite, ProcessId, Reach, Sent};
use crate::platform::proc_table::is_running;

/// The facts of the host that starts the daemon's pseudoconsoles, decided once, before any has
/// started, from the files present ([`bundled_host_in_use`]). The two hosts need opposite facts:
/// the inbox `conhost.exe` repaints what the program draws and answers its queries itself, while
/// the bundled `OpenConsole.exe` passes the program's own bytes through, forwards every query and
/// holds the console until it is answered. Should the host that actually starts differ, the facts
/// stay as decided and [`report_host`] only logs a warning.
pub(super) fn host_facts() -> TermFacts {
    static FACTS: OnceLock<TermFacts> = OnceLock::new();
    *FACTS.get_or_init(|| {
        // Codex reads a paste as keys through the console and draws them only once its paste burst
        // flushes, 100–150 ms after the paste under either host, and Claude Code's and Codex's
        // start after the trust screen has gaps of up to 300 ms.
        let settle_window = Duration::from_millis(250);
        if bundled_host_in_use() {
            TermFacts {
                host_renders: false,
                host_answers_queries: false,
                host_waits_on_queries: true,
                paste_as_keys: true,
                foreground_job: false,
                settle_window,
            }
        } else {
            TermFacts {
                host_renders: true,
                host_answers_queries: true,
                host_waits_on_queries: false,
                paste_as_keys: true,
                foreground_job: false,
                settle_window,
            }
        }
    })
}

/// The bundled host asks for the window to be shown, for the terminal's device attributes, and for
/// focus reports and keys as input records; the inbox host hides the cursor and clears the screen.
#[cfg(test)]
pub(super) fn host_output_preamble() -> &'static [u8] {
    if bundled_host_in_use() {
        b"\x1b[1t\x1b[c\x1b[?1004h\x1b[?9001h"
    } else {
        b"\x1b[?25l\x1b[2J\x1b[m\x1b[H"
    }
}

/// Whether the pseudoconsoles run on the bundled host, as the files present tell it: the
/// `conpty.dll` loaded, and the `OpenConsole.exe` it starts is where it looks for it. Without that
/// file it starts the inbox host without a word. The host that actually starts is not asked, since
/// the facts are needed before the first pseudoconsole; a mismatch is a warning of
/// [`report_host`].
fn bundled_host_in_use() -> bool {
    api().bundled
        && bundled_dll().is_some_and(|dll| {
            let dir = dll.parent().unwrap_or(Path::new(""));
            dir.join("OpenConsole.exe").is_file()
                || native_architecture()
                    .is_some_and(|arch| dir.join(arch).join("OpenConsole.exe").is_file())
        })
}

/// The `conpty.dll` shipped beside the daemon's executable, whether or not it loads.
fn bundled_dll() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.with_file_name("conpty.dll"))
}

/// The name of the subdirectory `conpty.dll` looks in for its host after its own: the machine's
/// native architecture, not the emulated one an x64 daemon on ARM64 runs as.
fn native_architecture() -> Option<&'static str> {
    use windows_sys::Win32::System::SystemInformation::{
        IMAGE_FILE_MACHINE, IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_ARM64,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, IsWow64Process2};
    let (mut process, mut native): (IMAGE_FILE_MACHINE, IMAGE_FILE_MACHINE) = (0, 0);
    // SAFETY: the pseudo handle of the current process is valid, and both outputs are written.
    if unsafe { IsWow64Process2(GetCurrentProcess(), &mut process, &mut native) } == 0 {
        return None;
    }
    match native {
        IMAGE_FILE_MACHINE_AMD64 => Some("x64"),
        IMAGE_FILE_MACHINE_ARM64 => Some("arm64"),
        _ => None,
    }
}

/// Slice size for one write into the input pipe. The host takes input slowly (the inbox host has
/// been measured at over a second for 4 KiB), so a write goes in slices and each one that goes in
/// renews the deadline.
const WRITE_SLICE: usize = 4096;

/// How long a write may wait for the host to take one slice before giving up. The host reads its
/// input continuously into memory of its own whatever the program does, so a write that waits this
/// long has a host that is wedged, not a program that is busy.
const WRITE_DEADLINE: Duration = Duration::from_secs(5);

/// How long after the process exits the console is closed: the host draws what the process
/// printed last a little after it, and what it has not drawn by the close is lost.
const EXIT_SETTLE: Duration = Duration::from_millis(200);

/// The size the host's pipes are created with, for each direction.
const PIPE_BUFFER: u32 = 64 * 1024;

/// The exit code a process that could not be taken on is ended with.
const KILLED: u32 = 1;

/// `PROCESS_CREATION_DESKTOP_APP_BREAKAWAY_DISABLE_PROCESS_TREE`, the desktop app policy under
/// which what a process starts, and what that starts in turn, stays in the process's jobs.
const DESKTOP_APP_BREAKAWAY_DISABLE_PROCESS_TREE: u32 = 2;

fn last_error() -> io::Error {
    io::Error::last_os_error()
}

/// `text` as a NUL-terminated wide string.
fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(std::iter::once(0)).collect()
}

// --- The pseudoconsole functions.

type CreateFn =
    unsafe extern "system" fn(COORD, HANDLE, HANDLE, u32, *mut HPCON) -> windows_sys::core::HRESULT;
type ResizeFn = unsafe extern "system" fn(HPCON, COORD) -> windows_sys::core::HRESULT;
type CloseFn = unsafe extern "system" fn(HPCON);

/// Where the pseudoconsole functions come from: a `conpty.dll` shipped beside the daemon, whose
/// host behaves the same on every Windows version, or the system's own.
struct Api {
    create: CreateFn,
    resize: ResizeFn,
    close: CloseFn,
    bundled: bool,
}

fn api() -> &'static Api {
    static API: OnceLock<Api> = OnceLock::new();
    API.get_or_init(|| {
        bundled().unwrap_or_else(|| {
            use windows_sys::Win32::System::Console as system;
            Api {
                create: system::CreatePseudoConsole,
                resize: system::ResizePseudoConsole,
                close: system::ClosePseudoConsole,
                bundled: false,
            }
        })
    })
}

/// The functions of the `conpty.dll` beside the daemon's executable, when there is one that loads.
fn bundled() -> Option<Api> {
    let dll = bundled_dll()?;
    if !dll.is_file() {
        return None;
    }
    let name = wide(dll.as_os_str());
    // SAFETY: `name` is NUL-terminated and alive for the call. The flags keep the DLL's own
    // dependencies to its directory and the system's.
    let module = unsafe {
        LoadLibraryExW(
            name.as_ptr(),
            std::ptr::null_mut(),
            LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
        )
    };
    if module.is_null() {
        let err = last_error();
        tracing::warn!(%err, path = %dll.display(), "loading the bundled conpty.dll failed; the system's pseudoconsole is used");
        return None;
    }
    // SAFETY: `module` is loaded and never freed; each name is NUL-terminated.
    let find = |symbol: &[u8]| unsafe { GetProcAddress(module, symbol.as_ptr()) };
    let (Some(create), Some(resize), Some(close)) = (
        find(b"ConptyCreatePseudoConsole\0"),
        find(b"ConptyResizePseudoConsole\0"),
        find(b"ConptyClosePseudoConsole\0"),
    ) else {
        tracing::warn!(path = %dll.display(), "the bundled conpty.dll lacks a pseudoconsole function; the system's pseudoconsole is used");
        return None;
    };
    // SAFETY: the exports have the signatures of the system's functions of the same names.
    unsafe {
        Some(Api {
            create: std::mem::transmute::<unsafe extern "system" fn() -> isize, CreateFn>(create),
            resize: std::mem::transmute::<unsafe extern "system" fn() -> isize, ResizeFn>(resize),
            close: std::mem::transmute::<unsafe extern "system" fn() -> isize, CloseFn>(close),
            bundled: true,
        })
    }
}

/// Logs, once, which host the daemon's pseudoconsoles run on. A bundled `conpty.dll` that finds no
/// `OpenConsole.exe` beside it (or in a subdirectory named after the machine's architecture) falls
/// back to the system's `conhost.exe` without a word, and the host's output differs between the
/// two: the bundled one passes the program's output through, the system's repaints it. A host that
/// contradicts the facts in use ([`host_facts`]) is only warned of: the facts are not changed.
fn report_host(api: &Api) {
    static REPORTED: Once = Once::new();
    REPORTED.call_once(|| {
        let own = std::process::id();
        let hosts: Vec<String> = crate::platform::proc_table::snapshot()
            .unwrap_or_default()
            .into_iter()
            .filter(|process| process.ppid == own)
            .map(|process| process.name)
            .filter(|name| {
                name.eq_ignore_ascii_case("OpenConsole.exe")
                    || name.eq_ignore_ascii_case("conhost.exe")
            })
            .collect();
        let bundled_host = hosts
            .iter()
            .any(|name| name.eq_ignore_ascii_case("OpenConsole.exe"));
        let facts_say_bundled = !host_facts().host_renders;
        if api.bundled && !bundled_host {
            tracing::warn!(
                ?hosts,
                "the bundled conpty.dll started the system's console host: OpenConsole.exe is missing beside it"
            );
        } else if facts_say_bundled != bundled_host {
            tracing::warn!(
                ?hosts,
                facts_say_bundled,
                "the console host that started contradicts the terminal facts in use"
            );
        } else {
            tracing::info!(
                host = if bundled_host { "OpenConsole.exe" } else { "conhost.exe" },
                bundled = api.bundled,
                "pseudoconsoles run on this console host"
            );
        }
    });
}

/// A pseudoconsole's handle, which only the thread that closes it uses after it is taken out.
struct Console(HPCON);

// SAFETY: a pseudoconsole handle is a value the functions above take from any thread.
unsafe impl Send for Console {}

fn coord(size: Size) -> COORD {
    COORD {
        X: size.cols.min(i16::MAX as u16) as i16,
        Y: size.rows.min(i16::MAX as u16) as i16,
    }
}

// --- Pipes and overlapped I/O.

/// A manual-reset event.
struct Event(OwnedHandle);

impl Event {
    fn new() -> io::Result<Self> {
        // SAFETY: null attributes and name ask for a plain unnamed event.
        let raw = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        if raw.is_null() {
            return Err(last_error());
        }
        // SAFETY: a valid handle the call above returned, owned from here.
        Ok(Self(unsafe { OwnedHandle::from_raw_handle(raw) }))
    }

    fn raw(&self) -> HANDLE {
        self.0.as_raw_handle()
    }

    fn set(&self) {
        // SAFETY: the event is open.
        unsafe { SetEvent(self.raw()) };
    }
}

/// The two ends of a pipe between the daemon and the host: the daemon's, open for overlapped I/O,
/// and the host's, which `CreatePseudoConsole` takes a copy of. A named pipe, as an anonymous one
/// cannot be opened for overlapped I/O; its name is random, it takes one instance and no remote
/// client, and the host's end is opened at once, so nothing else can be the other end.
fn pipe(inbound: bool) -> io::Result<(OwnedHandle, OwnedHandle)> {
    let name = format!(
        r"\\.\pipe\octoboard-conpty-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    );
    let name = wide(OsStr::new(&name));
    let access = if inbound {
        PIPE_ACCESS_INBOUND
    } else {
        PIPE_ACCESS_OUTBOUND
    };
    // SAFETY: `name` is NUL-terminated and alive for the call; null attributes give the pipe the
    // default security of the daemon's user.
    let ours = unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            access | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            PIPE_BUFFER,
            PIPE_BUFFER,
            0,
            std::ptr::null(),
        )
    };
    if ours == INVALID_HANDLE_VALUE {
        return Err(last_error());
    }
    // SAFETY: a valid handle the call above returned, owned from here.
    let ours = unsafe { OwnedHandle::from_raw_handle(ours) };
    let theirs_access = if inbound {
        windows_sys::Win32::Foundation::GENERIC_WRITE | FILE_READ_ATTRIBUTES
    } else {
        windows_sys::Win32::Foundation::GENERIC_READ | FILE_WRITE_ATTRIBUTES
    };
    // SAFETY: as above; the handle is not inheritable.
    let theirs = unsafe {
        CreateFileW(
            name.as_ptr(),
            theirs_access,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if theirs == INVALID_HANDLE_VALUE {
        return Err(last_error());
    }
    // SAFETY: as above.
    let theirs = unsafe { OwnedHandle::from_raw_handle(theirs) };
    Ok((ours, theirs))
}

/// How an overlapped transfer ended.
enum Transfer {
    Done(usize),
    /// Given up on, by the stop event or the deadline, with what it moved before.
    Interrupted(usize),
}

/// Runs one overlapped transfer on `file`, started by `start`, and waits for it to finish, for
/// `stop` to be set, or for `timeout`, whichever comes first. The transfer is always over when
/// this returns, cancelled where it was given up on, so `overlapped` and the buffer `start` passed
/// are free again.
fn transfer(
    file: HANDLE,
    event: &Event,
    stop: Option<&Event>,
    timeout: Option<Duration>,
    start: impl FnOnce(*mut OVERLAPPED) -> windows_sys::core::BOOL,
) -> io::Result<Transfer> {
    let mut overlapped = OVERLAPPED {
        hEvent: event.raw(),
        ..Default::default()
    };
    if start(&mut overlapped) == 0 {
        // SAFETY: a plain call.
        let code = unsafe { GetLastError() };
        if code != ERROR_IO_PENDING {
            return Err(io::Error::from_raw_os_error(code as i32));
        }
        let waits: Vec<HANDLE> = std::iter::once(event.raw())
            .chain(stop.map(Event::raw))
            .collect();
        let millis = timeout.map_or(INFINITE, |timeout| {
            timeout.as_millis().min(u128::from(INFINITE - 1)) as u32
        });
        // SAFETY: every handle is open for the wait.
        let woke = unsafe { WaitForMultipleObjects(waits.len() as u32, waits.as_ptr(), 0, millis) };
        // A failed wait is an error, not the stop: the transfer is cancelled all the same.
        let failed = (woke == WAIT_FAILED).then(last_error);
        if woke != WAIT_OBJECT_0 {
            let mut moved = 0u32;
            // SAFETY: `overlapped` is the pending transfer's; the wait for its end makes the
            // cancellation complete before `overlapped` goes.
            unsafe {
                CancelIoEx(file, &overlapped);
                GetOverlappedResult(file, &overlapped, &mut moved, 1);
            }
            if let Some(err) = failed {
                return Err(err);
            }
            return Ok(Transfer::Interrupted(overlapped.InternalHigh));
        }
    }
    let mut moved = 0u32;
    // SAFETY: the transfer is over or completes during the call, which waits for it.
    if unsafe { GetOverlappedResult(file, &overlapped, &mut moved, 1) } == 0 {
        return Err(last_error());
    }
    Ok(Transfer::Done(moved as usize))
}

/// Reads once from the output pipe into `buf`: `Ok(Some(0))` once the host has gone, `Ok(None)` when
/// `stop` was set first.
fn read_once(
    output: HANDLE,
    event: &Event,
    stop: Option<&Event>,
    buf: &mut [u8],
) -> io::Result<Option<usize>> {
    let len = buf.len().min(u32::MAX as usize) as u32;
    let read = transfer(output, event, stop, None, |overlapped| {
        // SAFETY: `buf` is writable for `len` bytes until the transfer is over, which `transfer`
        // waits for.
        unsafe {
            ReadFile(
                output,
                buf.as_mut_ptr(),
                len,
                std::ptr::null_mut(),
                overlapped,
            )
        }
    });
    match read {
        Ok(Transfer::Done(n)) => Ok(Some(n)),
        Ok(Transfer::Interrupted(_)) => Ok(None),
        Err(err)
            if [ERROR_BROKEN_PIPE, ERROR_PIPE_NOT_CONNECTED]
                .iter()
                .any(|&code| err.raw_os_error() == Some(code as i32)) =>
        {
            Ok(Some(0))
        }
        Err(err) => Err(err),
    }
}

/// Writes every byte of `data` into the input pipe, in slices, failing with `TimedOut` once the
/// host has taken nothing for [`WRITE_DEADLINE`].
fn write_all(input: HANDLE, data: &[u8]) -> Result<(), PartialWrite> {
    let event = Event::new().map_err(|error| PartialWrite { written: 0, error })?;
    let mut offset = 0;
    while offset < data.len() {
        let slice = &data[offset..(offset + WRITE_SLICE).min(data.len())];
        let written = transfer(input, &event, None, Some(WRITE_DEADLINE), |overlapped| {
            // SAFETY: `slice` is readable for its length until the transfer is over, which
            // `transfer` waits for.
            unsafe {
                WriteFile(
                    input,
                    slice.as_ptr(),
                    slice.len() as u32,
                    std::ptr::null_mut(),
                    overlapped,
                )
            }
        });
        match written {
            Ok(Transfer::Done(n)) => offset += n,
            Ok(Transfer::Interrupted(n)) => {
                return Err(PartialWrite {
                    written: offset + n,
                    error: io::Error::new(
                        io::ErrorKind::TimedOut,
                        "the session did not accept input within the write deadline",
                    ),
                })
            }
            Err(error) => {
                return Err(PartialWrite {
                    written: offset,
                    error,
                })
            }
        }
    }
    Ok(())
}

// --- Command lines and environments.

/// The command line and the application `command` is started with: the program and its arguments
/// quoted as the C runtime splits them, or, for a batch file, `cmd` running it with its arguments
/// quoted as `cmd` reads them.
fn command_line(command: &Command) -> io::Result<(Option<Vec<u16>>, Vec<u16>)> {
    let program = Path::new(&command.program);
    let batch = program
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
        });
    let mut line = Vec::new();
    let application = if batch {
        let cmd = std::env::var_os("ComSpec")
            .filter(|path| Path::new(path).is_absolute())
            .unwrap_or_else(|| OsString::from(r"C:\Windows\System32\cmd.exe"));
        line.extend("cmd.exe /e:ON /v:OFF /d /c \"".encode_utf16());
        push_program(&mut line, &command.program)?;
        for arg in &command.args {
            line.push(u16::from(b' '));
            push_batch_arg(&mut line, arg)?;
        }
        line.push(u16::from(b'"'));
        Some(wide(&cmd))
    } else {
        push_program(&mut line, &command.program)?;
        for arg in &command.args {
            line.push(u16::from(b' '));
            push_arg(&mut line, arg)?;
        }
        None
    };
    line.push(0);
    Ok((application, line))
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

/// The program, quoted whole: a file name holds no `"` and cannot end in `\`.
fn push_program(line: &mut Vec<u16>, program: &OsStr) -> io::Result<()> {
    let units: Vec<u16> = program.encode_wide().collect();
    if units.is_empty()
        || units
            .iter()
            .any(|&unit| unit == 0 || unit == u16::from(b'"'))
        || units.last() == Some(&u16::from(b'\\'))
    {
        return Err(invalid(
            "the program's path cannot be put on a command line",
        ));
    }
    line.push(u16::from(b'"'));
    line.extend(units);
    line.push(u16::from(b'"'));
    Ok(())
}

/// One argument as the C runtime splits a command line: quoted when it is empty or holds a space
/// or a tab, a `"` escaped with a backslash, and the backslashes before a `"` (or before the
/// closing quote) doubled.
fn push_arg(line: &mut Vec<u16>, arg: &OsStr) -> io::Result<()> {
    let units: Vec<u16> = arg.encode_wide().collect();
    if units.contains(&0) {
        return Err(invalid("an argument holds a NUL"));
    }
    let quote = units.is_empty()
        || units
            .iter()
            .any(|&unit| unit == u16::from(b' ') || unit == u16::from(b'\t'));
    if quote {
        line.push(u16::from(b'"'));
    }
    let mut backslashes = 0usize;
    for unit in units {
        if unit == u16::from(b'\\') {
            backslashes += 1;
        } else {
            if unit == u16::from(b'"') {
                line.extend(std::iter::repeat_n(u16::from(b'\\'), backslashes + 1));
            }
            backslashes = 0;
        }
        line.push(unit);
    }
    if quote {
        line.extend(std::iter::repeat_n(u16::from(b'\\'), backslashes));
        line.push(u16::from(b'"'));
    }
    Ok(())
}

/// One argument of a batch file, as `cmd` reads it and the batch file's `%1` then holds it: quoted
/// unless every character is one `cmd` gives no meaning to, a `"` doubled, and a `%` broken up
/// (`%%cd:~,%`, an empty substring of a variable that always exists) so that nothing in it is
/// expanded. A line break would end the command, so it is refused.
fn push_batch_arg(line: &mut Vec<u16>, arg: &OsStr) -> io::Result<()> {
    let text: Vec<u16> = arg.encode_wide().collect();
    if text
        .iter()
        .any(|&unit| unit == 0 || unit == u16::from(b'\r') || unit == u16::from(b'\n'))
    {
        return Err(invalid(
            "a batch file's argument holds a NUL or a line break",
        ));
    }
    const UNQUOTED: &str = r"#$*+-./:?@\_";
    let quote = text.is_empty()
        || text.last() == Some(&u16::from(b'\\'))
        || char::decode_utf16(text.iter().copied()).any(|unit| match unit {
            Ok(c) => {
                (c.is_ascii() && !(c.is_ascii_alphanumeric() || UNQUOTED.contains(c)))
                    || c.is_control()
            }
            Err(_) => false,
        });
    if quote {
        line.push(u16::from(b'"'));
    }
    let mut backslashes = 0usize;
    for unit in text {
        if unit == u16::from(b'\\') {
            backslashes += 1;
        } else {
            if unit == u16::from(b'"') {
                line.extend(std::iter::repeat_n(u16::from(b'\\'), backslashes));
                line.push(u16::from(b'"'));
            } else if unit == u16::from(b'%') {
                line.extend("%%cd:~,".encode_utf16());
            }
            backslashes = 0;
        }
        line.push(unit);
    }
    if quote {
        line.extend(std::iter::repeat_n(u16::from(b'\\'), backslashes));
        line.push(u16::from(b'"'));
    }
    Ok(())
}

/// The environment block `command` is started over: the daemon's own under what it sets, or only
/// what it sets, each name once whatever its case, as Windows compares them.
fn environment_block(command: &Command) -> io::Result<Vec<u16>> {
    let mut variables: BTreeMap<String, (OsString, OsString)> = BTreeMap::new();
    let mut set = |key: &OsStr, value: &OsStr| {
        variables.insert(
            key.to_string_lossy().to_uppercase(),
            (key.to_os_string(), value.to_os_string()),
        );
    };
    if command.inherits_env {
        for (key, value) in std::env::vars_os() {
            set(&key, &value);
        }
    }
    for (key, value) in &command.env {
        set(key, value);
    }
    let mut block = Vec::new();
    for (key, value) in variables.values() {
        let key: Vec<u16> = key.encode_wide().collect();
        let value: Vec<u16> = value.encode_wide().collect();
        if key.is_empty() || key.contains(&0) || key[1..].contains(&u16::from(b'=')) {
            return Err(invalid("an environment variable's name cannot be set"));
        }
        if value.contains(&0) {
            return Err(invalid("an environment variable's value holds a NUL"));
        }
        block.extend(key);
        block.push(u16::from(b'='));
        block.extend(value);
        block.push(0);
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    Ok(block)
}

/// Encoded parameters shared by terminal and pipe-backed process creation.
pub(crate) struct ProcessParameters {
    pub(crate) application: Option<Vec<u16>>,
    pub(crate) line: Vec<u16>,
    pub(crate) environment: Vec<u16>,
    pub(crate) directory: Option<Vec<u16>>,
}

pub(crate) fn process_parameters(command: &Command) -> io::Result<ProcessParameters> {
    let (application, line) = command_line(command)?;
    Ok(ProcessParameters {
        application,
        line,
        environment: environment_block(command)?,
        directory: command.cwd.as_deref().map(|path| wide(path.as_os_str())),
    })
}

// --- Starting a process.

/// Opens a pseudoconsole of `size` and starts `command` on it, in its own job inside
/// `containment`'s when given, from its first instruction on.
pub fn spawn(command: Command, size: Size, containment: Option<&Containment>) -> Result<Spawned> {
    let joining = Joining::of(&command.program);
    spawn_joining(command, size, containment, joining)
}

/// [`spawn`], with the process put into its jobs as `joining` says.
fn spawn_joining(
    command: Command,
    size: Size,
    containment: Option<&Containment>,
    joining: Joining,
) -> Result<Spawned> {
    // Everything that can fail without a process to clean up comes first.
    let (application, mut line) = command_line(&command).context("building the command line")?;
    let environment = environment_block(&command).context("building the environment")?;
    let cwd = command.cwd.as_deref().map(|dir| wide(dir.as_os_str()));
    let (input, console_input) = pipe(false).context("opening the console's input pipe")?;
    let (output, console_output) = pipe(true).context("opening the console's output pipe")?;
    let stop = Event::new().context("creating the reader's stop event")?;
    let placement =
        Placement::new(Reach::Session, containment).context("creating the process's job")?;
    take_ctrl_c();

    let api = api();
    let mut console: HPCON = 0;
    // SAFETY: both pipe ends are open for the call; the host takes copies of them.
    let created = unsafe {
        (api.create)(
            coord(size),
            console_input.as_raw_handle(),
            console_output.as_raw_handle(),
            0,
            &mut console,
        )
    };
    drop((console_input, console_output));
    if created != 0 {
        anyhow::bail!("creating a pseudoconsole failed (HRESULT {created:#010x})");
    }
    let console = Console(console);

    let started = start_process(
        console.0,
        &placement,
        joining,
        application.as_deref(),
        &mut line,
        &environment,
        cwd.as_deref(),
    );
    let (pid, process) = match started {
        Ok(started) => started,
        Err(err) => {
            // SAFETY: nothing else has the console, and nothing was started on it.
            unsafe { (api.close)(console.0) };
            return Err(err).with_context(|| {
                format!(
                    "starting `{}` on a pseudoconsole",
                    Path::new(&command.program).display()
                )
            });
        }
    };
    report_host(api);
    let shared = Arc::new(Shared {
        api,
        console: Mutex::new(Some(console)),
        input: Mutex::new(Some(Arc::new(input))),
        output,
        stop,
        stopped: AtomicBool::new(false),
        readers: AtomicUsize::new(0),
        closing: AtomicBool::new(false),
        draining: AtomicBool::new(false),
        size: Mutex::new(size),
        quit_keys: command.quit_keys,
        process,
        input_lock: Mutex::new(()),
    });
    let taken_on = shared
        .process
        .try_clone()
        .context("keeping a handle on the process")
        .and_then(|handle| {
            placement
                .started(ProcessId(pid), handle)
                .context("putting the process into its containment")
        });
    let child = match taken_on {
        Ok(child) => child,
        Err(err) => {
            // SAFETY: the handle is open; the process is the one just started.
            unsafe { TerminateProcess(shared.process.as_raw_handle(), KILLED) };
            shared.close_console();
            return Err(err);
        }
    };
    shared.watch_exit();
    let asked = shared.clone();
    let child = child.with_polite_stop(Box::new(move |how| asked.ask_to_end(how)));
    Ok(Spawned {
        child,
        pid: ProcessId(pid),
        pty: Pty { shared },
    })
}

/// Has the processes the daemon starts from here on take Ctrl-C. Ignoring Ctrl-C is a flag a
/// process inherits from the one that starts it (and gets when it is started in a process group of
/// its own), and with it set the console calls none of the process's handlers for a Ctrl-C, the
/// default one that ends it included: a Ctrl-C typed into a terminal would cut a line being read
/// short but interrupt nothing. The daemon may have been started with the flag set (a session over
/// SSH starts its programs so), so it clears it on itself, once, before its first terminal
/// process. Ctrl-C on a console of the daemon's own then reaches it too, which takes it as a
/// request to end.
fn take_ctrl_c() {
    static CLEARED: Once = Once::new();
    CLEARED.call_once(|| {
        // SAFETY: with no handler and FALSE the call only clears the daemon's own flag.
        if unsafe { SetConsoleCtrlHandler(None, 0) } == 0 {
            tracing::warn!(err = %last_error(), "taking Ctrl-C failed; a Ctrl-C typed into a terminal may interrupt nothing");
        }
    });
}

/// How a process is put into its jobs. Either way it is in them before it runs its first
/// instruction, so nothing it starts is outside them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Joining {
    /// Named in its creation attributes (`PROC_THREAD_ATTRIBUTE_JOB_LIST`).
    AtCreation,
    /// Created suspended without them, with the desktop app policy that keeps what it starts in
    /// them, put into them, then let run.
    WhileSuspended,
}

impl Joining {
    /// How the process of `program` joins its jobs: while suspended where `program` is an app
    /// execution alias ([`fs::is_app_execution_alias`]), at creation otherwise. Starting an alias
    /// starts a packaged app, and while any process of that package runs for the user (a
    /// PowerShell 7 from the Store open in a window of its own, say) its creation with the job list
    /// is refused as access denied; created suspended, it takes the same jobs.
    ///
    /// What a packaged app starts otherwise leaves every job it is in (the desktop app breakaway),
    /// so its jobs would hold the app alone: a shell's busy reading and the end of its jobs would
    /// miss what it runs. The policy `DISABLE_PROCESS_TREE` keeps all of it in, and has it run in
    /// the app's package, with the package's identity.
    fn of(program: &OsStr) -> Self {
        if fs::is_app_execution_alias(Path::new(program)) {
            Self::WhileSuspended
        } else {
            Self::AtCreation
        }
    }
}

/// Starts the process of `line` on `console`, in the jobs of `placement` from its first
/// instruction on, as `joining` says. Returns its pid and a handle on it. A process that started
/// but could not be kept is ended here.
fn start_process(
    console: HPCON,
    placement: &Placement,
    joining: Joining,
    application: Option<&[u16]>,
    line: &mut [u16],
    environment: &[u16],
    cwd: Option<&[u16]>,
) -> Result<(u32, OwnedHandle)> {
    let mut list_size = 0usize;
    // SAFETY: a null list asks for the size a list of three attributes needs.
    unsafe { InitializeProcThreadAttributeList(std::ptr::null_mut(), 3, 0, &mut list_size) };
    let mut list_buffer = vec![0u64; list_size.div_ceil(8)];
    let list = list_buffer.as_mut_ptr() as LPPROC_THREAD_ATTRIBUTE_LIST;
    // SAFETY: `list_buffer` is writable for the size the call asked for.
    if unsafe { InitializeProcThreadAttributeList(list, 3, 0, &mut list_size) } == 0 {
        return Err(last_error()).context("initialising the process's attributes");
    }
    struct List(LPPROC_THREAD_ATTRIBUTE_LIST);
    impl Drop for List {
        fn drop(&mut self) {
            // SAFETY: an initialised list, deleted once.
            unsafe { DeleteProcThreadAttributeList(self.0) };
        }
    }
    let list = List(list);
    // SAFETY: the value is the console's handle itself, as the attribute takes it; the list is
    // initialised.
    if unsafe {
        UpdateProcThreadAttribute(
            list.0,
            0,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
            console as *const std::ffi::c_void,
            size_of::<HPCON>(),
            std::ptr::null_mut(),
            std::ptr::null(),
        )
    } == 0
    {
        return Err(last_error()).context("attaching the pseudoconsole");
    }
    let jobs = placement.job_list();
    // SAFETY: `jobs` outlives the process's creation, which reads it; the list is initialised.
    if joining == Joining::AtCreation
        && unsafe {
            UpdateProcThreadAttribute(
                list.0,
                0,
                PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
                jobs.as_ptr().cast(),
                std::mem::size_of_val(jobs.as_slice()),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        } == 0
    {
        return Err(last_error()).context("naming the process's jobs");
    }
    let policy = DESKTOP_APP_BREAKAWAY_DISABLE_PROCESS_TREE;
    // SAFETY: `policy` outlives the process's creation, which reads it; the list is initialised.
    if joining == Joining::WhileSuspended
        && unsafe {
            UpdateProcThreadAttribute(
                list.0,
                0,
                PROC_THREAD_ATTRIBUTE_DESKTOP_APP_POLICY as usize,
                std::ptr::from_ref(&policy).cast(),
                size_of::<u32>(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        } == 0
    {
        return Err(last_error()).context("keeping what the process starts in its jobs");
    }
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    // No standard handles of the daemon's: the process's are the pseudoconsole's.
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = INVALID_HANDLE_VALUE;
    startup.StartupInfo.hStdOutput = INVALID_HANDLE_VALUE;
    startup.StartupInfo.hStdError = INVALID_HANDLE_VALUE;
    startup.lpAttributeList = list.0;
    let suspended = match joining {
        Joining::AtCreation => 0,
        Joining::WhileSuspended => CREATE_SUSPENDED,
    };
    let mut information = PROCESS_INFORMATION::default();
    // SAFETY: every string is NUL-terminated (the environment block double-terminated) and alive
    // for the call; `line` is writable, as the call requires.
    let created = unsafe {
        CreateProcessW(
            application.map_or(std::ptr::null(), <[u16]>::as_ptr),
            line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT | suspended,
            environment.as_ptr().cast(),
            cwd.map_or(std::ptr::null(), <[u16]>::as_ptr),
            &startup.StartupInfo,
            &mut information,
        )
    };
    if created == 0 {
        return Err(last_error()).context("creating the process");
    }
    // SAFETY: valid handles the call returned, owned from here.
    let (process, thread) = unsafe {
        (
            OwnedHandle::from_raw_handle(information.hProcess),
            OwnedHandle::from_raw_handle(information.hThread),
        )
    };
    if joining == Joining::WhileSuspended {
        let joined = placement
            .assign(process.as_raw_handle())
            .context("putting the process into its jobs")
            .and_then(|()| {
                // SAFETY: the handle is on the process's one thread, with every access.
                if unsafe { ResumeThread(thread.as_raw_handle()) } == u32::MAX {
                    return Err(last_error()).context("letting the process run");
                }
                Ok(())
            });
        if let Err(err) = joined {
            // SAFETY: the handle is open; the process is the one just created.
            unsafe { TerminateProcess(process.as_raw_handle(), KILLED) };
            return Err(err);
        }
    }
    Ok((information.dwProcessId, process))
}

// --- The terminal.

/// What the terminal, its reader, the process's polite stop and the threads that close the console
/// share.
struct Shared {
    api: &'static Api,
    /// `None` once the console is closed, or being closed.
    console: Mutex<Option<Console>>,
    /// The daemon's end of the input pipe; `None` once the terminal is hung up. A write in progress
    /// holds its own reference.
    input: Mutex<Option<Arc<OwnedHandle>>>,
    /// The daemon's end of the output pipe.
    output: OwnedHandle,
    /// Set for good by [`Pty::stop_reader`]: every read returns `Ok(None)` from then on.
    stop: Event,
    stopped: AtomicBool,
    /// How many [`PtyReader`]s exist.
    readers: AtomicUsize,
    /// The console is closed or being closed.
    closing: AtomicBool,
    /// A drainer reads the output in place of a reader.
    draining: AtomicBool,
    size: Mutex<Size>,
    quit_keys: Option<QuitKeys>,
    /// A handle on the process, to wait on its exit.
    process: OwnedHandle,
    /// See [`Pty::input_lock`]; the quit keys are pressed under it too, so that none lands
    /// inside a paste.
    input_lock: Mutex<()>,
}

impl Shared {
    /// Whether the process exits within `timeout`; a timeout longer than a wait takes waits without
    /// end.
    fn exits_within(&self, timeout: Duration) -> bool {
        let millis = u32::try_from(timeout.as_millis()).unwrap_or(INFINITE);
        // SAFETY: the handle is open for the wait.
        unsafe { WaitForSingleObject(self.process.as_raw_handle(), millis) == WAIT_OBJECT_0 }
    }

    /// Closes the console once the process has exited, as its output never ends otherwise. On a
    /// thread of its own, which ends with the process.
    fn watch_exit(self: &Arc<Self>) {
        let shared = self.clone();
        let watching = std::thread::Builder::new()
            .name("conpty-exit".to_string())
            .spawn(move || {
                shared.exits_within(Duration::MAX);
                std::thread::sleep(EXIT_SETTLE);
                shared.close_console();
            });
        if let Err(err) = watching {
            tracing::warn!(%err, "starting the pseudoconsole's exit watch failed; its reader ends only with a hang-up");
        }
    }

    /// Closes the console, which asks every process on it to end, on a thread of its own: the close
    /// waits for the host to have its output read, so it must not run where the reading is done or
    /// waited on.
    fn close_console(self: &Arc<Self>) {
        let Some(console) = self.console.lock().expect("console mutex poisoned").take() else {
            return;
        };
        self.closing.store(true, Ordering::SeqCst);
        self.drain_if_unread();
        let close = self.api.close;
        let closing = std::thread::Builder::new()
            .name("conpty-close".to_string())
            .spawn(move || {
                let console = console;
                // SAFETY: the console was taken out of the terminal, so nothing else uses it.
                unsafe { close(console.0) }
            });
        if let Err(err) = closing {
            tracing::warn!(%err, "starting the pseudoconsole's close failed; it stays open");
        }
    }

    /// Starts reading the output and dropping it once the console is closing and no reader takes
    /// it, so that the close can finish. Once per terminal. Each of the close, the stop of the
    /// readers and the drop of the last reader writes its own flag and then reads the others';
    /// those writes and reads are sequentially consistent, so of two that race at least one sees
    /// both, and the drain is never left to nobody.
    fn drain_if_unread(self: &Arc<Self>) {
        if !self.closing.load(Ordering::SeqCst)
            || !(self.stopped.load(Ordering::SeqCst) || self.readers.load(Ordering::SeqCst) == 0)
            || self.draining.swap(true, Ordering::AcqRel)
        {
            return;
        }
        let shared = self.clone();
        let draining = std::thread::Builder::new()
            .name("conpty-drain".to_string())
            .spawn(move || {
                let Ok(event) = Event::new() else {
                    return;
                };
                let mut buf = vec![0u8; 8192];
                while let Ok(Some(n)) =
                    read_once(shared.output.as_raw_handle(), &event, None, &mut buf)
                {
                    if n == 0 {
                        break;
                    }
                }
            });
        if let Err(err) = draining {
            tracing::warn!(%err, "starting the pseudoconsole's drain failed; its close may not finish");
        }
    }

    /// A polite request to end the process: its quit keys first, when it declared them and is
    /// asked to terminate, then the close of its console. Made on a thread of its own, as it takes
    /// seconds; what comes of it the caller finds out by watching the process.
    fn ask_to_end(self: &Arc<Self>, how: Polite) -> Sent {
        if !is_running(self.process.as_raw_handle()) {
            return Sent::Gone;
        }
        let keys = match how {
            Polite::Terminate => self.quit_keys,
            Polite::HangUp => None,
        };
        let shared = self.clone();
        let asking = std::thread::Builder::new()
            .name("conpty-stop".to_string())
            .spawn(move || {
                if let Some(keys) = keys {
                    shared.press(keys);
                }
                shared.close_console();
            });
        match asking {
            Ok(_) => Sent::Delivered,
            Err(err) => Sent::Failed(err),
        }
    }

    /// Presses the quit keys and waits for the process to quit, for as long as they say.
    fn press(&self, keys: QuitKeys) {
        let bytes = encode(keys.key);
        for press in 0..keys.presses {
            if press > 0 && self.exits_within(keys.apart) {
                return;
            }
            let _writing = self.input_lock.lock().expect("input lock poisoned");
            let Some(input) = self.input.lock().expect("input mutex poisoned").clone() else {
                return;
            };
            if let Err(err) = write_all(input.as_raw_handle(), &bytes) {
                tracing::debug!(%err, "pressing a quit key failed");
                return;
            }
        }
        self.exits_within(keys.wait);
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        // Only a terminal whose exit watch could not start gets here with its console open: the
        // watch keeps every other alive until it has closed theirs.
        if let Some(console) = self.console.get_mut().ok().and_then(Option::take) {
            // SAFETY: the console is this terminal's, and nothing else uses it any more.
            unsafe { (self.api.close)(console.0) };
        }
    }
}

/// The terminal a process runs on, from the daemon's side.
pub struct Pty {
    shared: Arc<Shared>,
}

/// Reads what the process on a [`Pty`] prints, from one thread at a time.
pub struct PtyReader {
    shared: Arc<Shared>,
    /// What the reader's overlapped reads signal; `None` when it could not be made, which makes
    /// every read fail.
    event: Option<Event>,
}

impl PtyReader {
    /// Waits for output, then reads once into `buf`. Returns `Ok(Some(0))` for end of stream, once
    /// the console is closed and the host has gone, and `Ok(None)` as soon as the reader has been
    /// stopped ([`Pty::stop_reader`], [`Pty::hang_up`]) instead: it reads nothing more.
    pub fn read_chunk(&self, buf: &mut [u8]) -> io::Result<Option<usize>> {
        if self.shared.stopped.load(Ordering::Acquire) {
            return Ok(None);
        }
        let Some(event) = &self.event else {
            return Err(io::Error::other("the reader has no event to wait on"));
        };
        read_once(
            self.shared.output.as_raw_handle(),
            event,
            Some(&self.shared.stop),
            buf,
        )
    }
}

impl Drop for PtyReader {
    fn drop(&mut self) {
        self.shared.readers.fetch_sub(1, Ordering::SeqCst);
        self.shared.drain_if_unread();
    }
}

impl Pty {
    /// The lock the owner makes every write into the input under, so that writes that belong
    /// together (a bracketed paste) are never split by another, a quit key pressed by a polite
    /// stop included.
    pub fn input_lock(&self) -> &Mutex<()> {
        &self.shared.input_lock
    }

    /// A reader of the terminal's output. While one exists and is not stopped, it is what drains
    /// the output, the close of the console included.
    pub fn reader(&self) -> PtyReader {
        self.shared.readers.fetch_add(1, Ordering::SeqCst);
        PtyReader {
            shared: self.shared.clone(),
            event: Event::new().ok(),
        }
    }

    /// Makes every reader of this terminal return `Ok(None)`, now and from then on, without
    /// closing anything.
    pub fn stop_reader(&self) {
        self.shared.stopped.store(true, Ordering::SeqCst);
        self.shared.stop.set();
        self.shared.drain_if_unread();
    }

    /// Writes `bytes` as they are: a client's keystrokes and its replies to the program's queries.
    /// Fails with `TimedOut` once the host has taken nothing for a few seconds.
    pub fn write_raw(&self, bytes: &[u8]) -> Result<(), PartialWrite> {
        let Some(input) = self
            .shared
            .input
            .lock()
            .expect("input mutex poisoned")
            .clone()
        else {
            return Err(hung_up());
        };
        write_all(input.as_raw_handle(), bytes)
    }

    /// Writes the daemon's own `input`, encoded as the terminal sends it, as [`Self::write_raw`]
    /// writes; the error counts encoded bytes. A NUL (`Ctrl('@')`) is refused before anything is
    /// written: the console host drops it on its way to a program reading its console as text, so
    /// the key would never arrive.
    pub fn write_input(&self, input: &[Input]) -> Result<(), PartialWrite> {
        if input.contains(&Input::Ctrl('@')) {
            return Err(PartialWrite {
                written: 0,
                error: io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "the console host drops a NUL, so Ctrl-@ cannot be sent",
                ),
            });
        }
        self.write_raw(&encode(input))
    }

    /// The console's size, as last set.
    pub fn size(&self) -> Result<Size> {
        if self
            .shared
            .console
            .lock()
            .expect("console mutex poisoned")
            .is_none()
        {
            anyhow::bail!("the terminal has been hung up");
        }
        Ok(*self.shared.size.lock().expect("size mutex poisoned"))
    }

    pub fn resize(&self, size: Size) -> Result<()> {
        let console = self.shared.console.lock().expect("console mutex poisoned");
        let Some(console) = console.as_ref() else {
            anyhow::bail!("the terminal has been hung up");
        };
        // SAFETY: the console is open while its lock is held.
        let resized = unsafe { (self.shared.api.resize)(console.0, coord(size)) };
        if resized != 0 {
            anyhow::bail!("resizing the pseudoconsole failed (HRESULT {resized:#010x})");
        }
        *self.shared.size.lock().expect("size mutex poisoned") = size;
        Ok(())
    }

    /// Hangs the terminal up, as closing a console window does: stops every reader, closes the
    /// console, which asks every process on it to end, and drains what the host still writes.
    /// Every write after this one fails.
    pub fn hang_up(&self) {
        self.stop_reader();
        self.shared
            .input
            .lock()
            .expect("input mutex poisoned")
            .take();
        self.shared.close_console();
    }

    /// How long a polite request to terminate the process takes before the console is closed: the
    /// presses of its quit keys and the wait on them, when it declared some.
    pub fn polite_stop_lead(&self, how: Polite) -> Duration {
        match (how, self.shared.quit_keys) {
            (Polite::Terminate, Some(keys)) => keys.lead(),
            _ => Duration::ZERO,
        }
    }

    /// Whether [`Self::hang_up`] has run.
    #[cfg(test)]
    pub fn is_hung_up(&self) -> bool {
        self.shared
            .input
            .lock()
            .expect("input mutex poisoned")
            .is_none()
    }

    /// A console has no foreground job to tell.
    pub fn foreground_job(&self) -> Foreground {
        Foreground::Unsupported
    }

    /// A console has no device a process opens it by.
    pub fn slave_path(&self) -> Option<PathBuf> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_of(program: &str, args: &[&str]) -> String {
        let mut command = Command::new(program);
        command.args(args);
        let (_, line) = command_line(&command).expect("a command line");
        String::from_utf16(&line[..line.len() - 1]).expect("UTF-16")
    }

    #[test]
    fn arguments_are_quoted_as_the_c_runtime_splits_them() {
        assert_eq!(
            line_of(
                r"C:\bin\tool.exe",
                &["plain", "two words", "", r#"a"b"#, r"tail\", r"q \"]
            ),
            r#""C:\bin\tool.exe" plain "two words" "" a\"b tail\ "q \\""#
        );
        assert_eq!(line_of("tool", &[r#"x\"y"#]), r#""tool" x\\\"y"#);
    }

    #[test]
    fn a_batch_file_runs_through_cmd_with_its_arguments_quoted_for_it() {
        assert_eq!(
            line_of(
                r"C:\npm\codex.cmd",
                &["-c", "a b", "100%", r#"say "hi""#, "x&y"]
            ),
            r#"cmd.exe /e:ON /v:OFF /d /c ""C:\npm\codex.cmd" -c "a b" "100%%cd:~,%" "say ""hi""" "x&y"""#
        );
        let mut command = Command::new(r"C:\x.bat");
        command.args(["line\nbreak"]);
        assert!(command_line(&command).is_err());
    }

    #[test]
    fn the_environment_holds_each_name_once_whatever_its_case() {
        let mut command = Command::new("tool");
        command
            .env_clear()
            .env("Path", r"C:\a")
            .env("PATH", r"C:\b")
            .env("TERM", "xterm-256color");
        let block = environment_block(&command).expect("a block");
        let text = String::from_utf16(&block).expect("UTF-16");
        assert_eq!(text, "PATH=C:\\b\0TERM=xterm-256color\0\0");
    }

    #[test]
    fn a_nul_is_refused_before_anything_is_written() {
        let spawned = spawn(
            crate::test_support::programs::sleep(30).term_command(),
            Size { rows: 24, cols: 80 },
            None,
        )
        .expect("the process starts");
        let refused = spawned
            .pty
            .write_input(&[Input::Text("x".into()), Input::Ctrl('@')])
            .expect_err("a NUL is refused");
        assert_eq!(
            (refused.written, refused.error.kind()),
            (0, io::ErrorKind::InvalidInput)
        );
        assert!(spawned.child.kill().delivered());
    }

    /// The route an app execution alias takes, its desktop app policy included, on a program
    /// every host has: what the process starts is in its jobs too.
    #[test]
    fn a_process_put_into_its_jobs_while_suspended_is_in_them_with_what_it_starts() {
        let launch = format!("joining-{}", std::process::id());
        let containment = Containment::create("term-test", &launch).expect("a containment");
        let mut command = Command::new("cmd.exe");
        command.args(["/d", "/c", "ping", "-n", "30", "127.0.0.1"]);
        let spawned = spawn_joining(
            command,
            Size { rows: 24, cols: 80 },
            Some(&containment),
            Joining::WhileSuspended,
        )
        .expect("the process starts");
        assert_holds_what_it_starts(&containment, &spawned);
    }

    /// PowerShell 7 from the Store, through its app execution alias, while another of its
    /// processes runs for the user: the shell and what it runs are in its jobs. Not every host
    /// has it, so the test runs only when asked for.
    #[test]
    #[ignore = "needs PowerShell 7 from the Store; run with --ignored where it is installed"]
    fn a_store_powershell_is_in_its_jobs_with_what_it_runs() {
        let alias = PathBuf::from(std::env::var_os("LOCALAPPDATA").expect("LOCALAPPDATA"))
            .join(r"Microsoft\WindowsApps\pwsh.exe");
        assert!(
            fs::is_app_execution_alias(&alias),
            "{} is no app execution alias",
            alias.display()
        );
        let mut users = std::process::Command::new(&alias)
            .args(["-NoProfile", "-Command", "Start-Sleep 60"])
            .spawn()
            .expect("the user's PowerShell starts");
        std::thread::sleep(Duration::from_secs(2));
        let launch = format!("store-{}", std::process::id());
        let containment = Containment::create("term-test", &launch).expect("a containment");
        let mut command = Command::new(alias.into_os_string());
        command.args(["-NoLogo", "-NoProfile", "-Command", "ping -n 30 127.0.0.1"]);
        let spawned = spawn(command, Size { rows: 24, cols: 80 }, Some(&containment));
        let _ = users.kill();
        let _ = users.wait();
        let spawned = spawned.expect("the shell starts");
        assert_holds_what_it_starts(&containment, &spawned);
    }

    /// Waits for `containment` to hold `spawned` and a process it started, then ends them.
    fn assert_holds_what_it_starts(containment: &Containment, spawned: &Spawned) {
        let deadline = std::time::Instant::now() + crate::test_support::PATIENCE;
        loop {
            let members = containment.members().expect("the job is read");
            let child = members.iter().any(|&member| {
                member != spawned.pid && !crate::platform::proc::is_console_host(member)
            });
            if members.contains(&spawned.pid) && child {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the jobs hold {members:?}, not {:?} and what it started",
                spawned.pid
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(spawned.child.kill().delivered());
        spawned.child.kill_leftovers(&[]);
    }
}
