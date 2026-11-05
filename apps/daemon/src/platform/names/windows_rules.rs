//! The Windows rules for names, as plain functions over bytes and text: the host-specific part of
//! `names` that needs no call into the OS, kept apart so that its tests run on every host.

/// The UTF-16 code units of the name whose bytes are `bytes` (see `names::name_bytes`), or `None`
/// where they are not WTF-8. WTF-8 is UTF-8 that may also hold a surrogate code point on its own,
/// written as the three bytes UTF-8 would give it: a Windows name is UTF-16 that need not be valid,
/// and its bytes keep an unpaired surrogate that way. A lead and a trail surrogate written as two
/// such sequences are refused, as no name has those bytes: the pair is one character, written in
/// its four-byte form, and one name has one spelling.
pub(super) fn wide(bytes: &[u8]) -> Option<Vec<u16>> {
    let mut units = Vec::with_capacity(bytes.len());
    let mut rest = bytes;
    loop {
        let (valid, after) = match std::str::from_utf8(rest) {
            Ok(text) => (text, &[][..]),
            Err(err) => {
                let (valid, after) = rest.split_at(err.valid_up_to());
                (std::str::from_utf8(valid).ok()?, after)
            }
        };
        units.extend(valid.encode_utf16());
        let [0xED, second @ 0xA0..=0xBF, third @ 0x80..=0xBF, tail @ ..] = after else {
            return after.is_empty().then_some(units);
        };
        let unit = 0xD000 | (u16::from(second & 0x3F) << 6) | u16::from(third & 0x3F);
        let pairs = (0xDC00..=0xDFFF).contains(&unit)
            && units
                .last()
                .is_some_and(|last| (0xD800..=0xDBFF).contains(last));
        if pairs {
            return None;
        }
        units.push(unit);
        rest = tail;
    }
}

/// The WTF-8 bytes of the name whose UTF-16 code units are `units`, the inverse of [`wide`]: each
/// character as UTF-8, and an unpaired surrogate as the three bytes UTF-8 would give it.
pub(super) fn wtf8(units: impl IntoIterator<Item = u16>) -> Vec<u8> {
    let mut bytes = Vec::new();
    for decoded in char::decode_utf16(units) {
        match decoded {
            Ok(ch) => bytes.extend_from_slice(ch.encode_utf8(&mut [0; 4]).as_bytes()),
            Err(err) => {
                let unit = err.unpaired_surrogate();
                bytes.extend_from_slice(&[
                    0xE0 | (unit >> 12) as u8,
                    0x80 | ((unit >> 6) & 0x3F) as u8,
                    0x80 | (unit & 0x3F) as u8,
                ]);
            }
        }
    }
    bytes
}

/// The UTF-16 code units of the path git printed as `bytes`: WTF-8, as [`wide`] reads a name, and
/// without NUL.
pub(super) fn path_wide(bytes: &[u8]) -> Option<Vec<u16>> {
    if bytes.contains(&0) {
        return None;
    }
    wide(bytes)
}

/// The device names Windows resolves in every directory, with or without an extension.
const RESERVED: &[&str] = &["CON", "PRN", "AUX", "NUL"];

fn is_reserved(component: &str) -> bool {
    let stem = component.split('.').next().unwrap_or_default().trim_end();
    let upper = stem.to_ascii_uppercase();
    if RESERVED.contains(&upper.as_str()) {
        return true;
    }
    let Some(number) = upper
        .strip_prefix("COM")
        .or_else(|| upper.strip_prefix("LPT"))
    else {
        return false;
    };
    matches!(number.as_bytes(), [b'1'..=b'9'])
}

/// Whether `bytes` may be one path component on Windows; see `names::component_is_valid`.
pub(super) fn component_is_valid(bytes: &[u8]) -> bool {
    let Some(units) = wide(bytes) else {
        return false;
    };
    // An unpaired surrogate is a character none of the rules names, and judged as U+FFFD it is
    // judged as one: `CON.<surrogate>` is still the device.
    let component = String::from_utf16_lossy(&units);
    let component = component.as_str();
    !component
        .chars()
        .any(|ch| ch < ' ' || matches!(ch, '\\' | '/' | ':' | '<' | '>' | '"' | '|' | '?' | '*'))
        && !component.ends_with(['.', ' '])
        && !is_reserved(component)
}

/// The comparison key of a path's text: `/` as `\`, and case folded (Windows file systems compare
/// names case-insensitively by default).
pub(super) fn lexical_key(path: &str) -> String {
    path.replace('/', "\\").to_lowercase()
}

/// Whether `text` is an absolute path in a form only Windows writes: a drive with a separator
/// (`C:\a`, `C:/a`), or a path starting with two backslashes (`\\server\share`, `\\?\C:\a`,
/// `\\.\pipe`). A `/` or `//` start is not here: a Unix host reads it as its own.
pub(super) fn is_absolute_spelling(text: &str) -> bool {
    let bytes = text.as_bytes();
    let drive = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    drive || text.starts_with(r"\\")
}

/// Whether `text` starts like a UNC path written with slashes (`//server/share`), which Windows
/// reads as absolute, not as a path from the drive's root.
pub(super) fn is_slash_unc(text: &str) -> bool {
    let Some(rest) = text.strip_prefix("//") else {
        return false;
    };
    let mut parts = rest.split('/');
    matches!((parts.next(), parts.next()), (Some(server), Some(share)) if !server.is_empty() && !share.is_empty())
}

/// `path` without its verbatim prefix, or `None` where it has none or dropping it would change what
/// it names: `\\?\C:\a` is `C:\a` and `\\?\UNC\server\share\a` is `\\server\share\a`. A verbatim path
/// is taken as written, so one with a component the plain form would normalise away (a trailing
/// `.` or space, `.` or `..`, a reserved device name) or a `/` keeps its prefix.
///
/// Over the path's bytes (see `names::name_bytes`) rather than its text, so that a path through a
/// name with an unpaired surrogate, which is no text, loses its prefix like any other: kept, it
/// would not start with the plain form of the folder it lies in.
pub(super) fn without_verbatim_prefix(path: &[u8]) -> Option<Vec<u8>> {
    let rest = path.strip_prefix(br"\\?\")?;
    let (plain, components) = if let Some(unc) = rest.strip_prefix(br"UNC\") {
        ([br"\\".as_slice(), unc].concat(), unc)
    } else {
        let is_drive =
            rest.len() >= 3 && rest[0].is_ascii_alphabetic() && rest[1] == b':' && rest[2] == b'\\';
        if !is_drive {
            return None;
        }
        (rest.to_vec(), &rest[3..])
    };
    let plain_spells_it = !components.contains(&b'/')
        && components
            .split(|&byte| byte == b'\\')
            .filter(|component| !component.is_empty())
            .all(|component| {
                component != b"." && component != b".." && component_is_valid(component)
            });
    plain_spells_it.then_some(plain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn components_windows_cannot_create_are_refused() {
        for bad in [
            "a\\b",
            "a:b",
            "a<b",
            "a>b",
            "a\"b",
            "a|b",
            "a?b",
            "a*b",
            "a\0b",
            "a\tb",
            "a.",
            "a ",
            "CON",
            "con",
            "NUL.txt",
            "Aux.tar.gz",
            "COM1",
            "lpt9.log",
            "com1 .txt",
        ] {
            assert!(!component_is_valid(bad.as_bytes()), "{bad:?} was accepted");
        }
        assert!(!component_is_valid(b"caf\xe9"));
        // A device name stays one with an unpaired surrogate in its extension; a surrogate pair
        // written as two surrogates is not WTF-8.
        assert!(!component_is_valid(b"CON.\xED\xA0\x80"));
        assert!(!component_is_valid(b"\xED\xA0\xBD\xED\xB8\x80"));
    }

    #[test]
    fn components_windows_can_create_are_accepted() {
        for good in [
            "a",
            "a.b",
            ".git",
            "日本語",
            "a b",
            "COM",
            "COM10",
            "COM0",
            "console",
            "nul0",
            "a-b_c",
        ] {
            assert!(component_is_valid(good.as_bytes()), "{good:?} was refused");
        }
        assert!(component_is_valid(b"surrogate-\xED\xA0\x80"));
        assert!(component_is_valid(b"\xED\xB0\x80x"));
    }

    /// A name's bytes decode to the UTF-16 Windows holds it as, an unpaired surrogate included,
    /// and nothing else that is not WTF-8 does.
    #[test]
    fn wtf8_bytes_decode_to_the_names_utf16() {
        assert_eq!(wide(b"a\xC3\xA9"), Some(vec![0x61, 0xE9]));
        assert_eq!(wide("😀".as_bytes()), Some(vec![0xD83D, 0xDE00]));
        assert_eq!(
            wide(b"s-\xED\xA0\x80"),
            Some(vec![0x73, 0x2D, 0xD800]),
            "a lone lead surrogate"
        );
        assert_eq!(
            wide(b"\xED\xBF\xBFz"),
            Some(vec![0xDFFF, 0x7A]),
            "a lone trail surrogate"
        );
        assert_eq!(
            wide(b"\xED\xB0\x80\xED\xA0\x80"),
            Some(vec![0xDC00, 0xD800]),
            "a trail before a lead is no pair"
        );
        assert_eq!(wide(b""), Some(vec![]));
        for bad in [
            &b"caf\xe9"[..],
            b"\xED\xA0",
            b"\xED\xA0\x80\xED\xB0\x80",
            b"\xC0\xAF",
            b"\xED\x9F",
        ] {
            assert_eq!(wide(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn the_key_folds_separators_and_case() {
        assert_eq!(lexical_key("C:/Users/Me/a.TXT"), "c:\\users\\me\\a.txt");
        assert_eq!(
            lexical_key("C:\\Users\\Me\\a.TXT"),
            lexical_key("c:/users/me/A.txt")
        );
    }

    #[test]
    fn the_verbatim_prefix_is_dropped_where_the_plain_form_names_the_same_path() {
        let without_verbatim_prefix = |path: &str| {
            without_verbatim_prefix(path.as_bytes()).map(|plain| String::from_utf8(plain).unwrap())
        };
        assert_eq!(
            without_verbatim_prefix(r"\\?\C:\Users\Me").as_deref(),
            Some(r"C:\Users\Me")
        );
        assert_eq!(without_verbatim_prefix(r"\\?\C:\").as_deref(), Some(r"C:\"));
        assert_eq!(
            without_verbatim_prefix(r"\\?\UNC\server\share\dir").as_deref(),
            Some(r"\\server\share\dir")
        );
        assert_eq!(without_verbatim_prefix(r"C:\Users"), None);
        assert_eq!(
            without_verbatim_prefix(r"\\?\Volume{0b1c}\dir"),
            None,
            "a volume GUID path has no plain form"
        );
        // A folder whose name holds an unpaired surrogate (U+D800, WTF-8 `ED A0 80`) is no text.
        assert_eq!(
            super::without_verbatim_prefix(b"\\\\?\\C:\\a\\s-\xED\xA0\x80\\b"),
            Some(b"C:\\a\\s-\xED\xA0\x80\\b".to_vec())
        );
        for kept in [
            r"\\?\C:\a.\b",
            r"\\?\C:\a \b",
            r"\\?\C:\CON",
            r"\\?\C:\a\..\b",
        ] {
            assert_eq!(without_verbatim_prefix(kept), None, "{kept}");
        }
    }

    #[test]
    fn the_absolute_spellings_only_windows_writes_are_told_from_the_rest() {
        for windows in [
            r"C:\Users\me",
            "C:/Users/me",
            r"d:\",
            r"\\server\share\dir",
            r"\\?\C:\a",
            r"\\.\pipe\x",
        ] {
            assert!(is_absolute_spelling(windows), "{windows}");
        }
        for other in [
            "/Users/me",
            "//x",
            "C:",
            "C:a",
            "C",
            "~/a",
            "a/b",
            r"a\b",
            "",
        ] {
            assert!(!is_absolute_spelling(other), "{other}");
        }
        assert!(is_slash_unc("//server/share"));
        assert!(is_slash_unc("//server/share/dir"));
        for other in [
            "//server",
            "//server/",
            "///x/y",
            "/server/share",
            "C://a/b",
        ] {
            assert!(!is_slash_unc(other), "{other}");
        }
    }

    #[test]
    fn only_wtf8_without_nul_is_a_path() {
        assert_eq!(path_wide(b"C:/a"), Some(vec![0x43, 0x3A, 0x2F, 0x61]));
        assert_eq!(path_wide(b"a/\xED\xA0\x80"), Some(vec![0x61, 0x2F, 0xD800]));
        assert_eq!(path_wide(b"a\xffb"), None);
        assert_eq!(path_wide(b"a\0b"), None);
    }

    /// A name's UTF-16, an unpaired surrogate included, goes to WTF-8 and back unchanged.
    #[test]
    fn utf16_encodes_to_wtf8_and_back() {
        for units in [
            vec![0x61, 0xE9],
            vec![0xD83D, 0xDE00],
            vec![0x73, 0xD800],
            vec![0xDFFF, 0x7A],
            vec![0xDC00, 0xD800],
            vec![],
        ] {
            let bytes = wtf8(units.iter().copied());
            assert_eq!(wide(&bytes), Some(units.clone()), "{bytes:?}");
        }
        assert_eq!(wtf8([0x73, 0xD800]), b"s\xED\xA0\x80");
        assert_eq!(wtf8("😀".encode_utf16()), "😀".as_bytes());
    }
}
