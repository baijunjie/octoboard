//! Reading the files on disk under a scope directory: resolving a client's relative path inside
//! it, listing a directory, and reading one regular file — each bounded while it reads, and each
//! refusing anything that resolves outside the scope or is not what it claims to be.
//!
//! A path is resolved with every symbolic link followed and then held to the scope, so a link that
//! stays inside is read through and one that leads out is refused. The resolved path has no link
//! left in it, and it is then opened refusing a link in any of its components (`O_NOFOLLOW_ANY` on
//! macOS, `openat2` with `RESOLVE_NO_SYMLINKS` on Linux), so a link swapped in between the check
//! and the open makes the open fail instead of redirecting it; a listing then reads the directory
//! it opened, not the path again. Where neither is available — another system, a Linux kernel
//! before 5.6, a sandbox that refuses `openat2` — only the last component is held to that, and a
//! link swapped into a directory above it is followed.
//!
//! The opened descriptor's own path cannot stand in for that check: on macOS `F_GETPATH` reports
//! whichever hard link to the file any process looked up last, so a file hard-linked from outside
//! the scope (a package manager's store, say) would be refused at random.
//!
//! A FIFO, socket or device is refused from its metadata and never read: opening a FIFO blocks
//! until a writer appears, and reading a device can be endless. One swapped in after that check is
//! opened without blocking (`O_NONBLOCK`) and refused before anything is read from it.

use std::ffi::CStr;
use std::fs::{self, File, Metadata};
use std::io::{self, Read};
use std::os::fd::IntoRawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;

use crate::browse::source::file_id;
use crate::browse::wire_path::{self, RelPath};
use crate::browse::{budget, ignored, json_escaped_len, Cancelled, Git};
use crate::protocol::{error_code, BrowseEntry, CodedError, EntryKind, LinkTarget};

/// Resolves `path` inside the scope directory `scope` (already canonical) to the canonical path
/// it names, refusing one that leads outside.
pub fn resolve(scope: &Path, path: &RelPath) -> Result<PathBuf> {
    let joined = scope.join(path.as_path());
    let canonical = fs::canonicalize(&joined).map_err(|err| io_failure(path, &err))?;
    if !canonical.starts_with(scope) {
        return Err(outside(path));
    }
    Ok(canonical)
}

/// Opens `path` for reading with `flags`, failing with `ELOOP` should any component of it be a
/// symbolic link — see the module doc.
fn open_without_links(path: &Path, flags: libc::c_int) -> io::Result<File> {
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
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
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(flags | libc::O_NOFOLLOW)
            .open(path)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(flags | libc::O_NOFOLLOW)
            .open(path)
    }
}

/// A file read whole off the disk, with the version it was read at.
#[derive(Debug)]
pub struct LiveFile {
    pub bytes: Vec<u8>,
    /// The file's [`version`] at the read.
    pub version: String,
}

/// Reads the regular file `path` names inside `scope`, refusing anything else, anything larger
/// than `limit`, and a file that changed while it was being read.
pub fn read_file(
    scope: &Path,
    path: &RelPath,
    limit: usize,
    cancel: &AtomicBool,
) -> Result<LiveFile> {
    let canonical = resolve(scope, path)?;
    read_resolved(&canonical, path, limit, cancel)
}

/// [`read_file`] from the point `path` has been resolved to `canonical`, a path with no link in
/// it.
pub(super) fn read_resolved(
    canonical: &Path,
    path: &RelPath,
    limit: usize,
    cancel: &AtomicBool,
) -> Result<LiveFile> {
    let before = fs::symlink_metadata(canonical).map_err(|err| io_failure(path, &err))?;
    if before.file_type().is_symlink() {
        return Err(changed(path));
    }
    if !before.is_file() {
        return Err(unsupported(path, file_type_name(&before)));
    }
    if before.len() > limit as u64 {
        return Err(too_large(limit, Some(before.len())));
    }
    let file = open_without_links(canonical, libc::O_NONBLOCK | libc::O_NOCTTY)
        .map_err(|err| open_failure(path, &err))?;
    let opened = file.metadata().map_err(|err| io_failure(path, &err))?;
    if !opened.is_file() || file_id(&opened) != file_id(&before) {
        return Err(changed(path));
    }
    let bytes = read_bounded(&file, limit, opened.len(), cancel).map_err(|err| match err {
        ReadFailure::Exceeded => too_large(limit, None),
        ReadFailure::Cancelled => anyhow::Error::new(Cancelled),
        ReadFailure::Io(err) => io_failure(path, &err),
    })?;
    let after = file.metadata().map_err(|err| io_failure(path, &err))?;
    if version(&opened) != version(&after) || bytes.len() as u64 != after.len() {
        return Err(changed(path));
    }
    Ok(LiveFile {
        bytes,
        version: version(&after),
    })
}

/// A regular file's version: its device and inode, size, and modification and change times, as one
/// opaque string. The change time moves with every write and every replacement of the inode, so two
/// equal versions mean nothing about the file changed in between. It is built from fields a plain
/// `stat` carries, so a listing's [`entry_version`] and a read's [`version`] of the same file agree.
fn version_of(dev: u64, ino: u64, size: u64, mtime: (i64, i64), ctime: (i64, i64)) -> String {
    format!(
        "{dev}:{ino}:{size}:{}.{}:{}.{}",
        mtime.0, mtime.1, ctime.0, ctime.1
    )
}

pub(super) fn version(metadata: &Metadata) -> String {
    version_of(
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        (metadata.mtime(), metadata.mtime_nsec()),
        (metadata.ctime(), metadata.ctime_nsec()),
    )
}

/// The [`version`] a read of the regular file `stat` describes would report.
#[allow(clippy::unnecessary_cast)] // The `stat` fields' types differ between platforms.
fn entry_version(stat: &libc::stat) -> String {
    version_of(
        stat.st_dev as u64,
        stat.st_ino as u64,
        stat.st_size as u64,
        (stat.st_mtime as i64, stat.st_mtime_nsec as i64),
        (stat.st_ctime as i64, stat.st_ctime_nsec as i64),
    )
}

#[derive(Debug)]
pub enum ReadFailure {
    Exceeded,
    Cancelled,
    Io(io::Error),
}

/// The size of each read, and how often a cancel is noticed.
const READ_CHUNK: usize = 64 * 1024;

/// Reads `source` to its end, holding at most `limit` bytes: one byte more and the read stops
/// there, whatever the source would still give. A file that grows while it is read is therefore
/// bounded by the read, not by the size it had when it was opened. The buffer is sized from
/// `expected` and grown at most once, straight to the limit, so it never holds more than that.
pub fn read_bounded(
    mut source: impl Read,
    limit: usize,
    expected: u64,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, ReadFailure> {
    let mut bytes = Vec::with_capacity(expected.min(limit as u64) as usize + 1);
    let mut chunk = vec![0u8; READ_CHUNK];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(ReadFailure::Cancelled);
        }
        let room = (limit - bytes.len() + 1).min(READ_CHUNK);
        match source.read(&mut chunk[..room]) {
            Ok(0) => return Ok(bytes),
            Ok(n) => {
                if bytes.len() + n > bytes.capacity() {
                    bytes.reserve_exact(limit + 1 - bytes.len());
                }
                bytes.extend_from_slice(&chunk[..n]);
                if bytes.len() > limit {
                    return Err(ReadFailure::Exceeded);
                }
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(err) => return Err(ReadFailure::Io(err)),
        }
    }
}

/// A directory listing, possibly cut at a budget.
#[derive(Debug)]
pub struct Listing {
    pub entries: Vec<BrowseEntry>,
    pub complete: bool,
}

/// Lists the directory `path` names inside `scope`, sorted by the entries' wire names. Stops,
/// marking the listing incomplete, at [`budget::MAX_DIR_ENTRIES`] entries or
/// [`budget::MAX_LISTING_NAME_BYTES`] of names as they will be sent, without reading the rest of
/// the directory; an entry that cannot be read is left out and marks it incomplete too. The
/// entries Git ignores are marked as [`ignored`] says.
pub fn list_dir(scope: &Path, path: &RelPath, git: &Git, cancel: &AtomicBool) -> Result<Listing> {
    let canonical = resolve(scope, path)?;
    let mut listing = list_resolved(
        scope,
        &canonical,
        path,
        budget::MAX_DIR_ENTRIES,
        budget::MAX_LISTING_NAME_BYTES,
        cancel,
    )?;
    ignored::mark_listing(git, &canonical, &mut listing.entries, cancel)?;
    Ok(listing)
}

/// [`list_dir`] from the point `path` has been resolved to `canonical`, a path with no link in it,
/// under the budgets given.
pub(super) fn list_resolved(
    scope: &Path,
    canonical: &Path,
    path: &RelPath,
    max_entries: usize,
    max_name_bytes: usize,
    cancel: &AtomicBool,
) -> Result<Listing> {
    let metadata = fs::symlink_metadata(canonical).map_err(|err| io_failure(path, &err))?;
    if metadata.file_type().is_symlink() {
        return Err(changed(path));
    }
    if !metadata.is_dir() {
        return Err(unsupported(path, file_type_name(&metadata)));
    }
    let directory = open_without_links(canonical, libc::O_DIRECTORY | libc::O_NOCTTY)
        .map_err(|err| open_failure(path, &err))?;
    let opened = directory.metadata().map_err(|err| io_failure(path, &err))?;
    if file_id(&opened) != file_id(&metadata) {
        return Err(changed(path));
    }
    let mut reader = DirReader::new(directory).map_err(|err| io_failure(path, &err))?;
    let mut entries = Vec::new();
    let mut name_bytes = 0usize;
    let mut complete = true;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(anyhow::Error::new(Cancelled));
        }
        let (raw, d_type) = match reader.next_entry() {
            Ok(Some(entry)) => entry,
            Ok(None) => break,
            Err(err) => {
                tracing::debug!(%err, "a directory stopped reading part-way");
                complete = false;
                break;
            }
        };
        let name = wire_path::encode(&raw);
        let sent = json_escaped_len(name.as_bytes());
        if entries.len() == max_entries || name_bytes + sent > max_name_bytes {
            complete = false;
            break;
        }
        name_bytes += sent;
        let Some(stat) = reader.stat(&raw) else {
            complete = false;
            continue;
        };
        let kind = match d_type {
            libc::DT_LNK => libc::S_IFLNK,
            libc::DT_DIR => libc::S_IFDIR,
            libc::DT_REG => libc::S_IFREG,
            _ => stat.st_mode as libc::mode_t & libc::S_IFMT,
        };
        let (kind, size, version, target) = match kind {
            libc::S_IFLNK => {
                let link = canonical.join(std::ffi::OsStr::from_bytes(&raw));
                (
                    EntryKind::Symlink,
                    None,
                    None,
                    Some(link_target(scope, &link)),
                )
            }
            libc::S_IFDIR => (EntryKind::Directory, None, None, None),
            libc::S_IFREG => (
                EntryKind::File,
                Some(stat.st_size as u64),
                Some(entry_version(&stat)),
                None,
            ),
            _ => (EntryKind::Other, None, None, None),
        };
        entries.push(BrowseEntry {
            name,
            kind,
            size,
            version,
            target,
            ignored: false,
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Listing { entries, complete })
}

/// Recursive matches in canonical wire path form, bounded independently of directory listings.
#[derive(Debug)]
pub struct FileMatches {
    pub paths: Vec<String>,
    /// The matches Git ignores, which only a search including ignored files can have.
    pub ignored: Vec<String>,
    pub complete: bool,
}

/// Searches every real subdirectory, including hidden entries, apart from the names
/// [`is_left_out`] leaves out. Files Git ignores, and ignored folders with all beneath them, are
/// searched only with `include_ignored`. Directory symlinks are never traversed; file symlinks are
/// included only when their targets stay in the scope.
pub fn search_files(
    scope: &Path,
    query: &str,
    include_ignored: bool,
    git: &Git,
    cancel: &AtomicBool,
) -> Result<FileMatches> {
    let started = std::time::Instant::now();
    let mut ignored = ignored::IgnoredPaths::new(git, started + budget::IGNORED_PATHS_TIMEOUT);
    ignored.read_scope(scope, cancel)?;
    search_bounded(
        scope,
        query,
        &mut ignored,
        include_ignored,
        budget::MAX_SEARCH_VISITED,
        budget::MAX_SEARCH_MATCHES,
        budget::MAX_SEARCH_PATH_BYTES,
        budget::MAX_SEARCH_DEPTH,
        budget::SEARCH_TIMEOUT,
        started,
        cancel,
    )
}

#[allow(clippy::too_many_arguments)]
fn search_bounded(
    scope: &Path,
    query: &str,
    ignored: &mut ignored::IgnoredPaths,
    include_ignored: bool,
    max_visited: usize,
    max_matches: usize,
    max_path_bytes: usize,
    max_depth: usize,
    timeout: std::time::Duration,
    started: std::time::Instant,
    cancel: &AtomicBool,
) -> Result<FileMatches> {
    let mut result = FileMatches {
        paths: Vec::new(),
        ignored: Vec::new(),
        complete: true,
    };
    let query = query.trim().to_lowercase();
    if cancel.load(Ordering::Relaxed) {
        return Err(anyhow::Error::new(Cancelled));
    }
    let root = RelPath::parse("").expect("the root is a valid path");
    let directory = open_without_links(scope, libc::O_DIRECTORY | libc::O_NOCTTY)
        .map_err(|err| open_failure(&root, &err))?;
    let mut stack = vec![(root, DirReader::new(directory)?)];
    let mut visited = 0;
    let mut path_bytes = 0;
    while let Some((parent, reader)) = stack.last_mut() {
        if cancel.load(Ordering::Relaxed) {
            return Err(anyhow::Error::new(Cancelled));
        }
        if started.elapsed() >= timeout {
            result.complete = false;
            break;
        }
        let raw = match reader.next_entry() {
            Ok(Some((raw, _))) => raw,
            Ok(None) => {
                stack.pop();
                continue;
            }
            Err(_) => {
                result.complete = false;
                stack.pop();
                continue;
            }
        };
        if visited == max_visited {
            result.complete = false;
            break;
        }
        visited += 1;
        if is_left_out(&raw) {
            continue;
        }
        let Some(stat) = reader.stat(&raw) else {
            result.complete = false;
            continue;
        };
        let path =
            parent.join(&RelPath::from_bytes(raw.clone()).expect("a directory entry is a name"));
        // An ignored folder is ignored itself, so a search leaving ignored entries out never goes
        // into it.
        let is_ignored = ignored.covers(path.as_bytes());
        if is_ignored && !include_ignored {
            continue;
        }
        let canonical = scope.join(path.as_path());
        let kind = stat.st_mode as libc::mode_t & libc::S_IFMT;
        if kind == libc::S_IFDIR {
            if stack.len() >= max_depth {
                result.complete = false;
                continue;
            }
            // Do not canonicalize this path: a directory replaced with a symlink since stat
            // must fail the same no-follow open that protects ordinary browse reads.
            match open_without_links(&canonical, libc::O_DIRECTORY | libc::O_NOCTTY)
                .and_then(DirReader::new)
            {
                Ok(reader) => {
                    // A repository of its own answers for what is below it.
                    if std::fs::symlink_metadata(canonical.join(".git")).is_ok() {
                        ignored.enter_repository(&canonical, path.as_bytes(), cancel)?;
                    }
                    stack.push((path, reader));
                }
                Err(_) => result.complete = false,
            }
            continue;
        }
        if kind != libc::S_IFREG
            && !(kind == libc::S_IFLNK
                && matches!(link_target(scope, &canonical), LinkTarget::File))
        {
            continue;
        }
        if !String::from_utf8_lossy(&raw)
            .to_lowercase()
            .contains(&query)
        {
            continue;
        }
        let wire = path.to_wire();
        let bytes = json_escaped_len(wire.as_bytes());
        if result.paths.len() == max_matches || path_bytes + bytes > max_path_bytes {
            result.complete = false;
            break;
        }
        path_bytes += bytes;
        if include_ignored && is_ignored {
            result.ignored.push(wire.clone());
        }
        result.paths.push(wire);
    }
    result.paths.sort();
    result.ignored.sort();
    Ok(result)
}

/// Whether a search leaves out an entry of this name, with all beneath it: Git's own metadata and
/// the files an operating system leaves in any folder it shows, never project content. `._` starts
/// the AppleDouble files macOS writes beside each file on a volume that cannot hold its metadata.
/// ASCII case is ignored, since the systems that write these files do not keep to one spelling
/// (Windows writes `Desktop.ini` too). The list is stated in docs/product/project-pane.md's "What
/// is listed" section.
fn is_left_out(name: &[u8]) -> bool {
    const NAMES: [&[u8]; 9] = [
        b".git",
        b".DS_Store",
        b".AppleDouble",
        b".Spotlight-V100",
        b".Trashes",
        b".fseventsd",
        b".TemporaryItems",
        b"Thumbs.db",
        b"desktop.ini",
    ];
    NAMES
        .iter()
        .any(|left_out| left_out.eq_ignore_ascii_case(name))
        || name.starts_with(b"._")
}

/// Reads the entries of an open directory through its descriptor, so what is listed is the
/// directory that was opened, whatever has happened to its path since.
struct DirReader {
    dir: std::ptr::NonNull<libc::DIR>,
}

impl DirReader {
    fn new(directory: File) -> io::Result<Self> {
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

    /// The next entry's name and `d_type`, skipping `.` and `..`; `None` at the end.
    fn next_entry(&mut self) -> io::Result<Option<(Vec<u8>, u8)>> {
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
                return Ok(Some((name, d_type)));
            }
        }
    }

    /// The entry `name`'s own metadata, not following a link it is.
    fn stat(&self, name: &[u8]) -> Option<libc::stat> {
        let name = std::ffi::CString::new(name).ok()?;
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
        (result == 0).then(|| unsafe { stat.assume_init() })
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

/// What a symbolic link inside `scope` resolves to, without following one that leads outside.
fn link_target(scope: &Path, link: &Path) -> LinkTarget {
    let Ok(target) = fs::canonicalize(link) else {
        return LinkTarget::Missing;
    };
    if !target.starts_with(scope) {
        return LinkTarget::Outside;
    }
    match fs::metadata(&target) {
        Ok(metadata) if metadata.is_file() => LinkTarget::File,
        Ok(metadata) if metadata.is_dir() => LinkTarget::Directory,
        Ok(_) => LinkTarget::Other,
        Err(_) => LinkTarget::Missing,
    }
}

/// The name `unsupported_file_type` reports for something that is not a regular file.
fn file_type_name(metadata: &Metadata) -> &'static str {
    let file_type = metadata.file_type();
    if file_type.is_dir() {
        "directory"
    } else if file_type.is_file() {
        "file"
    } else if file_type.is_fifo() {
        "fifo"
    } else if file_type.is_socket() {
        "socket"
    } else if file_type.is_block_device() || file_type.is_char_device() {
        "device"
    } else {
        "other"
    }
}

pub fn unsupported(path: &RelPath, file_type: &str) -> anyhow::Error {
    let shown = path.to_wire();
    CodedError::raised(
        error_code::UNSUPPORTED_FILE_TYPE,
        format!("{shown} is a {file_type}, which cannot be read here"),
        &[("path", &shown), ("file_type", file_type)],
    )
}

fn outside(path: &RelPath) -> anyhow::Error {
    let shown = path.to_wire();
    CodedError::raised(
        error_code::OUTSIDE_SCOPE,
        format!("{shown} leads outside the project"),
        &[("path", &shown)],
    )
}

pub(super) fn changed(path: &RelPath) -> anyhow::Error {
    let shown = path.to_wire();
    CodedError::raised(
        error_code::SOURCE_CHANGED,
        format!("{shown} changed while it was being read"),
        &[("path", &shown)],
    )
}

pub fn too_large(limit: usize, size: Option<u64>) -> anyhow::Error {
    let max = limit.to_string();
    let mut params = vec![("limit", "file_bytes"), ("max", max.as_str())];
    let size = size.map(|size| size.to_string());
    if let Some(size) = &size {
        params.push(("size", size));
    }
    CodedError::raised(
        error_code::LIMIT_EXCEEDED,
        format!("the file is larger than the {limit}-byte limit"),
        &params,
    )
}

/// A failure to open a path that was just resolved with no link in it: a link found there now
/// means one was swapped in since.
fn open_failure(path: &RelPath, err: &io::Error) -> anyhow::Error {
    match err.raw_os_error() {
        Some(libc::ELOOP) => changed(path),
        _ => io_failure(path, err),
    }
}

/// An I/O failure on a path inside the scope, as the code a client words it by. Nothing at the
/// path — including a link that leads nowhere or round in a loop — is `file_not_found`; a failure
/// with no meaning of its own here is an internal error with the system's message.
fn io_failure(path: &RelPath, err: &io::Error) -> anyhow::Error {
    let shown = path.to_wire();
    if crate::hostfs::is_not_found(err) {
        return CodedError::raised(
            error_code::FILE_NOT_FOUND,
            format!("{shown} does not exist"),
            &[("path", &shown)],
        );
    }
    if err.kind() == io::ErrorKind::PermissionDenied {
        return CodedError::raised(
            error_code::PERMISSION_DENIED,
            format!("{shown} could not be read: {err}"),
            &[("path", &shown), ("detail", &err.to_string())],
        );
    }
    if err.raw_os_error() == Some(libc::ENAMETOOLONG) {
        return CodedError::raised(
            error_code::INVALID_PATH,
            format!("{shown} is too long a path"),
            &[("path", &shown)],
        );
    }
    anyhow::anyhow!("{err}").context(format!("reading {shown}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;

    fn never() -> AtomicBool {
        AtomicBool::new(false)
    }

    /// A search over a `git` that cannot be run, in scopes that hold no repository: nothing is
    /// ignored, and including ignored files only keeps the search from leaving anything out.
    fn search_files(scope: &Path, query: &str, cancel: &AtomicBool) -> Result<FileMatches> {
        let git = Git::new(&|| anyhow::bail!("no git in this test"));
        super::search_files(scope, query, true, &git, cancel)
    }

    fn code(err: anyhow::Error) -> crate::protocol::Code {
        err.downcast_ref::<CodedError>()
            .expect("a coded refusal")
            .code
    }

    #[test]
    fn search_finds_unopened_nested_hidden_and_ignored_files_without_following_directories() {
        use std::os::unix::fs::symlink;
        let scope = ScratchDir::new("file-search");
        let outside = ScratchDir::new("file-search-outside");
        let scope = fs::canonicalize(&*scope).unwrap();
        fs::create_dir_all(scope.join("unopened/deeper")).unwrap();
        fs::create_dir_all(scope.join("ignored")).unwrap();
        fs::create_dir_all(scope.join("unopened/.git")).unwrap();
        fs::write(scope.join(".gitignore"), b"ignored/").unwrap();
        for path in [
            "unopened/deeper/Needle.txt",
            ".needle",
            "ignored/needle.txt",
            "unopened/.git/needle",
            "needle%name.txt",
        ] {
            fs::write(scope.join(path), b"match").unwrap();
        }
        fs::write(outside.join("needle.txt"), b"outside").unwrap();
        symlink("unopened", scope.join("needle-dir")).unwrap();
        symlink(".", scope.join("loop")).unwrap();
        symlink("unopened/deeper/Needle.txt", scope.join("needle-link")).unwrap();
        symlink(outside.join("needle.txt"), scope.join("needle-outside")).unwrap();
        let found = search_files(&scope, "  NEEDLE  ", &never()).unwrap();
        assert!(found.complete);
        assert_eq!(
            found.paths,
            [
                ".needle",
                "ignored/needle.txt",
                "needle%25name.txt",
                "needle-link",
                "unopened/deeper/Needle.txt"
            ]
        );
        assert!(search_files(&scope, "unopened", &never())
            .unwrap()
            .paths
            .is_empty());
        let all = search_files(&scope, "", &never()).unwrap();
        assert!(all.complete);
        let mut expected = found.paths;
        expected.push(".gitignore".into());
        expected.sort();
        assert_eq!(all.paths, expected);
    }

    #[test]
    fn search_leaves_out_the_files_an_operating_system_leaves_in_a_folder() {
        let scope = ScratchDir::new("file-search-system");
        let scope = fs::canonicalize(&*scope).unwrap();
        fs::create_dir_all(scope.join("src/.Spotlight-V100")).unwrap();
        for path in [
            "src/.DS_Store",
            "src/._notes.md",
            "src/Thumbs.db",
            "src/Desktop.ini",
            ".ds_store",
            "src/.Spotlight-V100/store.db",
            "src/notes.md",
            ".env",
        ] {
            fs::write(scope.join(path), b"").unwrap();
        }
        let found = search_files(&scope, "", &never()).unwrap();
        assert!(found.complete);
        assert_eq!(found.paths, [".env", "src/notes.md"]);
    }

    #[test]
    fn search_reports_visit_result_path_depth_and_time_limits_and_cancellation() {
        let scope = ScratchDir::new("file-search-limits");
        let scope = fs::canonicalize(&*scope).unwrap();
        fs::create_dir_all(scope.join("nested")).unwrap();
        fs::write(scope.join("needle.txt"), b"match").unwrap();
        fs::write(scope.join("nested/needle.txt"), b"match").unwrap();
        let git = Git::new(&|| anyhow::bail!("no git in this test"));
        for (visited, matches, bytes, depth, timeout) in [
            (0, 10, 1024, 10, std::time::Duration::from_secs(5)),
            (100, 1, 1024, 10, std::time::Duration::from_secs(5)),
            (100, 10, 1, 10, std::time::Duration::from_secs(5)),
            (100, 10, 1024, 1, std::time::Duration::from_secs(5)),
            (100, 10, 1024, 10, std::time::Duration::ZERO),
        ] {
            let found = search_bounded(
                &scope,
                "needle",
                &mut ignored::IgnoredPaths::new(&git, std::time::Instant::now()),
                true,
                visited,
                matches,
                bytes,
                depth,
                timeout,
                std::time::Instant::now(),
                &never(),
            )
            .unwrap();
            assert!(!found.complete);
            assert!(found.paths.len() <= matches);
        }
        assert!(search_files(&scope, "needle", &AtomicBool::new(true))
            .unwrap_err()
            .downcast_ref::<Cancelled>()
            .is_some());
        let empty = search_files(&scope, "  ", &never()).unwrap();
        assert!(empty.complete);
        assert_eq!(empty.paths, ["needle.txt", "nested/needle.txt"]);
        let limited = search_bounded(
            &scope,
            "",
            &mut ignored::IgnoredPaths::new(&git, std::time::Instant::now()),
            true,
            100,
            1,
            1024,
            10,
            std::time::Duration::from_secs(5),
            std::time::Instant::now(),
            &never(),
        )
        .unwrap();
        assert!(!limited.complete);
        assert_eq!(limited.paths.len(), 1);
    }

    /// Whether this Linux system refuses `openat2`, so that an open falls back to holding only the
    /// last component (see the module doc).
    #[cfg(target_os = "linux")]
    fn openat2_is_refused() -> bool {
        // SAFETY: `open_how` is plain integers, for which all zeroes is a valid value.
        let how: libc::open_how = unsafe { std::mem::zeroed() };
        // SAFETY: `openat2` reads `how` (of the size passed) and the NUL-terminated path, both
        // alive for the call.
        let fd = unsafe {
            libc::syscall(
                libc::SYS_openat2,
                libc::AT_FDCWD,
                c".".as_ptr(),
                &how as *const libc::open_how,
                std::mem::size_of::<libc::open_how>(),
            )
        };
        if fd >= 0 {
            // SAFETY: the descriptor was just opened here and is used nowhere else.
            unsafe { libc::close(fd as libc::c_int) };
            return false;
        }
        matches!(
            io::Error::last_os_error().raw_os_error(),
            Some(libc::ENOSYS | libc::EPERM)
        )
    }

    /// A directory on the resolved path replaced by a link to elsewhere between the resolution
    /// and the open: the read and the listing refuse, rather than follow it.
    #[test]
    fn a_link_swapped_in_after_resolution_is_never_followed() {
        #[cfg(target_os = "linux")]
        if openat2_is_refused() {
            eprintln!("skipped: openat2 is refused, so only the last component is held");
            return;
        }
        let scope = ScratchDir::new("live-swap");
        let scope = fs::canonicalize(&*scope).unwrap();
        let outside = ScratchDir::new("live-swap-outside");
        fs::create_dir(outside.join("sub")).unwrap();
        fs::write(outside.join("file.txt"), b"outside").unwrap();
        fs::create_dir_all(scope.join("dir/sub")).unwrap();
        fs::write(scope.join("dir/file.txt"), b"inside").unwrap();
        let file = RelPath::parse("dir/file.txt").unwrap();
        let sub = RelPath::parse("dir/sub").unwrap();
        let resolved_file = resolve(&scope, &file).unwrap();
        let resolved_sub = resolve(&scope, &sub).unwrap();

        fs::rename(scope.join("dir"), scope.join("moved")).unwrap();
        std::os::unix::fs::symlink(&*outside, scope.join("dir")).unwrap();

        let read = read_resolved(&resolved_file, &file, 1024, &never()).unwrap_err();
        assert_eq!(code(read), error_code::SOURCE_CHANGED);
        let listed = list_resolved(&scope, &resolved_sub, &sub, 10, 1024, &never()).unwrap_err();
        assert_eq!(code(listed), error_code::SOURCE_CHANGED);
    }
}
