//! How the host spells names: a file name as bytes and back, which names a client may send, a git
//! path as a path, the key two paths are compared by, the path style the host announces and the
//! canonical form of a path on it.
//!
//! A name travels as bytes in one form on every host: raw bytes on Unix, WTF-8 on Windows (see
//! [`name_bytes`]). The wire-path module turns those bytes into the text a client sees; this
//! module is what makes them from, and back into, what the host's file system takes.

use std::borrow::Cow;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

#[cfg(unix)]
mod unix;

#[cfg(unix)]
use unix as host;

#[cfg(windows)]
mod windows;

#[cfg(windows)]
use windows as host;

// The Windows rules are plain functions over bytes and text, so their tests run on every host, and
// every host reads the Windows spelling of an absolute path (`is_foreign_absolute`).
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "the rest is exercised by tests off Windows")
)]
mod windows_rules;

pub use host::{canonical, PATH_STYLE};

/// A path the file system reported in its verbatim form, in the plain form [`canonical`] gives.
#[cfg(windows)]
pub(super) use host::plain_form;

/// Whether the host's default file system takes two names that differ only in case for one: a
/// fact for comparing names the user typed, not a promise about every mounted volume.
pub use host::FOLDS_CASE;

/// How a host writes an absolute path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathStyle {
    #[cfg_attr(
        windows,
        allow(
            dead_code,
            reason = "only the Unix host constructs it; the facts name it"
        )
    )]
    Unix,
    Windows,
}

/// Whether `recorded` is an absolute path in the other host family's style: on a Windows host a
/// path in Unix form (starting with `/`, a `//server/share` form being Windows's own), on a Unix
/// host a Windows one (`C:\`, `C:/`, `\\server\share`, `\\?\`). Such a path, written by a host
/// of the other style, names nothing here and is not to be read as a name or a relative path.
/// Text only: nothing is read from disk.
pub fn is_foreign_absolute(recorded: &str) -> bool {
    is_foreign_absolute_to(PATH_STYLE, recorded)
}

fn is_foreign_absolute_to(host: PathStyle, recorded: &str) -> bool {
    match host {
        PathStyle::Windows => recorded.starts_with('/') && !windows_rules::is_slash_unc(recorded),
        PathStyle::Unix => windows_rules::is_absolute_spelling(recorded),
    }
}

/// The bytes of a file name, one name to one byte string: on Unix the name's own bytes, on
/// Windows its WTF-8 form (UTF-8, with a lone surrogate written as the three bytes it would
/// have). Borrowed where they are the name's own: on Unix always, on Windows for valid Unicode.
pub fn name_bytes(name: &OsStr) -> Cow<'_, [u8]> {
    host::name_bytes(name)
}

/// The file name `bytes` stand for, the inverse of [`name_bytes`], or `None` where this host
/// cannot spell them: every byte string on Unix; on Windows WTF-8, so a name with an unpaired
/// surrogate comes back as the name it is, and bytes that are not WTF-8 (`caf\xe9`) as `None`.
pub fn name_from_bytes(bytes: &[u8]) -> Option<OsString> {
    host::name_from_bytes(bytes)
}

/// Whether `bytes` may be one path component this host will create or read, as a client sends it:
/// on Unix anything without a NUL; on Windows also none of `\ / : < > " | ? *`, no control
/// character, no trailing `.` or space, and no reserved device name (`CON`, `NUL`, `COM1`…,
/// with or without an extension), because Windows resolves those to something other than a
/// file of that name. The empty component, `.` and `..` are the caller's to refuse.
pub fn component_is_valid(bytes: &[u8]) -> bool {
    host::component_is_valid(bytes)
}

/// The path `git` printed as `bytes` (git writes `/` between components on every host), or `None`
/// where this host cannot spell it: every byte string on Unix; on Windows WTF-8 without NUL, as
/// [`name_from_bytes`] reads a name.
pub fn path_from_name_bytes(bytes: &[u8]) -> Option<PathBuf> {
    host::path_from_name_bytes(bytes)
}

/// The key two lexically normalised paths are compared by, so that spellings the host reads as
/// one path compare equal: on Unix the path itself; on Windows with `/` as `\` and case folded.
/// Lexical only, like normalisation: no link is resolved and nothing is read from disk, so it
/// answers "the same path as written", never "the same file".
pub fn lexical_key(path: &Path) -> OsString {
    host::lexical_key(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Windows name need not be valid UTF-16; one with an unpaired surrogate goes to its WTF-8
    /// bytes and back to itself, and the bytes name a component the host takes.
    #[cfg(windows)]
    #[test]
    fn a_name_with_an_unpaired_surrogate_survives_the_trip_through_bytes() {
        use std::os::windows::ffi::OsStringExt;
        let mut units: Vec<u16> = "surrogate-".encode_utf16().collect();
        units.push(0xD800);
        let name = OsString::from_wide(&units);
        let bytes = name_bytes(&name);
        assert_eq!(&*bytes, b"surrogate-\xED\xA0\x80");
        assert_eq!(name_from_bytes(&bytes).as_deref(), Some(name.as_os_str()));
        assert!(component_is_valid(&bytes));
        assert_eq!(name_from_bytes(b"caf\xe9"), None);
    }

    #[test]
    fn a_name_survives_the_trip_through_bytes() {
        for name in ["a", "a b.txt", ".git", "日本語.md", "100%"] {
            let bytes = name_bytes(OsStr::new(name));
            assert_eq!(&*bytes, name.as_bytes());
            assert_eq!(name_from_bytes(&bytes).as_deref(), Some(OsStr::new(name)));
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_unix_name_that_is_not_utf8_keeps_every_byte() {
        use std::os::unix::ffi::OsStrExt;

        let raw: &[u8] = b"caf\xe9\xff.txt";
        let name = name_from_bytes(raw).expect("every byte string is a name on Unix");
        assert_eq!(name.as_bytes(), raw);
        assert_eq!(&*name_bytes(&name), raw);
        assert!(matches!(name_bytes(&name), Cow::Borrowed(_)));
    }

    #[test]
    fn a_path_of_the_other_style_is_foreign_to_each_host() {
        for unix in ["/Users/x/app", "/", "//x"] {
            assert!(is_foreign_absolute_to(PathStyle::Windows, unix), "{unix}");
            assert!(!is_foreign_absolute_to(PathStyle::Unix, unix), "{unix}");
        }
        for windows in [r"C:\x\app", "C:/x/app", r"\\server\share", r"\\?\C:\a"] {
            assert!(
                is_foreign_absolute_to(PathStyle::Unix, windows),
                "{windows}"
            );
            assert!(
                !is_foreign_absolute_to(PathStyle::Windows, windows),
                "{windows}"
            );
        }
        // Windows reads this as a UNC path of its own.
        assert!(!is_foreign_absolute_to(
            PathStyle::Windows,
            "//server/share/a"
        ));
        for neither in ["app", "work/app", "~/app", "", ".", "C:", r"a\b"] {
            for host in [PathStyle::Unix, PathStyle::Windows] {
                assert!(!is_foreign_absolute_to(host, neither), "{neither}");
            }
        }
    }

    #[test]
    fn the_components_every_host_creates_are_valid_and_a_nul_never_is() {
        for good in ["a", "a.b", ".git", "日本語", "a b", "a-b_c", "100%"] {
            assert!(component_is_valid(good.as_bytes()), "{good:?} was refused");
        }
        assert!(!component_is_valid(b"a\0b"));
    }

    #[test]
    fn git_bytes_become_the_path_of_their_components() {
        assert_eq!(
            path_from_name_bytes(b"dir/sub/file.txt"),
            Some(Path::new("dir").join("sub").join("file.txt"))
        );
        assert_eq!(path_from_name_bytes(b"file"), Some(PathBuf::from("file")));
    }

    #[cfg(unix)]
    #[test]
    fn git_bytes_that_are_not_utf8_are_a_path_on_unix() {
        use std::os::unix::ffi::OsStrExt;

        let path = path_from_name_bytes(b"d\xe9/f").expect("every byte string is a path on Unix");
        assert_eq!(path.as_os_str().as_bytes(), b"d\xe9/f");
    }

    #[test]
    fn a_path_is_its_own_key_and_spellings_of_one_path_share_it() {
        let path = Path::new("a").join("b");
        assert_eq!(lexical_key(&path), lexical_key(&path.clone()));
        assert_ne!(lexical_key(Path::new("a")), lexical_key(Path::new("b")));
        #[cfg(unix)]
        assert_eq!(lexical_key(Path::new("/a/B")), OsString::from("/a/B"));
        #[cfg(windows)]
        assert_eq!(
            lexical_key(Path::new("C:/Users/Me")),
            lexical_key(Path::new("c:\\users\\me"))
        );
    }

    #[test]
    fn the_canonical_form_resolves_a_linked_directory_and_reports_a_missing_one() {
        let base = std::env::temp_dir().join(format!("octoboard-names-{}", std::process::id()));
        let real = base.join("real");
        let link = base.join("link");
        std::fs::create_dir_all(&real).unwrap();
        crate::platform::fs::alias_dir(&real, &link).unwrap();
        let found = canonical(&link);
        let again = found.as_ref().ok().map(|found| canonical(found));
        let missing = canonical(&base.join("missing"));
        std::fs::remove_dir_all(&base).unwrap();
        let found = found.unwrap();
        assert!(found.is_absolute());
        assert!(found.ends_with("real"), "{found:?} still names the link");
        // The form a user types: no verbatim prefix where the plain form says the same.
        assert!(!found.to_string_lossy().starts_with(r"\\?\"), "{found:?}");
        assert_eq!(
            found,
            again.unwrap().unwrap(),
            "the canonical form is its own"
        );
        assert_eq!(missing.unwrap_err().kind(), std::io::ErrorKind::NotFound);
    }
}
