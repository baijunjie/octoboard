//! The terminal queries a holder answers itself while no daemon is linked.
//!
//! A program asks its terminal things through its output and waits for the answer as input; a
//! host that holds the program until a query is answered (the bundled console host on Windows,
//! `TermFacts::host_waits_on_queries`) stalls it for each one nobody answers. A linked daemon
//! answers them from its parsed screen. A holder keeps no screen, so on its own it answers only a
//! query whose answer needs none, with the answer the daemon gives: the primary device attributes.
//! The cursor position and every other query that needs the screen go unanswered while no daemon
//! is linked, and the program waits out the host's hold for each.
//!
//! [`OfflineQueries`] follows the output as a terminal's parser does, one byte at a time, so a
//! query cut across two reads is found once its last byte arrives, and nothing inside a control
//! string (OSC, DCS, APC, PM or SOS) is read as a query. It keeps no more than one short sequence's
//! parameters, and it only ever reports a query as its last byte is read: what was read before
//! is never looked at again, so a query is never answered late.

/// The answer to the primary device attributes, the one the daemon's parsed terminal gives
/// (`alacritty_terminal`'s, a VT102), so that a program is told the same whoever answers.
pub(super) const PRIMARY_DEVICE_ATTRIBUTES: &[u8] = b"\x1b[?6c";

/// The most parameter and intermediate bytes of a control sequence followed; a longer sequence is
/// no query answered here, and is skipped to its end.
const LONGEST_PARAMS: usize = 32;

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;
/// CAN and SUB, which cut any sequence short.
const CAN: u8 = 0x18;
const SUB: u8 = 0x1a;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum State {
    #[default]
    Ground,
    /// After `ESC`.
    Escape,
    /// In an escape sequence's intermediate bytes (`ESC ( B`, say).
    EscapeIntermediate,
    /// In `ESC [`, its parameters and intermediates kept.
    Csi,
    /// In a control sequence too long to be a query answered here, until its final byte.
    CsiIgnore,
    /// In a control string: OSC (`ESC ]`, which `BEL` ends too) or DCS, APC, PM or SOS.
    String { osc: bool },
    /// After an `ESC` inside a control string, which `\` makes its terminator (ST).
    StringEscape,
}

/// Follows a terminal's output for the queries a holder answers itself; see the module doc.
#[derive(Debug, Default)]
pub(super) struct OfflineQueries {
    state: State,
    /// The parameter and intermediate bytes of the control sequence being read.
    params: Vec<u8>,
}

impl OfflineQueries {
    /// Reads `bytes`, the next of the output, and returns how many primary device attributes
    /// queries end in them.
    pub(super) fn feed(&mut self, bytes: &[u8]) -> usize {
        bytes.iter().filter(|&&byte| self.step(byte)).count()
    }

    /// Reads one byte; `true` when it ends a primary device attributes query.
    fn step(&mut self, byte: u8) -> bool {
        // Wherever it comes, an `ESC` starts a new sequence and CAN or SUB ends the one being
        // read, as in a terminal's parser; inside a control string an `ESC` may begin its ST.
        match (self.state, byte) {
            (State::String { .. }, ESC) => {
                self.state = State::StringEscape;
                return false;
            }
            (State::StringEscape, b'\\') => {
                self.state = State::Ground;
                return false;
            }
            // The `ESC` before it cut the string short and starts a sequence of its own.
            (State::StringEscape, _) => {
                self.state = State::Escape;
                return self.step(byte);
            }
            (_, ESC) => {
                self.state = State::Escape;
                return false;
            }
            (_, CAN | SUB) => {
                self.state = State::Ground;
                return false;
            }
            _ => {}
        }
        match self.state {
            State::Ground => false,
            State::Escape => {
                self.state = match byte {
                    b'[' => {
                        self.params.clear();
                        State::Csi
                    }
                    b']' => State::String { osc: true },
                    b'P' | b'_' | b'^' | b'X' => State::String { osc: false },
                    0x20..=0x2f => State::EscapeIntermediate,
                    _ => State::Ground,
                };
                false
            }
            State::EscapeIntermediate => {
                if !(0x20..=0x2f).contains(&byte) {
                    self.state = State::Ground;
                }
                false
            }
            State::Csi => match byte {
                0x20..=0x3f => {
                    if self.params.len() < LONGEST_PARAMS {
                        self.params.push(byte);
                    } else {
                        self.state = State::CsiIgnore;
                    }
                    false
                }
                0x40..=0x7e => {
                    self.state = State::Ground;
                    byte == b'c' && is_primary(&self.params)
                }
                // A control character inside a sequence is carried out and leaves it as it is.
                _ => false,
            },
            State::CsiIgnore => {
                if (0x40..=0x7e).contains(&byte) {
                    self.state = State::Ground;
                }
                false
            }
            State::String { osc } => {
                if osc && byte == BEL {
                    self.state = State::Ground;
                }
                false
            }
            State::StringEscape => unreachable!("handled above"),
        }
    }
}

/// Whether `params`, the bytes between `ESC [` and a final `c`, ask for the primary device
/// attributes: none, or a single parameter of 0, which the daemon's query scanner reads the same.
fn is_primary(params: &[u8]) -> bool {
    params.iter().all(|&byte| byte == b'0')
}

#[cfg(test)]
mod tests {
    use super::OfflineQueries;

    fn asked(chunks: &[&[u8]]) -> usize {
        let mut queries = OfflineQueries::default();
        chunks.iter().map(|chunk| queries.feed(chunk)).sum()
    }

    #[test]
    fn the_primary_device_attributes_query_is_found_and_nothing_else() {
        assert_eq!(asked(&[b"hello \x1b[c world"]), 1);
        assert_eq!(asked(&[b"\x1b[0c\x1b[c"]), 2);
        // Secondary and tertiary attributes, the cursor position, a parameter of 1, a
        // sequence with an intermediate: none is answered here.
        assert_eq!(asked(&[b"\x1b[>c\x1b[=c\x1b[6n\x1b[1c\x1b[ c\x1b[0;0c"]), 0);
        assert_eq!(asked(&[b"plain [c and c"]), 0);
    }

    #[test]
    fn a_query_cut_across_reads_is_found_once_its_last_byte_arrives() {
        let mut queries = OfflineQueries::default();
        assert_eq!(queries.feed(b"text\x1b"), 0);
        assert_eq!(queries.feed(b"["), 0);
        assert_eq!(queries.feed(b"0"), 0);
        assert_eq!(queries.feed(b"c more"), 1);
        assert_eq!(queries.feed(b"more"), 0, "a query is reported once");
    }

    #[test]
    fn nothing_inside_a_control_string_is_read_as_a_query() {
        // A title, a DCS, an APC and a PM, each holding what would read as a query outside it.
        assert_eq!(asked(&[b"\x1b]2;[c and [0c\x07after"]), 0);
        assert_eq!(asked(&[b"\x1bP+q[c\x1b\\"]), 0);
        assert_eq!(asked(&[b"\x1b_G[c;[0c\x1b\\\x1b^[c\x1b\\"]), 0);
        // Cut across reads, the string still holds it.
        assert_eq!(asked(&[b"\x1b]0;ti", b"tle [c", b"\x07"]), 0);
        // A query right after the string's end is a query.
        assert_eq!(asked(&[b"\x1b]2;t\x1b\\\x1b[c"]), 1);
        assert_eq!(asked(&[b"\x1b]2;t\x07", b"\x1b[c"]), 1);
    }

    #[test]
    fn an_escape_inside_a_control_string_ends_it_as_a_terminals_parser_does() {
        // The `ESC [` cuts the title short and starts a sequence of its own, which a terminal
        // answers and so the daemon would.
        assert_eq!(asked(&[b"\x1b]2;x\x1b[c"]), 1);
        // A BEL ends only an OSC: inside a DCS it is part of the string.
        assert_eq!(asked(&[b"\x1bPq\x07[c\x1b\\"]), 0);
    }

    #[test]
    fn a_sequence_too_long_or_cut_short_is_no_query() {
        let long = [&b"\x1b["[..], &[b'0'; 40][..], &b"c"[..]].concat();
        assert_eq!(asked(&[&long]), 0);
        assert_eq!(asked(&[b"\x1b[\x18c\x1b[c"]), 1);
        assert_eq!(asked(&[b"\x1b(B\x1b[c"]), 1);
    }
}
