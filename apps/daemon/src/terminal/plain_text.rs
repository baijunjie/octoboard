//! Terminal output as plain text.

/// `text` without its escape sequences: CSI (colour and cursor control), the string-introducing
/// sequences (OSC, DCS and the like, which run to a terminator rather than to a single final
/// byte) and the shorter escapes (`ESC =`, `ESC ( B`). Everything else is kept, control characters
/// included.
pub(crate) fn strip_escapes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\x1b' {
            out.push(ch);
            continue;
        }
        match chars.peek() {
            // CSI: parameters and intermediates, then one final byte in `@`..`~`.
            Some('[') => {
                chars.next();
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            Some(']') | Some('P') | Some('X') | Some('^') | Some('_') => {
                chars.next();
                while let Some(next) = chars.next() {
                    if next == '\x07' {
                        break;
                    }
                    if next == '\x1b' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            // Any other escape: intermediates (`0x20`..`0x2F`, as in a charset selection
            // `ESC ( B`), then one final byte.
            Some(_) => {
                while chars
                    .next_if(|next| ('\x20'..='\x2f').contains(next))
                    .is_some()
                {}
                chars.next();
            }
            None => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Colour and cursor sequences, string sequences ending in BEL or ST, and shorter escapes go;
    /// the text between them stays.
    #[test]
    fn escape_sequences_are_removed_and_the_text_between_them_kept() {
        assert_eq!(
            strip_escapes("\x1b[1;32mABCDE-12345\x1b[0m \x1b[2K\x1b[1Gdone"),
            "ABCDE-12345 done"
        );
        assert_eq!(
            strip_escapes("\x1b]8;;https://example.com\x07link\x1b]8;;\x1b\\ \x1b]0;title\x07ok"),
            "link ok"
        );
        assert_eq!(strip_escapes("a\x1b=b\x1b>c"), "abc");
        assert_eq!(strip_escapes("a\x1b(Bb"), "ab");
        assert_eq!(strip_escapes("plain ✓\r\n\x07"), "plain ✓\r\n\x07");
        assert_eq!(strip_escapes("cut off\x1b"), "cut off");
        assert_eq!(strip_escapes("cut off\x1b[31"), "cut off");
    }
}
