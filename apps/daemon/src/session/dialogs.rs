//! Dialogs an agent raises that only a person can answer, and no hook announces.
//!
//! A message Octoboard writes is a bracketed paste and an Enter, and a modal dialog answers that
//! destructively: the paste is discarded and the Enter picks the focused row. Claude Code's
//! "Resume this conversation?" at a launch with no prompt on the command line loses the message (or
//! starts a new conversation), its "Resume from summary" runs `/compact`, and Codex's and Grok
//! Build's own screens (sign-in, a working-directory chooser, a model migration, a usage limit)
//! take the Enter as well. So do Claude Code's views of its background agents, where what is sent
//! goes to a subagent or starts another session. None runs a hook or sets a terminal title that
//! says the user is wanted (one raised mid-turn leaves Claude Code's title back at its prompt, as a
//! cancel would), so the only evidence is the screen. A session showing one holds its writes and
//! reads as waiting for the user ([`crate::state::WriteHold::AgentDialog`]); it is let go the
//! moment the screen shows none.
//!
//! **Recognition** is from the rendered screen (the parsed terminal, visible rows only), never from
//! the byte stream, with all whitespace removed on both sides: Claude Code places words with cursor
//! moves and Codex redraws only the cells that changed, so no phrase is ever in the bytes. A
//! dialog is a set of strings that must all be on the screen at once, a title and its footer where
//! the screen has one, so a transcript that quotes a title does not match; an entry whose screen
//! has no measured footer relies on its title and options together instead, and one whose text a
//! quote or a task's name could contain must be a whole row of the screen. A dialog drawn over
//! the middle of the screen, with the conversation's text left on its rows to either side, has no
//! row of its own: its title row is asked for as strings in order on one row with nothing but a
//! run of blanks between them, judged on the row as drawn, where the blanks still tell a title
//! bar from a sentence. Nothing here sends a key to a dialog. The ages, token counts and model names the screens print vary, so no string
//! uses them.
//!
//! Each provider's dialogs are its own, listed by its screen reader
//! ([`crate::session::screen::ScreenReader::dialogs`]); adding one is one more entry there.

use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use crate::protocol::AgentId;
use crate::session::text::bare;
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

/// What a dialog is to the session showing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKind {
    /// Something put to the person, which the session waits on them for.
    Question,
    /// The dialog the agent raises on reaching its usage limit. A session whose only wait is this
    /// one is switched to another account all the same: ending the modal is the point of the
    /// switch (see `crate::coordinator::auto_switch`).
    UsageLimit,
    /// A view of the agent's background agents. The person is looking at the agents, not at a
    /// question put to them, so a session showing one reads as dormant rather than waiting for
    /// the user, though it holds its writes all the same.
    AgentsView,
    /// A view the person opened themselves over the conversation: a list of commands, a picker,
    /// a help screen. Nothing is put to them and nothing of the agent's waits on it, so the
    /// session reads as it did before the view came up and nobody is told it waits; it holds its
    /// writes all the same, since what is written would go to the view.
    OwnView,
}

/// A dialog on an agent's screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dialog {
    /// Stable name, reported to clients as `writes_held.dialog`.
    pub code: &'static str,
    /// What becomes of a message written while it is up.
    pub written: WrittenMessage,
    pub kind: DialogKind,
}

impl Dialog {
    /// Whether this is the dialog an agent raises on reaching its usage limit
    /// ([`DialogKind::UsageLimit`]).
    pub fn is_usage_limit(&self) -> bool {
        self.kind == DialogKind::UsageLimit
    }

    /// Whether this is a view of the agent's background agents ([`DialogKind::AgentsView`]).
    pub fn is_agents_view(&self) -> bool {
        self.kind == DialogKind::AgentsView
    }

    /// Whether this is a view the person opened themselves ([`DialogKind::OwnView`]).
    pub fn is_own_view(&self) -> bool {
        self.kind == DialogKind::OwnView
    }
}

/// Strings asked for in order on one row of the screen as drawn ([`DialogPattern::with_row_run`]).
#[derive(Debug, Clone, Copy)]
struct RowRun {
    /// The strings as drawn, their own blanks included.
    parts: &'static [&'static str],
    /// How many blank cells at least stand between one string and the next.
    min_gap: usize,
}

impl RowRun {
    /// Whether `row`, a row of the screen as drawn, holds the strings in order with nothing but
    /// blanks between one and the next, at least [`Self::min_gap`] of them. Whatever stands on
    /// the row before the first and after the last is not looked at.
    fn on(&self, row: &str) -> bool {
        let Some((first, rest)) = self.parts.split_first() else {
            return false;
        };
        row.match_indices(*first).any(|(at, _)| {
            let mut after = &row[at + first.len()..];
            rest.iter().all(|part| {
                let text = after.trim_start_matches(char::is_whitespace);
                let gap = after[..after.len() - text.len()].chars().count();
                match text.strip_prefix(*part) {
                    Some(left) if gap >= self.min_gap.max(1) => {
                        after = left;
                        true
                    }
                    _ => false,
                }
            })
        })
    }
}

/// How one dialog is recognised on a provider's screen: an entry of the provider's
/// `crate::session::screen::ScreenReader::dialogs`.
pub struct DialogPattern {
    dialog: Dialog,
    /// Whitespace-free strings that must all be on the screen.
    all_of: &'static [&'static str],
    /// A whitespace-free row that must be on the screen as a row of its own, for text that would
    /// also match as part of a longer row (a task named after it, or a sentence quoting it).
    row: Option<&'static str>,
    /// Strings that must stand in order on one row of the screen as drawn, with only blanks
    /// between them.
    row_run: Option<RowRun>,
}

impl DialogPattern {
    /// A [`DialogKind::Question`] recognised by whitespace-free strings that must all be on the
    /// screen.
    pub const fn all_of(
        code: &'static str,
        written: WrittenMessage,
        all_of: &'static [&'static str],
    ) -> Self {
        Self {
            dialog: Dialog {
                code,
                written,
                kind: DialogKind::Question,
            },
            all_of,
            row: None,
            row_run: None,
        }
    }

    /// A [`DialogKind::Question`] recognised by a whitespace-free row of its own.
    pub const fn on_row(code: &'static str, written: WrittenMessage, row: &'static str) -> Self {
        Self {
            dialog: Dialog {
                code,
                written,
                kind: DialogKind::Question,
            },
            all_of: &[],
            row: Some(row),
            row_run: None,
        }
    }

    /// The same dialog, also requiring `row`, whitespace-free, as a row of its own.
    pub const fn with_row(mut self, row: &'static str) -> Self {
        self.row = Some(row);
        self
    }

    /// The same dialog, also requiring `parts` in order on one row of the screen as drawn, with
    /// nothing but blanks between one and the next, at least `min_gap` cells of them (one at
    /// least, whatever is given): the title row of a dialog drawn over the middle of the screen,
    /// which the conversation's text shares to the left and right, so that no whole row is the
    /// dialog's. Each part is given as it is drawn, with its own blanks, since the row is judged
    /// before whitespace is removed: the run of blanks is what tells a title and the key at the
    /// far end of its bar from a sentence that spells the two together. `min_gap` is the
    /// narrowest run measured on the provider's screens.
    pub const fn with_row_run(mut self, parts: &'static [&'static str], min_gap: usize) -> Self {
        self.row_run = Some(RowRun { parts, min_gap });
        self
    }

    /// The same dialog, of `kind`.
    pub const fn of_kind(mut self, kind: DialogKind) -> Self {
        self.dialog.kind = kind;
        self
    }
}

/// The dialog `agent` is showing on a screen with this text, if any.
pub fn recognise(agent: AgentId, screen: &str) -> Option<Dialog> {
    let drawn: Vec<&str> = screen.lines().collect();
    let rows: Vec<String> = drawn.iter().map(|row| bare(row)).collect();
    let screen = bare(screen);
    crate::adapter::provider(agent)
        .screen()
        .dialogs()
        .iter()
        .find(|pattern| {
            pattern.all_of.iter().all(|part| screen.contains(part))
                && pattern
                    .row
                    .is_none_or(|row| rows.iter().any(|candidate| candidate == row))
                && pattern
                    .row_run
                    .is_none_or(|run| drawn.iter().any(|row| run.on(row)))
        })
        .map(|pattern| pattern.dialog)
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
    /// When the screen was last seen with no dialog after one, the dialog gone. A title back at
    /// the prompt while a dialog was up counts as settled only from here (see
    /// `crate::session::title_interrupt`): an agent that raised the dialog mid-turn can take a
    /// moment after the answer to show its turn running again.
    pub gone_at: Option<Instant>,
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
            gone_at: if current.is_none() && self.current.is_some() {
                Some(Instant::now())
            } else {
                self.gone_at
            },
        }
    }
}

/// Watches the session's screen for dialogs for as long as the session lives. Publishes the
/// session when one comes or goes, so clients see it waiting for the user, and writes out what was
/// queued when one goes, or when the input box is drawn empty after keys that held it
/// (`LiveSession::schedule_draft_release`).
pub fn watch(state: &Arc<AppState>, live: &Arc<LiveSession>) {
    let state = state.clone();
    // Weak, so that the session being dropped ends this task.
    let live = Arc::downgrade(live);
    tokio::spawn(async move { poll(state, live).await });
}

/// Whether `dialog` is the session waiting for the user, which its owner is told of. A usage-limit
/// dialog is not: the session stopped on the limit, and neither a permission nor a question is
/// open for the user to answer. Nor is a view of the background agents, which the session is
/// shown at dormant for, nor a view the person opened themselves, which puts nothing to them.
fn is_a_wait_for_the_user(dialog: &Dialog) -> bool {
    !dialog.is_usage_limit() && !dialog.is_agents_view() && !dialog.is_own_view()
}

async fn poll(state: Arc<AppState>, live: Weak<LiveSession>) {
    let mut output = 0;
    let mut acted_at = 0;
    let mut retry_at = std::time::Instant::now();
    loop {
        tokio::time::sleep(POLL).await;
        let Some(live) = live.upgrade() else { return };
        if retry_at.elapsed() >= Duration::from_secs(2) {
            live.schedule_input_recovery();
            live.schedule_unsent_submit_retry();
            state.retry_pending_outbox(&live.id);
            retry_at = std::time::Instant::now();
        }
        // A message held for what the user typed goes in once the input box is drawn empty, not
        // at the next retry, which can be past the draft's grace and then no longer checks the box.
        if state.outbox.takeable(&live.id) > 0 {
            live.schedule_draft_release();
        }
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
        react_to_dialog(&state, &live.id, now.current);
    }
}

/// What clients and the owner are told once the screen of the session `id` shows `current` after
/// showing something else: the session is published, and its owner is told it waits for the user,
/// or what was queued is flushed when the dialog is gone.
pub(crate) fn react_to_dialog(state: &Arc<AppState>, id: &str, current: Option<Dialog>) {
    if let Ok(session) = state.session_record(id) {
        state.publish_session(&session);
        match current {
            None => state.spawn_flush_outbox(id, state.agent_status(&session)),
            Some(dialog) if is_a_wait_for_the_user(&dialog) => {
                crate::messaging::owner_notices::spawn_tell_owner_it_waits(state, id)
            }
            Some(_) => {}
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
    // Codex 0.167.0's workspace usage prompt as a user saw it, and the two forms its binary holds.
    const CODEX_CREDIT_LIMIT: &str = "You've reached your workspace credit limit\nYour workspace is out of credits. Ask your workspace owner to add more. Your included usage resets at 10:09 on 14 Nov.\n\u{203a} 1. Request credits\nPress a number to choose\n";
    const CODEX_CREDIT_LIMIT_NOTIFY: &str = "  You've reached your workspace credit limit\n  Your workspace is out of credits. Ask your workspace owner to add more. Notify owner?\n\u{203a} 1. Yes\n  2. No (default)\n";
    const CODEX_LIMIT_INCREASE: &str = "  Usage limit reached\n  Request a limit increase from your owner to continue using codex. Request increase?\n\u{203a} 1. Yes\n  2. No (default)\n";
    const GROK_USAGE_LIMIT: &str = "You hit your weekly limit.\n1 (○) Upgrade tier\n2 (○) Buy more credits\n3 (○) Try Again\n↑/↓ navigate · y copy Enter :submit / Shift+x :dismiss\n";
    // Grok Build 1.0.77's layout as measured, with the dismiss key in the bottom bar rather than
    // the modal; and the same screen with the focus on the scrollback, where that bar changes.
    const GROK_USAGE_LIMIT_1077: &str = "  \u{2503}  You hit your weekly limit.\n  \u{2503}\n  \u{2503}  1 (\u{25CB}) Upgrade tier      Upgrade to a higher tier for more usage\n  \u{2503}  2 (\u{25CB}) Buy more credits  Purchase credits to keep using Grok Build\n  \u{2503}  3 (\u{25CB}) Try Again         Resubmit the last prompt once you have usage again\n  \u{2503}\n  \u{2503}  \u{2191}/\u{2193} navigate \u{B7} y copy                    Enter:submit\n\n  Tab:next answer  \u{2502}  Esc:scrollback  \u{2502}  Shift+x:dismiss\n";
    const GROK_USAGE_LIMIT_1077_SCROLLBACK: &str = "  \u{2503}  You hit your weekly limit.\n  \u{2503}\n  \u{2503}  1 (\u{25CB}) Upgrade tier      Upgrade to a higher tier for more usage\n  \u{2503}  2 (\u{25CB}) Buy more credits  Purchase credits to keep using Grok Build\n  \u{2503}  3 (\u{25CB}) Try Again         Resubmit the last prompt once you have usage again\n  \u{2503}\n  \u{2503}  \u{2191}/\u{2193} navigate \u{B7} y copy                    Enter:submit\n\n  Tab/Space:question  \u{2502}  Ctrl+e:collapse thinking  \u{2502}  Ctrl+x:shortcuts\n";
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
    // Claude Code's computer-use access request, drawn as in a user's screenshot (strings read from
    // the 2.1.325 executable), with the focus moved to Allow.
    const COMPUTER_USE_ACCESS: &str = "\u{25CF} Calling computer-use \u{B7} 8h 24m 2s\u{2026}\n\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n Computer Use wants to control these apps\n\n  \u{2502} Drive the isolated verification build to connect to a test Linux daemon over SSH.\n\n  \u{25C9} Octoboard Verification\n\n 2 other apps will be hidden while Claude works.\n\n  Deny, and tell Claude what to do differently (esc)\n\u{276F} Allow for this session (1 app)\n\n Enter to confirm \u{B7} Esc to cancel\n";
    // Its macOS permissions screen, laid out as the access request is, with strings read from the
    // 2.1.325 executable.
    const COMPUTER_USE_PERMISSIONS: &str = " Computer Use needs macOS permissions\n\n  Accessibility: not granted\n  Screen Recording: granted\n\n  Grant the missing permissions in System Settings, then select \"Try again\".\n\n\u{276F} Open System Settings \u{2192} Accessibility\n  Try again\n\n Enter to confirm \u{B7} Esc to cancel\n";
    // The same request as 2.1.329 draws it, for the one app Simulator, and the titles the request has
    // from 2.1.328 for several apps and for none.
    const COMPUTER_USE_ACCESS_ONE_APP: &str = "\u{25CF} Calling computer-use \u{B7} 3m 55s\u{2026}\n\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n Claude wants to use Simulator\n\n  \u{2502} Drive the Octoboard iOS app in my own simulator to verify the new terminal view.\n\n  \u{25CF} Simulator (full access)\n\n 2 other apps will be hidden while Claude works.\n With full access, Claude can change or delete files on your computer without asking first.\n\n \u{276F} Deny, and tell Claude what to do differently (esc)\n   Allow for this session (1 app)\n\n Enter to confirm \u{B7} Esc to cancel\n";
    const COMPUTER_USE_ACCESS_SEVERAL_APPS: &str = "\u{25CF} Calling computer-use \u{B7} 12s\u{2026}\n\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n Claude wants to use these apps\n\n  \u{25CF} Simulator\n  \u{25CF} Xcode\n\n \u{276F} Deny, and tell Claude what to do differently (esc)\n   Allow for this session (2 apps)\n\n Enter to confirm \u{B7} Esc to cancel\n";
    const COMPUTER_USE_ACCESS_NO_APP: &str = "\u{25CF} Calling computer-use \u{B7} 12s\u{2026}\n\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n Claude wants these permissions\n\n  \u{25CF} Clipboard read\n\n \u{276F} Deny, and tell Claude what to do differently (esc)\n   Allow for this session\n\n Enter to confirm \u{B7} Esc to cancel\n";
    // Its macOS permissions screen as 2.1.329 titles it, with a lowercase "use".
    const COMPUTER_USE_PERMISSIONS_LOWERCASE: &str = " Computer use needs macOS permissions\n\n  Accessibility: not granted\n  Screen Recording: granted\n\n \u{276F} Open System Settings \u{2192} Accessibility\n   Try again\n\n Enter to confirm \u{B7} Esc to cancel\n";
    const AGENTS_VIEW: &str = "New sessions: Opus 5.5 · medium effort\n0 awaiting input · 1 working · 0 completed\nYour conversation moved to the background — enter opens it · esc returns to it\n❯ describe a task for a new session\n  enter to return · space to reply · ctrl+x to delete · ? for shortcuts\n";

    #[test]
    fn every_dialog_is_recognised_for_its_own_agent_only() {
        for (agent, screen, code) in [
            (AgentId::CLAUDE, RESUME_AT_LAUNCH, "claude_resume_at_launch"),
            (AgentId::CLAUDE, RESUME_IDLE, "claude_resume_idle"),
            (
                AgentId::CLAUDE,
                RESUME_IDLE_AFTER_PASTE,
                "claude_resume_idle",
            ),
            (
                AgentId::CLAUDE,
                RESUME_FROM_SUMMARY,
                "claude_resume_from_summary",
            ),
            (AgentId::CODEX, CODEX_SIGN_IN, "codex_sign_in"),
            (AgentId::CODEX, CODEX_RESUME_CWD, "codex_resume_cwd"),
            (
                AgentId::CODEX,
                CODEX_MODEL_MIGRATION,
                "codex_model_migration",
            ),
            (AgentId::CODEX, CODEX_RATE_LIMIT, "codex_rate_limit_nudge"),
            (AgentId::CODEX, CODEX_CREDIT_LIMIT, "codex_usage_limit"),
            (
                AgentId::CODEX,
                CODEX_CREDIT_LIMIT_NOTIFY,
                "codex_usage_limit",
            ),
            (AgentId::CODEX, CODEX_LIMIT_INCREASE, "codex_usage_limit"),
            (AgentId::GROK, GROK_USAGE_LIMIT, "grok_usage_limit"),
            (AgentId::GROK, GROK_USAGE_LIMIT_1077, "grok_usage_limit"),
            (
                AgentId::GROK,
                GROK_USAGE_LIMIT_1077_SCROLLBACK,
                "grok_usage_limit",
            ),
            (AgentId::CLAUDE, SUBAGENT_VIEW, "claude_subagent_view"),
            (
                AgentId::CLAUDE,
                SUBAGENT_VIEW_MAIN_SELECTED,
                "claude_subagent_view",
            ),
            (AgentId::CLAUDE, AGENT_ROW_FOCUS, "claude_agent_list_focus"),
            (AgentId::CLAUDE, MAIN_ROW_FOCUS, "claude_agent_list_focus"),
            (AgentId::CLAUDE, VIEW_LIST_FOCUS, "claude_agent_list_focus"),
            (
                AgentId::CLAUDE,
                VIEW_AGENT_ROW_FOCUS,
                "claude_agent_list_focus",
            ),
            (AgentId::CLAUDE, AGENTS_VIEW, "claude_agents_view"),
            (AgentId::CLAUDE, CLAUDE_NOTICE, "claude_notice"),
            (
                AgentId::CLAUDE,
                COMPUTER_USE_ACCESS,
                "claude_computer_use_access",
            ),
            (
                AgentId::CLAUDE,
                COMPUTER_USE_PERMISSIONS,
                "claude_computer_use_permissions",
            ),
            (
                AgentId::CLAUDE,
                COMPUTER_USE_ACCESS_ONE_APP,
                "claude_computer_use_access",
            ),
            (
                AgentId::CLAUDE,
                COMPUTER_USE_ACCESS_SEVERAL_APPS,
                "claude_computer_use_access",
            ),
            (
                AgentId::CLAUDE,
                COMPUTER_USE_ACCESS_NO_APP,
                "claude_computer_use_access",
            ),
            (
                AgentId::CLAUDE,
                COMPUTER_USE_PERMISSIONS_LOWERCASE,
                "claude_computer_use_permissions",
            ),
            (AgentId::CLAUDE, AUTO_DEFAULT, "claude_auto_mode_default"),
            (
                AgentId::CLAUDE,
                AUTO_DEFAULT_NO_FOCUSED,
                "claude_auto_mode_default",
            ),
        ] {
            assert_eq!(recognise(agent, screen).map(|d| d.code), Some(code));
            for other in [AgentId::CLAUDE, AgentId::CODEX, AgentId::GROK] {
                if other != agent {
                    assert_eq!(recognise(other, screen), None, "{code} for {other:?}");
                }
            }
        }
    }

    #[test]
    fn each_dialog_says_what_becomes_of_a_message_written_into_it() {
        let written = |screen| recognise(AgentId::CLAUDE, screen).unwrap().written;
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
        assert_eq!(written(COMPUTER_USE_ACCESS), WrittenMessage::Discarded);
        assert_eq!(written(COMPUTER_USE_PERMISSIONS), WrittenMessage::Discarded);
        for screen in [
            COMPUTER_USE_ACCESS_ONE_APP,
            COMPUTER_USE_ACCESS_SEVERAL_APPS,
            COMPUTER_USE_ACCESS_NO_APP,
            COMPUTER_USE_PERMISSIONS_LOWERCASE,
        ] {
            assert_eq!(written(screen), WrittenMessage::Discarded, "{screen}");
        }
    }

    #[test]
    fn a_sighting_notes_when_a_dialog_goes_away_and_keeps_it_otherwise() {
        let launch = recognise(AgentId::CLAUDE, RESUME_AT_LAUNCH);
        let idle = recognise(AgentId::CLAUDE, RESUME_IDLE);
        let nothing = Sighting::default();
        assert_eq!(nothing.after(None).gone_at, None);
        assert_eq!(nothing.after(launch).gone_at, None);
        let gone = nothing.after(launch).after(None);
        let at = gone.gone_at.expect("the dialog going is noted");
        assert_eq!(gone.after(None).gone_at, Some(at));
        assert_eq!(gone.after(launch).gone_at, Some(at));
        assert_eq!(gone.after(launch).after(idle).gone_at, Some(at));
    }

    #[test]
    fn words_split_by_cursor_moves_and_wrapping_still_match() {
        let screen = "R e s u m e\nthis   conver\nsation ?\nEnter  to\nconfirm·Esc to resume";
        assert!(recognise(AgentId::CLAUDE, screen).is_some());
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
            // A transcript quoting the computer-use access request's title, without its footer.
            "> it said \"Computer Use wants to control these apps\"\n",
            // A transcript row quoting both the title and the footer.
            "\u{25CF} \"Computer Use wants to control these apps\" \u{2026} \"Enter to confirm \u{B7} Esc to cancel\"\n",
            // The same for the titles of 2.1.328 and later, and the lowercase permissions title.
            "> it said \"Claude wants to use Simulator\"\n",
            "> it said \"Claude wants these permissions\"\n",
            "> it said \"Computer use needs macOS permissions\"\n",
            "\u{25CF} \"Claude wants to use Simulator\" \u{2026} \"Enter to confirm \u{B7} Esc to cancel\"\n",
            "\u{25CF} \"Claude wants to use these apps\" \u{2026} \"Enter to confirm \u{B7} Esc to cancel\"\n",
            "\u{25CF} \"Claude wants these permissions\" \u{2026} \"Enter to confirm \u{B7} Esc to cancel\"\n",
            "\u{25CF} \"Computer use needs macOS permissions\" \u{2026} \"Enter to confirm \u{B7} Esc to cancel\"\n",
            // The footer alone, with none of the titles.
            " Enter to confirm \u{B7} Esc to cancel\n",
            "",
        ] {
            assert_eq!(recognise(AgentId::CLAUDE, screen), None, "{screen}");
        }
    }

    #[test]
    fn a_dialog_is_counted_by_its_kind_each_time_it_comes_up() {
        let launch = recognise(AgentId::CLAUDE, RESUME_AT_LAUNCH);
        let idle = recognise(AgentId::CLAUDE, RESUME_IDLE);
        let view = recognise(AgentId::CLAUDE, SUBAGENT_VIEW);
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
        let idle = recognise(AgentId::CLAUDE, RESUME_IDLE);
        let launch = recognise(AgentId::CLAUDE, RESUME_AT_LAUNCH);
        let up = Sighting::default().after(idle);
        assert_eq!(up.changes, 1);
        assert_eq!(up.after(idle).changes, 1);
        assert_eq!(up.after(launch).changes, 2);
        assert_eq!(up.after(None).changes, 2);
        assert_eq!(Sighting::default().after(None).changes, 0);
    }

    /// A usage-limit dialog and a view of the background agents are no wait for the user, so the
    /// owner is not told the session waits.
    #[test]
    fn only_a_dialog_that_is_neither_the_usage_limit_nor_an_agents_view_is_a_wait_for_the_user() {
        let limit = recognise(AgentId::GROK, GROK_USAGE_LIMIT).unwrap();
        let resume = recognise(AgentId::CLAUDE, RESUME_IDLE).unwrap();
        assert!(!is_a_wait_for_the_user(&limit));
        for screen in [
            CODEX_CREDIT_LIMIT,
            CODEX_CREDIT_LIMIT_NOTIFY,
            CODEX_LIMIT_INCREASE,
        ] {
            let limit = recognise(AgentId::CODEX, screen).unwrap();
            assert!(limit.is_usage_limit(), "{screen}");
            assert!(!is_a_wait_for_the_user(&limit), "{screen}");
            assert_eq!(limit.written, WrittenMessage::Discarded);
        }
        assert!(is_a_wait_for_the_user(&resume));
        for screen in [SUBAGENT_VIEW, AGENTS_VIEW, AGENT_ROW_FOCUS] {
            let view = recognise(AgentId::CLAUDE, screen).unwrap();
            assert!(!is_a_wait_for_the_user(&view), "{}", view.code);
        }
    }

    /// Exactly the three views of the background agents are agents views; every other recognised
    /// screen stays a wait for the user.
    #[test]
    fn only_the_three_views_of_the_background_agents_are_agents_views() {
        for (agent, screen, view) in [
            (AgentId::CLAUDE, SUBAGENT_VIEW, true),
            (AgentId::CLAUDE, AGENTS_VIEW, true),
            (AgentId::CLAUDE, AGENT_ROW_FOCUS, true),
            (AgentId::CLAUDE, RESUME_IDLE, false),
            (AgentId::CLAUDE, RESUME_AT_LAUNCH, false),
            (AgentId::CLAUDE, RESUME_FROM_SUMMARY, false),
            (AgentId::CLAUDE, CLAUDE_NOTICE, false),
            (AgentId::CLAUDE, AUTO_DEFAULT, false),
            (AgentId::CODEX, CODEX_SIGN_IN, false),
            (AgentId::CODEX, CODEX_RESUME_CWD, false),
            (AgentId::CODEX, CODEX_MODEL_MIGRATION, false),
            (AgentId::CODEX, CODEX_RATE_LIMIT, false),
            (AgentId::CODEX, CODEX_CREDIT_LIMIT, false),
            (AgentId::GROK, GROK_USAGE_LIMIT, false),
        ] {
            let dialog = recognise(agent, screen).unwrap();
            assert_eq!(dialog.is_agents_view(), view, "{}", dialog.code);
        }
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
            assert_eq!(recognise(AgentId::CODEX, screen).map(|d| d.code), code);
        }
    }

    /// The usage prompt is recognised wrapped on a narrow terminal and drawn under the
    /// conversation, while the errors its failed turns print in the transcript, which name the
    /// same owner and the same limit, are no dialog.
    #[test]
    fn codex_s_usage_prompt_is_told_from_the_errors_of_its_failed_turns() {
        let errors = "\u{203a} say hi\n\u{25a0} Your workspace is out of credits. Ask your workspace owner to refill in order to continue.\n\u{203a} next\n\u{25a0} Your workspace is out of credits. Ask your workspace owner to refill in order to continue.\n\u{25a0} Usage limit reached. You've reached your usage limit. Increase your limits to continue using codex.\n";
        let narrow = format!(
            "{errors}  You've reached your workspace\n  credit limit\n  Your workspace is out of credits. Ask\n  your workspace owner to add more.\n\u{203a} 1. Request credits\n  Press a number to choose\n"
        );
        assert_eq!(recognise(AgentId::CODEX, errors), None);
        assert_eq!(
            recognise(AgentId::CODEX, &narrow).map(|d| d.code),
            Some("codex_usage_limit")
        );
    }

    /// The two other forms of the usage prompt are recognised wrapped on a narrow terminal as well,
    /// each row indented as Codex draws it.
    #[test]
    fn codex_s_other_usage_prompts_are_recognised_wrapped_on_a_narrow_terminal() {
        let notify = "  You've reached your workspace\n  credit limit\n  Your workspace is out of\n  credits. Ask your workspace\n  owner to add more. Notify\n  owner?\n\u{203a} 1. Yes\n  2. No (default)\n";
        let increase = "  Usage limit\n  reached\n  Request a limit increase\n  from your owner to continue\n  using codex. Request\n  increase?\n\u{203a} 1. Yes\n  2. No (default)\n";
        for screen in [notify, increase] {
            let dialog = recognise(AgentId::CODEX, screen).expect(screen);
            assert_eq!(dialog.code, "codex_usage_limit", "{screen}");
            assert!(dialog.is_usage_limit(), "{screen}");
        }
    }

    /// Each error a failed turn prints in the transcript is no dialog on its own, whichever of the
    /// two it is: they say what the prompt says in other words.
    #[test]
    fn a_failed_turn_s_transcript_error_alone_is_no_codex_usage_prompt() {
        for line in [
            "\u{25a0} Your workspace is out of credits. Ask your workspace owner to refill in order to continue.\n",
            "\u{25a0} Usage limit reached. You've reached your usage limit. Increase your limits to continue using codex.\n",
        ] {
            assert_eq!(recognise(AgentId::CODEX, line), None, "{line}");
        }
    }

    /// A provider whose dialogs are drawn over the middle of the screen, the conversation's text
    /// left on their rows to either side, and are views the person opens themselves: the
    /// test-only provider, reading through this reader.
    struct Overlays;

    static OVERLAYS: Overlays = Overlays;

    const OVERLAY_DIALOGS: &[DialogPattern] = &[
        DialogPattern::all_of("test_commands", WrittenMessage::Discarded, &[])
            .with_row_run(&["Commands", "esc"], 20)
            .of_kind(DialogKind::OwnView),
        DialogPattern::all_of(
            "test_select_model",
            WrittenMessage::Discarded,
            &["Connectprovider"],
        )
        .with_row_run(&["Select model", "esc"], 20)
        .of_kind(DialogKind::OwnView),
        DialogPattern::all_of("test_three", WrittenMessage::Discarded, &[])
            .with_row_run(&["One", "two", "three"], 0),
    ];

    impl crate::session::screen::ScreenReader for Overlays {
        fn dialogs(&self) -> &'static [DialogPattern] {
            OVERLAY_DIALOGS
        }
    }

    /// The title row of a dialog drawn over the conversation, `title` and `esc` with `gap`
    /// blanks between them and the conversation's text on both sides.
    fn title_row(title: &str, gap: usize) -> String {
        format!("line 381 xxxx    {title}{}esc    xxxx\n", " ".repeat(gap))
    }

    /// A title and the key at the far end of its bar are a dialog only on one row with a run of
    /// blanks between them, as wide as the provider states: the same words in a sentence, run
    /// together, or on two rows are the conversation's.
    #[test]
    fn a_title_row_is_told_by_the_run_of_blanks_between_its_parts() {
        let _reader = crate::test_support::test_provider::declaring_screen(&OVERLAYS);
        let code = |screen: &str| recognise(AgentId::TEST, screen).map(|dialog| dialog.code);
        assert_eq!(code(&title_row("Commands", 41)), Some("test_commands"));
        assert_eq!(code(&title_row("Commands", 20)), Some("test_commands"));
        assert_eq!(
            code(&title_row("Commands", 19)),
            None,
            "narrower than stated"
        );
        for screen in [
            "The Commands esc key closes it.\n",
            "Commandsesc\n",
            "Status escalated; see Commands\n                         esc to close\n",
            "esc                         Commands\n",
            "Commands                    x                    esc\n",
        ] {
            assert_eq!(code(screen), None, "{screen}");
        }
        // A part keeps its own blanks, and the other strings of the pattern are still asked for.
        let model = title_row("Select model", 30);
        assert_eq!(code(&model), None, "its other string is missing");
        assert_eq!(
            code(&format!("{model}  Connect provider ctrl+a\n")),
            Some("test_select_model")
        );
        assert_eq!(code(&title_row("Selectmodel", 30)), None);
    }

    /// More than two parts stand in their order with a run between each two, one blank at least
    /// whatever gap is stated; the first part is looked for at every place it stands on the row.
    #[test]
    fn every_part_of_a_row_run_follows_the_one_before_after_blanks() {
        let _reader = crate::test_support::test_provider::declaring_screen(&OVERLAYS);
        let code = |screen: &str| recognise(AgentId::TEST, screen).map(|dialog| dialog.code);
        assert_eq!(code("One two   three\n"), Some("test_three"));
        assert_eq!(code("One x. One  two three\n"), Some("test_three"));
        assert_eq!(code("Onetwo three\n"), None);
        assert_eq!(code("One two x three\n"), None);
        assert_eq!(code("One three two\n"), None);
    }

    /// A view the person opened themselves holds the session's writes and is no wait for the
    /// user: the session is shown as it was, its owner is not told it waits, and an interrupt is
    /// refused as one that would go to the view.
    #[tokio::test]
    async fn a_view_the_person_opened_holds_writes_without_raising_a_hand() {
        use crate::protocol::SessionStatus;
        let _reader = crate::test_support::test_provider::declaring_screen(&OVERLAYS);
        let (state, dir) = crate::test_support::app_state("dialogs-own-view");
        crate::state::test_support::insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = crate::test_support::fake_live_session("s", AgentId::TEST, 120, 40, "sleep 30");
        state.register_live(live.clone());
        live.on_output(title_row("Commands", 41).replace('\n', "\r\n").as_bytes());
        let view = live.look_for_dialog().current.expect("the view is up");
        assert!(view.is_own_view() && !view.is_agents_view() && !view.is_usage_limit());
        assert!(!is_a_wait_for_the_user(&view));

        assert!(live.waits_for_user(), "something is on the screen");
        assert!(!live.waits_for_user_outside_views());
        assert!(!live.waits_for_user_beyond_usage_limit());
        let session = state.session_record("s").unwrap();
        assert_eq!(state.shown_status(&session), SessionStatus::Idle);
        assert!(matches!(
            state.write_hold("s"),
            Some(crate::state::WriteHold::AgentDialog(dialog)) if dialog.is_own_view()
        ));
        react_to_dialog(&state, "s", Some(view));
        assert!(state.claim_waiting_notice("s"), "nobody was told it waits");

        let refused = crate::session::interrupt::interrupt_session(&state, "s")
            .await
            .expect_err("a key would go to the view");
        let code = refused
            .downcast_ref::<crate::protocol::CodedError>()
            .map(|coded| coded.code.to_string());
        assert_eq!(
            code,
            Some(crate::protocol::error_code::SESSION_WAITING_FOR_USER.to_string())
        );
        let message = refused.to_string();
        assert!(
            message.contains("a view of the agent's own open"),
            "{message}"
        );
        assert!(!message.contains("who alone can answer"), "{message}");
    }
}
