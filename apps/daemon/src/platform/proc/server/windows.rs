//! Pipe-backed server creation with its owner-bound job installed atomically by the kernel.
//! Starting suspended and assigning the job afterwards would leave an orphan if the owner died
//! between those calls, even though the server had not run an instruction yet.

use std::fs::OpenOptions;
use std::io;
use std::os::windows::io::{AsRawHandle, OwnedHandle};

use windows_sys::Win32::Foundation::WAIT_FAILED;
use windows_sys::Win32::System::Threading::{
    ResumeThread, TerminateProcess, WaitForSingleObject, CREATE_NEW_PROCESS_GROUP,
    CREATE_NO_WINDOW, CREATE_SUSPENDED, INFINITE, PROC_THREAD_ATTRIBUTE_JOB_LIST,
};

use crate::platform::proc::{
    create_with_std_handles, inherited, output, Attributes, ChildHandle, Containment, Placement,
    ProcessId,
};
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
    let mut attributes = Attributes::new(2)?;
    attributes.handles(PROC_THREAD_ATTRIBUTE_JOB_LIST, &jobs)?;
    // Only the three standard handles are inherited; jobs are assigned by creation attributes.
    let (process, thread, pid) = create_with_std_handles(
        &mut parameters,
        &mut attributes,
        handles,
        CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP | CREATE_SUSPENDED,
    )?;
    let finish = || {
        let copy = process.try_clone()?;
        let handle = placement.started(ProcessId(pid), copy)?;
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
