//! The titles the parsed screen reports (`TerminalSnapshot::report_titles`), read through
//! [`Parser::take`] as a host that renders what the program draws has them read, against the
//! titles [`Parser::feed`] reads off the same output: the same sequence over every capture, except
//! for a title that comes before anything is drawn, and for title sequences the emulator reads
//! otherwise than the observer does.

use super::*;
use crate::terminal::snapshot::TerminalSnapshot;

/// A completed title as a session takes it: the title, and the activity glyph it started with.
type Seen = (Option<String>, Option<char>);

/// Every raw capture of an agent's output in `testdata`, by file name.
fn captures() -> Vec<(String, Vec<u8>)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata");
    let mut captures: Vec<_> = std::fs::read_dir(&dir)
        .expect("the testdata directory")
        .map(|entry| entry.expect("a testdata entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "bin"))
        .map(|path| {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            (name, std::fs::read(&path).expect("a capture"))
        })
        .collect();
    captures.sort();
    assert!(captures.len() >= 30, "{} captures", captures.len());
    captures
}

/// The titles `output`, cut into reads of `read` bytes, completes as the output's own title
/// sequences say, the last of each read.
fn off_the_output(output: &[u8], read: usize) -> Vec<Seen> {
    let mut parser = Parser::default();
    output
        .chunks(read)
        .filter_map(|chunk| {
            let title = parser.feed(chunk)?;
            Some((title, parser.leading_glyph()))
        })
        .collect()
}

/// [`off_the_output`] as the parsed screen reports the titles, after `before` has been drawn.
fn off_the_parsed_screen(before: &[u8], output: &[u8], read: usize) -> Vec<Seen> {
    let mut parser = Parser::default();
    let mut screen = TerminalSnapshot::new(32, 120);
    screen.report_titles();
    screen.process(before);
    assert_eq!(parser.take(&screen.take_titles()), None);
    output
        .chunks(read)
        .filter_map(|chunk| {
            screen.process(chunk);
            let title = parser.take(&screen.take_titles())?;
            Some((title, parser.leading_glyph()))
        })
        .collect()
}

/// A row drawn before the output, as a session's own output has drawn something before the
/// title captures, which keep only the title sequences, were sent.
const DRAWN: &[u8] = b"$ ";

/// After something has been drawn, every capture's titles come off the parsed screen as they come
/// off the output, however the output is cut into reads.
#[test]
fn the_parsed_screen_reports_the_titles_the_output_completes() {
    let mut titled = 0;
    for (name, output) in captures() {
        for read in [1, 7, 64, 4096, output.len().max(1)] {
            let expected = off_the_output(&output, read);
            titled += usize::from(read == 1 && !expected.is_empty());
            assert_eq!(
                off_the_parsed_screen(DRAWN, &output, read),
                expected,
                "{name} in reads of {read}"
            );
        }
    }
    assert!(titled >= 20, "{titled} captures with titles");
}

/// A title that comes before anything is drawn is not reported: a host that renders what the
/// program draws may set one of its own as it starts. Each agent sets its own as it starts, before
/// it draws anything, and so loses it: Claude Code's `✳ Claude Code` and Grok Build's `grok`, which
/// Grok sets again once it has drawn; Codex sets its first once it has drawn. The inbox console
/// host's captures (`testdata/conhost-captures.md`) hold the host's own titles: `claude` before
/// Claude Code's first drawing, which is lost, and Codex's path after its first frame, which is
/// not. The title captures keep only the title sequences, so on their own every title in them
/// comes first.
#[test]
fn a_title_before_anything_is_drawn_is_not_reported() {
    let at_prompt = (Some("Claude Code".to_string()), Some('\u{2733}'));
    let grok = (Some("grok".to_string()), None);
    let host_claude = (Some("claude".to_string()), None);
    for (name, output) in captures() {
        let expected = off_the_output(&output, 1);
        let reported = off_the_parsed_screen(b"", &output, 1);
        let lost: &[Seen] = match name.as_str() {
            "claude_input_placeholder.bin"
            | "claude_input_queued.bin"
            | "claude_input_queued_draft.bin"
            | "claude_input_restored.bin" => std::slice::from_ref(&at_prompt),
            "grok_composer_draft.bin"
            | "grok_composer_restored.bin"
            | "grok_native_stashed.bin"
            | "grok_trust_screen.bin" => std::slice::from_ref(&grok),
            name if name.starts_with("conhost_claude_") => std::slice::from_ref(&host_claude),
            "claude_title_cancel.bin"
            | "claude_title_idle.bin"
            | "claude_title_permission.bin"
            | "claude_trust_dismissed.bin"
            | "codex_approval_title.bin"
            | "codex_working_title.bin"
            | "grok_title_cancel_before_output.bin"
            | "grok_title_tool.bin"
            | "grok_title_turn_end.bin" => &expected,
            _ => &[],
        };
        assert_eq!(&expected[..lost.len()], lost, "{name}: what comes first");
        assert_eq!(reported, &expected[lost.len()..], "{name}");
    }
}

/// A title that comes before anything is drawn is left out however the output is cut into reads,
/// and one that comes after is reported, even when what was drawn has been cleared away again.
#[test]
fn only_what_was_drawn_before_a_title_decides_whether_it_is_reported() {
    let output =
        b"\x1b]0;C:\\Windows\\conhost.exe\x07\x1b[2J\x1b[Hdrawn\x1b[2J\x1b[H\x1b]0;mine\x07";
    for read in 1..=output.len() {
        assert_eq!(
            off_the_parsed_screen(b"", output, read),
            [(Some("mine".to_string()), None)],
            "in reads of {read}"
        );
    }
    let mut screen = TerminalSnapshot::new(4, 20);
    screen.process(b"drawn\x1b]0;mine\x07");
    assert!(screen.take_titles().is_empty(), "reported only once asked");
}

/// The title sequences the parsed screen reads otherwise than the output's observer, which takes
/// only an OSC 0 or 2 ended by BEL or ST with nothing but text in it and no title stack: the
/// emulator drops the control characters inside, ends the sequence at an escape or a cancel, reads
/// one inside a device control string, makes invalid UTF-8 an empty title, and resets the title
/// when a pushed one is popped. None of these is in a capture.
#[test]
fn malformed_title_sequences_read_as_the_emulator_reads_them() {
    let title = |text: &str| (Some(text.to_string()), None);
    for (output, observed, reported) in [
        (
            &b"\x1b]2;bad\ncontrol\x07"[..],
            vec![],
            vec![title("badcontrol")],
        ),
        (b"\x1b]2;cancelled\x18", vec![], vec![title("cancelled")]),
        (b"\x1b]2;bad\x1b[31m\x07", vec![], vec![title("bad")]),
        (
            b"\x1bP\x1b]2;inside DCS\x07\x1b\\",
            vec![],
            vec![title("inside DCS")],
        ),
        (b"\x1b]2;bad\xff\x07", vec![], vec![(None, None)]),
        (
            b"\x1b[22;0t\x1b]2;pushed\x07\x1b[23;0t",
            vec![title("pushed")],
            vec![title("pushed"), (None, None)],
        ),
    ] {
        assert_eq!(off_the_output(output, 1), observed, "{output:?}");
        assert_eq!(
            off_the_parsed_screen(DRAWN, output, 1),
            reported,
            "{output:?}"
        );
    }
    // Read alike: other strings and commands, a C1 control, a title too long to keep, and the
    // forms the observer's own tests check.
    let oversized = format!("\x1b]2;{}\x07", "x".repeat(MAX_TITLE_BYTES + 1));
    for output in [
        &b"\x1b]1;icon\x07"[..],
        b"\x1b]52;clipboard\x07",
        b"\x1b]2;bad\xc2\x85control\x07",
        oversized.as_bytes(),
        b"\x1b]0;a;b\x07",
        b"\x1b]2; padded \x07",
        b"\x1b]2;old\x07\x1b]2; \x07",
        "\x1b]2;\u{25D0} Fix the build\x07".as_bytes(),
        "\x1b]2;\u{2733}\x07".as_bytes(),
        "\x1b]0;\u{4FEE}\u{590D}\x1b\\".as_bytes(),
    ] {
        assert_eq!(
            off_the_parsed_screen(DRAWN, output, 1),
            off_the_output(output, 1),
            "{output:?}"
        );
    }
}
