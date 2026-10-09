//! How a path inside a browse scope travels on the wire without losing a byte.
//!
//! A filename on Unix is any bytes but `/` and NUL, and JSON strings carry only Unicode, so a path
//! is sent in one canonical text form: valid UTF-8 as itself, except that `%` is written `%25`,
//! and every byte that is not part of valid UTF-8 as `%XX` in upper-case hex. The form is its own
//! identity — two paths are the same file name exactly when their forms are equal — and it is never
//! display text: a client derives that by decoding the form back to bytes and reading them as
//! UTF-8 with replacement characters. Anything not in the canonical form (a lower-case escape, an
//! escaped byte that needed none) is refused rather than normalised, so one name has one spelling.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// A validated path relative to a browse scope, held as its raw bytes: components separated by
/// `/`, none empty, `.` or `..`, none containing NUL. The empty path is the scope itself.
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
        let valid = bytes
            .split(|&b| b == b'/')
            .all(|component| !component.is_empty() && component != b"." && component != b"..")
            && !bytes.contains(&0);
        valid.then_some(Self(bytes))
    }

    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn as_path(&self) -> &Path {
        Path::new(OsStr::from_bytes(&self.0))
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
            let parsed = RelPath::parse(wire).expect("canonical form parses");
            assert_eq!(parsed.as_bytes(), *bytes);
        }
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
}
