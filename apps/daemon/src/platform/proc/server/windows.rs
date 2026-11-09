//! Pipe-backed server creation with its owner-bound job installed atomically by the kernel.
//! Starting suspended and assigning the job afterwards would leave an orphan if the owner died
//! between those calls, even though the server had not run an instruction yet.

use std::fs::OpenOptions;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

use windows_sys::Win32::Foundation::{DuplicateHandle, DUPLICATE_SAME_ACCESS, HANDLE, WAIT_FAILED};
use windows_sys::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, GetCurrentProcess,
    InitializeProcThreadAttributeList, ResumeThread, TerminateProcess, UpdateProcThreadAttribute,
    WaitForSingleObject, CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, CREATE_SUSPENDED,
    CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT, INFINITE,
    LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
    PROC_THREAD_ATTRIBUTE_JOB_LIST, STARTF_USESTDHANDLES, STARTUPINFOEXW,
};

use crate::platform::proc::{ChildHandle, Containment, Placement, ProcessId};
use crate::platform::term::{process_parameters, Command};

pub(super) struct Child {
    process: OwnedHandle,
    pub(super) stdout: Option<io::PipeReader>,
    pub(super) stderr: Option<io::PipeReader>,
}

impl Child {
    pub(super) fn wait(&mut self) -> io::Result<()> {
        // SAFETY: the process handle stays open for the entire wait.
        if unsafe { WaitForSingleObject(self.process.as_raw_handle(), INFINITE) } == WAIT_FAILED {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

/// A copy included in the explicit inheritance list. No owner job handle is inheritable.
fn inherited(handle: &impl AsRawHandle) -> io::Result<OwnedHandle> {
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
        return Err(io::Error::last_os_error());
    }
    // SAFETY: DuplicateHandle returned a new handle, exclusively owned here.
    Ok(unsafe { OwnedHandle::from_raw_handle(copy) })
}

fn output(read_output: bool) -> io::Result<(Option<io::PipeReader>, OwnedHandle)> {
    if read_output {
        let (reader, writer) = io::pipe()?;
        Ok((Some(reader), inherited(&writer)?))
    } else {
        let null = OpenOptions::new().write(true).open("NUL")?;
        Ok((None, inherited(&null)?))
    }
}

struct Attributes {
    list: LPPROC_THREAD_ATTRIBUTE_LIST,
    _buffer: Vec<usize>,
}

impl Attributes {
    fn new() -> io::Result<Self> {
        let mut bytes = 0;
        // SAFETY: a null pointer asks for the required allocation size.
        unsafe { InitializeProcThreadAttributeList(std::ptr::null_mut(), 2, 0, &mut bytes) };
        let mut buffer = vec![0usize; bytes.div_ceil(size_of::<usize>())];
        let list = buffer.as_mut_ptr().cast();
        // SAFETY: the backing allocation is aligned and has the required size.
        if unsafe { InitializeProcThreadAttributeList(list, 2, 0, &mut bytes) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            list,
            _buffer: buffer,
        })
    }

    fn handles(&mut self, attribute: u32, handles: &[HANDLE]) -> io::Result<()> {
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
            return Err(io::Error::last_os_error());
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

pub(super) fn start(
    command: &Command,
    read_output: bool,
    containment: Option<&Containment>,
) -> io::Result<(Child, ChildHandle)> {
    let placement = Placement::server(containment)?;
    let jobs = placement.job_list();
    let mut parameters = process_parameters(command)?;
    let null = OpenOptions::new().read(true).open("NUL")?;
    let input = inherited(&null)?;
    let (stdout, output) = output(read_output)?;
    let (stderr, error) = self::output(read_output)?;
    let handles = [
        input.as_raw_handle(),
        output.as_raw_handle(),
        error.as_raw_handle(),
    ];
    let mut attributes = Attributes::new()?;
    attributes.handles(PROC_THREAD_ATTRIBUTE_JOB_LIST, &jobs)?;
    attributes.handles(PROC_THREAD_ATTRIBUTE_HANDLE_LIST, &handles)?;
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = handles[0];
    startup.StartupInfo.hStdOutput = handles[1];
    startup.StartupInfo.hStdError = handles[2];
    startup.lpAttributeList = attributes.list;
    let mut information = PROCESS_INFORMATION::default();
    // SAFETY: all encoded strings and attribute values remain alive for the call. Only the
    // three listed standard handles are inherited; jobs are assigned by creation attributes.
    if unsafe {
        CreateProcessW(
            parameters
                .application
                .as_ref()
                .map_or(std::ptr::null(), |text| text.as_ptr()),
            parameters.line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
            CREATE_NO_WINDOW
                | CREATE_NEW_PROCESS_GROUP
                | CREATE_SUSPENDED
                | CREATE_UNICODE_ENVIRONMENT
                | EXTENDED_STARTUPINFO_PRESENT,
            parameters.environment.as_ptr().cast(),
            parameters
                .directory
                .as_ref()
                .map_or(std::ptr::null(), |text| text.as_ptr()),
            &startup.StartupInfo,
            &mut information,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful creation returned both exclusively owned handles.
    let (process, thread) = unsafe {
        (
            OwnedHandle::from_raw_handle(information.hProcess),
            OwnedHandle::from_raw_handle(information.hThread),
        )
    };
    let finish = || {
        let copy = process.try_clone()?;
        let handle = placement.started(ProcessId(information.dwProcessId), copy)?;
        // SAFETY: the primary thread is suspended and its handle remains open.
        if unsafe { ResumeThread(thread.as_raw_handle()) } == u32::MAX {
            return Err(io::Error::last_os_error());
        }
        Ok(handle)
    };
    let handle = match finish() {
        Ok(handle) => handle,
        Err(error) => {
            // SAFETY: this is the server just created here. It is still in its owner-bound job.
            unsafe {
                TerminateProcess(process.as_raw_handle(), 1);
                WaitForSingleObject(process.as_raw_handle(), INFINITE);
            }
            return Err(error);
        }
    };
    Ok((
        Child {
            process,
            stdout,
            stderr,
        },
        handle,
    ))
}
