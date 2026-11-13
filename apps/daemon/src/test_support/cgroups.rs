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
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use super::PATIENCE;
use crate::platform::proc::Containment;

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

/// Where the cgroup at `path` in the cgroup v2 hierarchy is under the first cgroup v2 mount of a
/// `/proc/self/mountinfo` content, or `None` where that mount does not reach it, as the daemon's
/// own reading of the hierarchy (`platform::proc::cgroup`) places it.
fn cgroup_dir(mountinfo: &str, path: &str) -> Option<PathBuf> {
    let (root, mount) = mountinfo.lines().find_map(|line| {
        let (fields, after) = line.split_once(" - ")?;
        if after.split(' ').next()? != "cgroup2" {
            return None;
        }
        let mut fields = fields.split(' ');
        let root = fields.nth(3)?;
        let mount = fields.next()?;
        Some((
            PathBuf::from(unescape(root)),
            PathBuf::from(unescape(mount)),
        ))
    })?;
    let inside = Path::new(path).strip_prefix(&root).ok()?;
    if inside
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        return None;
    }
    Some(mount.join(inside))
}

/// A mountinfo field with its octal escapes (`\040` for a space) spelled out.
fn unescape(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let code = bytes.get(i + 1..i + 4).and_then(|digits| {
            if !digits.iter().all(|digit| (b'0'..=b'7').contains(digit)) {
                return None;
            }
            u8::from_str_radix(std::str::from_utf8(digits).ok()?, 8).ok()
        });
        match code {
            Some(code) if bytes[i] == b'\\' => {
                out.push(code);
                i += 4;
            }
            _ => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
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

    #[test]
    fn the_own_cgroup_is_found_under_the_cgroup2_mount_of_mountinfo() {
        let unified = "\
24 30 0:22 / /sys/fs/cgroup rw,nosuid - cgroup2 cgroup2 rw,nsdelegate";
        assert_eq!(
            cgroup_dir(unified, "/user.slice/test.scope"),
            Some(PathBuf::from("/sys/fs/cgroup/user.slice/test.scope"))
        );
        // A hybrid host mounts v1 controllers at the usual place and v2 below it.
        let hybrid = "\
25 30 0:23 / /sys/fs/cgroup ro,nosuid - tmpfs tmpfs ro,mode=755
26 25 0:24 / /sys/fs/cgroup/unified rw,nosuid - cgroup2 cgroup2 rw
27 25 0:25 / /sys/fs/cgroup/memory rw,nosuid - cgroup cgroup rw,memory";
        assert_eq!(
            cgroup_dir(hybrid, "/user.slice"),
            Some(PathBuf::from("/sys/fs/cgroup/unified/user.slice"))
        );
    }

    #[test]
    fn a_cgroup_outside_the_mounted_root_or_without_a_mount_has_no_dir() {
        let partial = "\
24 30 0:22 /kubepods/pod1 /sys/fs/cgroup rw - cgroup2 cgroup2 rw";
        assert_eq!(
            cgroup_dir(partial, "/kubepods/pod1/ctr"),
            Some(PathBuf::from("/sys/fs/cgroup/ctr"))
        );
        assert_eq!(cgroup_dir(partial, "/other"), None);
        assert_eq!(cgroup_dir(partial, "/kubepods/pod1/../x"), None);
        assert_eq!(
            cgroup_dir("25 30 0:23 / /sys/fs/cgroup rw - tmpfs tmpfs rw", "/"),
            None
        );
    }

    #[test]
    fn an_escaped_mount_point_is_spelled_out() {
        let spaced = "24 30 0:22 / /mnt/cgroup\\040v2 rw - cgroup2 cgroup2 rw";
        assert_eq!(
            cgroup_dir(spaced, "/a"),
            Some(PathBuf::from("/mnt/cgroup v2/a"))
        );
    }

    #[test]
    fn an_escaped_root_is_spelled_out_before_the_path_is_placed_in_it() {
        let spaced = "24 30 0:22 /pod\\040one /sys/fs/cgroup rw - cgroup2 cgroup2 rw";
        assert_eq!(
            cgroup_dir(spaced, "/pod one/ctr"),
            Some(PathBuf::from("/sys/fs/cgroup/ctr"))
        );
        assert_eq!(cgroup_dir(spaced, "/pod\\040one/ctr"), None);
    }

    #[test]
    fn the_first_cgroup2_mount_is_the_one_used() {
        let two = "\
24 30 0:22 / /sys/fs/cgroup rw - cgroup2 cgroup2 rw
25 30 0:23 / /mnt/other rw - cgroup2 cgroup2 rw";
        assert_eq!(
            cgroup_dir(two, "/a"),
            Some(PathBuf::from("/sys/fs/cgroup/a"))
        );
    }

    #[test]
    fn optional_mountinfo_fields_before_the_separator_do_not_shift_the_fields() {
        let shared = "24 30 0:22 / /sys/fs/cgroup rw shared:5 master:2 - cgroup2 cgroup2 rw";
        assert_eq!(
            cgroup_dir(shared, "/a"),
            Some(PathBuf::from("/sys/fs/cgroup/a"))
        );
    }

    #[test]
    fn the_root_of_the_hierarchy_is_the_mount_itself() {
        let unified = "24 30 0:22 / /sys/fs/cgroup rw - cgroup2 cgroup2 rw";
        assert_eq!(
            cgroup_dir(unified, "/"),
            Some(PathBuf::from("/sys/fs/cgroup"))
        );
    }

    #[test]
    fn a_mountinfo_without_a_cgroup2_mount_has_no_dir() {
        assert_eq!(cgroup_dir("", "/a"), None);
        let v1 = "\
25 30 0:23 / /sys/fs/cgroup ro - tmpfs tmpfs ro
27 25 0:25 / /sys/fs/cgroup/memory rw - cgroup cgroup rw,memory";
        assert_eq!(cgroup_dir(v1, "/a"), None);
        // A line without the separator is not a mount.
        assert_eq!(cgroup_dir("24 30 0:22 / /sys/fs/cgroup rw", "/a"), None);
    }

    #[test]
    fn a_cgroup_beside_the_mounted_root_is_not_inside_it() {
        // A prefix of the text is not a prefix of the path.
        let partial = "24 30 0:22 /pod1 /sys/fs/cgroup rw - cgroup2 cgroup2 rw";
        assert_eq!(cgroup_dir(partial, "/pod10/ctr"), None);
        assert_eq!(
            cgroup_dir(partial, "/pod1"),
            Some(PathBuf::from("/sys/fs/cgroup"))
        );
    }

    #[test]
    fn a_cgroup_path_that_is_not_made_of_plain_names_has_no_dir() {
        let unified = "24 30 0:22 / /sys/fs/cgroup rw - cgroup2 cgroup2 rw";
        assert_eq!(cgroup_dir(unified, "/a/../b"), None);
        assert_eq!(cgroup_dir(unified, "/a/../../etc"), None);
        assert_eq!(cgroup_dir(unified, "relative"), None);
    }

    #[test]
    fn unescape_spells_out_octal_escapes_and_keeps_everything_else() {
        assert_eq!(unescape("/plain/path"), "/plain/path");
        assert_eq!(unescape(""), "");
        assert_eq!(unescape("a\\040b"), "a b");
        assert_eq!(unescape("a\\011b\\012c"), "a\tb\nc");
        assert_eq!(unescape("a\\134b"), "a\\b");
        assert_eq!(unescape("\\040\\040"), "  ");
        // The escape is three digits, and what follows it is not part of it.
        assert_eq!(unescape("\\0401"), " 1");
    }

    #[test]
    fn unescape_keeps_what_is_not_a_valid_octal_escape() {
        // Fewer than three digits, a non-octal digit, a value above a byte, a bare or trailing backslash.
        assert_eq!(unescape("a\\04"), "a\\04");
        assert_eq!(unescape("a\\0"), "a\\0");
        assert_eq!(unescape("a\\089"), "a\\089");
        assert_eq!(unescape("a\\xyz"), "a\\xyz");
        assert_eq!(unescape("a\\400"), "a\\400");
        assert_eq!(unescape("a\\"), "a\\");
        assert_eq!(unescape("\\\\040"), "\\ ");
        // A sign is not a digit.
        assert_eq!(unescape("a\\+12"), "a\\+12");
        assert_eq!(unescape("a\\-12"), "a\\-12");
    }
}
