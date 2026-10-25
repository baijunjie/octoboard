//! Dialogs an agent raises that only a person can answer, and no hook announces.
//!
//! A message Octoboard writes is a bracketed paste and an Enter, and a modal dialog answers that
//! destructively: the paste is discarded and the Enter picks the focused row. Claude Code's
//! "Resume this conversation?" at a launch with no prompt on the command line loses the message (or
//! starts a new conversation), its "Resume from summary" runs `/compact`, and Codex's and Grok
//! Build's own screens (sign-in, a working-directory chooser, a model migration, a usage limit)
//! take the Enter as well. So do Claude Code's views of its background agents, where what is sent
//! goes to a subagent or starts another session. None runs a hook or sets a terminal title, so the
//! only evidence is the screen. A session showing one holds its writes and reads as waiting for the
//! user ([`crate::state::WriteHold::AgentDialog`]); it is let go the moment the screen shows none.
//!
//! **Recognition** is from the rendered screen (the parsed terminal, visible rows only), never from
//! the byte stream, with all whitespace removed on both sides: Claude Code places words with cursor
//! moves and Codex redraws only the cells that changed, so no phrase is ever in the bytes. A
//! dialog is a set of strings that must all be on the screen at once, a title and its footer where
//! the screen has one, so a transcript that quotes a title does not match; an entry whose screen
//! has no measured footer relies on its title and options together instead, and one whose text a
//! quote or a task's name could contain must be a whole row of the screen. Nothing here sends a
//! key to a dialog. The ages, token counts and model names the screens print vary, so no string
//! uses them.
//!
//! Adding a dialog is one more entry in [`DIALOGS`].

use std::sync::{Arc, Weak};
use std::time::Duration;

use crate::protocol::Agent;
use crate::session::LiveSession;
use crate::state::AppState;

/// How often a session's screen is looked at, when its output has moved on since the last look.
const POLL: Duration = Duration::from_millis(250);

/// What becomes of a message written while a dialog is up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrittenMessage {
    /// The paste goes into the input box and the Enter is swallowed, so the message waits there as
    /// a draft.
    Draft,
    /// The paste is dropped and the Enter answers the dialog: nothing of the message is anywhere.
    Discarded,
    /// The message reaches a view of the agent's background agents (a subagent, or a new
    /// session) in place of the conversation: it is delivered, to the wrong place.
    Diverted,
}

/// A dialog on an agent's screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dialog {
    /// Stable name, reported to clients as `writes_held.dialog`.
    pub code: &'static str,
    /// What becomes of a message written while it is up.
    pub written: WrittenMessage,
}

struct Entry {
    agent: Agent,
    dialog: Dialog,
    /// Whitespace-free strings that must all be on the screen.
    all_of: &'static [&'static str],
    /// A whitespace-free row that must be on the screen as a row of its own, for text that would
    /// also match as part of a longer row (a task named after it, or a sentence quoting it).
    row: Option<&'static str>,
}

const fn entry(
    agent: Agent,
    code: &'static str,
    written: WrittenMessage,
    all_of: &'static [&'static str],
) -> Entry {
    Entry {
        agent,
        dialog: Dialog { code, written },
        all_of,
        row: None,
    }
}

const fn entry_on_row(
    agent: Agent,
    code: &'static str,
    written: WrittenMessage,
    row: &'static str,
) -> Entry {
    Entry {
        agent,
        dialog: Dialog { code, written },
        all_of: &[],
        row: Some(row),
    }
}

const DIALOGS: &[Entry] = &[
    // Shown at a launch that resumes a conversation with no prompt on the command line; a paste is
    // discarded and the Enter picks the focused row.
    entry(
        Agent::Claude,
        "claude_resume_at_launch",
        WrittenMessage::Discarded,
        &["Resumethisconversation?", "Entertoconfirm", "Esctoresume"],
    ),
    // Shown on its own in an idle session once the prompt cache has expired; a paste goes into the
    // input box and the Enter is swallowed, so the message waits there as a draft.
    entry(
        Agent::Claude,
        "claude_resume_idle",
        WrittenMessage::Draft,
        &["Resumethisconversation?", "Entertoconfirm", "Esctodismiss"],
    ),
    entry(
        Agent::Claude,
        "claude_resume_from_summary",
        WrittenMessage::Discarded,
        &["Resumefromsummary", "Resumefullsessionas-is", "Esctocancel"],
    ),
    // A notice that waits for Enter, such as the one 2.1.308 raises mid-turn about auto mode's
    // classifier billing behind a gateway, or the rewind dialog: a paste is dropped and the Enter
    // continues past it.
    entry(
        Agent::Claude,
        "claude_notice",
        WrittenMessage::Discarded,
        &["Entertocontinue\u{B7}Esctocancel"],
    ),
    // Offered once an account's settings name a default mode other than auto, at a launch or once a
    // turn ends: a paste is dropped and the Enter picks the focused Yes, which writes auto into the
    // account's settings. It has no footer, so its Yes option, which does not vary with the mode,
    // stands in for one.
    entry(
        Agent::Claude,
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
    entry_on_row(
        Agent::Claude,
        "claude_agent_list_focus",
        WrittenMessage::Discarded,
        "Entertoview\u{B7}xtostop",
    ),
    entry_on_row(
        Agent::Claude,
        "claude_agent_list_focus",
        WrittenMessage::Discarded,
        "\u{2191}/\u{2193}toselect",
    ),
    // A background subagent's transcript in place of the conversation. Its prompt box stays on
    // screen, but what is sent there goes to the subagent, with no `UserPromptSubmit`. The main
    // row of the task list is filled while the conversation is shown and hollow while a subagent
    // is; 2.1.309 marks it with `❯` when it is selected without leaving the view, and shows no
    // hint then. Leaving is never safe to do for the user: Esc stops the agent, Ctrl+C kills it,
    // and ← opens the agents view below.
    entry_on_row(
        Agent::Claude,
        "claude_subagent_view",
        WrittenMessage::Diverted,
        "\u{25EF}main",
    ),
    entry_on_row(
        Agent::Claude,
        "claude_subagent_view",
        WrittenMessage::Diverted,
        "\u{276F}\u{25EF}main",
    ),
    // The agents view (`claude agents`), which ← at an empty prompt opens after moving the
    // conversation into a background session of its own: a paste and Enter here start another
    // session with that text.
    entry(
        Agent::Claude,
        "claude_agents_view",
        WrittenMessage::Diverted,
        &["Newsessions:", "awaitinginput\u{B7}"],
    ),
    entry(
        Agent::Codex,
        "codex_sign_in",
        WrittenMessage::Discarded,
        &[
            "WelcometoCodex",
            "SigninwithChatGPT",
            "Pressentertocontinue",
        ],
    ),
    entry(
        Agent::Codex,
        "codex_resume_cwd",
        WrittenMessage::Discarded,
        &[
            "Usesessiondirectory",
            "Usecurrentdirectory",
            "escusesession",
        ],
    ),
    entry(
        Agent::Codex,
        "codex_model_migration",
        WrittenMessage::Discarded,
        &["Trynewmodel", "Useexistingmodel", "enter/escconfirm"],
    ),
    entry(
        Agent::Codex,
        "codex_rate_limit_nudge",
        WrittenMessage::Discarded,
        &["Approachingratelimits", "Keepcurrentmodel"],
    ),
    entry(
        Agent::Grok,
        "grok_usage_limit",
        WrittenMessage::Discarded,
        &["Upgradetier", "Buymorecredits", "Enter:submit"],
    ),
];

/// The dialog `agent` is showing on a screen with this text, if any.
pub fn recognise(agent: Agent, screen: &str) -> Option<Dialog> {
    let strip = |text: &str| -> String { text.chars().filter(|c| !c.is_whitespace()).collect() };
    let rows: Vec<String> = screen.lines().map(strip).collect();
    let screen = strip(screen);
    DIALOGS
        .iter()
        .filter(|entry| entry.agent == agent)
        .find(|entry| {
            entry.all_of.iter().all(|part| screen.contains(part))
                && entry
                    .row
                    .is_none_or(|row| rows.iter().any(|candidate| candidate == row))
        })
        .map(|entry| entry.dialog)
}

/// What a session's screen showed at the latest look.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sighting {
    /// The dialog up at the look.
    pub current: Option<Dialog>,
    /// How many times what is up has changed, a dialog coming or going. Whoever acts on a change
    /// compares this with the count it acted at, so a dialog another look saw come and go in
    /// between is not missed.
    pub changes: u64,
    /// How many times a dialog that discards messages has been seen to come up. A write made after
    /// reading this and followed by a larger count had such a dialog come up around it, even if it
    /// is gone again.
    pub discarded_seen: u64,
    /// The same for a dialog that diverts messages to a view of background agents.
    pub diverted_seen: u64,
}

impl Sighting {
    /// The next sighting after `current` was read off the screen.
    pub fn after(self, current: Option<Dialog>) -> Self {
        let appeared = current.filter(|dialog| Some(*dialog) != self.current);
        let seen = |kind| u64::from(appeared.is_some_and(|dialog| dialog.written == kind));
        Self {
            current,
            changes: self.changes + u64::from(current != self.current),
            discarded_seen: self.discarded_seen + seen(WrittenMessage::Discarded),
            diverted_seen: self.diverted_seen + seen(WrittenMessage::Diverted),
        }
    }
}

/// Watches the session's screen for dialogs for as long as the session lives. Publishes the
/// session when one comes or goes, so clients see it waiting for the user, and writes out what was
/// queued when one goes.
pub fn watch(state: &Arc<AppState>, live: &Arc<LiveSession>) {
    let state = state.clone();
    // Weak, so that the session being dropped ends this task.
    let live = Arc::downgrade(live);
    tokio::spawn(async move { poll(state, live).await });
}

async fn poll(state: Arc<AppState>, live: Weak<LiveSession>) {
    let mut output = 0;
    let mut acted_at = 0;
    loop {
        tokio::time::sleep(POLL).await;
        let Some(live) = live.upgrade() else { return };
        let total = live.output_total();
        if total == output {
            continue;
        }
        output = total;
        let now = live.look_for_dialog();
        // Against the count acted at, not the last look's: a dialog another look saw come and go
        // since has changed what clients are shown and what may be written all the same.
        if now.changes == acted_at {
            continue;
        }
        acted_at = now.changes;
        if let Ok(session) = state.session_record(&live.id) {
            state.publish_session(&session);
            if now.current.is_none() {
                state.spawn_flush_outbox(&live.id, state.agent_status(&session));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RESUME_AT_LAUNCH: &str = "\n  Resume this conversation?\n  This conversation has been inactive for 3d 0h 1m and is 42k tokens long. Resuming it will use about 42% of your\n  5-hour usage limit.\n  ❯ 1. Resume\n    2. Start a new conversation  Claude can look this one up if you refer to it later\n\n  Enter to confirm · Esc to resume\n";
    const RESUME_IDLE: &str = "  Resume this conversation?\n  This conversation has been inactive for 1h and is 42k tokens long.\n  ❯ Resume\n    Start a new conversation  Claude can look this one up if you refer to it later\n\n  Enter to confirm · Esc to dismiss\n";
    const RESUME_IDLE_AFTER_PASTE: &str = "  Resume this conversation?\n  ❯ Resume\n    Start a new conversation\n  Your unsent message will still be there after you answer.\n  ↑/↓ to pick a row first · Enter to confirm · Esc to dismiss\n";
    const RESUME_FROM_SUMMARY: &str = "This session is 3d old and 150k tokens.\nResuming the full session will consume a substantial portion of your usage limits. We recommend resuming from a summary.\n❯ 1. Resume from summary (recommended)\n  2. Resume full session as-is\n  3. Don't ask me again\nEnter to confirm · Esc to cancel\n";
    const CODEX_SIGN_IN: &str = "  Welcome to Codex, OpenAI's command-line coding agent\n  > 1. Sign in with ChatGPT\n    2. Sign in with Device Code\n    3. Provide your own API key\n  Press enter to continue\n";
    const CODEX_RESUME_CWD: &str = "Working directory · resume\n› 1. Use session directory (/a/b)\n  2. Use current directory\n  3. Always use session directory\n  4. Always use current directory\nenter continue · esc use session · ⌃c quit\n";
    const CODEX_MODEL_MIGRATION: &str =
        "Meet Codex Next\n› 1. Try new model\n  2. Use existing model\nenter/esc confirm\n";
    const CODEX_RATE_LIMIT: &str = "Approaching rate limits\nSwitch to gpt-mini for lower credit usage?\n› 1. Switch to gpt-mini\n  2. Keep current model\n  3. Keep current model (never show again)\n";
    const GROK_USAGE_LIMIT: &str = "You hit your weekly limit.\n1 (○) Upgrade tier\n2 (○) Buy more credits\n3 (○) Try Again\n↑/↓ navigate · y copy Enter :submit / Shift+x :dismiss\n";
    const CLAUDE_NOTICE: &str = "⏺ Agent(Slow probe agent)\n────────\n  We're changing auto mode to no longer charge for classifier requests in Claude Code.\n  Nothing breaks: auto mode keeps working, and its classifier requests are billed as before.\n  Enter to continue · Esc to cancel\n";
    // Claude Code 2.1.307's agent screens, as measured.
    const SUBAGENT_VIEW: &str = "❯ SUBAGENT-SLOW work slowly\n──── Slow probe agent ─\n❯ Message @general-purpose…\n  ◯ main\n  ⏺ general-purpose  Slow probe agent  8s · ↑ 20 tokens\n";
    const AGENT_ROW_FOCUS: &str =
        "❯ \n  Enter to view · x to stop\n  ⏺ main\n❯ ◯ general-purpose  Slow probe agent\n";
    const MAIN_ROW_FOCUS: &str =
        "❯ \n  ↑/↓ to select\n❯ ⏺ main\n  ◯ general-purpose  Slow probe agent\n";
    // Claude Code 2.1.309's subagent view with the main row selected.
    const SUBAGENT_VIEW_MAIN_SELECTED: &str = "\u{276F} Message @implement\u{2026}\n  \u{25B8}\u{25B8} auto mode on \u{B7} 2 shells\n\u{276F} \u{25EF} main\n  \u{23FA} implement  Updating FileViewer.test.tsx image assertions\n";
    // Inside a subagent's view with the list focused, where the hint shows the focus.
    const VIEW_LIST_FOCUS: &str = "\u{276F} Message @general-purpose\u{2026}\n  \u{2191}/\u{2193} to select\n\u{276F} \u{25EF} main\n  \u{23FA} general-purpose  Slow probe agent\n";
    const VIEW_AGENT_ROW_FOCUS: &str = "\u{276F} Message @general-purpose\u{2026}\n  Enter to view \u{B7} x to stop\n  \u{25EF} main\n\u{276F} \u{23FA} general-purpose  Slow probe agent\n";
    // Claude Code 2.1.310's prompt to make auto mode the default, as drawn under the transcript;
    // it has no footer.
    const AUTO_DEFAULT: &str = "\u{23FA} Auto mode lets Claude handle permission prompts automatically.\n\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n Make auto mode your default permission mode?\n\n  Auto mode lets Claude handle permission prompts automatically. Claude checks each tool call for risky actions and\n  prompt injection before executing, runs the ones it assesses as lower-risk, and blocks the rest.\n\n  \u{276F} Yes, set auto mode as my default permission mode\n    No, keep manual mode\n";
    const AUTO_DEFAULT_NO_FOCUSED: &str = " Make auto mode your default permission mode?\n\n  Auto mode lets Claude handle permission prompts automatically.\n\n    Yes, set auto mode as my default permission mode\n  \u{276F} No, keep accept edits\n";
    const AGENTS_VIEW: &str = "New sessions: Opus 5.5 · medium effort\n0 awaiting input · 1 working · 0 completed\nYour conversation moved to the background — enter opens it · esc returns to it\n❯ describe a task for a new session\n  enter to return · space to reply · ctrl+x to delete · ? for shortcuts\n";

    #[test]
    fn every_dialog_is_recognised_for_its_own_agent_only() {
        for (agent, screen, code) in [
            (Agent::Claude, RESUME_AT_LAUNCH, "claude_resume_at_launch"),
            (Agent::Claude, RESUME_IDLE, "claude_resume_idle"),
            (Agent::Claude, RESUME_IDLE_AFTER_PASTE, "claude_resume_idle"),
            (
                Agent::Claude,
                RESUME_FROM_SUMMARY,
                "claude_resume_from_summary",
            ),
            (Agent::Codex, CODEX_SIGN_IN, "codex_sign_in"),
            (Agent::Codex, CODEX_RESUME_CWD, "codex_resume_cwd"),
            (Agent::Codex, CODEX_MODEL_MIGRATION, "codex_model_migration"),
            (Agent::Codex, CODEX_RATE_LIMIT, "codex_rate_limit_nudge"),
            (Agent::Grok, GROK_USAGE_LIMIT, "grok_usage_limit"),
            (Agent::Claude, SUBAGENT_VIEW, "claude_subagent_view"),
            (
                Agent::Claude,
                SUBAGENT_VIEW_MAIN_SELECTED,
                "claude_subagent_view",
            ),
            (Agent::Claude, AGENT_ROW_FOCUS, "claude_agent_list_focus"),
            (Agent::Claude, MAIN_ROW_FOCUS, "claude_agent_list_focus"),
            (Agent::Claude, VIEW_LIST_FOCUS, "claude_agent_list_focus"),
            (
                Agent::Claude,
                VIEW_AGENT_ROW_FOCUS,
                "claude_agent_list_focus",
            ),
            (Agent::Claude, AGENTS_VIEW, "claude_agents_view"),
            (Agent::Claude, CLAUDE_NOTICE, "claude_notice"),
            (Agent::Claude, AUTO_DEFAULT, "claude_auto_mode_default"),
            (
                Agent::Claude,
                AUTO_DEFAULT_NO_FOCUSED,
                "claude_auto_mode_default",
            ),
        ] {
            assert_eq!(recognise(agent, screen).map(|d| d.code), Some(code));
            for other in [Agent::Claude, Agent::Codex, Agent::Grok] {
                if other != agent {
                    assert_eq!(recognise(other, screen), None, "{code} for {other:?}");
                }
            }
        }
    }

    #[test]
    fn each_dialog_says_what_becomes_of_a_message_written_into_it() {
        let written = |screen| recognise(Agent::Claude, screen).unwrap().written;
        assert_eq!(written(RESUME_AT_LAUNCH), WrittenMessage::Discarded);
        assert_eq!(written(RESUME_FROM_SUMMARY), WrittenMessage::Discarded);
        assert_eq!(written(AGENT_ROW_FOCUS), WrittenMessage::Discarded);
        assert_eq!(written(RESUME_IDLE), WrittenMessage::Draft);
        assert_eq!(written(SUBAGENT_VIEW), WrittenMessage::Diverted);
        assert_eq!(
            written(SUBAGENT_VIEW_MAIN_SELECTED),
            WrittenMessage::Diverted
        );
        assert_eq!(written(VIEW_LIST_FOCUS), WrittenMessage::Discarded);
        assert_eq!(written(AGENTS_VIEW), WrittenMessage::Diverted);
        assert_eq!(written(AUTO_DEFAULT), WrittenMessage::Discarded);
    }

    #[test]
    fn words_split_by_cursor_moves_and_wrapping_still_match() {
        let screen = "R e s u m e\nthis   conver\nsation ?\nEnter  to\nconfirm·Esc to resume";
        assert!(recognise(Agent::Claude, screen).is_some());
    }

    #[test]
    fn near_misses_are_not_dialogs() {
        for screen in [
            // A transcript quoting the title, with no footer of the dialog.
            "> what does \"Resume this conversation?\" mean\nIt asks whether to resume.",
            // The title and one footer word, but not the dialog's footer.
            "Resume this conversation?\nEnter to confirm",
            // The banner printed on exit.
            "Resume this session with:\n  claude --resume 0f3c\n",
            // The conversation with a background agent running and the prompt focused.
            "\u{276F} \n  ? for shortcuts \u{B7} \u{2193} to manage\n  \u{23FA} main\n  \u{25EF} \
             general-purpose  Slow probe agent        8s \u{B7} \u{2191} 20 tokens\n",
            // A hollow row of a task whose name only begins with `main`, in the ordinary task list.
            "\u{276F} \n  \u{23FA} main\n  \u{25EF} maintenance-bot  Nightly sweep        8s\n",
            "\u{276F} \n  \u{23FA} main\n  \u{25EF} main-loop  Poll the queue        8s\n",
            // A transcript quoting the agent list's hints in a sentence.
            "\u{25CF} matches \u{300C}\u{2191}/\u{2193} to select\u{300D} and \u{300C}Enter to view \u{B7} x to stop\u{300D}\n",
            // A transcript quoting the auto mode prompt's title, without its option.
            "> why did it ask \"Make auto mode your default permission mode?\"\n",
            "",
        ] {
            assert_eq!(recognise(Agent::Claude, screen), None, "{screen}");
        }
    }

    #[test]
    fn a_dialog_is_counted_by_its_kind_each_time_it_comes_up() {
        let launch = recognise(Agent::Claude, RESUME_AT_LAUNCH);
        let idle = recognise(Agent::Claude, RESUME_IDLE);
        let view = recognise(Agent::Claude, SUBAGENT_VIEW);
        let sighting = Sighting::default();
        let up = sighting.after(launch);
        assert_eq!(up.discarded_seen, 1);
        assert_eq!(up.after(launch).discarded_seen, 1);
        assert_eq!(up.after(None).after(launch).discarded_seen, 2);
        assert_eq!(sighting.after(idle).discarded_seen, 0);
        assert_eq!(up.diverted_seen, 0);
        assert_eq!(sighting.after(view).diverted_seen, 1);
        assert_eq!(sighting.after(view).discarded_seen, 0);
    }

    #[test]
    fn every_dialog_coming_or_going_is_a_change() {
        let idle = recognise(Agent::Claude, RESUME_IDLE);
        let launch = recognise(Agent::Claude, RESUME_AT_LAUNCH);
        let up = Sighting::default().after(idle);
        assert_eq!(up.changes, 1);
        assert_eq!(up.after(idle).changes, 1);
        assert_eq!(up.after(launch).changes, 2);
        assert_eq!(up.after(None).changes, 2);
        assert_eq!(Sighting::default().after(None).changes, 0);
    }

    /// Screens as Codex 0.162.1 draws them: padded rows, a path wrapped inside an option, a
    /// footer, and the nudge drawn under the conversation it interrupts.
    #[test]
    fn codex_screens_as_drawn_are_recognised_and_an_ordinary_screen_is_not() {
        let cwd = "  Working directory \u{b7} resume                  \n  Session = latest cwd recorded in the resumed session\n\u{203a} 1. Use session directory (/tmp/a/b/c/d-\n     e/work-codex)\n  2. Use current directory (/tmp/a/b/c/d-\n     e/work-codex2)\n  3. Always use session directory\n  4. Always use current directory\n  enter continue \u{b7} esc use session \u{b7} \u{2303}c quit\n";
        let sign_in = "  Welcome to Codex, OpenAI's command-line coding agent\n> 1. Sign in with ChatGPT\n     Usage included with Plus, Pro, Business, and Enterprise plans\n  2. Sign in with Device Code\n  3. Provide your own API key\n  Press enter to continue\n";
        let migration = "  Meet GPT-6 Sol\n  Our latest Sol is more intelligent and more efficient so your usage limits go further.\n\u{203a} 1. Try new model\n  2. Use existing model\n  enter/esc confirm \u{b7} \u{2303}c quit\n";
        let nudge = "\u{203a} first\n\u{2022} stub reply\n  Approaching rate limits\n  Switch to gpt-6-luna for lower credit usage?\n\u{203a} 1. Switch to gpt-6-luna      Fast and affordable model for easier tasks.\n  2. Keep current model\n  3. Keep current model (never\n";
        let session = ">_ OpenAI Codex (v0.162.1)\n\u{203a} first\n\u{2022} stub reply\n  Worked for <1s\n  ? for shortcuts\n";
        for (screen, code) in [
            (cwd, Some("codex_resume_cwd")),
            (sign_in, Some("codex_sign_in")),
            (migration, Some("codex_model_migration")),
            (nudge, Some("codex_rate_limit_nudge")),
            (session, None),
        ] {
            assert_eq!(recognise(Agent::Codex, screen).map(|d| d.code), code);
        }
    }
}
