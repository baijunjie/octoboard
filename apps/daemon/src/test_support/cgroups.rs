//! Cgroups of a test's own, on a Linux host whose cgroup v2 hierarchy the test process may write
//! in: the cgroup the test process is in must be writable by the user running the suite, as in a
//! container made writable. A test that needs one returns early, saying so through [`skip`],
//! where [`ChildCgroup::new`] or [`grouped_launch`] finds none, as the launch-group tests do;
//! with [`REQUIRE_CGROUP`] set to `1` (the CI job that gives the suite a writable hierarchy) it
//! fails instead, so those tests cannot pass there without running. [`cgroup_of`] reads which
//! cgroup a process is in.

use std::ffi::OsStr;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use super::PATIENCE;
use crate::platform::proc::{cgroup_dir, Containment};

/// The environment variable that, set to `1`, turns a cgroup test's skip into a failure: where the
/// suite is run to exercise the groups, a hierarchy it cannot write in is a broken setup, not a
/// host without one.
pub(crate) const REQUIRE_CGROUP: &str = "OCTOBOARD_REQUIRE_CGROUP";

/// Whether a value of [`REQUIRE_CGROUP`] requires the cgroup tests to run.
fn required(value: Option<&OsStr>) -> bool {
    value.is_some_and(|value| value == "1")
}

/// Says on the test's output that the test is skipped for `reason`, for want of a cgroup it may
/// make, or fails it where [`REQUIRE_CGROUP`] is `1`.
pub(crate) fn skip(reason: &str) {
    if required(std::env::var_os(REQUIRE_CGROUP).as_deref()) {
        panic!("{REQUIRE_CGROUP}=1, and the test would be skipped: {reason}");
    }
    eprintln!("skipped: {reason}");
}

/// The containment of an agent launch of the daemon instance `instance`, named for `name` and
/// the test process, with the group a Linux host lets the calling process make, or `None`, said
/// on the test's output, where it does not: a plain container's read-only `/sys/fs/cgroup`, a
/// cgroup owned by someone else. The launch's name comes with it. With [`REQUIRE_CGROUP`] set,
/// no group fails the test.
pub(crate) fn grouped_launch(instance: &str, name: &str) -> Option<(String, Containment)> {
    let launch = format!("{name}-{}", std::process::id());
    let containment = Containment::for_launch(instance, &launch).expect("the containment");
    if containment.members().is_none() {
        skip("no writable cgroup v2 subtree to make a launch's group in");
        return None;
    }
    Some((launch, containment))
}

/// What `/proc/<pid>/cgroup` holds for the process `pid`, which names the group it is in.
pub(crate) fn cgroup_of(pid: u32) -> String {
    fs::read_to_string(format!("/proc/{pid}/cgroup")).expect("the process's cgroup is read")
}

/// The directory of the cgroup the test process is in, under the cgroup v2 mount
/// `/proc/self/mountinfo` names (`/sys/fs/cgroup`, or `/sys/fs/cgroup/unified` on a hybrid host),
/// or `None` where there is none: the directory has to list the test process.
fn own_cgroup_dir() -> Option<PathBuf> {
    let mountinfo = fs::read_to_string("/proc/self/mountinfo").ok()?;
    let content = fs::read_to_string("/proc/self/cgroup").ok()?;
    let path = content.lines().find_map(|line| line.strip_prefix("0::"))?;
    let dir = cgroup_dir(&mountinfo, path)?;
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
    /// may not make one; with [`REQUIRE_CGROUP`] set, none fails the test.
    pub(crate) fn new(label: &str) -> Option<Self> {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let Some(own) = own_cgroup_dir() else {
            skip("the test process's cgroup v2 directory is not found");
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
                skip(&format!(
                    "no writable cgroup v2 subtree to make a cgroup in ({err})"
                ));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_value_of_1_requires_the_cgroup_tests() {
        assert!(required(Some(OsStr::new("1"))));
        assert!(!required(None));
        assert!(!required(Some(OsStr::new(""))));
        assert!(!required(Some(OsStr::new("0"))));
        assert!(!required(Some(OsStr::new("true"))));
    }
}
