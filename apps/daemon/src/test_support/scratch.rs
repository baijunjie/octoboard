//! The scratch directory a test works in and removes on drop, the files, links and fifos made in
//! one, and the second real path a macOS firmlink gives it.

use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Weak};

/// A scratch directory for one test, removed when whatever holds it is dropped. Dereferences to
/// the path, so it is passed as one.
///
/// The name is `octoboardd-<label>-<pid>-<counter>`: the label attributes a leftover directory to
/// the test that made it, and the counter (not a thread id, which is reused within a process)
/// keeps two directories of one run apart. Across runs a pid can be recycled, which is why the
/// directory is also removed before being created: a run killed outright leaves its directory
/// behind, and a leftover file in it (a SQLite database, say) would be picked up by the next run
/// that reuses the name.
///
/// Bind it to a named local: a temporary guard, such as one passed straight into `Store::open`,
/// removes the directory at the end of that statement.
///
/// Work a test leaves running can write into the directory after the guard has gone, and the
/// daemon's writers create the directories they write to, so such a write puts the directory back.
/// What writes there through a shared value — an `AppState`, a process ledger — is named with
/// [`ScratchDir::removed_again_after`], and the directory is removed again once nothing holds it.
pub(crate) struct ScratchDir {
    path: PathBuf,
    writers: Vec<Writer>,
}

/// A value whose holders may still write into a scratch directory, held weakly: while it is alive,
/// so is someone who can write.
type Writer = Weak<dyn std::any::Any + Send + Sync>;

impl ScratchDir {
    pub(crate) fn new(label: &str) -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "octoboardd-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        remove_scratch(&path);
        std::fs::create_dir_all(&path).expect("temporary directory");
        later_removals::created(&path);
        Self {
            path,
            writers: Vec::new(),
        }
    }

    /// Has the directory removed again, once the guard is gone, when the last holder of `writer`
    /// lets it go: a task or thread the test left running — an exit watcher still waiting on its
    /// poll, a blocking task the runtime finishes as it shuts down, a ledger's scanner in the
    /// middle of a tick — may hold it past the end of the test and write into the directory.
    pub(crate) fn removed_again_after<T: Send + Sync + 'static>(&mut self, writer: &Arc<T>) {
        let writer: Arc<dyn std::any::Any + Send + Sync> = writer.clone();
        self.writers.push(Arc::downgrade(&writer));
    }
}

impl std::fmt::Debug for ScratchDir {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ScratchDir").field(&self.path).finish()
    }
}

impl Deref for ScratchDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for ScratchDir {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        // Before the removal: a writer gone by now can write no more, while one that lets go only
        // after the check may have written during the removal, and is waited for.
        self.writers.retain(|writer| writer.strong_count() > 0);
        remove_scratch(&self.path);
        if !self.writers.is_empty() {
            later_removals::add(
                std::mem::take(&mut self.path),
                std::mem::take(&mut self.writers),
            );
        }
    }
}

/// Removes the scratch directory at `path` with everything in it. A fixture may leave a directory
/// in it read-only, as the session git hooks are written, whose entries cannot be removed until it
/// is writable again; so where a removal fails, every directory in the tree is made writable to
/// its owner and the removal tried again.
fn remove_scratch(path: &Path) {
    if std::fs::remove_dir_all(path).is_ok() || !path.exists() {
        return;
    }
    make_writable(path);
    std::fs::remove_dir_all(path).ok();
}

fn make_writable(dir: &Path) {
    let Ok(meta) = std::fs::symlink_metadata(dir) else {
        return;
    };
    if !meta.is_dir() {
        return;
    }
    let mut permissions = meta.permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(permissions.mode() | 0o700);
    }
    // On Windows the attribute is all there is to clear; the lint is about Unix's mode bits.
    #[cfg(windows)]
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    std::fs::set_permissions(dir, permissions).ok();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        make_writable(&entry.path());
    }
}

/// The scratch directories removed while something that may write into them was still alive: one
/// thread removes each again once its writers are all gone. When the test binary exits, every
/// scratch directory it created that exists again is removed, which also covers a writer nobody
/// named with [`ScratchDir::removed_again_after`].
mod later_removals {
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, Once, PoisonError};
    use std::time::Duration;

    use super::{remove_scratch, Writer};

    #[cfg(unix)]
    use libc::atexit;

    // The C runtime's own, which every Windows Rust program links.
    #[cfg(windows)]
    extern "C" {
        fn atexit(callback: extern "C" fn()) -> std::ffi::c_int;
    }

    static PENDING: Mutex<Vec<(PathBuf, Vec<Writer>)>> = Mutex::new(Vec::new());
    static CREATED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

    /// Notes a scratch directory just created, for the sweep at exit.
    pub(super) fn created(path: &Path) {
        static AT_EXIT: Once = Once::new();
        AT_EXIT.call_once(|| {
            // SAFETY: registers a plain function with no captured state, which `exit` calls once.
            unsafe { atexit(at_exit) };
        });
        CREATED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(path.to_path_buf());
    }

    pub(super) fn add(path: PathBuf, writers: Vec<Writer>) {
        static START: Once = Once::new();
        PENDING
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((path, writers));
        START.call_once(|| {
            std::thread::Builder::new()
                .name("scratch-removals".to_string())
                .spawn(|| loop {
                    std::thread::sleep(Duration::from_millis(50));
                    remove_released();
                })
                .expect("the scratch removal thread");
        });
    }

    /// Removes the pending directories whose writers are all gone. They are taken out under the
    /// lock and removed after it, so a guard being dropped meanwhile does not wait on a large tree.
    fn remove_released() {
        let released: Vec<PathBuf> = {
            let mut pending = PENDING.lock().unwrap_or_else(PoisonError::into_inner);
            let (released, kept) = std::mem::take(&mut *pending)
                .into_iter()
                .partition(|(_, writers)| writers.iter().all(|writer| writer.strong_count() == 0));
            *pending = kept;
            released.into_iter().map(|(path, _)| path).collect()
        };
        for path in released {
            remove_scratch(&path);
        }
    }

    /// The test binary is exiting, so no later removal can come: a directory put back by a writer
    /// still alive, held by a thread still running or never let go, is removed now all the same.
    extern "C" fn at_exit() {
        // Nothing may unwind out of a function `exit` calls.
        let _ = std::panic::catch_unwind(|| {
            let created =
                std::mem::take(&mut *CREATED.lock().unwrap_or_else(PoisonError::into_inner));
            for path in created.iter().filter(|path| path.exists()) {
                remove_scratch(path);
            }
        });
    }
}

/// The path of one file inside a scratch directory that goes away with it. Dereferences to the
/// path, so it is passed as one; the file itself is not created.
#[derive(Debug)]
pub(crate) struct ScratchFile {
    _dir: ScratchDir,
    path: PathBuf,
}

impl ScratchFile {
    pub(crate) fn new(label: &str, file_name: &str) -> Self {
        let dir = ScratchDir::new(label);
        Self {
            path: dir.join(file_name),
            _dir: dir,
        }
    }
}

impl Deref for ScratchFile {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for ScratchFile {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

/// A symbolic link at `at` to `target`, which is taken relative to the link's directory where it is
/// relative. Windows makes a link to a directory and one to a file differently, so there the kind
/// follows what `target` is now (a file link where it does not exist), and a `/` in the target is
/// written `\`, the only separator a Windows link follows.
pub(crate) fn symlink(target: impl AsRef<Path>, at: impl AsRef<Path>) -> std::io::Result<()> {
    let (target, at) = (target.as_ref(), at.as_ref());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, at)
    }
    #[cfg(windows)]
    {
        let target = std::path::PathBuf::from(target.to_string_lossy().replace('/', "\\"));
        let target = target.as_path();
        let resolved = at.parent().unwrap_or(Path::new("")).join(target);
        if resolved.is_dir() {
            std::os::windows::fs::symlink_dir(target, at)
        } else {
            std::os::windows::fs::symlink_file(target, at)
        }
    }
}

/// Makes the file at `path` one the system would run: execute permission for everyone on Unix.
/// Windows runs a file by its extension, so there it changes nothing.
pub(crate) fn make_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .expect("the file is made executable");
    }
    #[cfg(windows)]
    let _ = path;
}

/// A FIFO at `path`, replacing whatever is there: a file whose open blocks until a writer appears.
#[cfg(unix)]
pub(crate) fn mkfifo(path: &Path) {
    use std::os::unix::ffi::OsStrExt;
    let _ = std::fs::remove_file(path);
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
}

/// The second real path of `real`, for a test that also spells a directory that way: `None`, with
/// a note, where there is none to be had, and a failure where there must be one, so a Mac whose
/// scratch directory is on its data volume never loses that spelling silently.
pub(crate) fn firmlink_spelling(real: &Path) -> Option<PathBuf> {
    let second = second_real_path(real);
    if second.is_none() {
        assert!(
            !must_have_second_real_path(real),
            "{} has no second real path",
            real.display()
        );
        eprintln!("{} has no second real path", real.display());
    }
    second
}

/// `/System/Volumes/Data` followed by `real`, where that is the same directory as `real` under
/// another real path: on macOS the data volume's firmlinks give everything under `/private` and
/// `/Users` one, as a bind mount does on Linux.
fn second_real_path(real: &Path) -> Option<PathBuf> {
    let second = Path::new("/System/Volumes/Data").join(real.strip_prefix("/").ok()?);
    let identity =
        |path: &Path| crate::platform::fs::identity(path, crate::platform::fs::Links::Follow).ok();
    let distinct =
        crate::hostfs::canonical(&second).ok()? != crate::hostfs::canonical(real).ok()?;
    (distinct && identity(&second).is_some() && identity(&second) == identity(real))
        .then_some(second)
}

/// Whether [`second_real_path`] must find one for `real`: on macOS, where `real` is on the same
/// device as `/System/Volumes/Data`, which the system volume shares, and is not already spelled
/// through it. A path on another mounted volume (`TMPDIR` on an external disk) has none:
/// `realpath` folds its `/System/Volumes/Data/...` spelling back to the volume's own.
fn must_have_second_real_path(real: &Path) -> bool {
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::MetadataExt as _;
        let data = Path::new("/System/Volumes/Data");
        let device = |path: &Path| std::fs::metadata(path).ok().map(|meta| meta.dev());
        !real.starts_with(data) && device(real).is_some() && device(real) == device(data)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = real;
        false
    }
}
