//! A matching line as a content search sends it: its text clipped around its first match, and
//! where the text and every match lie in the whole line, in UTF-16 code units.

use super::Bounds;
use crate::protocol::MatchRange;

pub(super) fn trim_line_ending(line: &[u8]) -> &[u8] {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    line.strip_suffix(b"\r").unwrap_or(line)
}

/// A line's text as sent: clipped to `max_line_chars` around its first match, where it starts at
/// `text_start`, with every match's byte range turned into UTF-16 offsets in the whole line.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Clipped {
    pub(super) text: String,
    pub(super) text_start: u32,
    pub(super) ranges: Vec<MatchRange>,
    pub(super) clipped_end: bool,
}

pub(super) fn clip(line: &[u8], ranges: &[std::ops::Range<usize>], bounds: &Bounds) -> Clipped {
    let first = ranges.first().map_or(0, |range| range.start);
    // Only the part of a long line that can be sent is decoded: a character is at most 4 bytes.
    let window_bytes = 4 * (bounds.max_line_chars + bounds.line_chars_before);
    let mut low = if line.len() <= window_bytes {
        0
    } else {
        first.saturating_sub(4 * bounds.line_chars_before)
    };
    while low > 0 && low < line.len() && is_continuation(line[low]) {
        low += 1;
    }
    let mut high = line.len().min(low + window_bytes);
    while high < line.len() && high > low && is_continuation(line[high]) {
        high -= 1;
    }
    // Each character of the window, with the byte (in `line`) it starts at, as UTF-8 with
    // replacement characters reads it.
    let mut chars: Vec<(usize, char)> = Vec::new();
    let mut at = low;
    for chunk in line[low..high].utf8_chunks() {
        for (offset, ch) in chunk.valid().char_indices() {
            chars.push((at + offset, ch));
        }
        at += chunk.valid().len();
        if !chunk.invalid().is_empty() {
            chars.push((at, char::REPLACEMENT_CHARACTER));
            at += chunk.invalid().len();
        }
    }
    let char_at = |byte: usize| chars.partition_point(|&(start, _)| start < byte);
    let total = chars.len();
    let first_char = char_at(first);
    let first_end_char = ranges.first().map_or(0, |range| char_at(range.end));
    // The text starts at the line's start only where the window does and the first match ends
    // within the characters sent; otherwise it starts a little before that match.
    let start = if low == 0
        && (total <= bounds.max_line_chars || first_end_char <= bounds.max_line_chars)
    {
        0
    } else {
        first_char
            .saturating_sub(bounds.line_chars_before)
            .min(total.saturating_sub(bounds.max_line_chars))
    };
    let end = total.min(start + bounds.max_line_chars);
    let text: String = chars[start..end].iter().map(|&(_, ch)| ch).collect();
    let text_start_byte = chars.get(start).map_or(high, |&(byte, _)| byte);
    let mut boundaries: Vec<usize> = std::iter::once(text_start_byte)
        .chain(ranges.iter().flat_map(|range| [range.start, range.end]))
        .collect();
    boundaries.sort_unstable();
    let offsets = utf16_offsets(line, &boundaries);
    let offset_of = |byte: usize| offsets[boundaries.partition_point(|&b| b < byte)];
    Clipped {
        text,
        text_start: offset_of(text_start_byte),
        ranges: ranges
            .iter()
            .map(|range| MatchRange {
                start: offset_of(range.start),
                end: offset_of(range.end),
            })
            .collect(),
        clipped_end: high < line.len() || end < total,
    }
}

/// For each of `bytes` (sorted), the UTF-16 length of the characters of `line`, read as UTF-8 with
/// replacement characters, that start before it.
fn utf16_offsets(line: &[u8], bytes: &[usize]) -> Vec<u32> {
    let mut offsets = Vec::with_capacity(bytes.len());
    let mut next = 0;
    let mut units = 0u32;
    let record = |start: usize, units: u32, offsets: &mut Vec<u32>, next: &mut usize| {
        while *next < bytes.len() && bytes[*next] <= start {
            offsets.push(units);
            *next += 1;
        }
    };
    let mut at = 0;
    for chunk in line.utf8_chunks() {
        for (offset, ch) in chunk.valid().char_indices() {
            record(at + offset, units, &mut offsets, &mut next);
            units += ch.len_utf16() as u32;
        }
        at += chunk.valid().len();
        if !chunk.invalid().is_empty() {
            record(at, units, &mut offsets, &mut next);
            units += 1;
            at += chunk.invalid().len();
        }
        if next == bytes.len() {
            break;
        }
    }
    while offsets.len() < bytes.len() {
        offsets.push(units);
    }
    offsets
}

fn is_continuation(byte: u8) -> bool {
    byte & 0b1100_0000 == 0b1000_0000
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(max_line_chars: usize, line_chars_before: usize) -> Bounds {
        Bounds {
            max_line_chars,
            line_chars_before,
            ..Bounds::standard()
        }
    }

    #[test]
    fn a_short_line_is_sent_whole_with_its_ranges_in_utf16_units() {
        let line = "é😀 needle".as_bytes();
        let start = line.len() - "needle".len();
        let clipped = clip(
            line,
            std::slice::from_ref(&(start..line.len())),
            &bounds(250, 40),
        );
        assert_eq!(clipped.text, "é😀 needle");
        // é is one UTF-16 unit, 😀 two, the space one.
        assert_eq!(clipped.ranges, vec![MatchRange { start: 4, end: 10 }]);
        assert_eq!((clipped.text_start, clipped.clipped_end), (0, false));
    }

    #[test]
    fn a_long_line_is_clipped_around_its_first_match_with_ranges_in_the_whole_line() {
        let text = format!("😀{}needle{}", "a".repeat(99), "b".repeat(100));
        // The emoji is 4 bytes and 2 UTF-16 units, so the needle starts at byte 103, unit 101.
        let clipped = clip(
            text.as_bytes(),
            std::slice::from_ref(&(103..109)),
            &bounds(20, 4),
        );
        assert_eq!(clipped.text, format!("aaaaneedle{}", "b".repeat(10)));
        assert_eq!(clipped.text_start, 97);
        assert_eq!(
            clipped.ranges,
            vec![MatchRange {
                start: 101,
                end: 107
            }]
        );
        assert!(clipped.clipped_end);
        // A match the start of the line already shows keeps the start.
        let early = clip(
            text.as_bytes(),
            std::slice::from_ref(&(4..7)),
            &bounds(20, 4),
        );
        assert_eq!(early.text, format!("😀{}", "a".repeat(19)));
        assert_eq!((early.text_start, early.clipped_end), (0, true));
        // Matches outside the clipped text keep their places in the whole line.
        let all = clip(text.as_bytes(), &[103..109, 200..205], &bounds(20, 4));
        assert_eq!(
            all.ranges,
            vec![
                MatchRange {
                    start: 101,
                    end: 107
                },
                MatchRange {
                    start: 198,
                    end: 203
                }
            ]
        );
    }

    #[test]
    fn invalid_utf8_is_read_with_replacement_characters() {
        let line = b"a\xffneedle";
        let clipped = clip(line, std::slice::from_ref(&(2..8)), &bounds(250, 40));
        assert_eq!(clipped.text, "a\u{fffd}needle");
        assert_eq!(clipped.ranges, vec![MatchRange { start: 2, end: 8 }]);
    }

    #[test]
    fn a_line_longer_than_the_decoded_window_keeps_only_the_context_asked_for() {
        // Longer than 4 * (250 + 40) bytes, so only a window around the first match is decoded.
        let text = format!("{}needle{}", "a".repeat(1500), "b".repeat(300));
        let clipped = clip(
            text.as_bytes(),
            std::slice::from_ref(&(1500..1506)),
            &Bounds::standard(),
        );
        assert_eq!(clipped.text_start, 1460);
        assert!(clipped
            .text
            .starts_with(&format!("{}needle", "a".repeat(40))));
        assert_eq!(clipped.text.chars().count(), 250);
        assert_eq!(
            clipped.ranges,
            vec![MatchRange {
                start: 1500,
                end: 1506
            }]
        );
    }
}
