use std::borrow::Cow;
use std::ffi::{OsStr, OsString};
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

use super::windows_rules;
use super::PathStyle;

#[allow(
    dead_code,
    reason = "read by the host facts, which nothing consumes yet"
)]
pub const PATH_STYLE: PathStyle = PathStyle::Windows;

/// NTFS compares names without regard to case.
pub const FOLDS_CASE: bool = true;

/// Encoded from the UTF-16 the name is, not taken from the bytes the standard library holds it in,
/// whose encoding it leaves unspecified; a name that is valid Unicode is its own UTF-8, borrowed.
pub fn name_bytes(name: &OsStr) -> Cow<'_, [u8]> {
    match name.to_str() {
        Some(text) => Cow::Borrowed(text.as_bytes()),
        None => Cow::Owned(windows_rules::wtf8(name.encode_wide())),
    }
}

/// Built from the UTF-16 the bytes decode to, as [`name_bytes`] is encoded from it.
pub fn name_from_bytes(bytes: &[u8]) -> Option<OsString> {
    windows_rules::wide(bytes).map(|units| OsString::from_wide(&units))
}

pub fn component_is_valid(bytes: &[u8]) -> bool {
    windows_rules::component_is_valid(bytes)
}

pub fn path_from_name_bytes(bytes: &[u8]) -> Option<PathBuf> {
    windows_rules::path_wide(bytes).map(|units| PathBuf::from(OsString::from_wide(&units)))
}

pub fn lexical_key(path: &Path) -> OsString {
    match path.to_str() {
        Some(text) => OsString::from(windows_rules::lexical_key(text)),
        None => path.as_os_str().to_owned(),
    }
}

/// `path` made absolute with every link resolved, as the file system spells it, in the form a user
/// types: the file system answers with the verbatim prefix (`\\?\C:\…`, `\\?\UNC\…`), which is
/// dropped where the rest is a drive or UNC path the plain form spells the same (see
/// `windows_rules::without_verbatim_prefix`). The daemon is long-path aware (its manifest), so the
/// plain form reaches past `MAX_PATH` too.
pub fn canonical(path: &Path) -> io::Result<PathBuf> {
    std::fs::canonicalize(path).map(plain_form)
}

/// `path` without its verbatim prefix where the plain form names the same path; as it is otherwise.
pub fn plain_form(path: PathBuf) -> PathBuf {
    windows_rules::without_verbatim_prefix(&name_bytes(path.as_os_str()))
        .and_then(|plain| name_from_bytes(&plain))
        .map(PathBuf::from)
        .unwrap_or(path)
}
