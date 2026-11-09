//! Linux cgroup v2 groups for a launch's processes. A group is a directory under a [`Subtree`],
//! which by default is the cgroup the calling process is in, so it is made wherever that process
//! may make one: the subtree a service definition delegates to it (`Delegate=yes`), or a
//! container's writable hierarchy. Where it may not (no cgroup v2 hierarchy, a read-only one, a
//! cgroup owned by someone else), there is no group, which is said once, and the launch is told by
//! its tag alone.
//!
//! What a process starts is born in its parent's group, wherever it goes after that (another
//! session, a double fork), and leaves only when it is moved: so the group lists all of it, and
//! ending the group kills all of it. Nothing here moves a process out unless asked, and a group is
//! never ended with a process the caller asked to spare still in it.
//!
//! No controller is enabled anywhere: membership, `cgroup.kill` and removal need none, and with
//! none a cgroup may hold processes and groups side by side.

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use super::{ProcessId, ProcessIdentity};
use crate::platform::proc_table;

/// What every group's name starts with, before the instance and the launch it is for.
const PREFIX: &str = "octoboard-";

/// How deep below a group the members are looked for: the groups a daemon started inside a
/// launch makes in its own cgroup, which is that launch's group.
const MAX_DEPTH: usize = 8;

/// How many times the processes started before a child was moved are looked for again, since one
/// not moved yet may fork in the meantime.
const ADOPT_PASSES: usize = 3;

/// The most descendants of a child moved with it, so a fork bomb cannot hold the start up.
const MAX_ADOPTED: usize = 512;

/// How long an ended group's processes have to go before it is left in place.
#[cfg(not(test))]
const END_GRACE: Duration = Duration::from_secs(1);

/// [`END_GRACE`] under test, long enough for a kill to land while the suite loads the machine.
#[cfg(test)]
const END_GRACE: Duration = Duration::from_secs(10);

/// The cgroup v2 hierarchy as the calling process sees it.
#[derive(Debug)]
struct Hierarchy {
    /// The path, in the hierarchy, of the directory it is mounted from: `/` unless only part of
    /// it is mounted.
    root: PathBuf,
    /// Where it is mounted.
    mount: PathBuf,
    /// The directory of the calling process's own cgroup.
    own: PathBuf,
}

impl Hierarchy {
    /// The directory of the cgroup a process is in by its `/proc/<pid>/cgroup` content, as this
    /// mount shows it, or `None` for one outside it.
    fn dir_of(&self, cgroup_file: &str) -> Option<PathBuf> {
        let path = Path::new(unified_path(cgroup_file)?);
        let inside = path.strip_prefix(&self.root).ok()?;
        if inside
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
        {
            return None;
        }
        Some(self.mount.join(inside))
    }
}

/// The hierarchy, read once, or `None` where there is none to be had.
fn hierarchy() -> Option<&'static Hierarchy> {
    static HIERARCHY: OnceLock<Option<Hierarchy>> = OnceLock::new();
    HIERARCHY
        .get_or_init(|| {
            let mountinfo = fs::read_to_string("/proc/self/mountinfo").ok()?;
            let own = fs::read_to_string("/proc/self/cgroup").ok()?;
            let (root, mount) = cgroup2_mount(&mountinfo)?;
            let mut hierarchy = Hierarchy {
                root,
                mount,
                own: PathBuf::new(),
            };
            hierarchy.own = hierarchy.dir_of(&own)?;
            Some(hierarchy)
        })
        .as_ref()
}

/// The path of the cgroup v2 line of a `/proc/<pid>/cgroup` content (`0::<path>`).
fn unified_path(cgroup_file: &str) -> Option<&str> {
    cgroup_file
        .lines()
        .find_map(|line| line.strip_prefix("0::"))
        .filter(|path| path.starts_with('/'))
}

/// The root and the mount point of the first cgroup v2 mount in a `/proc/self/mountinfo`
/// content, its escaped characters (`\040` for a space) spelled out.
fn cgroup2_mount(mountinfo: &str) -> Option<(PathBuf, PathBuf)> {
    mountinfo.lines().find_map(|line| {
        let (fields, after) = line.split_once(" - ")?;
        if after.split(' ').next()? != "cgroup2" {
            return None;
        }
        let mut fields = fields.split(' ');
        let root = fields.nth(3)?;
        let mount = fields.next()?;
        Some((unescape(root).into(), unescape(mount).into()))
    })
}

/// A mountinfo field with its octal escapes (`\040`, `\011`, `\012`, `\134`) spelled out.
fn unescape(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let escape = bytes.get(at + 1..at + 4).filter(|_| bytes[at] == b'\\');
        match escape
            .and_then(|digits| u8::from_str_radix(std::str::from_utf8(digits).ok()?, 8).ok())
        {
            Some(byte) => {
                out.push(byte);
                at += 4;
            }
            None => {
                out.push(bytes[at]);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Whether `part` holds only what a cgroup's name had better be made of: ASCII letters, digits,
/// `-` and `_`.
fn plain(part: &str) -> bool {
    !part.is_empty()
        && part.len() <= 100
        && part
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// What the names of the groups of the daemon instance `instance` start with, before the launch,
/// or `None` when the instance holds a character a cgroup's name had better not.
fn prefix(instance: &str) -> Option<String> {
    plain(instance).then(|| format!("{PREFIX}{instance}-"))
}

/// The name of the group of the launch `launch` of the daemon instance `instance`, or `None` when
/// either holds a character a cgroup's name had better not.
fn group_name(instance: &str, launch: &str) -> Option<String> {
    plain(launch).then(|| format!("{}{launch}", prefix(instance)?))
}

/// Whether making a group failed because this process may not make one here, as opposed to
/// something that should not happen: a cgroup owned by someone else, or a read-only hierarchy.
fn no_writable_subtree(err: &io::Error) -> bool {
    matches!(
        err.raw_os_error(),
        Some(libc::EACCES | libc::EPERM | libc::EROFS)
    )
}

/// Says once per run that launches get no group, and why.
fn no_group(err: &dyn std::fmt::Display) {
    static SAID: AtomicBool = AtomicBool::new(false);
    if !SAID.swap(true, Ordering::Relaxed) {
        tracing::info!(
            %err,
            "no writable cgroup v2 subtree; agent launches are followed by the process ledger alone"
        );
    }
}

/// The cgroup below which launches' groups are made: a directory that may hold groups of its own,
/// and where what leaves a group without a group to go to is put back. The calling process's own
/// cgroup by default ([`Self::below_own`]); a process that is given a delegated base of its own
/// supplies that instead.
#[derive(Debug, Clone)]
pub(super) struct Subtree {
    dir: PathBuf,
}

impl Subtree {
    /// The calling process's own cgroup, or `None` where the process is in no cgroup v2
    /// hierarchy, which is said once.
    pub(super) fn below_own() -> Option<Self> {
        let Some(hierarchy) = hierarchy() else {
            no_group(&"the process is in no cgroup v2 hierarchy");
            return None;
        };
        Some(Self {
            dir: hierarchy.own.clone(),
        })
    }
}

/// The group of one launch: a directory in the cgroup v2 hierarchy, below the [`Subtree`] it was
/// made in. Removed when dropped, once nothing is in it, unless it was joined ([`Self::join`]).
#[derive(Debug)]
pub(super) struct Group {
    dir: PathBuf,
    /// Removed when dropped: the group was made or opened again by the process that ends it.
    owned: bool,
}

impl Group {
    /// The group of the launch `launch` of the daemon instance `instance`, made now below `base`,
    /// or `None` where none can be made. That this process may not make one there is said once;
    /// any other failure is said each time.
    pub(super) fn create(base: &Subtree, instance: &str, launch: &str) -> Option<Self> {
        let dir = base.dir.join(group_name(instance, launch)?);
        match fs::create_dir(&dir) {
            Ok(()) => Some(Self { dir, owned: true }),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                Some(Self { dir, owned: true })
            }
            Err(err) if no_writable_subtree(&err) => {
                no_group(&err);
                None
            }
            Err(err) => {
                tracing::warn!(
                    %err,
                    group = %dir.display(),
                    "making a launch's cgroup v2 group failed; the launch is followed by the process ledger alone"
                );
                None
            }
        }
    }

    /// The group of a launch of an earlier run of the daemon, while it has not been removed.
    pub(super) fn open(base: &Subtree, instance: &str, launch: &str) -> Option<Self> {
        let dir = base.dir.join(group_name(instance, launch)?);
        dir.is_dir().then_some(Self { dir, owned: true })
    }

    /// The group another process made for the launch `launch` of the daemon instance `instance`,
    /// below `base`, for the process that starts the launch's program in it, while it has not been
    /// removed. Never removed when dropped: its end stays its maker's.
    pub(super) fn join(base: &Subtree, instance: &str, launch: &str) -> Option<Self> {
        let dir = base.dir.join(group_name(instance, launch)?);
        dir.is_dir().then_some(Self { dir, owned: false })
    }

    /// The launches of the daemon instance `instance` whose groups are still there, below `base`.
    pub(super) fn leftovers(base: &Subtree, instance: &str) -> Vec<String> {
        let Some(prefix) = prefix(instance) else {
            return Vec::new();
        };
        let Ok(entries) = fs::read_dir(&base.dir) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                let launch = name.strip_prefix(&prefix)?;
                plain(launch).then(|| launch.to_string())
            })
            .collect()
    }

    /// The processes in the group and in the groups below it, or `None` when it cannot be read,
    /// as once it is removed.
    pub(super) fn members(&self) -> Option<Vec<ProcessId>> {
        let mut members = Vec::new();
        collect_members(&self.dir, 0, &mut members).then_some(members)
    }

    /// Moves the child `pid`, just started and not reaped, into the group, with what it has
    /// started already: a child started without a hook that runs before its program may have
    /// forked before it was moved. Says why when it could not move the child itself; a
    /// descendant that could not be moved is left where it is, to the ledger. A descendant is
    /// moved only while its parent is still the one it was listed under, which a pid that passed
    /// to another process since the listing is not, short of a window of microseconds between
    /// that check and the move.
    pub(super) fn adopt_child(&self, pid: ProcessId) -> io::Result<()> {
        self.adopt(pid)?;
        let mut moved: HashSet<i32> = HashSet::from([pid.0 as i32]);
        for _ in 0..ADOPT_PASSES {
            let mut children = proc_table::children_lister(pid.0 as i32);
            let mut pending: Vec<(i32, i32)> = children(pid.0 as i32)
                .unwrap_or_default()
                .into_iter()
                .map(|child| (child, pid.0 as i32))
                .collect();
            let mut seen = 0;
            let before = moved.len();
            while let Some((descendant, parent)) = pending.pop() {
                if seen == MAX_ADOPTED {
                    break;
                }
                seen += 1;
                if moved.insert(descendant) {
                    if proc_table::parent_of(descendant) != Some(parent) {
                        continue;
                    }
                    if let Err(err) = self.adopt(ProcessId(descendant as u32)) {
                        tracing::debug!(pid = descendant, %err, "moving a process into its launch's group failed");
                    }
                }
                pending.extend(
                    children(descendant)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|child| (child, descendant)),
                );
            }
            if moved.len() == before {
                break;
            }
        }
        Ok(())
    }

    /// Moves the processes `ids` names that are still running into the group, from wherever they
    /// are.
    pub(super) fn adopt_running(&self, ids: &[ProcessIdentity]) {
        for id in ids.iter().filter(|id| id.is_running()) {
            if let Err(err) = self.adopt(id.pid) {
                tracing::debug!(pid = id.pid.0, %err, "moving a process into a launch's group failed");
            }
        }
    }

    /// Moves those of the processes `ids` names that are in the group, or below it, into `into`,
    /// or with `None` into the cgroup the group is in. Answers whether none of them is left in it.
    pub(super) fn release(&self, ids: &[ProcessIdentity], into: Option<&Group>) -> bool {
        let Some(hierarchy) = hierarchy() else {
            return true;
        };
        let Some(target) = into
            .map(|group| group.dir.as_path())
            .or_else(|| self.dir.parent())
        else {
            return true;
        };
        let mut released = true;
        for id in ids {
            if !self.holds(hierarchy, id.pid) || !id.is_running() {
                continue;
            }
            if let Err(err) = write_pid(target, id.pid) {
                // Gone in the meantime, it is left nowhere.
                if self.holds(hierarchy, id.pid) {
                    tracing::warn!(pid = id.pid.0, %err, "moving a process out of its launch's group failed");
                    released = false;
                }
            }
        }
        released
    }

    /// Kills everything in the group and removes it, other than the processes `spared` names,
    /// which are moved first, each into the group it names or with `None` into the cgroup the group
    /// is in, with the children they started that are still in the group. When one
    /// of them cannot be moved, the group is neither killed nor removed. Answers whether the group
    /// is gone. **Blocks** until its processes have gone, for at most [`END_GRACE`].
    pub(super) fn end(&self, spared: &[(ProcessIdentity, Option<&Group>)]) -> bool {
        if self.members().is_none() {
            // Removed already, or never readable: nothing of it is left to kill.
            return !self.dir.exists();
        }
        if !self.spare(spared) {
            tracing::warn!(
                group = %self.dir.display(),
                "a process to be spared could not be moved out of its launch's group, so the group is not ended"
            );
            return false;
        }
        self.kill();
        let deadline = Instant::now() + END_GRACE;
        while self.members().is_some_and(|members| !members.is_empty()) {
            if Instant::now() >= deadline {
                tracing::warn!(
                    group = %self.dir.display(),
                    "processes were still in a launch's group after it was killed; it is left in place"
                );
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        remove_tree(&self.dir);
        !self.dir.exists()
    }

    /// Moves the processes of `spared` out of the group, and then what they started that is
    /// still in it, as the group's members show it each round. Answers whether none is left in it.
    /// A child of a spared process that has already left it for another parent (a double fork) is
    /// not found as its child, so it is not spared.
    fn spare(&self, spared: &[(ProcessIdentity, Option<&Group>)]) -> bool {
        let mut next: Vec<(ProcessIdentity, Option<&Group>)> = spared.to_vec();
        loop {
            let Some(members) = self.members() else {
                // Removed meanwhile: nothing of it is left.
                return !self.dir.exists();
            };
            let members: HashSet<ProcessId> = members.into_iter().collect();
            next.retain(|(id, _)| members.contains(&id.pid) && id.is_running());
            if next.is_empty() {
                return true;
            }
            for (id, into) in &next {
                if !self.release(std::slice::from_ref(id), *into) {
                    return false;
                }
            }
            // A child a spared process forked before it was moved is still in the group; it is
            // the spared process's, and goes where it went.
            let Some(table) = proc_table::list() else {
                return false;
            };
            next = table
                .into_iter()
                .filter(|info| members.contains(&ProcessId(info.pid as u32)))
                .filter_map(|info| {
                    let (_, into) = next
                        .iter()
                        .find(|(parent, _)| parent.pid.0 as i32 == info.ppid)?;
                    Some((
                        ProcessIdentity {
                            pid: ProcessId(info.pid as u32),
                            start_micros: info.start,
                        },
                        *into,
                    ))
                })
                .collect();
        }
    }

    /// Kills every process in the group and below it: at once through `cgroup.kill` where the
    /// kernel has it (5.14 on), else one by one until none is listed.
    fn kill(&self) {
        match OpenOptions::new()
            .write(true)
            .open(self.dir.join("cgroup.kill"))
            .and_then(|mut file| file.write_all(b"1"))
        {
            Ok(()) => return,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => {
                tracing::debug!(%err, "writing to a group's cgroup.kill failed; killing its processes one by one");
            }
        }
        let Some(hierarchy) = hierarchy() else {
            return;
        };
        let deadline = Instant::now() + END_GRACE;
        while let Some(members) = self.members().filter(|members| !members.is_empty()) {
            for pid in members {
                // Read again just before the signal, so that a member that exited since the
                // listing, whose pid another process outside the group took, is not killed. A
                // process could still exit and its pid be reused between this read and the
                // signal, a window of microseconds that the kernels with `cgroup.kill` do not
                // have.
                if !self.holds(hierarchy, pid) {
                    continue;
                }
                // SAFETY: a plain signal to a pid that was in the group a moment ago; one that
                // exited since is the `ESRCH` that is ignored.
                unsafe { libc::kill(pid.0 as libc::pid_t, libc::SIGKILL) };
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Whether the process `pid` is in the group or below it.
    fn holds(&self, hierarchy: &Hierarchy, pid: ProcessId) -> bool {
        fs::read_to_string(format!("/proc/{}/cgroup", pid.0))
            .ok()
            .and_then(|content| hierarchy.dir_of(&content))
            .is_some_and(|dir| dir.starts_with(&self.dir))
    }

    /// Moves the process `pid` into the group.
    fn adopt(&self, pid: ProcessId) -> io::Result<()> {
        write_pid(&self.dir, pid)
    }
}

impl Drop for Group {
    fn drop(&mut self) {
        if self.owned {
            remove_tree(&self.dir);
        }
    }
}

/// Moves the process `pid` into the cgroup whose directory is `dir`.
fn write_pid(dir: &Path, pid: ProcessId) -> io::Result<()> {
    OpenOptions::new()
        .write(true)
        .open(dir.join("cgroup.procs"))?
        .write_all(pid.0.to_string().as_bytes())
}

/// Adds the pids `cgroup.procs` lists in `dir` and the groups below it to `members`, and says
/// whether `dir` itself could be read.
fn collect_members(dir: &Path, depth: usize, members: &mut Vec<ProcessId>) -> bool {
    let Ok(procs) = fs::read_to_string(dir.join("cgroup.procs")) else {
        return false;
    };
    members.extend(
        procs
            .lines()
            .filter_map(|line| line.trim().parse::<u32>().ok())
            .map(ProcessId),
    );
    if depth < MAX_DEPTH {
        for below in subgroups(dir) {
            collect_members(&below, depth + 1, members);
        }
    }
    true
}

/// The groups directly below the cgroup whose directory is `dir`.
fn subgroups(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                .map(|entry| entry.path())
                .collect()
        })
        .unwrap_or_default()
}

/// Removes the group whose directory is `dir` and the groups below it, deepest first, those that
/// have nothing in them; one that still holds a process stays, and so does what is above it.
fn remove_tree(dir: &Path) {
    fn remove(dir: &Path, depth: usize) {
        if depth < MAX_DEPTH {
            for below in subgroups(dir) {
                remove(&below, depth + 1);
            }
        }
        // A cgroup's directory is removed with `rmdir` alone; its interface files go with it.
        let _ = fs::remove_dir(dir);
    }
    remove(dir, 0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_unified_line_of_a_cgroup_file_names_the_path() {
        assert_eq!(
            unified_path("12:pids:/user.slice\n0::/user.slice/octoboard.service\n"),
            Some("/user.slice/octoboard.service")
        );
        assert_eq!(unified_path("0::/\n"), Some("/"));
        assert_eq!(unified_path("12:pids:/user.slice\n"), None);
        assert_eq!(unified_path("0::\n"), None);
    }

    #[test]
    fn the_cgroup2_mount_is_found_and_unescaped() {
        let mountinfo = "\
22 1 0:21 / /proc rw,nosuid shared:5 - proc proc rw
30 22 0:26 / /sys/fs/cgroup rw,nosuid shared:9 - cgroup2 cgroup2 rw,nsdelegate
31 22 0:27 /a\\040b /mnt/my\\040cg rw - cgroup2 cgroup2 rw
";
        assert_eq!(
            cgroup2_mount(mountinfo),
            Some(("/".into(), "/sys/fs/cgroup".into()))
        );
        assert_eq!(
            cgroup2_mount(&mountinfo.lines().skip(2).collect::<Vec<_>>().join("\n")),
            Some(("/a b".into(), "/mnt/my cg".into()))
        );
        assert_eq!(cgroup2_mount(mountinfo.lines().next().unwrap()), None);
    }

    #[test]
    fn a_cgroup_path_is_placed_under_the_mount_only_inside_its_root() {
        let hierarchy = Hierarchy {
            root: "/user.slice".into(),
            mount: "/sys/fs/cgroup".into(),
            own: PathBuf::new(),
        };
        assert_eq!(
            hierarchy.dir_of("0::/user.slice/octoboard.service\n"),
            Some("/sys/fs/cgroup/octoboard.service".into())
        );
        assert_eq!(
            hierarchy.dir_of("0::/user.slice\n"),
            Some("/sys/fs/cgroup".into())
        );
        assert_eq!(hierarchy.dir_of("0::/system.slice/x\n"), None);
        assert_eq!(hierarchy.dir_of("0::/user.slice/../x\n"), None);
    }

    #[test]
    fn a_group_is_named_only_from_plain_parts() {
        assert_eq!(
            group_name("inst-1", "launch_2").as_deref(),
            Some("octoboard-inst-1-launch_2")
        );
        for (instance, launch) in [
            ("", "a"),
            ("a", ""),
            ("a", "../b"),
            ("a/b", "c"),
            ("a", "b c"),
        ] {
            assert_eq!(
                group_name(instance, launch),
                None,
                "{instance:?} {launch:?}"
            );
        }
    }
}
