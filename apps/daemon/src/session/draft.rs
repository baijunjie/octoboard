//! A conservative mirror of input, used only to match actual submitted prompts to edit generations.

use std::collections::VecDeque;

use crate::protocol::Agent;

#[derive(Default)]
pub(super) struct Draft {
    edited: u64,
    submitted: u64,
    submissions: VecDeque<(Option<u64>, String)>,
    text: Option<Vec<u8>>,
    started: bool,
    paste: bool,
    escape: Vec<u8>,
    overflowed: bool,
    utf8_remaining: u8,
}

impl Draft {
    pub(super) fn input(&mut self, data: &[u8]) {
        if !self.started {
            self.text = Some(Vec::new());
            self.started = true;
        }
        for &byte in data {
            self.edited += 1;
            self.utf8_remaining = match byte {
                0xc2..=0xdf => 1,
                0xe0..=0xef => 2,
                0xf0..=0xf4 => 3,
                0x80..=0xbf => self.utf8_remaining.saturating_sub(1),
                _ => 0,
            };
            if byte == 0x1b || !self.escape.is_empty() {
                self.escape.push(byte);
                match self.escape.as_slice() {
                    b"\x1b[200~" => self.paste = true,
                    b"\x1b[201~" => self.paste = false,
                    b"\x1b\r" if !self.paste => self.append(b'\n'),
                    sequence
                        if b"\x1b[200~".starts_with(sequence)
                            || b"\x1b[201~".starts_with(sequence) =>
                    {
                        continue
                    }
                    sequence if incomplete_escape(sequence) => {
                        if self.escape.len() > 128 {
                            self.escape.truncate(2);
                        }
                        continue;
                    }
                    _ => self.text = None,
                }
                self.escape.clear();
                continue;
            }
            if self.paste {
                self.append(if byte == b'\r' { b'\n' } else { byte });
                continue;
            }
            match byte {
                b'\r' => {
                    if let Some(text) = self
                        .text
                        .take()
                        .and_then(|text| String::from_utf8(text).ok())
                    {
                        if !text.is_empty() && !text.starts_with('/') {
                            self.submission(Some(self.edited), text);
                        }
                    }
                    // If this Enter did not submit, the next candidate will not match the hook's
                    // full prompt. An unknown edit can recover after a subsequent simple prompt.
                    self.text = Some(Vec::new());
                }
                0x08 | 0x7f => {
                    if let Some(text) = &mut self.text {
                        while text.pop().is_some_and(|byte| byte & 0xc0 == 0x80) {}
                    }
                }
                0x15 => {
                    if let Some(text) = &mut self.text {
                        let start = text
                            .iter()
                            .rposition(|&byte| byte == b'\n')
                            .map_or(0, |at| at + 1);
                        text.truncate(start);
                    }
                }
                b'\n' | b'\t' | 0x20..=0xff => self.append(byte),
                _ => self.text = None,
            }
        }
    }

    fn append(&mut self, byte: u8) {
        if let Some(text) = &mut self.text {
            if text.len() < 1024 * 1024 {
                text.push(byte);
            } else {
                self.text = None;
            }
        }
    }

    pub(super) fn submission(&mut self, generation: Option<u64>, text: String) {
        if self.submissions.len() < 256
            && self
                .submissions
                .iter()
                .map(|(_, text)| text.len())
                .sum::<usize>()
                + text.len()
                <= 1024 * 1024
            && !self.overflowed
        {
            self.submissions.push_back((generation, text));
        } else {
            // Losing an origin could make an old program prompt acknowledge a later user edit.
            self.overflowed = true;
        }
    }

    pub(super) fn prompt_sent(&mut self, text: Option<&str>, agent: Agent) {
        if self.overflowed {
            return;
        }
        let Some(at) = text.and_then(|text| {
            self.submissions.iter().position(|(_, candidate)| {
                let candidate = match agent {
                    Agent::Claude => crate::hooks::without_paste_wrapper(candidate),
                    Agent::Codex => candidate.trim(),
                    Agent::Grok => candidate,
                };
                candidate == text
            })
        }) else {
            return;
        };
        if let Some((Some(generation), _)) = self.submissions.remove(at) {
            self.submitted = self.submitted.max(generation);
        }
    }

    pub(super) fn at_input_boundary(&self) -> bool {
        !self.paste && self.escape.is_empty() && self.utf8_remaining == 0
    }

    pub(super) fn holds(&self) -> bool {
        self.overflowed || self.edited > self.submitted
    }
}

fn incomplete_escape(sequence: &[u8]) -> bool {
    match sequence {
        b"\x1b" | b"\x1b[" | b"\x1bO" => true,
        [0x1b, b'[', rest @ ..] => rest.iter().all(|byte| (0x20..=0x3f).contains(byte)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_partial_paste_escape_or_utf8_character_cannot_be_taken_over() {
        for (first, rest) in [
            (&b"\x1b[200~draft"[..], &b"\x1b[201~"[..]),
            (&b"\x1b[1;"[..], &b"5D"[..]),
            (&b"\x1bO"[..], &b"D"[..]),
            (&[0xe7, 0x8c][..], &[0xab][..]),
        ] {
            let mut draft = Draft::default();
            draft.input(first);
            assert!(!draft.at_input_boundary());
            draft.input(rest);
            assert!(draft.at_input_boundary());
        }
    }

    #[test]
    fn only_a_matching_submission_acknowledges_the_input_before_its_enter() {
        let mut draft = Draft::default();
        draft.submission(None, "program".to_string());
        draft.input(b"first\rnext draft");
        draft.prompt_sent(Some("program"), Agent::Grok);
        draft.prompt_sent(Some("background notification"), Agent::Grok);
        assert!(draft.holds());
        draft.prompt_sent(Some("first"), Agent::Grok);
        assert!(draft.holds(), "typing after Enter is a new draft");
        draft.input(b"\r");
        draft.prompt_sent(Some("next draft"), Agent::Grok);
        assert!(!draft.holds());
    }

    #[test]
    fn paste_and_enter_in_one_chunk_can_be_acknowledged_but_embedded_enters_cannot() {
        let mut draft = Draft::default();
        for chunk in [&b"\x1b[20"[..], b"0~line one\rline two\x1b[201~"] {
            draft.input(chunk);
        }
        draft.prompt_sent(Some("line one\nline two"), Agent::Grok);
        assert!(draft.holds());
        draft.input(b"\r");
        draft.prompt_sent(Some("line one\nline two"), Agent::Grok);
        assert!(!draft.holds());
        draft.input(b"\x1b[200~next\nlines\x1b[201~\r");
        draft.prompt_sent(Some("next\nlines"), Agent::Grok);
        assert!(!draft.holds());
    }

    #[test]
    fn non_submitting_enters_and_unknown_edits_do_not_make_later_submissions_lag() {
        let mut draft = Draft::default();
        draft.input(b"\r/model\rhello\x1b[A\r");
        draft.prompt_sent(Some("a recalled prompt"), Agent::Grok);
        assert!(draft.holds());
        draft.input(b"a simple prompt\r");
        draft.prompt_sent(Some("a simple prompt"), Agent::Grok);
        assert!(!draft.holds());
    }

    #[test]
    fn clearing_a_draft_does_not_acknowledge_it_and_alt_enter_inserts_a_newline() {
        let mut draft = Draft::default();
        draft.input(b"draft\x15");
        draft.prompt_sent(Some("draft"), Agent::Grok);
        assert!(draft.holds());
        draft.input(b"first\x1b\rsecond\r");
        draft.prompt_sent(Some("first\nsecond"), Agent::Grok);
        assert!(!draft.holds());
    }

    #[test]
    fn backspace_removes_one_unicode_character() {
        let mut draft = Draft::default();
        draft.input("hi猫".as_bytes());
        draft.input(b"\x7f!\r");
        draft.prompt_sent(Some("hi!"), Agent::Grok);
        assert!(!draft.holds());
    }

    #[test]
    fn excess_candidates_stay_bounded_and_cannot_be_acknowledged_out_of_order() {
        let mut draft = Draft::default();
        for _ in 0..1000 {
            draft.input(b"ignored\r");
        }
        assert_eq!(draft.submissions.len(), 256);
        draft.prompt_sent(Some("ignored"), Agent::Grok);
        assert!(draft.holds());
    }

    #[test]
    fn codex_trims_only_outer_whitespace_when_matching_a_pasted_submission() {
        let mut draft = Draft::default();
        draft.input(b"\x1b[200~\n  first\nsecond\n\n\x1b[201~\r");
        draft.prompt_sent(Some("first\nsecond"), Agent::Codex);
        assert!(!draft.holds());
    }

    #[test]
    fn claude_paste_wrapper_normalization_is_applied_to_candidates_too() {
        let mut draft = Draft::default();
        draft.input(b" leading space\r");
        draft.prompt_sent(Some("leading space"), Agent::Claude);
        assert!(!draft.holds());
    }
}
