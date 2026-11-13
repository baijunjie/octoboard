use std::ffi::{CStr, OsStr, OsString};
use std::fs::{File, OpenOptions, Permissions};
use std::io;
use std::os::fd::IntoRawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use super::{AliasKind, EntryType, Identity, Links, Listed, Opens, Privacy, Stat};

pub(super) fn host_stat(path: &Path, links: Links) -> io::Result<Stat> {
    let metadata = match links {
        Links::Follow => std::fs::metadata(path)?,
        Links::Keep => std::fs::symlink_metadata(path)?,
    };
    Ok(stat_from(metadata))
}

pub(super) fn host_stat_of(file: &File) -> io::Result<Stat> {
    Ok(stat_from(file.metadata()?))
}

/// The pid of the process holding a lock on the file at `path`, where the host says and exactly one
/// process does; `None` otherwise. Only Linux says, in `/proc/locks`.
pub fn lock_holder(path: &Path) -> Option<u32> {
    #[cfg(target_os = "linux")]
    {
        let metadata = std::fs::metadata(path).ok()?;
        let dev = metadata.dev();
        let locks = std::fs::read_to_string("/proc/locks").ok()?;
        super::lock_table::holder_in(&locks, libc::major(dev), libc::minor(dev), metadata.ino())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
        None
    }
}

fn stat_from(metadata: std::fs::Metadata) -> Stat {
    let identity = Identity {
        volume: metadata.dev(),
        file: metadata.ino(),
    };
    Stat { metadata, identity }
}

/// A regular file's version: its device and inode, size, and modification and change times, as one
/// opaque string. The change time moves with every write and every replacement of the inode, so two
/// equal versions mean nothing about the file changed in between. It is built from fields a plain
/// `stat` carries, so [`version`] and [`EntryStat::version`] of the same file agree.
fn version_of(dev: u64, ino: u64, size: u64, mtime: (i64, i64), ctime: (i64, i64)) -> String {
    format!(
        "{dev}:{ino}:{size}:{}.{}:{}.{}",
        mtime.0, mtime.1, ctime.0, ctime.1
    )
}

/// The version of the file `stat` describes (see [`EntryStat::version`] for the same file seen
/// through a directory listing).
pub fn version(stat: &Stat) -> String {
    let metadata = &stat.metadata;
    version_of(
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        (metadata.mtime(), metadata.mtime_nsec()),
        (metadata.ctime(), metadata.ctime_nsec()),
    )
}

/// The name of what a file is where it is none of a regular file, a directory or a link and the
/// host tells it apart: `fifo`, `socket` or `device`; `None` for anything else.
pub fn special_file_name(metadata: &std::fs::Metadata) -> Option<&'static str> {
    let file_type = metadata.file_type();
    if file_type.is_fifo() {
        Some("fifo")
    } else if file_type.is_socket() {
        Some("socket")
    } else if file_type.is_block_device() || file_type.is_char_device() {
        Some("device")
    } else {
        None
    }
}

/// Makes `options` open without waiting, and without making the file the process's terminal.
pub(super) fn no_wait(options: &mut OpenOptions) {
    options.custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY);
}

pub(super) fn host_open_without_links(
    scope: &Path,
    below: &Path,
    opens: Opens,
) -> io::Result<File> {
    let path = if below.as_os_str().is_empty() {
        scope.to_path_buf()
    } else {
        scope.join(below)
    };
    let flags = match opens {
        Opens::File => libc::O_NONBLOCK | libc::O_NOCTTY,
        Opens::Directory => libc::O_DIRECTORY | libc::O_NOCTTY,
    };
    open_refusing_links(&path, flags)
}

/// Opens `path` for reading with `flags`, failing with `ELOOP` should any component of it be a
/// symbolic link: `O_NOFOLLOW_ANY` on macOS, `openat2` with `RESOLVE_NO_SYMLINKS` on Linux. Where
/// neither is available (another system, a Linux kernel before 5.6, a sandbox that refuses
/// `openat2`) only the last component is held to that, and a link swapped into a directory above
/// it is followed.
fn open_refusing_links(path: &Path, flags: libc::c_int) -> io::Result<File> {
    #[cfg(target_os = "macos")]
    {
        OpenOptions::new()
            .read(true)
            .custom_flags(flags | libc::O_NOFOLLOW_ANY)
            .open(path)
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::fd::FromRawFd;
        let c_path = std::ffi::CString::new(path.as_os_str().as_bytes())?;
        // SAFETY: `open_how` is plain integers, for which all zeroes is a valid value (and the
        // value the kernel requires of any field not set).
        let mut how: libc::open_how = unsafe { std::mem::zeroed() };
        how.flags = (flags | libc::O_RDONLY | libc::O_CLOEXEC) as u64;
        how.resolve = libc::RESOLVE_NO_SYMLINKS;
        // SAFETY: `openat2` reads `how` (of the size passed) and the NUL-terminated path, both
        // alive for the call; a non-negative result is a descriptor this process now owns.
        let fd = unsafe {
            libc::syscall(
                libc::SYS_openat2,
                libc::AT_FDCWD,
                c_path.as_ptr(),
                &how as *const libc::open_how,
                std::mem::size_of::<libc::open_how>(),
            )
        };
        if fd >= 0 {
            // SAFETY: see above.
            return Ok(unsafe { File::from_raw_fd(fd as libc::c_int) });
        }
        let err = io::Error::last_os_error();
        // `ENOSYS` from a kernel older than `openat2` (5.6), `EPERM` from a seccomp filter (older
        // container runtimes) that does not know it: fall back, holding only the last component.
        if !matches!(err.raw_os_error(), Some(libc::ENOSYS | libc::EPERM)) {
            return Err(err);
        }
        OpenOptions::new()
            .read(true)
            .custom_flags(flags | libc::O_NOFOLLOW)
            .open(path)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        OpenOptions::new()
            .read(true)
            .custom_flags(flags | libc::O_NOFOLLOW)
            .open(path)
    }
}

/// Whether `err` is the refusal of a link on the way to a file: a path with a link where none may
/// be, or links round in a loop.
pub fn is_symlink_loop(err: &io::Error) -> bool {
    err.raw_os_error() == Some(libc::ELOOP)
}

/// Whether `err` says a file lies where a directory on the way should be.
pub fn is_not_a_directory(err: &io::Error) -> bool {
    err.raw_os_error() == Some(libc::ENOTDIR)
}

/// Whether `err` says a name or a path is longer than the host takes.
pub fn is_name_too_long(err: &io::Error) -> bool {
    err.raw_os_error() == Some(libc::ENAMETOOLONG)
}

/// Opens `staged` for writing, creating or truncating it, with the permissions `privacy` says for
/// the file it is to replace at `target`, so that no wider ones are ever in effect.
pub fn create_replacement(staged: &Path, target: &Path, privacy: Privacy) -> io::Result<File> {
    let kept = || {
        std::fs::metadata(target)
            .ok()
            .map(|metadata| metadata.permissions().mode() & 0o7777)
    };
    // The mode an open applies is cut by the umask and ignored for a file already there, so a mode
    // that must hold exactly is set again once the file is open.
    let (create_mode, exact_mode) = match privacy {
        Privacy::Private => (0o600, Some(0o600)),
        Privacy::KeptOrPrivate => {
            let mode = kept().unwrap_or(0o600);
            (mode, Some(mode))
        }
        Privacy::KeptOrDefault => match kept() {
            Some(mode) => (mode, Some(mode)),
            None => (0o666, None),
        },
    };
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(create_mode)
        .open(staged)?;
    if let Some(mode) = exact_mode {
        let set = file.set_permissions(Permissions::from_mode(mode));
        if privacy != Privacy::KeptOrDefault {
            set?;
        }
    }
    Ok(file)
}

/// Creates `staged`, which must not exist, with the permissions of the file `target`, a link at
/// `staged` being refused rather than followed: the file a program is about to rename over
/// `target`, which another user of the directory must not be able to redirect.
pub fn create_new_like(staged: &Path, target: &Path) -> io::Result<File> {
    let mode = std::fs::metadata(target)?.permissions().mode() & 0o7777;
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .custom_flags(libc::O_NOFOLLOW)
        .open(staged)?;
    // The mode an open applies is cut by the umask.
    file.set_permissions(Permissions::from_mode(mode))?;
    Ok(file)
}

/// Opens `path` with `options`, creating it readable by its owner alone from the first byte.
pub fn create_private(path: &Path, options: &mut OpenOptions) -> io::Result<File> {
    options.mode(0o600).open(path)
}

/// Creates the directory `dir`, and any missing parent, as the owner's alone to enter, list and
/// change, from the start. A directory that is there already is left as it is.
pub fn create_private_dir(dir: &Path) -> io::Result<()> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
}

/// Makes the directory `dir` the owner's alone to enter, list and change.
pub fn restrict_to_owner(dir: &Path) -> io::Result<()> {
    std::fs::set_permissions(dir, Permissions::from_mode(0o700))
}

/// Makes everything the daemon creates below the data directory `dir` private to the user. On Unix
/// each file is made private as it is created ([`create_private`], [`create_replacement`]); the
/// directory itself is closed to everyone else as well, as one created by an earlier build under a
/// permissive umask may not be, since the owner-only socket the browser reaches the daemon by lives
/// in it and is only as private as the directory. A directory another user owns is left as it is:
/// changing its mode is not this user's to do, and the error says so.
pub fn keep_private_below(dir: &Path) -> io::Result<()> {
    let metadata = std::fs::metadata(dir)?;
    // SAFETY: `geteuid` takes no arguments and cannot fail.
    let me = unsafe { libc::geteuid() };
    if metadata.uid() != me {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "{} belongs to another user ({})",
                dir.display(),
                metadata.uid()
            ),
        ));
    }
    if metadata.mode() & 0o077 != 0 {
        restrict_to_owner(dir)?;
    }
    Ok(())
}

/// The contents of the file at `path`, or `None` when it is not a private file of this user's: a
/// symlink (never followed), something other than a regular file, a file another user owns, or one
/// anyone else may read or write. Never waits on a FIFO.
pub fn read_private_file(path: &Path) -> io::Result<Option<Vec<u8>>> {
    use std::io::Read;

    let mut file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(err) if is_symlink_loop(&err) => return Ok(None),
        Err(err) => return Err(err),
    };
    let metadata = file.metadata()?;
    // SAFETY: `geteuid` takes no arguments and cannot fail.
    let me = unsafe { libc::geteuid() };
    if !metadata.is_file() || metadata.uid() != me || metadata.mode() & 0o077 != 0 {
        return Ok(None);
    }
    let mut content = Vec::new();
    file.read_to_end(&mut content)?;
    Ok(Some(content))
}

/// Whether `path` leads to a regular file the system would run.
pub fn is_executable(path: &Path) -> bool {
    std::fs::metadata(path)
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// Whether this process may create files in the directory `dir` and rename them there: the
/// system's own answer (`access(2)`), which counts a read-only mount, a mode and an ACL alike.
pub fn directory_writable(dir: &Path) -> bool {
    let Ok(path) = std::ffi::CString::new(dir.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `path` is a NUL-terminated string that outlives the call.
    unsafe { libc::access(path.as_ptr(), libc::W_OK | libc::X_OK) == 0 }
}

/// Flushes the entries of the directory `dir` to the disk, so that a rename in it survives a crash.
pub fn sync_directory(dir: &Path) -> io::Result<()> {
    File::open(dir)?.sync_all()
}

/// Whether `a` and `b` lead to the same file, symlinks followed: the same device and inode, as two
/// spellings of a path can name one file.
pub fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(a), Ok(b)) => (a.dev(), a.ino()) == (b.dev(), b.ino()),
        _ => false,
    }
}

/// The directories the host keeps scratch files in, as written: `/tmp`, macOS's `/private/tmp`
/// that it leads to, and the process's temporary directory (`$TMPDIR`). Some may not exist.
pub fn temp_roots() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/tmp"),
        PathBuf::from("/private/tmp"),
        std::env::temp_dir(),
    ]
}

/// Whether `path` leads to a file git would run as a hook: a regular file it may execute.
pub fn is_hook_script(path: &Path) -> bool {
    is_executable(path)
}

/// Sets the modification time of the directory `dir`. Its owner may set a time on a directory
/// whatever its mode is.
pub fn set_dir_modified(dir: &Path, time: std::time::SystemTime) -> io::Result<()> {
    File::open(dir)?.set_modified(time)
}

/// Whether the entry at `path` itself (a link is not followed) belongs to the user the daemon runs
/// as and cannot be written by anyone else. A link has no mode of its own to judge (it is always
/// open on Linux), so only its owner counts.
pub fn is_private_to_user(path: &Path) -> bool {
    // SAFETY: `geteuid` takes no arguments and cannot fail.
    let me = unsafe { libc::geteuid() };
    std::fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.uid() == me && (metadata.is_symlink() || metadata.mode() & 0o022 == 0)
    })
}

/// Marks an executable file or a directory the daemon publishes as read-only (everyone may read
/// and run or enter it, no one may change it), or writable by its owner again.
pub fn set_read_only(path: &Path, read_only: bool) -> io::Result<()> {
    let mode = if read_only { 0o555 } else { 0o755 };
    std::fs::set_permissions(path, Permissions::from_mode(mode))
}

/// Whether the directory `path` is the root of a volume: `/`, or a mount point, which sits on a
/// different device from its parent. A directory or parent that cannot be read counts as one,
/// since unknown never destroys.
pub fn is_volume_root(path: &Path) -> bool {
    let Some(parent) = path.parent() else {
        return true;
    };
    match (std::fs::metadata(path), std::fs::metadata(parent)) {
        (Ok(this), Ok(above)) => this.dev() != above.dev(),
        _ => true,
    }
}

/// Makes the directory alias `at` to `target`.
pub fn alias_dir(target: &Path, at: &Path) -> io::Result<AliasKind> {
    std::os::unix::fs::symlink(target, at)?;
    Ok(AliasKind::Symlink)
}

/// Makes the file alias `at` to `target`.
pub fn alias_file(target: &Path, at: &Path) -> io::Result<AliasKind> {
    std::os::unix::fs::symlink(target, at)?;
    Ok(AliasKind::Symlink)
}

/// Where the alias at `at` leads, as it is written (a relative target stays relative); `None`
/// where something is there that is no alias.
pub fn read_alias(at: &Path) -> io::Result<Option<PathBuf>> {
    match std::fs::read_link(at) {
        Ok(target) => Ok(Some(target)),
        Err(err) if err.kind() == io::ErrorKind::InvalidInput => Ok(None),
        Err(err) => Err(err),
    }
}

/// Removes the file or link at `path` itself, never what a link leads to; a directory of its own
/// is refused.
pub fn remove_file_or_link(path: &Path) -> io::Result<()> {
    std::fs::remove_file(path)
}

/// Whether [`alias_file`] can make an alias of a file on this host: always on Unix.
pub fn file_links_available() -> bool {
    true
}

/// Whether a SQLite database opened through a file alias shares its write-ahead log (`-wal`) and
/// its index (`-shm`) with the alias's target: SQLite's Unix layer resolves the alias before it
/// names them, so whoever opens the database, by either path, coordinates through the target's.
pub const SQLITE_FOLLOWS_FILE_ALIASES: bool = true;

/// What a directory listing tells of one entry, read without following a link.
pub struct EntryStat {
    pub kind: EntryType,
    pub size: u64,
    stat: libc::stat,
}

impl EntryStat {
    /// The [`version`] a read of this entry, a regular file, would report.
    #[allow(clippy::unnecessary_cast)] // The `stat` fields' types differ between platforms.
    pub fn version(&self) -> String {
        version_of(
            self.stat.st_dev as u64,
            self.stat.st_ino as u64,
            self.stat.st_size as u64,
            (self.stat.st_mtime as i64, self.stat.st_mtime_nsec as i64),
            (self.stat.st_ctime as i64, self.stat.st_ctime_nsec as i64),
        )
    }
}

/// Reads the entries of an open directory through its handle, so what is listed is the directory
/// that was opened, whatever has happened to its path since.
pub struct DirReader {
    dir: std::ptr::NonNull<libc::DIR>,
}

impl DirReader {
    pub fn new(directory: File) -> io::Result<Self> {
        let fd = directory.into_raw_fd();
        // SAFETY: `fd` is an open directory descriptor this process owns; on success the stream
        // takes it over and `closedir` closes it, and on failure it is closed here.
        let dir = unsafe { libc::fdopendir(fd) };
        match std::ptr::NonNull::new(dir) {
            Some(dir) => Ok(Self { dir }),
            None => {
                let err = io::Error::last_os_error();
                // SAFETY: the descriptor was not taken over by a stream.
                unsafe { libc::close(fd) };
                Err(err)
            }
        }
    }

    /// The next entry, skipping `.` and `..`; `None` at the end.
    pub fn next_entry(&mut self) -> io::Result<Option<Listed>> {
        loop {
            clear_errno();
            // SAFETY: `self.dir` is an open stream; the entry it returns is valid until the next
            // call on the same stream, and is copied out before that.
            let entry = unsafe { libc::readdir(self.dir.as_ptr()) };
            if entry.is_null() {
                let err = io::Error::last_os_error();
                return match err.raw_os_error() {
                    Some(0) | None => Ok(None),
                    Some(_) => Err(err),
                };
            }
            // SAFETY: see above; `d_name` is NUL-terminated.
            let (name, d_type) = unsafe {
                let entry = &*entry;
                (
                    CStr::from_ptr(entry.d_name.as_ptr()).to_bytes().to_vec(),
                    entry.d_type,
                )
            };
            if name != b"." && name != b".." {
                let hint = match d_type {
                    libc::DT_LNK => Some(EntryType::Link),
                    libc::DT_DIR => Some(EntryType::Directory),
                    libc::DT_REG => Some(EntryType::File),
                    _ => None,
                };
                return Ok(Some(Listed { name, hint }));
            }
        }
    }

    /// The entry's own metadata, not following a link it is; `None` where it cannot be read.
    pub fn stat(&self, entry: &Listed) -> Option<EntryStat> {
        let name = std::ffi::CString::new(entry.name.as_slice()).ok()?;
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: the stream's descriptor is open, `name` is NUL-terminated, and `stat` is
        // written in full on success.
        let result = unsafe {
            libc::fstatat(
                libc::dirfd(self.dir.as_ptr()),
                name.as_ptr(),
                stat.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        };
        // SAFETY: initialised by the successful call.
        let stat = (result == 0).then(|| unsafe { stat.assume_init() })?;
        let kind = match stat.st_mode as libc::mode_t & libc::S_IFMT {
            libc::S_IFLNK => EntryType::Link,
            libc::S_IFDIR => EntryType::Directory,
            libc::S_IFREG => EntryType::File,
            _ => EntryType::Other,
        };
        Some(EntryStat {
            kind,
            size: stat.st_size as u64,
            stat,
        })
    }
}

impl Drop for DirReader {
    fn drop(&mut self) {
        // SAFETY: the stream is open and closed only here.
        unsafe { libc::closedir(self.dir.as_ptr()) };
    }
}

/// Zeroes `errno`, which `readdir` leaves alone at the end of a directory and sets on an error.
fn clear_errno() {
    // SAFETY: each returns this thread's `errno` location.
    #[cfg(target_os = "macos")]
    unsafe {
        *libc::__error() = 0;
    }
    #[cfg(target_os = "linux")]
    unsafe {
        *libc::__errno_location() = 0;
    }
}

/// The `HOME` environment variable as it is set, if it is.
pub fn env_home() -> Option<OsString> {
    std::env::var_os("HOME")
}

/// The home directory the system user database records for the effective user, as it is written
/// there.
pub fn account_home() -> Option<PathBuf> {
    account_field(|entry| entry.pw_dir).map(PathBuf::from)
}

/// The login name the system user database records for the effective user, or `None` where it
/// records none.
pub fn account_name() -> Option<OsString> {
    account_field(|entry| entry.pw_name)
}

/// The login shell the system user database records for the effective user, as it is written
/// there, or `None` where it records none.
pub fn account_shell() -> Option<PathBuf> {
    account_field(|entry| entry.pw_shell)
        .filter(|shell| !shell.is_empty())
        .map(PathBuf::from)
}

/// The string field `field` picks from the system user database's entry for the effective user,
/// or `None` where there is no entry or the field is null.
fn account_field(field: fn(&libc::passwd) -> *mut libc::c_char) -> Option<OsString> {
    let mut buffer = vec![0 as libc::c_char; 4096];
    loop {
        // SAFETY: an all-zero `passwd` is a valid value, being only pointers and integers. `entry`,
        // `found` and `buffer` outlive the call, which is told `buffer.len()` as the size it may
        // write into. The field is dereferenced only once the call succeeded and found an entry,
        // when it is null or a NUL-terminated string inside `buffer`, and it is copied out before
        // `buffer` is grown or dropped.
        let (status, value) = unsafe {
            let mut entry: libc::passwd = std::mem::zeroed();
            let mut found: *mut libc::passwd = std::ptr::null_mut();
            let status = libc::getpwuid_r(
                libc::geteuid(),
                &mut entry,
                buffer.as_mut_ptr(),
                buffer.len(),
                &mut found,
            );
            let value = field(&entry);
            let value = (status == 0 && !found.is_null() && !value.is_null())
                .then(|| OsStr::from_bytes(CStr::from_ptr(value).to_bytes()).to_owned());
            (status, value)
        };
        if status == libc::ERANGE && buffer.len() < 1 << 20 {
            buffer.resize(buffer.len() * 2, 0);
            continue;
        }
        return value;
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    /// A scratch directory of the test's own, removed when it is dropped, a failed test's included.
    fn scratch(label: &str) -> crate::test_support::ScratchDir {
        crate::test_support::ScratchDir::new(&format!("fs-{label}"))
    }

    fn mode_of(path: &std::path::Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
    }

    /// A data directory left open to others by an earlier build is closed to them; a private one
    /// is left as it is; a new one is private from the start.
    #[test]
    fn the_data_directory_is_made_private() {
        let dir = scratch("keep-private");
        let data = dir.join("data");
        std::fs::create_dir(&data).unwrap();
        for mode in [0o775, 0o755, 0o750, 0o700] {
            std::fs::set_permissions(&data, std::fs::Permissions::from_mode(mode)).unwrap();
            super::keep_private_below(&data).unwrap();
            assert_eq!(mode_of(&data), 0o700, "{mode:o}");
        }
        let fresh = dir.join("fresh").join("data");
        super::create_private_dir(&fresh).unwrap();
        assert_eq!(mode_of(&fresh), 0o700);
    }

    /// A directory another user owns is an error, which the daemon only warns of: its mode is not
    /// this user's to change, so it is left as it is. A directory that is not there is an error
    /// too. And a directory made private from the start is not tightened or loosened by being made
    /// again.
    #[test]
    fn a_data_directory_of_another_user_is_refused_and_left_as_it_is() {
        // SAFETY: `geteuid` takes no arguments and cannot fail.
        if unsafe { libc::geteuid() } != 0 {
            let root_owned = std::path::Path::new("/usr");
            let before = mode_of(root_owned);
            let refused = super::keep_private_below(root_owned).unwrap_err();
            assert_eq!(refused.kind(), std::io::ErrorKind::PermissionDenied);
            assert_eq!(mode_of(root_owned), before);
        }

        let dir = scratch("keep-private-missing");
        assert!(super::keep_private_below(&dir.join("missing")).is_err());
        let open = dir.join("open");
        std::fs::create_dir(&open).unwrap();
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o755)).unwrap();
        super::create_private_dir(&open).unwrap();
        assert_eq!(
            mode_of(&open),
            0o755,
            "an existing directory is left as it is"
        );
    }

    /// A file that cannot be read is an error, not "no key": the caller must be able to tell a file
    /// to replace from one to leave alone. A process that ignores modes (root) reads it.
    #[test]
    fn an_unreadable_private_file_is_an_error_not_none() {
        let dir = scratch("read-private-denied");
        let file = dir.join("key");
        std::fs::write(&file, "secret").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::read(&file).is_err() {
            let err = super::read_private_file(&file).unwrap_err();
            assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
        }
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    }

    /// Only a regular file of this user's that nobody else may read or write is read: a symlink
    /// is not followed, and a FIFO is not waited on.
    #[test]
    fn only_a_private_regular_file_is_read() {
        let dir = scratch("read-private");
        let file = dir.join("key");
        std::fs::write(&file, "secret").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            super::read_private_file(&file).unwrap().as_deref(),
            Some(&b"secret"[..])
        );
        for mode in [0o644, 0o640, 0o604, 0o620, 0o602] {
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode)).unwrap();
            assert_eq!(super::read_private_file(&file).unwrap(), None, "{mode:o}");
        }
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        let link = dir.join("link");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        assert_eq!(super::read_private_file(&link).unwrap(), None);

        let fifo = dir.join("fifo");
        let path = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: `path` is a NUL-terminated string that outlives the call.
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        assert_eq!(super::read_private_file(&fifo).unwrap(), None);
        assert!(super::read_private_file(&dir.join("missing")).is_err());
    }

    /// The effective user has an entry with an absolute home on any machine the tests run on.
    #[test]
    fn the_user_database_records_an_absolute_home() {
        assert!(super::account_home().is_some_and(|home| home.is_absolute()));
    }

    /// The string a field of the user database's entry for the effective user holds, read through
    /// `getpwuid`, which is not the call the code under test makes, or `None` where there is no
    /// entry or the field is null.
    fn recorded(field: fn(&libc::passwd) -> *mut libc::c_char) -> Option<std::ffi::OsString> {
        use std::os::unix::ffi::OsStrExt;
        // SAFETY: the entry `getpwuid` returns is read at once, and its fields are null or
        // NUL-terminated strings. Nothing else in the crate calls `getpwuid`, and its one caller
        // is a single test, so no other call can replace the entry while it is read.
        unsafe {
            let entry = libc::getpwuid(libc::geteuid());
            if entry.is_null() || field(&*entry).is_null() {
                return None;
            }
            Some(
                std::ffi::OsStr::from_bytes(std::ffi::CStr::from_ptr(field(&*entry)).to_bytes())
                    .to_owned(),
            )
        }
    }

    /// The home and the shell are what the database records, as they are written there, the shell
    /// none where it records none or an empty field. One test, so that `recorded` is never called
    /// from two threads at once.
    #[test]
    fn the_account_home_and_shell_are_what_the_user_database_records() {
        assert_eq!(
            super::account_home().map(std::path::PathBuf::into_os_string),
            recorded(|entry| entry.pw_dir)
        );
        assert_eq!(
            super::account_shell().map(std::path::PathBuf::into_os_string),
            recorded(|entry| entry.pw_shell).filter(|shell| !shell.is_empty())
        );
    }
}
