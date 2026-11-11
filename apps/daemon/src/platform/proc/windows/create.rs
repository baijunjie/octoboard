//! Processes created explicitly with `CreateProcessW` instead of `std::process::Command`, which
//! would have them inherit every inheritable handle of the daemon: an attribute list, the
//! standard handles a child gets and nothing else, the detached start of an application that
//! outlives the daemon, and the background start of the daemon the command line runs.

use std::ffi::OsStr;
use std::fs::File;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::ExitStatusExt;
use std::process::{Command, ExitStatus};

use windows_sys::Win32::Foundation::{
    DuplicateHandle, DUPLICATE_SAME_ACCESS, ERROR_ACCESS_DENIED, HANDLE, WAIT_FAILED, WAIT_TIMEOUT,
};
use windows_sys::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, GetCurrentProcess, GetExitCodeProcess,
    InitializeProcThreadAttributeList, UpdateProcThreadAttribute, WaitForSingleObject,
    CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW,
    CREATE_UNICODE_ENVIRONMENT, DETACHED_PROCESS, EXTENDED_STARTUPINFO_PRESENT, INFINITE,
    LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_CREATE_PROCESS, PROCESS_INFORMATION,
    PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROC_THREAD_ATTRIBUTE_PARENT_PROCESS, STARTF_USESTDHANDLES,
    STARTUPINFOEXW, STARTUPINFOW,
};

use super::{last_error, terminate};
use crate::platform::proc::Sent;
use crate::platform::proc_table::Process;
use crate::platform::term::ProcessParameters;

/// Starts `command` as a process that outlives the daemon, with no console, no inherited handles
/// and a process group of its own, and outside the jobs the daemon is in, which a kill-on-close job
/// would end it with. It runs in the command's working directory, and in the daemon's environment:
/// a command that sets its own is refused.
///
/// It asks to break away from the daemon's jobs first. A job that does not allow it refuses with
/// access denied; the process is then created as a child of the process `application` names (the
/// desktop application, which is outside the job), taking that process's job membership instead.
/// When there is no application, or it cannot be opened for that (it is gone, access is denied),
/// or creating a child of it fails, the process is started without breakaway and ends with the
/// job.
pub(crate) fn start_detached(command: Command, application: Option<u32>) -> io::Result<()> {
    if command.get_envs().next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a detached process starts in the daemon's environment",
        ));
    }
    let program = command.get_program();
    let mut line = Vec::new();
    push_quoted(&mut line, program)?;
    for arg in command.get_args() {
        line.push(u16::from(b' '));
        push_quoted(&mut line, arg)?;
    }
    line.push(0);
    let directory: Option<Vec<u16>> = command.get_current_dir().map(|directory| {
        directory
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    });
    let directory = directory.as_deref();

    match create_detached(&mut line, directory, Detached::Breakaway) {
        Err(err) if err.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) => {}
        started => return started,
    }
    let parent = match application {
        Some(pid) if pid != 0 => match Process::open(pid, PROCESS_CREATE_PROCESS) {
            Some(parent) => Some(parent),
            None => {
                let err = last_error();
                tracing::warn!(application = pid, %err, "the application cannot be opened to start a process under it");
                None
            }
        },
        _ => None,
    };
    if let Some(parent) = &parent {
        match create_detached(&mut line, directory, Detached::ChildOf(parent)) {
            Ok(()) => return Ok(()),
            Err(err) => {
                tracing::warn!(application = ?application, %err, "a process cannot be created under the application");
            }
        }
    }
    tracing::warn!(
        "the daemon's job does not let a process break away and none can be started under the application: the holder's terminals end with the job"
    );
    create_detached(&mut line, directory, Detached::InJob)
}

/// Where [`create_detached`] puts the process, relative to the creator's jobs.
#[derive(Clone, Copy)]
enum Detached<'a> {
    /// Out of them, by breakaway.
    Breakaway,
    /// In them: nothing is asked.
    InJob,
    /// As a child of the process this is a handle on, in the jobs it is in.
    ChildOf(&'a Process),
}

/// Appends `arg` to the command line `line` as `CommandLineToArgvW` reads it back.
fn push_quoted(line: &mut Vec<u16>, arg: &OsStr) -> io::Result<()> {
    let units: Vec<u16> = arg.encode_wide().collect();
    if units.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "an argument holds a NUL",
        ));
    }
    let plain = !units.is_empty()
        && !units.iter().any(|&unit| {
            unit == u16::from(b' ') || unit == u16::from(b'\t') || unit == u16::from(b'"')
        });
    if plain {
        line.extend(units);
        return Ok(());
    }
    line.push(u16::from(b'"'));
    let mut backslashes = 0;
    for unit in units {
        if unit == u16::from(b'\\') {
            backslashes += 1;
        } else {
            if unit == u16::from(b'"') {
                // The backslashes before a quote are doubled, and the quote itself escaped.
                line.extend(std::iter::repeat_n(u16::from(b'\\'), backslashes + 1));
            }
            backslashes = 0;
        }
        line.push(unit);
    }
    // Doubled again before the closing quote, which they would otherwise escape.
    line.extend(std::iter::repeat_n(u16::from(b'\\'), backslashes));
    line.push(u16::from(b'"'));
    Ok(())
}

/// Creates the process `line` (NUL-terminated) names, as [`start_detached`] describes, in
/// `directory` (NUL-terminated) when there is one, placed as `placement` says, and lets go of its
/// handles.
fn create_detached(
    line: &mut [u16],
    directory: Option<&[u16]>,
    placement: Detached<'_>,
) -> io::Result<()> {
    let mut flags = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOW>() as u32;
    if matches!(placement, Detached::Breakaway) {
        flags |= CREATE_BREAKAWAY_FROM_JOB;
    }

    // The list keeps a pointer to the handle, not the handle itself, so it lives until the
    // process is created.
    let parent_handle: [HANDLE; 1];
    let mut list: Option<Attributes> = None;
    if let Detached::ChildOf(parent) = placement {
        parent_handle = [parent.raw()];
        let attributes = list.insert(Attributes::new(1)?);
        attributes.handles(PROC_THREAD_ATTRIBUTE_PARENT_PROCESS, &parent_handle)?;
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.lpAttributeList = attributes.list;
        flags |= EXTENDED_STARTUPINFO_PRESENT;
    }

    let mut information = PROCESS_INFORMATION::default();
    // SAFETY: `line` is NUL-terminated and writable, as the call requires, and `directory` is
    // NUL-terminated when there is one; no handle is inherited, and the environment is the
    // daemon's own.
    let created = unsafe {
        CreateProcessW(
            std::ptr::null(),
            line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            flags,
            std::ptr::null(),
            directory.map_or(std::ptr::null(), <[u16]>::as_ptr),
            &startup.StartupInfo,
            &mut information,
        )
    };
    drop(list);
    if created == 0 {
        return Err(last_error());
    }
    // SAFETY: valid handles the call returned, closed here: nothing waits on the process.
    unsafe {
        drop(OwnedHandle::from_raw_handle(information.hProcess));
        drop(OwnedHandle::from_raw_handle(information.hThread));
    }
    Ok(())
}

/// An initialised process attribute list, with room for as many attributes as it was made for,
/// deleted when dropped.
pub(in crate::platform::proc) struct Attributes {
    list: LPPROC_THREAD_ATTRIBUTE_LIST,
    _buffer: Vec<usize>,
}

impl Attributes {
    pub(in crate::platform::proc) fn new(count: u32) -> io::Result<Self> {
        let mut bytes = 0;
        // SAFETY: a null pointer asks for the required allocation size.
        unsafe { InitializeProcThreadAttributeList(std::ptr::null_mut(), count, 0, &mut bytes) };
        let mut buffer = vec![0usize; bytes.div_ceil(size_of::<usize>())];
        let list = buffer.as_mut_ptr().cast();
        // SAFETY: the backing allocation is aligned and has the required size.
        if unsafe { InitializeProcThreadAttributeList(list, count, 0, &mut bytes) } == 0 {
            return Err(last_error());
        }
        Ok(Self {
            list,
            _buffer: buffer,
        })
    }

    /// Sets `attribute` to `handles`. The list keeps a pointer to the slice, which the caller
    /// keeps alive until `CreateProcessW` has read the attributes.
    pub(in crate::platform::proc) fn handles(
        &mut self,
        attribute: u32,
        handles: &[HANDLE],
    ) -> io::Result<()> {
        // SAFETY: the initialized list is writable, and the caller keeps the handles slice
        // alive until CreateProcessW has read the attributes.
        if unsafe {
            UpdateProcThreadAttribute(
                self.list,
                0,
                attribute as usize,
                handles.as_ptr().cast(),
                std::mem::size_of_val(handles),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        } == 0
        {
            return Err(last_error());
        }
        Ok(())
    }
}

impl Drop for Attributes {
    fn drop(&mut self) {
        // SAFETY: the initialized list is deleted once, before its backing allocation drops.
        unsafe { DeleteProcThreadAttributeList(self.list) };
    }
}

/// An inheritable copy of `handle`, for a process's explicit inheritance list
/// (`PROC_THREAD_ATTRIBUTE_HANDLE_LIST`), which takes only inheritable handles.
pub(in crate::platform::proc) fn inherited(handle: &impl AsRawHandle) -> io::Result<OwnedHandle> {
    let mut copy: HANDLE = std::ptr::null_mut();
    // SAFETY: the source and current-process handles are open, and `copy` is writable.
    if unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            handle.as_raw_handle(),
            GetCurrentProcess(),
            &mut copy,
            0,
            1,
            DUPLICATE_SAME_ACCESS,
        )
    } == 0
    {
        return Err(last_error());
    }
    // SAFETY: DuplicateHandle returned a new handle, exclusively owned here.
    Ok(unsafe { OwnedHandle::from_raw_handle(copy) })
}

/// The daemon [`spawn_in_background`] started, with the ends of its pipes the starter keeps, as
/// `std::process::Child` has them on Unix.
pub(crate) struct BackgroundChild {
    process: OwnedHandle,
    pid: u32,
    pub(crate) stdin: Option<io::PipeWriter>,
    pub(crate) stdout: Option<io::PipeReader>,
    pub(crate) stderr: Option<io::PipeReader>,
}

impl BackgroundChild {
    pub(crate) fn id(&self) -> u32 {
        self.pid
    }

    /// Ends the process; one that has already exited is not an error.
    pub(crate) fn kill(&mut self) -> io::Result<()> {
        match terminate(self.process.as_raw_handle()) {
            Sent::Failed(err) => Err(err),
            Sent::Delivered | Sent::Gone => Ok(()),
        }
    }

    /// Waits for the process to exit, and gives its exit status.
    pub(crate) fn wait(&mut self) -> io::Result<ExitStatus> {
        // SAFETY: the process handle stays open for the entire wait.
        if unsafe { WaitForSingleObject(self.process.as_raw_handle(), INFINITE) } == WAIT_FAILED {
            return Err(last_error());
        }
        self.exit_status()
    }

    /// The process's exit status if it has exited, without waiting.
    pub(crate) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        // SAFETY: the process handle is open for the call.
        match unsafe { WaitForSingleObject(self.process.as_raw_handle(), 0) } {
            WAIT_TIMEOUT => Ok(None),
            WAIT_FAILED => Err(last_error()),
            _ => self.exit_status().map(Some),
        }
    }

    /// The exit code of the process, which has exited.
    fn exit_status(&self) -> io::Result<ExitStatus> {
        let mut code = 0;
        // SAFETY: the process handle is open, and `code` is writable.
        if unsafe { GetExitCodeProcess(self.process.as_raw_handle(), &mut code) } == 0 {
            return Err(last_error());
        }
        Ok(ExitStatus::from_raw(code))
    }
}

/// Spawns `command` as the daemon the command line runs in the background: with a hidden console
/// of its own instead of the starter's, so that closing that console does not end it, in a process
/// group of its own, and breaking away from the starter's job where the job allows it. Its stdout
/// and stderr are pipes to the caller, and so is its stdin when `pipe_stdin`, `NUL` otherwise.
///
/// Those three are the only handles it inherits. `std::process::Command` would have it inherit
/// every inheritable handle of the starter, the standard handles the starter itself was given
/// among them: whatever reads the starter's output through a pipe (a PowerShell capture, `| more`,
/// an SSH session) would then wait for the daemon to exit, not the starter. Only the command's
/// program, arguments, environment variables and working directory are used: `env_clear`,
/// `creation_flags` and `raw_arg` are not, and a command that removes a variable is refused.
///
/// A job that does not allow breakaway (a kill-on-close job such as an SSH session's) refuses it
/// with access denied; the command is then spawned again without asking, in the job, and the
/// answer says so (`true`): the process ends with that job, and the caller tells whoever started
/// it.
pub(crate) fn spawn_in_background(
    command: &mut Command,
    pipe_stdin: bool,
) -> io::Result<(BackgroundChild, bool)> {
    let mut plain = crate::platform::term::Command::new(command.get_program());
    plain.args(command.get_args());
    for (key, value) in command.get_envs() {
        let value = value.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "a background start removes no environment variable",
            )
        })?;
        plain.env(key, value);
    }
    if let Some(directory) = command.get_current_dir() {
        plain.cwd(directory);
    }
    let mut parameters = crate::platform::term::process_parameters(&plain)?;

    // The starter's ends are not inheritable; the child's are inheritable copies, closed here once
    // the child has its own.
    let (stdin, input) = if pipe_stdin {
        let (reader, writer) = io::pipe()?;
        (Some(writer), inherited(&reader)?)
    } else {
        (None, inherited(&File::open("NUL")?)?)
    };
    let (stdout, out_handle) = output(true)?;
    let (stderr, error) = output(true)?;
    let handles = [
        input.as_raw_handle(),
        out_handle.as_raw_handle(),
        error.as_raw_handle(),
    ];

    let flags = CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW;
    let (started, in_starter_job) =
        match create_background(&mut parameters, handles, flags | CREATE_BREAKAWAY_FROM_JOB) {
            Err(err) if err.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) => {
                (create_background(&mut parameters, handles, flags)?, true)
            }
            started => (started?, false),
        };
    let (process, pid) = started;
    Ok((
        BackgroundChild {
            process,
            pid,
            stdin,
            stdout,
            stderr,
        },
        in_starter_job,
    ))
}

/// Creates the process `parameters` describe with `flags`, its standard input, output and error
/// the three `handles` (inheritable), which are all it inherits. The process handle and the pid.
fn create_background(
    parameters: &mut ProcessParameters,
    handles: [HANDLE; 3],
    flags: u32,
) -> io::Result<(OwnedHandle, u32)> {
    let mut attributes = Attributes::new(1)?;
    let (process, _thread, pid) =
        create_with_std_handles(parameters, &mut attributes, handles, flags)?;
    Ok((process, pid))
}

/// Creates the process `parameters` describe with `flags` added to the ones every explicit creation
/// here sets, its standard input, output and error the three `handles` (inheritable), which are
/// all it inherits. `attributes` holds the caller's own attributes and has room for one more, the
/// handle list. The process handle, its primary thread's and the pid.
pub(in crate::platform::proc) fn create_with_std_handles(
    parameters: &mut ProcessParameters,
    attributes: &mut Attributes,
    handles: [HANDLE; 3],
    flags: u32,
) -> io::Result<(OwnedHandle, OwnedHandle, u32)> {
    attributes.handles(PROC_THREAD_ATTRIBUTE_HANDLE_LIST, &handles)?;
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = handles[0];
    startup.StartupInfo.hStdOutput = handles[1];
    startup.StartupInfo.hStdError = handles[2];
    startup.lpAttributeList = attributes.list;
    let mut information = PROCESS_INFORMATION::default();
    // SAFETY: the encoded strings, the handles and the attribute list stay alive for the call.
    // Inheritance is on, and limited by the list to the three standard handles.
    let created = unsafe {
        CreateProcessW(
            parameters
                .application
                .as_ref()
                .map_or(std::ptr::null(), |text| text.as_ptr()),
            parameters.line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
            flags | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
            parameters.environment.as_ptr().cast(),
            parameters
                .directory
                .as_ref()
                .map_or(std::ptr::null(), |text| text.as_ptr()),
            &startup.StartupInfo,
            &mut information,
        )
    };
    if created == 0 {
        return Err(last_error());
    }
    // SAFETY: successful creation returned both handles, each exclusively owned here.
    let (process, thread) = unsafe {
        (
            OwnedHandle::from_raw_handle(information.hProcess),
            OwnedHandle::from_raw_handle(information.hThread),
        )
    };
    Ok((process, thread, information.dwProcessId))
}

/// A standard output or error for a process: the write end of a new pipe, with its read end for the
/// caller, when `read_output`, else `NUL`. The process's end is an inheritable copy.
pub(in crate::platform::proc) fn output(
    read_output: bool,
) -> io::Result<(Option<io::PipeReader>, OwnedHandle)> {
    if read_output {
        let (reader, writer) = io::pipe()?;
        Ok((Some(reader), inherited(&writer)?))
    } else {
        let null = File::options().write(true).open("NUL")?;
        Ok((None, inherited(&null)?))
    }
}
