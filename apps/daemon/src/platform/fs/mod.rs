//! File system facts and primitives that differ by host: what tells a file apart, how a file is
//! opened without a link in its path or without waiting, who may read what the daemon writes,
//! where home and scratch space are, how an alias to a file or directory is made, and the
//! exclusive file lock that keeps one daemon to a data directory and guards other programs' files,
//! with the shared lock that looks whether it is held.
//!
//! Policy stays with the callers: what a browse scope is, which paths are reachable, what a
//! listing reports. This module only answers what the host does when asked.

use std::fs::{File, Metadata, OpenOptions, TryLockError};
use std::io;
use std::path::Path;

#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub use unix::*;

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::*;

#[cfg(test)]
mod contract_tests;

/// What tells a file or directory apart on the file system, whatever path it is reached by: on
/// Unix its device and inode, on Windows its volume serial number and file index. Opaque: compare
/// it, hash it, or ask for its [`token`](Self::token).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Identity {
    volume: u64,
    file: u64,
}

impl Identity {
    /// Whether the file system reported a file number: a file system that has none gives 0, and
    /// then the identity tells nothing apart from the other files of its volume.
    pub fn is_known(&self) -> bool {
        self.file != 0
    }

    /// The identity as text that never changes for the same file and differs for another: the
    /// form it is sent in, and the start of a version tag.
    pub fn token(&self) -> String {
        format!("{}:{}", self.volume, self.file)
    }
}

/// Whether a lookup follows a link at the end of the path or looks at the link itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Links {
    Follow,
    Keep,
}

/// A file's metadata together with its [`Identity`], read at one moment from one handle on the
/// file system, so the two never describe two different files.
pub struct Stat {
    pub metadata: Metadata,
    pub identity: Identity,
    /// When the file's contents or attributes last changed, which Windows keeps apart from the
    /// metadata std reads; a file's version needs it.
    #[cfg(windows)]
    changed: i64,
}

/// [`Stat`] of the file at `path`, following a link there or not as `links` says.
pub fn stat(path: &Path, links: Links) -> io::Result<Stat> {
    host_stat(path, links)
}

/// [`Stat`] of the open `file`, the file itself whatever its path has become.
pub fn stat_of(file: &File) -> io::Result<Stat> {
    host_stat_of(file)
}

/// The [`Identity`] of what `path` leads to, or of the link itself with [`Links::Keep`].
pub fn identity(path: &Path, links: Links) -> io::Result<Identity> {
    stat(path, links).map(|stat| stat.identity)
}

/// The identity of the directory at `path`; `None` where it cannot be read, is not a directory, or
/// the file system reports no file number ([`Identity::is_known`]).
pub fn directory_identity(path: &Path, links: Links) -> Option<Identity> {
    let stat = stat(path, links).ok()?;
    (stat.metadata.is_dir() && stat.identity.is_known()).then_some(stat.identity)
}

/// What a file turns out to be, as far as listing a directory tells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    windows,
    allow(
        dead_code,
        reason = "a Windows directory holds no FIFO, socket or device"
    )
)]
pub enum EntryType {
    Link,
    Directory,
    File,
    Other,
}

/// An entry read from a directory: its name as bytes (in the form of `names::name_bytes`), and
/// its type where reading the directory already told it.
pub struct Listed {
    pub name: Vec<u8>,
    pub hint: Option<EntryType>,
}

/// What a client opens a file for: a regular file it reads, or a directory it lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opens {
    File,
    Directory,
}

/// Opens `below` inside `scope` for reading, refusing a link in any component of the path: a link
/// found makes the open fail with an error [`is_symlink_loop`] answers true to, and a link swapped
/// in after the caller looked at the path is refused the same way, so nothing is ever read from
/// outside `scope` through one. A link in `scope` itself is refused too, so the caller passes it
/// canonical. An empty `below` opens `scope` itself. A file is opened without
/// waiting for a writer, so a FIFO swapped in for it cannot block the open; the caller checks the
/// type of what it got. Where the host cannot refuse a link in every component, it refuses one
/// in the last.
pub fn open_without_links(scope: &Path, below: &Path, opens: Opens) -> io::Result<File> {
    host_open_without_links(scope, below, opens)
}

/// Opens `path` for reading, following links, without blocking, and refuses anything but a regular
/// file: a FIFO's open would otherwise wait for a writer, and a device reads forever.
pub fn open_regular(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    no_wait(&mut options);
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }
    Ok(file)
}

/// Opens `path` to read and append to, without blocking, refusing anything but a regular file.
pub fn open_regular_to_append(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).append(true);
    no_wait(&mut options);
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("`{}` is not a regular file", path.display()),
        ));
    }
    Ok(file)
}

/// The permissions [`create_replacement`] gives the file it creates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Privacy {
    /// The owner's alone, whatever the file had: for content that can hold secrets.
    Private,
    /// The permissions of the file being replaced, and the owner's alone for a new one.
    KeptOrPrivate,
    /// The permissions of the file being replaced, and for a new one what the process's umask
    /// leaves of the usual file permissions, as for any file a program creates. Unlike the
    /// others, a failure to give the file the kept permissions is ignored: a file that cannot be
    /// given them is still written.
    KeptOrDefault,
}

/// How a link made by [`alias_dir`] or [`alias_file`] was made: a symbolic link, a directory
/// junction, or a hard link.
#[allow(
    dead_code,
    reason = "the junction and hard-link kinds exist only where the host cannot make a symbolic link"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AliasKind {
    Symlink,
    Junction,
    HardLink,
}

/// Why [`try_lock_file`] or [`try_lock_file_shared`] did not give the lock.
#[derive(Debug)]
pub enum Busy {
    /// Another holder has it.
    Held,
    /// The lock file could not be opened.
    Open(io::Error),
    /// The host failed to take the lock for a reason other than another holder.
    Lock(io::Error),
}

/// Opens the lock file at `path`, creating it, and takes an exclusive lock on it without waiting:
/// the daemon's own instance lock, and the lock files other programs keep beside their files.
/// The lock lasts as long as the returned file is held: the host releases it when the file is
/// closed or the process exits, so a crashed holder leaves no stale lock behind. The file is not
/// truncated, since another program may keep the lock file for its own use too.
pub fn try_lock_file(path: &Path) -> Result<File, Busy> {
    lock_file(path, true)
}

/// [`try_lock_file`] with a shared lock: refused while an exclusive lock is held, and taken
/// alongside other shared ones. What looks whether a lock is held takes this, so that two who look
/// at once do not each find the other holding it.
pub fn try_lock_file_shared(path: &Path) -> Result<File, Busy> {
    lock_file(path, false)
}

fn lock_file(path: &Path, exclusive: bool) -> Result<File, Busy> {
    // Read access too: Windows locks only through a handle that may read or write the file's
    // data, and appending alone is neither.
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .append(true)
        .open(path)
        .map_err(Busy::Open)?;
    let locked = if exclusive {
        file.try_lock()
    } else {
        file.try_lock_shared()
    };
    match locked {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(Busy::Held),
        Err(TryLockError::Error(err)) => Err(Busy::Lock(err)),
    }
}

/// The system Trash as this host reaches it. On macOS the move goes through `NSFileManager`: the
/// default, Finder through AppleScript, needs an Automation permission the hardened app does not
/// hold.
pub fn trash_context() -> trash::TrashContext {
    #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
    let mut context = trash::TrashContext::default();
    #[cfg(target_os = "macos")]
    {
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        context.set_delete_method(DeleteMethod::NsFileManager);
    }
    context
}
