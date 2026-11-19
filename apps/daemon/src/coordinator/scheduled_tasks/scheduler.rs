//! The scheduler: one daemon task that runs each enabled scheduled task when it comes due. Its
//! first pass, at the daemon's start, catches up what was missed while the daemon was not running;
//! every later pass runs what has come due since. It waits until the earliest next run, at most
//! [`CHECK_EVERY`] at a time, since the clock that counts is the wall clock and a timer does not
//! advance while the machine sleeps, and it is woken early when a task is created, changed, turned
//! on or off or deleted ([`wake`]).

use std::sync::Arc;
use std::time::Duration;

use jiff::tz::TimeZone;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::protocol::{Event, Schedule, ScheduledRunTrigger, ScheduledTask};
use crate::state::AppState;
use crate::time::now_millis;

use super::runs::{agent_launcher, run_task, Launcher, NotRecorded};
use super::{local_zone, next_run};

/// The longest the scheduler waits between two looks at the wall clock.
const CHECK_EVERY: Duration = Duration::from_secs(60);

/// How old a missed occurrence may be and still be made up: one older is skipped.
pub(crate) const CATCH_UP_WINDOW_MILLIS: i64 = 24 * 60 * 60 * 1000;

/// How late past its occurrence a run still counts as on schedule rather than as a catch-up: a pass
/// that wakes somewhat late (a busy machine, the wait's own granularity) is still the schedule;
/// one that wakes later than this was held up by the machine sleeping.
pub(crate) const ON_TIME_MILLIS: i64 = 2 * CHECK_EVERY.as_millis() as i64;

/// What wakes the scheduler early; held by [`AppState`] so the request handlers can reach it.
#[derive(Default)]
pub struct SchedulerWake(Arc<Notify>);

impl SchedulerWake {
    /// Wakes the scheduler for a look at the tasks now. A wake while it is not waiting is kept for
    /// its next wait, so a change made during a pass is not missed.
    pub fn wake(&self) {
        self.0.notify_one();
    }
}

/// Which pass the scheduler is making.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Pass {
    /// The first, at the daemon's start: every occurrence that has passed was missed.
    Start,
    /// A later one.
    Running,
}

/// Starts the scheduler. Started once the store is open and the sessions are adopted, so that a
/// catch-up launch does not race the start-up. It holds the state weakly and ends once the daemon
/// is asked to stop, of either kind: a run due then is left for the daemon that starts next.
pub fn spawn(state: &Arc<AppState>) {
    spawn_with(state, CHECK_EVERY, agent_launcher());
}

pub(super) fn spawn_with(
    state: &Arc<AppState>,
    check_every: Duration,
    launch: Launcher,
) -> JoinHandle<()> {
    let weak = Arc::downgrade(state);
    let wake = state.scheduler_wake.0.clone();
    tokio::spawn(async move {
        let mut pass = Pass::Start;
        loop {
            let wait = {
                let Some(state) = weak.upgrade() else { return };
                if state.stop_asked() {
                    return;
                }
                let now = now_millis();
                let earliest = run_due(&state, now, &local_zone(), pass, &launch);
                pass = Pass::Running;
                earliest.map_or(check_every, |at| {
                    Duration::from_millis((at - now).max(0) as u64).min(check_every)
                })
            };
            tokio::select! {
                _ = tokio::time::sleep(wait) => {}
                _ = wake.notified() => {}
            }
        }
    })
}

/// One pass at `now`: for each enabled task whose next run has passed, takes the occurrence (moves
/// its next run past `now`, telling every client) and runs it in the background, or skips it when
/// it is too old to make up. Returns the earliest next run among the enabled tasks.
///
/// Several missed occurrences of a task make one run, of the most recent no older than
/// [`CATCH_UP_WINDOW_MILLIS`]. It counts as on schedule only on a later pass, for the occurrence
/// the task was waiting for, at most [`ON_TIME_MILLIS`] late; otherwise it is a catch-up.
pub(super) fn run_due(
    state: &Arc<AppState>,
    now: i64,
    zone: &TimeZone,
    pass: Pass,
    launch: &Launcher,
) -> Option<i64> {
    let tasks = match state.store.list_scheduled_tasks() {
        Ok(tasks) => tasks,
        Err(err) => {
            tracing::warn!(%err, "reading the scheduled tasks failed");
            return None;
        }
    };
    let mut earliest: Option<i64> = None;
    let mut note = |at: Option<i64>| {
        if let Some(at) = at {
            earliest = Some(earliest.map_or(at, |known| known.min(at)));
        }
    };
    for task in tasks {
        let Some(stored) = task.next_run_at.filter(|_| task.enabled) else {
            continue;
        };
        if stored > now {
            note(Some(stored));
            continue;
        }
        if state.stop_asked() {
            break;
        }
        let next = next_run(&task.schedule, now, zone);
        let run = latest_occurrence(&task.schedule, stored, now, zone).map(|due| {
            let on_time = pass == Pass::Running && due == stored && now - due <= ON_TIME_MILLIS;
            let trigger = if on_time {
                ScheduledRunTrigger::Scheduled
            } else {
                ScheduledRunTrigger::CatchUp
            };
            (trigger, due)
        });
        let taken = match state
            .store
            .take_scheduled_occurrence(&task.id, stored, next)
        {
            Ok(Some(taken)) => taken,
            // Changed since it was read: the change woke the scheduler, which reads it again.
            Ok(None) => continue,
            Err(err) => {
                tracing::warn!(task = %task.id, %err, "moving a scheduled task's next run failed");
                continue;
            }
        };
        note(next);
        state.broadcast(Event::ScheduledTaskUpserted {
            task: taken.clone(),
        });
        match run {
            Some((trigger, due)) => spawn_run(state, taken, trigger, due, stored, launch.clone()),
            None => tracing::info!(
                task = %task.id,
                missed = stored,
                "a scheduled task's missed run is too old to make up; skipped"
            ),
        }
    }
    earliest
}

/// Runs `task` for the occurrence `due` in the background. When the daemon's stop refuses the
/// launch, the occurrence the task was waiting for (`stored`) is given back, so the daemon that
/// starts next makes it up.
fn spawn_run(
    state: &Arc<AppState>,
    task: ScheduledTask,
    trigger: ScheduledRunTrigger,
    due: i64,
    stored: i64,
    launch: Launcher,
) {
    let state = state.clone();
    tokio::spawn(async move {
        if run_task(&state, &task, trigger, Some(due), &launch).await != Err(NotRecorded::Stopping)
        {
            return;
        }
        match state
            .store
            .give_back_scheduled_occurrence(&task.id, stored, task.next_run_at)
        {
            Ok(Some(restored)) => state.broadcast(Event::ScheduledTaskUpserted { task: restored }),
            Ok(None) => {}
            Err(err) => {
                tracing::warn!(task = %task.id, %err, "giving back a scheduled task's run failed")
            }
        }
    });
}

/// The most recent occurrence of `schedule` from `first` (the occurrence the task was waiting for)
/// to `now`, both included, no older than [`CATCH_UP_WINDOW_MILLIS`]; `None` when there is none.
pub(super) fn latest_occurrence(
    schedule: &Schedule,
    first: i64,
    now: i64,
    zone: &TimeZone,
) -> Option<i64> {
    let oldest = now - CATCH_UP_WINDOW_MILLIS;
    if first > now {
        return None;
    }
    let mut latest = (first >= oldest).then_some(first);
    let mut after = first.max(oldest - 1);
    while let Some(at) = next_run(schedule, after, zone).filter(|at| *at <= now) {
        latest = Some(at);
        after = at;
    }
    latest
}
