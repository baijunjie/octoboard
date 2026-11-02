//! A body as the wire carries it: whether it travels as text or as base64, and what is counted of
//! it on the way (`file_content`, `json_escaped_len`, `line_count`).

use base64::Engine as _;

use crate::protocol::{ContentKind, FileContent};

/// How long `text` becomes once JSON-escaped: `"`, `\` and the five control characters JSON has a
/// short escape for take two bytes, every other control character six (`\u00XX`).
pub(crate) fn json_escaped_len(text: &[u8]) -> usize {
    text.iter()
        .map(|&byte| match byte {
            b'"' | b'\\' | 0x08 | 0x09 | 0x0a | 0x0c | 0x0d => 2,
            0x00..=0x1f => 6,
            _ => 1,
        })
        .sum()
}

/// A body as the wire carries it. Text is valid UTF-8 with no NUL whose JSON escaping at most
/// doubles it — so a file of control characters, which escaping would sextuple, travels as
/// binary — and is sent as a string; anything else is base64-encoded and never decoded as text
/// here. That doubling is the ceiling the reply's memory reservation is sized by.
pub(crate) fn file_content(bytes: Vec<u8>) -> FileContent {
    let size = bytes.len() as u64;
    let textual = !bytes.contains(&0) && json_escaped_len(&bytes) <= 2 * bytes.len();
    let bytes = if textual {
        match String::from_utf8(bytes) {
            Ok(text) => {
                return FileContent {
                    size,
                    kind: ContentKind::Text,
                    media_type: None,
                    text: Some(text),
                    data: None,
                }
            }
            Err(err) => err.into_bytes(),
        }
    } else {
        bytes
    };
    let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
    FileContent {
        size,
        kind: ContentKind::Binary,
        media_type: sniff_image(&bytes),
        text: None,
        data: Some(data),
    }
}

/// The number of lines `bytes` holds: its newlines, and one more when the last line is
/// unterminated, so empty bytes hold none.
pub(crate) fn line_count(bytes: &[u8]) -> u64 {
    let newlines = bytes.iter().filter(|&&b| b == b'\n').count() as u64;
    newlines + u64::from(bytes.last().is_some_and(|&b| b != b'\n'))
}

/// The image type `bytes` starts like, by its magic number.
fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    let starts = |magic: &[u8]| bytes.starts_with(magic);
    if starts(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if starts(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if starts(b"GIF87a") || starts(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if starts(b"BM") {
        Some("image/bmp")
    } else if starts(b"\x00\x00\x01\x00") {
        Some("image/x-icon")
    } else if bytes.len() >= 12
        && &bytes[4..8] == b"ftyp"
        && matches!(&bytes[8..12], b"avif" | b"avis")
    {
        Some("image/avif")
    } else {
        None
    }
}
