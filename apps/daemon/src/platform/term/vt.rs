//! The bytes a VT-compatible terminal sends for each [`Input`]: what a program on a Unix PTY
//! reads when a person types or pastes the same thing into a terminal emulator. Pure, so the
//! table is tested on every host.

use super::Input;

/// The bracketed-paste markers around a paste (`DECSET 2004`).
const PASTE_START: &[u8] = b"\x1b[200~";
const PASTE_END: &[u8] = b"\x1b[201~";

/// The inputs as one buffer, written in one go: a paste followed by Enter arrives as a single
/// submitted message, with no delay needed between the two.
pub(super) fn encode(input: &[Input]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(input.iter().map(encoded_len).sum());
    for input in input {
        match input {
            Input::Paste(text) => {
                bytes.extend_from_slice(PASTE_START);
                bytes.extend_from_slice(text.as_bytes());
                bytes.extend_from_slice(PASTE_END);
            }
            // A carriage return, as a terminal sends the key: a line feed would insert a newline
            // into a program's input box instead of submitting it.
            Input::Enter => bytes.push(b'\r'),
            Input::Escape => bytes.push(0x1b),
            Input::Ctrl(key) => match key.to_ascii_uppercase() {
                key @ '@'..='_' => bytes.push(key as u8 & 0x1f),
                other => debug_assert!(false, "no control byte for {other:?}"),
            },
            // The normal cursor-key form, which is what the daemon has always written.
            Input::Down => bytes.extend_from_slice(b"\x1b[B"),
            Input::Text(text) => bytes.extend_from_slice(text.as_bytes()),
        }
    }
    bytes
}

fn encoded_len(input: &Input) -> usize {
    match input {
        Input::Paste(text) => PASTE_START.len() + text.len() + PASTE_END.len(),
        Input::Down => 3,
        Input::Text(text) => text.len(),
        Input::Enter | Input::Escape | Input::Ctrl(_) => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_input_is_written_as_a_terminal_sends_it() {
        let table: &[(Input, &[u8])] = &[
            (
                Input::Paste("line one\nline two".into()),
                b"\x1b[200~line one\nline two\x1b[201~",
            ),
            (Input::Paste(String::new()), b"\x1b[200~\x1b[201~"),
            (Input::Enter, b"\r"),
            (Input::Escape, b"\x1b"),
            (Input::Ctrl('c'), b"\x03"),
            (Input::Ctrl('C'), b"\x03"),
            (Input::Ctrl('s'), b"\x13"),
            (Input::Ctrl('u'), b"\x15"),
            (Input::Ctrl('@'), b"\x00"),
            (Input::Ctrl('['), b"\x1b"),
            (Input::Down, b"\x1b[B"),
            (Input::Text("y".into()), b"y"),
            (Input::Text("é".into()), "é".as_bytes()),
        ];
        for (input, bytes) in table {
            assert_eq!(
                encode(std::slice::from_ref(input)),
                *bytes,
                "{input:?} is written as the terminal sends it"
            );
        }
    }

    #[test]
    fn a_sequence_of_inputs_is_one_buffer_in_order() {
        let encoded = encode(&[Input::Paste("hello".into()), Input::Enter]);
        assert_eq!(encoded, b"\x1b[200~hello\x1b[201~\r");
    }
}
