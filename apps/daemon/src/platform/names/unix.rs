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
/// macOS also `/home` (the `auto_home` mount, which is also reached as `/System/Volumes/Data/home`)
/// and `/Network/Servers`; on Linux `/home` is an ordinary folder. macOS looks names up without
/// regard to case, so `/Home` is `/home` there.
pub fn reaches_network_or_device(path: &Path) -> bool {
    reaches_automount(path, cfg!(target_os = "macos"))
}

/// No link target names a local volume that a device-namespace spelling would be refused for: the
/// device namespace is a Windows form.
pub fn names_local_volume(_path: &Path) -> bool {
    false
}

fn reaches_automount(path: &Path, macos: bool) -> bool {
    if !macos {
        return path.starts_with("/net");
    }
    [
        "/net",
        "/home",
        "/System/Volumes/Data/home",
        "/Network/Servers",
    ]
    .iter()
    .any(|place| starts_with_ignoring_case(path, Path::new(place)))
}

/// Whether `path` is `place` or below it, component by component, case ignored as the macOS file
/// system folds it: ASCII case, and the two other characters whose simple case folding is ASCII,
/// the long s (U+017F, `s`) and the Kelvin sign (U+212A, `k`).
fn starts_with_ignoring_case(path: &Path, place: &Path) -> bool {
    let folded = |component: std::path::Component| -> String {
        component
            .as_os_str()
            .to_string_lossy()
            .chars()
            .map(|letter| match letter {
                '\u{17F}' => 's',
                '\u{212A}' => 'k',
                other => other.to_ascii_lowercase(),
            })
            .collect()
    };
    let mut path = path.components();
    place.components().all(|expected| {
        path.next()
            .is_some_and(|found| folded(found) == folded(expected))
    })
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

    /// On macOS the automount places are refused whatever the case they are written in, with
    /// everything below them; elsewhere only `/net` is, and by its exact name.
    #[test]
    fn the_macos_automount_places_are_refused_without_regard_to_case() {
        for place in [
            "/net",
            "/Net/host",
            "/HOME/me",
            "/Home",
            "/system/volumes/data/home/me",
            "/System/Volumes/Data/home",
            "/network/servers/host/x",
            "/Network/Servers",
            // The file system folds the long s and the Kelvin sign to `s` and `k`.
            "/\u{17F}ystem/Volumes/Data/home/u",
            "/Network/\u{17F}ervers/h",
            "/Networ\u{212A}/Servers/h",
        ] {
            assert!(reaches_automount(Path::new(place), true), "{place}");
        }
        for other in [
            "/Network",
            "/Network/Servers2",
            "/System/Volumes/Data",
            "/System/Volumes/Data/Users",
            "/System/Volumes/Data/homes",
            "/netx",
        ] {
            assert!(!reaches_automount(Path::new(other), true), "{other}");
        }
        for place in [
            "/Net",
            "/Home",
            "/Network/Servers",
            "/System/Volumes/Data/home",
        ] {
            assert!(!reaches_automount(Path::new(place), false), "{place}");
        }
        assert!(reaches_automount(Path::new("/net/x"), false));
    }
}
