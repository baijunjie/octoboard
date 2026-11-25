//! How Antigravity CLI's screens read: its folder-trust screen, its input box and its interrupt
//! key (1.5.14; `docs/agent-cli-reference.md`, section "Antigravity CLI", and the captures of
//! `testdata/antigravity-captures.md`). Its status comes from its own OSC 7501 reports; besides
//! the box, what is read of its screen is the images attached above the box and the slash-command
//! completion under it. The terminal title is the one the launch's own title command gives
//! (`super::title`).

use crate::session::screen::{Interrupt, InterruptKey, ScreenReader};
use crate::session::trust::{Press, Screen, StartSignal, HOOK_FALLBACK};

pub(super) struct AntigravityScreen;

impl ScreenReader for AntigravityScreen {
    fn input_box_rows(&self, screen: &str) -> Option<Vec<String>> {
        input_box_rows(screen)
    }

    /// An empty box names the session's mode where it is not the default one, in grey, which is
    /// not an attribute the screen a reader is given drops ([`input_box_rows`] reads such a box as
    /// empty by itself): `Auto mode: edits & safe commands
    /// auto-approved (shift+tab to cycle)` with a signed-in account, whose default mode is auto
    /// (1.5.14). The hints of the other modes end the same way (read from the executable, not
    /// seen): accept-edits, plan and best-of-N.
    fn input_placeholder(&self, row: &str) -> bool {
        is_mode_hint(row)
    }

    /// Ctrl-U empties the row the cursor is on, and pressed on an empty row takes the row away
    /// (the two-row draft of the captures, and two messages a cancel gave back: three presses
    /// left the box empty), so a box of several rows is cleared a press at a time.
    fn clears_input_box_with_ctrl_u(&self) -> bool {
        true
    }

    /// An image file's path pasted on its own is taken as an attachment, not as text, and an
    /// Enter right behind it is lost: nothing is sent and the path is not in the box (1.5.14).
    /// A path with other text in the same paste stays text.
    fn pasted_image_path_loses_enter(&self) -> bool {
        true
    }

    /// The attachments a message will carry, which the CLI draws above the box as one row,
    /// `▸ 📎 <n> media attached …`, and which [`input_box_rows`] gives as the box's last row.
    fn image_placeholders(&self, text: &str) -> usize {
        text.lines().filter_map(attached_count).sum()
    }

    fn without_image_placeholders(&self, text: &str) -> String {
        text.lines()
            .filter(|row| attached_count(row).is_none())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The command the completion under the box highlights, `> /<name>  <description>`: an
    /// Enter runs it when the box holds exactly its name and completes the box to it otherwise.
    /// `No matches` offers none, and an Enter then answers `Unknown command`.
    fn command_completion(&self, screen: &str) -> Option<String> {
        command_completion(screen)
    }

    /// A message written into a running turn is held above the box until the turn ends, and a
    /// cancel puts what is still held back into the box, unsent: one row a message, in the order
    /// they were written, with nothing of the interrupted prompt beside them (1.5.14).
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

/// The folder-trust screen, shown in a folder that is not among the trusted ones of the CLI's
/// settings: the cursor starts on "Yes, I trust this folder", the first of two options, and Enter
/// accepts in place. Nothing of the prompt is drawn before the answer and no hook runs, so what
/// says the startup is behind it is the input box.
pub(super) const TRUST: Screen = Screen {
    first_draw: &[&[
        "Accessingworkspace:",
        "Doyoutrustthecontentsofthisproject?",
        ">Yes,Itrustthisfolder",
        "No,exit",
    ]],
    accept: Some("Yes,Itrustthisfolder"),
    others: &["No,exit"],
    cursor: Some('>'),
    press: Press::Enter,
    next_screen: None,
    // No hook follows the answer: a new conversation's first comes with its first turn.
    input_ends_wait: true,
    start_signal: StartSignal::InputBox,
    // For a box the reader fails to recognise.
    hook_fallback: Some(HOOK_FALLBACK),
    // It turns bracketed paste on as it starts, before the screen.
    waits_for_paste_mode: false,
};

/// One Esc stops a turn (`Interrupted · What should Antigravity CLI do instead?`). The
/// interrupted prompt is not put back in the input box; the messages still held are
/// ([`ScreenReader::cancel_restores_held_messages`]). They are left for the next write to clear
/// (`crate::session::draft_clear`), which tells them from what the user typed after them, rather
/// than cleared with the key: `restores_prompt` would clear a box that also holds what the user
/// typed since.
const INTERRUPT: Interrupt = Interrupt {
    key: InterruptKey::Esc,
    restores_prompt: false,
    written_during_turn,
    queued_waiting: None,
};

/// A message written into a running turn waits above the input box (`queuedMessages` is `queue`
/// unless the user changed it) and is sent once the turn ends. A cancel puts what still waited
/// back into the input box, unsent, one row a message in the order written. Octoboard clears them
/// out of the box before its next message goes in and writes them again after that message, in
/// their order; only what the user typed after them is told as a draft.
fn written_during_turn(written: usize) -> String {
    format!(
        " Octoboard wrote about {written} message(s) into the session during that turn. \
         Antigravity CLI holds such messages until the turn ends, and the cancel put those still \
         held back into its input box, unsent. Octoboard clears them from the box before its next \
         message and sends them again after that message, in the order they were written; do not \
         send them again yourself. Check the session with `get_session` if one of them matters now."
    )
}

/// How the hint an empty box shows for a mode reads: `<mode> mode: <what it does> (shift+tab to
/// cycle)`.
const MODE_HINT_MARK: &str = " mode: ";
const MODE_HINT_END: &str = "(shift+tab to cycle)";

/// The rule that frames the input box above and below.
const RULE: char = '\u{2500}';

/// The mark the box's first row starts with.
const PROMPT_MARK: char = '>';

/// The most rows with text that follow the box. Its footer (`? for shortcuts`, `esc to cancel`
/// during a turn, the model's name at the right) is one row at every width measured, 60, 80 and
/// 120 columns; one more is let pass, and blank rows are not counted. The slash-command
/// completion takes more rows, and is told by its hint ([`COMPLETION_HINT`]).
const ROWS_UNDER_THE_BOX: usize = 2;

/// The hint row under the slash-command completion, `↑/↓ Navigate · enter Select · tab Complete`.
const COMPLETION_HINT: &str = "tab Complete";

/// What the row of the completion's highlighted command starts with, the slash after it.
const COMPLETION_MARK: &str = "> /";

/// How the row of the attachments above the box reads: `▸ 📎 1 media attached (file, 168 B,
/// image/png)  (ctrl+o to expand)`, `▸ 📎 2 media attached  (ctrl+o to expand)`. A message once
/// sent shows its images as `📎 2 image(s)`, which is not this row.
const ATTACHED_MARK: &str = "\u{1f4ce} ";
const ATTACHED_END: &str = " media attached";

/// What every row of the panel of background work starts with, between the box and a rule of its
/// own above the footer: a command left running (`● [22:50:05] sleep 6; echo finished-bg running`),
/// a timer (`● [09:09:16] 15m timer running`), a cron job (`● [09:09:16] cron (daily at 03:00)
/// running`) and a subagent (`● Agent(sleeper: Probe Sleeper) · Gemini 3.1 Pro  Working · 1s`)
/// alike.
const TASK_ROW_MARK: &str = "\u{25cf} ";

/// The rows of the input box: what lies between the last two rules of the screen, which are of
/// one width, where the first row carries the prompt mark and at most the footer follows the
/// lower rule. A permission prompt or a question has no such frame at the bottom of the screen,
/// and the shorter rule that parts two turns of the conversation closes no box.
///
/// While a command or a subagent runs in the background its panel sits between the box and the
/// footer, framed by a rule of its own, and is passed over. The images attached to the next message
/// are drawn on the row right above the box; that row is given after the box's own rows, for
/// [`ScreenReader::image_placeholders`] to count and for a box holding only attachments not to read
/// as empty: Ctrl-U does not take them away.
fn input_box_rows(screen: &str) -> Option<Vec<String>> {
    let rows: Vec<&str> = screen.lines().collect();
    let mut lower = rows.iter().rposition(|row| rule_width(row).is_some())?;
    let under: Vec<&str> = rows[lower + 1..]
        .iter()
        .copied()
        .filter(|row| !row.trim().is_empty())
        .collect();
    let completion = under
        .iter()
        .any(|row| row.trim_end().ends_with(COMPLETION_HINT));
    if under.len() > ROWS_UNDER_THE_BOX && !completion {
        return None;
    }
    let (upper, framed) = loop {
        let upper = rows[..lower]
            .iter()
            .rposition(|row| rule_width(row).is_some())?;
        if rule_width(rows[upper]) != rule_width(rows[lower]) {
            return None;
        }
        let framed = &rows[upper + 1..lower];
        if framed.first()?.trim_start().starts_with(PROMPT_MARK) {
            break (upper, framed);
        }
        if !is_task_panel(framed) {
            return None;
        }
        lower = upper;
    };
    let mut box_rows = crate::session::input_box::box_rows(framed.iter().copied(), PROMPT_MARK);
    // The hint an empty box shows for the session's mode is grey, not faint, so it is still on
    // the screen a reader is given on every host: a box that shows nothing else is empty. Done
    // here as well as in `input_placeholder`, which the session applies only where its host
    // renders what the program draws.
    if is_mode_hint(&box_rows[0]) && box_rows[1..].iter().all(String::is_empty) {
        box_rows[0].clear();
    }
    if let Some(attached) = upper
        .checked_sub(1)
        .map(|above| rows[above].trim())
        .filter(|row| attached_count(row).is_some())
    {
        box_rows.push(attached.to_string());
    }
    Some(box_rows)
}

/// Whether `framed`, the rows between two rules under the box, is the panel of background work.
fn is_task_panel(framed: &[&str]) -> bool {
    framed.iter().any(|row| !row.trim().is_empty())
        && framed
            .iter()
            .all(|row| row.trim().is_empty() || row.trim_start().starts_with(TASK_ROW_MARK))
}

/// How many images the row of attachments `row` counts, `None` for any other row.
fn attached_count(row: &str) -> Option<usize> {
    let (_, rest) = row.split_once(ATTACHED_MARK)?;
    let (count, _) = rest.split_once(ATTACHED_END)?;
    count.trim().parse().ok()
}

/// The command the completion under the last rule of `screen` highlights, by its name.
fn command_completion(screen: &str) -> Option<String> {
    let rows: Vec<&str> = screen.lines().collect();
    let lower = rows.iter().rposition(|row| rule_width(row).is_some())?;
    let under = &rows[lower + 1..];
    if !under
        .iter()
        .any(|row| row.trim_end().ends_with(COMPLETION_HINT))
    {
        return None;
    }
    let highlighted = under
        .iter()
        .find_map(|row| row.trim_start().strip_prefix(COMPLETION_MARK))?;
    let name = highlighted.split_whitespace().next()?;
    (!name.is_empty()).then(|| name.to_string())
}

/// Whether `row` is the hint an empty box shows for a mode.
fn is_mode_hint(row: &str) -> bool {
    row.contains(MODE_HINT_MARK) && row.ends_with(MODE_HINT_END)
}

/// The width of a row that is a rule and nothing else.
pub(super) fn rule_width(row: &str) -> Option<usize> {
    let row = row.trim();
    let width = row.chars().count();
    (width >= 20 && row.chars().all(|c| c == RULE)).then_some(width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::trust::{reading_of, Press, StartSignal};
    use crate::terminal::program_status::{
        parse, BlockedKind, ProgramStatus, Report, ReportedState,
    };
    use crate::terminal::queries::{Piece, Query, QueryScanner};
    use crate::terminal::snapshot::TerminalSnapshot;

    // Antigravity CLI 1.5.14, `testdata/antigravity-captures.md`: the terminal output of a launch
    // in a folder not yet trusted up to the trust screen, what one Down wrote, the output from
    // Enter on each option on.
    const TRUST_SCREEN: &[u8] = include_bytes!("../../../testdata/antigravity_trust_screen.bin");
    const TRUST_CURSOR_MOVED: &[u8] =
        include_bytes!("../../../testdata/antigravity_trust_cursor_moved.bin");
    const TRUST_ACCEPTED: &[u8] =
        include_bytes!("../../../testdata/antigravity_trust_accepted.bin");
    const TRUST_DECLINED: &[u8] =
        include_bytes!("../../../testdata/antigravity_trust_declined.bin");

    // The terminal output of a launch, from its start to each state, replayed from an empty
    // terminal of the size it was taken at (120x40).
    const INPUT_EMPTY: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_input_empty.bin");
    const INPUT_DRAFT: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_input_draft.bin");
    const INPUT_TWO_ROWS: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_input_draft_two_rows.bin");
    const INPUT_DURING_TURN: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_input_during_turn.bin");
    const INPUT_AUTO_MODE: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_input_empty_auto_mode.bin");
    const PERMISSION_COMMAND: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_permission_command.bin");
    const QUESTION: &[u8] = include_bytes!("../../../testdata/antigravity_screen_question.bin");
    const AFTER_FAILED_TURN: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_after_failed_turn.bin");

    // The empty box and the draft `a draft` at other widths, each from the launch.
    const INPUT_EMPTY_80: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_input_empty_80.bin");
    const INPUT_DRAFT_80: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_input_draft_80.bin");
    const INPUT_EMPTY_60: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_input_empty_60.bin");
    const INPUT_DRAFT_60: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_input_draft_60.bin");

    // More states, each the terminal output of a launch from its start: the two-row draft after
    // Ctrl-U once and twice, the other permission prompts, and the feedback survey that takes the
    // box's place.
    const AFTER_CTRL_U: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_input_after_ctrl_u.bin");
    const AFTER_CTRL_U_TWICE: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_input_after_ctrl_u_twice.bin");
    const PERMISSION_ON_NO: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_permission_command_on_no.bin");
    const PERMISSION_EDIT: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_permission_edit.bin");
    const PERMISSION_WRITE: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_permission_write.bin");
    const SURVEY: &[u8] = include_bytes!("../../../testdata/antigravity_screen_survey.bin");

    // A turn with two messages written into it and held, the same after an Esc (the two given
    // back into the box), a turn that held one after its Esc, and the two-row box after three
    // presses of Ctrl-U; each the terminal output of a launch from its start.
    const HELD_TWO: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_two_messages_held.bin");
    const GAVE_TWO_BACK: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_cancel_gave_two_back.bin");
    const GAVE_ONE_BACK: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_cancel_gave_one_back.bin");
    const GIVEN_BACK_CLEARED: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_given_back_cleared.bin");

    // The terminal output of one turn each, from the paste of its prompt to the settled screen,
    // and the end of the process.
    const TURN_ANSWER: &[u8] = include_bytes!("../../../testdata/antigravity_turn_answer.bin");
    const TURN_CANCEL: &[u8] = include_bytes!("../../../testdata/antigravity_turn_cancel_esc.bin");
    const TURN_APPROVED: &[u8] =
        include_bytes!("../../../testdata/antigravity_turn_permission_approved.bin");
    const TURN_DECLINED: &[u8] =
        include_bytes!("../../../testdata/antigravity_turn_permission_declined.bin");
    const TURN_WRITE_FILE: &[u8] =
        include_bytes!("../../../testdata/antigravity_turn_write_file.bin");
    const TURN_EDIT_FILE: &[u8] =
        include_bytes!("../../../testdata/antigravity_turn_edit_file.bin");
    const TURN_QUESTION: &[u8] = include_bytes!("../../../testdata/antigravity_turn_question.bin");
    const TURN_MCP_TOOL: &[u8] = include_bytes!("../../../testdata/antigravity_turn_mcp_tool.bin");
    const TURN_FAILED: &[u8] = include_bytes!("../../../testdata/antigravity_turn_failed.bin");
    const TURN_LONG_REPLY: &[u8] =
        include_bytes!("../../../testdata/antigravity_turn_long_reply.bin");
    const EXIT: &[u8] = include_bytes!("../../../testdata/antigravity_exit.bin");

    const MODE_HINT: &str = "Auto mode: edits & safe commands auto-approved (shift+tab to cycle)";

    /// What a 120x40 terminal shows after `captures`, in order. Antigravity reads drafts with
    /// faint cells blanked (`visible_text_undimmed`) and looks for the start signal in the screen
    /// as drawn (`visible_text`).
    fn rendered(captures: &[&[u8]]) -> [String; 2] {
        rendered_at(120, captures)
    }

    /// [`rendered`] for a terminal of `cols` columns, the width the captures were taken at.
    fn rendered_at(cols: u16, captures: &[&[u8]]) -> [String; 2] {
        let mut terminal = TerminalSnapshot::new(40, cols);
        for capture in captures {
            terminal.process(capture);
        }
        [terminal.visible_text_undimmed(), terminal.visible_text()]
    }

    fn rows_of(screen: &str) -> Option<Vec<String>> {
        AntigravityScreen.input_box_rows(screen)
    }

    /// One state the input box is captured in: its name, the raw capture, a text the blanked
    /// screen has to show, and the rows the box reads as.
    type InputBoxCase = (
        &'static str,
        &'static [u8],
        &'static str,
        Option<Vec<String>>,
    );

    fn rows(texts: &[&str]) -> Option<Vec<String>> {
        Some(texts.iter().map(|text| text.to_string()).collect())
    }

    /// The input box reads as its rows in every state it is drawn in, off both renderings of the
    /// raw capture: with the faint cells blanked as production reads a draft, and as drawn as the
    /// start signal reads it. The one thing the CLI draws faint is the model name in the footer, so
    /// nothing the reader relies on is lost; each case checks that what it reads is on the blanked
    /// screen.
    #[test]
    fn the_input_box_is_read_in_every_state_off_both_renderings() {
        let cases: [InputBoxCase; 5] = [
            ("empty", INPUT_EMPTY, "? for shortcuts", rows(&[""])),
            ("a draft", INPUT_DRAFT, "> a draft", rows(&["a draft"])),
            (
                "two rows",
                INPUT_TWO_ROWS,
                "second row",
                rows(&["a draft", "second row"]),
            ),
            (
                "during a turn",
                INPUT_DURING_TURN,
                "esc to cancel",
                rows(&[""]),
            ),
            (
                "after a failed turn",
                AFTER_FAILED_TURN,
                "agent executor error",
                rows(&[""]),
            ),
        ];
        for (name, capture, shown, expected) in cases {
            let [undimmed, plain] = rendered(&[capture]);
            assert!(undimmed.contains(shown), "{name}: {undimmed}");
            assert_eq!(rows_of(&undimmed), expected, "{name} undimmed: {undimmed}");
            assert_eq!(rows_of(&plain), expected, "{name} plain: {plain}");
        }
    }

    /// The faint cell of the footer is what the production rendering drops, so the two renderings
    /// above differ, and the reader gets the same rows from both.
    #[test]
    fn the_footers_model_name_is_the_faint_part_of_the_screen() {
        let [undimmed, plain] = rendered(&[INPUT_EMPTY]);
        assert!(plain.contains("· low"), "{plain}");
        assert!(!undimmed.contains("· low"), "{undimmed}");
    }

    /// A signed-in account's empty box shows the grey hint of its mode: the box reads as one empty
    /// row, off the rendering that blanks faint cells and off the one that keeps them, which both
    /// hold the hint (it is grey, not faint), and the hint is a placeholder.
    #[test]
    fn the_mode_hint_of_an_empty_box_reads_as_one_empty_row() {
        let [undimmed, plain] = rendered(&[INPUT_AUTO_MODE]);
        for screen in [&undimmed, &plain] {
            assert!(screen.contains(MODE_HINT), "{screen}");
            assert_eq!(rows_of(screen), rows(&[""]), "{screen}");
        }
        assert!(AntigravityScreen.input_placeholder(MODE_HINT));
        for hint in [
            "Accept-edits mode: file edits are auto-approved (shift+tab to cycle)",
            "Plan mode: read-only (shift+tab to cycle)",
        ] {
            assert!(AntigravityScreen.input_placeholder(hint), "{hint}");
        }
        for draft in [
            "a draft",
            "",
            "Auto mode",
            "Auto mode: edits & safe commands auto-approved",
            "(shift+tab to cycle)",
            "a draft mentioning (shift+tab to cycle)",
        ] {
            assert!(!AntigravityScreen.input_placeholder(draft), "{draft:?}");
        }
    }

    /// Only a box that shows nothing but the hint is empty: the hint with more under it, or text
    /// that merely mentions a mode, is a draft. Rows laid out as the real screen lays them.
    #[test]
    fn only_a_box_showing_nothing_but_the_hint_is_empty() {
        let rule = "\u{2500}".repeat(120);
        let screen_with = |first: &str, more: &[&str]| {
            let mut box_rows = vec![format!("> {first}")];
            box_rows.extend(more.iter().map(|row| format!("  {row}")));
            format!("{rule}\n{}\n{rule}\n? for shortcuts\n", box_rows.join("\n"))
        };
        assert_eq!(rows_of(&screen_with(MODE_HINT, &[])), rows(&[""]));
        assert_eq!(
            rows_of(&screen_with(MODE_HINT, &["", ""])),
            rows(&["", "", ""])
        );
        assert_eq!(
            rows_of(&screen_with(MODE_HINT, &["more"])),
            rows(&[MODE_HINT, "more"])
        );
        assert_eq!(
            rows_of(&screen_with("note the auto mode: please", &[])),
            rows(&["note the auto mode: please"])
        );
    }

    /// The box reads the same at 80 and at 60 columns, off both renderings: the rules are as wide
    /// as the terminal, and the footer, which is one row there, is not mistaken for more under the
    /// box (the empty box shows `? for shortcuts` there, a draft nothing readable).
    #[test]
    fn the_box_reads_the_same_at_other_widths() {
        for (cols, empty, draft) in [
            (80, INPUT_EMPTY_80, INPUT_DRAFT_80),
            (60, INPUT_EMPTY_60, INPUT_DRAFT_60),
        ] {
            for (capture, footer, expected) in [
                (empty, true, rows(&[""])),
                (draft, false, rows(&["a draft"])),
            ] {
                for screen in rendered_at(cols, &[capture]) {
                    assert!(
                        screen.contains(&"\u{2500}".repeat(usize::from(cols))),
                        "{cols}: {screen}"
                    );
                    assert_eq!(
                        screen.contains("? for shortcuts"),
                        footer,
                        "{cols}: {screen}"
                    );
                    assert_eq!(rows_of(&screen), expected, "{cols}: {screen}");
                }
            }
        }
    }

    /// A permission prompt and a question have no input box at the bottom, whatever the CLI drew
    /// above them, off both renderings.
    #[test]
    fn a_permission_prompt_and_a_question_have_no_input_box() {
        for (name, capture, shown) in [
            ("permission", PERMISSION_COMMAND, "Run this command?"),
            ("question", QUESTION, "Which target?"),
        ] {
            let [undimmed, plain] = rendered(&[capture]);
            assert!(undimmed.contains(shown), "{name}: {undimmed}");
            assert_eq!(rows_of(&undimmed), None, "{name} undimmed: {undimmed}");
            assert_eq!(rows_of(&plain), None, "{name} plain: {plain}");
        }
    }

    /// The shorter rule that parts two turns of the conversation closes no box: the question
    /// screen cut right after its header's full-width rule has that rule at the bottom with a short
    /// one above it.
    #[test]
    fn the_short_rule_between_turns_closes_no_box() {
        let [undimmed, _] = rendered(&[QUESTION]);
        let full = "\u{2500}".repeat(120);
        let short = "\u{2500}".repeat(60);
        let all: Vec<&str> = undimmed.lines().collect();
        let header = all
            .iter()
            .rposition(|row| row.trim() == full)
            .expect("the header's rule");
        assert!(
            all[..header].iter().any(|row| row.trim() == short),
            "a short rule above it: {undimmed}"
        );
        let cut = all[..=header].join("\n");
        assert_eq!(rows_of(&cut), None, "{cut}");
    }

    /// Ctrl-U empties the row the cursor is on and no more, so the box keeps its first row; the
    /// other permission prompts and the survey, which takes the box's place, have no box. Read
    /// off both renderings of each raw capture.
    #[test]
    fn ctrl_u_leaves_the_first_row_and_the_other_prompts_and_the_survey_have_no_box() {
        for screen in rendered(&[AFTER_CTRL_U]) {
            assert_eq!(rows_of(&screen), rows(&["a draft", ""]), "{screen}");
        }
        for screen in rendered(&[AFTER_CTRL_U_TWICE]) {
            assert_eq!(rows_of(&screen), rows(&["a draft"]), "{screen}");
        }
        for (name, capture, shown) in [
            ("command on No", PERMISSION_ON_NO, "Run this command?"),
            ("edit", PERMISSION_EDIT, "Accept this file edit?"),
            ("write", PERMISSION_WRITE, "Allow creation of this file?"),
            ("survey", SURVEY, "How's the CLI experience so far?"),
        ] {
            for screen in rendered(&[capture]) {
                assert!(screen.contains(shown), "{name}: {screen}");
                assert_eq!(rows_of(&screen), None, "{name}: {screen}");
            }
        }
    }

    #[test]
    fn a_screen_with_no_frame_has_no_box() {
        assert_eq!(rows_of(""), None);
        assert_eq!(rows_of("> not between rules\nnor under one\n"), None);
        // A rule too short to be the box's does not open one.
        assert_eq!(
            rows_of("\u{2500}\u{2500}\u{2500}\n> hello\n\u{2500}\u{2500}\u{2500}\n"),
            None
        );
    }

    /// The box is never drawn before the trust screen, or its drawing would end the wait for it,
    /// and it is there once the screen is accepted, off both renderings.
    #[test]
    fn the_box_comes_only_after_the_trust_screen_is_accepted() {
        for screen in rendered(&[TRUST_SCREEN]) {
            assert_eq!(rows_of(&screen), None, "{screen}");
        }
        for screen in rendered(&[TRUST_SCREEN, TRUST_CURSOR_MOVED]) {
            assert_eq!(rows_of(&screen), None, "{screen}");
        }
        for screen in rendered(&[TRUST_SCREEN, TRUST_ACCEPTED]) {
            assert_eq!(rows_of(&screen), rows(&[""]), "{screen}");
        }
    }

    /// The first drawing is sighted whether it is read off the output or off the parsed screen,
    /// the screen is up, and the cursor is on "Yes, I trust this folder", the option Octoboard
    /// presses with an Enter.
    #[test]
    fn the_first_drawing_is_the_trust_screen_with_the_cursor_on_the_option_that_trusts() {
        let trust = AntigravityScreen.trust_screen();
        let on_the_output = reading_of(trust, TRUST_SCREEN);
        assert!(on_the_output.shown, "{on_the_output:?}");
        assert!(on_the_output.up, "{on_the_output:?}");
        assert_eq!(
            on_the_output.cursor_on_accept,
            Some(true),
            "{on_the_output:?}"
        );
        assert!(
            !on_the_output.gone && !on_the_output.replaced,
            "{on_the_output:?}"
        );

        for screen in rendered(&[TRUST_SCREEN]) {
            let on_the_screen = reading_of(trust, screen.as_bytes());
            assert!(on_the_screen.shown, "{screen}");
            assert!(on_the_screen.up, "{screen}");
            assert_eq!(on_the_screen.cursor_on_accept, Some(true), "{screen}");
            assert!(!on_the_screen.gone && !on_the_screen.replaced, "{screen}");
        }
        assert_eq!(trust.press, Press::Enter);
    }

    /// Nothing but the whole first drawing sights it: the input box, the options alone and an empty
    /// capture are not it.
    #[test]
    fn a_partial_drawing_and_the_input_box_are_not_the_trust_screen() {
        let trust = AntigravityScreen.trust_screen();
        for (name, capture) in [
            ("the options", TRUST_CURSOR_MOVED),
            ("the accepted output", TRUST_ACCEPTED),
            ("nothing", b"" as &[u8]),
        ] {
            let reading = reading_of(trust, capture);
            assert!(!reading.shown, "{name}: {reading:?}");
            assert!(!reading.up, "{name}: {reading:?}");
        }
        for screen in rendered(&[INPUT_EMPTY]) {
            let reading = reading_of(trust, screen.as_bytes());
            assert!(!reading.shown && !reading.up, "{screen}");
        }
    }

    /// After one Down the cursor is on "No, exit": read as not on the option that trusts, off the
    /// output, whose last cursor glyph is the moved one, and off the parsed screen, where the
    /// screen is still up and not gone.
    #[test]
    fn a_cursor_moved_by_a_down_is_not_on_the_option_that_trusts() {
        let trust = AntigravityScreen.trust_screen();
        let on_the_moved_bytes = reading_of(trust, TRUST_CURSOR_MOVED);
        assert_eq!(on_the_moved_bytes.cursor_on_accept, Some(false));

        let whole: Vec<u8> = [TRUST_SCREEN, TRUST_CURSOR_MOVED].concat();
        let on_the_output = reading_of(trust, &whole);
        assert!(on_the_output.shown, "{on_the_output:?}");
        assert_eq!(
            on_the_output.cursor_on_accept,
            Some(false),
            "{on_the_output:?}"
        );

        for screen in rendered(&[TRUST_SCREEN, TRUST_CURSOR_MOVED]) {
            assert!(screen.contains("> No, exit"), "{screen}");
            let on_the_screen = reading_of(trust, screen.as_bytes());
            assert!(on_the_screen.up, "{screen}");
            assert_eq!(on_the_screen.cursor_on_accept, Some(false), "{screen}");
            assert!(!on_the_screen.gone, "{screen}");
        }
    }

    /// Once Enter has accepted, what is printed holds nothing of the screen: it is gone, and
    /// replaced by what follows, off the output after the key and off the parsed screen at the
    /// end. Declining ends the process and leaves the cursor on "No, exit".
    #[test]
    fn the_screen_is_gone_once_it_is_accepted() {
        let trust = AntigravityScreen.trust_screen();
        let after = reading_of(trust, TRUST_ACCEPTED);
        assert!(after.gone && after.replaced, "{after:?}");
        assert!(!after.shown && !after.up, "{after:?}");
        assert_eq!(after.cursor_on_accept, None, "{after:?}");

        for screen in rendered(&[TRUST_SCREEN, TRUST_ACCEPTED]) {
            let at_the_end = reading_of(trust, screen.as_bytes());
            assert!(at_the_end.gone, "{screen}");
            assert!(!at_the_end.shown && !at_the_end.up, "{screen}");
        }

        let declined = reading_of(trust, TRUST_DECLINED);
        assert_eq!(declined.cursor_on_accept, Some(false), "{declined:?}");
    }

    /// Nothing marks the startup as over before its input box is drawn, and a box the reader fails
    /// to recognise is let through after the fallback.
    #[test]
    fn the_input_box_is_the_start_signal_with_a_fallback_for_a_box_not_recognised() {
        let trust = AntigravityScreen.trust_screen();
        assert_eq!(trust.start_signal, StartSignal::InputBox);
        assert_eq!(
            trust.hook_fallback,
            Some(crate::session::trust::HOOK_FALLBACK)
        );
    }

    /// One Esc stops a turn. The key clears nothing by itself; what Octoboard wrote into the turn
    /// is told to the caller together with where the cancel left it and what becomes of it.
    #[test]
    fn a_turn_is_interrupted_by_one_esc() {
        let interrupt = AntigravityScreen
            .interrupt()
            .expect("Antigravity CLI can be interrupted");
        assert_eq!(interrupt.key, InterruptKey::Esc);
        assert!(!interrupt.restores_prompt);
        assert!(interrupt.queued_waiting.is_none());
        let told = (interrupt.written_during_turn)(3);
        assert!(told.contains('3'), "{told}");
        assert!(told.contains("input box"), "{told}");
        assert!(told.contains("unsent"), "{told}");
        assert!(told.contains("get_session"), "{told}");
        assert!(!told.contains("until someone"), "{told}");
        // Several given back at once are written again like one: none is told as a draft.
        assert!(told.contains("sends them again"), "{told}");
        assert!(!told.contains("draft"), "{told}");
    }

    /// Messages written into a running turn are held above the box, which stays empty; a cancel
    /// puts them back into it, unsent, a row a message in the order written, and nothing of the
    /// interrupted prompt with them; Ctrl-U pressed until the box reads empty clears them (three
    /// presses for two rows). So the provider declares both: the cancel gives held messages back,
    /// and Ctrl-U clears the box a row a press. Read off both renderings of each raw capture.
    #[test]
    fn a_cancel_gives_held_messages_back_a_row_each_and_ctrl_u_clears_them() {
        for screen in rendered(&[HELD_TWO]) {
            assert!(screen.contains("held message one"), "{screen}");
            assert!(screen.contains("held message two"), "{screen}");
            assert_eq!(rows_of(&screen), rows(&[""]), "{screen}");
        }
        for screen in rendered(&[GAVE_TWO_BACK]) {
            assert!(screen.contains("Interrupted"), "{screen}");
            assert_eq!(
                rows_of(&screen),
                rows(&["held message one", "held message two"]),
                "{screen}"
            );
        }
        for screen in rendered(&[GAVE_ONE_BACK]) {
            assert_eq!(rows_of(&screen), rows(&["held message one"]), "{screen}");
        }
        for screen in rendered(&[GIVEN_BACK_CLEARED]) {
            assert!(screen.contains("Interrupted"), "{screen}");
            assert_eq!(rows_of(&screen), rows(&[""]), "{screen}");
        }
        assert!(AntigravityScreen.cancel_restores_held_messages());
        assert!(AntigravityScreen.clears_input_box_with_ctrl_u());
    }

    /// What the generic draft path needs of the box to tell several messages a cancel gave back
    /// from a draft (`crate::session::draft_clear`): the messages first, each whole, in the order
    /// they were written, nothing between or after them but whitespace. While the two are still
    /// held their text is on the screen above the box and none of it is in the box, so nothing
    /// reads as given back before the cancel.
    #[test]
    fn two_held_messages_read_as_given_back_only_after_the_cancel_and_in_the_order_written() {
        let written = ["held message one", "held message two"];
        let bare = |text: &str| -> String { text.split_whitespace().collect() };
        let in_order: String = written.iter().map(|message| bare(message)).collect();

        for screen in rendered(&[HELD_TWO]) {
            for message in written {
                assert!(screen.contains(message), "{screen}");
            }
            let shown = rows_of(&screen)
                .expect("the box of a running turn")
                .join("\n");
            assert_eq!(bare(&shown), "", "{screen}");
        }
        for screen in rendered(&[GAVE_TWO_BACK]) {
            let shown = rows_of(&screen)
                .expect("the box after the cancel")
                .join("\n");
            assert_eq!(bare(&shown), in_order, "{screen}");
        }
    }

    // The OSC 7501 reports each capture holds, replayed through the scanner and parser the
    // session's terminal uses. They are what the CLI writes and the session's status depends on.

    #[derive(Debug, Clone, PartialEq)]
    struct Reported {
        state: ReportedState,
        kind: Option<BlockedKind>,
        msg: Option<String>,
    }

    fn reported(state: ReportedState, kind: Option<BlockedKind>, msg: Option<&str>) -> Reported {
        Reported {
            state,
            kind,
            msg: msg.map(str::to_string),
        }
    }

    /// The reports in `capture`, in order, fed to the scanner in pieces of `chunk` bytes.
    fn reports_in(capture: &[u8], chunk: usize) -> (Vec<Reported>, Option<ReportedState>) {
        let mut scanner = QueryScanner::default();
        let mut status = ProgramStatus::default();
        let mut reports = Vec::new();
        for part in capture.chunks(chunk) {
            for piece in scanner.scan(part) {
                let Piece::Sequence(Query::ProgramStatusReport(body), _) = piece else {
                    continue;
                };
                let report = parse(&body).expect("a report the parser takes");
                let Report::Set { id: None, record } = &report else {
                    panic!("not a report of the root record: {report:?}");
                };
                reports.push(Reported {
                    state: record.state,
                    kind: record.kind,
                    msg: record.msg.clone(),
                });
                status.apply(report);
            }
        }
        (reports, status.root())
    }

    /// The reports of `capture`, which come out the same whatever the size of the reads they
    /// arrive in.
    fn reports_of(capture: &[u8]) -> Vec<Reported> {
        let (whole, root) = reports_in(capture, capture.len().max(1));
        for chunk in [1, 7, 61] {
            assert_eq!(reports_in(capture, chunk).0, whole, "chunks of {chunk}");
        }
        assert_eq!(root, whole.last().map(|last| last.state));
        whole
    }

    fn idle() -> Reported {
        reported(ReportedState::Idle, None, None)
    }

    fn working() -> Reported {
        reported(ReportedState::Working, None, None)
    }

    fn done(msg: &str) -> Reported {
        reported(ReportedState::Done, None, Some(msg))
    }

    fn blocked(kind: BlockedKind, msg: &str) -> Reported {
        reported(ReportedState::Blocked, Some(kind), Some(msg))
    }

    /// Each turn's reports: `working` when the message is submitted, `done` with the reply's text,
    /// `error` with the error's text, `idle` for a turn cancelled with Esc or a prompt declined,
    /// and `blocked` with the kind of what waits on the user.
    #[test]
    fn a_turn_reports_working_and_then_how_it_ended() {
        assert_eq!(reports_of(TURN_ANSWER), [working(), done("Stub: hello.")]);
        assert_eq!(reports_of(TURN_CANCEL), [working(), idle()]);
        assert_eq!(
            reports_of(TURN_APPROVED),
            [
                working(),
                blocked(
                    BlockedKind::Permission,
                    "Approve Bash: echo octo > made.txt"
                ),
                working(),
                done("Stub: tool finished."),
            ]
        );
        assert_eq!(
            reports_of(TURN_DECLINED),
            [
                working(),
                blocked(
                    BlockedKind::Permission,
                    "Approve Bash: echo octo > other.txt"
                ),
                idle(),
            ]
        );
        assert_eq!(
            reports_of(TURN_QUESTION),
            [
                working(),
                blocked(BlockedKind::Question, "Which target?"),
                working(),
                done("Stub: tool finished."),
            ]
        );
        assert_eq!(
            reports_of(TURN_FAILED),
            [
                working(),
                reported(
                    ReportedState::Error,
                    None,
                    Some(
                        "agent executor error: generating and executing: Error 402, Message: \
                         Stub: credits depleted., Status: RESOURCE_EXHAUSTED, Details: []"
                    )
                ),
            ]
        );
    }

    /// A file's creation and its edit are each a permission wait named `Approve Edit: …`.
    #[test]
    fn a_file_write_waits_on_a_permission_named_for_the_edit() {
        for capture in [TURN_WRITE_FILE, TURN_EDIT_FILE] {
            let reports = reports_of(capture);
            assert_eq!(reports.len(), 4, "{reports:?}");
            assert_eq!(reports[0], working());
            assert_eq!(
                reports[1],
                blocked(
                    BlockedKind::Permission,
                    "Approve Edit: Write: /scratch/project/note.txt"
                ),
                "{reports:?}"
            );
            assert_eq!(reports[2..], [working(), done("Stub: tool finished.")]);
        }
    }

    /// A tool of Octoboard's server, allowed by the launch's rule, is a turn with no prompt:
    /// `working` and `done`. (The capture starts with the `idle` of the turn before it.)
    #[test]
    fn a_tool_the_launch_allows_raises_no_prompt() {
        let reports = reports_of(TURN_MCP_TOOL);
        let (before, turn) = reports.split_at(reports.len() - 2);
        assert_eq!(turn, [working(), done("Stub: tool finished.")]);
        assert!(before.iter().all(|report| *report == idle()), "{reports:?}");
    }

    /// The `done` report carries only the start of a long reply: its first 199 characters and an
    /// ellipsis, 202 bytes, which is what the CLI writes and not what Octoboard cuts.
    #[test]
    fn the_report_of_a_long_reply_carries_only_its_start() {
        let reports = reports_of(TURN_LONG_REPLY);
        assert_eq!(reports.len(), 2, "{reports:?}");
        assert_eq!(reports[0], working());
        assert_eq!(reports[1].state, ReportedState::Done);
        let msg = reports[1].msg.as_deref().expect("the reply's start");
        assert_eq!(msg.len(), 202);
        assert_eq!(msg.chars().count(), 200);
        assert!(msg.starts_with("BEGIN"), "{msg}");
        assert!(msg.ends_with('\u{2026}'), "{msg}");
        assert!(!msg.contains("END"), "{msg}");
    }

    /// The trust screen is a permission wait, answering it makes the prompt `idle`, declining ends
    /// the process on `idle`, and so does leaving it with Ctrl-D.
    #[test]
    fn the_trust_screen_and_the_exit_report_as_they_were_measured() {
        assert_eq!(
            reports_of(TRUST_SCREEN),
            [blocked(BlockedKind::Permission, "Trust this folder?")]
        );
        assert_eq!(reports_of(TRUST_ACCEPTED), [idle()]);
        assert_eq!(reports_of(TRUST_DECLINED), [idle()]);
        assert_eq!(reports_of(EXIT), [idle(), idle()]);
    }

    /// A launch is `idle` once the prompt is up and `working` while a turn runs, and the screens
    /// of a launch end on the state the capture is named for. A launch with the user's account
    /// reports a wait on a sign-in for a moment first, which a launch with the stand-in did not.
    #[test]
    fn the_screens_of_a_launch_end_on_the_state_they_are_named_for() {
        for capture in [INPUT_EMPTY, INPUT_DRAFT, INPUT_TWO_ROWS] {
            assert_eq!(reports_of(capture), [idle()]);
        }
        assert_eq!(reports_of(INPUT_DURING_TURN), [idle(), working()]);
        assert_eq!(
            reports_of(INPUT_AUTO_MODE),
            [blocked(BlockedKind::Auth, "Sign in required"), idle()]
        );
        assert_eq!(
            reports_of(PERMISSION_COMMAND).last(),
            Some(&blocked(
                BlockedKind::Permission,
                "Approve Bash: echo octo > made.txt"
            ))
        );
        assert_eq!(
            reports_of(QUESTION).last(),
            Some(&blocked(BlockedKind::Question, "Which target?"))
        );
        for capture in [
            AFTER_CTRL_U,
            AFTER_CTRL_U_TWICE,
            INPUT_EMPTY_80,
            INPUT_DRAFT_80,
            INPUT_EMPTY_60,
            INPUT_DRAFT_60,
        ] {
            assert_eq!(reports_of(capture), [idle()]);
        }
        assert_eq!(
            reports_of(PERMISSION_ON_NO).last(),
            Some(&blocked(
                BlockedKind::Permission,
                "Approve Bash: echo octo > other.txt"
            ))
        );
        for capture in [PERMISSION_WRITE, PERMISSION_EDIT] {
            assert_eq!(
                reports_of(capture).last(),
                Some(&blocked(
                    BlockedKind::Permission,
                    "Approve Edit: Write: /scratch/project/note.txt"
                ))
            );
        }
        assert_eq!(
            reports_of(SURVEY).last().map(|last| last.state),
            Some(ReportedState::Error)
        );
        let after_failed = reports_of(AFTER_FAILED_TURN);
        let last = after_failed.last().expect("reports");
        assert_eq!(last.state, ReportedState::Error);
        assert!(
            last.msg
                .as_deref()
                .is_some_and(|msg| msg.starts_with("agent executor error: ")),
            "{last:?}"
        );
    }

    // Taken on 2026-11-23 against the stand-in (`testdata/antigravity-captures.md`, "Attachments,
    // the completion and the task panel"), each from the start of a launch.
    const IMAGE_ONE: &[u8] = include_bytes!("../../../testdata/antigravity_screen_image_one.bin");
    const IMAGE_TWO: &[u8] = include_bytes!("../../../testdata/antigravity_screen_image_two.bin");
    const IMAGE_TWO_TEXT: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_image_two_text.bin");
    const COMPLETION_PREFIX: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_completion_prefix.bin");
    const COMPLETION_NONE: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_completion_none.bin");
    const BACKGROUND_TASK: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_background_task.bin");
    const BACKGROUND_TASK_DRAFT: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_background_task_draft.bin");
    const SUBAGENT_PANEL: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_subagent_panel.bin");
    const SUBAGENT_PANEL_DRAFT: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_subagent_panel_draft.bin");
    const SUBAGENT_PANEL_PERMISSION: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_subagent_panel_permission.bin");
    // Taken on 2026-11-24 against the stand-in (`testdata/antigravity-captures.md`, "The timer's
    // and the cron job's rows in the panel"), each from the start of one launch.
    const SCHEDULE_PANEL: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_schedule_panel.bin");
    const SCHEDULE_PANEL_DRAFT: &[u8] =
        include_bytes!("../../../testdata/antigravity_screen_schedule_panel_draft.bin");

    fn box_text(capture: &[u8]) -> [String; 2] {
        rendered(&[capture]).map(|screen| {
            rows_of(&screen)
                .unwrap_or_else(|| panic!("no box on:\n{screen}"))
                .join("\n")
        })
    }

    /// An image path pasted on its own is drawn as a row of attachments above the box, which the
    /// box's rows end with: one image, then two, then two beside a draft. Each is counted as an
    /// attachment, and taking the attachments away leaves what was typed.
    #[test]
    fn attached_images_are_counted_off_the_row_above_the_box() {
        for (capture, images, typed) in [
            (IMAGE_ONE, 1, ""),
            (IMAGE_TWO, 2, ""),
            (IMAGE_TWO_TEXT, 2, "with text"),
        ] {
            for text in box_text(capture) {
                assert_eq!(
                    AntigravityScreen.image_placeholders(&text),
                    images,
                    "{text:?}"
                );
                assert_eq!(
                    AntigravityScreen.without_image_placeholders(&text).trim(),
                    typed,
                    "{text:?}"
                );
            }
        }
        // A box with no attachment counts none, and a sent message's images are not the row.
        for text in box_text(INPUT_DRAFT) {
            assert_eq!(AntigravityScreen.image_placeholders(&text), 0);
        }
        assert_eq!(
            attached_count("\u{25b8} \u{1f4ce} 2 image(s) (336 B)  (ctrl+o to expand)"),
            None
        );
        assert!(AntigravityScreen.pasted_image_path_loses_enter());
    }

    /// A box holding nothing but attachments is not empty: they go with the next message.
    #[test]
    fn a_box_with_only_attachments_is_not_empty() {
        for screen in rendered(&[IMAGE_ONE]) {
            let rows = rows_of(&screen).expect("a box");
            assert_eq!(rows.len(), 2, "{rows:?}");
            assert_eq!(rows[0], "");
            assert!(rows[1].contains("1 media attached"), "{rows:?}");
        }
    }

    /// The completion under the box names the command it highlights; `No matches` names none,
    /// and a screen with no completion up names none either. The box stays readable under it.
    #[test]
    fn the_completion_names_its_highlighted_command() {
        for screen in rendered(&[COMPLETION_PREFIX]) {
            assert_eq!(
                AntigravityScreen.command_completion(&screen).as_deref(),
                Some("changelog")
            );
            assert_eq!(rows_of(&screen), rows(&["/cha"]));
        }
        for screen in rendered(&[COMPLETION_NONE]) {
            assert_eq!(AntigravityScreen.command_completion(&screen), None);
            assert_eq!(rows_of(&screen), rows(&["/nosuchcmd"]));
        }
        for screen in rendered(&[INPUT_DRAFT]) {
            assert_eq!(AntigravityScreen.command_completion(&screen), None);
        }
    }

    /// While a command runs in the background its panel sits between the box and the footer under
    /// a rule of its own; the box above it is still read, empty and with a draft.
    #[test]
    fn the_box_is_read_above_the_panel_of_background_tasks() {
        for screen in rendered(&[BACKGROUND_TASK]) {
            assert_eq!(rows_of(&screen), rows(&[""]), "{screen}");
        }
        for screen in rendered(&[BACKGROUND_TASK_DRAFT]) {
            assert_eq!(rows_of(&screen), rows(&["a draft"]), "{screen}");
        }
        // Rows between two rules that are neither the box nor tasks close no box.
        assert!(!is_task_panel(&["  something else"]));
        assert!(!is_task_panel(&["", ""]));
    }

    /// A subagent's row in the panel (`● Agent(sleeper: Probe Sleeper) · Gemini 3.1 Pro  Working ·
    /// 1s`) is passed over as a command's is: the box above it is read, empty and with a draft. A
    /// permission prompt drawn while the subagent runs, which hides the panel and the box, is
    /// still no box.
    #[test]
    fn the_box_is_read_above_a_subagents_row_in_the_panel() {
        for screen in rendered(&[SUBAGENT_PANEL]) {
            assert!(screen.contains("\u{25cf} Agent(sleeper"), "{screen}");
            assert_eq!(rows_of(&screen), rows(&[""]), "{screen}");
        }
        for screen in rendered(&[SUBAGENT_PANEL_DRAFT]) {
            assert_eq!(rows_of(&screen), rows(&["a draft"]), "{screen}");
        }
        for screen in rendered(&[SUBAGENT_PANEL_PERMISSION]) {
            assert!(screen.contains("Run this command?"), "{screen}");
            assert_eq!(rows_of(&screen), None, "{screen}");
        }
        // A row with the mark and no space after it, or text before it, is not a panel's.
        assert!(!is_task_panel(&["  \u{25cf}Agent(sleeper: Probe Sleeper)"]));
        assert!(!is_task_panel(&[
            "  x \u{25cf} Agent(sleeper: Probe Sleeper)"
        ]));
        assert!(is_task_panel(&[
            "  \u{25cf} [22:53:30] sleep 8 running",
            "  \u{25cf} Agent(sleeper: Probe Sleeper) \u{b7} Gemini 3.1 Pro  Working \u{b7} 1s",
        ]));
    }

    /// A timer's and a cron job's rows in the panel (`● [09:09:16] 15m timer running`,
    /// `● [09:09:16] cron (daily at 03:00) running`) are passed over as a command's are: the box
    /// above them is read, empty and with a draft.
    #[test]
    fn the_box_is_read_above_a_timers_and_a_cron_jobs_rows() {
        for screen in rendered(&[SCHEDULE_PANEL]) {
            assert!(screen.contains("15m timer running"), "{screen}");
            assert!(screen.contains("cron (daily at 03:00) running"), "{screen}");
            assert_eq!(rows_of(&screen), rows(&[""]), "{screen}");
        }
        for screen in rendered(&[SCHEDULE_PANEL_DRAFT]) {
            assert_eq!(rows_of(&screen), rows(&["a draft"]), "{screen}");
        }
    }
}
