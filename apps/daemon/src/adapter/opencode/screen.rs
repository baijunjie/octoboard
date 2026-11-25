//! How OpenCode's terminal reads, measured on 1.19.6.
//!
//! It shows no folder-trust confirmation. Its session starts once its terminal title is written
//! (`OpenCode`): a paste sent before that is lost, one sent right after is taken, and no event of
//! its plugin comes before the first prompt. The launch keeps the title on
//! (`OPENCODE_DISABLE_TERMINAL_TITLE`, in `super`).
//!
//! A turn is interrupted by Esc pressed twice: the first press only arms it (`esc again to
//! interrupt`), the second, taken as a key of its own a tenth of a second later, ends the turn.
//! A lone Esc at the idle prompt does nothing and leaves a draft in the input box as it was, and
//! so do two. The interrupted prompt is not put back in the input box. A prompt submitted while
//! the turn ran had joined it, and does not run once the turn is cancelled.
//!
//! Its input box is a block of rows down a `┃` bar, closed by a rule that starts with `╹`: an
//! empty row, the rows of text, an empty row, then the row naming the agent and the model. A
//! wide terminal draws a sidebar to the right of the box, on the same rows, so a row is read no
//! further than the rule reaches. A permission prompt or a question is drawn in place of the box
//! and has no such rule under it. Outside a session an empty box shows a hint, drawn grey and not
//! faint. Ctrl-U deletes back to the start of the line, and at its start the line break before
//! it, while a turn runs as at the idle prompt.
//!
//! The list OpenCode opens for a `/` or an `@` typed in the box is drawn right above the box, down
//! the same bar, its rows indented one cell where the box's are indented two; the box under it is
//! drawn whole and still takes the keys. A dialog (the command list of ctrl+p, the session list,
//! the model list and the like) is drawn over the conversation with the box whole under it, and
//! takes the keys itself: a paste goes into its search field and the Enter picks from its list,
//! so a message written then is lost. Each has its title and `esc` on its first row, a run of
//! blanks between them, which the conversation's text shares to the left and right of the dialog.
//! The person opens each one themselves, so a session showing one does not wait on them.

use crate::session::dialogs::{DialogKind, DialogPattern, WrittenMessage};
use crate::session::screen::{Interrupt, InterruptKey, ScreenReader};
use crate::session::trust::{Press, Screen, StartSignal};

pub(super) struct OpenCodeScreen;

impl ScreenReader for OpenCodeScreen {
    fn dialogs(&self) -> &'static [DialogPattern] {
        DIALOGS
    }

    fn input_box_rows(&self, screen: &str) -> Option<Vec<String>> {
        input_box_rows(screen)
    }

    /// The hint of an empty box outside a session, `Ask anything… "<an example>"`, drawn grey and
    /// not faint, so it is still there on a screen with faint cells blanked.
    fn input_placeholder(&self, row: &str) -> bool {
        row.strip_prefix(PLACEHOLDER_START)
            .is_some_and(|example| example.len() > 1 && example.ends_with('"'))
    }

    /// A press deletes back to the start of the line, and at its start the line break before it.
    fn clears_input_box_with_ctrl_u(&self) -> bool {
        true
    }

    /// A message OpenCode does not report sending at its prompt while the box reads empty went
    /// into a dialog drawn over the box: measured on 1.19.6, a dialog the reader does not recognise
    /// (one drawn narrower than 40 columns) takes the paste into its own field and submits nothing,
    /// and seven of the ten leave the box drawn whole and empty at 30 columns. A box the dialog
    /// covers (the session list's, the upper rows under the model and provider lists) reads as
    /// unknown, not empty.
    fn empty_box_means_write_went_elsewhere(&self) -> bool {
        true
    }

    /// OpenCode reports a prompt with images as the box held it, each image's `[Image <n>]` and
    /// the space after it before the text (`[Image 1] [Image 2] look at these`, 1.19.6), where
    /// the text recorded as written has none.
    fn same_as_submitted(&self, typed: &str, reported: &str) -> bool {
        if placeholder_spans(reported).is_empty() {
            return typed == reported;
        }
        typed.trim_end() == without_placeholders(reported).trim_end()
    }

    /// `[Image <n>]`, which OpenCode draws in its box for each image path pasted on its own
    /// (1.19.6). A path pasted with other text stays text.
    fn image_placeholders(&self, text: &str) -> usize {
        placeholder_spans(text).len()
    }

    fn without_image_placeholders(&self, text: &str) -> String {
        without_placeholders(text)
    }

    /// The first command of the list OpenCode opens above the box for a lone `/name`, which it
    /// highlights first and an Enter takes (1.19.6): its rows are drawn down the box's bar,
    /// indented one cell, each `/name`, blanks and the command's description.
    fn command_completion(&self, screen: &str) -> Option<String> {
        command_completion(screen)
    }

    /// Whether the list OpenCode opens above the box for a `/` or an `@` is up: a row of the bar
    /// right above the box, indented one cell, as each of its commands or mentions is and as its
    /// `No matching items` is, where nothing matches what was typed (1.19.6). A space typed after
    /// a lone `/name` closes it, whether it matched or not. `None` where no box is read, as under
    /// a permission prompt or a question drawn in its place.
    fn list_shown(&self, screen: &str) -> Option<bool> {
        list_shown(screen)
    }

    /// The list reads `No matching items` where nothing matches what was typed (1.19.6, drawn in
    /// a grey of the palette, not faint): no command is offered, and a space and an Enter would
    /// send `/name ` to the model as a prompt.
    fn list_offers_no_command(&self, screen: &str) -> bool {
        list_row(screen).flatten().as_deref() == Some(NO_MATCH_ROW)
    }

    /// An Enter on a lone `/name` takes the list's first row into the box (`/name `) rather than
    /// running it, and a second Enter runs it; with the space typed after `/name` the list closes
    /// and one Enter runs it, while a space and an Enter written together lose the Enter (1.19.6).
    fn writes_lone_command_with_trailing_space(&self) -> bool {
        true
    }

    fn trust_screen(&self) -> &'static Screen {
        &START
    }

    fn interrupt(&self) -> Option<&'static Interrupt> {
        Some(&INTERRUPT)
    }

    fn interrupt_running_screen(&self) -> Option<fn(&str) -> bool> {
        Some(interrupt_footer)
    }
}

/// OpenCode's attached TUI does not receive the plugin's terminal reports. Its running indicator
/// is the spinner and interrupt hint immediately under the composer.
fn interrupt_footer(screen: &str) -> bool {
    let mut visible = screen.lines().rev().filter(|row| !row.trim().is_empty());
    let Some(footer) = visible.next() else {
        return false;
    };
    let Some((spinner, hint)) = footer.trim_start().split_once(char::is_whitespace) else {
        return false;
    };
    !spinner.is_empty()
        && spinner.chars().all(|cell| matches!(cell, '■' | '⬝'))
        && ["esc interrupt", "esc again to interrupt"]
            .into_iter()
            .any(|text| {
                hint.trim_start()
                    .strip_prefix(text)
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
            })
        && visible.next().is_some_and(|row| rule(row).is_some())
        && input_box_rows(screen).is_some()
}

/// What OpenCode draws in its box for an image pasted as a path, around the image's number.
const IMAGE_OPEN: &str = "[Image ";
const IMAGE_CLOSE: char = ']';

/// Where each `[Image <n>]` of `text` is, with the one space after it where there is one.
fn placeholder_spans(text: &str) -> Vec<std::ops::Range<usize>> {
    let mut spans = Vec::new();
    let mut from = 0;
    while let Some(found) = text[from..].find(IMAGE_OPEN) {
        let start = from + found;
        let digits_at = start + IMAGE_OPEN.len();
        let digits = text[digits_at..]
            .bytes()
            .take_while(u8::is_ascii_digit)
            .count();
        let close = digits_at + digits;
        if digits > 0 && text[close..].starts_with(IMAGE_CLOSE) {
            let mut end = close + IMAGE_CLOSE.len_utf8();
            if text[end..].starts_with(' ') {
                end += 1;
            }
            spans.push(start..end);
            from = end;
        } else {
            from = digits_at;
        }
    }
    spans
}

/// `text` without its `[Image <n>]` placeholders and the space after each.
fn without_placeholders(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    for span in placeholder_spans(text) {
        out.push_str(&text[copied..span.start]);
        copied = span.end;
    }
    out.push_str(&text[copied..]);
    out
}

/// The command the first row of the list above the input box names: the rows right above the
/// box's first row, down the same bar, indented one cell, the first of them `/name`.
fn command_completion(screen: &str) -> Option<String> {
    let rows: Vec<&str> = screen.lines().collect();
    let (rule_row, indent, _) = rows
        .iter()
        .enumerate()
        .rev()
        .find_map(|(at, row)| rule(row).map(|(indent, width)| (at, indent, width)))?;
    // A row of the bar, with what follows the bar.
    let after_bar = |row: &str| -> Option<String> {
        let mut chars = row.chars();
        if chars.by_ref().take(indent).any(|c| c != ' ') || chars.next() != Some(BAR) {
            return None;
        }
        Some(chars.collect())
    };
    // Up from the rule: the box (the agent's row, an empty row, its text, an empty row at its
    // top), then the list's rows, each with text one cell in from the bar.
    let list_row = |text: &str| {
        text.starts_with(' ') && !text.starts_with(BOX_INDENT) && !text.trim().is_empty()
    };
    let mut empties = 0;
    let mut list = Vec::new();
    for row in rows[..rule_row].iter().rev() {
        let Some(text) = after_bar(row) else { break };
        if empties < 2 {
            if text.trim().is_empty() {
                empties += 1;
            }
            continue;
        }
        if !list_row(&text) {
            break;
        }
        list.push(text);
    }
    let first = list.last()?;
    let name = first
        .trim_start()
        .strip_prefix('/')?
        .split_whitespace()
        .next()?;
    (!name.is_empty()).then(|| name.to_string())
}

/// Whether a row of the list OpenCode opens for a `/` or an `@` stands right above the input box:
/// the row of the bar above the block of the box's rows (each empty or indented as the box's are),
/// with text one cell in from the bar. `None` where there is no box to read.
fn list_shown(screen: &str) -> Option<bool> {
    list_row(screen).map(|row| row.is_some())
}

/// The text of the list's row right above the input box ([`list_shown`]), trimmed, or `None`
/// inside where no list is up; `None` where there is no box to read.
fn list_row(screen: &str) -> Option<Option<String>> {
    input_box_rows(screen)?;
    let rows: Vec<&str> = screen.lines().collect();
    let (rule_row, indent, _) = rows
        .iter()
        .enumerate()
        .rev()
        .find_map(|(at, row)| rule(row).map(|(indent, width)| (at, indent, width)))?;
    let after_bar = |row: &str| -> Option<String> {
        let mut chars = row.chars();
        if chars.by_ref().take(indent).any(|c| c != ' ') || chars.next() != Some(BAR) {
            return None;
        }
        Some(chars.collect())
    };
    let above = rows[..rule_row].iter().rev().find(|row| {
        after_bar(row).is_none_or(|text| !(text.trim().is_empty() || text.starts_with(BOX_INDENT)))
    });
    Some(above.and_then(|row| after_bar(row)).and_then(|text| {
        (text.starts_with(' ') && !text.starts_with(BOX_INDENT) && !text.trim().is_empty())
            .then(|| text.trim().trim_end_matches(BAR).trim().to_string())
    }))
}

/// The list's one row where nothing matches what was typed.
const NO_MATCH_ROW: &str = "No matching items";

/// How the hint of an empty box starts, up to the example it quotes.
const PLACEHOLDER_START: &str = "Ask anything\u{2026} \"";
/// The bar down the left of the input box, and the corner the rule under it starts with.
pub(super) const BAR: char = '\u{2503}';
const RULE_START: char = '\u{2579}';
const RULE: char = '\u{2580}';
/// The fewest cells of a rule that is taken for the one under the input box.
const RULE_MIN: usize = 20;
/// How far a row of the box is indented right of the bar. A row of the list drawn above the box
/// (for a `/` or an `@`) is indented less.
pub(super) const BOX_INDENT: &str = "  ";

/// The dialogs OpenCode (1.19.6) draws over the conversation. Each is a view the person opens
/// themselves (ctrl+p, or a command such as `/models`): nothing is put to them, so the session
/// does not read as waiting for the user. While one is up the keys go to it: a pasted message
/// lands in its search field, or nowhere, and the Enter picks from its list, so writes are held.
///
/// Each is recognised by its title row: the title, a run of blanks and the `esc` at the far end,
/// on one row of the screen as drawn. That row cannot be asked for as a row of its own: the
/// dialog is drawn over the middle of the screen, and the conversation's text stays on its rows
/// to the left and right of it. The help, status and export dialogs also keep a string of their
/// own whatever is typed into them, asked for beside the title: each is short enough to stay on
/// one row of the dialog at the narrowest width the title row is recognised at (40 columns). The
/// other seven keep nothing that does: the key hints at the foot of the session and model lists
/// are split into columns that wrap at 40 columns, the placeholder of a search field goes with
/// the first key typed, and the rows of a list are replaced by what the search matches.
const DIALOGS: &[DialogPattern] = &[
    DialogPattern::all_of("opencode_commands", WrittenMessage::Discarded, &[])
        .with_row_run(&["Commands", "esc"], TITLE_GAP)
        .of_kind(DialogKind::OwnView),
    DialogPattern::all_of("opencode_sessions", WrittenMessage::Discarded, &[])
        .with_row_run(&["Sessions", "esc"], TITLE_GAP)
        .of_kind(DialogKind::OwnView),
    DialogPattern::all_of("opencode_models", WrittenMessage::Discarded, &[])
        .with_row_run(&["Select model", "esc"], TITLE_GAP)
        .of_kind(DialogKind::OwnView),
    DialogPattern::all_of("opencode_agents", WrittenMessage::Discarded, &[])
        .with_row_run(&["Select agent", "esc"], TITLE_GAP)
        .of_kind(DialogKind::OwnView),
    DialogPattern::all_of("opencode_themes", WrittenMessage::Discarded, &[])
        .with_row_run(&["Themes", "esc"], TITLE_GAP)
        .of_kind(DialogKind::OwnView),
    DialogPattern::all_of("opencode_help", WrittenMessage::Discarded, &["Pressctrl+p"])
        .with_row_run(&["Help", "esc/enter"], TITLE_GAP)
        .of_kind(DialogKind::OwnView),
    DialogPattern::all_of(
        "opencode_status",
        WrittenMessage::Discarded,
        &["MCPServers"],
    )
    .with_row_run(&["Status", "esc"], TITLE_GAP)
    .of_kind(DialogKind::OwnView),
    DialogPattern::all_of("opencode_connect", WrittenMessage::Discarded, &[])
        .with_row_run(&["Connect a provider", "esc"], TITLE_GAP)
        .of_kind(DialogKind::OwnView),
    DialogPattern::all_of(
        "opencode_export",
        WrittenMessage::Discarded,
        &["Filename:", "Pressreturntoconfirm"],
    )
    .with_row_run(&["Export Options", "esc"], TITLE_GAP)
    .of_kind(DialogKind::OwnView),
    DialogPattern::all_of("opencode_timeline", WrittenMessage::Discarded, &[])
        .with_row_run(&["Timeline", "esc"], TITLE_GAP)
        .of_kind(DialogKind::OwnView),
];

/// The fewest blank cells between a dialog's title and the `esc` at the end of its row: nine
/// under `Connect a provider`, the longest title, in a terminal 40 columns wide. The run is 29
/// cells or more from 60 columns up and shrinks with the terminal below that, so a terminal
/// narrower than 40 columns has some of the dialogs unrecognised: OpenCode still draws them down
/// to 20 columns (a longer title running into its `esc` at 30, wrapped at 20), and fails as it
/// starts at 16.
const TITLE_GAP: usize = 9;

/// The rows of OpenCode's input box: the rows of the bar right above the last rule on the screen,
/// without the empty row at the top and the empty row and the agent's row at the bottom, each cut
/// where the rule ends.
pub(super) fn input_box_rows(screen: &str) -> Option<Vec<String>> {
    let rows: Vec<&str> = screen.lines().collect();
    let (rule_row, indent, width) = rows
        .iter()
        .enumerate()
        .rev()
        .find_map(|(at, row)| rule(row).map(|(indent, width)| (at, indent, width)))?;
    // The cells of a row of the box right of the bar, as far as the rule reaches: a row of the
    // bar that is empty or indented as the box's rows are, so not a row of a list drawn above it.
    let inside = |row: &str| -> Option<String> {
        let mut chars = row.chars();
        if chars.by_ref().take(indent).any(|c| c != ' ') || chars.next() != Some(BAR) {
            return None;
        }
        let mut columns = 1;
        let mut text = String::new();
        for c in chars {
            columns += cells(c);
            if columns > width {
                break;
            }
            text.push(c);
        }
        (text.trim().is_empty() || text.starts_with(BOX_INDENT)).then(|| text.trim().to_string())
    };
    let mut block: Vec<String> = rows[..rule_row]
        .iter()
        .rev()
        .map_while(|row| inside(row))
        .collect();
    block.reverse();
    // An empty row, at least one row of text, an empty row, the agent's row.
    let text_rows = block.len().checked_sub(3).filter(|&count| count >= 1)?;
    let framed =
        block[0].is_empty() && block[text_rows + 1].is_empty() && !block[text_rows + 2].is_empty();
    framed.then(|| block[1..=text_rows].to_vec())
}

/// Where the rule under the input box starts on `row` and how many cells it spans, when the row
/// is such a rule: spaces, the corner, then nothing but the rule up to the box's right edge.
fn rule(row: &str) -> Option<(usize, usize)> {
    let indent = row.chars().take_while(|&c| c == ' ').count();
    let mut chars = row.chars().skip(indent);
    if chars.next() != Some(RULE_START) {
        return None;
    }
    let length = chars.take_while(|&c| c == RULE).count();
    (length >= RULE_MIN).then_some((indent, length + 1))
}

/// How many cells of a row `c` takes: two for the wide characters of East Asian scripts and for
/// emoji, none for a mark that combines with the character before it, one otherwise. The screen's
/// text holds one character for a wide one's two cells.
fn cells(c: char) -> usize {
    match c as u32 {
        0x0300..=0x036F | 0x200B..=0x200D | 0xFE00..=0xFE0F => 0,
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1FAFF
        | 0x20000..=0x3FFFD => 2,
        _ => 1,
    }
}

const INTERRUPT: Interrupt = Interrupt {
    key: InterruptKey::EscTwice,
    restores_prompt: false,
    written_during_turn,
    queued_waiting: None,
};

fn written_during_turn(written: usize) -> String {
    format!(
        " Octoboard wrote about {written} message(s) into the session during that turn. OpenCode \
         took them into the turn it cancelled and does not run them afterwards. Check it with \
         `get_session` before sending the next step, and resend what was not acted on."
    )
}

/// No screen to look for; only how the start of a session shows.
const START: Screen = Screen {
    first_draw: &[],
    accept: None,
    others: &[],
    cursor: None,
    press: Press::Yes,
    next_screen: None,
    input_ends_wait: true,
    start_signal: StartSignal::TerminalTitle,
    hook_fallback: None,
    waits_for_paste_mode: false,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// A turn is interrupted by Esc twice, and the interrupted prompt is not put back in the input
    /// box, so nothing is cleared after the key. No server of its own queues Octoboard's messages.
    #[test]
    fn a_turn_is_interrupted_by_esc_twice_and_the_prompt_is_not_restored() {
        let interrupt = OpenCodeScreen.interrupt().expect("an interrupt key");
        assert_eq!(interrupt.key, InterruptKey::EscTwice);
        assert!(!interrupt.restores_prompt);
        assert!(interrupt.queued_waiting.is_none());
    }

    #[test]
    fn the_attached_tui_shows_the_running_turn_in_its_footer() {
        let box_rows = format!(
            "  ┃\n  ┃\n  ┃\n  ┃  Build · M36 stand-in\n  ╹{}",
            "▀".repeat(119)
        );
        for hint in ["esc interrupt", "esc again to interrupt"] {
            let screen =
                format!("{box_rows}\n   ■■■■■■⬝⬝  {hint}      tab agents  ctrl+p commands");
            assert!(interrupt_footer(&screen));
        }
        assert!(!interrupt_footer(&format!(
            "{box_rows}\n tab agents  ctrl+p commands"
        )));
    }

    /// What the caller is told of the messages written during the cancelled turn: they joined the
    /// turn, do not run now, and the caller has to check and resend what was not acted on.
    #[test]
    fn the_caller_is_told_the_messages_written_during_the_turn_do_not_run() {
        let interrupt = OpenCodeScreen.interrupt().expect("an interrupt key");
        let told = (interrupt.written_during_turn)(3);
        assert!(told.contains('3'), "{told}");
        assert!(told.contains("does not run"), "{told}");
        assert!(told.contains("resend"), "{told}");
        assert!(told.contains("get_session"), "{told}");
        assert!(
            !(interrupt.written_during_turn)(1).contains('3'),
            "the count is the one given"
        );
        assert!(
            told.starts_with(' '),
            "it follows the interrupt's own sentence: {told:?}"
        );
    }

    /// OpenCode shows no confirmation at a first run in a folder, so no screen is looked for or
    /// pressed, and its session is taken to have started once its terminal title is written.
    #[test]
    fn it_has_no_folder_confirmation_and_starts_with_its_terminal_title() {
        let screen = OpenCodeScreen.trust_screen();
        assert!(screen.first_draw.is_empty());
        assert_eq!(screen.accept, None);
        assert!(screen.others.is_empty());
        assert_eq!(screen.cursor, None);
        assert!(screen.start_signal == StartSignal::TerminalTitle);
    }

    /// Real screens of OpenCode 1.19.6 (`testdata/opencode-captures.md`), as the terminal drew
    /// them: the rows of the visible screen, each cut at its last visible cell.
    const START_120: &str =
        include_str!("../../../testdata/opencode_screen_start_120x40_1.19.6.txt");
    const START_200: &str =
        include_str!("../../../testdata/opencode_screen_start_200x40_1.19.6.txt");
    const DRAFT_ONE_ROW: &str =
        include_str!("../../../testdata/opencode_screen_draft_one_row_120x40_1.19.6.txt");
    const DRAFT_TWO_ROWS: &str =
        include_str!("../../../testdata/opencode_screen_draft_two_rows_120x40_1.19.6.txt");
    const DRAFT_WIDE: &str =
        include_str!("../../../testdata/opencode_screen_draft_wide_120x40_1.19.6.txt");
    const SESSION_EMPTY_120: &str =
        include_str!("../../../testdata/opencode_screen_session_empty_120x40_1.19.6.txt");
    const SESSION_EMPTY_200: &str =
        include_str!("../../../testdata/opencode_screen_session_empty_200x40_1.19.6.txt");
    const SESSION_DRAFT_120: &str =
        include_str!("../../../testdata/opencode_screen_session_draft_120x40_1.19.6.txt");
    const SESSION_DRAFT_TWO_ROWS_200: &str =
        include_str!("../../../testdata/opencode_screen_session_draft_two_rows_200x40_1.19.6.txt");
    const SESSION_WIDE_SIDEBAR_200: &str = include_str!(
        "../../../testdata/opencode_screen_session_wide_draft_sidebar_200x16_1.19.6.txt"
    );

    /// What the terminal showed once the first `upto` bytes of `capture` were written (all of them
    /// past its end), read as the input box is read.
    fn replayed(capture: &[u8], upto: usize) -> String {
        let mut screen = crate::terminal::snapshot::TerminalSnapshot::new(40, 120);
        screen.process(&capture[..upto.min(capture.len())]);
        screen.visible_text_undimmed()
    }

    /// The screen of the raw capture `testdata/opencode_screen_<file>_120x40_1.19.6.bin` (the
    /// bytes OpenCode wrote in one launch, `testdata/opencode-captures.md`) once `upto` of them
    /// were written, the whole capture where none is given.
    macro_rules! raw_screen {
        ($name:ident, $file:literal) => {
            raw_screen!($name, $file, usize::MAX);
        };
        ($name:ident, $file:literal, $upto:expr) => {
            static $name: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
                replayed(
                    include_bytes!(concat!(
                        "../../../testdata/opencode_screen_",
                        $file,
                        "_120x40_1.19.6.bin"
                    )),
                    $upto,
                )
            });
        };
    }

    // A permission prompt for a directory outside the working directory and a question, each as it
    // opened (`testdata/opencode-captures.md`, "Raw screens of review round 6").
    raw_screen!(PERMISSION, "prompt_external");
    raw_screen!(QUESTION, "prompt_question_alpha");

    const START_HINT_120: &str = "Ask anything\u{2026} \"What is the tech stack of this project?\"";
    const START_HINT_200: &str = "Ask anything\u{2026} \"Fix a TODO in the codebase\"";

    fn rows(screen: &str) -> Option<Vec<String>> {
        OpenCodeScreen.input_box_rows(screen)
    }

    fn texts(rows: &[&str]) -> Option<Vec<String>> {
        Some(rows.iter().map(|row| row.to_string()).collect())
    }

    /// Outside a session an empty box shows the hint as its one row of text, at either width.
    #[test]
    fn an_empty_box_outside_a_session_reads_as_its_hint() {
        assert_eq!(rows(START_120), texts(&[START_HINT_120]));
        assert_eq!(rows(START_200), texts(&[START_HINT_200]));
        for hint in [START_HINT_120, START_HINT_200] {
            assert!(OpenCodeScreen.input_placeholder(hint), "{hint}");
        }
    }

    /// In a session an empty box shows nothing: its one row of text is empty, with a sidebar
    /// drawn beside it at 200 columns or without.
    #[test]
    fn an_empty_box_in_a_session_reads_as_one_empty_row() {
        assert_eq!(rows(SESSION_EMPTY_120), texts(&[""]));
        assert_eq!(rows(SESSION_EMPTY_200), texts(&[""]));
    }

    #[test]
    fn a_draft_reads_as_its_rows_trimmed() {
        assert_eq!(rows(DRAFT_ONE_ROW), texts(&["fix the build"]));
        assert_eq!(rows(DRAFT_TWO_ROWS), texts(&["first line", "second line"]));
        assert_eq!(rows(SESSION_DRAFT_120), texts(&["next step"]));
        assert_eq!(
            rows(SESSION_DRAFT_TWO_ROWS_200),
            texts(&["next step", "and more"])
        );
        assert!(!OpenCodeScreen.input_placeholder("fix the build"));
    }

    /// The rows read the same whether the screen's rows carry their trailing blanks or not.
    #[test]
    fn trailing_blanks_do_not_change_a_reading() {
        for (screen, expected) in [
            (START_120, texts(&[START_HINT_120])),
            (DRAFT_TWO_ROWS, texts(&["first line", "second line"])),
            (SESSION_DRAFT_120, texts(&["next step"])),
        ] {
            let padded: String = screen.lines().map(|row| format!("{row:<125}\n")).collect();
            assert_eq!(rows(&padded), expected);
        }
    }

    /// At 200 columns a sidebar is drawn on the same rows as the box, so the box's rows are read
    /// no further than its rule reaches: the sidebar's words on the box's rows are no part of
    /// them, however wide the characters before them are.
    #[test]
    fn a_sidebar_beside_the_box_is_not_part_of_its_rows() {
        let sidebar_rows = SESSION_WIDE_SIDEBAR_200
            .lines()
            .filter(|row| row.contains('\u{2503}') && row.contains("LSP"))
            .count();
        assert_eq!(sidebar_rows, 2, "the capture has the sidebar on box rows");
        assert_eq!(
            rows(SESSION_WIDE_SIDEBAR_200),
            texts(&["\u{4f60}\u{597d}\u{ff0c}\u{4e16}\u{754c}\u{554a}", "ok"])
        );
    }

    /// A wide character takes two cells, so a draft of them is read whole at 120 columns too.
    #[test]
    fn wide_characters_are_read_whole() {
        assert_eq!(
            rows(DRAFT_WIDE),
            texts(&["\u{4f60}\u{597d}\u{ff0c}\u{4e16}\u{754c} ok"])
        );
    }

    /// A permission prompt and a question are drawn in place of the box, down the same bar, with
    /// no rule under them: there is no input box to read.
    #[test]
    fn a_permission_prompt_and_a_question_have_no_box() {
        for screen in [PERMISSION.as_str(), QUESTION.as_str()] {
            assert!(screen.contains('\u{2503}'), "the capture has the bar");
            assert_eq!(rows(screen), None);
        }
    }

    /// The box is read from the last rule on the screen, so a rule drawn higher up does not
    /// take its place.
    #[test]
    fn the_last_rule_on_the_screen_closes_the_box() {
        let rule = DRAFT_ONE_ROW
            .lines()
            .find(|row| row.contains('\u{2579}'))
            .expect("a rule");
        let screen = format!("{rule}\n{DRAFT_ONE_ROW}");
        assert_eq!(rows(&screen), texts(&["fix the build"]));
    }

    /// Built by hand from the drawing of a real box (the rows of a block and the rule under it),
    /// varied where the capture cannot be: a block that is not framed as OpenCode frames it, and
    /// a rule too short to be the box's, are no box.
    #[test]
    fn what_is_not_framed_as_a_box_is_none() {
        let rule = format!("  \u{2579}{}", "\u{2580}".repeat(40));
        let short_rule = format!("  \u{2579}{}", "\u{2580}".repeat(18));
        let framed = [
            "  \u{2503}",
            "  \u{2503}  text",
            "  \u{2503}",
            "  \u{2503}  Build \u{b7} M Stub",
        ];
        let screen = |rows: &[&str], rule: &str| format!("{}\n{rule}\n", rows.join("\n"));
        assert_eq!(rows(&screen(&framed, &rule)), texts(&["text"]));
        // The block's own shape: the top row empty, the row under the text empty, an agent row.
        assert_eq!(
            rows(&screen(&framed, &short_rule)),
            None,
            "a rule of 19 cells"
        );
        assert_eq!(
            rows(&screen(&framed[1..], &rule)),
            None,
            "no empty row on top"
        );
        assert_eq!(rows(&screen(&framed[..3], &rule)), None, "no agent row");
        assert_eq!(rows(&screen(&framed[..2], &rule)), None, "a bare text row");
        assert_eq!(
            rows(&screen(&[framed[0], framed[2], framed[3]], &rule)),
            None,
            "no text row"
        );
        let blank_agent = [framed[0], framed[1], framed[2], "  \u{2503}"];
        assert_eq!(
            rows(&screen(&blank_agent, &rule)),
            None,
            "an empty agent row"
        );
        let crowded = [framed[0], framed[1], "  \u{2503}  more", framed[3]];
        assert_eq!(
            rows(&screen(&crowded, &rule)),
            None,
            "no empty row under the text"
        );
        assert_eq!(rows("no box on this screen\n"), None);
        assert_eq!(rows(""), None);
    }

    /// Ctrl-U deletes back to the start of the line, and at its start the line break before it, so
    /// it does clear an input box.
    #[test]
    fn ctrl_u_clears_the_input_box() {
        assert!(OpenCodeScreen.clears_input_box_with_ctrl_u());
    }

    /// A first Enter on a lone `/name` only takes the list's completion, so a lone command gets a
    /// space typed after it before its Enter (measured on 1.19.6).
    #[test]
    fn a_lone_command_gets_a_space_before_its_enter() {
        assert!(OpenCodeScreen.writes_lone_command_with_trailing_space());
    }

    /// Only the hint of an empty box outside a session is a placeholder, and only with an example
    /// quoted after it.
    #[test]
    fn only_the_hint_with_an_example_is_a_placeholder() {
        for row in [
            "",
            "say hi",
            "Ask anything\u{2026}",
            "Ask anything\u{2026} ",
            "Build",
        ] {
            assert!(!OpenCodeScreen.input_placeholder(row), "{row:?}");
        }
    }

    const SESSION_QUEUED: &str =
        include_str!("../../../testdata/opencode_screen_session_queued_120x16_1.19.6.txt");

    /// A prompt submitted while a turn runs is shown above the box marked `QUEUED`, drawn down the
    /// same bar, with an empty row between it and the box: the box reads empty, and the queued text
    /// is not taken for a draft.
    #[test]
    fn a_queued_prompt_above_the_box_is_not_a_draft() {
        assert!(SESSION_QUEUED.contains("second message"));
        assert!(SESSION_QUEUED.contains("QUEUED"));
        assert_eq!(rows(SESSION_QUEUED), texts(&[""]));
    }

    /// One session of OpenCode 1.19.6 at 120x40, as the bytes it wrote
    /// (`testdata/opencode-captures.md`, "Lists and dialogs"), cut where each screen stood.
    const DIALOG_SESSION: &[u8] =
        include_bytes!("../../../testdata/opencode_screen_dialogs_120x40_1.19.6.bin");

    /// What the terminal showed once the first `upto` bytes of the session were written, read as
    /// the input box is read.
    fn shown_at(upto: usize) -> String {
        let mut screen = crate::terminal::snapshot::TerminalSnapshot::new(40, 120);
        screen.process(&DIALOG_SESSION[..upto]);
        screen.visible_text_undimmed()
    }

    const AT_IDLE: usize = 42367;
    const AT_SLASH_LIST: usize = 45588;
    const AT_MENTION_LIST: usize = 47792;
    const AT_COMMANDS_OVER_A_DRAFT: usize = 243273;
    const AT_CLOSED: usize = 251110;

    /// One session of OpenCode's in which each dialog was opened and closed in turn, as the bytes
    /// it wrote, with the width of its terminal and where each screen stood.
    struct DialogCapture {
        cols: u16,
        bytes: &'static [u8],
        at_idle: usize,
        at_closed: usize,
        /// Where each dialog stood open, by its code.
        at_dialogs: [(&'static str, usize); 10],
    }

    /// The sessions at 120, 80 and 40 columns, the last the narrowest at which every title row is
    /// recognised (`testdata/opencode-captures.md`, "Lists and dialogs").
    const DIALOG_CAPTURES: [DialogCapture; 3] = [
        DialogCapture {
            cols: 120,
            bytes: DIALOG_SESSION,
            at_idle: AT_IDLE,
            at_closed: AT_CLOSED,
            at_dialogs: [
                ("opencode_commands", 59425),
                ("opencode_sessions", 78260),
                ("opencode_models", 97977),
                ("opencode_agents", 115937),
                ("opencode_themes", 135735),
                ("opencode_help", 152896),
                ("opencode_status", 170002),
                ("opencode_connect", 188909),
                ("opencode_export", 207170),
                ("opencode_timeline", 224329),
            ],
        },
        DialogCapture {
            cols: 80,
            bytes: include_bytes!("../../../testdata/opencode_screen_dialogs_80x40_1.19.6.bin"),
            at_idle: 60640,
            at_closed: 211095,
            at_dialogs: [
                ("opencode_commands", 70165),
                ("opencode_sessions", 86479),
                ("opencode_models", 102768),
                ("opencode_agents", 117218),
                ("opencode_themes", 133588),
                ("opencode_help", 147321),
                ("opencode_status", 160999),
                ("opencode_connect", 176478),
                ("opencode_export", 191311),
                ("opencode_timeline", 205018),
            ],
        },
        DialogCapture {
            cols: 40,
            bytes: include_bytes!("../../../testdata/opencode_screen_dialogs_40x40_1.19.6.bin"),
            at_idle: 95810,
            at_closed: 213007,
            at_dialogs: [
                ("opencode_commands", 103615),
                ("opencode_sessions", 116937),
                ("opencode_models", 129945),
                ("opencode_agents", 140927),
                ("opencode_themes", 153845),
                ("opencode_help", 164321),
                ("opencode_status", 174615),
                ("opencode_connect", 186682),
                ("opencode_export", 198243),
                ("opencode_timeline", 208579),
            ],
        },
    ];

    fn recognised(screen: &str) -> Option<&'static str> {
        crate::session::dialogs::recognise(crate::providers::AgentId::OPENCODE, screen)
            .map(|dialog| dialog.code)
    }

    impl DialogCapture {
        /// What the terminal showed once the first `upto` bytes were written, in the two renderings
        /// production reads a screen through: as drawn (`visible_text`, which dialogs are
        /// recognised on) and with faint cells blanked (`visible_text_undimmed`, which the input
        /// box is read on).
        fn shown(&self, upto: usize) -> [String; 2] {
            let mut screen = crate::terminal::snapshot::TerminalSnapshot::new(40, self.cols);
            screen.process(&self.bytes[..upto]);
            [screen.visible_text(), screen.visible_text_undimmed()]
        }

        /// The screen as drawn once the first `upto` bytes were written.
        fn drawn(&self, upto: usize) -> String {
            let [drawn, _] = self.shown(upto);
            drawn
        }

        /// The dialog recognised once the first `upto` bytes were written, the same on both
        /// renderings.
        fn dialog_at(&self, upto: usize) -> Option<&'static str> {
            let [drawn, undimmed] = self.shown(upto);
            let seen = recognised(&drawn);
            assert_eq!(
                recognised(&undimmed),
                seen,
                "{} columns at {upto}",
                self.cols
            );
            seen
        }

        fn at(&self, code: &str) -> usize {
            self.at_dialogs
                .iter()
                .find(|(known, _)| *known == code)
                .expect("a captured dialog")
                .1
        }
    }

    /// The list of commands OpenCode opens for a `/` typed in the box, and the one for an `@`, are
    /// drawn right above the box down the same bar: the box under them is read as what was typed,
    /// not as unreadable, and neither is a dialog.
    #[test]
    fn the_box_under_a_list_of_commands_or_mentions_reads_what_was_typed() {
        let slash = shown_at(AT_SLASH_LIST);
        assert!(slash.contains("/export"), "the list is up: {slash}");
        assert_eq!(input_box_rows(&slash), Some(vec!["/".to_string()]));
        let mention = shown_at(AT_MENTION_LIST);
        assert!(mention.contains("@general"), "the list is up: {mention}");
        assert_eq!(input_box_rows(&mention), Some(vec!["draft @".to_string()]));
        let wide = &DIALOG_CAPTURES[0];
        assert_eq!(wide.dialog_at(AT_SLASH_LIST), None);
        assert_eq!(wide.dialog_at(AT_MENTION_LIST), None);
    }

    /// Each dialog OpenCode draws over the conversation is recognised while it is up, as one that
    /// loses a message written into the session, at every width captured (120, 80 and 40
    /// columns), and none is seen before the first or once the last is closed.
    #[test]
    fn each_dialog_is_recognised_while_it_is_up_and_none_when_none_is() {
        for capture in &DIALOG_CAPTURES {
            for (code, upto) in capture.at_dialogs {
                assert_eq!(
                    capture.dialog_at(upto),
                    Some(code),
                    "{} columns: {}",
                    capture.cols,
                    capture.drawn(upto)
                );
            }
            assert_eq!(capture.dialog_at(capture.at_idle), None, "{}", capture.cols);
            assert_eq!(
                capture.dialog_at(capture.at_closed),
                None,
                "{}",
                capture.cols
            );
        }
        assert_eq!(
            DIALOG_CAPTURES[0].dialog_at(AT_COMMANDS_OVER_A_DRAFT),
            Some("opencode_commands")
        );
        assert_eq!(
            OpenCodeScreen.dialogs().len(),
            DIALOG_CAPTURES[0].at_dialogs.len()
        );
    }

    /// No screen of OpenCode's captured outside a dialog reads as one: the start screen, a draft,
    /// a session with its sidebar, a queued prompt, a permission prompt and a question.
    #[test]
    fn no_other_screen_of_opencodes_is_recognised_as_a_dialog() {
        for (name, screen) in [
            ("start 120", START_120),
            ("start 200", START_200),
            ("draft one row", DRAFT_ONE_ROW),
            ("draft two rows", DRAFT_TWO_ROWS),
            ("draft wide", DRAFT_WIDE),
            ("session empty 120", SESSION_EMPTY_120),
            ("session empty 200", SESSION_EMPTY_200),
            ("session draft 120", SESSION_DRAFT_120),
            ("session draft two rows 200", SESSION_DRAFT_TWO_ROWS_200),
            ("session wide sidebar 200", SESSION_WIDE_SIDEBAR_200),
            ("session queued", SESSION_QUEUED),
            ("permission", PERMISSION.as_str()),
            ("question", QUESTION.as_str()),
        ] {
            assert_eq!(recognised(screen), None, "{name}");
        }
    }

    /// The dialogs whose screens keep a second string of their own, with a row that holds it.
    const WITH_A_SECOND_STRING: [(&str, &str); 3] = [
        ("opencode_help", "Press ctrl+p"),
        ("opencode_status", "MCP Servers"),
        ("opencode_export", "Press return to confirm"),
    ];

    /// A dialog is drawn over the middle of the screen and the conversation's text stays on its
    /// rows to either side (measured over a conversation that fills the screen: `line 381 xxxx`
    /// stood left and right of each title). Hand-built from each captured dialog at each width,
    /// since the captures' conversation is two rows long: with text on both sides of every row,
    /// each dialog is still recognised.
    #[test]
    fn a_dialog_is_recognised_with_the_conversation_on_its_rows() {
        for capture in &DIALOG_CAPTURES {
            let width = usize::from(capture.cols);
            for (code, upto) in capture.at_dialogs {
                let over_text: String = capture
                    .drawn(upto)
                    .lines()
                    .map(|row| format!("line 381 xxxx{row:<width$}xxxx\n"))
                    .collect();
                assert_eq!(recognised(&over_text), Some(code), "{over_text}");
            }
        }
    }

    /// A dialog's title and `esc` with the rest of the dialog gone is not the dialog, for each
    /// dialog whose screen keeps a second string, at each width captured: the row holding that
    /// string is taken out of the captured screen (a variation no capture can give, since
    /// OpenCode draws the two together). The string stays on one row down to 40 columns.
    #[test]
    fn a_title_without_the_rest_of_its_dialog_is_not_the_dialog() {
        for capture in &DIALOG_CAPTURES {
            for (code, second) in WITH_A_SECOND_STRING {
                let shown = capture.drawn(capture.at(code));
                assert!(shown.contains(second), "{} {code}: {shown}", capture.cols);
                let without: String = shown
                    .lines()
                    .filter(|row| !row.contains(second))
                    .map(|row| format!("{row}\n"))
                    .collect();
                assert_eq!(recognised(&without), None, "{code}: {without}");
            }
        }
    }

    /// Conversation text that spells a title with `esc` after it is no dialog, for any of the
    /// ten: a sentence, a title at the end of one row with `esc` at the start of the next, the
    /// titles quoted from this very list, and a title with fewer blanks before `esc` than a
    /// dialog draws. Near misses built by hand on the captured idle screen, since OpenCode draws
    /// no such screen.
    #[test]
    fn conversation_text_that_spells_a_title_is_not_a_dialog() {
        let idle = DIALOG_CAPTURES[0].drawn(AT_IDLE);
        for text in [
            "The status escalated: Status escalated to the on-call.",
            "Sessions escape the sandbox when the flag is set.",
            "See Help\nesc/enter closes it.",
            "Select model escalation policy",
            "Export Options escape hatch",
            "Timeline escape hatch",
            "Commands esc, Themes esc and Select agent esc close with one key.",
            "Connect a provider\nesc",
            "Commandsesc Sessionsesc Selectmodelesc Selectagentesc Themesesc Helpesc/enter",
            "Statusesc Connectaprovideresc ExportOptionsesc Timelineesc",
            "| Commands | esc      |",
        ] {
            let screen = format!("{idle}\n     {text}\n");
            assert_eq!(recognised(&screen), None, "{text}");
        }
    }

    /// The run of blanks between a title and its `esc` shrinks with the terminal: at 40 columns
    /// nine cells stand under `Connect a provider`, the longest title. That captured screen with
    /// one blank fewer on the row is no dialog (a variation of the capture: a terminal one column
    /// narrower draws it so, among others whose rows are not captured).
    #[test]
    fn a_title_row_is_a_dialogs_from_the_narrowest_gap_captured() {
        let narrow = &DIALOG_CAPTURES[2];
        assert_eq!(narrow.cols, 40);
        let drawn = narrow.drawn(narrow.at("opencode_connect"));
        let row = |gap: usize| format!("Connect a provider{}esc", " ".repeat(gap));
        assert!(drawn.contains(&row(TITLE_GAP)), "{drawn}");
        assert!(!drawn.contains(&row(TITLE_GAP + 1)), "{drawn}");
        assert_eq!(recognised(&drawn), Some("opencode_connect"));
        let closer = drawn.replace(&row(TITLE_GAP), &format!(" {}", row(TITLE_GAP - 1)));
        assert_eq!(recognised(&closer), None, "{closer}");
    }

    /// The same for the titles of the other dialogs that keep no second string, on hand-built
    /// rows: nine blanks before `esc` are the dialog, eight are not.
    #[test]
    fn a_title_row_with_fewer_blanks_than_the_narrowest_gap_is_no_dialog() {
        let row = |title: &str, gap: usize| format!("  {title}{}esc  \n", " ".repeat(gap));
        for (code, title) in [
            ("opencode_commands", "Commands"),
            ("opencode_sessions", "Sessions"),
            ("opencode_models", "Select model"),
            ("opencode_agents", "Select agent"),
            ("opencode_themes", "Themes"),
            ("opencode_connect", "Connect a provider"),
            ("opencode_timeline", "Timeline"),
        ] {
            assert_eq!(recognised(&row(title, TITLE_GAP)), Some(code), "{title}");
            assert_eq!(recognised(&row(title, TITLE_GAP - 1)), None, "{title}");
            assert_eq!(recognised(&row(title, 1)), None, "{title}");
        }
    }

    /// Every dialog is a view the person opened themselves: it holds the session's writes, loses
    /// a message written into it, and is not a question put to the person, so nobody is told the
    /// session waits.
    #[test]
    fn every_dialog_is_a_view_the_person_opened() {
        for capture in &DIALOG_CAPTURES {
            for (code, upto) in capture.at_dialogs {
                let dialog = crate::session::dialogs::recognise(
                    crate::providers::AgentId::OPENCODE,
                    &capture.drawn(upto),
                )
                .unwrap_or_else(|| panic!("{code} is recognised at {}", capture.cols));
                assert_eq!(dialog.code, code);
                assert_eq!(dialog.kind, DialogKind::OwnView, "{code}");
                assert!(dialog.is_own_view(), "{code}");
                assert_eq!(dialog.written, WrittenMessage::Discarded, "{code}");
            }
        }
    }

    /// The box under a dialog is drawn whole, so it reads as it is: empty before, the draft kept
    /// under the command list and after it closes.
    #[test]
    fn the_box_under_a_dialog_reads_as_it_is_drawn() {
        assert_eq!(
            input_box_rows(&shown_at(AT_IDLE)),
            Some(vec![String::new()])
        );
        for at in [AT_COMMANDS_OVER_A_DRAFT, AT_CLOSED] {
            assert_eq!(
                input_box_rows(&shown_at(at)),
                Some(vec!["keep me".to_string()])
            );
        }
    }

    /// Hand-built variations of the captured list rows, since OpenCode draws no such screen: a row
    /// of the bar indented one cell ends the box above it, wherever it stands.
    #[test]
    fn a_row_indented_less_than_the_box_is_not_a_row_of_it() {
        let screen = "  \u{2503} /agents       Switch agent\n  \u{2503}\n  \u{2503}  /\n  \u{2503}\n  \u{2503}  Build \u{b7} M Stub\n  \u{2579}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}";
        assert_eq!(input_box_rows(screen), Some(vec!["/".to_string()]));
    }

    // Two image paths pasted on their own at the start screen, followed by text
    // (`testdata/opencode-captures.md`, "Images").
    raw_screen!(IMAGES, "images");

    /// The list's first command is the completion an Enter takes; the box under it still reads
    /// what was pasted.
    #[test]
    fn the_first_command_of_the_list_above_the_box_is_the_completion() {
        let screen = OpenCodeScreen;
        assert_eq!(
            screen.command_completion(LIST_SESSION.as_str()),
            Some("probecmd".to_string())
        );
        assert_eq!(
            screen.input_box_rows(LIST_SESSION.as_str()),
            Some(vec!["/probecmd".to_string()])
        );
    }

    /// No list is up on a screen with a box alone, a draft that begins with `/` included, nor on a
    /// prompt drawn in place of the box.
    #[test]
    fn no_completion_is_read_where_no_list_is_up() {
        let screen = OpenCodeScreen;
        for drawn in [
            START_120,
            SESSION_EMPTY_120,
            SESSION_DRAFT_120,
            SESSION_DRAFT_TWO_ROWS_200,
            PERMISSION.as_str(),
            QUESTION.as_str(),
            IMAGES.as_str(),
        ] {
            assert_eq!(screen.command_completion(drawn), None);
        }
        let typed = LIST_SESSION
            .as_str()
            .replace("  \u{2503} /probecmd ", "  \u{2503}  /probecmd");
        assert_eq!(screen.command_completion(&typed), None);
    }

    /// One session of OpenCode's in which `/probecmd`, `/zzz` and `/` were written at the start
    /// screen and in a session, each followed by a space, and `/probecmd` run in the session by a
    /// space and one Enter (`testdata/opencode-captures.md`, "The command list and the space"),
    /// with where each screen stood in it.
    const COMMANDS: &[u8] =
        include_bytes!("../../../testdata/opencode_screen_commands_120x40_1.19.6.bin");
    const AT_COMMAND_IDLE: usize = 39573;
    const AT_LIST_SESSION: usize = 39941;
    const AT_LIST_CLOSED: usize = 40164;
    const AT_COMMAND_RUN: usize = 41631;
    raw_screen!(LIST_START, "commands", 22657);
    raw_screen!(LIST_CLOSED_START, "commands", 22840);
    raw_screen!(NO_MATCH_START, "commands", 23330);
    raw_screen!(SLASH_START, "commands", 26582);
    raw_screen!(COMMAND_IDLE, "commands", AT_COMMAND_IDLE);
    raw_screen!(LIST_SESSION, "commands", AT_LIST_SESSION);
    raw_screen!(LIST_CLOSED, "commands", AT_LIST_CLOSED);
    raw_screen!(COMMAND_RUN, "commands", AT_COMMAND_RUN);
    raw_screen!(NO_MATCH, "commands", 41992);
    raw_screen!(NO_MATCH_CLOSED, "commands", 42215);

    /// The list above the box is up for a lone `/name` that matches, for `/` alone (many rows) and
    /// for the `@` list, at the start screen and in a session, the box under it reading what was
    /// typed.
    #[test]
    fn the_list_above_the_box_reads_as_up() {
        let screen = OpenCodeScreen;
        for (name, drawn, typed) in [
            ("start", LIST_START.as_str(), "/probecmd"),
            ("session", LIST_SESSION.as_str(), "/probecmd"),
            ("slash alone", SLASH_START.as_str(), "/"),
        ] {
            assert_eq!(screen.list_shown(drawn), Some(true), "{name}: {drawn}");
            assert_eq!(
                screen.input_box_rows(drawn),
                Some(vec![typed.to_string()]),
                "{name}"
            );
        }
        assert_eq!(screen.list_shown(&shown_at(AT_SLASH_LIST)), Some(true));
        assert_eq!(screen.list_shown(&shown_at(AT_MENTION_LIST)), Some(true));
    }

    /// Where nothing matches, the list's one row reads `No matching items`: the list is up, and
    /// offers no completion.
    #[test]
    fn no_matching_items_is_a_list_up_with_no_completion() {
        let screen = OpenCodeScreen;
        for drawn in [NO_MATCH_START.as_str(), NO_MATCH.as_str()] {
            assert!(drawn.contains("No matching items"), "{drawn}");
            assert_eq!(screen.list_shown(drawn), Some(true), "{drawn}");
            assert_eq!(screen.command_completion(drawn), None);
            assert!(screen.list_offers_no_command(drawn), "{drawn}");
            assert_eq!(screen.input_box_rows(drawn), Some(vec!["/zzz".to_string()]));
        }
    }

    /// Only `No matching items` offers no command: a list that offers one, the list of every
    /// command, the `@` list, a list closed by the space, a box with no list and a prompt with no
    /// box all do not say so.
    #[test]
    fn only_no_matching_items_offers_no_command() {
        let screen = OpenCodeScreen;
        for drawn in [
            LIST_START.as_str(),
            LIST_SESSION.as_str(),
            SLASH_START.as_str(),
            NO_MATCH_CLOSED.as_str(),
            LIST_CLOSED.as_str(),
            COMMAND_IDLE.as_str(),
            PERMISSION.as_str(),
            QUESTION.as_str(),
        ] {
            assert!(!screen.list_offers_no_command(drawn), "{drawn}");
        }
        assert!(!screen.list_offers_no_command(&shown_at(AT_MENTION_LIST)));
    }

    /// The space typed after a lone `/name` closes the list, whether it matched or read
    /// `No matching items`, with the command still in the box.
    #[test]
    fn the_list_reads_closed_after_the_space() {
        let screen = OpenCodeScreen;
        for (drawn, typed) in [
            (LIST_CLOSED_START.as_str(), "/probecmd"),
            (LIST_CLOSED.as_str(), "/probecmd"),
            (NO_MATCH_CLOSED.as_str(), "/zzz"),
        ] {
            assert_eq!(screen.list_shown(drawn), Some(false), "{drawn}");
            assert_eq!(screen.input_box_rows(drawn), Some(vec![typed.to_string()]));
        }
    }

    /// A box with no list above it reads as no list: empty, with a draft, with a queued prompt or
    /// a sidebar beside it, under a dialog, and once a command has run; where no box is drawn (a
    /// permission prompt, a question, a screen with no rule) the reader cannot tell.
    #[test]
    fn no_list_reads_closed_and_no_box_reads_unknown() {
        let screen = OpenCodeScreen;
        for (name, drawn) in [
            ("start", START_120),
            ("start 200", START_200),
            ("draft", DRAFT_ONE_ROW),
            ("draft two rows", DRAFT_TWO_ROWS),
            ("session empty", SESSION_EMPTY_120),
            ("session empty 200", SESSION_EMPTY_200),
            ("session draft", SESSION_DRAFT_120),
            ("session sidebar", SESSION_WIDE_SIDEBAR_200),
            ("queued", SESSION_QUEUED),
            ("images", IMAGES.as_str()),
            ("idle", COMMAND_IDLE.as_str()),
            ("command run", COMMAND_RUN.as_str()),
        ] {
            assert_eq!(screen.list_shown(drawn), Some(false), "{name}: {drawn}");
        }
        assert_eq!(screen.list_shown(&shown_at(AT_IDLE)), Some(false));
        assert_eq!(
            screen.list_shown(&shown_at(AT_COMMANDS_OVER_A_DRAFT)),
            Some(false),
            "a dialog is no list by the box"
        );
        for drawn in [
            PERMISSION.as_str(),
            QUESTION.as_str(),
            "no box on this screen\n",
            "",
        ] {
            assert_eq!(screen.list_shown(drawn), None, "{drawn}");
        }
    }

    /// Where the session's capture was when `/zzz` had been pasted after the command ran, with
    /// `No matching items` up, and when the space and the Ctrl-U after it had emptied the box.
    #[cfg(unix)]
    const AT_NO_MATCH: usize = 41992;
    #[cfg(unix)]
    const AT_NO_MATCH_CLEARED: usize = 42291;

    /// A live session of OpenCode's whose stand-in replays the captured `screens` (each a name and
    /// the bytes it writes) as [`CAPTURED_AGENT`] says, drawn up to its box at rest, with the file
    /// its stand-in logs to.
    #[cfg(unix)]
    fn replaying(
        label: &str,
        screens: &[(&str, &[u8])],
    ) -> (
        crate::test_support::StandIn,
        std::path::PathBuf,
        crate::test_support::ScratchDir,
    ) {
        let dir = crate::test_support::ScratchDir::new(label);
        for (name, bytes) in screens {
            std::fs::write(dir.join(name), bytes).unwrap();
        }
        let log = dir.join("log");
        std::fs::write(dir.join("agent.pl"), CAPTURED_AGENT).unwrap();
        let script = format!(
            "DIR='{}' LOG='{}' exec /usr/bin/perl '{}'",
            dir.display(),
            log.display(),
            dir.join("agent.pl").display()
        );
        let live = crate::test_support::fake_live_session(
            label,
            crate::providers::AgentId::OPENCODE,
            120,
            40,
            &script,
        )
        .with_composer();
        crate::session::spawn_reader_thread(live.clone(), 8 * 1024);
        let started = std::time::Instant::now();
        while live.input_box(&live.box_screen()) != Some(String::new()) {
            assert!(
                started.elapsed() < crate::test_support::PATIENCE,
                "the captured idle screen was not drawn: {}",
                live.screen_text()
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        (live, log, dir)
    }

    #[cfg(unix)]
    fn logged(log: &std::path::Path) -> Vec<String> {
        std::fs::read_to_string(log)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// A lone command written into a stand-in that replays the bytes OpenCode wrote, read by
    /// OpenCode's reader as a session of OpenCode's: `/probecmd` pasted brings its list up, the
    /// space closes it, and only then is the Enter pressed, which runs `/probecmd ` (the generic
    /// `a_lone_commands_enter_waits_for_its_list_to_be_read_closed`, on OpenCode's real flow).
    #[cfg(unix)]
    #[test]
    fn a_lone_command_runs_once_its_list_is_read_closed_after_the_space() {
        let (live, log, _dir) = replaying(
            "opencode-lone-command",
            &[
                ("idle", &COMMANDS[..AT_COMMAND_IDLE]),
                ("up", &COMMANDS[AT_COMMAND_IDLE..AT_LIST_SESSION]),
                ("closed", &COMMANDS[AT_LIST_SESSION..AT_LIST_CLOSED]),
                ("run", &COMMANDS[AT_LIST_CLOSED..AT_COMMAND_RUN]),
            ],
        );
        live.write_command("m", "/probecmd", false).unwrap();
        assert_eq!(
            logged(&log),
            ["PASTE /probecmd", "SPACE closed", "SENT /probecmd "],
            "the Enter came once the list was drawn closed"
        );
    }

    /// A lone command OpenCode has none of, written into the stand-in replaying the capture of
    /// `/zzz`: its list reads `No matching items`, so the command is refused as not found with
    /// no command offered and the box cleared, and neither its space nor its Enter is
    /// written.
    #[cfg(unix)]
    #[test]
    fn a_lone_command_with_no_matching_items_is_refused_and_cleared() {
        let (live, log, _dir) = replaying(
            "opencode-unknown-command",
            &[
                ("idle", &COMMANDS[..AT_COMMAND_RUN]),
                ("up", &COMMANDS[AT_COMMAND_RUN..AT_NO_MATCH]),
                ("cleared", &COMMANDS[AT_NO_MATCH..AT_NO_MATCH_CLEARED]),
            ],
        );
        let failure = live.write_command("m", "/zzz", false).unwrap_err();
        let refused = failure
            .error
            .get_ref()
            .and_then(|error| error.downcast_ref::<crate::session::term::CommandNotFound>())
            .expect("a command not found");
        assert_eq!(refused.command, "/zzz");
        assert!(refused.cleared, "{}", refused.reason);
        assert_eq!(logged(&log), ["PASTE /zzz", "CLEARED"]);
    }

    /// The stand-in of [`replaying`]: writes the bytes OpenCode wrote up to its session at rest
    /// (`idle`), then those it wrote after them once a paste lands (`up`, the list above the box),
    /// at a space typed after it (`closed`), at an Enter (`run`), and at a Ctrl-U (`cleared`),
    /// logging each to `LOG`. What the terminal answers to OpenCode's queries
    /// comes back as input, and is passed over.
    #[cfg(unix)]
    const CAPTURED_AGENT: &str = r#"
use strict; use warnings;
my ($dir, $log) = @ENV{qw(DIR LOG)};
system('stty raw -echo');
$| = 1;
binmode STDOUT;
sub note { open(my $f, '>>', $log) or die; print $f "$_[0]\n"; close $f; }
sub show {
  open(my $f, '<:raw', "$dir/$_[0]") or die; local $/; my $s = <$f>; close $f;
  print $s;
}
show('idle');
my ($acc, $in, $box) = ('', 0, '');
while (sysread(STDIN, my $c, 1)) {
  $acc .= $c;
  if (!$in) {
    if ($acc =~ /\e\[200~$/) { $in = 1; $acc = ''; }
    elsif ($acc =~ /^\e(\[[\x30-\x3f]*[\x20-\x2f]*[\x40-\x7e]|\][^\a\e]*(\a|\e\\)|[P_^][^\e]*\e\\)$/) { $acc = ''; }
    elsif ($acc =~ /^\e/) { }
    elsif ($acc eq "\r") { note("SENT $box"); $box = ''; $acc = ''; show('run'); }
    elsif ($acc eq ' ') { note('SPACE closed'); $box .= ' '; $acc = ''; show('closed'); }
    elsif ($acc eq "\x15") { note('CLEARED'); $box = ''; $acc = ''; show('cleared'); }
    elsif ($acc =~ /^[!-~]$/) { $box .= $acc; $acc = ''; }
    else { $acc = ''; }
  } elsif ($acc =~ /\e\[201~$/) {
    my $text = substr($acc, 0, length($acc) - 6); $in = 0; $acc = '';
    note("PASTE $text"); $box .= $text; show('up');
  }
}
"#;

    /// Each image pasted as a path is one `[Image <n>]` in the box, and the text beside them is
    /// read without them.
    #[test]
    fn images_are_counted_by_their_placeholders() {
        let screen = OpenCodeScreen;
        let text = screen
            .input_box_rows(IMAGES.as_str())
            .expect("box")
            .join("\n");
        assert_eq!(text, "[Image 1] [Image 2] look at these");
        assert_eq!(screen.image_placeholders(&text), 2);
        assert_eq!(screen.without_image_placeholders(&text), "look at these");
        assert_eq!(screen.image_placeholders("[Image] [Image x] Image 1"), 0);
        assert_eq!(screen.without_image_placeholders("[Image 12]"), "");
    }

    /// The prompt OpenCode reports with images carries their placeholders, which the text written
    /// has not: `[Image 1] ` alone for an image with no text (1.19.6).
    #[test]
    fn a_reported_prompt_with_images_is_the_text_written() {
        let screen = OpenCodeScreen;
        assert!(screen.same_as_submitted("look at these", "[Image 1] [Image 2] look at these"));
        assert!(screen.same_as_submitted("", "[Image 1] "));
        assert!(screen.same_as_submitted("hello", "hello"));
        assert!(!screen.same_as_submitted("hello", "[Image 1] other"));
    }
}
