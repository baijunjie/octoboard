//! Starting a process on a PTY: the slave opened from the master, the program and
//! environment resolved, and the child made a session leader with the PTY as its controlling
//! terminal before its program runs. `portable-pty` opens the PTY only, because it offers no hook
//! before the program runs, which a launch's cgroup group needs.

use std::collections::BTreeMap;
use std::ffi::{CString, OsStr, OsString};
use std::io;
use std::os::fd::{FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::RawFd;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use portable_pty::Child;

use super::Command;
use crate::platform::descriptors;
use crate::platform::fs;
use crate::platform::proc::Containment;

/// Starts `command` with the slave of the PTY whose master is `master` as its standard streams
/// and controlling terminal, in a session of its own. With a `containment` that has a group, the
/// process enters the group before its program runs.
pub(super) fn spawn_on_slave(
    command: &Command,
    master: RawFd,
    containment: Option<&Containment>,
) -> Result<Box<dyn Child + Send + Sync>> {
    let slave = open_slave(master).context("opening the PTY's slave")?;
    let mut process = process_of(command, std::env::vars_os())?;
    process
        .stdin(slave.try_clone().context("duplicating the PTY's slave")?)
        .stdout(slave.try_clone().context("duplicating the PTY's slave")?)
        .stderr(slave);
    // Worked out before the fork, as the child may not look it up.
    let descriptor_bound = descriptors::bound().context("finding the descriptors to close")?;
    // SAFETY: the hook makes system calls and nothing else (no allocation, no lock), which is all
    // that is safe between a fork and an exec; what it uses was made before the fork.
    unsafe {
        process.pre_exec(move || {
            for signal in [
                libc::SIGCHLD,
                libc::SIGHUP,
                libc::SIGINT,
                libc::SIGQUIT,
                libc::SIGTERM,
                libc::SIGALRM,
            ] {
                libc::signal(signal, libc::SIG_DFL);
            }
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            // Without the controlling terminal the process would not get SIGWINCH when the
            // window is resized, among other things.
            if libc::ioctl(0, libc::TIOCSCTTY as _, 0) == -1 {
                return Err(io::Error::last_os_error());
            }
            // A descriptor the daemon holds without close-on-exec would reach the program, so
            // every one above the standard three is closed, std's own pipe for reporting a failed
            // exec included. A failure to exec is then not reported by the start. The lookup of
            // the program ahead of it catches most (a program that is missing or not executable),
            // but not all (a working directory entry that merely exists, a file the kernel cannot
            // execute): such a start returns `Ok` and the child exits.
            descriptors::close_from(3, descriptor_bound);
            Ok(())
        });
    }
    if let Some(containment) = containment {
        containment.enter_before_exec(&mut process);
    }
    let mut child = process.spawn()?;
    // The slave descriptors the child holds are not needed (the master is the way in), and a
    // process started with them would keep the PTY open.
    child.stdin.take();
    child.stdout.take();
    child.stderr.take();
    Ok(Box::new(child))
}

/// The slave end of the PTY whose master is `master`, opened without making it this process's
/// controlling terminal. On Linux it is asked of the master, which names no path and so holds
/// across devpts instances; a kernel that cannot (before 4.13) has it opened by its path.
fn open_slave(master: RawFd) -> io::Result<OwnedFd> {
    let flags = libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC;
    #[cfg(target_os = "linux")]
    {
        // SAFETY: the request returns a new descriptor or fails; no memory is involved.
        let slave = unsafe { libc::ioctl(master, libc::TIOCGPTPEER as _, flags) };
        if slave >= 0 {
            // SAFETY: the ioctl succeeded, so this is an open descriptor owned by nothing else.
            return Ok(unsafe { OwnedFd::from_raw_fd(slave) });
        }
    }
    let name = slave_name(master)?;
    // SAFETY: `name` is a C string.
    let slave = unsafe { libc::open(name.as_ptr(), flags) };
    if slave < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `open` succeeded, so this is an open descriptor owned by nothing else.
    Ok(unsafe { OwnedFd::from_raw_fd(slave) })
}

/// The device path of the slave side of the PTY whose master is `master`, which has to stay open
/// for as long as this call.
pub(super) fn slave_name(master: RawFd) -> io::Result<CString> {
    let mut name = [0 as libc::c_char; 256];
    #[cfg(target_os = "linux")]
    {
        // SAFETY: the call writes at most the buffer's size.
        let error = unsafe { libc::ptsname_r(master, name.as_mut_ptr(), name.len()) };
        if error != 0 {
            return Err(io::Error::from_raw_os_error(error));
        }
    }
    #[cfg(target_os = "macos")]
    {
        // SAFETY: the request fills in a buffer of 128 bytes, which `name` exceeds.
        if unsafe { libc::ioctl(master, libc::TIOCPTYGNAME as _, name.as_mut_ptr()) } == -1 {
            return Err(io::Error::last_os_error());
        }
    }
    // SAFETY: the system wrote a NUL-terminated name into the zeroed buffer.
    Ok(unsafe { std::ffi::CStr::from_ptr(name.as_ptr()) }.to_owned())
}

/// `command` as a process to start. The program is looked up against the working directory and
/// then the `PATH` the process will have. The working directory is the one asked for if it is a
/// directory and the home directory otherwise. The environment is `daemon_env`, unless the command
/// starts over its own, with the command's on top, except `SHELL`, which is the command's own if
/// it sets one and the user's login shell (`/bin/sh` where that is not executable) otherwise,
/// whatever `daemon_env` holds.
pub(super) fn process_of(
    command: &Command,
    daemon_env: impl IntoIterator<Item = (OsString, OsString)>,
) -> Result<std::process::Command> {
    let mut env: BTreeMap<OsString, OsString> = if command.inherits_env {
        daemon_env.into_iter().collect()
    } else {
        BTreeMap::new()
    };
    env.remove(OsStr::new("SHELL"));
    env.extend(command.env.iter().cloned());
    let home = env
        .get(OsStr::new("HOME"))
        .filter(|home| home.to_str().is_some())
        .cloned()
        .or_else(|| fs::account_home().map(PathBuf::into_os_string))
        .unwrap_or_else(|| OsString::from("/"));
    let dir = command
        .cwd
        .as_ref()
        .filter(|cwd| cwd.is_dir())
        .map_or_else(|| PathBuf::from(&home), |cwd| cwd.clone());
    let resolved = resolve_program(&command.program, &dir, env.get(OsStr::new("PATH")))?;
    env.entry(OsString::from("SHELL")).or_insert_with(|| {
        fs::account_shell()
            .filter(|shell| is_executable(shell))
            .map_or_else(|| OsString::from("/bin/sh"), PathBuf::into_os_string)
    });
    let mut process = std::process::Command::new(resolved);
    process
        .arg0(&command.program)
        .args(&command.args)
        .current_dir(dir)
        .env_clear()
        .envs(env);
    Ok(process)
}

/// The program `program` names: a relative name is the file of that name in `dir` if there is
/// one, else the first file of that name in a directory of `path` that is executable; an absolute
/// one must be executable.
fn resolve_program(program: &OsStr, dir: &Path, path: Option<&OsString>) -> Result<OsString> {
    let program_path = Path::new(program);
    if program_path.is_relative() {
        let in_dir = dir.join(program_path);
        if in_dir.exists() {
            return Ok(in_dir.into_os_string());
        }
        for directory in path.iter().flat_map(std::env::split_paths) {
            let candidate = directory.join(program_path);
            if is_executable(&candidate) {
                return Ok(candidate.into_os_string());
            }
        }
        bail!(
            "unable to start {} because it does not exist on the filesystem and was not found in PATH",
            program_path.display()
        );
    }
    if !is_executable(program_path) {
        bail!(
            "unable to start {} because it does not exist on the filesystem or is not executable",
            program_path.display()
        );
    }
    Ok(program.to_os_string())
}

fn is_executable(path: &Path) -> bool {
    CString::new(path.as_os_str().as_bytes())
        // SAFETY: a valid C string.
        .is_ok_and(|path| unsafe { libc::access(path.as_ptr(), libc::X_OK) } == 0)
}
