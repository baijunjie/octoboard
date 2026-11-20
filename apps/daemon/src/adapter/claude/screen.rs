//! How Claude Code's screens read: its dialogs, its input box between two horizontal rules and its
//! draft stash, its folder-trust confirmation, the glyphs its terminal title starts with, and its
//! interrupt key. The measurements are in `docs/agent-cli-reference.md`, section "Claude Code".

use crate::adapter::{LaunchPlan, LaunchSpec};
use crate::session::dialogs::{DialogKind, DialogPattern, WrittenMessage};
use crate::session::input_box::box_rows;
use crate::session::screen::{Interrupt, InterruptKey, ScreenReader, TitleCancel};
use crate::session::title_activity::Shown;
use crate::session::trust::{Press, Screen, StartSignal};

pub(super) struct ClaudeScreen;

impl ScreenReader for ClaudeScreen {
    fn dialogs(&self) -> &'static [DialogPattern] {
        DIALOGS
    }

    fn input_box_rows(&self, screen: &str) -> Option<Vec<String>> {
        input_box_rows(screen)
    }

    fn input_placeholder(&self, row: &str) -> bool {
        is_placeholder(row)
    }

    fn as_submitted<'a>(&self, typed: &'a str) -> &'a str {
        crate::text::without_paste_wrapper(typed)
    }

    fn pasted_image_path_loses_enter(&self) -> bool {
        true
    }

    fn native_stash(&self, spec: &LaunchSpec<'_>, plan: &LaunchPlan) -> bool {
        super::stash::supported(spec, plan)
    }

    /// Claude Code shows a stash at the right end of the row just above its input box's top rule:
    /// `› stashed` (2.1.274 to 2.1.314), `draft stashed` (2.1.329), after the effort indicator
    /// while it is still shown. How far that row is from the bottom of the screen varies with the
    /// rows Claude Code draws under the box, so the row is found from the box. The word
    /// `stashed` is looked for anywhere in that row rather than one exact indicator, so a
    /// rewording that keeps the word is still read; that row holds only Claude Code's own
    /// indicators, never the user's text.
    fn stash_shown(&self, screen: &str) -> bool {
        indicator_row(screen).is_some_and(|row| row.contains(STASH_WORD))
    }

    fn stash_indicator_row(&self, screen: &str) -> Option<String> {
        indicator_row(screen).map(|row| row.trim().to_string())
    }

    /// `[Pasted text #<n>]` or `[Pasted text #<n> +<lines> lines]`, as Claude Code draws a long
    /// paste in its box (`docs/agent-cli-reference.md` records a message left there as
    /// `[Pasted text #1 +4 lines]`).
    fn collapsed_paste(&self, text: &str) -> bool {
        let digits =
            |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
        let Some(inner) = text
            .strip_prefix("[Pasted text #")
            .and_then(|rest| rest.strip_suffix(']'))
        else {
            return false;
        };
        match inner.split_once(" +") {
            None => digits(inner),
            Some((number, lines)) => {
                digits(number) && lines.strip_suffix(" lines").is_some_and(digits)
            }
        }
    }

    /// Measured on 2.1.310 in `docs/agent-cli-reference.md`, "Keys that interrupt a turn" under
    /// "Claude Code": the interrupt clears a prompt Claude Code put back with it.
    fn clears_input_box_with_ctrl_u(&self) -> bool {
        true
    }

    fn trust_screen(&self) -> &'static Screen {
        &TRUST
    }

    fn title_activity(&self, glyph: Option<char>) -> Option<Shown> {
        match glyph {
            Some(glyph) if RUNNING.contains(&glyph) => Some(Shown::Running),
            Some(AT_PROMPT) => Some(Shown::AtPrompt),
            _ => None,
        }
    }

    fn title_cancel(&self) -> Option<TitleCancel> {
        Some(TitleCancel::BackAtPrompt)
    }

    // The prompt hook alone also fires for a message queued mid-turn. A stopped title
    // proves the cancel landed before that prompt (measured on 2.1.322).
    fn prompt_after_title_stop_ends_turn(&self) -> bool {
        true
    }

    fn interrupt(&self) -> Option<&'static Interrupt> {
        Some(&INTERRUPT)
    }
}

/// Esc stops a turn. Ctrl-C is never sent: a single one arms "press again to exit". Claude Code
/// may take a message queued mid-turn into the turn it is running, and otherwise submits it once
/// the turn is over.
const INTERRUPT: Interrupt = Interrupt {
    key: InterruptKey::Esc,
    restores_prompt: true,
    written_during_turn,
    queued_waiting: None,
};

fn written_during_turn(written: usize) -> String {
    format!(
        " Octoboard wrote about {written} message(s) into the session during that turn. Claude \
         Code may already have taken some into the turn it cancelled; the rest stay queued in it \
         and run next, as a new turn. Check it with `get_session` before sending the next step, \
         and resend what was not acted on."
    )
}

/// The prompt row's mark in Claude Code's input box.
const PROMPT_MARK: char = '\u{276F}';

/// The glyphs Claude Code starts its title with while a turn runs, turning about once a second.
const RUNNING: std::ops::RangeInclusive<char> = '\u{25D0}'..='\u{25D3}';

/// The glyph Claude Code starts its title with when it is back at its prompt.
const AT_PROMPT: char = '\u{2733}';

/// The word every stash indicator of Claude Code's has held, in the row above its input box.
const STASH_WORD: &str = "stashed";

/// The row just above the input box's top rule, where Claude Code shows its indicators (effort,
/// stash) and never the user's text.
fn indicator_row(screen: &str) -> Option<&str> {
    let rows: Vec<&str> = screen.lines().map(str::trim_end).collect();
    let above = box_start(&rows)?.checked_sub(2)?;
    Some(rows[above])
}

/// The row of `rows` that the input box's first line is on: the last one starting with the prompt
/// mark under the box's top rule.
fn box_start(rows: &[&str]) -> Option<usize> {
    (1..rows.len())
        .rev()
        .find(|&row| rows[row].starts_with(PROMPT_MARK) && is_top_rule(rows[row - 1]))
}

/// The rows of Claude Code's input box: from the last one starting with `❯` right below a
/// horizontal rule, plain or labelled with a place in the prompt history ([`is_top_rule`]), up to
/// the next rule.
fn input_box_rows(screen: &str) -> Option<Vec<String>> {
    let rows: Vec<&str> = screen.lines().map(str::trim_end).collect();
    let start = box_start(&rows)?;
    let end = start + 1 + rows[start + 1..].iter().position(|row| is_rule(row))?;
    // A labelled rule spans the box as the plain rule below it does.
    let width = |row: &str| row.trim().chars().count();
    if !is_rule(rows[start - 1]) && width(rows[start - 1]) != width(rows[end]) {
        return None;
    }
    Some(box_rows(rows[start..end].iter().copied(), PROMPT_MARK))
}

/// Whether `row` is a hint Claude Code shows in its empty input box, both written faint (see
/// `testdata/claude-input-box.md`): a fresh session's suggestion, `Try "…"` with an example prompt
/// that varies (`Try "write a test for <filepath>"`, 2.1.310), and `Press up to edit queued
/// messages` while a message waits for the turn to end (2.1.316).
fn is_placeholder(row: &str) -> bool {
    row == "Press up to edit queued messages"
        || row
            .strip_prefix("Try \"")
            .is_some_and(|rest| rest.len() > 1 && rest.ends_with('"'))
}

/// Whether a row is the rule above Claude Code's input box: a plain rule, or, while the box holds
/// a prompt brought back from its history with Up, one that says where in the history it is,
/// `─── History 100/100 ───…` (2.1.322; see `testdata/claude-input-box.md`). Claude Code writes
/// that label faint and the spaces around it plain, so on a screen with faint cells blanked the
/// label is a gap of spaces as wide as it was.
fn is_top_rule(row: &str) -> bool {
    if is_rule(row) {
        return true;
    }
    let row = row.trim();
    if row.chars().count() < 20 {
        return false;
    }
    let rule = |part: &str, least: usize| {
        part.chars().count() >= least && part.chars().all(|c| c == '\u{2500}')
    };
    let Some(open) = row.find(' ') else {
        return false;
    };
    let Some(close) = row.rfind(' ') else {
        return false;
    };
    let (lead, label, tail) = (&row[..open], &row[open..=close], &row[close + 1..]);
    let label_text = label.trim();
    let place = |text: &str| {
        text.split_once('/').is_some_and(|(at, of)| {
            [at, of]
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
        })
    };
    let is_label = match label_text.strip_prefix("History") {
        Some("") => true,
        Some(rest) => rest.strip_prefix(' ').is_some_and(place),
        // Blanked: as wide as ` History ` up to ` History 99999/99999 `.
        None => label_text.is_empty() && (9..=21).contains(&label.chars().count()),
    };
    // Claude Code draws the label two cells in from the rule's start, after the corner's place.
    lead == "\u{2500}\u{2500}\u{2500}" && rule(tail, 3) && is_label
}

/// Whether a row is one of the horizontal rules around Claude Code's input box.
fn is_rule(row: &str) -> bool {
    let row = row.trim();
    row.chars().count() >= 20 && row.chars().all(|c| c == '\u{2500}')
}

/// Claude Code 2.1.289: the cursor on "No, exit", then "Yes, I trust this folder".
pub(crate) const TRUST: Screen = Screen {
    first_draw: &[&["\u{276F}No,exitYes,Itrustthisfolder"]],
    accept: Some("Yes,Itrustthisfolder"),
    decline: &["No,exit"],
    cursor: Some('\u{276F}'),
    press: Press::DownThenEnter,
    next_screen: None,
    // Claude Code's first hook follows an answer in the terminal at once, and its press looks at
    // the cursor again right before each key; a key the person pressed without answering, such as
    // a Down, is caught there.
    input_ends_wait: false,
    start_signal: StartSignal::Hook,
    hook_fallback: None,
    waits_for_paste_mode: true,
};

/// The names of the dialogs for Claude Code's views of its background agents: a subagent's view,
/// the agents view, and the task list holding focus.
const CLAUDE_SUBAGENT_VIEW: &str = "claude_subagent_view";
const CLAUDE_AGENTS_VIEW: &str = "claude_agents_view";
const CLAUDE_AGENT_LIST_FOCUS: &str = "claude_agent_list_focus";

/// The names of the dialogs of Claude Code's computer-use MCP server: its access request, and the
/// same request when macOS has not granted it what it needs.
const CLAUDE_COMPUTER_USE_ACCESS: &str = "claude_computer_use_access";
const CLAUDE_COMPUTER_USE_PERMISSIONS: &str = "claude_computer_use_permissions";

/// Claude Code prints a usage limit in its conversation; its `/rate-limit-options` menu is not
/// recognised as a dialog.
const DIALOGS: &[DialogPattern] = &[
    // Shown at a launch that resumes a conversation with no prompt on the command line; a paste is
    // discarded and the Enter picks the focused row.
    DialogPattern::all_of(
        "claude_resume_at_launch",
        WrittenMessage::Discarded,
        &["Resumethisconversation?", "Entertoconfirm", "Esctoresume"],
    ),
    // Shown on its own in an idle session once the prompt cache has expired; a paste goes into the
    // input box and the Enter is swallowed, so the message waits there as a draft.
    DialogPattern::all_of(
        "claude_resume_idle",
        WrittenMessage::Draft,
        &["Resumethisconversation?", "Entertoconfirm", "Esctodismiss"],
    ),
    DialogPattern::all_of(
        "claude_resume_from_summary",
        WrittenMessage::Discarded,
        &["Resumefromsummary", "Resumefullsessionas-is", "Esctocancel"],
    ),
    // A notice that waits for Enter, such as the one 2.1.308 raises mid-turn about auto mode's
    // classifier billing behind a gateway, or the rewind dialog: a paste is dropped and the Enter
    // continues past it.
    DialogPattern::all_of(
        "claude_notice",
        WrittenMessage::Discarded,
        &["Entertocontinue\u{B7}Esctocancel"],
    ),
    // Offered once an account's settings name a default mode other than auto, at a launch or once a
    // turn ends: a paste is dropped and the Enter picks the focused Yes, which writes auto into the
    // account's settings. It has no footer, so its Yes option, which does not vary with the mode,
    // stands in for one.
    DialogPattern::all_of(
        "claude_auto_mode_default",
        WrittenMessage::Discarded,
        &[
            "Makeautomodeyourdefaultpermissionmode?",
            "Yes,setautomodeasmydefaultpermissionmode",
        ],
    ),
    // The access request of Claude Code's computer-use MCP server (`request_access`), raised from
    // inside a running tool call, so no `PermissionRequest` hook announces it. A paste is dropped
    // and the Enter picks the focused row: Deny by default, which refuses Claude the apps, or Allow
    // if the user moved the focus. The footer must be a row of its own, so a transcript quoting the
    // title and the footer on one line does not match. Its title, read from the executables: up to
    // 2.1.327 "Computer Use wants to control these apps"; from 2.1.328 "Claude wants to use"
    // followed by the one app's name or by "these apps", or "Claude wants these permissions" when
    // it asks for no app. Footer from users' screenshots of both. A title this misses only loses
    // the raised hand: the title's cancel watch does not read a wait inside a tool call as a cancel.
    DialogPattern::all_of(
        CLAUDE_COMPUTER_USE_ACCESS,
        WrittenMessage::Discarded,
        &["ComputerUsewantstocontroltheseapps"],
    )
    .with_row("Entertoconfirm\u{B7}Esctocancel"),
    DialogPattern::all_of(
        CLAUDE_COMPUTER_USE_ACCESS,
        WrittenMessage::Discarded,
        &["Claudewantstouse"],
    )
    .with_row("Entertoconfirm\u{B7}Esctocancel"),
    DialogPattern::all_of(
        CLAUDE_COMPUTER_USE_ACCESS,
        WrittenMessage::Discarded,
        &["Claudewantsthesepermissions"],
    )
    .with_row("Entertoconfirm\u{B7}Esctocancel"),
    // The same request when Accessibility or Screen Recording is not granted (read from the
    // executables, not reproduced): its rows open System Settings or try again, and it is drawn in
    // the same frame, so its footer is taken to be the access request's. Titled "Computer Use
    // needs macOS permissions" on 2.1.325, "Computer use needs macOS permissions" on 2.1.329.
    DialogPattern::all_of(
        CLAUDE_COMPUTER_USE_PERMISSIONS,
        WrittenMessage::Discarded,
        &["ComputerUseneedsmacOSpermissions"],
    )
    .with_row("Entertoconfirm\u{B7}Esctocancel"),
    DialogPattern::all_of(
        CLAUDE_COMPUTER_USE_PERMISSIONS,
        WrittenMessage::Discarded,
        &["ComputeruseneedsmacOSpermissions"],
    )
    .with_row("Entertoconfirm\u{B7}Esctocancel"),
    // Focus moved from the prompt to the task list under the prompt box: a paste is dropped, and
    // the Enter opens a subagent's view (or, on the main row inside one, returns to the
    // conversation). The hints are rows of their own, so a transcript quoting them in a sentence
    // does not match. Inside a subagent's view the hint is what tells the list's focus from the
    // view itself, so these come before the view's entries.
    DialogPattern::on_row(
        CLAUDE_AGENT_LIST_FOCUS,
        WrittenMessage::Discarded,
        "Entertoview\u{B7}xtostop",
    )
    .of_kind(DialogKind::AgentsView),
    DialogPattern::on_row(
        CLAUDE_AGENT_LIST_FOCUS,
        WrittenMessage::Discarded,
        "\u{2191}/\u{2193}toselect",
    )
    .of_kind(DialogKind::AgentsView),
    // A background subagent's transcript in place of the conversation. Its prompt box stays on
    // screen, but what is sent there goes to the subagent, with no `UserPromptSubmit`. The main
    // row of the task list is filled while the conversation is shown and hollow while a subagent
    // is; 2.1.309 marks it with `❯` when it is selected without leaving the view, and shows no
    // hint then. Leaving is never safe to do for the user: Esc stops the agent, Ctrl+C kills it,
    // and ← opens the agents view below.
    DialogPattern::on_row(
        CLAUDE_SUBAGENT_VIEW,
        WrittenMessage::Diverted,
        "\u{25EF}main",
    )
    .of_kind(DialogKind::AgentsView),
    DialogPattern::on_row(
        CLAUDE_SUBAGENT_VIEW,
        WrittenMessage::Diverted,
        "\u{276F}\u{25EF}main",
    )
    .of_kind(DialogKind::AgentsView),
    // The agents view (`claude agents`), which ← at an empty prompt opens after moving the
    // conversation into a background session of its own: a paste and Enter here start another
    // session with that text.
    DialogPattern::all_of(
        CLAUDE_AGENTS_VIEW,
        WrittenMessage::Diverted,
        &["Newsessions:", "awaitinginput\u{B7}"],
    )
    .of_kind(DialogKind::AgentsView),
];

#[cfg(test)]
mod tests {
    use super::*;

    const WIDTH: usize = 100;

    /// A screen of Claude Code's input box with `above` the row right over its top rule and
    /// `below` the rows under its bottom rule.
    fn screen(above: &str, below: &[&str]) -> String {
        let rule = "\u{2500}".repeat(WIDTH);
        let mut rows = vec![
            "\u{23FA} an answer from the transcript".to_string(),
            String::new(),
            above.to_string(),
            rule.clone(),
            format!("{PROMPT_MARK}\u{A0}"),
            rule,
        ];
        rows.extend(below.iter().map(|row| row.to_string()));
        rows.join("\n")
    }

    fn right_aligned(text: &str) -> String {
        format!("{text:>WIDTH$}")
    }

    #[test]
    fn a_stash_word_anywhere_in_the_row_above_the_input_box_is_shown() {
        for (name, above) in [
            ("2.1.314 marker", right_aligned("\u{203A} stashed")),
            ("2.1.329 marker", right_aligned("draft stashed")),
            (
                "2.1.329 marker after the effort indicator",
                right_aligned("\u{25D0} medium \u{B7} /effort \u{B7} draft stashed"),
            ),
            ("marker alone", "draft stashed".to_string()),
            (
                "marker followed by other text",
                right_aligned("draft stashed for later"),
            ),
            // The word alone is read, so a reworded indicator that keeps it still is.
            ("a reworded indicator", right_aligned("stashed draft")),
            ("the bare word", right_aligned("stashed")),
            (
                "trailing spaces",
                format!("{}   ", right_aligned("draft stashed")),
            ),
        ] {
            assert!(
                ClaudeScreen.stash_shown(&screen(&above, &[])),
                "{name}: {above:?}"
            );
        }
    }

    #[test]
    fn the_rows_under_the_box_do_not_move_where_the_marker_is_read() {
        let above = right_aligned("draft stashed");
        for below in [
            &[][..],
            &["auto mode on"],
            &["warning line", "auto mode on (shift+tab to cycle)"],
            &["a", "b", "c", "d", "e", "f", "g", "h"],
        ] {
            assert!(
                ClaudeScreen.stash_shown(&screen(&above, below)),
                "{} rows under the box",
                below.len()
            );
        }
    }

    #[test]
    fn a_box_whose_top_rule_names_a_place_in_the_history_still_has_its_marker_read() {
        // As wide as the plain rule under the box: `─── History 3/9 ` is 16 cells.
        let rule = "\u{2500}".repeat(WIDTH - 16);
        let screen = format!(
            "{}\n\u{2500}\u{2500}\u{2500} History 3/9 {rule}\n{PROMPT_MARK}\u{A0}old prompt\n{}",
            right_aligned("draft stashed"),
            "\u{2500}".repeat(WIDTH),
        );
        assert!(ClaudeScreen.input_box_rows(&screen).is_some());
        assert!(ClaudeScreen.stash_shown(&screen));
    }

    #[test]
    fn no_stash_is_shown_without_the_marker_directly_above_the_top_rule() {
        let marker = right_aligned("draft stashed");
        for (name, screen) in [
            ("an empty row above", screen("", &[])),
            (
                "other text above",
                screen(&right_aligned("medium \u{B7} /effort"), &[]),
            ),
            (
                "the marker two rows above",
                format!("{marker}\n{}", screen("", &[])),
            ),
            (
                "the marker below the box",
                screen("", &[&marker, "auto mode on"]),
            ),
            (
                "the marker in the transcript",
                screen("", &[]).replacen(
                    "an answer from the transcript",
                    "the footer ends with draft stashed",
                    1,
                ),
            ),
            (
                "the marker typed in the box",
                format!(
                    "{}\n{PROMPT_MARK}\u{A0}draft stashed\n{}",
                    "\u{2500}".repeat(WIDTH),
                    "\u{2500}".repeat(WIDTH)
                ),
            ),
        ] {
            assert!(!ClaudeScreen.stash_shown(&screen), "{name}:\n{screen}");
        }
    }

    #[test]
    fn no_stash_is_shown_when_there_is_no_input_box() {
        let marker = right_aligned("draft stashed");
        let rule = "\u{2500}".repeat(WIDTH);
        for (name, screen) in [
            ("an empty screen", String::new()),
            ("only the marker", marker.clone()),
            ("a rule and no prompt row", format!("{marker}\n{rule}\n")),
            (
                "a prompt row and no rule",
                format!("{marker}\n{PROMPT_MARK}\u{A0}"),
            ),
            (
                "the rule is the first row",
                format!("{rule}\n{PROMPT_MARK}\u{A0}\n{rule}"),
            ),
        ] {
            assert!(!ClaudeScreen.stash_shown(&screen), "{name}");
        }
    }

    #[test]
    fn only_the_last_box_decides_where_the_marker_is_read() {
        let stashed_box = screen(&right_aligned("draft stashed"), &[]);
        let later_box = screen("", &[]);
        assert!(!ClaudeScreen.stash_shown(&format!("{stashed_box}\n{later_box}")));
        assert!(ClaudeScreen.stash_shown(&format!("{later_box}\n{stashed_box}")));
    }

    #[test]
    fn the_indicator_row_is_the_trimmed_row_directly_above_the_top_rule() {
        for (above, row) in [
            (
                right_aligned("\u{25D0} medium \u{B7} /effort"),
                "\u{25D0} medium \u{B7} /effort",
            ),
            (right_aligned("draft stashed"), "draft stashed"),
            (format!("  {}  ", "left padded"), "left padded"),
            (String::new(), ""),
        ] {
            assert_eq!(
                ClaudeScreen
                    .stash_indicator_row(&screen(&above, &["auto mode on"]))
                    .as_deref(),
                Some(row),
                "{above:?}"
            );
        }
    }

    #[test]
    fn the_indicator_row_is_never_a_row_of_the_box_or_the_transcript() {
        let rule = "\u{2500}".repeat(WIDTH);
        let text = screen("", &["below the box"]);
        let row = ClaudeScreen.stash_indicator_row(&text).unwrap();
        for other in [
            "an answer from the transcript",
            "below the box",
            &PROMPT_MARK.to_string(),
            &rule,
        ] {
            assert!(!row.contains(other), "{row:?} holds {other:?}");
        }
        // The last box decides, as for the stash itself.
        let two = format!("{}\n{text}", screen(&right_aligned("draft stashed"), &[]));
        assert_eq!(ClaudeScreen.stash_indicator_row(&two).as_deref(), Some(""));
    }

    #[test]
    fn there_is_no_indicator_row_without_an_input_box() {
        let rule = "\u{2500}".repeat(WIDTH);
        for (name, screen) in [
            ("an empty screen", String::new()),
            ("text only", "an answer\ndraft stashed".to_string()),
            (
                "a rule and no prompt row",
                format!("draft stashed\n{rule}\n"),
            ),
            (
                "the rule is the first row",
                format!("{rule}\n{PROMPT_MARK}\u{A0}\n{rule}"),
            ),
        ] {
            assert_eq!(ClaudeScreen.stash_indicator_row(&screen), None, "{name}");
        }
    }

    /// Replays `capture` into a terminal the size it was recorded at and returns its screen.
    fn replayed(capture: &[u8]) -> String {
        let mut terminal = crate::terminal::snapshot::TerminalSnapshot::new(40, 120);
        terminal.process(capture);
        terminal.visible_text()
    }

    /// The recordings that show a stash are replayed by
    /// `session::swap::tests::measured_native_stash_markers_require_an_empty_composer`; these are
    /// the screens beside them where none is drawn.
    #[test]
    fn a_recorded_screen_without_a_stash_shows_none() {
        let capture = include_bytes!("../../../testdata/claude_input_placeholder.bin");
        let text = replayed(capture);
        assert!(ClaudeScreen.input_box_rows(&text).is_some(), "{text}");
        assert!(!ClaudeScreen.stash_shown(&text), "{text}");
    }

    /// 2.1.329 draws the stash as `draft`, a move to the right edge and `stashed`, in the same
    /// frame that leaves the draft out of the box. The recording up to the start of that frame is
    /// the screen right before: the draft in the box, the indicator row reading `◐ medium ·
    /// /effort` with no marker.
    #[test]
    fn the_screen_before_2_1_329_draws_its_stash_marker_shows_none() {
        let capture = include_bytes!("../../../testdata/claude_native_stashed_2.1.329.bin");
        let find = |haystack: &[u8], needle: &[u8]| {
            haystack
                .windows(needle.len())
                .rposition(|window| window == needle)
                .expect("the capture holds the bytes")
        };
        let marker = find(capture, b"stashed");
        let frame = find(&capture[..marker], b"\x1b[?25l");

        let before = replayed(&capture[..frame]);
        assert!(ClaudeScreen.input_box_rows(&before).is_some(), "{before}");
        assert!(
            before.contains("\u{25D0} medium \u{B7} /effort"),
            "{before}"
        );
        assert!(before.contains("draft text"), "{before}");
        assert!(!ClaudeScreen.stash_shown(&before), "{before}");
        assert!(ClaudeScreen.stash_shown(&replayed(capture)));
    }
}
