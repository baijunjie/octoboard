//! The end of a session's process: the stop of every session at shutdown, and the terminal output
//! saved as each one ends; the settling of every session before a planned stop, which ends none;
//! and the stop of every session when an ordinary stop escalates a planned stop already begun.

use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use super::AppState;
use crate::session::LiveSession;

/// How long a shutdown waits, for all its sessions together, for the hook events already queued
/// to be applied before it records them interrupted. Far below the seconds the application gives
/// the daemon to stop (`DAEMON_EXIT_GRACE` in the desktop shell's `sidecar/stop.rs`), which the agents'
/// own graceful exit has to fit in too; events are applied in well under a millisecond, so only a
/// wedged queue reaches it.
const SHUTDOWN_HOOK_SETTLE: Duration = Duration::from_millis(500);

/// How long a planned stop waits, once it has raised its handover barrier, for the operations
/// admitted before to finish, the ends of the processes they began included (each waited for
/// until it has ended, which takes the graceful period of a few seconds), and the removal of the
/// branches they leave: archives, removals and restarts end their processes within seconds, and an
/// outbox write is cut short after its current message. Bounded, because the update or restart
/// that asked for the stop waits on it; what is still running then is the next daemon's to adopt
/// as after a crash, and a step that has not begun by then does not begin.
#[cfg_attr(test, allow(dead_code))]
const HANDOVER_BARRIER_WAIT: Duration = Duration::from_secs(5);

/// A test coordinating an admitted operation must not race the production barrier deadline.
fn handover_barrier_wait() -> Duration {
    #[cfg(test)]
    {
        crate::test_support::PATIENCE
    }
    #[cfg(not(test))]
    HANDOVER_BARRIER_WAIT
}

/// [`SHUTDOWN_HOOK_SETTLE`], widened to `PATIENCE` under test so a test holding an event back is
/// not raced by it.
fn shutdown_hook_settle() -> Duration {
    #[cfg(test)]
    {
        SHUTDOWN_HOOK_SETTLE.max(crate::test_support::PATIENCE)
    }
    #[cfg(not(test))]
    SHUTDOWN_HOOK_SETTLE
}

impl AppState {
    /// Ends every running session, leaving each one interrupted and resumable. Used by the exit
    /// flow and by a signal: the daemon owns the agent processes, so none of them may outlive it.
    pub async fn stop_all_sessions(self: &Arc<Self>) {
        self.shutting_down.store(true, Ordering::Release);
        let sessions = self.live_sessions();
        // A turn the agent ended a moment ago may still be on its way through the hook queue.
        // Applied first, its `Stop` decides how that turn ended; applied after the interruption
        // below, it would find the turn recorded as cut short and the session no longer running
        // for the flag to be cleared on.
        self.settle_sessions(&sessions).await;
        self.interrupt_records(&sessions);
        self.attribute_before_ending().await;
        let handles: Vec<_> = sessions
            .into_iter()
            .map(|live| self.spawn_end(live))
            .collect();
        for handle in handles {
            let _ = handle.await;
        }
        self.await_late_ends().await;
        self.end_held_launches().await;
    }

    /// [`Self::stop_all_sessions`] for an ordinary stop that escalates a planned stop already
    /// begun. A session whose terminal is held by one of `left`, the holders the planned stop has
    /// handed terminals over from, parted from or left to the next daemon, is told nothing: its
    /// program ends with its holder, which `end_left` ends, never terminal by terminal. Every
    /// session's screen is kept as its preview before anything else, the screen as it is now for
    /// a session still the daemon's to end and the screen up to the handover's mark for one handed
    /// over, so that a stop cut short by whoever is waiting for the daemon to go still leaves the
    /// previews on disk. `end_left` runs once what the agents started has been attributed,
    /// alongside the ends of the other sessions, and the launches of the sessions it ends are
    /// ended in the ledger after it. Every session is left interrupted, as
    /// [`Self::stop_all_sessions`] leaves it.
    pub async fn stop_all_sessions_after_handover(
        self: &Arc<Self>,
        left: &HashSet<String>,
        end_left: impl FnOnce() + Send + 'static,
    ) {
        self.shutting_down.store(true, Ordering::Release);
        let sessions = self.live_sessions();
        let (with_holder, own): (Vec<_>, Vec<_>) = sessions.iter().cloned().partition(|live| {
            live.held_at()
                .is_some_and(|at| left.contains(&at.holder_id))
        });
        // First, and all at once.
        let keeping: Vec<_> = own
            .iter()
            .map(|live| {
                let (state, live) = (self.clone(), live.clone());
                tokio::task::spawn_blocking(move || {
                    // Frozen now, as its end would freeze it: the frozen screen is final, so its
                    // reader is not waited for.
                    live.freeze_output();
                    if let Some(frozen) = live.frozen_output() {
                        state.keep_saved_output(&live.id, &frozen);
                    }
                })
            })
            .chain(with_holder.iter().map(|live| {
                let (state, live) = (self.clone(), live.clone());
                // A handed-over terminal's reader stops at its mark, and is waited for a moment
                // to get there: what it read up to there is the screen.
                tokio::task::spawn_blocking(move || state.save_output(&live))
            }))
            .collect();
        for kept in keeping {
            let _ = kept.await;
        }
        self.settle_sessions(&sessions).await;
        self.interrupt_records(&sessions);
        self.attribute_before_ending().await;
        let ending_left = tokio::task::spawn_blocking(end_left);
        let handles: Vec<_> = own.into_iter().map(|live| self.spawn_end(live)).collect();
        for handle in handles {
            let _ = handle.await;
        }
        let _ = ending_left.await;
        let ending: Vec<_> = with_holder
            .into_iter()
            .map(|live| {
                let ledger = Arc::clone(&self.process_ledger);
                tokio::task::spawn_blocking(move || ledger.end_launch(&live.launch_id))
            })
            .collect();
        for ended in ending {
            let _ = ended.await;
        }
        // An ordinary stop that cut the planned stop short before the handover began has the
        // launches registered after its listing ended late, as `stop_all_sessions` has: their
        // ends are waited for here too.
        self.await_late_ends().await;
        self.end_held_launches().await;
    }

    /// Marks each of `sessions` interrupted, as a stop leaves it.
    fn interrupt_records(&self, sessions: &[Arc<LiveSession>]) {
        for live in sessions {
            if let Err(err) = self.interrupt_session_record(&live.id) {
                tracing::warn!(session = %live.id, %err, "marking the session interrupted failed");
            }
        }
    }

    /// Closes the process ledger and attributes what the agents started a moment ago, while their
    /// chains through the agents still show, before any agent is ended. Nothing is relaunched from
    /// here on, so nothing is carried over.
    async fn attribute_before_ending(&self) {
        self.process_ledger.close();
        let ledger = Arc::clone(&self.process_ledger);
        let _ = tokio::task::spawn_blocking(move || ledger.scan_before_ending()).await;
    }

    /// Ends the session `live` as a stop ends it ([`Self::end_as_stopped`]), on a blocking thread.
    fn spawn_end(self: &Arc<Self>, live: Arc<LiveSession>) -> tokio::task::JoinHandle<()> {
        let state = self.clone();
        tokio::task::spawn_blocking(move || state.end_as_stopped(&live))
    }

    /// Ends the session `live` as a stop ends it. **Blocks**: `terminate` waits out the graceful
    /// period. The output is saved here rather than left to the exit watcher, which the daemon may
    /// not outlive. The terminal is hung up after that: a held terminal is released in its holder
    /// only once it is hung up and its program reaped, and the stop parts from the holders next,
    /// so a release left to the session being dropped would never reach the holder, which would
    /// keep the terminal and not exit.
    fn end_as_stopped(&self, live: &LiveSession) {
        live.terminate();
        self.save_output(live);
        live.hang_up();
        // The exit watcher would too, but the daemon may not outlive it.
        self.process_ledger.end_launch(&live.launch_id);
    }

    /// Ends `live`, a launch registered once the stop of the sessions had listed them, as the stop
    /// ends the rest ([`Self::end_as_stopped`]), on a thread of its own that the stop waits for
    /// ([`Self::await_late_ends`]). What the agent started a moment ago is attributed first, as
    /// the stop attributes it before ending anything.
    pub(super) fn end_late(self: &Arc<Self>, live: Arc<LiveSession>) {
        let state = self.clone();
        let ending = std::thread::spawn(move || {
            state.process_ledger.scan_before_ending();
            // Ended on purpose, so that its exit watcher tells no owner it ended by itself: the
            // stop did not mark it interrupted, having never listed it. What follows the end
            // then finds the process gone.
            live.end();
            state.end_as_stopped(&live);
        });
        self.late_ends
            .lock()
            .expect("late ends lock poisoned")
            .push(ending);
    }

    /// Waits for the ends of the launches registered late ([`Self::end_late`]), including those
    /// registered while it waits. A launch registered after it returns is ended all the same, but
    /// not waited for.
    pub(crate) async fn await_late_ends(&self) {
        loop {
            let ending =
                std::mem::take(&mut *self.late_ends.lock().expect("late ends lock poisoned"));
            if ending.is_empty() {
                return;
            }
            let _ = tokio::task::spawn_blocking(move || {
                for end in ending {
                    let _ = end.join();
                }
            })
            .await;
        }
    }

    /// Ends what an account switch under way held for its relaunch, which will not come.
    async fn end_held_launches(&self) {
        let ledger = Arc::clone(&self.process_ledger);
        let _ = tokio::task::spawn_blocking(move || ledger.end_held_launches()).await;
    }

    /// Readies every running session for a planned stop, which hands its terminal over to its
    /// holder and ends nothing. First the handover barrier is raised: nothing more is admitted
    /// that would change or end a terminal or remove what one works in, and what was admitted
    /// before is waited for, for at most [`HANDOVER_BARRIER_WAIT`] (see `super::handover_barrier`),
    /// so that the sessions listed here are the ones it left. Then nothing is started any more,
    /// nothing of this daemon records an exit, and the hook events and program status reports
    /// already received are applied, as [`Self::stop_all_sessions`] applies them, so the next
    /// daemon finds each session's status as they left it. Returns the running sessions.
    pub async fn settle_for_planned_stop(self: &Arc<Self>) -> Vec<Arc<LiveSession>> {
        // Before the flags below: an operation still admitted may be waiting for the exit of a
        // process it ended to be recorded, which nothing records once the handover has begun.
        if !self.raise_handover_barrier(handover_barrier_wait()).await {
            tracing::warn!("operations admitted before the planned stop were still running when its wait ran out; the terminals they change may be adopted as after a crash");
        }
        self.shutting_down.store(true, Ordering::Release);
        self.handing_over.store(true, Ordering::Release);
        let sessions = self.live_sessions();
        self.settle_sessions(&sessions).await;
        sessions
    }

    /// Applies the hook events queued for `sessions` and the program status reports their readers
    /// took in, for at most [`SHUTDOWN_HOOK_SETTLE`] together.
    async fn settle_sessions(&self, sessions: &[Arc<LiveSession>]) {
        let settling = async {
            for live in sessions {
                self.hook_queues.settled(&live.id).await;
                // The same goes for a program's status report already read.
                live.settle_program_status().await;
            }
        };
        let _ = tokio::time::timeout(shutdown_hook_settle(), settling).await;
    }

    /// Keeps the frozen or final parsed screen after exit. **Blocks** on the
    /// file and on the PTY reader (see `LiveSession::keep_final_output`).
    pub(crate) fn save_output(&self, live: &LiveSession) {
        live.keep_final_output(|output| {
            self.keep_saved_output(&live.id, output);
        });
    }

    /// Keeps `output` as the terminal preview of the session `id`, in place of any other, and says
    /// whether it was written. **Blocks** on the file.
    pub(crate) fn keep_saved_output(&self, id: &str, output: &[u8]) -> bool {
        let written = self.saved_output.write(id, output);
        // A record deleted while its process was exiting may have had its file removed before the
        // write above. Looked at after writing, and the deletion removes the file after the
        // record, so whichever comes second leaves no file behind.
        if let Ok(None) = self.store.get_session(id) {
            self.saved_output.remove(id);
        }
        written
    }

    /// The terminal preview saved for this session, if one was kept.
    pub fn saved_output(&self, id: &str) -> Option<Vec<u8>> {
        self.saved_output.read(id)
    }

    /// Removes the saved output of every session the store has no record of, and any file left
    /// half-written. Run once at startup, before anything is served.
    pub fn sweep_saved_output(&self) -> Result<()> {
        let ids: std::collections::HashSet<String> = self
            .store
            .list_sessions()?
            .into_iter()
            .map(|session| session.id)
            .collect();
        self.saved_output.sweep(|id| ids.contains(id));
        Ok(())
    }

    /// Drops a session's saved output. Called after its record is deleted, never before: see
    /// [`Self::save_output`]. Also when a new process takes the session over, whose output replaces
    /// it.
    pub fn forget_saved_output(&self, id: &str) {
        self.saved_output.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use crate::protocol::{error_code, AgentId, CodedError, Event, SessionStatus};
    use crate::session::spawn_reader_thread;
    #[cfg(unix)]
    use crate::session::LiveSession;
    use crate::state::test_support::insert_record;
    use crate::test_support::{app_state, fake_live_session, PATIENCE};
    #[cfg(unix)]
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    #[tokio::test]
    async fn a_no_process_status_publishes_only_after_its_screen_is_frozen() {
        for status in [SessionStatus::Archived, SessionStatus::Interrupted] {
            let (state, dir) = app_state("state-output-frozen-before-publication");
            insert_record(&state, &dir, "s", SessionStatus::Working);
            let live = fake_live_session("s", AgentId::CLAUDE, 80, 24, "sleep 30");
            state.register_live(live.clone());
            live.on_output(b"\x1b[?1049h\x1b[Hconversation");
            let mut events = state.subscribe();
            let mut record = state.session_record("s").unwrap();
            record.status = status;
            state.save_session(&record).unwrap();
            assert!(matches!(
                events.try_recv(),
                Ok(Event::SessionUpserted { .. })
            ));
            let frozen = live
                .frozen_output()
                .expect("published status has a snapshot");
            live.on_output(b"\x1b[?1049l\x1b[2J\x1b[Hexit message");
            state.save_output(&live);
            assert_eq!(state.saved_output("s"), Some(frozen));
            assert!(live.write_input(b"should stay archived").is_err());
        }
    }

    /// The daemon does not wait for its exit watchers on the way out, so a shutdown keeps the
    /// output itself.
    #[tokio::test]
    async fn stopping_every_session_keeps_each_ones_output() {
        let (state, dir) = app_state("state-output-kept-on-shutdown");
        insert_record(&state, &dir, "s", SessionStatus::Working);
        let live = fake_live_session(
            "s",
            AgentId::CLAUDE,
            80,
            24,
            "printf 'still here'; sleep 30",
        );
        spawn_reader_thread(live.clone(), 8 * 1024);
        state.register_live(live.clone());
        let deadline = Instant::now() + PATIENCE;
        while crate::test_support::printed(&live, 64).is_empty() {
            assert!(Instant::now() < deadline, "the stand-in printed nothing");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        state.stop_all_sessions().await;
        let saved = state.saved_output("s").expect("the output was kept");
        assert!(String::from_utf8_lossy(&saved).contains("still here"));
    }

    /// An ordinary stop that escalates a planned one keeps each session's preview, leaves it
    /// interrupted, ends a session the daemon still runs itself as an ordinary stop does, and has
    /// the holders left to the next daemon ended.
    #[tokio::test]
    async fn an_escalated_stop_keeps_each_preview_and_ends_what_is_left() {
        let (state, dir) = app_state("state-output-kept-on-escalation");
        insert_record(&state, &dir, "s", SessionStatus::Working);
        let live = fake_live_session(
            "s",
            AgentId::CLAUDE,
            80,
            24,
            "printf 'still here'; sleep 30",
        );
        spawn_reader_thread(live.clone(), 8 * 1024);
        state.register_live(live.clone());
        let deadline = Instant::now() + PATIENCE;
        while crate::test_support::printed(&live, 64).is_empty() {
            assert!(Instant::now() < deadline, "the stand-in printed nothing");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        let ended_left = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let ending = ended_left.clone();
        state
            .stop_all_sessions_after_handover(&std::collections::HashSet::new(), move || {
                ending.store(true, std::sync::atomic::Ordering::Release);
            })
            .await;
        let saved = state.saved_output("s").expect("the output was kept");
        assert!(String::from_utf8_lossy(&saved).contains("still here"));
        assert_eq!(
            state.session_record("s").unwrap().status,
            SessionStatus::Interrupted
        );
        assert!(live.poll_exit(), "the session's process was not ended");
        assert!(ended_left.load(std::sync::atomic::Ordering::Acquire));
    }

    /// A `Stop` queued before the shutdown and still being applied when it begins is applied
    /// before the session is recorded interrupted, so the turn it ended normally is not recorded
    /// as cut short.
    #[tokio::test]
    async fn a_stop_still_being_applied_at_shutdown_decides_how_the_turn_ended() {
        let (state, dir) = app_state("state-shutdown-stop-in-flight");
        insert_record(&state, &dir, "s", SessionStatus::Working);
        let live = fake_live_session("s", AgentId::CLAUDE, 80, 24, "sleep 30");
        state.register_live(live.clone());
        let stop = bytes::Bytes::from_static(br#"{"hook_event_name": "Stop"}"#);
        state.hook_queues.push("s", stop);

        let draining = state.clone();
        // A `biased` `join!` polls the stop first, so the queue is drained once the stop has begun
        // and waits on it.
        let drain = async move {
            tokio::task::spawn_blocking(move || {
                crate::hooks::apply::drain_hook_queue(&draining, "s")
            })
            .await
            .unwrap();
        };
        tokio::join!(biased; state.stop_all_sessions(), drain);

        assert_eq!(
            state.session_record("s").unwrap().status,
            SessionStatus::Interrupted
        );
        assert!(!state.last_turn_abnormal("s"));
    }

    /// Once the daemon has begun to stop its sessions, no claim is granted, and a process that
    /// was already past its claim is ended as soon as it is registered, so a launch under way
    /// cannot start an agent that outlives the stop.
    #[tokio::test]
    async fn no_agent_starts_or_stays_after_the_sessions_were_stopped() {
        let (state, _dir) = app_state("state-stopped-for-good");
        let early = crate::test_support::idle_stand_in("early");
        state.register_live(early.clone());

        state.stop_all_sessions().await;

        for refused in [state.begin_launch("s").err(), state.begin_switch("s").err()] {
            let err = refused.expect("refused");
            let coded = err.downcast_ref::<CodedError>().expect("a coded error");
            assert_eq!(coded.code, error_code::DAEMON_SHUTTING_DOWN);
        }
        let late = crate::test_support::idle_stand_in("late");
        let late_check = late.clone();
        state.register_live(late.clone());
        let ended =
            crate::test_support::within(crate::test_support::PATIENCE, move || late.poll_exit())
                .await;
        assert!(ended, "the late process was ended");
        assert!(
            late_check.was_ended_on_purpose(),
            "by `end`, so its owner is not told"
        );
    }

    /// A stand-in whose program ignores `SIGTERM`, so that ending it takes the whole of the polite
    /// stop's grace before the kill: nothing that did not wait for the end can see it over. It has
    /// printed its screen, and said so only once the signal is ignored.
    #[cfg(unix)]
    fn stubborn_stand_in(id: &str) -> crate::test_support::StandIn {
        let live = fake_live_session(
            id,
            AgentId::CLAUDE,
            80,
            24,
            "trap '' TERM; printf 'late screen\\r\\n'; echo ready; while :; do sleep 1; done",
        );
        spawn_reader_thread(live.clone(), 8 * 1024);
        let ready = crate::test_support::output_containing(&live, "ready");
        assert!(ready.contains("ready"), "{ready:?}");
        live
    }

    /// Registers `live` as a launch that was past its claim when the stop of the sessions listed
    /// them: the daemon is shutting down, and the stop never listed it, so that only the late end
    /// can end it, and the stop has nothing else to wait for.
    #[cfg(unix)]
    fn register_unlisted_late(state: &Arc<crate::state::AppState>, live: &Arc<LiveSession>) {
        state
            .shutting_down
            .store(true, std::sync::atomic::Ordering::Release);
        state.register_live(live.clone());
        state
            .live
            .write()
            .expect("live sessions lock poisoned")
            .sessions
            .remove(&live.id);
    }

    /// What a launch registered late must have been left with once the stop that waited for it
    /// returns: its program ended on purpose, its screen saved, its terminal hung up, and no end
    /// still to be waited for.
    #[cfg(unix)]
    fn assert_ended_as_the_stop_ends_the_rest(
        state: &Arc<crate::state::AppState>,
        live: &LiveSession,
    ) {
        assert!(
            live.poll_exit(),
            "the stop went on before the late launch was ended"
        );
        assert!(live.was_ended_on_purpose(), "its owner would be told");
        assert!(live.is_hung_up(), "its terminal was not hung up");
        let saved = state.saved_output(&live.id).expect("the output was kept");
        assert!(
            String::from_utf8_lossy(&saved).contains("late screen"),
            "{saved:?}"
        );
        // The ends are taken only by the wait, so one left is one it did not take. This is what
        // fails when the wait is gone from a stop that has nothing else to wait for, whatever the
        // machine's speed.
        assert!(
            state.late_ends.lock().unwrap().is_empty(),
            "the stop left a late end unwaited for"
        );
    }

    /// The ordinary stop waits for the end of a launch registered after it listed the sessions,
    /// before it goes on to part from the holders.
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn stopping_every_session_waits_for_a_launch_registered_after_its_listing() {
        let (state, dir) = app_state("state-stop-waits-for-late-launch");
        insert_record(&state, &dir, "late", SessionStatus::Working);
        let late = stubborn_stand_in("late");
        register_unlisted_late(&state, &late);

        state.stop_all_sessions().await;

        assert_ended_as_the_stop_ends_the_rest(&state, &late);
    }

    /// The ordinary stop that escalates a planned one waits for such an end too, when the
    /// escalation came before the handover began and the stop is no handover.
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn an_escalated_stop_waits_for_a_launch_registered_after_its_listing() {
        let (state, dir) = app_state("state-escalated-stop-waits-for-late-launch");
        insert_record(&state, &dir, "late", SessionStatus::Working);
        let late = stubborn_stand_in("late");
        register_unlisted_late(&state, &late);

        state
            .stop_all_sessions_after_handover(&std::collections::HashSet::new(), || {})
            .await;

        assert_ended_as_the_stop_ends_the_rest(&state, &late);
    }

    /// What the agent of a launch registered late had started is stopped with it, as a stop ends
    /// the launches of the sessions it lists: the launch is ended in the process ledger.
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn what_a_launch_registered_after_the_stop_listed_the_sessions_started_is_stopped_with_it(
    ) {
        use crate::test_support::{wait_for_pid, Background, KillOnDrop};
        let (state, dir) = app_state("state-late-launch-ledger");
        insert_record(&state, &dir, "late", SessionStatus::Working);
        let pid_file = dir.join("late-child");
        let late =
            crate::test_support::stand_in_starting("late", vec![Background::sleeper(&pid_file)]);
        let child = wait_for_pid(&pid_file);
        let _guard = KillOnDrop(vec![child]);

        state.stop_all_sessions().await;
        state.register_live(late.clone());
        state.await_late_ends().await;

        assert!(
            crate::test_support::stops_running(&state, child).await,
            "what the late launch's agent started was left running"
        );
    }

    /// A planned stop ends nothing: a launch registered once it has begun is left running in its
    /// holder for the next daemon, and no end is started for the stop to wait for.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_launch_registered_after_a_planned_stop_began_is_left_running() {
        let (state, _dir) = app_state("state-planned-stop-ends-no-late-launch");
        state.settle_for_planned_stop().await;
        let late = crate::test_support::idle_stand_in("late");

        state.register_live(late.clone());

        assert!(
            state.late_ends.lock().unwrap().is_empty(),
            "an end was started"
        );
        assert!(!late.poll_exit(), "the process was ended");
        assert!(!late.was_ended_on_purpose());
    }
}
