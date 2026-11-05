//! How a path, a branch name or any other byte string from the file system or `git` travels on
//! the wire without losing a byte.
//!
//! A filename on Unix is any bytes but `/` and NUL, and JSON strings carry only Unicode, so a path
//! is sent in one canonical text form: valid UTF-8 as itself, except that `%` is written `%25`,
//! and every byte that is not part of valid UTF-8 as `%XX` in upper-case hex. The form is its own
//! identity — two paths are the same file name exactly when their forms are equal — and it is never
//! display text: a client derives that by decoding the form back to bytes and reading them as
//! UTF-8 with replacement characters. Anything not in the canonical form (a lower-case escape, an
//! escaped byte that needed none) is refused rather than normalised, so one name has one spelling.

use std::path::{Component, Path, PathBuf};

use crate::platform::names;

/// A validated relative path, held as its raw bytes: components separated by `/`, none empty, `.`
/// or `..`, and none holding a NUL, which no file system and no `git` path holds. It names
/// something below a root (a browse scope, a project's directory) or is a `/`-separated name such
/// as a branch's; the empty path is the root itself. Whether this host can name each component is
/// asked only where the path is joined onto the disk ([`Self::host_path`]): `git` holds names
/// Windows cannot create (`CON`, `a:b`), which are listed and read from `git` all the same.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RelPath(Vec<u8>);

impl RelPath {
    /// Parses a path as a client sent it, refusing anything that is not a canonical, relative,
    /// `..`-free form — see the module doc.
    pub fn parse(wire: &str) -> Option<Self> {
        let bytes = decode(wire)?;
        if encode(&bytes) != wire {
            return None;
        }
        Self::from_bytes(bytes)
    }

    /// A path from raw bytes read off the filesystem or out of `git`, validated the same way.
    pub fn from_bytes(bytes: Vec<u8>) -> Option<Self> {
        if bytes.is_empty() {
            return Some(Self(bytes));
        }
        let valid = bytes.split(|&b| b == b'/').all(|component| {
            !component.is_empty()
                && component != b"."
                && component != b".."
                && !component.contains(&0)
        });
        valid.then_some(Self(bytes))
    }

    /// The path `path` names below `root`, or `None` where it is not below it, or what is left
    /// is not a valid relative path. Both are compared as the lexical paths they are: nothing is
    /// resolved.
    pub fn below(root: &Path, path: &Path) -> Option<Self> {
        let rest = path.strip_prefix(root).ok()?;
        let mut bytes = Vec::new();
        for component in rest.components() {
            let Component::Normal(name) = component else {
                return None;
            };
            if !bytes.is_empty() {
                bytes.push(b'/');
            }
            bytes.extend_from_slice(&names::name_bytes(name));
        }
        Self::from_bytes(bytes)
    }

    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// The path as the host spells it, built by pushing one component after another, for joining
    /// onto the disk; the root is the empty path. `None` where a component is one this host cannot
    /// name (`names::component_is_valid`): nothing on its disk has that path.
    pub fn host_path(&self) -> Option<PathBuf> {
        let mut path = PathBuf::new();
        for component in self.components() {
            if !names::component_is_valid(component) {
                return None;
            }
            path.push(names::name_from_bytes(component)?);
        }
        Some(path)
    }

    /// The path one component up, or `None` for the root, whose parent is not a path.
    pub fn parent(&self) -> Option<RelPath> {
        if self.is_root() {
            return None;
        }
        let end = self.0.iter().rposition(|&b| b == b'/').unwrap_or(0);
        Some(RelPath(self.0[..end].to_vec()))
    }

    fn components(&self) -> impl Iterator<Item = &[u8]> {
        self.0
            .split(|&b| b == b'/')
            .filter(|component| !component.is_empty())
    }

    pub fn to_wire(&self) -> String {
        encode(&self.0)
    }

    /// `other` appended below this path; either may be the root.
    pub fn join(&self, other: &RelPath) -> RelPath {
        match (self.is_root(), other.is_root()) {
            (true, _) => other.clone(),
            (_, true) => self.clone(),
            _ => {
                let mut bytes = self.0.clone();
                bytes.push(b'/');
                bytes.extend_from_slice(&other.0);
                RelPath(bytes)
            }
        }
    }
}

/// The canonical wire form of `bytes` — see the module doc.
pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for chunk in bytes.utf8_chunks() {
        for ch in chunk.valid().chars() {
            if ch == '%' {
                out.push_str("%25");
            } else {
                out.push(ch);
            }
        }
        for byte in chunk.invalid() {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The bytes a wire form stands for, or `None` for a malformed escape. Accepts non-canonical forms;
/// [`RelPath::parse`] is what refuses them.
pub fn decode(wire: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(wire.len());
    let mut bytes = wire.bytes();
    while let Some(byte) = bytes.next() {
        if byte != b'%' {
            out.push(byte);
            continue;
        }
        let high = hex_value(bytes.next()?)?;
        let low = hex_value(bytes.next()?)?;
        out.push(high << 4 | low);
    }
    Some(out)
}

fn hex_value(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'A'..=b'F' => Some(digit - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_byte_string_round_trips_through_its_one_canonical_form() {
        let cases: &[(&[u8], &str)] = &[
            (b"src/main.rs", "src/main.rs"),
            (b"100% done.txt", "100%25 done.txt"),
            (b"caf\xe9.txt", "caf%E9.txt"),
            ("日本語/a b\nc*?[x].md".as_bytes(), "日本語/a b\nc*?[x].md"),
        ];
        for (bytes, wire) in cases {
            assert_eq!(encode(bytes), *wire);
            let parsed = RelPath::parse(wire).expect("a canonical form parses");
            assert_eq!(parsed.as_bytes(), *bytes);
            // Only a path whose every name the host takes is one on its disk: Windows refuses
            // `*`, `?`, a newline and bytes that are not UTF-8.
            let valid = bytes.split(|&b| b == b'/').all(names::component_is_valid);
            assert_eq!(parsed.host_path().is_some(), valid, "{wire:?}");
        }
    }

    #[test]
    fn a_path_is_built_from_and_found_below_its_root() {
        let root = Path::new("root");
        let rel = RelPath::parse("a/b c/d.txt").expect("a canonical form");
        let path = root.join(rel.host_path().unwrap());
        assert_eq!(path, root.join("a").join("b c").join("d.txt"));
        assert_eq!(RelPath::below(root, &path), Some(rel.clone()));
        assert_eq!(
            RelPath::below(root, root),
            Some(RelPath::parse("").unwrap())
        );
        assert_eq!(RelPath::below(root, Path::new("other/a")), None);
        assert_eq!(RelPath::below(root, &root.join("..").join("a")), None);
        assert!(RelPath::parse("")
            .unwrap()
            .host_path()
            .unwrap()
            .as_os_str()
            .is_empty());
    }

    #[test]
    fn the_parent_of_a_path_is_the_path_without_its_last_component() {
        let parent = |wire: &str| RelPath::parse(wire).unwrap().parent().map(|p| p.to_wire());
        assert_eq!(parent("a/b/c").as_deref(), Some("a/b"));
        assert_eq!(parent("a").as_deref(), Some(""));
        assert_eq!(parent(""), None);
    }

    #[test]
    fn a_path_that_is_not_canonical_relative_and_dot_free_is_refused() {
        for wire in [
            "/etc/passwd",
            "a/../b",
            "..",
            "./a",
            "a//b",
            "a/",
            "%41",
            "caf%e9",
            "%2",
            "a%00b",
        ] {
            assert!(RelPath::parse(wire).is_none(), "{wire:?} was accepted");
        }
        assert!(RelPath::parse("").expect("the scope itself").is_root());
    }

    /// A name `git` may hold but Windows cannot create travels as any other path; only its
    /// place on the disk is refused there.
    #[test]
    fn a_name_the_host_cannot_create_parses_but_has_no_place_on_its_disk() {
        for wire in ["CON", "a/NUL.txt", "a:b", "trailing.", "x/y<z"] {
            let rel = RelPath::parse(wire).expect("a git path parses on every host");
            assert_eq!(rel.to_wire(), wire);
            assert_eq!(rel.host_path().is_some(), !cfg!(windows), "{wire:?}");
        }
    }
}
