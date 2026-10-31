//! The end of a session's process: the watcher that records how it ended, the stop of every
//! session at shutdown, and the terminal output saved as each one ends.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use super::AppState;
use crate::protocol::now_millis;
use crate::session::LiveSession;

/// How often a session's process is polled for having exited. The agent's own exit (the user
/// typing `/exit`, a crash) has no other signal: `SessionEnd` hooks are not reliable for this,
/// since a killed process fires nothing at all.
const EXIT_POLL_INTERVAL: Duration = Duration::from_millis(500);

impl AppState {
    /// Watches one session's process and records where it ended up. A session whose status was
    /// already set to archived was ended on purpose; anything else that stops is interrupted, which
    /// is the status a click resumes from. An owner is told of an end nobody asked for: one with
    /// the session neither archived, nor already interrupted by the daemon going down, nor ended
    /// by an operation that stopped the process itself (`LiveSession::end`: an account switch, a
    /// handoff or a change of sharing), nor one that Octoboard's own account switch or its
    /// restoring launched and is still watching come up (`LiveSession::watch_under_claim`), which
    /// reports the outcome itself; a switch someone asked for does not watch, so it is told. A
    /// claim held on the session says nothing else of it: a process that exits by itself under one
    /// is told, and so is one a claim that stops nothing (a handover of branches, a binding
    /// change) left running.
    pub fn watch_exit(self: &Arc<Self>, live: Arc<LiveSession>) {
        let state = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(EXIT_POLL_INTERVAL).await;
                if live.poll_exit() {
                    break;
                }
            }
            // Saved before the live entry goes, so a terminal client attaching meanwhile finds
            // either the process or what it left, never neither.
            let saving = {
                let (state, live) = (state.clone(), live.clone());
                tokio::task::spawn_blocking(move || state.save_output(&live))
            };
            // What the agent started outside its terminal session is stopped before the session
            // reads as having no process: whatever waits for that — the removal of an archived
            // session's worktree above all — must not find a dev server still working in it.
            let ending = {
                let (state, live) = (state.clone(), live.clone());
                tokio::task::spawn_blocking(move || {
                    state.process_ledger.end_launch(&live.launch_id)
                })
            };
            let _ = tokio::join!(saving, ending);
            live.note_input_recovery_ended();
            // Recorded while the process is still the session's: no launch can be claimed while
            // it is registered (`begin_launch`), so the row cannot belong to a relaunch yet — one
            // that reopened an archived session the moment the entry went would otherwise be made
            // interrupted by this write.
            let exit = state
                .live_session(&live.id)
                .is_some_and(|current| Arc::ptr_eq(&current, &live))
                .then(|| state.store.record_session_exit(&live.id, now_millis()));
            state.unregister_live(&live);
            match exit {
                Some(Ok(Some((before, session)))) => {
                    state.publish_session(&session);
                    let unasked = !before.is_dormant()
                        && !live.was_ended_on_purpose()
                        && !live.is_watched_under_claim();
                    if unasked && session.bound_to.is_some() {
                        // Delivery writes into the owner's PTY, which blocks.
                        let state = state.clone();
                        let _ = tokio::task::spawn_blocking(move || {
                            crate::owner_notices::tell_owner_it_ended(&state, &session)
                        })
                        .await;
                    }
                }
                // Deleted while its process was still exiting, which is allowed.
                Some(Ok(None)) => {
                    tracing::debug!(session = %live.id, "the session was deleted while exiting")
                }
                Some(Err(err)) => {
                    tracing::warn!(session = %live.id, %err, "recording the session's exit failed")
                }
                // Another process is the session's now, and the record is its.
                None => {}
            }
            live.settle_exit();
        });
    }

    /// Ends every running session, leaving each one interrupted and resumable. Used by the exit
    /// flow and by a signal: the daemon owns the agent processes, so none of them may outlive it.
    pub async fn stop_all_sessions(self: &Arc<Self>) {
        self.shutting_down.store(true, Ordering::Release);
        let sessions = self.live_sessions();
        for live in &sessions {
            if let Err(err) = self.interrupt_session_record(&live.id) {
                tracing::warn!(session = %live.id, %err, "marking the session interrupted failed");
            }
        }
        // What the agents started a moment ago is attributed while their chains through the
        // agents still show. Nothing is relaunched from here on, so nothing is carried over.
        self.process_ledger.close();
        let ledger = Arc::clone(&self.process_ledger);
        let _ = tokio::task::spawn_blocking(move || ledger.scan_before_ending()).await;
        // `terminate` waits out the graceful period, so it goes to a blocking thread. The output is
        // saved here rather than left to each exit watcher, which the daemon may not outlive.
        let handles: Vec<_> = sessions
            .into_iter()
            .map(|live| {
                let state = self.clone();
                tokio::task::spawn_blocking(move || {
                    live.terminate();
                    state.save_output(&live);
                    // The exit watcher would too, but the daemon may not outlive it.
                    state.process_ledger.end_launch(&live.launch_id);
                })
            })
            .collect();
        for handle in handles {
            let _ = handle.await;
        }
        // What an account switch under way held for its relaunch, which will not come.
        let ledger = Arc::clone(&self.process_ledger);
        let _ = tokio::task::spawn_blocking(move || ledger.end_held_launches()).await;
    }

    /// Keeps the frozen or final parsed screen after exit. **Blocks** on the
    /// file and on the PTY reader (see `LiveSession::keep_final_output`).
    fn save_output(&self, live: &LiveSession) {
        live.keep_final_output(|output| {
            self.saved_output.write(&live.id, output);
            // A record deleted while its process was exiting may have had its file removed before
            // the write above. Looked at after writing, and the deletion removes the file after
            // the record, so whichever comes second leaves no file behind.
            if let Ok(None) = self.store.get_session(&live.id) {
                self.saved_output.remove(&live.id);
            }
        });
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
    use crate::protocol::{error_code, Agent, CodedError, Event, SessionStatus};
    use crate::session::spawn_reader_thread;
    use crate::state::test_support::insert_record;
    use crate::state::AppState;
    use crate::test_support::{app_state, fake_live_session, PATIENCE};
    use std::time::{Duration, Instant};

    /// The output is saved before the live entry goes, so the moment the session stops being
    /// running there is already something to show for it.
    #[tokio::test]
    async fn a_process_that_ends_leaves_its_last_output_behind() {
        let (state, dir) = app_state("state-output-kept-on-exit");
        insert_record(&state, &dir, "s", SessionStatus::Working);
        let live = fake_live_session("s", Agent::Claude, 80, 24, "printf 'last words'; sleep 0.2");
        spawn_reader_thread(live.clone(), 8 * 1024);
        state.register_live(live.clone());
        state.watch_exit(live.clone());

        until_unregistered(&state, "s").await;
        let saved = state.saved_output("s").expect("the output was kept");
        assert!(String::from_utf8_lossy(&saved).contains("last words"));
    }

    #[tokio::test]
    async fn a_dormant_status_publishes_only_after_its_screen_is_frozen() {
        for status in [SessionStatus::Archived, SessionStatus::Interrupted] {
            let (state, dir) = app_state("state-output-frozen-before-publication");
            insert_record(&state, &dir, "s", SessionStatus::Working);
            let live = fake_live_session("s", Agent::Claude, 80, 24, "sleep 30");
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
        let live = fake_live_session("s", Agent::Claude, 80, 24, "printf 'still here'; sleep 30");
        spawn_reader_thread(live.clone(), 8 * 1024);
        state.register_live(live.clone());
        let deadline = Instant::now() + PATIENCE;
        while live.recent_output(64).is_empty() {
            assert!(Instant::now() < deadline, "the stand-in printed nothing");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        state.stop_all_sessions().await;
        let saved = state.saved_output("s").expect("the output was kept");
        assert!(String::from_utf8_lossy(&saved).contains("still here"));
    }

    /// A record deleted while its process is still exiting, which is allowed, gets no output saved
    /// after it.
    #[tokio::test]
    async fn a_session_deleted_while_exiting_leaves_no_output_behind() {
        let (state, dir) = app_state("state-output-deleted-while-exiting");
        insert_record(&state, &dir, "s", SessionStatus::Archived);
        let live = fake_live_session("s", Agent::Claude, 80, 24, "printf 'gone'; sleep 0.2");
        spawn_reader_thread(live.clone(), 8 * 1024);
        state.register_live(live.clone());
        assert!(state.delete_if_archived("s").unwrap());
        state.watch_exit(live.clone());

        until_unregistered(&state, "s").await;
        assert_eq!(state.saved_output("s"), None);
        assert!(!dir.join("output").join("s").exists());
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

    /// Waits, up to `PATIENCE`, for the exit watcher to let go of the session.
    async fn until_unregistered(state: &AppState, id: &str) {
        let deadline = Instant::now() + PATIENCE;
        while state.live_session(id).is_some() {
            assert!(Instant::now() < deadline, "the exit was never recorded");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}
