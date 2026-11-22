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
#[allow(
    clippy::disallowed_methods,
    reason = "the platform layer is where the OS call is made; `hostfs::canonical` is its answer"
)]
pub fn canonical(path: &Path) -> io::Result<PathBuf> {
    std::fs::canonicalize(path)
}

/// The automount trees mount a remote host on the first look at a name below them: `/net`, and on
/// macOS `/home` too (the `auto_home` mount; on Linux `/home` is an ordinary folder).
pub fn reaches_network_or_device(path: &Path) -> bool {
    reaches_automount(path, cfg!(target_os = "macos"))
}

fn reaches_automount(path: &Path, macos: bool) -> bool {
    path.starts_with("/net") || (macos && path.starts_with("/home"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `/home` is an automount only where the host says so: refused on macOS with everything below
    /// it, an ordinary folder elsewhere; a name that only starts with the same letters is never it.
    #[test]
    fn home_is_an_automount_on_macos_only() {
        for home in ["/home", "/home/", "/home/me", "/home/me/a.txt"] {
            assert!(reaches_automount(Path::new(home), true), "{home}");
            assert!(!reaches_automount(Path::new(home), false), "{home}");
        }
        for other in ["/homes", "/homework/a", "/Users/me/home", "/net/x"] {
            assert_eq!(
                reaches_automount(Path::new(other), true),
                other.starts_with("/net"),
                "{other}"
            );
        }
        assert!(reaches_automount(Path::new("/net/host"), false));
    }
}
