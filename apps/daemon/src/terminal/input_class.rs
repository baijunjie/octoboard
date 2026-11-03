//! Telling a terminal's own protocol traffic apart from what a person types.

/// Whether a chunk written to a session's input is nothing but a terminal's own protocol traffic:
/// focus in/out (`CSI I`, `CSI O`), device attributes (`CSI … c`), cursor position (`CSI … R`),
/// mode reports (`CSI … $ y`), window reports (`CSI … t`), status reports (`CSI … n`), mouse
/// movement, wheel and button-release reports (`CSI < b;x;y M` with the motion bit, 32, or the
/// wheel bit, 64, set in `b`, and `CSI < … m`; the older `CSI M` with three bytes alike), and
/// operating system or device control string replies. Conservative: an empty chunk, any other
/// byte, any other final byte — the arrow keys included — a mouse button press, and a sequence cut
/// short all say no.
///
/// A pointer passing or scrolling over the terminal is not the person typing. Where mouse
/// reporting is off the terminal turns the wheel into arrow keys, which say no.
pub(crate) fn is_terminal_protocol(chunk: &[u8]) -> bool {
    if chunk.is_empty() {
        return false;
    }
    let mut at = 0;
    while at < chunk.len() {
        if chunk[at] != 0x1b {
            return false;
        }
        match chunk.get(at + 1) {
            // The older mouse report: `M` and three bytes of button and position.
            Some(b'[') if chunk.get(at + 2) == Some(&b'M') => {
                if chunk.len() < at + 6 || chunk[at + 3..at + 6].iter().any(|byte| *byte < 0x20) {
                    return false;
                }
                // A release is button 3; movement has the motion bit, the wheel its own bit.
                let button = chunk[at + 3] - 0x20;
                if button & (32 | 64) == 0 && button & 3 != 3 {
                    return false;
                }
                at += 6;
            }
            Some(b'[') => {
                at += 2;
                let start = at;
                while at < chunk.len() && (0x20..=0x3f).contains(&chunk[at]) {
                    at += 1;
                }
                let Some(&final_byte) = chunk.get(at) else {
                    return false;
                };
                let known = match final_byte {
                    b'I' | b'O' | b'c' | b'R' | b't' | b'n' => true,
                    b'y' => chunk[start..at].contains(&b'$'),
                    b'm' => chunk.get(start) == Some(&b'<'),
                    b'M' => {
                        chunk.get(start) == Some(&b'<')
                            && std::str::from_utf8(&chunk[start + 1..at])
                                .ok()
                                .and_then(|params| params.split(';').next()?.parse::<u32>().ok())
                                .is_some_and(|button| button & (32 | 64) != 0)
                    }
                    _ => false,
                };
                if !known {
                    return false;
                }
                at += 1;
            }
            Some(&opener @ (b']' | b'P')) => {
                at += 2;
                // Up to a string terminator (an escape and a backslash), or for an operating system
                // command a bell as well. Nothing but printable bytes in between: a control byte is
                // typed input, not a reply.
                loop {
                    match chunk.get(at) {
                        None => return false,
                        Some(0x07) if opener == b']' => {
                            at += 1;
                            break;
                        }
                        Some(0x1b) if chunk.get(at + 1) == Some(&b'\\') => {
                            at += 2;
                            break;
                        }
                        Some(byte) if *byte < 0x20 => return false,
                        Some(_) => at += 1,
                    }
                }
            }
            _ => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::is_terminal_protocol;

    #[test]
    fn only_a_terminals_own_protocol_traffic_is_told_apart_from_input() {
        for protocol in [
            &b"\x1b[I"[..],
            b"\x1b[O",
            b"\x1b[I\x1b[O",
            b"\x1b[?1;2c",
            b"\x1b[>0;276;0c",
            b"\x1b[24;80R",
            b"\x1b[?2004;2$y",
            b"\x1b[8;32;120t",
            b"\x1b[0n",
            b"\x1b]11;rgb:0000/0000/0000\x07",
            b"\x1b]11;rgb:0000/0000/0000\x1b\\",
            b"\x1bP>|xterm.js\x1b\\",
            b"\x1b[I\x1b[?1;2c\x1b]10;rgb:ffff/ffff/ffff\x07",
            b"\x1b[<35;12;7M",
            b"\x1b[<0;12;7m",
            b"\x1b[M#,'",
            b"\x1b[MC,'",
            b"\x1b[<64;12;7M",
            b"\x1b[Ma,'",
        ] {
            assert!(is_terminal_protocol(protocol), "{protocol:?}");
        }
        for input in [
            &b""[..],
            b"\r",
            b"x",
            b"\x1b[B",
            b"\x1b[A",
            b"\x1b[C",
            b"\x1b[3~",
            b"\x1b",
            b"\x1b[",
            b"\x1b[I\x1b[B",
            b"\x1b[Ix",
            b"\x1b[I\r",
            b"\x1b[?2004;2y",
            b"\x1b[1",
            b"\x1b]11;rgb:0000",
            b"\x1b]11;rgb\r:0000\x07",
            b"\x1b]11;rgb\x1b[B:0000\x07",
            b"\x1bP>|xterm\x07.js\x1b\\",
            b"\x1bP>|xterm.js",
            b"\x1bOB",
            b"\x1b[35;12;7M",
            b"\x1b[M#",
            b"\x1b[<35;12;7",
            b"\x1b[<0;12;7M",
            b"\x1b[<0;12;7M\x1b[<0;12;7m",
            b"\x1b[M ,'",
        ] {
            assert!(!is_terminal_protocol(input), "{input:?}");
        }
    }
}
