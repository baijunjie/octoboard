//! How Qwen Code's screens read: its input box, its folder-trust dialog, its terminal title and its
//! interrupt key. The measurements, on 0.28.0, are in `docs/agent-cli-reference.md`, section
//! "Qwen Code".
//!
//! The title is `Qwen - <folder>`, padded with spaces, and starts with a glyph that says what the
//! agent is doing: `◐` while a turn runs, `✳` while a permission prompt or a question waits on the
//! user, none at its prompt. Each glyph is followed by a text variation selector (U+FE0E), which
//! keeps the generic title parser from taking the glyph off, so a title reaches this reader with
//! its glyph still on it. The launch pins the two settings that could switch the title or its
//! glyph off (`super::settings`).
//!
//! Qwen Code also reports its state through OSC 7501, from its first screen on, and the session's
//! terminal reads the running turn, the return to the prompt and the wait on the user from those
//! reports once one has been made (`crate::session::title_activity`,
//! `AgentProvider::reported_signals`). So what the title says below is read only until then.

use crate::session::input_box::box_rows;
use crate::session::screen::{Interrupt, InterruptKey, ScreenReader, TitleCancel};
use crate::session::title_activity::Shown;
use crate::session::trust::{Press, Screen, StartSignal, HOOK_FALLBACK};

pub(super) struct QwenScreen;

impl ScreenReader for QwenScreen {
    /// Qwen Code's input box is read wherever it sits at its prompt (`input_box_rows`): with either
    /// prompt mark, YOLO mode's included, empty or showing its hint, and with attachments above it.
    fn reads_input_box(&self) -> bool {
        true
    }

    fn input_box_rows(&self, screen: &str) -> Option<Vec<String>> {
        input_box_rows(screen)
    }

    /// The hint of an empty box. [`input_box_rows`] already reads it as an empty row; this says so
    /// for a host that draws the screen itself.
    fn input_placeholder(&self, row: &str) -> bool {
        row == PLACEHOLDER
    }

    /// `[Pasted Content <n> chars]`, as Qwen Code draws a long paste in its box.
    fn collapsed_paste(&self, text: &str) -> bool {
        text.strip_prefix("[Pasted Content ")
            .and_then(|rest| rest.strip_suffix(" chars]"))
            .is_some_and(|count| !count.is_empty() && count.bytes().all(|b| b.is_ascii_digit()))
    }

    /// A message's leading slash runs as a command even after a space, as `/quit` did (0.28.3); a
    /// zero-width space before it was sent to the model as text.
    fn leading_slash_guard(&self) -> &'static str {
        SLASH_GUARD
    }

    /// What Qwen Code reports of a prompt (`UserPromptSubmit`, 0.28.3) holds more than was
    /// written: the slash guard stays in it, invisible characters are ignored on both sides; the
    /// images attached go first, as `@<path>` references on a line of their own and a blank line;
    /// and the files a prompt names with `@` are read into it after the text, from `--- Content
    /// from referenced files ---` to `--- End of content ---`. The text is compared with the
    /// report without the files read in, and without the images' line where the report starts
    /// with one, so that a prompt which itself starts with an `@` reference still matches.
    fn same_as_submitted(&self, typed: &str, reported: &str) -> bool {
        use crate::session::term::without_invisible;
        let typed = without_invisible(typed);
        let typed = typed.trim();
        let reported = without_referenced_files(reported);
        let same = |text: &str| without_invisible(text).trim() == typed;
        same(reported) || without_attached_images(reported).is_some_and(same)
    }

    /// The images an `Attachments:` row holds, which [`input_box_rows`] gives as the box's first
    /// row: an image path pasted on its own is attached, drawn there as `[clipboard-<n>-0.png]`,
    /// and the box stays empty (0.28.3).
    fn image_placeholders(&self, text: &str) -> usize {
        attachments(text).map_or(0, |row| attachment_spans(row).len())
    }

    /// `text` without its `Attachments:` row.
    fn without_image_placeholders(&self, text: &str) -> String {
        match attachments(text) {
            Some(_) => text
                .split_once('\n')
                .map_or("", |(_, rest)| rest)
                .to_string(),
            None => text.to_string(),
        }
    }

    /// The command Qwen Code's completion highlights under the box once a `/word` is written:
    /// `> stats [model|…]  Show usage statistics dashboard.` for a pasted `/sta` (0.28.3), which
    /// an Enter would run.
    fn command_completion(&self, screen: &str) -> Option<String> {
        command_completion(screen)
    }

    /// A press deletes back to the start of the line, and at its start the line break before it,
    /// idle and mid-turn alike.
    fn clears_input_box_with_ctrl_u(&self) -> bool {
        true
    }

    /// Esc, the user's cancel, puts a message Qwen Code still held for the turn's next model call
    /// back into the box, unsent (0.28.3). Ctrl-C, the user's other cancel and Octoboard's own,
    /// drops it, and the box it leaves empty holds nothing to take for one given back.
    fn cancel_restores_held_messages(&self) -> bool {
        true
    }

    /// Ctrl-C, the user's and Octoboard's own, drops a message still held with nothing put back.
    /// A held message the turn takes is reported as a prompt sent, with its text (0.28.3): one
    /// steered with Enter by the `UserPromptSubmit` of the model call that takes it, inside the
    /// turn, and one queued with Ctrl+Q by the `UserPromptSubmit` of the turn it then runs as. So
    /// a message the cancel dropped can be told from one already taken. A launch with its hooks
    /// switched off reports no prompt, and says otherwise for itself
    /// ([`crate::adapter::AgentProvider::launch_cancel_drops_held_messages`]).
    fn cancel_drops_held_messages(&self) -> bool {
        true
    }

    fn trust_screen(&self) -> &'static Screen {
        &TRUST
    }

    fn title_activity(&self, glyph: Option<char>) -> Option<Shown> {
        match glyph {
            Some(RUNNING) => Some(Shown::Running),
            None => Some(Shown::AtPrompt),
            // A wait on the user is neither; `title_needs_user` reads it.
            Some(_) => None,
        }
    }

    /// Where a wait on the user is read until Qwen Code makes its first report, which then says it
    /// instead. No hook says a prompt was declined, so a hand a hook raised would have nothing to
    /// lower it (`super::hooks`).
    fn title_needs_user(&self, title: &str) -> bool {
        title.trim_start().starts_with(WAITING)
    }

    /// A declined prompt fires no hook and leaves the terminal at the prompt, as a cancel does
    /// where no report says it. The turn is closed as cancelled, so a `Stop` whose hook command
    /// starts slowly still closes it as its own end. The watch that does it is armed by
    /// `super::records`.
    fn title_cancel(&self) -> Option<TitleCancel> {
        Some(TitleCancel::CloseAsCancelled)
    }

    fn interrupt(&self) -> Option<&'static Interrupt> {
        Some(&INTERRUPT)
    }
}

/// The block Qwen Code adds to a reported prompt that names files with `@`: their content, read in
/// after the text.
const REFERENCED_FILES: &str = "\n--- Content from referenced files ---";
const END_OF_CONTENT: &str = "--- End of content ---";

/// `reported` without the files read into it after its text.
fn without_referenced_files(reported: &str) -> &str {
    match reported.find(REFERENCED_FILES) {
        Some(at) if reported.trim_end().ends_with(END_OF_CONTENT) => &reported[..at],
        _ => reported,
    }
}

/// `reported` without the line of `@<path>` references its attached images go out as and the blank
/// line after it, where it starts with one.
fn without_attached_images(reported: &str) -> Option<&str> {
    let (first, rest) = reported.split_once("\n\n")?;
    let images = first
        .split(' ')
        .all(|reference| reference.len() > 1 && reference.starts_with('@'));
    images.then_some(rest)
}

/// The glyph the title starts with while a turn runs.
const RUNNING: char = '\u{25D0}';

/// The glyph the title starts with while a permission prompt or a question waits on the user.
const WAITING: char = '\u{2733}';

/// The text variation selector that follows either glyph.
const TEXT_PRESENTATION: char = '\u{FE0E}';

/// A raw title without the glyph it starts with and the padding around it.
pub(super) fn without_status(raw: &str) -> &str {
    let title = raw.trim();
    title
        .strip_prefix([RUNNING, WAITING])
        .map_or(title, |rest| {
            rest.strip_prefix(TEXT_PRESENTATION).unwrap_or(rest).trim()
        })
}

/// Esc stops a turn only while the input box is empty, and puts the prompt back in the box when
/// the model has shown nothing; with text in the box it does nothing at all. Ctrl-C stops a turn
/// and puts nothing back, and with text in the box clears the text instead, so the box is emptied
/// first. Ctrl-C at an idle Qwen Code asks to be pressed again to quit.
///
/// A message submitted mid-turn is held: steered with Enter, until the next model call of the
/// turn; queued with Ctrl+Q, until the turn is over (0.28.3). Ctrl-C drops a message still held,
/// and Octoboard writes it again where the launch's hooks are on
/// ([`ScreenReader::cancel_drops_held_messages`]), not where they are switched off
/// ([`crate::adapter::AgentProvider::launch_cancel_drops_held_messages`]); Esc, which Octoboard
/// does not press, puts it back in the input box
/// ([`ScreenReader::cancel_restores_held_messages`]). So the key pressed here puts nothing back,
/// neither prompt nor held message, and `restores_prompt` stays off: there is nothing for the
/// interrupt to read the box for and clear after its own key.
const INTERRUPT: Interrupt = Interrupt {
    key: InterruptKey::CtrlCAfterClearing { undo: None },
    restores_prompt: false,
    written_during_turn,
    queued_waiting: None,
};

/// Qwen Code holds a message submitted mid-turn for the turn's next model call, and the Ctrl-C
/// that stops the turn drops what it still held (0.28.3). A launch with its hooks on has those
/// queued again, and one with them switched off has none
/// ([`crate::adapter::AgentProvider::launch_cancel_drops_held_messages`]); the note is the
/// reader's, one for every launch, so it says both and has the caller send again only what is
/// neither acted on nor queued, which `get_session` shows, never one Octoboard writes again.
fn written_during_turn(written: usize) -> String {
    format!(
        " Octoboard wrote about {written} message(s) into the session during that turn. Qwen \
         Code holds such a message for the turn's next model call, and the cancel dropped any it \
         still held. Where the session's hooks are on, Octoboard writes those again after the \
         next message, counted in `get_session`'s `queued_messages`; where they are switched off \
         (its start said so), none is written again. Check with `get_session` which of them the \
         session acted on and how many are queued, and send again only what is neither."
    )
}

/// The marks the prompt row of the input box starts with: `>`, and `*` in YOLO mode.
const PROMPT_MARKS: [char; 2] = ['>', '*'];

/// The hint Qwen Code shows in an empty input box.
const PLACEHOLDER: &str = "Type your message or @path/to/file";

/// The zero-width space Qwen Code draws where the cursor is in a box that holds text, after a
/// space when it is at the end of a row (0.28.3). It is no part of the text.
const CURSOR_MARK: char = '\u{200B}';

/// Whether `screen` shows the input box, which is not drawn while a prompt is up
/// (`super::interaction`).
pub(super) fn input_box_present(screen: &str) -> bool {
    input_box_rows(screen).is_some()
}

/// What a message's leading slash is preceded by.
const SLASH_GUARD: &str = "\u{200B}";

/// What the row above the box that lists attached images starts with.
const ATTACHMENTS: &str = "Attachments:";

/// The most rows an `Attachments:` row is read over where it wraps: ten images, the most a
/// message takes, make about three rows of the measured width.
const ATTACHMENT_ROWS: usize = 4;

/// The box text's first row where it is the attachments' row.
fn attachments(text: &str) -> Option<&str> {
    text.lines()
        .next()
        .filter(|row| row.starts_with(ATTACHMENTS))
}

/// The `[name]` spans of an attachments row.
fn attachment_spans(row: &str) -> Vec<&str> {
    let mut spans = Vec::new();
    let mut rest = row.strip_prefix(ATTACHMENTS).unwrap_or(row);
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find(']') else {
            break;
        };
        let name = &rest[open + 1..open + close];
        if !name.trim().is_empty() {
            spans.push(name);
        }
        rest = &rest[open + close + 1..];
    }
    spans
}

/// The rows of the `Attachments:` row right above the box's top rule at `rule`, joined into one
/// where it wraps; `None` where there is none.
fn attachments_above(rows: &[&str], rule: usize) -> Option<String> {
    let above = &rows[rule.saturating_sub(ATTACHMENT_ROWS)..rule];
    let at = above
        .iter()
        .rposition(|row| row.trim_start().starts_with(ATTACHMENTS))?;
    let joined: Vec<&str> = above[at..].iter().map(|row| row.trim()).collect();
    joined
        .iter()
        .skip(1)
        .all(|row| !row.is_empty())
        .then(|| joined.join(" "))
}

/// The command [`QwenScreen::command_completion`] reads: the name on the completion list's
/// highlighted row (`> name …`), the first of the rows right below the box's bottom rule that
/// starts with the mark.
fn command_completion(screen: &str) -> Option<String> {
    let rows: Vec<&str> = screen.lines().map(str::trim_end).collect();
    let typed = input_box_rows(screen)?;
    if !typed.iter().any(|row| row.starts_with('/')) {
        return None;
    }
    let top = (1..rows.len()).rev().find(|&row| {
        is_rule(rows[row - 1])
            && rows[row]
                .chars()
                .next()
                .is_some_and(|c| PROMPT_MARKS.contains(&c))
    })?;
    let bottom = top + rows[top..].iter().position(|row| is_rule(row))?;
    rows[bottom + 1..]
        .iter()
        .map(|row| row.trim())
        .take_while(|row| !row.is_empty())
        .find_map(|row| row.strip_prefix("> "))
        .and_then(|row| row.split_whitespace().next())
        .map(str::to_string)
}

/// The rows of Qwen Code's input box: from the last row that starts with a prompt mark right below
/// a horizontal rule, up to the next rule, without the mark of the cursor.
///
/// The hint of an empty box is drawn grey and not faint, so it is still there on a screen with
/// faint cells blanked, where a box holding it would read as holding a draft: a box that shows
/// nothing but the hint is given as one empty row.
fn input_box_rows(screen: &str) -> Option<Vec<String>> {
    let rows: Vec<&str> = screen.lines().map(str::trim_end).collect();
    let screen_rows = rows.clone();
    let mark = |row: &str| {
        let mut chars = row.chars();
        let mark = chars.next().filter(|c| PROMPT_MARKS.contains(c))?;
        matches!(chars.next(), None | Some(' ')).then_some(mark)
    };
    let start = (1..rows.len())
        .rev()
        .find(|&row| mark(rows[row]).is_some() && is_rule(rows[row - 1]))?;
    let end = start + 1 + rows[start + 1..].iter().position(|row| is_rule(row))?;
    let mut rows: Vec<String> = box_rows(rows[start..end].iter().copied(), mark(rows[start])?)
        .iter()
        .map(|row| row.replace(CURSOR_MARK, "").trim().to_string())
        .collect();
    if rows[0] == PLACEHOLDER && rows[1..].iter().all(String::is_empty) {
        rows[0].clear();
    }
    // Attached images are drawn above the box and sent with what it holds; neither Ctrl-U nor
    // Esc takes them off (0.28.3), so they are part of what the box holds.
    if let Some(attached) = attachments_above(&screen_rows, start - 1) {
        rows.insert(0, attached);
    }
    Some(rows)
}

/// Whether a row is one of the horizontal rules around the input box.
fn is_rule(row: &str) -> bool {
    let row = row.trim();
    row.chars().count() >= 20 && row.chars().all(|c| c == '\u{2500}')
}

/// Qwen Code 0.28.0, shown only where the user has turned folder trust on
/// (`security.folderTrust.enabled`, off by default): the question, then "Trust folder (<name>)"
/// with the cursor on it, "Trust parent folder (<name>)" and "Don't trust (esc)". Enter trusts the
/// folder, and Qwen Code then starts over in the same process.
///
/// Its `SessionStart` runs before the dialog is answered, so that hook is not registered (see
/// [`super`]) and no hook says its startup is behind it. Its input box does: it is never drawn
/// before the dialog, nor between the dialog being accepted and the process having started over,
/// and comes up 0.5 s to 1 s after the dialog goes, or at once when the dialog is declined. The
/// fallback bounds the wait for a box this reader fails to recognise.
pub(crate) const TRUST: Screen = Screen {
    first_draw: &[&[
        "Doyoutrustthisfolder?",
        "\u{203A}1.Trustfolder(",
        "2.Trustparentfolder(",
        "3.Don'ttrust(esc)",
    ]],
    accept: Some("1.Trustfolder("),
    others: &["2.Trustparentfolder(", "3.Don'ttrust(esc)"],
    cursor: Some('\u{203A}'),
    press: Press::Enter,
    next_screen: None,
    input_ends_wait: true,
    start_signal: StartSignal::InputBox,
    hook_fallback: Some(HOOK_FALLBACK),
    waits_for_paste_mode: false,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::provider;
    use crate::protocol::AgentId;
    use crate::session::trust::{reading_of, TrustState};
    use crate::terminal::snapshot::TerminalSnapshot;
    use crate::terminal::title::Parser;

    /// The reader declares that it reads its input box wherever the agent is at its prompt, so a turn
    /// opened off the title closes only while the box is drawn (`crate::session::title_status`);
    /// the tests of `input_box_rows` hold it to every prompt state captured.
    #[test]
    fn the_input_box_is_read_wherever_the_agent_is_at_its_prompt() {
        assert!(QwenScreen.reads_input_box());
    }

    // Qwen Code 0.28.0, see `testdata/qwen-captures.md`: the bottom of the rendered screen with
    // the input box in each state, and the terminal output up to the first drawing of the trust
    // dialog.
    const PLACEHOLDER_BOX: &str = include_str!("../../../testdata/qwen_input_placeholder.txt");
    const DRAFT_BOX: &str = include_str!("../../../testdata/qwen_input_draft.txt");
    const PASTED_BOX: &str = include_str!("../../../testdata/qwen_input_pasted.txt");
    const YOLO_BOX: &str = include_str!("../../../testdata/qwen_input_yolo.txt");
    // Qwen Code 0.28.3: a message submitted while a turn ran and still held, back in the box after
    // the turn was cancelled with Esc, with the cursor's zero-width space after it.
    const GIVEN_BACK_BOX: &str = include_str!("../../../testdata/qwen_input_given_back.txt");
    const TRUST_CAPTURE: &[u8] = include_bytes!("../../../testdata/qwen_trust_screen.bin");
    // Qwen Code 0.28.3, from the launch: through Enter at the dialog to the fresh start's input
    // box, through Esc to the input box, and through one Down, which leaves the cursor on the
    // second option.
    const TRUST_ACCEPTED: &[u8] = include_bytes!("../../../testdata/qwen_trust_accepted.bin");
    const TRUST_DECLINED: &[u8] = include_bytes!("../../../testdata/qwen_trust_declined.bin");
    const TRUST_CURSOR_MOVED: &[u8] =
        include_bytes!("../../../testdata/qwen_trust_cursor_moved.bin");

    const HINT: &str = "Type your message or @path/to/file";

    fn screen() -> &'static dyn ScreenReader {
        provider(AgentId::QWEN).screen()
    }

    /// What a 120x40 terminal shows after `bytes`, as the capture was taken.
    fn replayed(bytes: &[u8]) -> String {
        let mut terminal = TerminalSnapshot::new(40, 120);
        terminal.process(bytes);
        terminal.visible_text()
    }

    #[test]
    fn the_input_box_is_the_rows_between_its_rules_without_the_prompt_mark() {
        // The hint of an empty box is no text: the box reads as one empty row.
        assert_eq!(
            screen().input_box_rows(PLACEHOLDER_BOX),
            Some(vec![String::new()])
        );
        assert_eq!(
            screen().input_box_rows(DRAFT_BOX),
            Some(vec!["pasted line one".to_string(), "line two".to_string()])
        );
        assert_eq!(
            screen().input_box_rows(PASTED_BOX),
            Some(vec!["[Pasted Content 379 chars]".to_string()])
        );
    }

    /// A message Qwen Code held for a turn and gave back at an Esc reads as the text it is: the
    /// zero-width space drawn at the cursor, and the space before it, are not part of it.
    #[test]
    fn a_message_given_back_at_a_cancel_reads_without_the_cursors_mark() {
        assert!(GIVEN_BACK_BOX.contains("steered message \u{200B}"));
        assert_eq!(
            screen().input_box_rows(GIVEN_BACK_BOX),
            Some(vec!["steered message".to_string()])
        );
    }

    /// A cancel gives held messages back: after the user's Esc the box shows the one message that
    /// was held and nothing else, which is what a message given back is known by. Octoboard's own
    /// key, Ctrl-C, puts nothing back, so the interrupt reads no box for it.
    #[test]
    fn a_cancel_gives_a_held_message_back_alone_in_the_box() {
        assert!(screen().cancel_restores_held_messages());
        assert!(GIVEN_BACK_BOX.contains("Request cancelled."));
        let rows = screen()
            .input_box_rows(GIVEN_BACK_BOX)
            .expect("the box after the cancel");
        assert_eq!(rows, ["steered message"]);
        let interrupt = screen().interrupt().expect("Qwen Code can be interrupted");
        assert!(!interrupt.restores_prompt);
    }

    /// In YOLO mode the prompt mark is `*`, and the box reads the same.
    #[test]
    fn the_input_box_of_yolo_mode_reads_the_same() {
        assert_eq!(screen().input_box_rows(YOLO_BOX), Some(vec![String::new()]));
    }

    /// A screen of Qwen Code's shape with `rows` as the rows of its input box.
    fn box_of(rows: &[&str]) -> String {
        let rule = "\u{2500}".repeat(100);
        format!(
            "  \u{25C6} Hello from stub.\n\n{rule}\n{}\n{rule}\n  \u{279C} proj1\n",
            rows.join("\n")
        )
    }

    /// Only a box that shows nothing but the hint is empty: the hint with an empty row below it is
    /// one too, and text that merely starts with it, or has more under it, is a draft.
    #[test]
    fn only_a_box_showing_nothing_but_the_hint_reads_as_empty() {
        let rows_of = |rows: &[&str]| screen().input_box_rows(&box_of(rows));
        assert_eq!(
            rows_of(&[">   Type your message or @path/to/file"]),
            Some(vec![String::new()])
        );
        assert_eq!(
            rows_of(&[">   Type your message or @path/to/file", "", ""]),
            Some(vec![String::new(); 3])
        );
        assert_eq!(
            rows_of(&[">   Type your message or @path/to/file", "  and more"]),
            Some(vec![HINT.to_string(), "and more".to_string()])
        );
        assert_eq!(
            rows_of(&[">   Type your message or @path/to/file now"]),
            Some(vec![format!("{HINT} now")])
        );
        assert_eq!(
            rows_of(&[">   Type your message"]),
            Some(vec!["Type your message".to_string()])
        );
        // The box a user left truly empty is the same.
        assert_eq!(rows_of(&[">"]), Some(vec![String::new()]));
    }

    /// A `>` in the conversation above the box (indented, outside the rules) is not the box.
    #[test]
    fn the_conversation_above_the_box_is_not_taken_for_it() {
        assert!(PLACEHOLDER_BOX.contains("  > hello there"));
        let above = PLACEHOLDER_BOX
            .lines()
            .take_while(|row| !row.starts_with('\u{2500}'))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(screen().input_box_rows(&above), None);
    }

    #[test]
    fn a_screen_without_the_box_has_none() {
        assert_eq!(screen().input_box_rows(""), None);
        assert_eq!(
            screen().input_box_rows("> not between rules\nnor under one\n"),
            None
        );
        // A rule too short to be the box's does not open one.
        assert_eq!(
            screen()
                .input_box_rows("\u{2500}\u{2500}\u{2500}\n> hello\n\u{2500}\u{2500}\u{2500}\n"),
            None
        );
        // The box is never drawn before the trust dialog, or the dialog would end the wait.
        assert_eq!(screen().input_box_rows(&replayed(TRUST_CAPTURE)), None);
    }

    #[test]
    fn the_empty_boxs_hint_is_a_placeholder_and_a_draft_is_not() {
        assert!(screen().input_placeholder(HINT));
        assert!(!screen().input_placeholder("pasted line one"));
        assert!(!screen().input_placeholder(""));
        assert!(!screen().input_placeholder("Type your message"));
    }

    #[test]
    fn a_long_paste_shows_as_its_character_count() {
        assert!(screen().collapsed_paste("[Pasted Content 379 chars]"));
        assert!(screen().collapsed_paste("[Pasted Content 1 chars]"));
        for text in [
            "",
            "pasted line one",
            "[Pasted Content chars]",
            "[Pasted Content 12a chars]",
            "[Pasted Content -3 chars]",
            "look [Pasted Content 379 chars]",
            "[Pasted Content 379 chars] and more",
            "[Pasted Content 379 chars",
        ] {
            assert!(!screen().collapsed_paste(text), "{text:?}");
        }
    }

    #[test]
    fn ctrl_u_clears_the_box() {
        assert!(screen().clears_input_box_with_ctrl_u());
    }

    /// Qwen Code's title as the session's terminal observer reads it: sent as an OSC sequence,
    /// padded to 80 columns, with the glyph followed by U+FE0E.
    fn read_title(title: &str) -> (String, Option<char>) {
        let mut parser = Parser::default();
        let padded = format!("{title:<80}");
        let read = parser
            .feed(format!("\x1b]0;{padded}\x07").as_bytes())
            .expect("a completed title")
            .expect("a title with text");
        (read, parser.leading_glyph())
    }

    const RUNNING_TITLE: &str = "\u{25D0}\u{FE0E} Qwen - proj";
    const WAITING_TITLE: &str = "\u{2733}\u{FE0E} Qwen - proj";
    const PLAIN_TITLE: &str = "Qwen - proj";

    /// A running turn is the circle, the prompt is no glyph, and a wait on the user is neither, so
    /// the activity says nothing then and `title_needs_user` reads it.
    #[test]
    fn the_title_says_running_at_the_prompt_or_neither() {
        let (_, glyph) = read_title(RUNNING_TITLE);
        assert_eq!(screen().title_activity(glyph), Some(Shown::Running));
        let (_, glyph) = read_title(PLAIN_TITLE);
        assert_eq!(glyph, None);
        assert_eq!(screen().title_activity(glyph), Some(Shown::AtPrompt));
        let (_, glyph) = read_title(WAITING_TITLE);
        assert_eq!(screen().title_activity(glyph), None);
    }

    #[test]
    fn only_the_waiting_titles_star_says_the_user_is_needed() {
        let (waiting, _) = read_title(WAITING_TITLE);
        assert!(screen().title_needs_user(&waiting), "{waiting:?}");
        for title in [RUNNING_TITLE, PLAIN_TITLE] {
            let (read, _) = read_title(title);
            assert!(!screen().title_needs_user(&read), "{read:?}");
        }
        // The padding the agent sends is not part of the reading either way.
        assert!(screen().title_needs_user(&format!("{WAITING_TITLE:<80}")));
        assert!(!screen().title_needs_user(""));
    }

    #[test]
    fn a_cancel_with_no_hook_closes_the_turn_as_cancelled() {
        assert_eq!(screen().title_cancel(), Some(TitleCancel::CloseAsCancelled));
    }

    #[test]
    fn the_title_without_its_status_is_the_default_title() {
        for (raw, expected) in [
            (PLAIN_TITLE, PLAIN_TITLE),
            (RUNNING_TITLE, PLAIN_TITLE),
            (WAITING_TITLE, PLAIN_TITLE),
            // Padded as the agent sends it.
            (format!("{RUNNING_TITLE:<80}").as_str(), PLAIN_TITLE),
            (format!("{PLAIN_TITLE:<80}").as_str(), PLAIN_TITLE),
            // The selector is optional, and a glyph in the middle is the title's own.
            ("\u{25D0} Qwen - proj", PLAIN_TITLE),
            ("Qwen \u{25D0} proj", "Qwen \u{25D0} proj"),
            ("", ""),
        ] {
            assert_eq!(without_status(raw), expected, "{raw:?}");
        }
    }

    /// Ctrl-C stops a running turn and clears a draft instead when the box holds one, so the box
    /// is emptied first; it puts nothing back and the turn's own messages are accounted for.
    #[test]
    fn a_turn_is_interrupted_by_ctrl_c_after_clearing_the_box() {
        let interrupt = screen().interrupt().expect("Qwen Code can be interrupted");
        assert_eq!(
            interrupt.key,
            InterruptKey::CtrlCAfterClearing { undo: None }
        );
        assert!(!interrupt.restores_prompt);
        assert!(interrupt.queued_waiting.is_none());
        let told = (interrupt.written_during_turn)(3);
        assert!(told.contains('3'), "{told}");
        // A message the turn still held is dropped by the Ctrl-C and written again by Octoboard
        // where the hooks are on, not where they are off: the caller is told both, and to send
        // again only what is neither acted on nor queued, so nothing is delivered twice.
        assert!(screen().cancel_drops_held_messages());
        assert!(told.contains("dropped"), "{told}");
        assert!(told.contains("writes those again"), "{told}");
        assert!(told.contains("none is written again"), "{told}");
        assert!(told.contains("queued_messages"), "{told}");
        assert!(told.contains("send again only what is neither"), "{told}");
    }

    /// The capture replayed through the matcher the watch and the press use: the first drawing is
    /// sighted whether read off the output or off the parsed screen, the screen is up, and the
    /// cursor is on "1. Trust folder", the option Octoboard presses.
    #[test]
    fn the_capture_reads_as_the_trust_dialog_with_the_cursor_on_the_option_that_trusts() {
        let trust = screen().trust_screen();
        let on_the_output = reading_of(trust, TRUST_CAPTURE);
        assert!(on_the_output.shown, "{on_the_output:?}");
        assert!(on_the_output.up, "{on_the_output:?}");
        assert_eq!(
            on_the_output.cursor_on_accept,
            Some(true),
            "{on_the_output:?}"
        );

        let parsed = replayed(TRUST_CAPTURE);
        let on_the_screen = reading_of(trust, parsed.as_bytes());
        assert!(on_the_screen.shown, "{parsed}");
        assert!(on_the_screen.up, "{parsed}");
        assert_eq!(on_the_screen.cursor_on_accept, Some(true), "{parsed}");
    }

    /// Neither the dialog's own screen nor a capture of it is the screen gone or replaced by the
    /// next one, and nothing but the whole first drawing sights it.
    #[test]
    fn the_dialog_is_not_gone_while_it_is_up_and_a_partial_drawing_is_not_it() {
        let trust = screen().trust_screen();
        let parsed = replayed(TRUST_CAPTURE);
        for capture in [TRUST_CAPTURE, parsed.as_bytes()] {
            let reading = reading_of(trust, capture);
            assert!(!reading.gone, "{reading:?}");
            assert!(!reading.replaced, "{reading:?}");
        }
        for text in ["Do you trust this folder?\n", "", PLACEHOLDER_BOX] {
            let reading = reading_of(trust, text.as_bytes());
            assert!(!reading.shown, "{text:?}");
            assert!(!reading.up, "{text:?}");
            assert_eq!(reading.cursor_on_accept, None, "{text:?}");
        }
    }

    /// After one Down the cursor is on "2. Trust parent folder": read as not on the option that
    /// trusts, off the output, whose last cursor glyph is the moved one, and off the parsed screen,
    /// where the dialog is still up.
    #[test]
    fn a_cursor_moved_by_a_down_is_not_on_the_option_that_trusts() {
        let trust = screen().trust_screen();
        let on_the_output = reading_of(trust, TRUST_CURSOR_MOVED);
        assert!(on_the_output.shown, "{on_the_output:?}");
        assert_eq!(
            on_the_output.cursor_on_accept,
            Some(false),
            "{on_the_output:?}"
        );

        let parsed = replayed(TRUST_CURSOR_MOVED);
        assert!(
            parsed.contains("\u{203A} 2. Trust parent folder"),
            "the cursor is on the second option: {parsed}"
        );
        let on_the_screen = reading_of(trust, parsed.as_bytes());
        assert!(on_the_screen.up, "{parsed}");
        assert_eq!(on_the_screen.cursor_on_accept, Some(false), "{parsed}");
        assert!(!on_the_screen.gone, "{parsed}");
    }

    /// The bytes of `capture` after the last drawing of the dialog: what Qwen Code printed once
    /// the key that answered it went in.
    fn after_the_dialog(capture: &[u8]) -> &[u8] {
        let last = b"Don't trust";
        let at = capture
            .windows(last.len())
            .rposition(|window| window == last)
            .expect("the dialog is drawn");
        &capture[at + last.len()..]
    }

    /// Once Enter has trusted the folder, or Esc has declined, what is printed holds nothing of
    /// the dialog: the screen is gone, and it is replaced by what follows it, off the output after
    /// the key and off the parsed screen at the end of the capture. The whole capture still holds
    /// the dialog, so it is neither.
    #[test]
    fn the_dialog_is_gone_once_it_is_accepted_or_declined() {
        let trust = screen().trust_screen();
        for (name, capture) in [("accepted", TRUST_ACCEPTED), ("declined", TRUST_DECLINED)] {
            let whole = reading_of(trust, capture);
            assert!(whole.shown, "{name}: {whole:?}");
            assert!(!whole.gone, "{name}: {whole:?}");
            assert!(!whole.replaced, "{name}: {whole:?}");

            let after = reading_of(trust, after_the_dialog(capture));
            assert!(after.gone, "{name}: {after:?}");
            assert!(after.replaced, "{name}: {after:?}");
            assert!(!after.shown, "{name}: {after:?}");

            let parsed = replayed(capture);
            let at_the_end = reading_of(trust, parsed.as_bytes());
            assert!(!at_the_end.shown, "{name}: {parsed}");
            assert!(!at_the_end.up, "{name}: {parsed}");
            assert!(at_the_end.gone, "{name}: {parsed}");
            assert_eq!(at_the_end.cursor_on_accept, None, "{name}: {parsed}");
        }
    }

    /// Accepted, Qwen Code says it restarts and then starts over in the same process; the input
    /// box is drawn only after the fresh start, never between the dialog and it, which is what lets
    /// the box say the startup is behind it. Declined, the box comes up at once.
    #[test]
    fn the_input_box_comes_only_after_the_dialog_is_answered_and_started_over() {
        let restart = TRUST_ACCEPTED
            .windows(b"state=clear".len())
            .position(|window| window == b"state=clear")
            .expect("the report that ends the first run");
        let restarting = replayed(&TRUST_ACCEPTED[..restart]);
        assert!(
            restarting.contains("restarting to apply the trust changes"),
            "{restarting}"
        );
        assert_eq!(screen().input_box_rows(&restarting), None, "{restarting}");
        assert_eq!(screen().input_box_rows(&replayed(TRUST_CURSOR_MOVED)), None);
        for (name, capture) in [("accepted", TRUST_ACCEPTED), ("declined", TRUST_DECLINED)] {
            let parsed = replayed(capture);
            assert_eq!(
                screen().input_box_rows(&parsed),
                Some(vec![String::new()]),
                "{name}: {parsed}"
            );
        }
    }

    /// What the dialog leaves other options to: both of them named as the dialog words them, and
    /// Enter pressing the one the cursor starts on.
    #[test]
    fn the_other_options_are_named_and_enter_presses_the_first() {
        let trust = screen().trust_screen();
        assert_eq!(trust.accept, Some("1.Trustfolder("));
        assert_eq!(trust.others, ["2.Trustparentfolder(", "3.Don'ttrust(esc)"]);
        assert_eq!(trust.cursor, Some('\u{203A}'));
        assert!(matches!(trust.press, Press::Enter));
    }

    /// The first drawing, replayed off the terminal as the capture holds it, is sighted, and the
    /// session's messages are held for it until the input box is up.
    #[test]
    fn the_first_drawing_of_the_trust_dialog_is_sighted() {
        let state = TrustState::new(AgentId::QWEN, None);
        assert!(!state.waiting());
        assert!(state.holds_writes());
        state.feed_screen(|| replayed(TRUST_CAPTURE), 0, TRUST_CAPTURE.len() as u64);
        assert!(state.waiting());
        assert!(state.holds_writes());

        // The same drawing in the output stream.
        let state = TrustState::new(AgentId::QWEN, None);
        state.feed(TRUST_CAPTURE, 0, TRUST_CAPTURE.len() as u64);
        assert!(state.waiting());
    }

    /// A partial drawing, the input box or an empty screen is no sighting.
    #[test]
    fn a_screen_that_is_not_the_trust_dialog_is_not_sighted() {
        for text in [PLACEHOLDER_BOX, "Do you trust this folder?\n", ""] {
            let state = TrustState::new(AgentId::QWEN, None);
            state.feed_screen(|| text.to_string(), 0, 0);
            assert!(!state.waiting(), "{text:?}");
        }
    }

    /// No hook marks the end of its startup: its input box does, and a hook that does not come
    /// still lets the messages through after the fallback.
    #[test]
    fn its_startup_is_over_at_the_input_box_with_the_fallback_kept() {
        let screen = screen().trust_screen();
        assert_eq!(screen.start_signal, StartSignal::InputBox);
        assert_eq!(screen.hook_fallback, Some(HOOK_FALLBACK));
        assert!(screen.input_ends_wait);
        assert!(!screen.waits_for_paste_mode);
    }

    // Qwen Code 0.28.3: the output from just before the first paste, replayed and read with faint
    // cells blanked, as the session gives the input box to this reader
    // (`ScreenReader::input_box_screen`).
    fn input_capture(bytes: &[u8]) -> String {
        let mut terminal = TerminalSnapshot::new(40, 120);
        terminal.process(bytes);
        terminal.visible_text_undimmed()
    }
    fn attached_box() -> String {
        input_capture(include_bytes!(
            "../../../testdata/qwen_input_attachments.bin"
        ))
    }
    fn two_attached_box() -> String {
        input_capture(include_bytes!(
            "../../../testdata/qwen_input_two_attachments.bin"
        ))
    }
    fn slash_box() -> String {
        input_capture(include_bytes!(
            "../../../testdata/qwen_input_slash_completion.bin"
        ))
    }

    /// An attached image is drawn above the box and is part of what it holds: its row comes first,
    /// counted as one image, and the rest is the text written.
    #[test]
    fn attached_images_are_the_boxs_first_row() {
        let screen = screen();
        let rows = screen.input_box_rows(&attached_box()).expect("the box");
        assert_eq!(rows[0], "Attachments: [clipboard-1795444348751-0.png]");
        assert_eq!(rows[1], "hello");
        let text = rows.join("\n");
        assert_eq!(screen.image_placeholders(&text), 1);
        assert_eq!(screen.without_image_placeholders(&text), "hello");

        // Two images and no text: the box holds the images alone.
        let rows = screen.input_box_rows(&two_attached_box()).expect("the box");
        let text = rows.join("\n");
        assert_eq!(screen.image_placeholders(&text), 2);
        assert_eq!(screen.without_image_placeholders(&text).trim(), "");
    }

    /// Only a first row that is the attachments' counts; text that merely holds brackets does not.
    #[test]
    fn only_the_attachments_row_holds_images() {
        let screen = screen();
        assert_eq!(screen.image_placeholders("look [here] and [there]"), 0);
        assert_eq!(
            screen.image_placeholders("Attachments: [a.png] [b.jpg] []"),
            2
        );
        assert_eq!(
            screen.without_image_placeholders("text\nAttachments: [a.png]"),
            "text\nAttachments: [a.png]"
        );
        // A box with nothing attached reads as before.
        let draft = screen.input_box_rows(DRAFT_BOX).expect("the box");
        assert!(!draft[0].starts_with(ATTACHMENTS));
        assert_eq!(screen.image_placeholders(&draft.join("\n")), 0);
        assert!(attachments_above(&["x", "Attachments: [a.png]", ""], 3).is_none());
        assert_eq!(
            attachments_above(&["x", "  Attachments: [a.png]", "  [b.png]"], 3).as_deref(),
            Some("Attachments: [a.png] [b.png]")
        );
    }

    /// The completion's highlighted command under the box, for a `/word` written; none where the
    /// box holds no command or no list is drawn.
    #[test]
    fn the_completion_offers_its_highlighted_command() {
        let screen = screen();
        assert_eq!(
            screen.command_completion(&slash_box()).as_deref(),
            Some("stats")
        );
        assert_eq!(screen.command_completion(&attached_box()), None);
        assert_eq!(screen.command_completion(PLACEHOLDER_BOX), None);
    }

    /// A message's leading slash reaches Qwen Code behind a zero-width space, which keeps it from
    /// running as a command where a space does not (0.28.3).
    #[test]
    fn a_leading_slash_is_guarded_by_a_zero_width_space() {
        use crate::platform::term::Input;
        assert_eq!(
            crate::session::term::frame_message(AgentId::QWEN, "/x"),
            [Input::Paste("\u{200B}/x".to_string()), Input::Enter]
        );
        assert_eq!(
            crate::session::term::frame_message(AgentId::QWEN, "x /y"),
            [Input::Paste("x /y".to_string()), Input::Enter]
        );
    }

    /// The prompts Qwen Code reported (`testdata/qwen_hooks_prompts.jsonl`, from line 8 on), in
    /// order: two guarded messages, one with an image and one with an image and the guard.
    fn reported_prompts() -> Vec<String> {
        include_str!("../../../testdata/qwen_hooks_prompts.jsonl")
            .lines()
            .skip(7)
            .map(|line| {
                let payload: serde_json::Value = serde_json::from_str(line).expect("JSON");
                payload["prompt"].as_str().expect("a prompt").to_string()
            })
            .collect()
    }

    /// A guarded message is what Qwen Code reports with the guard in it, and a message sent with
    /// an image is its report without the image's reference and the content read in; another
    /// text is not.
    #[test]
    fn a_reported_prompt_is_the_message_written() {
        let screen = screen();
        let reported = reported_prompts();
        assert_eq!(reported.len(), 4);
        let pairs = [
            ("/stats probe guarded", &reported[0]),
            ("/help two\nlines", &reported[1]),
            ("hello", &reported[2]),
            ("/stats with image", &reported[3]),
        ];
        for (typed, reported) in pairs {
            assert!(screen.same_as_submitted(typed, reported), "{typed:?}");
            // As Octoboard writes it, the guard in front.
            assert!(
                screen.same_as_submitted(&format!("{SLASH_GUARD}{typed}"), reported),
                "{typed:?}"
            );
        }
        assert!(!screen.same_as_submitted("/stats probe", &reported[0]));
        assert!(!screen.same_as_submitted("hell", &reported[2]));
        // A message that itself starts with a reference keeps it; the content read in still goes.
        let named =
            "@a.txt\n\nexplain\n--- Content from referenced files ---\nContent from a.txt:\n\
                     one\n--- End of content ---";
        assert!(screen.same_as_submitted("@a.txt\n\nexplain", named));
        assert!(screen.same_as_submitted("explain", named));
        assert!(!screen.same_as_submitted("@a.txt", named));
    }
}
