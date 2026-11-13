//! The Windows file system: identity and versions from a handle's file information, the safe open
//! as an open that refuses a link at the end of the path and checks where the handle ended up,
//! privacy through the data directory's access control list, aliases as junctions and symbolic
//! links, and the directory listing read through the directory's own handle.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::AsRawHandle;
use std::path::{Component, Path, PathBuf, Prefix};
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, ERROR_CANT_RESOLVE_FILENAME, ERROR_DIRECTORY,
    ERROR_FILENAME_EXCED_RANGE, ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_FILES,
    ERROR_STOPPED_ON_SYMLINK, ERROR_SUCCESS, GENERIC_ALL, GENERIC_WRITE, HANDLE,
};
use windows_sys::Win32::Security::Authorization::{
    GetSecurityInfo, SetNamedSecurityInfoW, SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::PSECURITY_DESCRIPTOR;
use windows_sys::Win32::Security::{
    AddAccessAllowedAceEx, CreateWellKnownSid, EqualSid, GetAce, GetLengthSid, GetTokenInformation,
    InitializeAcl, TokenUser, WinBuiltinAdministratorsSid, WinLocalSystemSid, ACCESS_ALLOWED_ACE,
    ACE_HEADER, ACL, ACL_REVISION, CONTAINER_INHERIT_ACE, DACL_SECURITY_INFORMATION,
    OBJECT_INHERIT_ACE, OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, PSID,
    SECURITY_MAX_SID_SIZE, TOKEN_QUERY, TOKEN_USER, WELL_KNOWN_SID_TYPE,
};
use windows_sys::Win32::Storage::FileSystem::{
    FileAttributeTagInfo, FileBasicInfo, FileIdBothDirectoryInfo, GetFileInformationByHandle,
    GetFileInformationByHandleEx, GetFinalPathNameByHandleW, GetVolumePathNameW,
    BY_HANDLE_FILE_INFORMATION, DELETE, FILE_ALL_ACCESS, FILE_APPEND_DATA,
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_TAG_INFO,
    FILE_BASIC_INFO, FILE_DELETE_CHILD, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_ID_BOTH_DIR_INFO, FILE_INFO_BY_HANDLE_CLASS, FILE_NAME_NORMALIZED, FILE_READ_ATTRIBUTES,
    FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA, FILE_WRITE_EA, READ_CONTROL, VOLUME_NAME_DOS,
    WRITE_DAC, WRITE_OWNER,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::UI::Shell::GetUserProfileDirectoryW;

use super::{AliasKind, EntryType, Identity, Links, Listed, Opens, Privacy, Stat};
use crate::platform::names;

/// `FSCTL_SET_REPARSE_POINT` (winioctl.h), which makes a directory a junction.
const FSCTL_SET_REPARSE_POINT: u32 = 0x0009_00A4;
/// `IO_REPARSE_TAG_MOUNT_POINT` (winnt.h), the tag of a directory junction.
const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;
/// `IO_REPARSE_TAG_APPEXECLINK` (winnt.h), the tag of an app execution alias.
const IO_REPARSE_TAG_APPEXECLINK: u32 = 0x8000_001B;

/// Whether a reparse point with `tag` stands for another name, as a symbolic link and a junction
/// do (`IsReparseTagNameSurrogate` in winnt.h). Other reparse points — a cloud file placeholder, a
/// deduplicated file — are the file itself to whoever reads it.
fn is_name_surrogate(tag: u32) -> bool {
    tag & 0x2000_0000 != 0
}

/// `path` as a NUL-terminated wide string.
fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn last_error() -> io::Error {
    io::Error::last_os_error()
}

/// Reads the information of class `class` about the open `file` into a `T`.
fn information<T: Default>(file: &File, class: FILE_INFO_BY_HANDLE_CLASS) -> io::Result<T> {
    let mut info = T::default();
    // SAFETY: `info` is a `T` the class writes, of the size passed, alive for the call; the handle
    // is open for as long as `file` is.
    let read = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            class,
            (&mut info as *mut T).cast(),
            std::mem::size_of::<T>() as u32,
        )
    };
    if read == 0 {
        return Err(last_error());
    }
    Ok(info)
}

/// The volume serial number and file index of the open `file`.
fn file_identity(file: &File) -> io::Result<Identity> {
    // SAFETY: all zeroes is a valid value of this plain struct, which the call fills in.
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: the handle is open for as long as `file` is, and `info` outlives the call.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(last_error());
    }
    Ok(Identity {
        volume: u64::from(info.dwVolumeSerialNumber),
        file: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    })
}

/// Opens `path` to read its attributes only, which neither the sharing of a file another process
/// holds open nor its permissions to read the contents stand in the way of. A directory opens too,
/// and with [`Links::Keep`] a link at the end of the path is opened itself.
fn open_for_attributes(path: &Path, links: Links) -> io::Result<File> {
    let keep = match links {
        Links::Follow => 0,
        Links::Keep => FILE_FLAG_OPEN_REPARSE_POINT,
    };
    OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | keep)
        .open(path)
}

pub(super) fn host_stat(path: &Path, links: Links) -> io::Result<Stat> {
    host_stat_of(&open_for_attributes(path, links)?)
}

pub(super) fn host_stat_of(file: &File) -> io::Result<Stat> {
    let basic: FILE_BASIC_INFO = information(file, FileBasicInfo)?;
    Ok(Stat {
        metadata: file.metadata()?,
        identity: file_identity(file)?,
        changed: basic.ChangeTime,
    })
}

/// A regular file's version: its volume and file index, size, and last-write and change times,
/// as one opaque string. The change time moves with every write and every change of the file's
/// attributes, so two equal versions mean nothing about the file changed in between. A directory
/// listing reports the same fields for the same file, so [`version`] and [`EntryStat::version`]
/// agree — while nothing writes to it: NTFS brings a directory's copy of a file's size and times up
/// to date lazily, so a file open for writing can be listed with an older version than a read
/// reports. The times tick with the system clock's update interval (about 1 to 16 ms), so two
/// writes of the same size within one tick leave the version as it was.
fn version_of(volume: u64, file: u64, size: u64, written: i64, changed: i64) -> String {
    format!("{volume}:{file}:{size}:{written}:{changed}")
}

/// The version of the file `stat` describes (see [`EntryStat::version`] for the same file seen
/// through a directory listing).
pub fn version(stat: &Stat) -> String {
    version_of(
        stat.identity.volume,
        stat.identity.file,
        stat.metadata.len(),
        stat.metadata.last_write_time() as i64,
        stat.changed,
    )
}

/// Windows has no FIFO, socket or device among the files of a directory.
pub fn special_file_name(_metadata: &std::fs::Metadata) -> Option<&'static str> {
    None
}

/// An open on Windows never waits for a writer: a named pipe is not a file of a directory. A
/// directory opens too (only with the backup-semantics flag), so that the caller's check refuses
/// it as no regular file, as on Unix, rather than the open failing as access denied.
pub(super) fn no_wait(options: &mut OpenOptions) {
    options.custom_flags(FILE_FLAG_BACKUP_SEMANTICS);
}

/// The error a link found on the way to a file is refused with.
fn link_refused() -> io::Error {
    io::Error::from_raw_os_error(ERROR_STOPPED_ON_SYMLINK as i32)
}

/// Opens `path` for reading; a directory opens too. With `keep_last`, a link or other reparse point
/// at the end of the path is opened itself rather than followed.
fn open_to_read(path: &Path, keep_last: bool) -> io::Result<File> {
    let keep = if keep_last {
        FILE_FLAG_OPEN_REPARSE_POINT
    } else {
        0
    };
    OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | keep)
        .open(path)
}

/// The reparse tag of the open `file`, or `None` where it is no reparse point.
fn reparse_tag(file: &File) -> io::Result<Option<u32>> {
    let info: FILE_ATTRIBUTE_TAG_INFO = information(file, FileAttributeTagInfo)?;
    Ok((info.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0).then_some(info.ReparseTag))
}

/// Whether `path` is an app execution alias: the file under `WindowsApps` by which a packaged app
/// (one installed from the Store or by `winget`, such as PowerShell 7) is started, which
/// `CreateProcessW` starts as the package's program. Its reparse tag is what makes it one,
/// wherever it is; the file cannot be read. False where `path` cannot be opened.
pub fn is_app_execution_alias(path: &Path) -> bool {
    open_for_attributes(path, Links::Keep)
        .and_then(|file| reparse_tag(&file))
        .is_ok_and(|tag| tag == Some(IO_REPARSE_TAG_APPEXECLINK))
}

/// The path the open `file` was reached by, every link on the way resolved, in the plain form
/// [`names::canonical`] gives.
fn final_path(file: &File) -> io::Result<PathBuf> {
    let mut buffer = vec![0u16; 512];
    loop {
        // SAFETY: the buffer holds `buffer.len()` units and outlives the call; the handle is open.
        let length = unsafe {
            GetFinalPathNameByHandleW(
                file.as_raw_handle(),
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
            )
        } as usize;
        if length == 0 {
            return Err(last_error());
        }
        // Too small a buffer is answered with the size it needs, the NUL included.
        if length >= buffer.len() {
            buffer.resize(length + 1, 0);
            continue;
        }
        let found = PathBuf::from(OsString::from_wide(&buffer[..length]));
        return Ok(names::plain_form(found));
    }
}

/// Opens `scope` joined with `below` with no link in any component. A link at the end is refused
/// from the opened handle itself, which the open did not follow; a link in a component above it was
/// followed by the open, and is found by where the handle ended up: the path it was reached by must
/// be the path asked for, compared as a lexical key (separators and case). That holds `scope`
/// to the same rule, so it must be canonical, as every caller's is: then an empty `below` checks
/// every component of the scope itself, and a link swapped into any of them after the caller
/// resolved the path makes the open fail rather than read through it.
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
    let mut file = open_to_read(&path, true)?;
    match reparse_tag(&file)? {
        Some(tag) if is_name_surrogate(tag) => return Err(link_refused()),
        // A reparse point that is the file itself (a cloud placeholder) is read through its
        // filter; the path check below still holds the second open to the same rule.
        Some(_) => file = open_to_read(&path, false)?,
        None => {}
    }
    let asked = names::plain_form(path);
    if names::lexical_key(&final_path(&file)?) != names::lexical_key(&asked) {
        return Err(link_refused());
    }
    if opens == Opens::Directory && !file.metadata()?.is_dir() {
        return Err(io::Error::from_raw_os_error(ERROR_DIRECTORY as i32));
    }
    Ok(file)
}

/// Whether `err` is the refusal of a link on the way to a file, or links that lead round in a loop.
pub fn is_symlink_loop(err: &io::Error) -> bool {
    matches!(
        err.raw_os_error().map(|code| code as u32),
        Some(ERROR_STOPPED_ON_SYMLINK | ERROR_CANT_RESOLVE_FILENAME)
    )
}

/// Whether `err` says a file lies where a directory should be.
pub fn is_not_a_directory(err: &io::Error) -> bool {
    err.raw_os_error() == Some(ERROR_DIRECTORY as i32)
}

/// Whether `err` says a name or a path is longer than the host takes.
pub fn is_name_too_long(err: &io::Error) -> bool {
    err.raw_os_error() == Some(ERROR_FILENAME_EXCED_RANGE as i32)
}

/// Opens `staged` for writing, creating or truncating it. Windows keeps who may read a file in its
/// access control list, which a new file inherits from its directory: every directory the daemon
/// writes private files to is under the data directory, made private once at start
/// ([`keep_private_below`]), so `privacy` asks nothing more of the file itself.
pub fn create_replacement(staged: &Path, _target: &Path, _privacy: Privacy) -> io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(staged)
}

/// Opens `path` with `options`. Privacy is inherited from the directory (see
/// [`create_replacement`]).
pub fn create_private(path: &Path, options: &mut OpenOptions) -> io::Result<File> {
    options.open(path)
}

/// Creates the directory `dir`, and any missing parent; privacy is inherited from the directory
/// above (see [`create_replacement`]).
pub fn create_private_dir(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)
}

/// A process token's handle, closed on drop.
struct Token(HANDLE);

impl Token {
    fn of_this_process() -> io::Result<Self> {
        let mut token: HANDLE = std::ptr::null_mut();
        // SAFETY: the pseudo-handle of this process needs no closing; `token` outlives the call.
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return Err(last_error());
        }
        Ok(Self(token))
    }

    /// The security identifier of the user the token is for.
    fn user_sid(&self) -> io::Result<Vec<u8>> {
        let mut length = 0u32;
        // SAFETY: a call with no buffer only reports the length needed.
        unsafe { GetTokenInformation(self.0, TokenUser, std::ptr::null_mut(), 0, &mut length) };
        // SAFETY: `GetLastError` only reads this thread's last error.
        if unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
            return Err(last_error());
        }
        // `u64`s, so the `TOKEN_USER` read from the start of it is aligned.
        let mut buffer = vec![0u64; (length as usize).div_ceil(8)];
        // SAFETY: the buffer holds `length` bytes and outlives the call.
        let read = unsafe {
            GetTokenInformation(
                self.0,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                length,
                &mut length,
            )
        };
        if read == 0 {
            return Err(last_error());
        }
        // SAFETY: the call wrote a `TOKEN_USER` at the start of the aligned buffer, whose SID
        // points into the buffer, alive while it is copied out.
        unsafe {
            let user = &*(buffer.as_ptr() as *const TOKEN_USER);
            let sid = user.User.Sid;
            let sid_length = GetLengthSid(sid) as usize;
            Ok(std::slice::from_raw_parts(sid as *const u8, sid_length).to_vec())
        }
    }
}

impl Drop for Token {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by `of_this_process` and is closed only here.
        unsafe { CloseHandle(self.0) };
    }
}

/// The security identifier of the operating system itself (`LocalSystem`).
fn system_sid() -> io::Result<Vec<u8>> {
    well_known_sid(WinLocalSystemSid)
}

/// The security identifier of the well-known group or account `kind`.
fn well_known_sid(kind: WELL_KNOWN_SID_TYPE) -> io::Result<Vec<u8>> {
    let mut sid = vec![0u8; SECURITY_MAX_SID_SIZE as usize];
    let mut length = sid.len() as u32;
    // SAFETY: the buffer holds `length` bytes and outlives the call.
    let made = unsafe {
        CreateWellKnownSid(
            kind,
            std::ptr::null_mut(),
            sid.as_mut_ptr().cast(),
            &mut length,
        )
    };
    if made == 0 {
        return Err(last_error());
    }
    sid.truncate(length as usize);
    Ok(sid)
}

/// Makes the directory `dir` the owner's alone, together with everything in it and everything
/// created in it later: its access control list is replaced by one that grants full access to the
/// user the daemon runs as and to the system, inherited by files and directories below it, and is
/// protected, so nothing inherited from above widens it. What was below already is given the
/// inherited entries too.
pub fn restrict_to_owner(dir: &Path) -> io::Result<()> {
    let user = Token::of_this_process()?.user_sid()?;
    let system = system_sid()?;
    // An ACL header, then for each entry its header and mask (8 bytes) and its SID, kept to whole
    // `u32`s as the ACL's size must be.
    let size = 8 + [&user, &system]
        .iter()
        .map(|sid| 8 + sid.len())
        .sum::<usize>();
    let mut acl = vec![0u32; size.div_ceil(4)];
    let acl_ptr = acl.as_mut_ptr() as *mut ACL;
    // SAFETY: the buffer holds `size` bytes, aligned for an ACL, and outlives every call on it.
    if unsafe { InitializeAcl(acl_ptr, (acl.len() * 4) as u32, ACL_REVISION) } == 0 {
        return Err(last_error());
    }
    for sid in [&user, &system] {
        // SAFETY: the ACL was initialised above with room for both entries; the SID is a valid
        // one, alive for the call.
        let added = unsafe {
            AddAccessAllowedAceEx(
                acl_ptr,
                ACL_REVISION,
                OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE,
                FILE_ALL_ACCESS,
                sid.as_ptr() as *mut _,
            )
        };
        if added == 0 {
            return Err(last_error());
        }
    }
    let name = wide(dir);
    // SAFETY: the name is NUL-terminated and the ACL valid, both alive for the call.
    let status = unsafe {
        SetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            acl_ptr,
            std::ptr::null(),
        )
    };
    if status != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    Ok(())
}

/// Makes everything below the data directory `dir` private to the user: Windows inherits who may
/// read a file from its directory, so the directory is restricted once ([`restrict_to_owner`]).
pub fn keep_private_below(dir: &Path) -> io::Result<()> {
    restrict_to_owner(dir)
}

/// The contents of the file at `path`. A file takes its permissions from the data directory's
/// access control list here ([`keep_private_below`]), so there is nothing of its own to judge.
pub fn read_private_file(path: &Path) -> io::Result<Option<Vec<u8>>> {
    std::fs::read(path).map(Some)
}

/// Creates `staged`, which must not exist, to be renamed over `target`; a file already at `staged`
/// is refused. A file takes its permissions from its directory here.
pub fn create_new_like(staged: &Path, _target: &Path) -> io::Result<File> {
    OpenOptions::new().write(true).create_new(true).open(staged)
}

/// Whether the directory `dir` takes new files: it is not marked read-only.
pub fn directory_writable(dir: &Path) -> bool {
    std::fs::metadata(dir)
        .is_ok_and(|metadata| metadata.is_dir() && !metadata.permissions().readonly())
}

/// A directory's entries cannot be flushed through a handle on Windows; the rename is left to the
/// file system's own journal.
pub fn sync_directory(_dir: &Path) -> io::Result<()> {
    Ok(())
}

/// Whether `a` and `b` lead to the same file: their resolved paths are equal.
pub fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Whether `path` leads to a regular file the system would run: one whose extension is among those
/// `PATHEXT` lists (Windows runs a file by its extension, not by a permission).
pub fn is_executable(path: &Path) -> bool {
    let is_file = std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file());
    let Some(extension) = path.extension().and_then(OsStr::to_str) else {
        return false;
    };
    let listed = std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD;.VBS;.VBE;.JS;.JSE;.WSF;.WSH;.MSC".to_string());
    is_file
        && listed
            .split(';')
            .filter_map(|listed| listed.strip_prefix('.'))
            .any(|listed| listed.eq_ignore_ascii_case(extension))
}

/// The directories the host keeps scratch files in, as written: the process's temporary directory
/// (`%TEMP%`) alone, since Windows has no system-wide one.
pub fn temp_roots() -> Vec<PathBuf> {
    vec![std::env::temp_dir()]
}

/// Whether `path` leads to a file git would run as a hook: any regular file, since Git for Windows
/// runs a hook by the interpreter its first line names, whatever its name or attributes.
pub fn is_hook_script(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
}

/// Sets the modification time of the directory `dir`. A directory opens only with the
/// backup-semantics flag, and the time is set through the right to write its attributes alone,
/// which the read-only attribute does not bar on a directory.
pub fn set_dir_modified(dir: &Path, time: std::time::SystemTime) -> io::Result<()> {
    OpenOptions::new()
        .access_mode(FILE_WRITE_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(dir)?
        .set_modified(time)
}

/// Whether the entry at `path` itself (a link is not followed) is owned by the user the daemon runs
/// as and grants no write access to anyone else. The system and the administrators count as the
/// user's own: they can take the entry whatever its list says, and a daemon started elevated
/// creates entries owned by the administrators group. An entry whose list cannot be read, or that
/// has no list (everyone may do everything), is not private.
pub fn is_private_to_user(path: &Path) -> bool {
    private_to_user(path).unwrap_or(false)
}

/// `ACCESS_ALLOWED_ACE_TYPE` (winnt.h): the kind of list entry that grants rights.
const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;

/// The access rights that change an entry or what it holds: writing, appending, deleting, and
/// changing the list or the owner, in their specific and generic forms.
const WRITE_RIGHTS: u32 = FILE_WRITE_DATA
    | FILE_APPEND_DATA
    | FILE_WRITE_EA
    | FILE_DELETE_CHILD
    | FILE_WRITE_ATTRIBUTES
    | DELETE
    | WRITE_DAC
    | WRITE_OWNER
    | GENERIC_WRITE
    | GENERIC_ALL;

fn private_to_user(path: &Path) -> io::Result<bool> {
    let entry = OpenOptions::new()
        .access_mode(READ_CONTROL)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let mut owner: PSID = std::ptr::null_mut();
    let mut dacl: *mut ACL = std::ptr::null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: the handle is open with `READ_CONTROL`; the outputs are valid pointers, and what
    // they point into is the descriptor, freed below after its last use.
    let status = unsafe {
        GetSecurityInfo(
            entry.as_raw_handle(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            &mut dacl,
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    let verdict = (|| {
        let user = Token::of_this_process()?.user_sid()?;
        let system = system_sid()?;
        let administrators = well_known_sid(WinBuiltinAdministratorsSid)?;
        let own = [user, system, administrators];
        let is_own = |sid: PSID| {
            // SAFETY: both are valid SIDs, alive for the call.
            own.iter()
                .any(|own| unsafe { EqualSid(own.as_ptr() as PSID, sid) } != 0)
        };
        if owner.is_null() || dacl.is_null() || !is_own(owner) {
            return Ok(false);
        }
        // SAFETY: `dacl` points into the descriptor, alive here.
        let count = unsafe { (*dacl).AceCount };
        for index in 0..u32::from(count) {
            let mut ace: *mut core::ffi::c_void = std::ptr::null_mut();
            // SAFETY: `index` is below the list's entry count.
            if unsafe { GetAce(dacl, index, &mut ace) } == 0 {
                return Err(last_error());
            }
            // SAFETY: `GetAce` gave a pointer to an entry of the list, whose header every kind
            // starts with.
            let header = unsafe { &*(ace as *const ACE_HEADER) };
            if header.AceType != ACCESS_ALLOWED_ACE_TYPE {
                continue;
            }
            // SAFETY: an entry of this type is an `ACCESS_ALLOWED_ACE`, whose SID starts at
            // `SidStart`.
            let allowed = unsafe { &*(ace as *const ACCESS_ALLOWED_ACE) };
            let sid = std::ptr::addr_of!(allowed.SidStart) as PSID;
            if allowed.Mask & WRITE_RIGHTS != 0 && !is_own(sid) {
                return Ok(false);
            }
        }
        Ok(true)
    })();
    // SAFETY: the descriptor was allocated by `GetSecurityInfo` and nothing points into it now.
    unsafe { LocalFree(descriptor) };
    verdict
}

/// Sets or clears the read-only attribute of `path`. On a directory Windows does not read it as a
/// bar to changing what is inside.
pub fn set_read_only(path: &Path, read_only: bool) -> io::Result<()> {
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_readonly(read_only);
    std::fs::set_permissions(path, permissions)
}

/// Whether the directory `path` is the root of a volume: a drive or share root (`C:\`,
/// `\\server\share\`), or a folder another volume is mounted at. A directory that cannot be read
/// counts as one, since unknown never destroys.
pub fn is_volume_root(path: &Path) -> bool {
    if path.parent().is_none() || std::fs::metadata(path).is_err() {
        return true;
    }
    let mut components = path.components();
    if matches!(
        (components.next(), components.next(), components.next()),
        (Some(Component::Prefix(_)), Some(Component::RootDir), None)
    ) {
        return true;
    }
    let name = wide(path);
    let mut volume = vec![0u16; name.len() + 2];
    // SAFETY: the name is NUL-terminated and the buffer holds `volume.len()` units, both alive for
    // the call; the volume path is never longer than the path it is asked for plus a separator.
    if unsafe { GetVolumePathNameW(name.as_ptr(), volume.as_mut_ptr(), volume.len() as u32) } == 0 {
        return true;
    }
    let length = volume.iter().position(|&unit| unit == 0).unwrap_or(0);
    let volume = PathBuf::from(OsString::from_wide(&volume[..length]));
    let key = |path: &Path| {
        let key = names::lexical_key(path);
        key.to_string_lossy().trim_end_matches('\\').to_string()
    };
    key(&volume) == key(path)
}

/// Makes `at` a junction to the absolute directory `target`: a new directory whose reparse data
/// names `target`, as `mklink /J` makes it. Unlike a symbolic link it needs no privilege.
fn make_junction(target: &Path, at: &Path) -> io::Result<()> {
    std::fs::create_dir(at)?;
    let made = (|| {
        let directory = OpenOptions::new()
            .write(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(at)?;
        let print: Vec<u16> = target.as_os_str().encode_wide().collect();
        let substitute: Vec<u16> = OsStr::new(r"\??\")
            .encode_wide()
            .chain(print.iter().copied())
            .collect();
        // REPARSE_DATA_BUFFER's mount point form: the tag, the data length and a reserved word,
        // then the substitute and print names' offsets and lengths in bytes, then both names, each
        // followed by a NUL.
        let names_bytes = (substitute.len() + 1 + print.len() + 1) * 2;
        let mut data = Vec::with_capacity(16 + names_bytes);
        data.extend_from_slice(&IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
        data.extend_from_slice(&((8 + names_bytes) as u16).to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&((substitute.len() * 2) as u16).to_le_bytes());
        data.extend_from_slice(&(((substitute.len() + 1) * 2) as u16).to_le_bytes());
        data.extend_from_slice(&((print.len() * 2) as u16).to_le_bytes());
        for unit in substitute.iter().chain(&[0]).chain(&print).chain(&[0]) {
            data.extend_from_slice(&unit.to_le_bytes());
        }
        let mut returned = 0u32;
        // SAFETY: the directory is open for writing, the input buffer holds `data.len()` bytes,
        // and nothing is written back but the count.
        let set = unsafe {
            DeviceIoControl(
                directory.as_raw_handle(),
                FSCTL_SET_REPARSE_POINT,
                data.as_ptr().cast(),
                data.len() as u32,
                std::ptr::null_mut(),
                0,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        if set == 0 {
            return Err(last_error());
        }
        Ok(())
    })();
    if made.is_err() {
        let _ = std::fs::remove_dir(at);
    }
    made
}

/// Makes the directory alias `at` to `target`: a junction where `target` is an absolute path on a
/// local drive, which any user may make; a directory symbolic link otherwise (a relative target, or
/// one on a share, which a junction cannot name), which needs Developer Mode or the privilege.
pub fn alias_dir(target: &Path, at: &Path) -> io::Result<AliasKind> {
    let on_a_drive = matches!(
        target.components().next(),
        Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))
    );
    if on_a_drive && target.is_absolute() {
        let target = names::plain_form(target.to_path_buf());
        make_junction(&target, at)?;
        return Ok(AliasKind::Junction);
    }
    std::os::windows::fs::symlink_dir(target, at)?;
    Ok(AliasKind::Symlink)
}

/// Makes the file alias `at` to `target`, a symbolic link, where the host lets this process make
/// one ([`file_links_available`]); refused as unsupported otherwise.
pub fn alias_file(target: &Path, at: &Path) -> io::Result<AliasKind> {
    if !file_links_available() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this host does not let the daemon make file links",
        ));
    }
    std::os::windows::fs::symlink_file(target, at)?;
    Ok(AliasKind::Symlink)
}

/// Where the alias at `at` leads, as it is written (a relative target stays relative; a junction's
/// in its plain form); `None` where something is there that is no alias.
pub fn read_alias(at: &Path) -> io::Result<Option<PathBuf>> {
    let file = open_for_attributes(at, Links::Keep)?;
    if !reparse_tag(&file)?.is_some_and(is_name_surrogate) {
        return Ok(None);
    }
    drop(file);
    Ok(Some(names::plain_form(std::fs::read_link(at)?)))
}

/// Removes the file or link at `path` itself, never what a link leads to; a directory of its own
/// is refused. A link to a directory (a junction or a directory symbolic link) is a directory
/// entry here, which only a directory's removal takes away.
pub fn remove_file_or_link(path: &Path) -> io::Result<()> {
    let attributes = std::fs::symlink_metadata(path)?.file_attributes();
    let directory_link = FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DIRECTORY;
    if attributes & directory_link == directory_link {
        std::fs::remove_dir(path)
    } else {
        std::fs::remove_file(path)
    }
}

/// Whether [`alias_file`] can make an alias of a file on this host: a process makes a file
/// symbolic link with Developer Mode on (std asks for an unprivileged one) or while it holds the
/// privilege, as an elevated one does. Found out once, by making one in the temporary directory.
pub fn file_links_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.subsec_nanos());
        let probe = std::env::temp_dir().join(format!(
            "octoboard-link-probe-{}-{nanos}",
            std::process::id()
        ));
        let made =
            std::os::windows::fs::symlink_file("octoboard-link-probe-target", &probe).is_ok();
        if made {
            let _ = std::fs::remove_file(&probe);
        }
        made
    })
}

/// Whether a SQLite database opened through a file alias shares its write-ahead log (`-wal`) and
/// its index (`-shm`) with the alias's target: not here. SQLite's Windows layer names them from
/// the path as given, so a process that opens the alias and one that opens the target each keep a
/// log of their own over the one file: neither sees the other's writes, and the writes of the log
/// checkpointed first are overwritten by the other's.
pub const SQLITE_FOLLOWS_FILE_ALIASES: bool = false;

/// What a directory listing tells of one entry, read without following a link.
#[derive(Clone)]
pub struct EntryStat {
    pub kind: EntryType,
    pub size: u64,
    volume: u64,
    file: u64,
    written: i64,
    changed: i64,
}

impl EntryStat {
    /// The [`version`] a read of this entry, a regular file, would report.
    pub fn version(&self) -> String {
        version_of(
            self.volume,
            self.file,
            self.size,
            self.written,
            self.changed,
        )
    }
}

/// The size of the buffer a directory's entries are read into, a batch at a time.
const LISTING_BUFFER_BYTES: usize = 64 * 1024;

/// Reads the entries of an open directory through its handle, so what is listed is the directory
/// that was opened, whatever has happened to its path since. Each batch of entries carries their
/// file information too, kept for [`DirReader::stat`].
pub struct DirReader {
    directory: File,
    volume: u64,
    /// `u64`s, so every entry, which starts on an 8-byte boundary, is aligned.
    buffer: Vec<u64>,
    /// Where the next entry of the batch in the buffer starts, if there is one.
    next: Option<usize>,
    ended: bool,
    stats: HashMap<Vec<u8>, EntryStat>,
}

impl DirReader {
    pub fn new(directory: File) -> io::Result<Self> {
        let volume = file_identity(&directory)?.volume;
        Ok(Self {
            directory,
            volume,
            buffer: vec![0u64; LISTING_BUFFER_BYTES / 8],
            next: None,
            ended: false,
            stats: HashMap::new(),
        })
    }

    /// Reads the next batch of entries into the buffer; false at the end of the directory.
    fn read_batch(&mut self) -> io::Result<bool> {
        // SAFETY: the buffer holds `LISTING_BUFFER_BYTES` bytes and outlives the call; the handle is
        // open for listing for as long as `directory` is.
        let read = unsafe {
            GetFileInformationByHandleEx(
                self.directory.as_raw_handle(),
                FileIdBothDirectoryInfo,
                self.buffer.as_mut_ptr().cast(),
                LISTING_BUFFER_BYTES as u32,
            )
        };
        if read == 0 {
            let err = last_error();
            if err.raw_os_error() == Some(ERROR_NO_MORE_FILES as i32) {
                return Ok(false);
            }
            return Err(err);
        }
        Ok(true)
    }

    /// The next entry, skipping `.` and `..`; `None` at the end.
    pub fn next_entry(&mut self) -> io::Result<Option<Listed>> {
        loop {
            let offset = match self.next {
                Some(offset) => offset,
                None if self.ended => return Ok(None),
                None => {
                    if !self.read_batch()? {
                        self.ended = true;
                        return Ok(None);
                    }
                    0
                }
            };
            // SAFETY: `offset` is the start of an entry the last call wrote into the buffer, on an
            // 8-byte boundary; its name of `FileNameLength` bytes follows its fixed part inside
            // the buffer.
            let (info, name) = unsafe {
                let entry =
                    (self.buffer.as_ptr() as *const u8).add(offset) as *const FILE_ID_BOTH_DIR_INFO;
                let info = std::ptr::read(entry);
                let name = std::slice::from_raw_parts(
                    std::ptr::addr_of!((*entry).FileName) as *const u16,
                    info.FileNameLength as usize / 2,
                );
                (info, OsString::from_wide(name))
            };
            self.next = match info.NextEntryOffset {
                0 => None,
                step => Some(offset + step as usize),
            };
            if name == "." || name == ".." {
                continue;
            }
            // A reparse point's tag is where the entry's extended attribute size would be.
            let link = info.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
                && is_name_surrogate(info.EaSize);
            let kind = if link {
                EntryType::Link
            } else if info.FileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
                EntryType::Directory
            } else {
                EntryType::File
            };
            let name = names::name_bytes(&name).into_owned();
            self.stats.insert(
                name.clone(),
                EntryStat {
                    kind,
                    size: info.EndOfFile as u64,
                    volume: self.volume,
                    file: info.FileId as u64,
                    written: info.LastWriteTime,
                    changed: info.ChangeTime,
                },
            );
            return Ok(Some(Listed {
                name,
                hint: Some(kind),
            }));
        }
    }

    /// The entry's own metadata, as the listing read it; `None` for a name it did not list.
    pub fn stat(&self, entry: &Listed) -> Option<EntryStat> {
        self.stats.get(&entry.name).cloned()
    }
}

/// `USERPROFILE`, where Windows keeps the user's home, as it is set, if it is.
pub fn env_home() -> Option<OsString> {
    std::env::var_os("USERPROFILE")
}

/// The name of the user the daemon runs as, as Windows sets it in the environment.
pub fn account_name() -> Option<OsString> {
    std::env::var_os("USERNAME")
}

/// The profile directory Windows records for the user the daemon runs as.
pub fn account_home() -> Option<PathBuf> {
    let token = Token::of_this_process().ok()?;
    let mut length = 0u32;
    // SAFETY: a call with no buffer only reports the length needed.
    unsafe { GetUserProfileDirectoryW(token.0, std::ptr::null_mut(), &mut length) };
    let mut buffer = vec![0u16; length as usize];
    // SAFETY: the buffer holds `length` units and outlives the call.
    if unsafe { GetUserProfileDirectoryW(token.0, buffer.as_mut_ptr(), &mut length) } == 0 {
        return None;
    }
    let end = buffer
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(buffer.len());
    Some(PathBuf::from(OsString::from_wide(&buffer[..end])))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;

    /// Makes `at` an app execution alias of a package that does not exist, naming `target`, laid
    /// out as the Store lays its aliases out: version 3, then the package family name, the
    /// application's user model id, the target and the application type, each NUL-terminated.
    /// Any user may make one; starting it fails, as its package is not installed.
    fn make_alias(at: &Path, target: &Path) -> io::Result<()> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(at)?;
        let mut body = 3u32.to_le_bytes().to_vec();
        let family = OsStr::new("Octoboard.Test_0000000000000");
        let app = OsStr::new("Octoboard.Test_0000000000000!App");
        for text in [family, app, target.as_os_str(), OsStr::new("0")] {
            for unit in text.encode_wide().chain([0]) {
                body.extend_from_slice(&unit.to_le_bytes());
            }
        }
        let mut data = IO_REPARSE_TAG_APPEXECLINK.to_le_bytes().to_vec();
        data.extend_from_slice(&(body.len() as u16).to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&body);
        let mut returned = 0u32;
        // SAFETY: the file is open for writing, the input buffer holds `data.len()` bytes, and
        // nothing is written back but the count.
        let set = unsafe {
            DeviceIoControl(
                file.as_raw_handle(),
                FSCTL_SET_REPARSE_POINT,
                data.as_ptr().cast(),
                data.len() as u32,
                std::ptr::null_mut(),
                0,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        if set == 0 {
            return Err(last_error());
        }
        Ok(())
    }

    #[test]
    fn an_app_execution_alias_is_told_by_its_reparse_tag() {
        let dir = ScratchDir::new("fs-app-alias");
        let program = std::env::current_exe().expect("the test's own path");
        let alias = dir.join("pwsh.exe");
        make_alias(&alias, &program).expect("an alias is made");
        assert!(is_app_execution_alias(&alias));

        let folder = dir.join("folder");
        std::fs::create_dir(&folder).expect("a folder is made");
        let junction = dir.join("junction");
        make_junction(&folder, &junction).expect("a junction is made");
        assert!(!is_app_execution_alias(&program), "a program is no alias");
        assert!(!is_app_execution_alias(&junction), "a junction is no alias");
        assert!(!is_app_execution_alias(&dir.join("missing.exe")));
    }
}
