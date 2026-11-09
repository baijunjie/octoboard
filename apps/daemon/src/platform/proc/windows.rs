//! Processes on Windows, held by their handles and told apart by job objects. A child the daemon
//! starts is placed, before it runs its first instruction, in a job of its own (its reach: the
//! group or session it leads, on Unix) and, for a launch, in the launch's named job inside which
//! that one nests; what it starts lands in the same jobs, so a job lists exactly what descends
//! from its process, wherever it went. A job is never closed with its processes in it, and never
//! terminated as a whole: kept processes are ended one by one, by handle, or left.
//!
//! A session here is what is attached to its leader's console: what a process starts shares its
//! console unless it is started detached or on a console of its own, which is how a process leaves
//! the session. A session's sweep ([`ChildHandle::kill_leftovers`]) reaches only what is still on
//! the console, so a tree that left it outlives the leader however long it stays in the job, as a
//! tree kept or carried over to another launch must.
//!
//! A child started with `std::process::Command` (or tokio's) is started suspended by [`lead_group`]
//! or [`lead_session`] and let run by [`ChildHandle::started`] once it is in its jobs. A child the
//! platform starts itself, on a pseudoconsole, gets its jobs in its creation attributes from a
//! [`Placement`].

use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::Arc;

use windows_sys::Wdk::System::Threading::{NtQueryInformationProcess, PROCESSINFOCLASS};
use windows_sys::Win32::Foundation::{
    DuplicateHandle, GetLastError, DUPLICATE_SAME_ACCESS, ERROR_ACCESS_DENIED,
    ERROR_INVALID_PARAMETER, ERROR_MORE_DATA, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob, JobObjectBasicProcessIdList,
    OpenJobObjectW, QueryInformationJobObject,
};
use windows_sys::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, GetCurrentProcess,
    InitializeProcThreadAttributeList, OpenThread, QueryFullProcessImageNameW, ResumeThread,
    TerminateProcess, UpdateProcThreadAttribute, WaitForSingleObject, CREATE_BREAKAWAY_FROM_JOB,
    CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, CREATE_SUSPENDED, DETACHED_PROCESS,
    EXTENDED_STARTUPINFO_PRESENT, INFINITE, LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_ALL_ACCESS,
    PROCESS_CREATE_PROCESS, PROCESS_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, PROC_THREAD_ATTRIBUTE_PARENT_PROCESS, STARTUPINFOEXW,
    STARTUPINFOW, THREAD_SUSPEND_RESUME,
};

use super::{tag_value, TAG_VARIABLE};
use super::{Polite, ProcessId, ProcessIdentity, Reach, Sent, SetEnv, Signal, Swept, Unreaped};
use crate::platform::proc_table::{is_running, start_of_handle, Process};

/// `JOB_OBJECT_ALL_ACCESS` (winnt.h): what a job is opened again with, to list, assign and query.
const JOB_OBJECT_ALL_ACCESS: u32 = 0x001F_001F;

/// `ProcessConsoleHostProcess` (ntpsapi.h), which `windows-sys` does not name: the console host
/// serving a process's console.
const PROCESS_CONSOLE_HOST_PROCESS: PROCESSINFOCLASS = 49;

/// The exit code a process the daemon ends is given: 1, as `kill` gives a process it ends on
/// Unix a status that is not success.
const KILLED: u32 = 1;

/// How many bytes of arguments a caller hands one program it starts, with room to spare below
/// the 32767 UTF-16 units a command line holds: an argument's UTF-8 bytes are never fewer than its
/// UTF-16 units, and the rest is left for the program's path and the quoting.
pub(crate) const ARGUMENT_BYTES: usize = 24 * 1024;

fn last_error() -> io::Error {
    io::Error::last_os_error()
}

/// `name` as a NUL-terminated wide string.
fn wide(name: &str) -> Vec<u16> {
    name.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A job object.
#[derive(Debug)]
pub(crate) struct Job(OwnedHandle);

impl Job {
    /// A new job with no name, which nothing else can open.
    fn anonymous() -> io::Result<Self> {
        // SAFETY: null attributes and a null name ask for a plain unnamed job.
        let raw = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        Self::owned(raw)
    }

    /// The job called `name`: created, or the one that has the name already.
    fn named(name: &str) -> io::Result<Self> {
        let name = wide(name);
        // SAFETY: `name` is NUL-terminated and alive for the call.
        let raw = unsafe { CreateJobObjectW(std::ptr::null(), name.as_ptr()) };
        Self::owned(raw)
    }

    /// The job called `name`, while something holds a handle on it, which keeps its name.
    fn open(name: &str) -> Option<Self> {
        let name = wide(name);
        // SAFETY: `name` is NUL-terminated and alive for the call.
        let raw = unsafe { OpenJobObjectW(JOB_OBJECT_ALL_ACCESS, 0, name.as_ptr()) };
        Self::owned(raw).ok()
    }

    fn owned(raw: HANDLE) -> io::Result<Self> {
        if raw.is_null() {
            return Err(last_error());
        }
        // SAFETY: a valid handle a job call returned, owned from here.
        Ok(Self(unsafe { OwnedHandle::from_raw_handle(raw) }))
    }

    /// The handle, for a process's creation attributes ([`Placement::job_list`]).
    pub(crate) fn raw(&self) -> HANDLE {
        self.0.as_raw_handle()
    }

    fn assign(&self, process: HANDLE) -> io::Result<()> {
        // SAFETY: both handles are open for the call.
        if unsafe { AssignProcessToJobObject(self.raw(), process) } == 0 {
            return Err(last_error());
        }
        Ok(())
    }

    /// Whether the process `process` is a handle on is in the job: what makes a pid listed by
    /// [`Self::pids`] still the member's once it is opened, since the handle then holds the pid.
    fn holds(&self, process: HANDLE) -> bool {
        let mut result = 0;
        // SAFETY: `result` is a valid place for the answer; both handles are open.
        unsafe { IsProcessInJob(process, self.raw(), &mut result) != 0 && result != 0 }
    }

    /// The pids of the processes in the job and in the jobs nested in it. A list that may have
    /// been cut short is not answered.
    fn pids(&self) -> io::Result<Vec<u32>> {
        let mut room = 64usize;
        for _ in 0..6 {
            // Two `u32` counts, then the pids as `usize`s.
            let mut list = vec![0usize; room + 1];
            // SAFETY: the list is writable for the size given.
            let read = unsafe {
                QueryInformationJobObject(
                    self.raw(),
                    JobObjectBasicProcessIdList,
                    list.as_mut_ptr().cast(),
                    (list.len() * size_of::<usize>()) as u32,
                    std::ptr::null_mut(),
                )
            };
            let assigned = list[0] as u32 as usize;
            let listed = (list[0] >> 32) as usize;
            // A list too small for every member fails with `ERROR_MORE_DATA` on some systems and
            // succeeds cut short on others; either way it is taken again with room for all.
            if read == 0 || listed < assigned {
                // SAFETY: a plain call.
                if read == 0 && unsafe { GetLastError() } != ERROR_MORE_DATA {
                    return Err(last_error());
                }
                room = assigned.max(room) * 2;
                continue;
            }
            return Ok(list[1..=listed.min(room)]
                .iter()
                .map(|&pid| pid as u32)
                .collect());
        }
        Err(io::Error::other(
            "the job's members kept outgrowing the list",
        ))
    }
}

/// The name of the job of the launch `launch` of the daemon instance `instance`. `Local\` names
/// live in the user's logon session.
fn job_name(instance: &str, launch: &str) -> String {
    format!(r"Local\octoboard-{instance}-{launch}")
}

/// What tells one launch's processes from any other's: the launch's named job, which every process
/// started into it, and what those start, is in. A job's name lasts only while a handle on it is
/// open, so each process started into it is given one ([`Placement::started`]): the next start of
/// the daemon finds the job by name again while such a process runs. Its members also carry the
/// launch's tag in their environment, though nothing here reads another process's environment.
#[derive(Debug, Clone)]
pub struct Containment {
    tag: String,
    job: Arc<Job>,
}

impl Containment {
    /// The containment of the launch `launch` of the daemon instance `instance`: its job, made
    /// now.
    pub fn create(instance: &str, launch: &str) -> io::Result<Self> {
        Ok(Self {
            tag: tag_value(instance, launch),
            job: Arc::new(Job::named(&job_name(instance, launch))?),
        })
    }

    /// Puts the launch's tag into `environment`. Nothing on Windows reads it back; it keeps the
    /// launch's environment the same on every host.
    pub fn prepare(&self, environment: &mut impl SetEnv) {
        environment.set_env(TAG_VARIABLE, &self.tag);
    }

    /// The processes in the launch's job, and in the jobs nested in it: what the launch started
    /// and what descends from that, the hidden console host a process started without a window
    /// was given included. `None` when the job cannot be read.
    pub fn members(&self) -> Option<Vec<ProcessId>> {
        let pids = self.job.pids().ok()?;
        Some(pids.into_iter().map(ProcessId).collect())
    }

    /// The containment of a launch of an earlier run of the daemon, while one of the processes it
    /// started still holds the job's name.
    pub fn reopen(instance: &str, launch: &str) -> Option<Self> {
        Some(Self {
            tag: tag_value(instance, launch),
            job: Arc::new(Job::open(&job_name(instance, launch))?),
        })
    }

    /// The containment of the launch `launch` of the daemon instance `instance`, which another
    /// process made, for a process that starts the launch's program on its behalf (a holder): on
    /// Windows the same as [`Self::create`], which opens the job by its name while its maker holds
    /// it.
    pub fn join_launch(instance: &str, launch: &str) -> io::Result<Self> {
        Self::create(instance, launch)
    }

    /// The containment of the agent launch `launch` of the daemon instance `instance`: on Windows
    /// the same as [`Self::create`], since a job is never terminated as a whole.
    pub fn for_launch(instance: &str, launch: &str) -> io::Result<Self> {
        Self::create(instance, launch)
    }

    /// The launches whose containments are still there from an earlier run, where the host can
    /// list them: none here, since a job is found only by its name ([`Self::reopen`]).
    pub fn leftovers(_instance: &str) -> Vec<String> {
        Vec::new()
    }

    /// Whether processes can be moved out of the containment: not here, since a process never
    /// leaves a job.
    pub fn releases_members(&self) -> bool {
        false
    }

    /// Moves processes into the containment, where the host can: not here, since a process
    /// never leaves a job.
    pub fn adopt(&self, _ids: &[ProcessIdentity]) {}

    /// Moves processes out of the containment so that its end does not reach them, where the
    /// host can: not here, where its end reaches nothing ([`Self::end`]), so none is left to it.
    pub fn release(&self, _ids: &[ProcessIdentity], _into: Option<&Containment>) -> bool {
        true
    }

    /// Kills what is left in the containment, where the host does that as a whole: not here,
    /// where kept processes stay in the job and the launch's processes are ended one by one.
    pub fn end(&self, _spared: &[(ProcessIdentity, Option<&Containment>)]) -> bool {
        true
    }
}

/// The jobs a child is put in as it starts: the containment's, when it has one, and a job of its
/// own nested in it, its reach. In that order, since a job a process joins after another must be
/// empty or already above it. A child the platform starts itself with `CreateProcessW` takes them
/// in its `PROC_THREAD_ATTRIBUTE_JOB_LIST` ([`Self::job_list`]), which puts it in them before it
/// runs; a child started suspended is put in them by [`Self::assign`] before it is let run, as
/// [`ChildHandle::started`] does.
pub(crate) struct Placement {
    containment: Option<Containment>,
    reach: Job,
    session: bool,
}

impl Placement {
    /// The jobs of a child that leads what `reach` names, in `containment` when given.
    pub(crate) fn new(reach: Reach, containment: Option<&Containment>) -> io::Result<Self> {
        Ok(Self {
            containment: containment.cloned(),
            reach: Job::anonymous()?,
            session: reach == Reach::Session,
        })
    }

    /// The job handles for the child's job list attribute, in the order it must join them.
    pub(crate) fn job_list(&self) -> Vec<HANDLE> {
        self.containment
            .iter()
            .map(|containment| containment.job.raw())
            .chain(std::iter::once(self.reach.raw()))
            .collect()
    }

    /// Puts the child `process` is a handle on into the jobs, for one started suspended.
    pub(crate) fn assign(&self, process: HANDLE) -> io::Result<()> {
        if let Some(containment) = &self.containment {
            containment.job.assign(process)?;
        }
        self.reach.assign(process)
    }

    /// The handle on the child `pid`, already in the jobs, whose `process` handle the caller hands
    /// over. The child is given a handle on the containment's job, which keeps the job's name for
    /// as long as the child runs; the child itself never learns of it.
    pub(crate) fn started(self, pid: ProcessId, process: OwnedHandle) -> io::Result<ChildHandle> {
        if let Some(containment) = &self.containment {
            let mut held: HANDLE = std::ptr::null_mut();
            // SAFETY: every handle is open for the call; the copy is made in the child's handle
            // table and closed when the child exits.
            let duplicated = unsafe {
                DuplicateHandle(
                    GetCurrentProcess(),
                    containment.job.raw(),
                    process.as_raw_handle(),
                    &mut held,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS,
                )
            };
            if duplicated == 0 {
                return Err(last_error());
            }
        }
        Ok(ChildHandle {
            pid,
            process,
            reach: self.reach,
            session: self.session,
            polite: None,
        })
    }
}

/// What a polite stop of a child does where the platform knows how, as a terminal's process is
/// asked to end through its terminal ([`ChildHandle::with_polite_stop`]).
pub(crate) type PoliteStop = Box<dyn Fn(Polite) -> Sent + Send + Sync>;

/// A child process the caller started, held by a handle on it, which keeps its pid its own, and
/// with the job that holds what it starts.
pub struct ChildHandle {
    pid: ProcessId,
    process: OwnedHandle,
    /// The job only this child and what descends from it are in.
    reach: Job,
    /// The child leads a session: its kill ends it alone, and what it started on its console is
    /// ended by [`Self::kill_leftovers`]. A group's kill ends everything in the reach.
    session: bool,
    polite: Option<PoliteStop>,
}

impl std::fmt::Debug for ChildHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ChildHandle")
            .field("pid", &self.pid)
            .field("session", &self.session)
            .field("polite", &self.polite.is_some())
            .finish()
    }
}

impl ChildHandle {
    /// Takes on the child `pid`, just started suspended through [`lead_group`] or
    /// [`lead_session`] (as `reach` names) and not yet waited on: puts it into its jobs, in
    /// `containment` when given, and lets it run. On an error the child is still suspended, for
    /// the caller to kill.
    pub fn started(
        pid: ProcessId,
        reach: Reach,
        containment: Option<&Containment>,
    ) -> io::Result<Self> {
        let process = Process::open(pid.0, PROCESS_ALL_ACCESS)
            .ok_or_else(last_error)?
            .into_handle();
        let placement = Placement::new(reach, containment)?;
        placement.assign(process.as_raw_handle())?;
        let handle = placement.started(pid, process)?;
        resume(pid)?;
        Ok(handle)
    }

    /// Has a polite stop of the child do `stop`, for a child the platform knows how to ask.
    pub(crate) fn with_polite_stop(mut self, stop: PoliteStop) -> Self {
        self.polite = Some(stop);
        self
    }

    pub fn pid(&self) -> ProcessId {
        self.pid
    }

    /// The handle on the process, for the crash cleanup.
    pub(crate) fn process(&self) -> HANDLE {
        self.process.as_raw_handle()
    }

    /// The child's reach, for the crash cleanup.
    pub(crate) fn reach(&self) -> &Job {
        &self.reach
    }

    /// Whether the child has exited. The handle keeps its pid its own either way, so the host can
    /// always say.
    pub fn exited_unreaped(&self) -> Option<bool> {
        Some(!is_running(self.process()))
    }

    /// Asks the child to end as `how` says, where it can be asked: a terminal's process through
    /// its terminal. A process without one has no request it takes, so that is unsupported and
    /// the caller ends it.
    pub fn stop(&self, how: Polite) -> Sent {
        match &self.polite {
            Some(stop) => stop(how),
            None => unsupported(),
        }
    }

    /// Ends the child, and for a group what it started too.
    pub fn kill(&self) -> Sent {
        if self.session {
            return terminate(self.process());
        }
        let swept = terminate_members(&self.reach, &[], None, None);
        let ended = terminate(self.process());
        if swept.killed > 0 {
            Sent::Delivered
        } else {
            ended
        }
    }

    /// Ends what the child started that is still on its console, its session, wherever it went
    /// in the reach, other than the child itself and the processes `keep` names. What was started
    /// detached or on a console of its own has left the session and is not reached. The child's
    /// console is still known once it has exited, so the sweep is best made then.
    pub fn kill_leftovers(&self, keep: &[ProcessIdentity]) -> Swept {
        match console_of(self.process()) {
            Console::Host(host) => terminate_members(&self.reach, keep, Some(self.pid), Some(host)),
            // A child that never connected to a console can have started nothing on one.
            Console::None => Swept {
                killed: 0,
                complete: true,
            },
            Console::Unknown => Swept {
                killed: 0,
                complete: false,
            },
        }
    }

    /// Whether the child has exited, which is all reaping is here: the handle holds the pid until
    /// it is dropped.
    pub fn reap_if_exited(&self) -> io::Result<bool> {
        Ok(!is_running(self.process()))
    }

    /// Whether the child's reach has a process besides the child, or `None` when it cannot be
    /// read.
    pub fn group_has_members(&self) -> Option<bool> {
        let pids = self.reach.pids().ok()?;
        Some(pids.into_iter().any(|pid| pid != self.pid.0))
    }
}

/// Lets the child `pid`, started suspended, run: resumes its one thread.
fn resume(pid: ProcessId) -> io::Result<()> {
    // SAFETY: a plain call; the handle is owned below.
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if raw == INVALID_HANDLE_VALUE {
        return Err(last_error());
    }
    // SAFETY: a valid handle the call above returned, owned from here.
    let snapshot = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut entry = THREADENTRY32 {
        dwSize: size_of::<THREADENTRY32>() as u32,
        ..Default::default()
    };
    let mut resumed = false;
    // SAFETY: `entry` is a `THREADENTRY32` with its size set, alive for each call.
    let mut more = unsafe { Thread32First(snapshot.as_raw_handle(), &mut entry) } != 0;
    while more {
        if entry.th32OwnerProcessID == pid.0 {
            // SAFETY: a plain call; the handle is owned below.
            let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
            if thread.is_null() {
                return Err(last_error());
            }
            // SAFETY: a valid handle the call above returned, owned from here.
            let thread = unsafe { OwnedHandle::from_raw_handle(thread) };
            // SAFETY: the handle is open with the access the call needs.
            if unsafe { ResumeThread(thread.as_raw_handle()) } == u32::MAX {
                return Err(last_error());
            }
            resumed = true;
        }
        // SAFETY: as above.
        more = unsafe { Thread32Next(snapshot.as_raw_handle(), &mut entry) } != 0;
    }
    if resumed {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "the started process has no thread to resume",
        ))
    }
}

/// What a polite request answers where the host has none to send.
fn unsupported() -> Sent {
    Sent::Failed(io::Error::new(
        io::ErrorKind::Unsupported,
        "a process without a terminal takes no polite request to end on Windows",
    ))
}

/// Ends the process `process` is a handle on.
fn terminate(process: HANDLE) -> Sent {
    if !is_running(process) {
        return Sent::Gone;
    }
    // SAFETY: the handle is open; the process it names is still that process.
    if unsafe { TerminateProcess(process, KILLED) } != 0 {
        return Sent::Delivered;
    }
    let err = last_error();
    // Ending a process that is already exiting is refused.
    if err.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) && !is_running(process) {
        Sent::Gone
    } else {
        Sent::Failed(err)
    }
}

/// Ends every process in `job` other than `spared`, the processes `keep` names and console hosts,
/// and, given `on_console`, other than those not attached to the console the host with that pid
/// serves, one by one: a pid the job listed is opened and checked to be in the job still before it
/// is ended, so it is never another process's. A console host serves every process on its console,
/// a kept one among them, and ends by itself once none is left. Passes repeat while one ends a
/// process the last did not, since a process may start another just before it is ended.
fn terminate_members(
    job: &Job,
    keep: &[ProcessIdentity],
    spared: Option<ProcessId>,
    on_console: Option<u32>,
) -> Swept {
    const MAX_PASSES: usize = 3;
    let mut ended = std::collections::HashSet::new();
    for _ in 0..MAX_PASSES {
        let Ok(pids) = job.pids() else {
            return Swept {
                killed: ended.len(),
                complete: false,
            };
        };
        let before = ended.len();
        for pid in pids {
            if Some(ProcessId(pid)) == spared || ended.contains(&pid) {
                continue;
            }
            let Some(process) = Process::open(
                pid,
                PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            ) else {
                continue;
            };
            if !job.holds(process.raw()) || runs_console_host(process.raw()) {
                continue;
            }
            // Only what is on the leader's console: one detached or on a console of its own has
            // left the session, and one still starting, not connected to a console yet, is left,
            // as which it joins is not known.
            if on_console.is_some_and(|host| console_of(process.raw()) != Console::Host(host)) {
                continue;
            }
            let start = start_of_handle(process.raw());
            if keep
                .iter()
                .any(|kept| kept.pid.0 == pid && Some(kept.start_micros) == start)
            {
                continue;
            }
            if terminate(process.raw()).delivered() {
                ended.insert(pid);
            }
        }
        if ended.len() == before {
            break;
        }
    }
    Swept {
        killed: ended.len(),
        complete: true,
    }
}

/// Has `command` start in a process group of its own, without a window of its own, and suspended,
/// for [`ChildHandle::started`] to put into its jobs and let run. It shares the daemon's console.
pub(crate) fn lead_group(command: &mut Command) {
    command.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_SUSPENDED);
}

/// Has `command` start as [`lead_group`] does, with a hidden console of its own instead of the
/// daemon's, so that nothing that ends the daemon's console reaches it.
pub(crate) fn lead_session(command: &mut Command) {
    command.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW | CREATE_SUSPENDED);
}

/// Has `command` start apart from the daemon, as [`lead_session`] does, but running at once:
/// for an application that outlives the daemon, which nothing waits on or ends.
pub(crate) fn detach(command: &mut Command) {
    command.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
}

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

    // Declared before the list, which keeps a pointer to it: it must outlive the list.
    let parent_handle: HANDLE;
    let mut list_buffer = Vec::<u64>::new();
    let mut list: Option<AttributeList> = None;
    let parent = match placement {
        Detached::ChildOf(parent) => Some(parent),
        _ => None,
    };
    if let Some(parent) = parent {
        let mut list_size = 0usize;
        // SAFETY: a null list asks for the size a list of one attribute needs.
        unsafe { InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut list_size) };
        list_buffer.resize(list_size.div_ceil(8), 0);
        let raw = list_buffer.as_mut_ptr() as LPPROC_THREAD_ATTRIBUTE_LIST;
        // SAFETY: `list_buffer` is writable for the size the call asked for.
        if unsafe { InitializeProcThreadAttributeList(raw, 1, 0, &mut list_size) } == 0 {
            return Err(last_error());
        }
        let attributes = list.insert(AttributeList(raw));
        parent_handle = parent.raw();
        // SAFETY: the list keeps the pointer it is given, not the handle behind it, and
        // `parent_handle` is declared before the list, so it lives until the list is deleted,
        // after the process is created; the list is initialised.
        if unsafe {
            UpdateProcThreadAttribute(
                attributes.0,
                0,
                PROC_THREAD_ATTRIBUTE_PARENT_PROCESS as usize,
                std::ptr::from_ref(&parent_handle).cast(),
                size_of::<HANDLE>(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        } == 0
        {
            return Err(last_error());
        }
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.lpAttributeList = attributes.0;
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

/// An initialised process attribute list, deleted when dropped.
struct AttributeList(LPPROC_THREAD_ATTRIBUTE_LIST);

impl Drop for AttributeList {
    fn drop(&mut self) {
        // SAFETY: an initialised list, deleted once.
        unsafe { DeleteProcThreadAttributeList(self.0) };
    }
}

/// Starting the daemon in the background from the command line has no Windows implementation.
pub(crate) fn run_in_background(_command: &mut Command) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "running in the background is not supported on Windows",
    ))
}

/// Whether [`run_in_background`] can start a daemon here: never on Windows.
pub(crate) fn runs_in_background() -> bool {
    false
}

/// Whether a service manager restarts this daemon on the status a planned restart ends it with:
/// none does on Windows, whatever `--service-managed` asked.
pub(crate) fn restarted_by_service_manager(_asked: bool) -> bool {
    false
}

/// A planned-restart request has no Windows delivery: a process without a console takes no
/// request from another one but to be ended.
pub(crate) fn request_restart(_id: &ProcessIdentity) -> Sent {
    Sent::Failed(io::Error::new(
        io::ErrorKind::Unsupported,
        "a planned-restart request cannot be delivered on Windows",
    ))
}

/// Nothing to leave: a process on Windows is in no session a command's end could signal, and the
/// job it was started in ends with nothing the daemon does.
pub(crate) fn leave_session() {}

/// No child is told by its pid alone: `None`, as for a child already reaped.
#[allow(
    dead_code,
    reason = "only a test of the Codex transport asks it on Windows"
)]
pub(crate) fn unreaped_state(_pid: ProcessId) -> Option<Unreaped> {
    None
}

/// Ends the process `id` names, or, for a polite request, answers that it takes none. Its pid is
/// opened once, and the process behind that handle is checked to have started when `id` says
/// before it is ended through the same handle, so a pid passed to another process is never ended.
pub(crate) fn signal_identified(id: &ProcessIdentity, signal: Signal) -> Sent {
    match signal {
        Signal::Polite(_) => unsupported(),
        Signal::Kill => match Process::open(
            id.pid.0,
            PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
        ) {
            Some(process) if start_of_handle(process.raw()) == Some(id.start_micros) => {
                terminate(process.raw())
            }
            Some(_) => Sent::Gone,
            None => {
                let err = last_error();
                if err.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
                    Sent::Gone
                } else {
                    Sent::Failed(err)
                }
            }
        },
    }
}

/// A group is reached only through its leader's [`ChildHandle`], whose job holds it: a polite
/// request has nothing to go to, and a kill goes through the handle.
pub(crate) fn signal_group(_leader: ProcessId, _signal: Signal) -> Sent {
    unsupported()
}

/// The console a process is attached to, as the system tells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Console {
    /// The console the host with this pid serves.
    Host(u32),
    /// None: the process was started detached, or has not connected to its console yet.
    None,
    /// The system did not say.
    Unknown,
}

/// The console the process `process` is a handle on is attached to. Still answered once the
/// process has exited, for the console it last ran on.
///
/// The system answers the host's pid with the low bit set once the process has connected to a
/// console, and 1 for a process started detached; until it connects, the pid of the process that
/// started it, without that bit. The two low bits of a pid are always clear.
///
/// One system call and nothing else, so the crash cleanup can make it: it neither allocates nor
/// takes a lock.
pub(super) fn console_of(process: HANDLE) -> Console {
    let mut value = 0usize;
    // SAFETY: `value` is writable for the size given; the handle is open.
    let status = unsafe {
        NtQueryInformationProcess(
            process,
            PROCESS_CONSOLE_HOST_PROCESS,
            (&raw mut value).cast(),
            size_of::<usize>() as u32,
            std::ptr::null_mut(),
        )
    };
    if status != 0 {
        return Console::Unknown;
    }
    match value & !3 {
        host if value & 1 != 0 && host != 0 => Console::Host(host as u32),
        _ => Console::None,
    }
}

/// Whether the process `id` is attached to the console of the process `leader`: a leader on a
/// pseudoconsole takes every process attached to it down with it, since closing the pseudoconsole
/// ends them all, whatever keeps them. `None` when either cannot be opened, is no longer the
/// process named, or the system does not say which console it is on.
pub(crate) fn shares_console(leader: &ProcessIdentity, id: &ProcessIdentity) -> Option<bool> {
    let console = |id: &ProcessIdentity| {
        let process = Process::open(id.pid.0, PROCESS_QUERY_LIMITED_INFORMATION)?;
        (start_of_handle(process.raw()) == Some(id.start_micros)).then(|| console_of(process.raw()))
    };
    match (console(leader)?, console(id)?) {
        (Console::Unknown, _) | (_, Console::Unknown) => None,
        (Console::Host(leader), Console::Host(host)) => Some(leader == host),
        _ => Some(false),
    }
}

/// Whether `pid` is a console host: the process Windows starts to give a process started without
/// a window its hidden console. It is in its process's job without being anything the process
/// runs.
pub(crate) fn is_console_host(pid: ProcessId) -> bool {
    Process::open(pid.0, PROCESS_QUERY_LIMITED_INFORMATION)
        .is_some_and(|process| runs_console_host(process.raw()))
}

/// Whether the process `process` is a handle on is a console host ([`is_console_host`]).
fn runs_console_host(process: HANDLE) -> bool {
    let mut path = vec![0u16; 1024];
    let mut len = path.len() as u32;
    // SAFETY: `path` is writable for `len` characters.
    if unsafe { QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut len) } == 0 {
        return false;
    }
    let path = String::from_utf16_lossy(&path[..len as usize]);
    let name = path.rsplit('\\').next().unwrap_or(&path);
    name.eq_ignore_ascii_case("conhost.exe") || name.eq_ignore_ascii_case("OpenConsole.exe")
}

/// Calls `on_gone` once the process `pid` has exited, waiting on a handle opened now, which keeps
/// the pid from passing to another process meanwhile. A process that cannot be opened because it
/// is already gone calls it at once; one that cannot be opened otherwise is never watched.
pub(crate) fn watch_parent(pid: u32, on_gone: impl FnOnce() + Send + 'static) {
    let Some(process) = Process::open(pid, PROCESS_SYNCHRONIZE) else {
        let err = last_error();
        if err.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
            on_gone();
        } else {
            tracing::warn!(%err, pid, "opening the parent process failed; its exit is not watched");
        }
        return;
    };
    let spawned = std::thread::Builder::new()
        .name("parent-watch".to_string())
        .spawn(move || {
            // SAFETY: the handle is open for the wait, which ends only once the process exits.
            unsafe { WaitForSingleObject(process.raw(), INFINITE) };
            on_gone();
        });
    if let Err(err) = spawned {
        tracing::warn!(%err, "starting the parent watch failed; the parent's exit is not watched");
    }
}

/// Resolves when the host asks the daemon to end: Ctrl-C or Ctrl-Break on its console, or its
/// console closing, after which the system ends the daemon a few seconds later whatever it is
/// doing. The logoff and shutdown notices are waited on as well, but Windows does not send them
/// to a process that has loaded user32.dll or gdi32.dll, as the daemon has (Microsoft Learn,
/// `SetConsoleCtrlHandler`), so at a sign-out or shutdown the system ends the daemon without
/// this resolving. The handlers are installed by this call, not by the first poll of what it
/// returns, so a request made after it returns is not lost to the default action.
pub(crate) fn shutdown_requested() -> impl std::future::Future<Output = ()> {
    use tokio::signal::windows;

    // A request whose handler cannot be installed never arrives, so the others still end the
    // daemon.
    macro_rules! arrival {
        ($install:expr, $name:literal) => {{
            let installed = match $install {
                Ok(stream) => Some(stream),
                Err(err) => {
                    tracing::warn!(%err, "installing the {} handler failed", $name);
                    None
                }
            };
            async move {
                match installed {
                    Some(mut stream) => {
                        stream.recv().await;
                    }
                    None => std::future::pending::<()>().await,
                }
            }
        }};
    }
    let ctrl_c = arrival!(windows::ctrl_c(), "Ctrl-C");
    let ctrl_break = arrival!(windows::ctrl_break(), "Ctrl-Break");
    let ctrl_close = arrival!(windows::ctrl_close(), "console close");
    let ctrl_logoff = arrival!(windows::ctrl_logoff(), "logoff");
    let ctrl_shutdown = arrival!(windows::ctrl_shutdown(), "shutdown");
    async move {
        tokio::select! {
            _ = ctrl_c => {}
            _ = ctrl_break => {}
            _ = ctrl_close => {}
            _ = ctrl_logoff => {}
            _ = ctrl_shutdown => {}
        }
    }
}

/// The planned-restart requests the host delivers to the daemon: none on Windows (see
/// [`request_restart`]).
pub(crate) struct RestartRequests;

impl RestartRequests {
    /// Whether the handler is installed: there is none to install, and no request to end the
    /// daemon either.
    pub(crate) fn installed(&self) -> bool {
        true
    }

    /// Never resolves.
    pub(crate) async fn next(&mut self) {
        std::future::pending::<()>().await;
    }
}

/// What the daemon waits on for a planned-restart request, which never comes on Windows.
pub(crate) fn restart_requested() -> RestartRequests {
    RestartRequests
}
