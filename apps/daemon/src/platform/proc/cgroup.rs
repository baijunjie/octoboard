//! Linux cgroup v2 groups for a launch's processes. A group is a directory under a [`Subtree`],
//! which is the cgroup the calling process is in or that of a holder that starts the launch, so
//! it is made wherever that process may make one: the subtree a service definition delegates to
//! it (`Delegate=yes`), or a container's writable hierarchy. Where it may not (no cgroup v2
//! hierarchy, a read-only one, a cgroup owned by someone else), there is no group, which is said
//! once, and the launch is told by its tag alone.
//!
//! What a process starts is born in its parent's group, wherever it goes after that (another
//! session, a double fork), and leaves only when it is moved: so the group lists all of it, and
//! ending the group kills all of it. Nothing here moves a process out unless asked, and a group is
//! never ended with a process the caller asked to spare still in it.
//!
//! No controller is enabled anywhere: membership, `cgroup.kill` and removal need none, and with
//! none a cgroup may hold processes and groups side by side.

use std::collections::HashSet;
use std::ffi::{CStr, CString};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
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

/// How long a dropped group is tried again for when it cannot be removed at once: a process that
/// was killed or exited a moment ago leaves it only once the kernel has done with it, and until
/// then the group is busy.
#[cfg(not(test))]
const REMOVE_GRACE: Duration = Duration::from_secs(5);

/// [`REMOVE_GRACE`] under test, long enough for a process to go while the suite loads the machine.
#[cfg(test)]
const REMOVE_GRACE: Duration = crate::test_support::PATIENCE;

/// How often a group that could not be removed is tried again.
const REMOVE_RETRY: Duration = Duration::from_millis(20);

/// The least an ended group's removal is tried for once its processes are gone, however much of
/// [`END_GRACE`] their going took.
const REMOVE_AFTER_END: Duration = Duration::from_millis(100);

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
pub(super) fn unified_path(cgroup_file: &str) -> Option<&str> {
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
    let prefix = prefix(instance)?;
    plain(launch).then(|| format!("{prefix}{launch}"))
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
/// cgroup ([`Self::below_own`]), or the cgroup of another process of the user's that starts
/// launches (a holder), found with [`Self::of_process`] and listed by [`Self::all`] once noted
/// ([`Self::note`]).
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

    /// The cgroup of the process `host` names, while it runs, or `None` where it is in no cgroup
    /// v2 hierarchy this process sees. [`Self::all`] lists it only once it is noted
    /// ([`Self::note`]).
    pub(super) fn of_process(host: &ProcessIdentity) -> Option<Self> {
        let hierarchy = hierarchy()?;
        if !host.is_running() {
            return None;
        }
        let content = fs::read_to_string(format!("/proc/{}/cgroup", host.pid.0)).ok()?;
        let dir = hierarchy.dir_of(&content)?;
        // Read again after the file, so that a pid that passed to another process meanwhile is
        // not taken for the host.
        if !host.is_running() {
            return None;
        }
        Some(Self { dir })
    }

    /// Has [`Self::all`] list the cgroup of the process `host` names, while it runs.
    pub(super) fn note(host: &ProcessIdentity) {
        if let Some(subtree) = Self::of_process(host) {
            subtree.remember();
        }
    }

    /// Has [`Self::all`] list this subtree, once.
    pub(super) fn remember(&self) {
        remember(&self.dir);
    }

    /// The calling process's own cgroup, then those of the hosts noted ([`Self::note`]) that are
    /// still there, each once.
    pub(super) fn all() -> Vec<Self> {
        let mut dirs: Vec<PathBuf> = Self::below_own().map(|own| own.dir).into_iter().collect();
        for dir in noted().lock().unwrap_or_else(|e| e.into_inner()).iter() {
            if !dirs.contains(dir) && dir.is_dir() {
                dirs.push(dir.clone());
            }
        }
        dirs.into_iter().map(|dir| Self { dir }).collect()
    }
}

/// The cgroups of the hosts noted ([`Subtree::note`]), in the order they were first noted.
fn noted() -> &'static Mutex<Vec<PathBuf>> {
    static NOTED: OnceLock<Mutex<Vec<PathBuf>>> = OnceLock::new();
    NOTED.get_or_init(Mutex::default)
}

/// Adds `dir` to the cgroups noted, once.
fn remember(dir: &Path) {
    let mut noted = noted().lock().unwrap_or_else(|e| e.into_inner());
    if !noted.iter().any(|known| known == dir) {
        noted.push(dir.to_path_buf());
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
    /// is gone. **Blocks** until its processes have gone, for at most [`END_GRACE`] plus
    /// [`REMOVE_AFTER_END`], the latter while the kernel lets go of the group.
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
        // Its processes are gone, but the kernel may still count one that exited a moment ago.
        remove_tree_by(&self.dir, deadline.max(Instant::now() + REMOVE_AFTER_END))
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

    /// The path of the group's `cgroup.procs` as a C string, made ahead of a fork so that the
    /// child can move itself with [`Self::enter_from_child`] without allocating; `None` for a
    /// path no C string can hold.
    pub(super) fn procs_path(&self) -> Option<CString> {
        CString::new(self.dir.join("cgroup.procs").as_os_str().as_bytes()).ok()
    }

    /// Moves the calling process into the group whose `cgroup.procs` is `procs` (see
    /// [`Self::procs_path`]), by writing `0`, which names the writer itself. Meant for the child
    /// of a fork before its program runs, so it does only what is safe there: raw system calls,
    /// no allocation, no lock. Failure is ignored: the parent moves the child after the start
    /// ([`Self::adopt_child`]) whatever happened here.
    pub(super) fn enter_from_child(procs: &CStr) {
        // SAFETY: `procs` is a valid C string; the descriptor opened is closed again below, and
        // the buffer written is one byte of a static.
        unsafe {
            let fd = libc::open(procs.as_ptr(), libc::O_WRONLY | libc::O_CLOEXEC);
            if fd >= 0 {
                libc::write(fd, b"0".as_ptr().cast(), 1);
                libc::close(fd);
            }
        }
    }
}

impl Drop for Group {
    /// Removes the group once nothing is in it. One still busy, as a group is for a moment after
    /// its last process went, is tried again on a thread of its own for up to [`REMOVE_GRACE`],
    /// so that the drop does not wait; one that still holds a process after that stays, for the
    /// next start to find among the leftovers.
    fn drop(&mut self) {
        if !self.owned {
            return;
        }
        remove_tree(&self.dir);
        if !self.dir.exists() {
            return;
        }
        let dir = std::mem::take(&mut self.dir);
        let retrying = std::thread::Builder::new()
            .name("cgroup-remove".into())
            .spawn(move || {
                if !remove_tree_by(&dir, Instant::now() + REMOVE_GRACE) {
                    tracing::debug!(group = %dir.display(), "a launch's group still held a process when it was dropped; it is left in place");
                }
            });
        if let Err(err) = retrying {
            tracing::debug!(%err, "trying a launch's group again on a thread failed");
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

/// [`remove_tree`], tried again until the group is gone or `deadline` passes. Answers whether it
/// is gone.
fn remove_tree_by(dir: &Path, deadline: Instant) -> bool {
    loop {
        remove_tree(dir);
        if !dir.exists() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(REMOVE_RETRY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::proc::Containment;
    use crate::test_support::cgroups::ChildCgroup;
    use crate::test_support::{programs, Children};

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

    /// The identity of the running process `pid`.
    fn identity_of(pid: u32) -> ProcessIdentity {
        ProcessIdentity {
            pid: ProcessId(pid),
            start_micros: proc_table::start_of(pid as i32).expect("the process runs"),
        }
    }

    /// A process that runs in a cgroup of its own, as a holder run as a systemd unit does.
    struct Host {
        child: std::process::Child,
        cgroup: ChildCgroup,
    }

    impl Host {
        /// Starts one, or `None` where no cgroup can be made (said on the output).
        fn start(label: &str) -> Option<Self> {
            let cgroup = ChildCgroup::new(label)?;
            let child = programs::sleep(60)
                .command()
                .spawn()
                .expect("the host starts");
            cgroup.adopt(child.id()).expect("the host is moved");
            Some(Self { child, cgroup })
        }

        fn identity(&self) -> ProcessIdentity {
            identity_of(self.child.id())
        }

        /// Ends the process and has it reaped, leaving its cgroup.
        fn end(&mut self) {
            self.child.kill().ok();
            self.child.wait().ok();
        }
    }

    impl Drop for Host {
        fn drop(&mut self) {
            self.end();
        }
    }

    fn launch_name(label: &str) -> String {
        format!("{label}-{}", std::process::id())
    }

    #[test]
    fn the_cgroup_of_a_process_is_the_one_it_runs_in() {
        let Some(own) = Subtree::below_own() else {
            return;
        };
        let found = Subtree::of_process(&identity_of(std::process::id())).expect("own cgroup");
        assert_eq!(found.dir, own.dir);

        let Some(host) = Host::start("of-process") else {
            return;
        };
        let found = Subtree::of_process(&host.identity()).expect("the host's cgroup");
        assert_eq!(found.dir, host.cgroup.dir());
    }

    #[test]
    fn the_cgroup_of_a_process_that_is_not_that_process_is_none_and_not_noted() {
        let Some(mut host) = Host::start("of-process-gone") else {
            return;
        };
        let real = host.identity();
        // The pid now belongs to a process that started at another time.
        let reused = ProcessIdentity {
            start_micros: real.start_micros + 1,
            ..real
        };
        assert!(Subtree::of_process(&reused).is_none());
        let listed = |dir: &Path| Subtree::all().iter().any(|base| base.dir == dir);
        assert!(!listed(host.cgroup.dir()), "a mismatch was noted");

        host.end();
        assert!(Subtree::of_process(&real).is_none(), "the host is gone");
        assert!(!listed(host.cgroup.dir()), "a host that is gone was noted");
    }

    #[test]
    fn all_lists_the_own_cgroup_first_and_each_noted_one_once() {
        let Some(own) = Subtree::below_own() else {
            return;
        };
        let count = |dir: &Path| Subtree::all().iter().filter(|base| base.dir == dir).count();
        assert_eq!(Subtree::all()[0].dir, own.dir);
        // Noting the own cgroup lists it once, not twice.
        Subtree::note(&identity_of(std::process::id()));
        assert_eq!(Subtree::all()[0].dir, own.dir);
        assert_eq!(count(&own.dir), 1);

        let Some(host) = Host::start("all") else {
            return;
        };
        assert_eq!(count(host.cgroup.dir()), 0, "not listed before it is noted");
        Subtree::note(&host.identity());
        Subtree::note(&host.identity());
        assert_eq!(count(host.cgroup.dir()), 1, "listed once");
        let all = Subtree::all();
        let at = |dir: &Path| all.iter().position(|base| base.dir == dir).unwrap();
        assert_eq!(at(&own.dir), 0);
        assert!(at(host.cgroup.dir()) > at(&own.dir));

        let dir = host.cgroup.dir().to_path_buf();
        drop(host);
        assert_eq!(count(&dir), 0, "a cgroup that is gone is not listed");
    }

    /// A held launch's containment placed with a host that runs in a cgroup of its own has its
    /// group made below that cgroup, not below the calling process's, for every clone of it.
    #[test]
    fn a_held_launch_is_placed_below_its_host_s_cgroup() {
        let Some(host) = Host::start("place") else {
            return;
        };
        let launch = launch_name("place");
        let held = Containment::for_held_launch("held", &launch).expect("the containment");
        let clone = held.clone();
        assert_eq!(held.members(), None);
        assert!(!held.releases_members());

        held.place_with_host(&host.identity());
        let group = host.cgroup.launch_dir("held", &launch);
        assert!(group.is_dir(), "no group below the host's cgroup");
        let own = Subtree::below_own().expect("a hierarchy");
        assert!(!own.dir.join(group.file_name().unwrap()).exists());
        for containment in [&held, &clone] {
            assert_eq!(containment.members(), Some(Vec::new()));
            assert!(containment.releases_members());
        }

        // What the host starts is in the group, and the group's end removes it.
        let child = programs::sleep(60).command().spawn().expect("a process");
        let id = child.id();
        let mut children = Children(vec![child]);
        clone.adopt(&[identity_of(id)]);
        assert_eq!(held.members(), Some(vec![ProcessId(id)]));
        assert!(held.end(&[]), "the group is gone");
        children.0[0]
            .wait()
            .expect("the process was killed with the group");
        assert!(!group.exists());
    }

    #[test]
    fn a_held_launch_stays_where_it_was_placed() {
        let (Some(first), Some(second)) = (Host::start("again-1"), Host::start("again-2")) else {
            return;
        };
        let launch = launch_name("again");
        let held = Containment::for_held_launch("held", &launch).expect("the containment");
        held.place_with_host(&first.identity());
        held.place_with_host(&second.identity());
        assert!(first.cgroup.launch_dir("held", &launch).is_dir());
        assert!(
            !second.cgroup.launch_dir("held", &launch).exists(),
            "the group was moved by a second placement"
        );
        assert_eq!(held.members(), Some(Vec::new()));
    }

    /// A containment made with its group already settled is not given another by a placement:
    /// one with none stays without, one with a group keeps that group.
    #[test]
    fn placing_never_adds_a_group_to_a_settled_containment() {
        let Some(host) = Host::start("settled") else {
            return;
        };
        let tag_only = launch_name("settled-tag");
        let create = Containment::create("held", &tag_only).expect("the containment");
        create.place_with_host(&host.identity());
        assert_eq!(create.members(), None);
        assert!(!create.releases_members());
        assert!(!host.cgroup.launch_dir("held", &tag_only).exists());

        let grouped = launch_name("settled-group");
        let for_launch = Containment::for_launch("held", &grouped).expect("the containment");
        let own = Subtree::below_own().expect("a hierarchy");
        let own_group = own.dir.join(format!("octoboard-held-{grouped}"));
        assert!(own_group.is_dir());
        for_launch.place_with_host(&host.identity());
        assert!(own_group.is_dir(), "the launch's own group was replaced");
        assert!(!host.cgroup.launch_dir("held", &grouped).exists());
        assert_eq!(for_launch.members(), Some(Vec::new()));

        let joined = launch_name("settled-join");
        let maker = Containment::for_launch("held", &joined).expect("the containment");
        let join = Containment::join_launch("held", &joined).expect("the containment");
        join.place_with_host(&host.identity());
        assert!(!host.cgroup.launch_dir("held", &joined).exists());
        drop(join);
        assert_eq!(maker.members(), Some(Vec::new()), "the maker's group stays");
    }

    /// A host that no longer runs (its pid is another process's by now) gets no group, and the
    /// containment is settled to that: a later placement does not give it one.
    #[test]
    fn a_held_launch_whose_host_is_not_running_settles_to_no_group() {
        let Some(mut gone) = Host::start("gone") else {
            return;
        };
        let live = Host::start("gone-live").expect("a second cgroup");
        let launch = launch_name("gone");
        let held = Containment::for_held_launch("held", &launch).expect("the containment");
        let real = gone.identity();
        let reused = ProcessIdentity {
            start_micros: real.start_micros + 1,
            ..real
        };
        held.place_with_host(&reused);
        assert_eq!(held.members(), None);
        assert!(!held.releases_members());
        assert!(!gone.cgroup.launch_dir("held", &launch).exists());

        held.place_with_host(&live.identity());
        assert_eq!(held.members(), None, "it was settled to no group");
        assert!(!live.cgroup.launch_dir("held", &launch).exists());

        let other = launch_name("gone-ended");
        let ended = Containment::for_held_launch("held", &other).expect("the containment");
        gone.end();
        ended.place_with_host(&real);
        assert_eq!(ended.members(), None);
        assert!(!gone.cgroup.launch_dir("held", &other).exists());
    }

    /// Where this process may not make a group below the host's cgroup, there is none: not an
    /// error, and not one made elsewhere.
    #[test]
    fn a_held_launch_whose_host_cgroup_refuses_a_group_settles_to_no_group() {
        use std::os::unix::fs::PermissionsExt;

        // Root makes a group wherever it likes.
        // SAFETY: takes nothing and cannot fail.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let Some(host) = Host::start("refuses") else {
            return;
        };
        let dir = host.cgroup.dir().to_path_buf();
        if fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).is_err() {
            eprintln!("skipped: the cgroup's mode cannot be changed");
            return;
        }
        let launch = launch_name("refuses");
        let held = Containment::for_held_launch("held", &launch).expect("the containment");
        held.place_with_host(&host.identity());
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).ok();
        assert_eq!(held.members(), None);
        assert!(!held.releases_members());
        assert!(!host.cgroup.launch_dir("held", &launch).exists());
        let own = Subtree::below_own().expect("a hierarchy");
        assert!(!own.dir.join(format!("octoboard-held-{launch}")).exists());
    }

    /// A group dropped while a process it held is still in it is removed once that process has
    /// gone, rather than left for the next start; while the process runs, it stays.
    #[test]
    fn a_group_dropped_while_a_process_is_in_it_is_removed_once_the_process_goes() {
        let Some(base) = Subtree::below_own() else {
            return;
        };
        let launch = launch_name("drop-busy");
        let Some(group) = Group::create(&base, "drop-inst", &launch) else {
            return;
        };
        let dir = group.dir.clone();
        let child = programs::sleep(60).command().spawn().expect("a process");
        let id = child.id();
        let mut children = Children(vec![child]);
        if let Err(err) = group.adopt(ProcessId(id)) {
            eprintln!("skipped: the process cannot be moved into the group ({err})");
            return;
        }
        drop(group);
        assert!(dir.is_dir(), "the group was removed with a process in it");

        children.0[0].kill().expect("the process is killed");
        children.0[0].wait().expect("the process is reaped");
        let deadline = Instant::now() + crate::test_support::PATIENCE;
        while dir.exists() {
            assert!(
                Instant::now() < deadline,
                "the group was left in place after its process went"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// A group with nothing in it is removed by its drop itself, with no retry left to wait for.
    #[test]
    fn an_empty_group_is_removed_by_its_drop() {
        let Some(base) = Subtree::below_own() else {
            return;
        };
        let Some(group) = Group::create(&base, "drop-empty", &launch_name("drop-empty")) else {
            return;
        };
        let dir = group.dir.clone();
        assert!(dir.is_dir());
        drop(group);
        assert!(!dir.exists(), "the empty group was left in place");
    }

    /// A group another process made is not removed by the drop of the one that joined it, which
    /// leaves its end to its maker.
    #[test]
    fn a_joined_group_is_never_removed_by_its_drop() {
        let Some(base) = Subtree::below_own() else {
            return;
        };
        let launch = launch_name("drop-joined");
        let Some(owner) = Group::create(&base, "drop-joined", &launch) else {
            return;
        };
        let dir = owner.dir.clone();
        let joined = Group::join(&base, "drop-joined", &launch).expect("the group is joined");
        drop(joined);
        assert!(dir.is_dir(), "the joiner's drop removed its maker's group");
        drop(owner);
        assert!(!dir.exists(), "the maker's drop left its group in place");
    }

    /// A group that still holds a process when the window its removal is tried for ends stays, for
    /// the next start to find among the leftovers; once the process has gone, a window that is
    /// open removes it.
    #[test]
    fn a_group_still_holding_a_process_when_its_removal_window_ends_stays() {
        let Some(base) = Subtree::below_own() else {
            return;
        };
        let Some(group) = Group::create(&base, "window", &launch_name("window")) else {
            return;
        };
        let dir = group.dir.clone();
        let child = programs::sleep(60).command().spawn().expect("a process");
        let id = child.id();
        let mut children = Children(vec![child]);
        if let Err(err) = group.adopt(ProcessId(id)) {
            eprintln!("skipped: the process cannot be moved into the group ({err})");
            return;
        }

        // A window already over: tried once, and the group is still there.
        assert!(!remove_tree_by(&dir, Instant::now()));
        assert!(dir.is_dir(), "a group holding a process was removed");

        children.0[0].kill().expect("the process is killed");
        children.0[0].wait().expect("the process is reaped");
        assert!(remove_tree_by(
            &dir,
            Instant::now() + crate::test_support::PATIENCE
        ));
        assert!(!dir.exists());
    }

    /// A group ended while the kernel has not yet let go of a process that was killed in it, which
    /// the process's reaping a moment later does, is removed by that end rather than left behind.
    #[test]
    fn ending_a_group_whose_killed_process_is_reaped_a_moment_later_removes_it() {
        let Some(base) = Subtree::below_own() else {
            return;
        };
        let Some(group) = Group::create(&base, "end-busy", &launch_name("end-busy")) else {
            return;
        };
        let dir = group.dir.clone();
        let mut child = programs::sleep(60).command().spawn().expect("a process");
        if let Err(err) = group.adopt(ProcessId(child.id())) {
            child.kill().ok();
            child.wait().ok();
            eprintln!("skipped: the process cannot be moved into the group ({err})");
            return;
        }
        // Reaped as soon as the end kills it, which only then lets the kernel release the group.
        let reaper = std::thread::spawn(move || child.wait());

        assert!(group.end(&[]), "the group was left in place");
        assert!(!dir.exists());
        reaper
            .join()
            .expect("the reaper ran")
            .expect("the process was reaped");
    }

    /// A launch's group below the cgroup of a host the daemon was told of is found again, and
    /// listed among the leftovers once, even when the launch has one below the daemon's own too.
    #[test]
    fn a_group_below_a_noted_host_is_found_again() {
        let Some(host) = Host::start("noted") else {
            return;
        };
        let (found, both) = (launch_name("noted"), launch_name("noted-both"));
        host.cgroup.make_launch_dir("noted-inst", &found);
        host.cgroup.make_launch_dir("noted-inst", &both);
        let in_own = Containment::for_launch("noted-inst", &both).expect("the containment");
        if in_own.members().is_none() {
            return;
        }

        assert_eq!(
            Containment::reopen("noted-inst", &found).and_then(|c| c.members()),
            None,
            "found before the host is noted"
        );
        assert!(!Containment::leftovers("noted-inst").contains(&found));

        Containment::note_host(&host.identity());
        let reopened = Containment::reopen("noted-inst", &found).expect("the launch");
        assert_eq!(reopened.members(), Some(Vec::new()));
        let leftovers = Containment::leftovers("noted-inst");
        for launch in [&found, &both] {
            assert_eq!(
                leftovers.iter().filter(|name| *name == launch).count(),
                1,
                "{launch} in {leftovers:?}"
            );
        }
        let reopened_both = Containment::reopen("noted-inst", &both).expect("the launch");
        assert_eq!(reopened_both.members(), Some(Vec::new()));
        // A reopened group is its taker's to remove, once nothing is in it.
        drop((reopened, reopened_both, in_own));
        assert!(!host.cgroup.launch_dir("noted-inst", &found).exists());
    }
}
