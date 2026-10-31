//! Whether a reading means an account is used up, and which of several accounts to move to. The
//! judgement works from a held reading alone and never decides when one is taken (`poll.rs`'s):
//! [`Exhausted`] names the window that keeps an account from opening a session,
//! [`ExpiringWeekly`] and [`AccountSignal`] rank accounts by when their weekly window resets (see
//! [`preferred_signal`]), and the small predicates tell an account's state from a failure to read
//! it.

use std::collections::HashMap;

use super::{Entry, Key};
use crate::protocol::{quota_unavailable_code as code, Agent, Code, QuotaWindow, QuotaWindowKind};

/// The wire name of the session window, whichever agent it belongs to. Each agent's own interface
/// calls it something else ([`Exhausted::named_for`]).
const SESSION_WINDOW: &str = "5-hour";

/// The window that keeps an account from opening a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exhausted {
    /// The window's name in English, the same for every agent: `5-hour`, `Weekly`, `Monthly` or
    /// `Credits`, or the window's own label when it carries one.
    pub window: String,
    /// The window's kind, from which a client words it as the agent's own interface does.
    pub kind: QuotaWindowKind,
    /// The window's length, if known; what names a credit pool.
    pub window_minutes: Option<u32>,
    /// Whether the same reading also has a model's own weekly window, which makes an account-wide
    /// weekly window "Weekly · all models" in Claude Code's interface.
    pub has_model_weekly: bool,
    /// When it resets, in milliseconds since the epoch; `None` when the agent does not say.
    pub resets_at: Option<i64>,
}

impl Exhausted {
    /// The window's name for a sentence about `agent`'s account, as that agent's own interface
    /// calls it: Claude Code's `session` and `Weekly · all models` (beside a model's own weekly
    /// window), Codex's `5h`; any other name as `window` has it.
    pub fn named_for(&self, agent: Agent) -> &str {
        match (self.kind, agent) {
            (QuotaWindowKind::Session, Agent::Claude) => "session",
            (QuotaWindowKind::Session, _) => "5h",
            (QuotaWindowKind::Weekly, Agent::Claude) if self.has_model_weekly => {
                "Weekly · all models"
            }
            _ => &self.window,
        }
    }
}

/// The window of the account's reading that keeps it from opening a session at `now`, if any: a
/// session, weekly or credit-period window used up (100% or more) that has not reset yet or says
/// nothing of when it does. A model's own weekly window never counts, as the account still serves
/// the other models. A window whose reset time has passed is stale rather than exhausted. With
/// several, the one that resets last is reported, since the account is unusable until then (one
/// without a reset time counts as last). No reading, or one without windows, passes: an unknown
/// quota never refuses. A reading kept over a failed re-read is judged by the windows it kept.
/// The reading must be the account's under `config_dir`, as `Key` has it.
pub(super) fn exhausted_in(
    entries: &HashMap<Key, Entry>,
    agent: Agent,
    account: Option<&str>,
    config_dir: Option<&str>,
    now: i64,
) -> Option<Exhausted> {
    let key = Key::of(agent, account, config_dir);
    let windows = &entries.get(&key)?.quota.windows;
    let has_model_weekly = windows
        .iter()
        .any(|window| window.kind == QuotaWindowKind::WeeklyModel);
    windows
        .iter()
        .filter(|window| {
            window.kind != QuotaWindowKind::WeeklyModel
                && window.used_percent >= 100.0
                && window.resets_at.is_none_or(|at| at > now)
        })
        .reduce(|latest, window| {
            let last = |window: &QuotaWindow| window.resets_at.unwrap_or(i64::MAX);
            if last(window) > last(latest) {
                window
            } else {
                latest
            }
        })
        .map(|window| Exhausted {
            window: window_name(window),
            kind: window.kind,
            window_minutes: window.window_minutes,
            has_model_weekly,
            resets_at: window.resets_at,
        })
}

/// What a window is called: by kind, with a credit pool named after its period (about a week,
/// about a month, or just credits). A window that carries its own label is named by it; the windows
/// that count today carry none, so that branch is a fallback.
fn window_name(window: &QuotaWindow) -> String {
    if let Some(label) = window.label.as_ref().filter(|label| !label.is_empty()) {
        return label.clone();
    }
    match window.kind {
        QuotaWindowKind::Session => SESSION_WINDOW,
        QuotaWindowKind::Weekly | QuotaWindowKind::WeeklyModel => "Weekly",
        QuotaWindowKind::CreditPeriod => match window.window_minutes {
            Some(minutes) if (6 * 24 * 60..=8 * 24 * 60).contains(&minutes) => "Weekly",
            Some(minutes) if (28 * 24 * 60..=31 * 24 * 60).contains(&minutes) => "Monthly",
            _ => "Credits",
        },
    }
    .to_string()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExpiringWeekly {
    pub resets_at: i64,
    pub remaining: f64,
}

impl ExpiringWeekly {
    /// Whether this window ranks ahead of `other`: a sooner reset, then a larger remainder.
    /// An equal pair does not, so the earlier candidate stays.
    pub fn ranks_ahead_of(self, other: Self) -> bool {
        self.resets_at < other.resets_at
            || (self.resets_at == other.resets_at
                && self.remaining.total_cmp(&other.remaining).is_gt())
    }
}

/// A weekly window that still has quota and whose reset is still ahead.
/// Account-wide `Weekly`, or `CreditPeriod` whose `window_minutes` is in
/// `6 * 24 * 60..=8 * 24 * 60`. A model's own weekly window never counts.
/// `used_percent` must be finite and below 100. A reset that is missing or
/// not strictly after `now` does not rank. Among the windows of one account,
/// the soonest reset wins, so quota about to reset is spent first, then the
/// larger remaining (`(100 - used_percent).max(0)`).
fn expiring_weekly(windows: &[QuotaWindow], now: i64) -> Option<ExpiringWeekly> {
    let mut best: Option<ExpiringWeekly> = None;
    for window in windows {
        let weekly = match window.kind {
            QuotaWindowKind::Weekly => true,
            QuotaWindowKind::CreditPeriod => window
                .window_minutes
                .is_some_and(|minutes| (6 * 24 * 60..=8 * 24 * 60).contains(&minutes)),
            QuotaWindowKind::Session | QuotaWindowKind::WeeklyModel => false,
        };
        if !weekly || !window.used_percent.is_finite() || window.used_percent >= 100.0 {
            continue;
        }
        let Some(resets_at) = window.resets_at else {
            continue;
        };
        if resets_at <= now {
            continue;
        }
        let candidate = ExpiringWeekly {
            resets_at,
            remaining: window.remaining_percent(),
        };
        if best
            .as_ref()
            .is_none_or(|current| candidate.ranks_ahead_of(*current))
        {
            best = Some(candidate);
        }
    }
    best
}

/// How the quota of one candidate account ranks for automatic selection: its soonest rankable
/// weekly window and, when its 5-hour session window is already running, when that resets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AccountSignal {
    pub weekly: ExpiringWeekly,
    pub session_resets_at: Option<i64>,
}

impl AccountSignal {
    /// The signal of an account's `windows`; `None` when no weekly window ranks.
    pub fn of(windows: &[QuotaWindow], now: i64) -> Option<Self> {
        Some(Self {
            weekly: expiring_weekly(windows, now)?,
            session_resets_at: running_session_reset(windows, now),
        })
    }

    /// How this account ranks among accounts tied on the weekly reset: a running session window
    /// before none, the sooner session reset first, then the sooner weekly reset, then the larger
    /// remainder. `Less` ranks ahead, and `Equal` leaves the earlier candidate in front.
    fn rank_among_tied(&self, other: &Self) -> std::cmp::Ordering {
        let session = |signal: &Self| signal.session_resets_at.unwrap_or(i64::MAX);
        session(self)
            .cmp(&session(other))
            .then(self.weekly.resets_at.cmp(&other.weekly.resets_at))
            .then(other.weekly.remaining.total_cmp(&self.weekly.remaining))
    }
}

/// How close two weekly resets must be, in milliseconds, to count as the same deadline: one
/// session window. Quota of a session window that is already running is lost at its reset, so
/// among accounts whose weekly quota expires within this of the soonest one, the running session
/// window is spent first and the weekly reset decides only between equals.
const SESSION_WINDOW_TOLERANCE_MS: i64 = 5 * 60 * 60 * 1000;

/// When the session window of `windows` that is running resets: one with quota left and a reset
/// still ahead. The agents report a reset only once the window has started, so a window with
/// none has not. With several, the soonest reset.
fn running_session_reset(windows: &[QuotaWindow], now: i64) -> Option<i64> {
    windows
        .iter()
        .filter(|window| {
            window.kind == QuotaWindowKind::Session
                && window.used_percent.is_finite()
                && window.used_percent < 100.0
        })
        .filter_map(|window| window.resets_at)
        .filter(|resets_at| *resets_at > now)
        .min()
}

/// The index in `signals` of the account to use first: of those whose weekly reset is less than
/// [`SESSION_WINDOW_TOLERANCE_MS`] after the soonest one, the one that ranks first by
/// [`AccountSignal::rank_among_tied`]; an earlier one wins an exact tie. `None` when empty.
pub fn preferred_signal(signals: &[AccountSignal]) -> Option<usize> {
    let soonest = signals.iter().map(|signal| signal.weekly.resets_at).min()?;
    let mut best: Option<usize> = None;
    for (index, signal) in signals.iter().enumerate() {
        if signal.weekly.resets_at - soonest >= SESSION_WINDOW_TOLERANCE_MS {
            continue;
        }
        if best.is_none_or(|current| signal.rank_among_tied(&signals[current]).is_lt()) {
            best = Some(index);
        }
    }
    best
}

/// Whether `code` is one of an account's states rather than a failure to read it.
pub(super) fn is_account_state(code: Code) -> bool {
    code == self::code::CONFIG_DIR_MISSING || code == self::code::NOT_SIGNED_IN
}

/// Whether `code` is a failure to read the account, which backs it off: neither one of its states
/// nor the CLI having no quota to report.
pub(super) fn is_failure(code: Code) -> bool {
    !is_account_state(code) && code != self::code::NO_WINDOWS
}

#[cfg(test)]
mod tests;
