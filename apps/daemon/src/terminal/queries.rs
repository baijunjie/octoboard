//! Finding the program's terminal queries in its output, so that the daemon can answer them
//! itself and take them out of what the attached clients receive.
//!
//! A program asks its terminal things through its output — the cursor position (`CSI 6 n`), the
//! background colour (`OSC 11 ; ?`), the device attributes (`CSI c`) — and reads the answer back
//! as input. [`QueryScanner`] cuts the output into pieces: plain output, and the whole escape
//! sequences of the queries the daemon may answer, each named. A sequence cut across two reads is
//! held back until its end arrives, so a query is always found whole; what it holds back is never
//! more than one escape sequence, which draws nothing until it is complete: a short one, a shell's
//! prompt start (OSC 133 A) of up to 128 bytes, or a program's status report (OSC 7501) up to its
//! limit of [`LONGEST_REPORT`](crate::terminal::program_status::LONGEST_REPORT) bytes.
//!
//! Only the shapes the daemon answers or reads are recognised. Anything else, an escape sequence
//! with a control character inside it or one longer than any of them, is plain output and
//! passes through to the clients untouched, for one of them to answer if it is a query at all.
//! Like the parser of the daemon's terminal (and of every client's), an `ESC` always starts a new
//! sequence, whatever it interrupts.

use crate::terminal::program_status::LONGEST_REPORT;

/// A dynamic colour a program can ask for (`OSC 10`, `OSC 11`, `OSC 12`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynamicColor {
    Foreground,
    Background,
    Cursor,
}

/// What a recognised escape sequence is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Query {
    /// The primary device attributes (`CSI c`, `CSI 0 c`).
    DeviceAttributes,
    /// The secondary device attributes (`CSI > c`, `CSI > 0 c`).
    SecondaryAttributes,
    /// The device status report (`CSI 5 n`).
    Status,
    /// The cursor position report (`CSI 6 n`).
    CursorPosition,
    /// Whether the colour scheme is dark or light (`CSI ? 996 n`).
    ColorScheme,
    /// A mode's state (DECRQM: `CSI Ps $ p`, or `CSI ? Ps $ p` with `private`).
    Mode { private: bool, mode: u16 },
    /// The text area's size in characters (`CSI 18 t`).
    TextAreaCells,
    /// The text area's size in pixels (`CSI 14 t`).
    TextAreaPixels,
    /// A cell's size in pixels (`CSI 16 t`).
    CellPixels,
    /// A dynamic colour (`OSC 1x ; ?`), ended with `BEL` or with `ESC \` (`st`).
    Color { color: DynamicColor, st: bool },
    /// The Program Status Protocol's support query (`OSC 7501 ; ?`), ended with `BEL` or with
    /// `ESC \` (`st`).
    ProgramStatusSupport { st: bool },
    /// Not a query: a program's status report (`OSC 7501 ; …`), with its body, the bytes between
    /// `OSC 7501 ;` and the `ST`; see [`crate::terminal::program_status`].
    ProgramStatusReport(Vec<u8>),
    /// Not a query: a shell prompt starting (OSC 133 A, FinalTerm's prompt marking, with or
    /// without options), which ends what running programs reported about themselves; see
    /// [`crate::terminal::program_status::ProgramStatus::end_volatile`].
    PromptStart,
    /// Not a query: private modes set (`CSI ? Pm h`) or reset (`CSI ? Pm l`), which the daemon
    /// reads for the modes its terminal does not keep (colour scheme updates, mode 2031).
    PrivateModes { set: bool, modes: Vec<u16> },
}

/// A piece of the output, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Output(Vec<u8>),
    Sequence(Query, Vec<u8>),
}

/// The longest escape sequence held back while it may still become a recognised one, other than a
/// prompt start ([`LONGEST_PROMPT_START`]) and a program's status report ([`LONGEST_REPORT`]);
/// past it the sequence is plain output.
const LONGEST: usize = 48;

/// What a program's status report starts with.
const STATUS_REPORT: &[u8] = b"\x1b]7501;";

/// What a prompt start (OSC 133 A) starts with.
const PROMPT_START: &[u8] = b"\x1b]133;A";
/// The longest prompt start, its options included, held back; a longer one is plain output.
const LONGEST_PROMPT_START: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum State {
    #[default]
    Ground,
    /// After `ESC`.
    Escape,
    /// In `ESC [`.
    Csi,
    /// In `ESC ]`.
    Osc,
    /// In `ESC ]`, after an `ESC` that may begin its `ST`.
    OscEscape,
}

/// Cuts output into [`Piece`]s, holding back a sequence that a read cut short.
#[derive(Debug, Default)]
pub struct QueryScanner {
    state: State,
    /// The sequence being read, from its `ESC`.
    held: Vec<u8>,
}

impl QueryScanner {
    /// The pieces of `bytes`, with whatever the last call held back in front of them. A sequence
    /// still open at the end is held back for the next call.
    pub fn scan(&mut self, bytes: &[u8]) -> Vec<Piece> {
        let mut pieces = Vec::new();
        let mut output = Vec::new();
        for &byte in bytes {
            self.step(byte, &mut output, &mut pieces);
        }
        if !output.is_empty() {
            pieces.push(Piece::Output(output));
        }
        pieces
    }

    /// The bytes held back, which no piece has carried yet. Fed to a fresh scanner, they leave it
    /// holding back the same, in the same state: they start with the `ESC` that starts the
    /// sequence, which the scanner always takes afresh.
    pub fn held(&self) -> &[u8] {
        &self.held
    }

    fn step(&mut self, byte: u8, output: &mut Vec<u8>, pieces: &mut Vec<Piece>) {
        match self.state {
            State::Ground => {
                if byte == 0x1b {
                    self.held.push(byte);
                    self.state = State::Escape;
                } else {
                    output.push(byte);
                }
            }
            State::Escape => match byte {
                b'[' => {
                    self.held.push(byte);
                    self.state = State::Csi;
                }
                b']' => {
                    self.held.push(byte);
                    self.state = State::Osc;
                }
                _ => self.mismatch(byte, output, pieces),
            },
            State::Csi => {
                let at = self.held.len() - 2;
                let fits = match byte {
                    b'?' | b'>' => at == 0,
                    b'0'..=b'9' | b';' => !self.held.ends_with(b"$"),
                    b'$' => !self.held.ends_with(b"$"),
                    0x40..=0x7e => {
                        self.held.push(byte);
                        let sequence = std::mem::take(&mut self.held);
                        self.state = State::Ground;
                        match csi_query(&sequence[2..]) {
                            Some(query) => Self::emit(output, pieces, query, sequence),
                            None => output.extend_from_slice(&sequence),
                        }
                        return;
                    }
                    _ => false,
                };
                if fits && self.held.len() < LONGEST {
                    self.held.push(byte);
                } else {
                    self.mismatch(byte, output, pieces);
                }
            }
            State::Osc => match byte {
                0x07 => {
                    self.held.push(byte);
                    self.end_osc(false, output, pieces);
                }
                0x1b => {
                    self.held.push(byte);
                    self.state = State::OscEscape;
                }
                // A report's body is read by `program_status::parse`, which skips a pair with a
                // byte outside its sets: a tab or a byte of UTF-8 does not end the report.
                0x09 | 0x20..=0x7e | 0x80..=0xff
                    if self.held.starts_with(STATUS_REPORT) && self.held.len() < LONGEST_REPORT =>
                {
                    self.held.push(byte)
                }
                b'A' if self.held.as_slice() == &PROMPT_START[..PROMPT_START.len() - 1] => {
                    self.held.push(byte)
                }
                0x20..=0x7e
                    if self.held.starts_with(PROMPT_START)
                        && self.held.len() < LONGEST_PROMPT_START =>
                {
                    self.held.push(byte)
                }
                b'0'..=b'9' | b';' | b'?' if self.held.len() < LONGEST => self.held.push(byte),
                _ => self.mismatch(byte, output, pieces),
            },
            State::OscEscape => {
                if byte == b'\\' {
                    self.held.push(byte);
                    self.end_osc(true, output, pieces);
                } else {
                    // The `ESC` ends the string unanswered and starts a sequence of its own.
                    self.held.pop();
                    output.append(&mut self.held);
                    self.held.push(0x1b);
                    self.state = State::Escape;
                    self.step(byte, output, pieces);
                }
            }
        }
    }

    /// The sequence being read is not one of those recognised: it is output, and `byte`, which
    /// broke it, is read afresh, as it may start a sequence of its own.
    fn mismatch(&mut self, byte: u8, output: &mut Vec<u8>, pieces: &mut Vec<Piece>) {
        output.append(&mut self.held);
        self.state = State::Ground;
        self.step(byte, output, pieces);
    }

    fn end_osc(&mut self, st: bool, output: &mut Vec<u8>, pieces: &mut Vec<Piece>) {
        let sequence = std::mem::take(&mut self.held);
        self.state = State::Ground;
        let body = &sequence[2..sequence.len() - if st { 2 } else { 1 }];
        let query = match body {
            b"10;?" => Some(Query::Color {
                color: DynamicColor::Foreground,
                st,
            }),
            b"11;?" => Some(Query::Color {
                color: DynamicColor::Background,
                st,
            }),
            b"12;?" => Some(Query::Color {
                color: DynamicColor::Cursor,
                st,
            }),
            b"7501;?" => Some(Query::ProgramStatusSupport { st }),
            _ if body == b"133;A" || body.starts_with(b"133;A;") => Some(Query::PromptStart),
            // Its end may take it past the limit, which discards it: it is output then.
            _ if sequence.len() <= LONGEST_REPORT => body
                .strip_prefix(&STATUS_REPORT[2..])
                .map(|report| Query::ProgramStatusReport(report.to_vec())),
            _ => None,
        };
        match query {
            Some(query) => Self::emit(output, pieces, query, sequence),
            None => output.extend_from_slice(&sequence),
        }
    }

    fn emit(output: &mut Vec<u8>, pieces: &mut Vec<Piece>, query: Query, sequence: Vec<u8>) {
        if !output.is_empty() {
            pieces.push(Piece::Output(std::mem::take(output)));
        }
        pieces.push(Piece::Sequence(query, sequence));
    }
}

/// What the CSI sequence whose bytes after `ESC [` are `body` (its final byte last) is, among
/// those recognised.
fn csi_query(body: &[u8]) -> Option<Query> {
    let (&last, body) = body.split_last()?;
    let (marker, body) = match body.first() {
        Some(&marker @ (b'?' | b'>')) => (Some(marker), &body[1..]),
        _ => (None, body),
    };
    let (dollar, body) = match body.strip_suffix(b"$") {
        Some(body) => (true, body),
        None => (false, body),
    };
    let params = body
        .split(|&byte| byte == b';')
        .map(|param| {
            if param.is_empty() {
                Some(0)
            } else {
                std::str::from_utf8(param).ok()?.parse::<u16>().ok()
            }
        })
        .collect::<Option<Vec<u16>>>()?;
    // An empty body is one empty parameter, which counts as 0.
    let zero = params == [0];
    let query = match (marker, dollar, last) {
        (None, false, b'c') if zero => Query::DeviceAttributes,
        (Some(b'>'), false, b'c') if zero => Query::SecondaryAttributes,
        (None, false, b'n') if params == [5] => Query::Status,
        (None, false, b'n') if params == [6] => Query::CursorPosition,
        (Some(b'?'), false, b'n') if params == [996] => Query::ColorScheme,
        (None | Some(b'?'), true, b'p') if params.len() == 1 && !body.is_empty() => Query::Mode {
            private: marker.is_some(),
            mode: params[0],
        },
        (None, false, b't') if params == [18] => Query::TextAreaCells,
        (None, false, b't') if params == [14] => Query::TextAreaPixels,
        (None, false, b't') if params == [16] => Query::CellPixels,
        (Some(b'?'), false, b'h' | b'l') if !body.is_empty() => Query::PrivateModes {
            set: last == b'h',
            modes: params,
        },
        _ => return None,
    };
    Some(query)
}

#[cfg(test)]
mod tests {
    use super::{DynamicColor, Piece, Query, QueryScanner, LONGEST_REPORT};

    fn scan(bytes: &[u8]) -> Vec<Piece> {
        QueryScanner::default().scan(bytes)
    }

    fn output(bytes: &[u8]) -> Piece {
        Piece::Output(bytes.to_vec())
    }

    fn sequence(query: Query, bytes: &[u8]) -> Piece {
        Piece::Sequence(query, bytes.to_vec())
    }

    #[test]
    fn each_query_is_cut_out_of_the_output_around_it() {
        assert_eq!(
            scan(b"ab\x1b[6ncd\x1b[c\x1b[0c\x1b[>c\x1b[5n"),
            [
                output(b"ab"),
                sequence(Query::CursorPosition, b"\x1b[6n"),
                output(b"cd"),
                sequence(Query::DeviceAttributes, b"\x1b[c"),
                sequence(Query::DeviceAttributes, b"\x1b[0c"),
                sequence(Query::SecondaryAttributes, b"\x1b[>c"),
                sequence(Query::Status, b"\x1b[5n"),
            ]
        );
        assert_eq!(
            scan(b"\x1b[?996n\x1b[?2004$p\x1b[4$p\x1b[18t\x1b[14t\x1b[16t"),
            [
                sequence(Query::ColorScheme, b"\x1b[?996n"),
                sequence(
                    Query::Mode {
                        private: true,
                        mode: 2004
                    },
                    b"\x1b[?2004$p"
                ),
                sequence(
                    Query::Mode {
                        private: false,
                        mode: 4
                    },
                    b"\x1b[4$p"
                ),
                sequence(Query::TextAreaCells, b"\x1b[18t"),
                sequence(Query::TextAreaPixels, b"\x1b[14t"),
                sequence(Query::CellPixels, b"\x1b[16t"),
            ]
        );
        assert_eq!(
            scan(b"\x1b]10;?\x07\x1b]11;?\x1b\\\x1b]12;?\x07"),
            [
                sequence(
                    Query::Color {
                        color: DynamicColor::Foreground,
                        st: false
                    },
                    b"\x1b]10;?\x07"
                ),
                sequence(
                    Query::Color {
                        color: DynamicColor::Background,
                        st: true
                    },
                    b"\x1b]11;?\x1b\\"
                ),
                sequence(
                    Query::Color {
                        color: DynamicColor::Cursor,
                        st: false
                    },
                    b"\x1b]12;?\x07"
                ),
            ]
        );
    }

    #[test]
    fn private_modes_are_named_and_other_sequences_are_output() {
        assert_eq!(
            scan(b"\x1b[?1049;2031h\x1b[?2031l"),
            [
                sequence(
                    Query::PrivateModes {
                        set: true,
                        modes: vec![1049, 2031]
                    },
                    b"\x1b[?1049;2031h"
                ),
                sequence(
                    Query::PrivateModes {
                        set: false,
                        modes: vec![2031]
                    },
                    b"\x1b[?2031l"
                ),
            ]
        );
        for other in [
            &b"\x1b[31mred\x1b[0m"[..],
            b"\x1b[1;5H",
            b"\x1b[38:2:1:2:3m",
            b"\x1b[2 q",
            b"\x1b]0;title\x07",
            b"\x1b]4;1;?\x07",
            b"\x1b]10;?;?\x07",
            b"\x1b[?6n",
            b"\x1b[>q",
            b"\x1b[6;1n",
            b"\x1b[14;2t",
            b"\x1b[$p",
            b"\x1bc",
            b"\x1b(B",
        ] {
            assert_eq!(
                scan(other),
                [output(other)],
                "{:?}",
                String::from_utf8_lossy(other)
            );
        }
    }

    #[test]
    fn a_programs_status_reports_and_support_query_are_cut_out() {
        assert_eq!(
            scan(b"a\x1b]7501;?\x07\x1b]7501;?\x1b\\\x1b]7501;state=working:msg=aGk=\x1b\\b"),
            [
                output(b"a"),
                sequence(
                    Query::ProgramStatusSupport { st: false },
                    b"\x1b]7501;?\x07"
                ),
                sequence(
                    Query::ProgramStatusSupport { st: true },
                    b"\x1b]7501;?\x1b\\"
                ),
                sequence(
                    Query::ProgramStatusReport(b"state=working:msg=aGk=".to_vec()),
                    b"\x1b]7501;state=working:msg=aGk=\x1b\\"
                ),
                output(b"b"),
            ]
        );
        // A tab or UTF-8 inside a report does not end it.
        assert!(matches!(
            scan("\x1b]7501;state=idle:\tapp=\u{e9}\x07".as_bytes()).as_slice(),
            [Piece::Sequence(Query::ProgramStatusReport(_), _)]
        ));
        // A prompt start, with or without options, is named, not consumed: it is output too.
        assert_eq!(
            scan(b"\x1b]133;A\x07\x1b]133;A;cl=m;aid=7\x1b\\\x1b]133;B\x07\x1b]133;AB\x07"),
            [
                sequence(Query::PromptStart, b"\x1b]133;A\x07"),
                sequence(Query::PromptStart, b"\x1b]133;A;cl=m;aid=7\x1b\\"),
                output(b"\x1b]133;B\x07\x1b]133;AB\x07"),
            ]
        );
        let mut scanner = QueryScanner::default();
        assert_eq!(scanner.scan(b"\x1b]13"), []);
        assert_eq!(scanner.scan(b"3;A"), []);
        assert_eq!(
            scanner.scan(b"\x07"),
            [sequence(Query::PromptStart, b"\x1b]133;A\x07")]
        );
        // Other strings keep to the short shapes: a title is output however it continues.
        assert_eq!(scan(b"\x1b]0;a b\x07"), [output(b"\x1b]0;a b\x07")]);
        assert_eq!(scan(b"\x1b]75011;x\x07"), [output(b"\x1b]75011;x\x07")]);
    }

    #[test]
    fn a_status_report_is_held_back_up_to_its_limit_and_no_further() {
        let mut scanner = QueryScanner::default();
        let body = vec![b'x'; 4000];
        let report = [b"\x1b]7501;state=idle:k=".as_slice(), &body, b"\x07"].concat();
        let (head, tail) = report.split_at(2000);
        assert_eq!(scanner.scan(head), []);
        assert_eq!(scanner.held(), head);
        assert!(matches!(
            scanner.scan(tail).as_slice(),
            [Piece::Sequence(Query::ProgramStatusReport(_), _)]
        ));

        let report_of = |filler: usize, end: &[u8]| {
            [b"\x1b]7501;".as_slice(), &vec![b'x'; filler], end].concat()
        };
        // Exactly at the limit, it is a report, whichever `ST` ends it.
        for end in [&b"\x07"[..], b"\x1b\\"] {
            let at = report_of(LONGEST_REPORT - 7 - end.len(), end);
            assert_eq!(at.len(), LONGEST_REPORT);
            assert!(matches!(
                scan(&at).as_slice(),
                [Piece::Sequence(Query::ProgramStatusReport(_), _)]
            ));
            // One byte past it, output, sent on as it is.
            let over = report_of(LONGEST_REPORT - 7 - end.len() + 1, end);
            let pieces = scan(&over);
            assert_eq!(pieces.concat_output(), over);
            assert!(pieces.iter().all(|piece| matches!(piece, Piece::Output(_))));
        }
    }

    #[test]
    fn a_control_character_inside_a_query_makes_it_output() {
        assert_eq!(scan(b"\x1b[6\nn"), [output(b"\x1b[6\nn")]);
        assert_eq!(scan(b"\x1b]11;?\x18"), [output(b"\x1b]11;?\x18")]);
    }

    #[test]
    fn an_escape_starts_a_sequence_whatever_it_interrupts() {
        assert_eq!(
            scan(b"\x1b[3\x1b[6n"),
            [
                output(b"\x1b[3"),
                sequence(Query::CursorPosition, b"\x1b[6n")
            ]
        );
        assert_eq!(
            scan(b"\x1b]11;?\x1b[6n"),
            [
                output(b"\x1b]11;?"),
                sequence(Query::CursorPosition, b"\x1b[6n")
            ]
        );
        assert_eq!(
            scan(b"\x1b\x1b[c"),
            [
                output(b"\x1b"),
                sequence(Query::DeviceAttributes, b"\x1b[c")
            ]
        );
    }

    #[test]
    fn a_query_cut_across_reads_is_held_back_until_it_is_whole() {
        let mut scanner = QueryScanner::default();
        assert_eq!(scanner.scan(b"ab\x1b["), [output(b"ab")]);
        assert_eq!(scanner.held(), b"\x1b[");
        assert_eq!(scanner.scan(b"6"), []);
        assert_eq!(
            scanner.scan(b"ncd"),
            [sequence(Query::CursorPosition, b"\x1b[6n"), output(b"cd")]
        );
        assert!(scanner.held().is_empty());

        assert_eq!(scanner.scan(b"\x1b]11;?\x1b"), []);
        assert_eq!(
            scanner.scan(b"\\"),
            [sequence(
                Query::Color {
                    color: DynamicColor::Background,
                    st: true
                },
                b"\x1b]11;?\x1b\\"
            )]
        );

        // Cut short and then not a query after all: all of it comes out, in order.
        assert_eq!(scanner.scan(b"\x1b[3"), []);
        assert_eq!(scanner.scan(b"1mx"), [output(b"\x1b[31mx")]);
    }

    #[test]
    fn a_sequence_too_long_for_a_query_is_output_without_waiting_for_its_end() {
        let mut scanner = QueryScanner::default();
        let long = [b"\x1b[".as_slice(), &[b'1'; 60]].concat();
        let pieces = scanner.scan(&long);
        assert_eq!(pieces.concat_output(), long);
        assert!(scanner.held().is_empty());
    }

    trait ConcatOutput {
        fn concat_output(&self) -> Vec<u8>;
    }

    impl ConcatOutput for Vec<Piece> {
        fn concat_output(&self) -> Vec<u8> {
            self.iter()
                .flat_map(|piece| match piece {
                    Piece::Output(bytes) | Piece::Sequence(_, bytes) => bytes.clone(),
                })
                .collect()
        }
    }
}
