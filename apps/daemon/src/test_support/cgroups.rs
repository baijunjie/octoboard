//! Cgroups of a test's own, on a Linux host whose cgroup v2 hierarchy the test process may write
//! in: the cgroup the test process is in must be writable by the user running the suite, as in a
//! container made writable. A test that needs one returns early, saying so, where
//! [`ChildCgroup::new`] or [`grouped_launch`] finds none, as the launch-group tests do.
//! [`cgroup_of`] reads which cgroup a process is in.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use super::PATIENCE;
use crate::platform::proc::Containment;

/// The containment of an agent launch of the daemon instance `instance`, named for `name` and
/// the test process, with the group a Linux host lets the calling process make, or `None`, said
/// on the test's output, where it does not: a plain container's read-only `/sys/fs/cgroup`, a
/// cgroup owned by someone else. The launch's name comes with it.
pub(crate) fn grouped_launch(instance: &str, name: &str) -> Option<(String, Containment)> {
    let launch = format!("{name}-{}", std::process::id());
    let containment = Containment::for_launch(instance, &launch).expect("the containment");
    if containment.members().is_none() {
        eprintln!("skipped: no writable cgroup v2 subtree to make a launch's group in");
        return None;
    }
    Some((launch, containment))
}

/// What `/proc/<pid>/cgroup` holds for the process `pid`, which names the group it is in.
pub(crate) fn cgroup_of(pid: u32) -> String {
    fs::read_to_string(format!("/proc/{pid}/cgroup")).expect("the process's cgroup is read")
}

/// The directory of the cgroup the test process is in, under the usual mount, or `None` where
/// that is not where it is: the directory has to list the test process.
fn own_cgroup_dir() -> Option<PathBuf> {
    let content = fs::read_to_string("/proc/self/cgroup").ok()?;
    let path = content.lines().find_map(|line| line.strip_prefix("0::"))?;
    let dir = Path::new("/sys/fs/cgroup").join(path.trim_start_matches('/'));
    let procs = fs::read_to_string(dir.join("cgroup.procs")).ok()?;
    let own = std::process::id().to_string();
    procs.lines().any(|line| line.trim() == own).then_some(dir)
}

/// A cgroup made below the test process's own, removed with what is below it when dropped, once
/// nothing runs in it: the cgroup a stand-in for a process of another cgroup (a holder run as a
/// systemd unit) is moved into.
pub(crate) struct ChildCgroup {
    dir: PathBuf,
}

impl ChildCgroup {
    /// A new cgroup named for `label`, or `None`, said on the test's output, where this process
    /// may not make one.
    pub(crate) fn new(label: &str) -> Option<Self> {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let Some(own) = own_cgroup_dir() else {
            eprintln!("skipped: the test process's cgroup v2 directory is not found");
            return None;
        };
        let dir = own.join(format!(
            "testcg-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match fs::create_dir(&dir) {
            Ok(()) => Some(Self { dir }),
            Err(err) => {
                eprintln!("skipped: no writable cgroup v2 subtree to make a cgroup in ({err})");
                None
            }
        }
    }

    /// The cgroup's directory.
    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }

    /// Moves the process `pid` into the cgroup.
    pub(crate) fn adopt(&self, pid: u32) -> io::Result<()> {
        fs::OpenOptions::new()
            .write(true)
            .open(self.dir.join("cgroup.procs"))?
            .write_all(pid.to_string().as_bytes())
    }

    /// Where the group of the launch `launch` of the daemon instance `instance` is made below the
    /// cgroup.
    pub(crate) fn launch_dir(&self, instance: &str, launch: &str) -> PathBuf {
        self.dir.join(format!("octoboard-{instance}-{launch}"))
    }

    /// Makes the group of the launch `launch` of `instance` below the cgroup, as the process that
    /// runs in it would.
    pub(crate) fn make_launch_dir(&self, instance: &str, launch: &str) {
        fs::create_dir(self.launch_dir(instance, launch)).expect("the launch's group is made");
    }
}

/// Removes `dir` and the cgroups below it, deepest first.
fn remove_tree(dir: &Path) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                remove_tree(&entry.path());
            }
        }
    }
    fs::remove_dir(dir).ok();
}

impl Drop for ChildCgroup {
    fn drop(&mut self) {
        // A process that was killed just now may take a moment to leave it.
        let deadline = Instant::now() + PATIENCE;
        loop {
            remove_tree(&self.dir);
            if !self.dir.exists() || Instant::now() >= deadline {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}
