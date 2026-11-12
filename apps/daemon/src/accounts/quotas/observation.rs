//! Telling a window that is counting from one that has not begun, by how its reset time moves.
//! An agent that reports a window no request has started gives it no reset time, or a placeholder
//! that follows the clock (Codex answers with one whole window from the moment it does). Only
//! readings of the same window over time tell that placeholder from a real reset, which stays put.

use super::{window_id, WindowId};
use crate::protocol::QuotaWindow;

/// How long a window's reset time is watched before a verdict is made on it. The reset time of a
/// started window stands still while the clock moves on; over less than this, the agent's rounding
/// and the time it takes to answer would pass for movement.
pub(super) const MIN_OBSERVATION: i64 = 2 * 60 * 1000;

/// How far, in milliseconds, a reset time may differ from standing still and still be taken for
/// it: the time the agent takes to answer, and its rounding of the reset to whole seconds.
pub(super) const OBSERVATION_SLACK: i64 = 60 * 1000;

/// What was last seen of one window, as the agent reported it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Observation {
    /// The reset time the agent gave, in milliseconds since the epoch.
    resets_at: i64,
    /// When it was given, in milliseconds since the epoch.
    at: i64,
    /// Whether the window was judged to be counting when this was seen.
    counting: bool,
}

/// Judges each window of a reading taken at `now` against what `tracked` holds of the same account
/// and leaves the reset time of one judged not to be counting as `None`, as an agent that has none
/// to give would; `tracked` keeps the reset time the agent gave, so that later readings are still
/// compared with it.
///
/// A window with no baseline is taken to be counting until observed otherwise. Once
/// [`MIN_OBSERVATION`] has passed since the baseline it is judged not to be counting when its time
/// to reset has not shrunk by about the time that has passed while nothing of it is used (a window
/// no request has started has nothing used), and the reading becomes the new baseline; before that
/// the baseline and its verdict stand. A baseline whose reset time has passed, or a reset time
/// further ahead than the time passed accounts for, belongs to an earlier window: the new one
/// starts afresh, counting.
pub(super) fn judge(
    tracked: &mut Vec<(WindowId, Observation)>,
    windows: &mut [QuotaWindow],
    now: i64,
) {
    let mut seen = Vec::new();
    for window in windows {
        let Some(resets_at) = window.resets_at else {
            continue;
        };
        let id = window_id(window);
        let fresh = Observation {
            resets_at,
            at: now,
            counting: true,
        };
        let base = tracked
            .iter()
            .position(|(tracked_id, _)| *tracked_id == id)
            .map(|index| tracked.swap_remove(index).1);
        let observation = match base {
            None => fresh,
            Some(base) => {
                let elapsed = now - base.at;
                let moved = resets_at - base.resets_at;
                if base.resets_at <= now || moved > elapsed + OBSERVATION_SLACK {
                    fresh
                } else if elapsed < MIN_OBSERVATION {
                    // Too soon to compare again, but a window with some of it used is counting
                    // whatever its reset did.
                    Observation {
                        counting: base.counting || window.used_percent != 0.0,
                        ..base
                    }
                } else {
                    Observation {
                        counting: moved < elapsed - OBSERVATION_SLACK || window.used_percent != 0.0,
                        ..fresh
                    }
                }
            }
        };
        if !observation.counting {
            window.resets_at = None;
        }
        seen.push((id, observation));
    }
    *tracked = seen;
}

#[cfg(test)]
mod tests;
