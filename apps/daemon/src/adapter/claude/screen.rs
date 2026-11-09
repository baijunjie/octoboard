//! How Claude Code's screens read: its dialogs, its input box between two horizontal rules and its
//! draft stash, its folder-trust confirmation, the glyphs its terminal title starts with, and its
//! interrupt key. The measurements are in `docs/agent-cli-reference.md`, section "## Claude Code".

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

    fn stash_shown(&self, screen: &str) -> bool {
        screen
            .lines()
            .rev()
            .take(6)
            .any(|line| line.contains("› stashed"))
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

/// The rows of Claude Code's input box: from the last one starting with `❯` right below a
/// horizontal rule up to the next rule.
fn input_box_rows(screen: &str) -> Option<Vec<String>> {
    let rows: Vec<&str> = screen.lines().map(str::trim_end).collect();
    let start = (1..rows.len())
        .rev()
        .find(|&row| rows[row].starts_with(PROMPT_MARK) && is_rule(rows[row - 1]))?;
    let end = start + 1 + rows[start + 1..].iter().position(|row| is_rule(row))?;
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
