//! A bounded observer of OSC 0/2 titles in the unmodified PTY byte stream.

const MAX_TITLE_BYTES: usize = 4096;

/// Both phases of Codex's blinking activity title identify a visible user interaction.
pub(crate) fn codex_action_required(title: &str) -> bool {
    matches!(
        title.split(" | ").next().map(str::trim),
        Some("[ ! ] Action Required" | "[ . ] Action Required")
    )
}

/// Whether `c` is one of the glyphs an agent puts at the start of its title to show what it is
/// doing: Claude Code's rotating circle and its idle star, the braille spinners Codex and other
/// terminal tools use, and the star and diamond markers other agent CLIs use. The set is
/// specified in `docs/product/session-titles.md`'s "Leading activity glyphs" section.
fn is_spinner_glyph(c: char) -> bool {
    matches!(c,
        '\u{2800}'..='\u{28FF}'
        | '\u{25D0}'..='\u{25D3}'
        | '\u{25F0}'..='\u{25F7}'
        | '\u{2722}'..='\u{2727}'
        | '\u{2729}'..='\u{273D}'
        | '\u{00B7}' | '\u{2022}' | '\u{25CF}' | '\u{25CB}' | '\u{25C9}' | '\u{25CE}'
        | '\u{25C6}' | '\u{25C7}')
}

/// The title without the animated glyphs it starts with, so a title that only spins compares equal
/// from one frame to the next and is neither stored nor announced again. The rule for which leading
/// runs count is in `docs/product/session-titles.md`'s "Leading activity glyphs" section.
fn without_spinner_glyphs(title: &str) -> &str {
    let rest = title.trim_start_matches(is_spinner_glyph);
    if rest.len() == title.len() || !(rest.is_empty() || rest.starts_with(char::is_whitespace)) {
        return title;
    }
    rest.trim_start()
}

#[derive(Default)]
enum Mode {
    #[default]
    Ground,
    Escape,
    Osc,
    OscEscape,
    IgnoreOsc,
    IgnoreOscEscape,
    String,
    StringEscape,
}

#[derive(Default)]
pub(crate) struct Parser {
    mode: Mode,
    osc: Vec<u8>,
}

impl Parser {
    /// Returns only the last completed title in a chunk. An empty title clears the stored title.
    pub(crate) fn feed(&mut self, data: &[u8]) -> Option<Option<String>> {
        let mut latest = None;
        for &byte in data {
            self.mode = match std::mem::take(&mut self.mode) {
                Mode::Ground if byte == 0x1b => Mode::Escape,
                Mode::Ground => Mode::Ground,
                Mode::Escape => match byte {
                    b']' => {
                        self.osc.clear();
                        Mode::Osc
                    }
                    b'P' | b'X' | b'^' | b'_' => Mode::String,
                    0x1b => Mode::Escape,
                    _ => Mode::Ground,
                },
                Mode::Osc if byte == 7 => {
                    self.finish(&mut latest);
                    Mode::Ground
                }
                Mode::Osc if byte == 0x1b => Mode::OscEscape,
                Mode::Osc if byte == 0x18 || byte == 0x1a => {
                    self.osc.clear();
                    Mode::Ground
                }
                Mode::Osc
                    if byte < 0x20 || byte == 0x7f || self.osc.len() >= MAX_TITLE_BYTES + 2 =>
                {
                    self.osc.clear();
                    Mode::IgnoreOsc
                }
                Mode::Osc => {
                    self.osc.push(byte);
                    Mode::Osc
                }
                Mode::OscEscape if byte == b'\\' => {
                    self.finish(&mut latest);
                    Mode::Ground
                }
                Mode::OscEscape => {
                    self.osc.clear();
                    match byte {
                        7 | 0x18 | 0x1a => Mode::Ground,
                        0x1b => Mode::IgnoreOscEscape,
                        _ => Mode::IgnoreOsc,
                    }
                }
                Mode::IgnoreOsc => match byte {
                    7 | 0x18 | 0x1a => Mode::Ground,
                    0x1b => Mode::IgnoreOscEscape,
                    _ => Mode::IgnoreOsc,
                },
                Mode::IgnoreOscEscape => match byte {
                    b'\\' | 7 | 0x18 | 0x1a => Mode::Ground,
                    0x1b => Mode::IgnoreOscEscape,
                    _ => Mode::IgnoreOsc,
                },
                Mode::String => match byte {
                    0x18 | 0x1a => Mode::Ground,
                    0x1b => Mode::StringEscape,
                    _ => Mode::String,
                },
                Mode::StringEscape => match byte {
                    b'\\' | 0x18 | 0x1a => Mode::Ground,
                    0x1b => Mode::StringEscape,
                    _ => Mode::String,
                },
            };
        }
        latest
    }

    fn finish(&mut self, latest: &mut Option<Option<String>>) {
        if let Some(bytes) = self
            .osc
            .strip_prefix(b"0;")
            .or_else(|| self.osc.strip_prefix(b"2;"))
        {
            if let Ok(text) = std::str::from_utf8(bytes) {
                if !text.chars().any(char::is_control) {
                    let title = without_spinner_glyphs(text.trim());
                    *latest = Some((!title.is_empty()).then(|| title.to_owned()));
                }
            }
        }
        self.osc.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_action_markers_are_activity_items_rather_than_task_text() {
        assert!(codex_action_required("[ ! ] Action Required | project"));
        assert!(codex_action_required("[ . ] Action Required | project"));
        for title in [
            "| [ ! ] Action Required | project",
            "Fix [ ! ] Action Required | project",
            "Task | [ ! ] Action Required | project",
            "| project",
        ] {
            assert!(!codex_action_required(title), "{title}");
        }
    }

    #[test]
    fn titles_survive_every_byte_boundary_and_both_terminators() {
        for sequence in ["\x1b]0;修复界面\x07", "\x1b]2;修复界面\x1b\\"] {
            for split in 0..sequence.len() {
                let mut parser = Parser::default();
                assert_eq!(parser.feed(&sequence.as_bytes()[..split]), None);
                assert_eq!(
                    parser.feed(&sequence.as_bytes()[split..]),
                    Some(Some("修复界面".into()))
                );
            }
        }
        let mut parser = Parser::default();
        assert_eq!(parser.feed(b"\x1b]2;old\x07\x1b]2; \x07"), Some(None));
    }

    #[test]
    fn leading_spinner_glyphs_are_not_part_of_the_title() {
        let mut parser = Parser::default();
        for (sent, kept) in [
            ("\x1b]2;◐ Fix the build\x07", Some("Fix the build")),
            ("\x1b]2;◑ Fix the build\x07", Some("Fix the build")),
            ("\x1b]2;⠙⠹  Build\x07", Some("Build")),
            ("\x1b]2;✳\x07", None),
            ("\x1b]2;●Recording\x07", Some("●Recording")),
            ("\x1b]2;Build ◐ done\x07", Some("Build ◐ done")),
        ] {
            assert_eq!(
                parser.feed(sent.as_bytes()),
                Some(kept.map(str::to_owned)),
                "{sent:?}"
            );
        }
    }

    #[test]
    fn other_strings_and_invalid_titles_cannot_change_the_title() {
        for sequence in [
            &b"\x1b]1;icon\x07"[..],
            b"\x1b]52;clipboard\x07",
            b"\x1bP\x1b]2;inside DCS\x07\x1b\\",
            b"\x1b]2;bad\ncontrol\x07",
            b"\x1b]2;bad\xc2\x85control\x07",
            b"\x1b]2;bad\xff\x07",
            b"\x1b]2;cancelled\x18",
            b"\x1b]2;bad\x1b[31m\x07",
        ] {
            let mut parser = Parser::default();
            assert_eq!(parser.feed(sequence), None, "{sequence:?}");
            assert_eq!(parser.feed(b"\x1b]2;next\x07"), Some(Some("next".into())));
        }
        let mut parser = Parser::default();
        let oversized = format!("\x1b]2;{}", "x".repeat(MAX_TITLE_BYTES + 1));
        assert_eq!(parser.feed(oversized.as_bytes()), None);
        assert!(parser.osc.len() <= MAX_TITLE_BYTES + 2);
        assert_eq!(parser.feed(b"\x1b]2;hidden\x07"), None);
        assert_eq!(parser.feed(b"\x1b]2;next\x07"), Some(Some("next".into())));
    }
}
