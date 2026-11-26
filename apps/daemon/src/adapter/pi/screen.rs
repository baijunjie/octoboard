//! How Pi's screens read: its input box, its project-trust confirmation and its interrupt key. Its
//! status comes from OSC 7501 reports or the extension's launch hook. The measurements, on
//! 1.5.2, are in `docs/agent-cli-reference.md`, section "Pi".
//!
//! The input box is the rows between the last two full-width rules of the screen, above Pi's
//! footer. Its own rows carry no mark and no indent, so the rules are all it can be found by, and
//! Pi draws them in the colour of the thinking level, which for `off` is faint: the box is read
//! off the screen as drawn ([`ScreenReader::input_box_screen`]). Pi draws its selectors and
//! dialogs (`/model`, `/resume`, an extension's question, the trust confirmation) between rules in
//! the box's place; those are told from the box by their list cursor or their key hints, and read
//! as no box.
//!
//! Pi trims what it takes from the box and runs a known slash command written there, a pasted one
//! included, so a message's leading slash is guarded with a zero-width space, which it keeps. A
//! pasted command draws no completion: Pi offers its command list only as `/` is typed, and an
//! Enter runs the pasted text as it is, a command Pi does not know going to the model as a message
//! (1.5.2). Pi shows a pasted image path as the text it is; Octoboard's extension makes the images
//! Octoboard pasted attachments, and their paths are what the box shows of them.

use std::ops::Range;

use crate::session::pasted_image;
use crate::session::screen::{Interrupt, InterruptKey, ScreenReader, ScreenText};
use crate::session::trust::{Press, Screen, StartSignal, HOOK_FALLBACK};

pub(super) struct PiScreen;

/// U+200B ZERO WIDTH SPACE, which Pi 1.5.2 does not trim, before a message's leading slash.
const SLASH_GUARD: &str = "\u{200B}";

/// The byte ranges of `text`, the box's rows joined with line breaks, that hold the path of an
/// image Octoboard pasted: a file named as `session::pasted_image` names one, in a directory named
/// as it names theirs, bare or in single quotes. Pi wraps a path longer than the box onto the next
/// row, so a range may hold line breaks. Octoboard pastes each path on its own, so the next one,
/// or the text, can follow with nothing between; a path is taken to start no earlier than where
/// the one before it ended.
fn pasted_image_spans(text: &str) -> Vec<Range<usize>> {
    let chars: Vec<(usize, char)> = text.char_indices().filter(|(_, c)| *c != '\n').collect();
    let byte_at = |index: usize| chars.get(index).map_or(text.len(), |(at, _)| *at);
    let mut spans = Vec::new();
    let mut floor = 0;
    let mut at = 0;
    while at < chars.len() {
        let Some(end) = pasted_file_end(&chars, at) else {
            at += 1;
            continue;
        };
        let mut start = at;
        while start > floor && !chars[start - 1].1.is_whitespace() && chars[start - 1].1 != '\'' {
            start -= 1;
        }
        let (start, end) = match chars.get(end) {
            // A quoted path may hold spaces: it starts at its opening quote.
            Some((_, '\'')) => match (floor..at).rev().find(|&index| chars[index].1 == '\'') {
                Some(quote) => (quote, end + 1),
                None => (start, end),
            },
            _ => (start, end),
        };
        spans.push(byte_at(start)..byte_at(end));
        floor = end;
        at = end;
    }
    spans
}

/// How many characters are read for a pasted image's name: more than the longest the daemon gives.
const NAME_READ: usize = 64;

/// Where a pasted image's file name ends, as an index into `chars`, where `chars[at..]` starts
/// with the separator before the images' directory (`pasted_image::DIRECTORY`), a separator and a
/// saved image's name (`pasted_image::saved_name_len`); `None` where it does not.
fn pasted_file_end(chars: &[(usize, char)], at: usize) -> Option<usize> {
    let is_separator = |index: usize| matches!(chars.get(index), Some((_, '/' | '\\')));
    let directory = pasted_image::DIRECTORY;
    let in_directory = directory.chars().enumerate().all(|(offset, c)| {
        chars
            .get(at + 1 + offset)
            .is_some_and(|(_, found)| *found == c)
    });
    let name_at = at + 1 + directory.chars().count() + 1;
    if !(is_separator(at) && in_directory && is_separator(name_at - 1)) {
        return None;
    }
    let name: String = chars[name_at.min(chars.len())..]
        .iter()
        .take(NAME_READ)
        .map(|(_, c)| *c)
        .collect();
    pasted_image::saved_name_len(&name).map(|len| name_at + len)
}

impl ScreenReader for PiScreen {
    /// Pi trims a message before it reads it as a command (1.5.2): ` /name x` renamed the session,
    /// while `\u{200B}/name x` reached the model as text.
    fn leading_slash_guard(&self) -> &'static str {
        SLASH_GUARD
    }

    /// Pi trims the text it takes from the box, and the guard is in what it reports.
    fn same_as_submitted(&self, typed: &str, reported: &str) -> bool {
        use crate::session::term::without_invisible;
        without_invisible(typed).trim() == without_invisible(reported).trim()
    }

    /// The paths of the images Octoboard pasted, which the box shows as text, each an attachment
    /// once the message is sent.
    fn image_placeholders(&self, text: &str) -> usize {
        pasted_image_spans(text).len()
    }

    fn without_image_placeholders(&self, text: &str) -> String {
        let mut rest = String::new();
        let mut from = 0;
        for span in pasted_image_spans(text) {
            rest.push_str(&text[from..span.start]);
            from = span.end;
        }
        rest.push_str(&text[from..]);
        rest
    }

    /// Pi's input box is read wherever it sits at its prompt (`input_box_rows`), at every thinking
    /// level: the screen it is read off keeps the faint rules of the level `off`.
    fn reads_input_box(&self) -> bool {
        true
    }

    fn input_box_rows(&self, screen: &str) -> Option<Vec<String>> {
        input_box_rows(screen)
    }

    /// The rules that frame the box are faint at the thinking level `off`, and nothing else marks
    /// it. An empty box is one empty row with no hint in it, so no faint text reads as a draft.
    fn input_box_screen(&self) -> ScreenText {
        ScreenText::AsDrawn
    }

    /// `[paste #<n> +<lines> lines]` or `[paste #<n> <chars> chars]`, as Pi draws a paste of more
    /// than ten lines or a thousand characters in its box.
    fn collapsed_paste(&self, text: &str) -> bool {
        let Some(rest) = text
            .strip_prefix("[paste #")
            .and_then(|rest| rest.strip_suffix(']'))
        else {
            return false;
        };
        let digits = |text: &str| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
        let Some((id, size)) = rest.split_once(' ') else {
            return digits(rest);
        };
        digits(id)
            && (size
                .strip_prefix('+')
                .and_then(|size| size.strip_suffix(" lines"))
                .is_some_and(digits)
                || size.strip_suffix(" chars").is_some_and(digits))
    }

    /// A press deletes back to the start of the line, a wrapped line whole, and at the start of a
    /// line the line break before it; on an empty box it does nothing.
    fn clears_input_box_with_ctrl_u(&self) -> bool {
        true
    }

    /// Esc puts every steering message Pi still held back into the box, unsent, in order with an
    /// empty row between two, before whatever was typed there.
    fn cancel_restores_held_messages(&self) -> bool {
        true
    }

    fn trust_screen(&self) -> &'static Screen {
        &TRUST
    }

    fn interrupt(&self) -> Option<&'static Interrupt> {
        Some(&INTERRUPT)
    }
}

/// Pi 1.5.2's project-trust confirmation, shown only for a folder holding project resources of its
/// own (`.pi/settings.json` and the like): the cursor starts on "Trust", the first of five options.
/// `others` matches the other four before "Trust", so that the cursor on any of them reads as not
/// on it: two of them start with "Trust" themselves, and `Donottrust` is also how "Do not trust
/// (this session only)" starts.
pub(super) const TRUST: Screen = Screen {
    first_draw: &[&[
        "Trustprojectfolder?",
        "Thisallowspitoload.pisettingsandresources",
        "\u{2192}Trust",
        "Trustparentfolder",
        "Trust(thissessiononly)",
        "Donottrust",
    ]],
    accept: Some("Trust"),
    others: &["Trustparentfolder", "Trust(thissessiononly)", "Donottrust"],
    cursor: Some('\u{2192}'),
    press: Press::Enter,
    next_screen: None,
    // The extension's `session_start` follows an answer in the terminal at once.
    input_ends_wait: false,
    start_signal: StartSignal::Hook,
    // That hook is the extension's own, which a user's extension could keep from loading; held
    // past the fallback only while a sighted screen is still up.
    hook_fallback: Some(HOOK_FALLBACK),
    // Pi turns bracketed paste on as it starts, before its trust screen.
    waits_for_paste_mode: false,
};

/// One Esc stops a turn, whether the model has shown anything or not. Pi does not put the
/// interrupted prompt back in its input box (1.5.2), but it does put the steering messages it
/// still held there, which the interrupt clears as it clears a prompt put back.
const INTERRUPT: Interrupt = Interrupt {
    key: InterruptKey::Esc,
    restores_prompt: true,
    written_during_turn,
    queued_waiting: None,
};

/// A message written into a Pi turn through its terminal waits as a steering message until the
/// turn's current response and tool calls are done, and a cancel puts every message still waiting
/// back into the input box, unsent, each as its own text, which is how Octoboard knows them there
/// (`crate::session::draft_clear`): it takes them out of the box and writes them again, in their
/// order, after the next message written into the session; text the user typed after them is
/// cleared with them and handed back to the user as a draft. Whether Octoboard cleared them there
/// is for the sentences the interrupt's note goes on with, not for this one.
fn written_during_turn(written: usize) -> String {
    format!(
        " Octoboard wrote about {written} message(s) into the session during that turn. Pi takes \
         such a message into the turn once the turn's current step is done, and the cancel put \
         those it still held back into its input box, unsent. Octoboard takes those out of the \
         box and, where it knows them there, writes them again itself, in their order, right \
         after the next message written into the session. Check the session with `get_session` \
         before sending any of them again."
    )
}

/// The least a rule that frames the box ends with: the rule runs to the right edge whatever Pi
/// writes into its left part.
const RULE_TAIL: &str =
    "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}";

/// Whether a row is one of the rules above and below the box: a full-width line of `─` from the
/// first column, which during a run carries `<spinner> Working` after its first two cells, and
/// where the box is scrolled a count of the rows hidden (`↑ 3 more`).
fn is_rule(row: &str) -> bool {
    row.starts_with("\u{2500}\u{2500}") && row.ends_with(RULE_TAIL)
}

/// The cursors Pi's lists mark their current entry with.
const LIST_CURSORS: [&str; 2] = ["\u{2192} ", "\u{203A} "];

/// Whether the rows between the rules are a selector or a dialog Pi draws in the box's place
/// rather than the box: an entry under a list cursor, or a last row of key hints that ends with
/// what Esc does there. A draft whose text has the same shape reads as no box, and the message
/// for the session then waits as it does for any box that cannot be read.
fn is_panel(rows: &[&str]) -> bool {
    let cursor = rows.iter().any(|row| {
        LIST_CURSORS
            .iter()
            .any(|cursor| row.trim_start().starts_with(cursor))
    });
    let hints = rows
        .iter()
        .rev()
        .map(|row| row.trim().to_lowercase())
        .find(|row| !row.is_empty())
        .is_some_and(|row| {
            row.contains("esc") && (row.ends_with(" cancel") || row.ends_with(" close"))
        });
    let picker = rows
        .iter()
        .find(|row| !row.trim().is_empty())
        .is_some_and(|row| row.starts_with("Resume Session ("));
    cursor || hints || picker
}

/// The rows of Pi's input box: those between the last two rules, where the lower one has the
/// footer under it and what they frame is not a selector or a dialog. A box taller than Pi shows
/// (five rows, or three tenths of the terminal's height) gives the rows shown.
fn input_box_rows(screen: &str) -> Option<Vec<String>> {
    let rows: Vec<&str> = screen.lines().map(str::trim_end).collect();
    let lower = rows.iter().rposition(|row| is_rule(row))?;
    let upper = rows[..lower].iter().rposition(|row| is_rule(row))?;
    if !rows[lower + 1..].iter().any(|row| !row.is_empty()) {
        return None;
    }
    let framed = &rows[upper + 1..lower];
    if framed.is_empty() || is_panel(framed) {
        return None;
    }
    Some(framed.iter().map(|row| row.trim().to_string()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::AgentId;
    use crate::session::trust::{reading_of, Reading, TrustState};
    use crate::terminal::snapshot::TerminalSnapshot;

    /// The reader declares that it reads its input box wherever the agent is at its prompt, so a turn
    /// opened off the title closes only while the box is drawn (`crate::session::title_status`);
    /// the tests of `input_box_rows` hold it to every prompt state captured.
    #[test]
    fn the_input_box_is_read_wherever_the_agent_is_at_its_prompt() {
        assert!(PiScreen.reads_input_box());
    }

    // What Pi 1.5.2 printed in a folder holding `.pi/settings.json` that it had not been told to
    // trust, on a 100x30 PTY (`testdata/pi-captures.md`): the confirmation as first drawn, the
    // redraw after one Down, and what replaced it after Enter, which is Pi's start-up screen.
    const SCREEN: &[u8] = include_bytes!("../../../testdata/pi_trust_screen.bin");
    const AFTER_DOWN: &[u8] = include_bytes!("../../../testdata/pi_trust_after_down.bin");
    const DISMISSED: &[u8] = include_bytes!("../../../testdata/pi_trust_dismissed.bin");
    /// Another agent's trust screen, a near miss.
    const CLAUDE_SCREEN: &[u8] = include_bytes!("../../../testdata/claude_trust_screen.bin");

    /// The screen the captures were taken on.
    const ROWS: u16 = 30;
    const COLS: u16 = 100;

    /// The text of the parsed screen the captures `frames`, drawn one after another, leave, as the
    /// trust path reads a host that renders what the program draws.
    fn parsed(frames: &[&[u8]]) -> Vec<u8> {
        let mut terminal = TerminalSnapshot::new(ROWS, COLS);
        for frame in frames {
            terminal.process(frame);
        }
        terminal.visible_text().into_bytes()
    }

    fn read(capture: &[u8]) -> Reading {
        reading_of(&TRUST, capture)
    }

    /// The confirmation as first drawn is sighted, with the cursor on the option Octoboard
    /// presses, and nothing in it reads as gone or replaced.
    #[test]
    fn the_first_drawing_is_sighted_with_the_cursor_on_the_option_that_trusts() {
        assert_eq!(
            read(SCREEN),
            Reading {
                shown: true,
                up: true,
                cursor_on_accept: Some(true),
                gone: false,
                replaced: false,
            }
        );
    }

    /// After a Down the cursor is on another option, though that option's text begins with the
    /// pressed option's own. Read as a parsed screen, the confirmation is still up with the cursor
    /// moved, and is not the first drawing any more.
    #[test]
    fn after_a_down_the_cursor_is_on_another_option_and_the_screen_is_still_up() {
        let redraw = read(AFTER_DOWN);
        assert_eq!(redraw.cursor_on_accept, Some(false));
        assert!(!redraw.shown);

        let screen = read(&parsed(&[SCREEN, AFTER_DOWN]));
        assert!(screen.up);
        assert_eq!(screen.cursor_on_accept, Some(false));
        assert!(!screen.gone);

        let back = read(&parsed(&[SCREEN]));
        assert!(back.up);
        assert_eq!(back.cursor_on_accept, Some(true));
    }

    /// Once Enter has answered it, Pi draws its start-up screen in its place: text of its own and
    /// none of the confirmation's, so it reads as gone and replaced, with no cursor read and the
    /// confirmation no longer up.
    #[test]
    fn after_enter_the_confirmation_is_gone_and_something_else_is_drawn() {
        assert_eq!(
            read(DISMISSED),
            Reading {
                shown: false,
                up: false,
                cursor_on_accept: None,
                gone: true,
                replaced: true,
            }
        );
        let screen = read(&parsed(&[SCREEN, AFTER_DOWN, DISMISSED]));
        assert!(screen.gone);
        assert!(!screen.up);
    }

    /// The confirmation is recognised in its first drawing alone: not in another agent's screen,
    /// not in nothing, and not in the first drawing with any piece that says what it is taken out.
    #[test]
    fn nothing_but_the_complete_first_drawing_is_the_trust_screen() {
        for (name, capture) in [
            ("another agent's", CLAUDE_SCREEN),
            ("nothing", b"" as &[u8]),
        ] {
            assert!(!read(capture).shown, "{name}");
        }
        let text = String::from_utf8_lossy(SCREEN).into_owned();
        for piece in [
            "Trust project folder?",
            "This allows pi to load",
            "Trust parent folder",
            "Do not trust",
        ] {
            let without = text.replace(piece, "");
            assert_ne!(without, text, "{piece} is in the capture");
            assert!(!read(without.as_bytes()).shown, "without {piece}");
        }
    }

    /// The watch sights the screen however its first drawing is cut into reads, and not in what
    /// follows a key or in another agent's screen.
    #[test]
    fn the_watch_sights_the_first_drawing_however_it_is_read() {
        let sighted_in = |reads: &mut dyn Iterator<Item = &[u8]>| {
            let watch = TrustState::new(AgentId::PI, None);
            watch.read_the_output();
            for chunk in reads {
                watch.feed(chunk, 0, 0);
            }
            watch.waiting()
        };
        for size in [1, 7, 64, 1000, SCREEN.len()] {
            assert!(sighted_in(&mut SCREEN.chunks(size)), "in reads of {size}");
        }
        for (name, capture) in [
            ("after a Down", AFTER_DOWN),
            ("after Enter", DISMISSED),
            ("another agent's", CLAUDE_SCREEN),
        ] {
            assert!(!sighted_in(&mut capture.chunks(64)), "{name}");
        }
    }

    /// Pi is pressed with its cursor already on the option that trusts, so one Enter.
    #[test]
    fn the_press_is_one_enter() {
        assert_eq!(TRUST.press, Press::Enter);
    }

    /// Pi starts the session once its confirmation is behind it, announced by the extension's
    /// hook, which a user's extension could keep from loading: so messages wait for the hook, and
    /// let go after the fallback only where no screen was sighted.
    #[test]
    fn the_session_starts_at_the_extensions_hook() {
        assert_eq!(TRUST.start_signal, StartSignal::Hook);
        assert_eq!(TRUST.hook_fallback, Some(HOOK_FALLBACK));
    }

    /// Esc stops a turn, and what Pi puts back in its box at a cancel is cleared: the note says
    /// what the cancel did with the messages written during the turn and that Octoboard writes
    /// those given back again itself; not that several of them are told as a draft, and not that
    /// the box was cleared, which the interrupt says by how the clearing went.
    #[test]
    fn esc_interrupts_and_what_the_cancel_gives_back_is_cleared() {
        let interrupt = PiScreen.interrupt().expect("an interrupt key");
        assert_eq!(interrupt.key, InterruptKey::Esc);
        assert!(interrupt.restores_prompt);
        assert!(PiScreen.cancel_restores_held_messages());
        assert!(interrupt.queued_waiting.is_none());
        let note = (interrupt.written_during_turn)(3);
        assert!(note.contains('3'), "{note}");
        assert!(note.contains("back into its input box, unsent"), "{note}");
        assert!(
            note.contains("writes them again itself, in their order"),
            "{note}"
        );
        assert!(!note.contains("cleared"), "{note}");
        assert!(!note.contains("draft"), "{note}");
        assert!(!(interrupt.written_during_turn)(1).contains('3'));
    }

    // Pi 1.5.2 on a 100x30 PTY against a stand-in model, one run cut where each key had been
    // drawn (`testdata/pi-captures.md`): a turn running, two steering messages written into it,
    // Esc, three Ctrl-U, a draft typed, and the model selector opened over an emptied box.
    const BOX_WORKING: &[u8] = include_bytes!("../../../testdata/pi_box_working.bin");
    const BOX_STEERED: &[u8] = include_bytes!("../../../testdata/pi_box_steered.bin");
    const BOX_CANCELLED: &[u8] = include_bytes!("../../../testdata/pi_box_cancelled.bin");
    const BOX_CTRL_U: [&[u8]; 3] = [
        include_bytes!("../../../testdata/pi_box_ctrl_u_1.bin"),
        include_bytes!("../../../testdata/pi_box_ctrl_u_2.bin"),
        include_bytes!("../../../testdata/pi_box_ctrl_u_3.bin"),
    ];
    const BOX_DRAFT: &[u8] = include_bytes!("../../../testdata/pi_box_draft.bin");
    const BOX_MODEL_PICKER: &[u8] = include_bytes!("../../../testdata/pi_box_model_picker.bin");

    fn rows(of: &[&str]) -> Option<Vec<String>> {
        Some(of.iter().map(|row| row.to_string()).collect())
    }

    /// The box as the screen shows it after each piece of the run, read as the session reads it:
    /// off the screen as drawn.
    #[test]
    fn the_box_is_read_through_a_turn_a_cancel_and_each_clearing_press() {
        assert_eq!(PiScreen.input_box_screen(), ScreenText::AsDrawn);
        assert!(PiScreen.clears_input_box_with_ctrl_u());
        let mut terminal = TerminalSnapshot::new(ROWS, COLS);
        let mut after = |piece: &[u8]| {
            terminal.process(piece);
            PiScreen.input_box_rows(&terminal.visible_text())
        };
        // Under the rule that reads `Working`, with the steering rows above it and not in it.
        assert_eq!(after(BOX_WORKING), rows(&[""]));
        assert_eq!(after(BOX_STEERED), rows(&[""]));
        // The cancel gives both held messages back, an empty row between them.
        assert_eq!(
            after(BOX_CANCELLED),
            rows(&["steer two", "", "steer three"])
        );
        // A press takes a row's text, then a line break a press.
        assert_eq!(after(BOX_CTRL_U[0]), rows(&["steer two", "", ""]));
        assert_eq!(after(BOX_CTRL_U[1]), rows(&["steer two", ""]));
        assert_eq!(after(BOX_CTRL_U[2]), rows(&["steer two"]));
        assert_eq!(after(BOX_DRAFT), rows(&["steer twomy draft"]));
        // The selector is drawn between rules where the box was, and is no box.
        assert_eq!(after(BOX_MODEL_PICKER), None);
    }

    /// The two messages written into the run of the captures, in the order they were written in.
    const STEERED: [&str; 2] = ["steer two", "steer three"];

    /// `text` without its whitespace, as the given-back messages are looked for in the box.
    fn bare(text: &str) -> String {
        text.split_whitespace().collect()
    }

    // A second run of the same kind (`testdata/pi-captures.md`): the turn running, the same two
    // steering messages, `my draft` typed into the box while the turn still ran, then Esc.
    const DRAFTED: [&[u8]; 4] = [
        include_bytes!("../../../testdata/pi_box_drafted_working.bin"),
        include_bytes!("../../../testdata/pi_box_drafted_steered.bin"),
        include_bytes!("../../../testdata/pi_box_drafted_draft_typed.bin"),
        include_bytes!("../../../testdata/pi_box_drafted_cancelled.bin"),
    ];

    /// What the cancel gives back is read so that the messages can be known in the box: the box's
    /// rows begin with the first message written and hold each message's own text, in the order
    /// written, an empty row between two and no interrupted prompt before them. Where the user had
    /// typed a draft before the cancel, it comes after them, set off by an empty row.
    #[test]
    fn the_messages_a_cancel_gives_back_come_first_in_the_order_written() {
        let mut terminal = TerminalSnapshot::new(ROWS, COLS);
        for piece in [BOX_WORKING, BOX_STEERED, BOX_CANCELLED] {
            terminal.process(piece);
        }
        let given_back = PiScreen
            .input_box_rows(&terminal.visible_text())
            .expect("the box after the cancel");
        assert_eq!(given_back.first().map(String::as_str), Some(STEERED[0]));
        assert_eq!(bare(&given_back.join("\n")), bare(&STEERED.concat()));

        let mut terminal = TerminalSnapshot::new(ROWS, COLS);
        let mut after = |piece: &[u8]| {
            terminal.process(piece);
            PiScreen.input_box_rows(&terminal.visible_text())
        };
        after(DRAFTED[0]);
        after(DRAFTED[1]);
        // During the run the draft is the box's only text; the messages wait above it.
        assert_eq!(after(DRAFTED[2]), rows(&["my draft"]));
        assert_eq!(
            after(DRAFTED[3]),
            rows(&[STEERED[0], "", STEERED[1], "", "my draft"])
        );
    }

    /// With faint cells blanked the rules are gone at the thinking level `off`, and with them the
    /// box: which is why the reader asks for the screen as drawn.
    #[test]
    fn the_box_cannot_be_found_with_faint_cells_blanked() {
        let mut terminal = TerminalSnapshot::new(ROWS, COLS);
        terminal.process(BOX_WORKING);
        terminal.process(BOX_STEERED);
        terminal.process(BOX_CANCELLED);
        assert!(PiScreen.input_box_rows(&terminal.visible_text()).is_some());
        assert_eq!(
            PiScreen.input_box_rows(&terminal.visible_text_undimmed()),
            None
        );
    }

    const RULE: &str = "────────────────────────────────────────";
    const FOOTER: &str = "/private/tmp/project\n0.0%/128k (auto)        m1\n";

    fn framed(upper: &str, between: &[&str]) -> String {
        format!(
            " transcript\n{upper}\n{}\n{RULE}\n{FOOTER}",
            between.join("\n")
        )
    }

    /// The rules Pi draws above the box: plain, with the run's status, and with the count of rows
    /// scrolled out of sight; a completion list under the lower rule leaves the box as it is.
    #[test]
    fn the_box_is_found_under_every_rule_pi_draws() {
        let working = "── ⠸ Working ───────────────────────────";
        let scrolled = "─────────────── ↑ 3 more ───────────────";
        for upper in [RULE, working, scrolled] {
            assert_eq!(
                PiScreen.input_box_rows(&framed(upper, &["first row", "  second"])),
                rows(&["first row", "second"]),
                "{upper}"
            );
        }
        let completing = format!(
            "{RULE}\n/mo\n{RULE}\n→ model          Select model\n  tree           Navigate\n{FOOTER}"
        );
        assert_eq!(PiScreen.input_box_rows(&completing), rows(&["/mo"]));
    }

    /// No box: no rules, one rule, rules with nothing under the lower one, and a transcript rule
    /// that does not start in the first column.
    #[test]
    fn rows_that_are_not_framed_above_the_footer_are_no_box() {
        for screen in [
            String::new(),
            "plain text\nmore\n".to_string(),
            format!("{RULE}\ntext\n{FOOTER}"),
            format!("{RULE}\ntext\n{RULE}\n"),
            format!(" {RULE}\ntext\n {RULE}\n{FOOTER}"),
        ] {
            assert_eq!(PiScreen.input_box_rows(&screen), None, "{screen}");
        }
        // The trust confirmation is framed by rules too.
        let trust = String::from_utf8_lossy(&parsed(&[SCREEN])).into_owned();
        assert_eq!(PiScreen.input_box_rows(&trust), None);
    }

    /// What Pi 1.5.2 draws between rules in the box's place is no box: its selectors, an
    /// extension's dialogs, the sign-in menu and the MCP list, each as measured.
    #[test]
    fn selectors_and_dialogs_are_not_the_box() {
        let panels: [&[&str]; 8] = [
            &[
                " Probe confirm",
                " Proceed with it?",
                "",
                " → Yes",
                "   No",
                "",
                " ↑↓ navigate  enter select  escape/ctrl+c cancel",
            ],
            &[
                " Probe input",
                "",
                ">",
                "",
                " enter submit  escape/ctrl+c cancel",
            ],
            &[
                "Thinking Level",
                ">",
                "",
                "  Enter to select · Ctrl+S to set as default · Escape/Ctrl+C to cancel",
            ],
            &[
                "→ Auto-compact                      true",
                "  (1/33)",
                "  Type to search · Enter/Space to change · Esc to cancel",
            ],
            &["› SLOW one", "  Message 1 of 1"],
            &[
                " MCP servers",
                " No MCP servers configured.",
                "",
                " escape/ctrl+c close",
            ],
            &[
                "Resume Session (Current Folder)        ◉ Current Folder | ○ All",
                ">",
            ],
            &["  • [system]", "› • assistant: slow start", "  (3/3)"],
        ];
        for panel in panels {
            assert_eq!(
                PiScreen.input_box_rows(&framed(RULE, panel)),
                None,
                "{panel:?}"
            );
        }
        // A draft that only mentions the same words is still the box.
        assert_eq!(
            PiScreen.input_box_rows(&framed(RULE, &["> quoted", "please cancel", ""])),
            rows(&["> quoted", "please cancel", ""])
        );
    }

    /// Pi's placeholder for a long paste, in both its forms, and nothing near it.
    #[test]
    fn a_long_paste_is_read_by_its_placeholder() {
        for text in [
            "[paste #1 +24 lines]",
            "[paste #12 2400 chars]",
            "[paste #3]",
        ] {
            assert!(PiScreen.collapsed_paste(text), "{text}");
        }
        for text in [
            "",
            "[paste #1 +24 lines] and more",
            "[paste # +24 lines]",
            "[paste #1 24 lines]",
            "[paste #1 +x lines]",
            "[paste #1 chars]",
            "paste #1 +24 lines",
        ] {
            assert!(!PiScreen.collapsed_paste(text), "{text}");
        }
    }

    /// The run's own status reports, in the order Pi made them: `idle` once started, `working`
    /// for the turn, and `idle` with no `done` at the Esc, which is how the program status path
    /// reads a turn as interrupted.
    #[test]
    fn the_cancelled_turn_reports_idle_and_no_done() {
        let report = |state: &str| format!("\u{1b}]7501;state={state}:app=pi\u{1b}\\");
        let find = |capture: &[u8], state: &str| {
            let report = report(state);
            capture
                .windows(report.len())
                .position(|window| window == report.as_bytes())
        };
        let started = find(BOX_WORKING, "idle").expect("idle as it starts");
        let turn = find(BOX_WORKING, "working").expect("working for the turn");
        assert!(started < turn);
        assert!(find(BOX_STEERED, "idle").is_none());
        assert!(find(BOX_CANCELLED, "idle").is_some());
        for capture in [BOX_WORKING, BOX_STEERED, BOX_CANCELLED] {
            assert!(find(capture, "done").is_none());
        }
    }

    /// What a 60-column, 30-row Pi 1.5.2 drew from its launch, with two pasted image paths and
    /// text pasted one after another and not sent (`testdata/pi-captures.md`): each path wraps onto
    /// a second row, and the text follows the second path with nothing between.
    const IMAGE_PATHS_60: &[u8] = include_bytes!("../../../testdata/pi_box_image_paths_60.bin");

    /// The box shows each image Octoboard pasted as its path, which counts as one attachment
    /// however the box wraps it, and the text beside them is what is left.
    #[test]
    fn each_pasted_image_path_in_the_box_is_one_attachment() {
        assert_eq!(PiScreen.input_box_screen(), ScreenText::AsDrawn);
        let mut terminal = TerminalSnapshot::new(30, 60);
        terminal.process(IMAGE_PATHS_60);
        let rows = PiScreen
            .input_box_rows(&terminal.visible_text())
            .expect("the box");
        assert_eq!(rows.len(), 4, "{rows:?}");
        let text = rows.join("\n");
        assert_eq!(PiScreen.image_placeholders(&text), 2, "{text}");
        assert_eq!(
            PiScreen.without_image_placeholders(&text).trim(),
            "text after"
        );
    }

    /// A path counts only as the file Octoboard names in a `pasted` directory, bare or quoted,
    /// with either separator; anything else stays text.
    #[test]
    fn only_octoboards_pasted_image_files_count() {
        let screen = PiScreen;
        for (text, count, rest) in [
            ("/run/s/pasted/paste-1-0.png", 1, ""),
            ("/run/s/pasted/paste-17954-12.jpg and more", 1, " and more"),
            ("look /run/s/pasted/paste-1-0.webp", 1, "look "),
            ("'/Users/a b/run/s/pasted/paste-1-0.gif'what", 1, "what"),
            (r"C:\run\s\pasted\paste-1-0.png", 1, ""),
            ("/a/pasted/paste-1-0.png/a/pasted/paste-2-0.png", 2, ""),
            (
                "/run/s/images/paste-1-0.png",
                0,
                "/run/s/images/paste-1-0.png",
            ),
            (
                "/run/s/pasted/shot-1-0.png",
                0,
                "/run/s/pasted/shot-1-0.png",
            ),
            (
                "/run/s/pasted/paste-1-0.bmp",
                0,
                "/run/s/pasted/paste-1-0.bmp",
            ),
            (
                "/run/s/pasted/paste--0.png",
                0,
                "/run/s/pasted/paste--0.png",
            ),
            ("[paste #1 +24 lines]", 0, "[paste #1 +24 lines]"),
            ("", 0, ""),
        ] {
            assert_eq!(screen.image_placeholders(text), count, "{text}");
            assert_eq!(screen.without_image_placeholders(text), rest, "{text}");
        }
    }

    /// A message's leading slash is guarded with a zero-width space, which Pi keeps, not with a
    /// space, which it trims before it runs the command.
    #[test]
    fn a_leading_slash_is_guarded_with_a_zero_width_space() {
        assert_eq!(PiScreen.leading_slash_guard(), "\u{200B}");
        assert_eq!(
            crate::session::term::frame_message(AgentId::PI, "/name x")[0],
            crate::platform::term::Input::Paste("\u{200B}/name x".to_string())
        );
    }

    /// The prompt Pi reports is the text it took from the box, trimmed and with the slash guard.
    #[test]
    fn a_reported_prompt_is_the_typed_text_trimmed() {
        let screen = PiScreen;
        assert!(screen.same_as_submitted("  hello \n", "hello"));
        assert!(screen.same_as_submitted("/name x", "\u{200B}/name x"));
        assert!(!screen.same_as_submitted("hello", "hello there"));
    }

    /// A pasted command draws no completion in Pi, so nothing is offered in its place.
    #[test]
    fn a_pasted_command_is_offered_no_completion() {
        let screen = format!(
            "{rule}\n/na\n{rule}\n/private/tmp/proj\n0.0%/128k (auto)   m1",
            rule = "\u{2500}".repeat(60)
        );
        assert_eq!(PiScreen.command_completion(&screen), None);
        assert!(!PiScreen.pasted_image_path_loses_enter());
    }
}
