//! The table reader's process groups, over real processes.

use super::*;
use crate::test_support::{Children, PATIENCE};

/// What the table lists for `pid`, waited for up to [`PATIENCE`].
fn listed(pid: i32) -> ProcInfo {
    let deadline = std::time::Instant::now() + PATIENCE;
    loop {
        if let Some(info) = list().and_then(|all| all.into_iter().find(|info| info.pid == pid)) {
            return info;
        }
        assert!(std::time::Instant::now() < deadline, "{pid} never listed");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// The group a process is listed in is the group it is in: its parent's, or the one it made
/// itself, named by the pid of its leader.
#[cfg(unix)]
#[test]
fn a_process_is_listed_with_its_process_group() {
    use std::os::unix::process::CommandExt;

    // SAFETY: `getpgrp` only reads this process's group.
    let own_group = unsafe { libc::getpgrp() };
    let mut inherits = std::process::Command::new("/bin/sleep");
    inherits.arg("300");
    let mut leads = std::process::Command::new("/bin/sleep");
    leads.arg("300").process_group(0);
    let inheriting = inherits.spawn().expect("sleep starts");
    let leading = leads.spawn().expect("sleep starts");
    let (inheriting_pid, leading_pid) = (inheriting.id() as i32, leading.id() as i32);
    let _children = Children(vec![inheriting, leading]);

    assert_eq!(listed(std::process::id() as i32).pgid, Some(own_group));
    assert_eq!(listed(inheriting_pid).pgid, Some(own_group));
    assert_eq!(listed(leading_pid).pgid, Some(leading_pid));
}

/// Windows has no process groups to list.
#[cfg(windows)]
#[test]
fn no_process_is_listed_with_a_process_group_on_windows() {
    let own = listed(std::process::id() as i32);
    assert_eq!(own.pgid, None);
}

/// A process's parent is the process that started it, read without starting anything: this
/// process is the parent of a child it starts, and has a parent itself. The child is a program
/// every host has, so each host's reader is held to it.
#[test]
fn the_parent_of_a_process_is_the_one_that_started_it() {
    let child = crate::test_support::programs::sleep(300)
        .command()
        .spawn()
        .expect("sleep starts");
    let pid = child.id() as i32;
    let _children = Children(vec![child]);
    // Listed first, so the child is there to be asked about.
    listed(pid);
    assert_eq!(parent_of(pid), Some(std::process::id() as i32));
    assert!(parent_of(std::process::id() as i32).is_some_and(|parent| parent > 0));
}

/// A pid no process has names no parent.
#[test]
fn no_process_has_no_parent() {
    assert_eq!(parent_of(i32::MAX), None);
}
