//! What an agent's terminal says about its turn: whether one is running, and how often it has gone
//! back to the prompt. Each agent starts its title with a glyph of its own while a turn runs and
//! drops or changes it when the turn is over; the forms are in `docs/agent-cli-reference.md`,
//! section "## Across the three agents" ("Terminal titles and the titles in an agent's own
//! records"). The title observer in `crate::session` notes every completed title here, and
//! `crate::session::program_status` every move of the program's status reports; `title_interrupt`
//! reads a cancel no hook reports off it, and `interrupt` checks that a turn is
//! running before it sends a key.
//!
//! An agent that reports its state through OSC 7501 (`crate::terminal::program_status`) once the
//! terminal answers the protocol's support query can drop the glyphs from its title: Claude Code
//! 2.1.322 titles itself `Claude Code` or the conversation's title, with no glyph, and reports
//! `working` when a turn starts, `idle` when it is cancelled and `done` when it ends. So the first
//! report the program makes of its root record takes over from the title for the rest of the launch
//! ([`TitleActivity::note_report`]): `working` is a turn running and every other state, the record
//! gone included, is back at the prompt, as the title showed it. A report is made only when the
//! state changes, so a running state reported once stays current until the next report.

use std::time::{Duration, Instant};

use crate::protocol::AgentId;
use crate::terminal::program_status::ReportedState;

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
/// `crate::terminal::title` strips, or none) says, for `agent`'s titles. `empty` is a title with no
/// text at all, such as the one an agent sends at exit, which says nothing.
pub fn shown(agent: AgentId, glyph: Option<char>, empty: bool) -> Option<Shown> {
    if empty {
        return None;
    }
    crate::adapter::provider(agent)
        .screen()
        .title_activity(glyph)
}

/// What a title says for an agent that starts it with a braille spinner while a turn runs and
/// drops the spinner when it is back at its prompt.
pub fn spinner_activity(glyph: Option<char>) -> Option<Shown> {
    match glyph {
        Some(glyph) if crate::terminal::title::is_braille_spinner_glyph(glyph) => {
            Some(Shown::Running)
        }
        None => Some(Shown::AtPrompt),
        Some(_) => None,
    }
}

/// What a session's terminal has said about its turns, by its title or, once the program reports
/// its status, by those reports; kept per session on the PTY reader thread. "The title" below is
/// whichever of the two says it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TitleActivity {
    /// Whether the last title or report that said anything showed a turn running.
    working: bool,
    /// When the title last showed a turn running.
    running_at: Option<Instant>,
    /// How many times the title has gone from a running turn back to the prompt.
    stops: u64,
    /// When it last did.
    stopped_at: Option<Instant>,
    /// The program has reported its root status record, whose reports say what the title said
    /// before ([`Self::note_report`]); titles change nothing from then on.
    reported: bool,
}

impl TitleActivity {
    /// Notes a completed title that said `shown`, or nothing. Ignored once the program reports its
    /// status ([`Self::note_report`]).
    pub fn note(&mut self, shown: Option<Shown>, now: Instant) {
        if !self.reported {
            self.take(shown, now);
        }
    }

    /// Notes a move of the program's root status record to `root` (`None`, the record gone), which
    /// from now on says what the title said: `working` is a turn running, every other state back
    /// at the prompt. `blocked` is a turn waiting on the user, which Claude Code's title showed as
    /// back at the prompt too.
    pub fn note_report(&mut self, root: Option<ReportedState>, now: Instant) {
        self.reported = true;
        let shown = match root {
            Some(ReportedState::Working) => Shown::Running,
            _ => Shown::AtPrompt,
        };
        self.take(Some(shown), now);
    }

    fn take(&mut self, shown: Option<Shown>, now: Instant) {
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

    /// How many times the title (or the reports) has gone from a running turn back to the prompt
    /// so far.
    pub fn stops(&self) -> u64 {
        self.stops
    }

    /// Whether the last title or report that said anything showed a turn running, however long
    /// ago: a turn whose output has stalled still reads as running here.
    pub fn last_said_running(&self) -> bool {
        self.working
    }

    /// Whether the title shows a turn running, and has sent a running title within [`FRESH`] of
    /// `now`; or, once the program reports its status, whether it last reported `working`, which
    /// it does not repeat.
    pub fn shows_running(&self, now: Instant) -> bool {
        self.working
            && (self.reported
                || self
                    .running_at
                    .is_some_and(|at| now.saturating_duration_since(at) < FRESH))
    }

    /// Whether the title has gone back to the prompt since `armed`, the count of stops taken
    /// earlier, and shows no turn running now.
    fn stopped_since(&self, armed: u64) -> bool {
        self.stops > armed && !self.working
    }

    /// When the title last went back to the prompt, if it has since `armed`, the count of stops
    /// taken earlier, and shows no turn running now.
    pub fn at_prompt_since(&self, armed: u64) -> Option<Instant> {
        self.stopped_since(armed)
            .then_some(self.stopped_at)
            .flatten()
    }

    /// Whether the title went back to the prompt after `armed`, the count of stops taken earlier,
    /// and has stayed there for `settle` by `now`.
    pub fn settled_at_prompt(&self, armed: u64, now: Instant, settle: Duration) -> bool {
        self.at_prompt_since(armed)
            .is_some_and(|at| now.saturating_duration_since(at) >= settle)
    }

    /// Whether the program's status reports have taken over from the title
    /// ([`Self::note_report`]).
    #[cfg(test)]
    pub fn reported(&self) -> bool {
        self.reported
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
            activity.note(shown(AgentId::CLAUDE, glyph, false), start);
        }
        activity.note(shown(AgentId::GROK, None, true), start);
        assert_eq!(activity, before);
    }

    /// Each agent's own glyphs, and only those, say a turn is running or back at the prompt.
    #[test]
    fn each_agent_s_titles_are_read_by_its_own_glyphs() {
        for (agent, glyph, expected) in [
            (AgentId::CLAUDE, Some('\u{25D0}'), Some(Shown::Running)),
            (AgentId::CLAUDE, Some('\u{25D1}'), Some(Shown::Running)),
            (AgentId::CLAUDE, Some('\u{2733}'), Some(Shown::AtPrompt)),
            (AgentId::CLAUDE, Some('\u{280B}'), None),
            (AgentId::CLAUDE, None, None),
            (AgentId::GROK, Some('\u{280B}'), Some(Shown::Running)),
            (AgentId::GROK, None, Some(Shown::AtPrompt)),
            (AgentId::GROK, Some('\u{25D0}'), None),
            (AgentId::CODEX, Some('\u{2819}'), Some(Shown::Running)),
            (AgentId::CODEX, None, Some(Shown::AtPrompt)),
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

    const WORKING_REPORT: Option<ReportedState> = Some(ReportedState::Working);
    const IDLE_REPORT: Option<ReportedState> = Some(ReportedState::Idle);

    /// A report of `working` is a turn running and of `idle` the turn back at the prompt, which
    /// counts a stop the way a title does.
    #[test]
    fn reports_say_whether_a_turn_is_running_and_count_its_stops() {
        let start = Instant::now();
        let mut activity = TitleActivity::default();
        activity.note_report(WORKING_REPORT, start);
        assert!(activity.shows_running(start));
        assert_eq!(activity.stops(), 0);
        assert_eq!(activity.at_prompt_since(0), None);

        let end = start + Duration::from_millis(80);
        activity.note_report(IDLE_REPORT, end);
        assert!(!activity.shows_running(end));
        assert_eq!(activity.stops(), 1);
        assert_eq!(activity.at_prompt_since(0), Some(end));
        assert_eq!(activity.at_prompt_since(1), None, "no stop since the first");

        activity.note_report(WORKING_REPORT, end);
        activity.note_report(IDLE_REPORT, end);
        assert_eq!(activity.stops(), 2);
    }

    /// Every state but `working`, and the record's going away, is the program back at its prompt.
    #[test]
    fn every_report_but_working_is_back_at_the_prompt() {
        use ReportedState::*;
        let start = Instant::now();
        for root in [Some(Idle), Some(Done), Some(Error), Some(Blocked), None] {
            let mut activity = TitleActivity::default();
            activity.note_report(WORKING_REPORT, start);
            activity.note_report(root, start);
            assert!(!activity.shows_running(start), "{root:?}");
            assert_eq!(activity.stops(), 1, "{root:?}");
            assert_eq!(activity.at_prompt_since(0), Some(start), "{root:?}");
        }
    }

    /// A report of a program that was not running a turn is no stop.
    #[test]
    fn a_report_from_the_prompt_to_the_prompt_counts_no_stop() {
        use ReportedState::*;
        let start = Instant::now();
        let mut activity = TitleActivity::default();
        for root in [Some(Idle), Some(Done), None, Some(Blocked)] {
            activity.note_report(root, start);
        }
        assert_eq!(activity.stops(), 0);
        assert_eq!(activity.at_prompt_since(0), None);
        assert!(!activity.shows_running(start));
    }

    /// A program reports only when its state changes, so a running state reported once stays
    /// current however long ago it was, where a running title stops counting after [`FRESH`].
    #[test]
    fn a_reported_working_state_stays_shown_running_until_the_next_report() {
        let start = Instant::now();
        let mut activity = TitleActivity::default();
        activity.note_report(WORKING_REPORT, start);
        for later in [FRESH, FRESH * 10, Duration::from_secs(3600)] {
            assert!(activity.shows_running(start + later), "{later:?}");
        }
        activity.note_report(IDLE_REPORT, start + FRESH * 10);
        assert!(!activity.shows_running(start + FRESH * 10));
    }

    /// Once the program has reported, the glyphs of its title say nothing: it drops them, and a
    /// stale one must not turn a state the reports gave.
    #[test]
    fn titles_are_ignored_once_the_program_has_reported() {
        let start = Instant::now();

        let mut at_prompt = TitleActivity::default();
        at_prompt.note_report(IDLE_REPORT, start);
        at_prompt.note(WORKING, start);
        at_prompt.note(TURNING, start);
        assert!(!at_prompt.shows_running(start), "a running title");
        assert_eq!(at_prompt.stops(), 0);

        let mut running = TitleActivity::default();
        running.note_report(WORKING_REPORT, start);
        running.note(IDLE, start);
        running.note(None, start);
        assert!(running.shows_running(start), "a prompt title");
        assert_eq!(running.stops(), 0, "a prompt title counts no stop");
        assert_eq!(running.at_prompt_since(0), None);
    }

    /// The report that takes over carries on from what the title had shown: a turn running by the
    /// title is ended by the report, and one the report confirms no longer lapses.
    #[test]
    fn the_first_report_takes_over_from_what_the_title_showed() {
        let start = Instant::now();

        let mut ended = TitleActivity::default();
        ended.note(WORKING, start);
        ended.note_report(IDLE_REPORT, start);
        assert!(!ended.shows_running(start));
        assert_eq!(ended.stops(), 1);

        let mut confirmed = TitleActivity::default();
        confirmed.note(WORKING, start);
        confirmed.note_report(WORKING_REPORT, start + FRESH / 2);
        assert!(confirmed.shows_running(start + FRESH * 10));
    }

    /// With no report, the title governs as it always did: only a title fresh within [`FRESH`]
    /// shows a turn running, and the stops it counts are its own.
    #[test]
    fn without_reports_titles_govern_as_before() {
        let start = Instant::now();
        let mut activity = TitleActivity::default();
        activity.note(WORKING, start);
        assert!(activity.shows_running(start));
        assert!(!activity.shows_running(start + FRESH * 10));
        activity.note(IDLE, start + FRESH * 10);
        activity.note(WORKING, start + FRESH * 11);
        activity.note(IDLE, start + FRESH * 12);
        assert_eq!(activity.stops(), 2);
        assert!(!activity.shows_running(start + FRESH * 12));
    }

    /// A turn running by the last title or report that said anything stays "last said running" for
    /// as long as nothing says otherwise, however long ago the title was sent, where
    /// `shows_running` lets a title lapse after [`FRESH`].
    #[test]
    fn the_last_thing_said_stays_running_after_the_title_has_lapsed() {
        let start = Instant::now();
        let mut activity = TitleActivity::default();
        assert!(!activity.last_said_running());
        activity.note(WORKING, start);
        assert!(activity.last_said_running());
        assert!(!activity.shows_running(start + FRESH * 10));
        assert!(activity.last_said_running(), "no freshness");

        // A title that says nothing leaves what was said.
        activity.note(None, start + FRESH * 11);
        assert!(activity.last_said_running());

        activity.note(IDLE, start + FRESH * 12);
        assert!(!activity.last_said_running());
        activity.note(None, start + FRESH * 13);
        assert!(!activity.last_said_running());

        let mut reported = TitleActivity::default();
        reported.note_report(WORKING_REPORT, start);
        assert!(reported.last_said_running());
        reported.note_report(IDLE_REPORT, start);
        assert!(!reported.last_said_running());
    }

    /// The title has settled at the prompt once it went back there after the stops counted when
    /// the watch was armed, with no turn running since, and has stayed for the settle.
    #[test]
    fn a_title_settles_at_the_prompt_after_the_wait_it_stayed_there() {
        let start = Instant::now();
        let settle = Duration::from_secs(2);
        let mut activity = TitleActivity::default();
        assert!(
            !activity.settled_at_prompt(0, start + settle * 10, Duration::ZERO),
            "no turn has run"
        );

        activity.note(WORKING, start);
        assert!(
            !activity.settled_at_prompt(0, start + settle * 10, settle),
            "still running"
        );

        let stopped = start + Duration::from_secs(5);
        activity.note(IDLE, stopped);
        assert!(!activity.settled_at_prompt(0, stopped, settle));
        assert!(!activity.settled_at_prompt(
            0,
            stopped + settle - Duration::from_millis(1),
            settle
        ));
        assert!(activity.settled_at_prompt(0, stopped + settle, settle));
        assert!(activity.settled_at_prompt(0, stopped + settle * 10, settle));
        assert!(activity.settled_at_prompt(0, stopped, Duration::ZERO));
        assert!(
            !activity.settled_at_prompt(1, stopped + settle * 10, settle),
            "no stop since the first"
        );

        // A turn running again unsettles it, and its end is a stop of its own.
        let again = stopped + settle * 11;
        activity.note(WORKING, again);
        assert!(!activity.settled_at_prompt(0, again + settle * 10, settle));
        activity.note(IDLE, again + settle);
        assert!(!activity.settled_at_prompt(1, again + settle, settle));
        assert!(activity.settled_at_prompt(1, again + settle * 2, settle));
    }

    /// Reports settle a turn at the prompt the way titles do.
    #[test]
    fn a_report_back_at_the_prompt_settles_after_the_wait_too() {
        let start = Instant::now();
        let settle = Duration::from_secs(2);
        let mut activity = TitleActivity::default();
        activity.note_report(WORKING_REPORT, start);
        assert!(!activity.settled_at_prompt(0, start + settle * 10, settle));
        activity.note_report(IDLE_REPORT, start + settle);
        assert!(!activity.settled_at_prompt(
            0,
            start + settle * 2 - Duration::from_millis(1),
            settle
        ));
        assert!(activity.settled_at_prompt(0, start + settle * 2, settle));
    }

    /// The program's reports take over from the first one on, whatever it says, and the titles
    /// before it change nothing about that.
    #[test]
    fn the_activity_reads_as_reported_from_the_first_report_on() {
        let start = Instant::now();
        for first in [
            WORKING_REPORT,
            IDLE_REPORT,
            None,
            Some(ReportedState::Blocked),
        ] {
            let mut activity = TitleActivity::default();
            assert!(!activity.reported());
            activity.note(WORKING, start);
            activity.note(IDLE, start);
            assert!(!activity.reported(), "titles are no reports");
            activity.note_report(first, start);
            assert!(activity.reported(), "{first:?}");
            activity.note_report(WORKING_REPORT, start);
            activity.note(IDLE, start);
            assert!(activity.reported(), "{first:?}");
        }
    }
}
