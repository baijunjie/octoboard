//! What an agent's terminal title says about its turn: whether one is running, and how often it
//! has gone back to the prompt. Each agent starts its title with a glyph of its own while a turn
//! runs and drops or changes it when the turn is over; the forms are in
//! `docs/agent-cli-reference.md`, section "## Across the three agents" ("Terminal titles and the
//! titles in an agent's own records"). The title observer in `crate::session` notes every completed
//! title here; `crate::title_interrupt` reads a cancel no hook reports off it, and
//! `crate::interrupt` checks that a turn is running before it sends a key.

use std::time::{Duration, Instant};

use crate::protocol::Agent;

/// The glyphs Claude Code starts its title with while a turn runs, turning about once a second.
const CLAUDE_RUNNING: std::ops::RangeInclusive<char> = '\u{25D0}'..='\u{25D3}';

/// The glyph Claude Code starts its title with when it is back at its prompt.
const CLAUDE_AT_PROMPT: char = '\u{2733}';

/// The braille spinner Codex and Grok Build start their titles with while a turn runs, a frame
/// every 100 to 300 ms.
const SPINNER: std::ops::RangeInclusive<char> = '\u{2800}'..='\u{28FF}';

/// How recently a running title must have been sent for the turn to count as still running. Every
/// agent changes its running title at least once a second (Claude Code's circle turns about once a
/// second, the spinners several times), so a running title older than this is one the agent has
/// stopped sending.
const FRESH: Duration = Duration::from_secs(3);

/// What a completed title says about the agent's turn, for the agents whose titles say anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    /// A turn is running.
    Running,
    /// The agent is back at its prompt.
    AtPrompt,
}

/// What a completed title that started with `glyph` (one of the activity glyphs
/// `crate::terminal_title` strips, or none) says, for `agent`'s titles. `empty` is a title with no
/// text at all, such as the one an agent sends at exit, which says nothing.
pub fn shown(agent: Agent, glyph: Option<char>, empty: bool) -> Option<Shown> {
    if empty {
        return None;
    }
    match (agent, glyph) {
        (Agent::Claude, Some(glyph)) if CLAUDE_RUNNING.contains(&glyph) => Some(Shown::Running),
        (Agent::Claude, Some(CLAUDE_AT_PROMPT)) => Some(Shown::AtPrompt),
        (Agent::Claude, _) => None,
        (Agent::Codex | Agent::Grok, Some(glyph)) if SPINNER.contains(&glyph) => {
            Some(Shown::Running)
        }
        // A title with no spinner: Codex's project or thread name, Grok's own name.
        (Agent::Codex | Agent::Grok, None) => Some(Shown::AtPrompt),
        (Agent::Codex | Agent::Grok, Some(_)) => None,
    }
}

/// What a session's terminal title has said about its turns, kept per session by the title
/// observer on the PTY reader thread.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TitleActivity {
    /// Whether the last title that said anything showed a turn running.
    working: bool,
    /// When the title last showed a turn running.
    running_at: Option<Instant>,
    /// How many times the title has gone from a running turn back to the prompt.
    stops: u64,
    /// When it last did.
    stopped_at: Option<Instant>,
}

impl TitleActivity {
    /// Notes a completed title that said `shown`, or nothing.
    pub fn note(&mut self, shown: Option<Shown>, now: Instant) {
        match shown {
            Some(Shown::Running) => {
                self.working = true;
                self.running_at = Some(now);
            }
            Some(Shown::AtPrompt) if self.working => {
                self.working = false;
                self.stops += 1;
                self.stopped_at = Some(now);
            }
            _ => {}
        }
    }

    /// How many times the title has gone from a running turn back to the prompt so far.
    pub fn stops(&self) -> u64 {
        self.stops
    }

    /// Whether the title shows a turn running, and has sent a running title within [`FRESH`] of
    /// `now`.
    pub fn shows_running(&self, now: Instant) -> bool {
        self.working
            && self
                .running_at
                .is_some_and(|at| now.saturating_duration_since(at) < FRESH)
    }

    /// Whether the title has gone back to the prompt since `armed`, the count of stops taken
    /// earlier, and shows no turn running now.
    pub fn stopped_since(&self, armed: u64) -> bool {
        self.stops > armed && !self.working
    }

    /// When the title last went back to the prompt, if it has since `armed`, the count of stops
    /// taken earlier, and shows no turn running now.
    pub fn at_prompt_since(&self, armed: u64) -> Option<Instant> {
        self.stopped_since(armed)
            .then_some(self.stopped_at)
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORKING: Option<Shown> = Some(Shown::Running);
    const TURNING: Option<Shown> = Some(Shown::Running);
    const IDLE: Option<Shown> = Some(Shown::AtPrompt);

    #[test]
    fn titles_with_neither_glyph_change_nothing() {
        let start = Instant::now();
        let mut activity = TitleActivity::default();
        activity.note(WORKING, start);
        let before = activity;
        for glyph in [None, Some('\u{2722}'), Some('\u{280B}')] {
            activity.note(shown(Agent::Claude, glyph, false), start);
        }
        activity.note(shown(Agent::Grok, None, true), start);
        assert_eq!(activity, before);
    }

    /// Each agent's own glyphs, and only those, say a turn is running or back at the prompt.
    #[test]
    fn each_agent_s_titles_are_read_by_its_own_glyphs() {
        for (agent, glyph, expected) in [
            (Agent::Claude, Some('\u{25D0}'), Some(Shown::Running)),
            (Agent::Claude, Some('\u{25D1}'), Some(Shown::Running)),
            (Agent::Claude, Some(CLAUDE_AT_PROMPT), Some(Shown::AtPrompt)),
            (Agent::Claude, Some('\u{280B}'), None),
            (Agent::Claude, None, None),
            (Agent::Grok, Some('\u{280B}'), Some(Shown::Running)),
            (Agent::Grok, None, Some(Shown::AtPrompt)),
            (Agent::Grok, Some('\u{25D0}'), None),
            (Agent::Codex, Some('\u{2819}'), Some(Shown::Running)),
            (Agent::Codex, None, Some(Shown::AtPrompt)),
        ] {
            assert_eq!(shown(agent, glyph, false), expected, "{agent:?} {glyph:?}");
        }
    }

    #[test]
    fn a_running_title_counts_only_while_it_keeps_coming() {
        let start = Instant::now();
        let mut activity = TitleActivity::default();
        assert!(!activity.shows_running(start));
        activity.note(WORKING, start);
        assert!(activity.shows_running(start + FRESH / 2));
        assert!(!activity.shows_running(start + FRESH));
        activity.note(TURNING, start + FRESH);
        assert!(activity.shows_running(start + FRESH));
        activity.note(IDLE, start + FRESH);
        assert!(!activity.shows_running(start + FRESH));
    }
}
