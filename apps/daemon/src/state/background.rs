//! Each session's background tasks and default effort, held in memory (the tasks copied to a file,
//! see `super::background_store`), with the delayed emptying of the task list.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use super::background_store::{Current, SavedWork};
use super::AppState;
use crate::hooks::BackgroundTasksChange;
use crate::protocol::{BackgroundTask, SessionStatus};

/// How long a subagent's stop that leaves no background work stands before the list is emptied.
/// A subagent handing over to the next one stops and starts within the same instant, and the list
/// must not publish empty in between.
pub(crate) const EMPTY_TASKS_DELAY: Duration = Duration::from_secs(2);

/// What is known of one session's background work.
#[derive(Default)]
pub(super) struct BackgroundWork {
    tasks: Vec<BackgroundTask>,
    /// The descriptions seen for each subagent id. A `SubagentStart` carries none, and a subagent
    /// that resumes is listed again without one until its next stop lists it.
    descriptions: HashMap<String, String>,
    /// When each subagent's `SubagentStart` last arrived, kept for [`EMPTY_TASKS_DELAY`]. With
    /// async command hooks the events can arrive out of order, so a `SubagentStop` may list a
    /// registry from before a start that has already been applied.
    started: HashMap<String, tokio::time::Instant>,
    /// The ticket of the emptying of the list that is waiting out [`EMPTY_TASKS_DELAY`]. A
    /// `Stop`, a `StopFailure` and a `SubagentStop` that leaves something running take it back; a
    /// `SubagentStart` applies the emptying at once, before adding itself; a completion
    /// notification does neither.
    pending_empty: Option<u64>,
    /// Set while the work was loaded at start and its session's process has not been found yet;
    /// the work is hidden until then, see [`AppState::restore_background_work`].
    loaded: Option<Loaded>,
}

/// How the work loaded at start stands, until its session's process is found.
enum Loaded {
    /// As loaded, saved for this launch.
    Saved(String),
    /// A full list ([`BackgroundTasksChange::Replace`]) was applied since the work was loaded: the
    /// work is then wholly what the reporting process says, whatever launch it was saved for.
    Replaced,
}

impl BackgroundWork {
    /// Applies one change. Returns the ticket when the change empties the list, which is not done
    /// yet: it takes effect through [`Self::expire`] once the caller has waited out
    /// [`EMPTY_TASKS_DELAY`], unless the ticket was taken back by then.
    fn apply(
        &mut self,
        change: BackgroundTasksChange,
        now: tokio::time::Instant,
        ticket: u64,
    ) -> Option<u64> {
        self.started
            .retain(|_, at| now.duration_since(*at) < EMPTY_TASKS_DELAY);
        match change {
            BackgroundTasksChange::Replace(list) => {
                self.pending_empty = None;
                if self.loaded.is_some() {
                    self.loaded = Some(Loaded::Replaced);
                }
                self.started.clear();
                self.remember_descriptions(&list);
                self.tasks = list;
            }
            BackgroundTasksChange::Finished(ids) => {
                self.tasks.retain(|task| !ids.contains(&task.id));
                self.started.retain(|id, _| !ids.contains(id));
            }
            BackgroundTasksChange::SubagentStopped { id, tasks: listed } => {
                self.remember_descriptions(&listed);
                self.started.remove(&id);
                let mut rest: Vec<_> = listed.into_iter().filter(|task| task.id != id).collect();
                // A start that arrived just before this stop is running even if the registry
                // the stop carries predates it.
                let recent: Vec<_> = self.started.keys().cloned().collect();
                for started in recent {
                    if !rest.iter().any(|task| task.id == started) {
                        rest.push(self.subagent_task(started));
                    }
                }
                if rest.is_empty() {
                    self.pending_empty = Some(ticket);
                    return Some(ticket);
                }
                self.pending_empty = None;
                self.tasks = rest;
            }
            BackgroundTasksChange::SubagentStarted(id) => {
                // The stop it follows left nothing else running, so its deferred emptying takes
                // effect now, before this subagent is added.
                if self.pending_empty.take().is_some() {
                    self.tasks.clear();
                }
                self.started.insert(id.clone(), now);
                if !self.tasks.iter().any(|task| task.id == id) {
                    self.tasks.push(self.subagent_task(id));
                }
            }
        }
        None
    }

    /// Empties the list if `ticket` is still the one waiting. Whether the list changed.
    fn expire(&mut self, ticket: u64) -> bool {
        if self.pending_empty != Some(ticket) {
            return false;
        }
        self.pending_empty = None;
        !std::mem::take(&mut self.tasks).is_empty()
    }

    /// What is kept of the work across a daemon restart, for the process of `launch`: the tasks and
    /// the descriptions. The rest is about the next two seconds, which a restart does not carry.
    /// The tasks are none while the emptying is waiting out [`EMPTY_TASKS_DELAY`]: a restart in
    /// that time must not give back a subagent that has stopped. `None` when there is neither.
    fn saved(&self, launch: &str) -> Option<SavedWork> {
        let tasks = if self.pending_empty.is_some() {
            Vec::new()
        } else {
            self.tasks.clone()
        };
        (!tasks.is_empty() || !self.descriptions.is_empty()).then(|| SavedWork {
            launch: launch.to_string(),
            tasks,
            descriptions: self.descriptions.clone(),
        })
    }

    /// The work as [`Self::saved`] kept it, not shown until its session's process is found.
    pub(super) fn restored(saved: SavedWork) -> Self {
        Self {
            tasks: saved.tasks,
            descriptions: saved.descriptions,
            loaded: Some(Loaded::Saved(saved.launch)),
            ..Self::default()
        }
    }

    /// Whether there is nothing left at all, descriptions included, to keep a map entry for.
    fn is_empty(&self) -> bool {
        self.tasks.is_empty() && self.pending_empty.is_none() && self.descriptions.is_empty()
    }

    fn subagent_task(&self, id: String) -> BackgroundTask {
        BackgroundTask {
            description: self.descriptions.get(&id).cloned(),
            id,
            kind: Some("subagent".to_string()),
        }
    }

    fn remember_descriptions(&mut self, tasks: &[BackgroundTask]) {
        for task in tasks {
            if let (Some("subagent"), Some(description)) = (task.kind.as_deref(), &task.description)
            {
                self.descriptions
                    .insert(task.id.clone(), description.clone());
            }
        }
    }
}

impl AppState {
    /// Applies what a hook event says about the session's background tasks, and tells every client
    /// when the list changed while it is shown. A subagent's stop that leaves the list empty only
    /// takes effect after [`EMPTY_TASKS_DELAY`], unless something else changes the list by then.
    ///
    /// A subagent's start or stop counts only while the session is idle: inside a turn the turn's
    /// own `Stop` lists everything anyway, and a turn cancelled without a `Stop` would leave a
    /// foreground subagent in the list.
    pub fn apply_background_tasks_change(
        self: &Arc<Self>,
        id: &str,
        change: BackgroundTasksChange,
    ) {
        if matches!(
            change,
            BackgroundTasksChange::SubagentStarted(_)
                | BackgroundTasksChange::SubagentStopped { .. }
        ) && !self.is_idle(id)
        {
            return;
        }
        let (changed, deferred, saved_changed) = {
            let mut all = self
                .background_tasks
                .lock()
                .expect("background tasks lock poisoned");
            let work = all.entry(id.to_string()).or_default();
            let before = work.tasks.clone();
            let was_pending = work.pending_empty.is_some();
            let ticket = self
                .next_tasks_ticket
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let deferred = work.apply(change, tokio::time::Instant::now(), ticket);
            let changed = work.tasks != before;
            // What is saved differs from the list while an emptying waits, so a change of that
            // alone changes it too.
            let saved_changed = changed || was_pending != work.pending_empty.is_some();
            if work.is_empty() {
                all.remove(id);
            }
            (changed, deferred, saved_changed)
        };
        if let Some(ticket) = deferred {
            let state = self.clone();
            let id = id.to_string();
            tokio::spawn(async move {
                tokio::time::sleep(EMPTY_TASKS_DELAY).await;
                // Writes the file, which blocks.
                let _ = tokio::task::spawn_blocking(move || {
                    state.empty_background_tasks_if_pending(&id, ticket)
                })
                .await;
            });
        }
        if saved_changed {
            self.persist_background_tasks(id);
        }
        if changed {
            self.publish_if_idle(id);
        }
    }

    /// Copies the session's work to the file, as it is now. The file is written under the saved
    /// copy's own lock and the work is read under that, never the other way round, so the hot
    /// lock on the map is not held across the write, and the last of two changes to get here
    /// writes what is current. The session's process is looked up under that lock too, so the last
    /// writer saves the launch that is current. A session with no process is not copied and its
    /// saved entry is left as it is: the entry is keyed by the launch. **Blocks** on the file.
    fn persist_background_tasks(&self, id: &str) {
        self.saved_background.sync(id, || {
            let Some(live) = self.live_session(id) else {
                return Current::Unchanged;
            };
            Current::Work(
                self.background_tasks
                    .lock()
                    .expect("background tasks lock poisoned")
                    .get(id)
                    .and_then(|work| work.saved(&live.launch_id)),
            )
        });
    }

    /// Removes the saved entry of `id` off the async runtime, as
    /// [`Self::drop_saved_background_unless_live`] does; the caller has dropped the work in memory
    /// already. A change that is persisted after the session's process left finds no process and
    /// leaves the entry alone, so it cannot bring a removed entry back. Where there is no runtime
    /// the entry is removed at once.
    pub(super) fn forget_saved_background(self: &Arc<Self>, id: &str) {
        let state = self.clone();
        let id = id.to_string();
        let remove = move || state.drop_saved_background_unless_live(&id);
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                runtime.spawn_blocking(remove);
            }
            Err(_) => remove(),
        }
    }

    /// Removes the saved entry of `id`, unless the session has a process by the time it is
    /// removed: that one's entry and work are its own. The work in memory is left alone: its
    /// caller has dropped it, and a process launched since may have reported work of its own.
    /// **Blocks** on the file.
    fn drop_saved_background_unless_live(&self, id: &str) {
        self.saved_background.sync(id, || {
            if self.live_session(id).is_some() {
                Current::Unchanged
            } else {
                Current::Work(None)
            }
        });
    }

    /// [`Self::drop_saved_background_unless_live`], and the work held in memory as well, which the
    /// file's entry was loaded into: for a claim released with no process adopted for it, whose
    /// work nothing else drops. **Blocks** on the file.
    pub(super) fn release_saved_background(&self, id: &str) {
        self.saved_background.sync(id, || {
            if self.live_session(id).is_some() {
                return Current::Unchanged;
            }
            self.background_tasks
                .lock()
                .expect("background tasks lock poisoned")
                .remove(id);
            Current::Work(None)
        });
    }

    /// Settles the adopted session `id`'s background work, which [`Self::load_saved_background`]
    /// loaded and the reports since were applied on top of, once the session is found running the
    /// launch `launch`: kept when the work was saved for this launch, was not loaded (it came from
    /// reports alone), or was wholly replaced by a report since it was loaded; cleared when it was
    /// left by another launch, whose process is gone, and only partial reports (a start, a stop, a
    /// completion) were applied on top of it, which say nothing of what that launch left.
    /// The saved copy then follows what memory holds. Called as the session is adopted, before it
    /// is registered live, so that nothing sees the session live with work not confirmed for its
    /// launch. **Blocks** on the file.
    pub(crate) fn restore_background_work(&self, id: &str, launch: &str) {
        {
            let mut all = self
                .background_tasks
                .lock()
                .expect("background tasks lock poisoned");
            match all.get_mut(id) {
                Some(work) if matches!(&work.loaded, Some(Loaded::Saved(saved)) if saved != launch) =>
                {
                    all.remove(id);
                }
                Some(work) => work.loaded = None,
                None => {}
            }
        }
        self.saved_background.sync(id, || {
            Current::Work(
                self.background_tasks
                    .lock()
                    .expect("background tasks lock poisoned")
                    .get(id)
                    .and_then(|work| work.saved(launch)),
            )
        });
    }

    /// Empties the list once a subagent's stop has stood, unless something took its ticket back.
    /// **Blocks** on the file.
    fn empty_background_tasks_if_pending(self: &Arc<Self>, id: &str, ticket: u64) {
        let changed = {
            let mut all = self
                .background_tasks
                .lock()
                .expect("background tasks lock poisoned");
            let changed = all.get_mut(id).is_some_and(|work| work.expire(ticket));
            if all.get(id).is_some_and(BackgroundWork::is_empty) {
                all.remove(id);
            }
            changed
        };
        if changed {
            self.persist_background_tasks(id);
            self.publish_if_idle(id);
        }
    }

    /// Whether the session's agent is idle: the record's status, except that a session request
    /// holds the record at waiting for the user while the agent stays at what it was.
    pub(super) fn is_idle(&self, id: &str) -> bool {
        let Ok(Some(session)) = self.store.get_session(id) else {
            return false;
        };
        let hand = self
            .request_hands
            .lock()
            .expect("request hands lock poisoned")
            .get(id)
            .copied();
        hand.unwrap_or(session.status) == SessionStatus::Idle
    }

    /// Records the effort the session's agent runs it at by itself, and says whether that changed
    /// what a client is shown; the caller publishes the session when it did.
    pub fn set_default_effort(&self, id: &str, effort: Option<String>) -> bool {
        let mut all = self
            .default_efforts
            .lock()
            .expect("default efforts lock poisoned");
        let changed = all.get(id) != effort.as_ref();
        match effort {
            Some(effort) => all.insert(id.to_string(), effort),
            None => all.remove(id),
        };
        changed
    }

    /// The effort the session's agent runs it at by itself, as [`Self::set_default_effort`] last
    /// recorded it.
    pub fn default_effort_of(&self, id: &str) -> Option<String> {
        self.default_efforts
            .lock()
            .expect("default efforts lock poisoned")
            .get(id)
            .cloned()
    }

    /// Forgets the default effort of every session not in `ids`, the sessions that exist.
    pub fn retain_default_efforts(&self, ids: &HashSet<&str>) {
        self.default_efforts
            .lock()
            .expect("default efforts lock poisoned")
            .retain(|id, _| ids.contains(id.as_str()));
    }

    /// The background tasks a client is shown for session `id`, whose status shown is `shown`: its
    /// list while that is idle, where work still running is told apart from nothing left, and none
    /// for any other status.
    pub(crate) fn shown_background_tasks(
        &self,
        id: &str,
        shown: SessionStatus,
    ) -> Vec<BackgroundTask> {
        if shown == SessionStatus::Idle {
            self.background_tasks_of(id)
        } else {
            Vec::new()
        }
    }

    /// The background work the agent last listed for the session; empty when none, when the
    /// session has not been seen to leave any, or while the work is what the previous daemon saved
    /// and the session's adoption has not confirmed it ([`Self::restore_background_work`]).
    /// Whether it is worth showing depends on the status: see [`Self::shown_background_tasks`].
    pub fn background_tasks_of(&self, id: &str) -> Vec<BackgroundTask> {
        self.background_tasks
            .lock()
            .expect("background tasks lock poisoned")
            .get(id)
            .filter(|work| work.loaded.is_none())
            .map(|work| work.tasks.clone())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::BackgroundTasksChange;
    use crate::protocol::{BackgroundTask, Event, SessionStatus};
    use crate::state::test_support::{insert_record, subagent};
    use crate::test_support::app_state;
    use std::time::Duration;

    fn stopped(id: &str, listed: &[&str]) -> BackgroundTasksChange {
        BackgroundTasksChange::SubagentStopped {
            id: id.to_string(),
            tasks: listed.iter().map(|id| subagent(id)).collect(),
        }
    }

    fn ids(work: &BackgroundWork) -> Vec<&str> {
        work.tasks.iter().map(|task| task.id.as_str()).collect()
    }

    /// A stop that leaves nothing running defers the emptying; a start applies it at once and brings
    /// only the new subagent, where a notification does not touch it.
    #[test]
    fn a_stop_that_leaves_nothing_defers_the_emptying() {
        let now = tokio::time::Instant::now();
        let mut work = BackgroundWork::default();
        work.apply(BackgroundTasksChange::SubagentStarted("a".into()), now, 0);
        work.apply(BackgroundTasksChange::SubagentStarted("b".into()), now, 1);
        assert_eq!(work.apply(stopped("a", &["a", "b"]), now, 2), None);
        assert_eq!(ids(&work), ["b"]);

        assert_eq!(work.apply(stopped("b", &["b"]), now, 3), Some(3));
        assert_eq!(ids(&work), ["b"]);
        // A notification does not take the ticket back.
        work.apply(BackgroundTasksChange::Finished(vec!["x".into()]), now, 4);
        assert_eq!(work.pending_empty, Some(3));
        // A newer ticket replaces it, and the older one then does nothing.
        assert_eq!(work.apply(stopped("b", &["b"]), now, 5), Some(5));
        assert!(!work.expire(3));
        assert!(work.expire(5));
        assert!(work.tasks.is_empty() && work.is_empty());

        work.apply(stopped("b", &["b"]), now, 6);
        work.apply(BackgroundTasksChange::SubagentStarted("a".into()), now, 7);
        assert_eq!(ids(&work), ["a"]);
        assert!(!work.expire(6));
    }

    /// With async command hooks a start can arrive before the stop whose registry predates it:
    /// the subagent that started within the delay survives that stop.
    #[test]
    fn a_start_just_before_a_stop_survives_it() {
        let now = tokio::time::Instant::now();
        let mut work = BackgroundWork::default();
        work.apply(BackgroundTasksChange::SubagentStarted("b".into()), now, 0);
        work.apply(BackgroundTasksChange::SubagentStarted("a".into()), now, 1);
        let later = now + Duration::from_millis(500);
        assert_eq!(work.apply(stopped("b", &["b"]), later, 2), None);
        assert_eq!(ids(&work), ["a"]);
        // The start has aged out by the time of a later stop.
        let much_later = now + EMPTY_TASKS_DELAY * 2;
        assert_eq!(work.apply(stopped("a", &["a"]), much_later, 3), Some(3));
        // A subagent that stopped itself is not brought back by a later stop.
        let mut work = BackgroundWork::default();
        work.apply(BackgroundTasksChange::SubagentStarted("x".into()), now, 0);
        work.apply(stopped("x", &["x"]), now, 1);
        assert_eq!(work.apply(stopped("y", &["y"]), now, 2), Some(2));
    }

    /// Only a subagent's description is remembered for its next start.
    #[test]
    fn a_subagents_description_comes_back_on_its_next_start() {
        let now = tokio::time::Instant::now();
        let mut work = BackgroundWork::default();
        let described = |id: &str, kind: &str| BackgroundTask {
            id: id.to_string(),
            kind: Some(kind.to_string()),
            description: Some("probe".to_string()),
        };
        work.apply(
            BackgroundTasksChange::Replace(vec![
                described("a", "subagent"),
                described("s", "shell"),
            ]),
            now,
            0,
        );
        work.apply(BackgroundTasksChange::Replace(Vec::new()), now, 1);
        work.apply(BackgroundTasksChange::SubagentStarted("a".into()), now, 2);
        assert_eq!(work.tasks[0].description.as_deref(), Some("probe"));
        assert!(!work.descriptions.contains_key("s"));
    }

    /// An idle session shows what it has left running, and only while it is idle: the tasks are
    /// tracked beside the status, so a message still goes in like it does for any idle session.
    #[test]
    fn background_tasks_show_on_an_idle_session_until_they_finish_or_the_session_goes() {
        let (state, dir) = app_state("background-tasks");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let mut events = state.subscribe();

        let both = vec![task("a"), task("b")];
        state.apply_background_tasks_change("s", BackgroundTasksChange::Replace(both.clone()));
        let Event::SessionUpserted { session } = events.try_recv().unwrap() else {
            panic!("a new list must be published")
        };
        assert_eq!(session.background_tasks, both);
        // The same list again says nothing new.
        state.apply_background_tasks_change("s", BackgroundTasksChange::Replace(both));
        assert!(events.try_recv().is_err());

        // A hook's idle changes nothing about them.
        state.apply_hook_status("s", SessionStatus::Idle).unwrap();
        let record = state.session_record("s").unwrap();
        assert_eq!(state.wire_session(record).background_tasks.len(), 2);

        state.apply_background_tasks_change(
            "s",
            BackgroundTasksChange::Finished(vec!["a".to_string()]),
        );
        let record = state.session_record("s").unwrap();
        assert_eq!(state.wire_session(record).background_tasks, vec![task("b")]);

        // Working again: the session is not idle, so nothing is shown, but the list is kept for
        // the next idle.
        state
            .apply_hook_status("s", SessionStatus::Working)
            .unwrap();
        let record = state.session_record("s").unwrap();
        assert!(state.wire_session(record).background_tasks.is_empty());
        state.apply_hook_status("s", SessionStatus::Idle).unwrap();
        let record = state.session_record("s").unwrap();
        assert_eq!(state.wire_session(record).background_tasks, vec![task("b")]);

        // An empty list, as a `Stop` with nothing left running brings, clears them.
        state.apply_background_tasks_change("s", BackgroundTasksChange::Replace(Vec::new()));
        let record = state.session_record("s").unwrap();
        assert!(state.wire_session(record).background_tasks.is_empty());

        // The session going away takes them with it.
        state.apply_background_tasks_change("s", BackgroundTasksChange::Replace(vec![task("c")]));
        state.forget_session_bookkeeping("s");
        assert!(state.background_tasks_of("s").is_empty());
    }

    fn task(id: &str) -> BackgroundTask {
        BackgroundTask {
            id: id.to_string(),
            kind: Some("shell".to_string()),
            description: None,
        }
    }
}
