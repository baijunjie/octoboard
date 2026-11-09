//! The escape sequence, or UTF-8 character, that the output has begun and not yet ended, as the
//! parser of a parsed terminal holds it: what a saved screen ends with, so that the output after
//! it parses as it would have ([`super::TerminalSnapshot::save`]).
//!
//! [`Unended`] follows the state machine of the parser (`vte` 0.15) closely enough to tell where a
//! sequence begins and ends, and keeps its bytes: fed to a parser in its ground state, they leave
//! it in the state the original one is in. Only `ESC` starts a sequence in the ground state, and
//! an `ESC` starts a new one in every other state, whatever it interrupts, so a chunk is read from
//! its last `ESC` on, found by a fast search; in the ground state only the end of a chunk, where a
//! UTF-8 character may be cut short, is looked at.

/// The most bytes of an unended sequence kept. A longer one leaves nothing to save until it ends:
/// such a sequence is an image or a clipboard transfer, which no program leaves half written.
const LONGEST_KEPT: usize = 64 * 1024;

const ESC: u8 = 0x1b;

/// What a control character the parser executes in the middle of a sequence is kept as: the
/// parser ignores it there, in every state where it executes controls, while the daemon's query
/// scanner, like the control, takes it for the end of the sequence it may be holding back, so the
/// scanner of a restored screen passes the sequence on as the original one did.
const DEL: u8 = 0x7f;

/// The parser's states, as far as where a sequence ends depends on them: the states of a CSI
/// sequence all end it alike, those of a DCS sequence's head do not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum State {
    #[default]
    Ground,
    Escape,
    EscapeIntermediate,
    Csi,
    DcsEntry,
    DcsParam,
    DcsIntermediate,
    DcsIgnore,
    DcsPassthrough,
    Osc,
    /// An SOS, PM or APC string, which only `CAN`, `SUB` and `ESC` end.
    SosPmApc,
}

/// What a byte does to the sequence it comes in.
enum Step {
    /// The byte is part of the sequence, which goes on in the state given.
    Keep(State),
    /// The parser executes the byte where it is, which leaves the sequence as it was.
    Execute,
    /// The byte ends the sequence.
    End,
}

/// The sequence the output has begun and not ended; see the module's documentation.
#[derive(Debug, Default)]
pub(super) struct Unended {
    state: State,
    /// The sequence from its `ESC`, or, in the ground state, the bytes of a UTF-8 character cut
    /// short.
    kept: Vec<u8>,
    /// The sequence grew past [`LONGEST_KEPT`]: nothing is kept of it until it ends.
    overflowed: bool,
}

impl Unended {
    /// Follows `bytes`, the next the parser takes in.
    pub(super) fn advance(&mut self, bytes: &[u8]) {
        let rest = match last_escape(bytes) {
            Some(at) => {
                self.begin();
                &bytes[at + 1..]
            }
            None => bytes,
        };
        for (at, &byte) in rest.iter().enumerate() {
            if self.state == State::Ground {
                // No `ESC` follows, so nothing starts a sequence again.
                self.ground(&rest[at..]);
                return;
            }
            self.step(byte);
        }
    }

    /// The bytes that take a parser in its ground state to the state the original one is in;
    /// `None` while the sequence is too long to keep.
    pub(super) fn bytes(&self) -> Option<&[u8]> {
        (!self.overflowed).then_some(self.kept.as_slice())
    }

    /// An `ESC` starts a sequence afresh, whatever it interrupts.
    fn begin(&mut self) {
        self.end();
        self.state = State::Escape;
        self.kept.push(ESC);
    }

    fn end(&mut self) {
        self.state = State::Ground;
        self.overflowed = false;
        if self.kept.capacity() > 4096 {
            self.kept = Vec::new();
        } else {
            self.kept.clear();
        }
    }

    fn keep(&mut self, byte: u8) {
        if self.overflowed {
            return;
        }
        if self.kept.len() >= LONGEST_KEPT {
            self.overflowed = true;
            self.kept = Vec::new();
            return;
        }
        self.kept.push(byte);
    }

    /// Takes `bytes`, which hold no `ESC`, in the ground state: all that is left of them is a
    /// UTF-8 character their end may cut short, which may also complete or break the one kept
    /// from before.
    fn ground(&mut self, bytes: &[u8]) {
        // A character cut short is at most three bytes long, so it lies within the last three.
        let mut window = [0u8; 3];
        let len = if bytes.len() >= window.len() {
            let tail = bytes.len() - window.len();
            window.copy_from_slice(&bytes[tail..]);
            window.len()
        } else {
            let from_kept = self.kept.len().min(window.len() - bytes.len());
            window[..from_kept].copy_from_slice(&self.kept[self.kept.len() - from_kept..]);
            window[from_kept..from_kept + bytes.len()].copy_from_slice(bytes);
            from_kept + bytes.len()
        };
        let partial = cut_short(&window[..len]);
        self.kept.clear();
        self.kept.extend_from_slice(partial);
    }

    /// Takes one byte, not `ESC`, outside the ground state.
    fn step(&mut self, byte: u8) {
        if byte == ESC {
            return self.begin();
        }
        let step = if matches!(byte, 0x18 | 0x1a) {
            // `CAN` and `SUB` abort every sequence.
            Step::End
        } else {
            self.step_in_state(byte)
        };
        match step {
            Step::Keep(state) => {
                self.state = state;
                self.keep(byte);
            }
            Step::Execute => {
                if self.kept.last() != Some(&DEL) {
                    self.keep(DEL);
                }
            }
            Step::End => self.end(),
        }
    }

    fn step_in_state(&self, byte: u8) -> Step {
        use State::*;
        let control = matches!(byte, 0x00..=0x17 | 0x19 | 0x1c..=0x1f);
        match self.state {
            Ground => unreachable!("the ground state is taken a run at a time"),
            Escape => match byte {
                _ if control => Step::Execute,
                0x20..=0x2f => Step::Keep(EscapeIntermediate),
                b'P' => Step::Keep(DcsEntry),
                b'[' => Step::Keep(Csi),
                b']' => Step::Keep(Osc),
                b'X' | b'^' | b'_' => Step::Keep(SosPmApc),
                0x30..=0x7e => Step::End,
                // `DEL` and bytes past ASCII are ignored.
                _ => Step::Keep(Escape),
            },
            EscapeIntermediate => match byte {
                _ if control => Step::Execute,
                0x30..=0x7e => Step::End,
                _ => Step::Keep(EscapeIntermediate),
            },
            Csi => match byte {
                _ if control => Step::Execute,
                0x40..=0x7e => Step::End,
                _ => Step::Keep(Csi),
            },
            // The head of a DCS sequence ignores controls; past an out-of-place parameter or
            // intermediate byte it ignores the whole sequence, its final byte included.
            DcsEntry => match byte {
                0x20..=0x2f => Step::Keep(DcsIntermediate),
                0x30..=0x3f => Step::Keep(DcsParam),
                0x40..=0x7e => Step::Keep(DcsPassthrough),
                _ => Step::Keep(DcsEntry),
            },
            DcsParam => match byte {
                0x20..=0x2f => Step::Keep(DcsIntermediate),
                0x3c..=0x3f => Step::Keep(DcsIgnore),
                0x40..=0x7e => Step::Keep(DcsPassthrough),
                _ => Step::Keep(DcsParam),
            },
            DcsIntermediate => match byte {
                0x30..=0x3f => Step::Keep(DcsIgnore),
                0x40..=0x7e => Step::Keep(DcsPassthrough),
                _ => Step::Keep(DcsIntermediate),
            },
            DcsIgnore => Step::Keep(DcsIgnore),
            // The 8-bit `ST` ends a DCS string, and no other.
            DcsPassthrough if byte == 0x9c => Step::End,
            DcsPassthrough => Step::Keep(DcsPassthrough),
            Osc if byte == 0x07 => Step::End,
            Osc => Step::Keep(Osc),
            SosPmApc => Step::Keep(SosPmApc),
        }
    }
}

/// The UTF-8 character `tail`, the last bytes of a run of plain output, ends cut short: its bytes
/// so far, or none.
fn cut_short(tail: &[u8]) -> &[u8] {
    // A character starts at a byte that cannot continue one, which an error never swallows.
    let Some(start) = tail.iter().rposition(|byte| !(0x80..=0xbf).contains(byte)) else {
        return &[];
    };
    let candidate = &tail[start..];
    match std::str::from_utf8(candidate) {
        Err(err) if err.valid_up_to() == 0 && err.error_len().is_none() => candidate,
        _ => &[],
    }
}

/// Where the last `ESC` in `bytes` is, searched a machine word at a time.
fn last_escape(bytes: &[u8]) -> Option<usize> {
    const LANES: usize = std::mem::size_of::<usize>();
    const ONES: usize = usize::MAX / 0xff;
    const HIGHS: usize = ONES << 7;
    const ESCAPES: usize = ONES * ESC as usize;
    let mut end = bytes.len();
    while end >= LANES {
        let start = end - LANES;
        let word = usize::from_ne_bytes(bytes[start..end].try_into().expect("a word of bytes"));
        // Zero in each byte that is an `ESC`; the test finds whether any byte is zero.
        let matched = word ^ ESCAPES;
        if matched.wrapping_sub(ONES) & !matched & HIGHS != 0 {
            return bytes[start..end]
                .iter()
                .rposition(|&byte| byte == ESC)
                .map(|at| start + at);
        }
        end = start;
    }
    bytes[..end].iter().rposition(|&byte| byte == ESC)
}

#[cfg(test)]
mod tests {
    use super::{last_escape, Unended, LONGEST_KEPT};

    /// What is kept once `chunks` have been followed one after another.
    fn kept(chunks: &[&[u8]]) -> Option<Vec<u8>> {
        let mut unended = Unended::default();
        for chunk in chunks {
            unended.advance(chunk);
        }
        unended.bytes().map(<[u8]>::to_vec)
    }

    /// `stream` followed whole and cut at every point gives the same.
    fn kept_wherever_cut(stream: &[u8]) -> Vec<u8> {
        let whole = kept(&[stream]).expect("short enough to keep");
        for at in 0..=stream.len() {
            let (head, tail) = stream.split_at(at);
            assert_eq!(
                kept(&[head, tail]).as_deref(),
                Some(whole.as_slice()),
                "cut at {at}"
            );
        }
        whole
    }

    #[test]
    fn the_last_escape_is_found_wherever_it_is() {
        for len in 0..40 {
            let mut bytes = vec![b'a'; len];
            assert_eq!(last_escape(&bytes), None);
            for at in 0..len {
                bytes[at] = 0x1b;
                assert_eq!(last_escape(&bytes), Some(at), "{len} bytes");
            }
        }
        assert_eq!(last_escape(b"\x1b\x9b\x1a\xff"), Some(0));
    }

    #[test]
    fn plain_output_and_ended_sequences_leave_nothing() {
        for stream in [
            &b"plain text\r\n"[..],
            b"\x1b[31mred\x1b[0m",
            b"\x1b]0;title\x07",
            b"\x1b]8;;https://example.com\x1b\\link",
            b"\x1bPq#0;2;0;0;0\x1b\\",
            b"\x1b_application\x1b\\",
            b"\x1b(B\x1b7\x1bc",
            "caf\u{e9} \u{1f600}".as_bytes(),
        ] {
            assert!(
                kept_wherever_cut(stream).is_empty(),
                "{:?}",
                String::from_utf8_lossy(stream)
            );
        }
    }

    #[test]
    fn each_kind_of_sequence_is_kept_from_its_escape_across_chunks() {
        for (stream, unended) in [
            (&b"text\x1b[3"[..], &b"\x1b[3"[..]),
            (b"\x1b[38;5;1", b"\x1b[38;5;1"),
            (b"\x1b[?2026", b"\x1b[?2026"),
            (b"\x1b]0;a ti", b"\x1b]0;a ti"),
            (b"\x1b]8;;https://exa", b"\x1b]8;;https://exa"),
            // The `ESC` of an `ST` to come.
            (b"\x1b]0;title\x1b", b"\x1b"),
            (b"\x1bPq#0;2", b"\x1bPq#0;2"),
            (b"\x1b_Gf=24", b"\x1b_Gf=24"),
            (b"\x1b^private", b"\x1b^private"),
            (b"\x1b(", b"\x1b("),
            (b"\x1b", b"\x1b"),
            // An `ESC` starts a sequence again, whatever it interrupts.
            (b"\x1b]0;x\x1b[1", b"\x1b[1"),
            (b"\x1b\x1b", b"\x1b"),
            // `BEL` ends a command string, but not an APC string.
            (b"\x1b_x\x07y", b"\x1b_x\x07y"),
        ] {
            assert_eq!(
                kept_wherever_cut(stream),
                unended,
                "{:?}",
                String::from_utf8_lossy(stream)
            );
        }
    }

    #[test]
    fn a_control_executed_inside_a_sequence_is_kept_as_one_ignored_byte() {
        assert_eq!(kept_wherever_cut(b"\x1b[3\n\r1"), b"\x1b[3\x7f1");
        assert_eq!(kept_wherever_cut(b"\x1b\x08("), b"\x1b\x7f(");
        // Inside a string controls are ignored where they are, and kept as they are.
        assert_eq!(kept_wherever_cut(b"\x1b]0;a\nb"), b"\x1b]0;a\nb");
        assert_eq!(kept_wherever_cut(b"\x1bPq\r\n"), b"\x1bPq\r\n");
    }

    #[test]
    fn can_and_sub_abort_a_sequence() {
        for stream in [
            &b"\x1b[31\x18"[..],
            b"\x1b]0;title\x1a",
            b"\x1bPq#0\x18",
            b"\x1b_x\x18",
            b"\x1b(\x1a",
            b"\x1b\x18",
        ] {
            assert!(
                kept_wherever_cut(stream).is_empty(),
                "{:?}",
                String::from_utf8_lossy(stream)
            );
        }
    }

    #[test]
    fn the_head_of_a_dcs_sequence_decides_whether_its_final_byte_starts_the_string() {
        // An out-of-place byte makes the parser ignore the whole sequence: its final byte does
        // not start a string, and neither does the 8-bit `ST` end it.
        assert_eq!(kept_wherever_cut(b"\x1bP1<q\x9c"), b"\x1bP1<q\x9c");
        assert!(kept_wherever_cut(b"\x1bP1q\x9c").is_empty());
        assert!(kept_wherever_cut(b"\x1bP$qm\x9c").is_empty());
        assert_eq!(kept_wherever_cut(b"\x1bP$0q\x9c"), b"\x1bP$0q\x9c");
    }

    #[test]
    fn a_utf8_character_cut_short_is_kept_until_it_ends() {
        let euro = "\u{20ac}".as_bytes();
        assert_eq!(kept(&[b"ab", &euro[..1]]).unwrap(), &euro[..1]);
        assert_eq!(kept(&[&euro[..1], &euro[1..2]]).unwrap(), &euro[..2]);
        assert!(kept(&[&euro[..1], &euro[1..]]).unwrap().is_empty());
        let face = "\u{1f600}".as_bytes();
        assert_eq!(
            kept(&[&face[..1], &face[1..2], &face[2..3]]).unwrap(),
            &face[..3]
        );
        assert!(kept(&[&face[..3], &face[3..]]).unwrap().is_empty());
        // A byte that cannot continue it breaks it, and may start another.
        assert!(kept(&[&euro[..1], b"a"]).unwrap().is_empty());
        assert_eq!(kept(&[&euro[..1], &face[..2]]).unwrap(), &face[..2]);
        // Bytes that start no character, or one that cannot be, are never kept.
        assert!(kept(&[b"\x80\x80"]).unwrap().is_empty());
        assert!(kept(&[b"\xc0"]).unwrap().is_empty());
        assert!(kept(&[b"\xe0\x80"]).unwrap().is_empty());
        // An `ESC` ends it, as the parser prints a replacement for it.
        assert_eq!(kept(&[&euro[..2], b"\x1b["]).unwrap(), b"\x1b[");
    }

    #[test]
    fn a_sequence_too_long_to_keep_leaves_nothing_to_save_until_it_ends() {
        let mut unended = Unended::default();
        unended.advance(b"\x1bPq");
        unended.advance(&vec![b'#'; LONGEST_KEPT - 3]);
        assert_eq!(unended.bytes().map(<[u8]>::len), Some(LONGEST_KEPT));
        unended.advance(b"#");
        assert_eq!(unended.bytes(), None);
        unended.advance(&vec![b'#'; 1000]);
        assert_eq!(unended.bytes(), None, "still in the sequence");
        unended.advance(b"\x1b\\");
        assert_eq!(unended.bytes(), Some(&b""[..]), "ended");

        unended.advance(b"\x1b]52;c;");
        unended.advance(&vec![b'A'; LONGEST_KEPT]);
        assert_eq!(unended.bytes(), None);
        unended.advance(b"\x1b[1");
        assert_eq!(
            unended.bytes(),
            Some(&b"\x1b[1"[..]),
            "an escape starts a sequence afresh"
        );
    }
}
