use std::borrow::Cow;
use std::ffi::{OsStr, OsString};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use super::PathStyle;

#[allow(
    dead_code,
    reason = "read by the host facts, which nothing consumes yet"
)]
pub const PATH_STYLE: PathStyle = PathStyle::Unix;

/// macOS's default file system is case-insensitive; the others Unix hosts use are not.
pub const FOLDS_CASE: bool = cfg!(target_os = "macos");

pub fn name_bytes(name: &OsStr) -> Cow<'_, [u8]> {
    Cow::Borrowed(name.as_bytes())
}

pub fn name_from_bytes(bytes: &[u8]) -> Option<OsString> {
    Some(OsStr::from_bytes(bytes).to_owned())
}

pub fn component_is_valid(bytes: &[u8]) -> bool {
    !bytes.contains(&0)
}

pub fn path_from_name_bytes(bytes: &[u8]) -> Option<PathBuf> {
    Some(PathBuf::from(OsStr::from_bytes(bytes)))
}

pub fn lexical_key(path: &Path) -> OsString {
    path.as_os_str().to_owned()
}

/// `path` made absolute with every link resolved, as the file system spells it.
pub fn canonical(path: &Path) -> io::Result<PathBuf> {
    std::fs::canonicalize(path)
}
