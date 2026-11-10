//! The exit watcher: what follows when a session's process ends — the output it left saved, what
//! it started outside its terminal session stopped, its exit recorded, and its owner told of an
//! end nobody asked for. Also the sequence that drops a live session from the daemon
//! ([`unregister_live`]), which the watcher ends with.

use std::sync::Arc;
use std::time::Duration;

use crate::session::LiveSession;
use crate::state::AppState;
use crate::time::now_millis;

/// How often a session's process is polled for having exited. The agent's own exit (the user
/// typing `/exit`, a crash) has no other signal: `SessionEnd` hooks are not reliable for this,
/// since a killed process fires nothing at all.
pub(crate) const EXIT_POLL_INTERVAL: Duration = Duration::from_millis(500);

/// How long the exit of a session adopted from a holder waits for the adoption to end, which is
/// when the reports its holder relayed meanwhile are queued: as long as such a report waits for it
/// (`server::relay`).
const ADOPTION_WAIT: Duration = Duration::from_secs(20);

/// Drops a session from the live map, if it is still *this* session, and what goes with it: its
/// queue, a session request it has waiting (its call has nobody left to return to), its
/// bookkeeping. Called wherever a live session is dropped.
pub(crate) fn unregister_live(state: &Arc<AppState>, live: &Arc<LiveSession>) {
    let was_current = state.remove_live(live);
    if was_current {
        crate::session_request::withdraw_for_session(state, &live.id);
    }
    state.after_live_removed(&live.id, was_current);
}

/// Ends the session's process the way its exit watcher does: it leaves the live map and its
/// queue goes with it.
#[cfg(test)]
pub(crate) fn end_process_for_test(state: &Arc<AppState>, id: &str) {
    if let Some(live) = state.live_session(id) {
        unregister_live(state, &live);
    }
}

/// Watches one session's process and records where it ended up. A session whose status was
/// already set to archived was ended on purpose; anything else that stops is interrupted, which
/// is the status a click resumes from. An owner is told of an end nobody asked for: one with
/// the session neither archived, nor already interrupted by the daemon going down, nor ended
/// by an operation that stopped the process itself (`LiveSession::end`: an account switch, a
/// handoff or a change of sharing), nor one that Octoboard's own account switch or its
/// restoring launched and is still watching come up (`LiveSession::watch_under_claim`), which
/// reports the outcome itself; a switch someone asked for does not watch, so it is told. A
/// claim held on the session says nothing else of it: a process that exits by itself under one
/// is told, and so is one a claim that stops nothing (a transfer of branches, a binding
/// change) left running. Once a planned stop begins to hand the terminals over, it stops
/// watching and records nothing: the process is the next daemon's.
pub fn watch_exit(state: &Arc<AppState>, live: Arc<LiveSession>) {
    let state = state.clone();
    #[cfg(test)]
    let watched = live.clone();
    let task = tokio::spawn(async move {
        loop {
            tokio::time::sleep(EXIT_POLL_INTERVAL).await;
            // Once a planned stop hands the terminals over, the session is the next daemon's: an
            // exit seen from here on is its to find, and nothing of the process is reaped here, nor
            // its scratch directory removed, which the adopted session's hooks still call.
            if state.is_handing_over() {
                return;
            }
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
            tokio::task::spawn_blocking(move || state.process_ledger.end_launch(&live.launch_id))
        };
        // A hook the agent waited on before it exited (Claude Code's HTTP hook, Grok Build's
        // command hook) is queued already, since the route answers once it has queued the
        // event, but may not have been applied yet. Through a holder that is likely, not
        // guaranteed: the holder answers the helper itself, and the event is queued when the
        // link has read it (in order), which may still be under way. Whether the exit cut a turn
        // short is read off the status those events leave, so a `Stop` still being applied would
        // otherwise find the turn recorded as cut short and the flag no longer its to clear.
        // Bounded (`HookQueues::settled`). An asynchronous command hook (Codex's, Claude Code's
        // command form) can still post after the exit; an end of the turn cut short that arrives
        // so is taken by `AppState::apply_late_turn_event`. The reports a holder relayed while
        // the daemon was adopting its sessions are queued only as the adoption ends, so the exit
        // of an adopted session waits for that first.
        let settling = async {
            state.await_adoption(ADOPTION_WAIT).await;
            state.hook_queues.settled(&live.id).await;
        };
        let _ = tokio::join!(saving, ending, settling);
        // The program's status reports, as far as the reader took them in by the time the output
        // was saved, decide how its turn ended in the same way, and are applied before the exit is
        // recorded (bounded, as above). Its running and held-up records go with the process.
        live.settle_program_status().await;
        live.end_program_status();
        live.note_input_recovery_ended();
        // Recorded while the process is still the session's: no launch can be claimed while
        // it is registered (`begin_launch`), so the row cannot belong to a relaunch yet — one
        // that reopened an archived session the moment the entry went would otherwise be made
        // interrupted by this write.
        let exit = state
            .live_session(&live.id)
            .is_some_and(|current| Arc::ptr_eq(&current, &live))
            .then(|| {
                state.end_process_record(&live.id, |own| {
                    state.store.record_session_exit(&live.id, now_millis(), own)
                })
            });
        unregister_live(&state, &live);
        match exit {
            Some(Ok(Some((before, session)))) => {
                state.publish_session(&session);
                let unasked = !before.has_no_process()
                    && !live.was_ended_on_purpose()
                    && !live.is_watched_under_claim();
                if unasked && session.bound_to.is_some() {
                    // Delivery writes into the owner's PTY, which blocks.
                    let state = state.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        crate::messaging::owner_notices::tell_owner_it_ended(&state, &session)
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
    #[cfg(test)]
    watched.keep_exit_watch_task(task);
    #[cfg(not(test))]
    drop(task);
}

#[cfg(test)]
mod tests {
    use super::watch_exit;
    use crate::protocol::{AgentId, SessionStatus};
    use crate::session::spawn_reader_thread;
    use crate::state::test_support::insert_record;
    use crate::state::AppState;
    use crate::test_support::{app_state, fake_live_session, PATIENCE};
    use std::time::{Duration, Instant};

    /// The output is saved before the live entry goes, so the moment the session stops being
    /// running there is already something to show for it.
    #[tokio::test]
    async fn a_process_that_ends_leaves_its_last_output_behind() {
        let (state, dir) = app_state("exit-watch-output-kept-on-exit");
        insert_record(&state, &dir, "s", SessionStatus::Working);
        let live = fake_live_session(
            "s",
            AgentId::CLAUDE,
            80,
            24,
            "printf 'last words'; sleep 0.2",
        );
        spawn_reader_thread(live.clone(), 8 * 1024);
        state.register_live(live.clone());
        watch_exit(&state, live.clone());

        until_unregistered(&state, "s").await;
        let saved = state.saved_output("s").expect("the output was kept");
        assert!(String::from_utf8_lossy(&saved).contains("last words"));
    }

    /// A record deleted while its process is still exiting, which is allowed, gets no output saved
    /// after it.
    #[tokio::test]
    async fn a_session_deleted_while_exiting_leaves_no_output_behind() {
        let (state, dir) = app_state("exit-watch-output-deleted-while-exiting");
        insert_record(&state, &dir, "s", SessionStatus::Archived);
        let live = fake_live_session("s", AgentId::CLAUDE, 80, 24, "printf 'gone'; sleep 0.2");
        spawn_reader_thread(live.clone(), 8 * 1024);
        state.register_live(live.clone());
        assert!(state.delete_if_archived("s").unwrap());
        watch_exit(&state, live.clone());

        until_unregistered(&state, "s").await;
        assert_eq!(state.saved_output("s"), None);
        assert!(!dir.join("output").join("s").exists());
    }

    /// An agent that exits at its prompt under a request's hand had no turn to cut short, though
    /// the hand holds its record at waiting for the user.
    #[tokio::test]
    async fn an_exit_under_a_request_hand_over_an_idle_agent_leaves_the_session_unpaused() {
        let (state, dir) = app_state("exit-watch-request-hand");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = fake_live_session("s", AgentId::CLAUDE, 80, 24, "sleep 0.2");
        state.register_live(live.clone());
        state.raise_request_hand("s").unwrap();
        watch_exit(&state, live.clone());

        until_unregistered(&state, "s").await;
        assert_eq!(
            state.session_record("s").unwrap().status,
            SessionStatus::Interrupted
        );
        assert!(!state.last_turn_abnormal("s"));
    }

    /// A `Stop` queued before the agent exited and still being applied when the exit is seen is
    /// applied before the exit is recorded, so the turn it ended normally is not recorded as cut
    /// short.
    #[tokio::test]
    async fn a_stop_still_being_applied_when_the_process_exits_decides_how_the_turn_ended() {
        let (state, dir) = app_state("exit-watch-stop-in-flight");
        insert_record(&state, &dir, "s", SessionStatus::Working);
        let live = fake_live_session("s", AgentId::CLAUDE, 80, 24, "true");
        state.register_live(live.clone());
        // Queued as the hook route queues it, with its application held back.
        let stop = bytes::Bytes::from_static(br#"{"hook_event_name": "Stop"}"#);
        assert_eq!(
            state.hook_queues.push("s", stop),
            crate::hooks::queue::Push::StartWorker
        );
        watch_exit(&state, live.clone());

        // Past the poll that sees the process gone: the exit waits for the event, as long as
        // `PATIENCE` under test.
        let deadline = Instant::now() + PATIENCE;
        while !live.poll_exit() {
            assert!(Instant::now() < deadline, "the stand-in never exited");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        crate::test_support::runtime_advanced(super::EXIT_POLL_INTERVAL * 2).await;
        assert_eq!(
            state.session_record("s").unwrap().status,
            SessionStatus::Working,
            "the exit is not recorded while the event is being applied"
        );
        let draining = state.clone();
        tokio::task::spawn_blocking(move || crate::hooks::apply::drain_hook_queue(&draining, "s"))
            .await
            .unwrap();

        until_unregistered(&state, "s").await;
        assert_eq!(
            state.session_record("s").unwrap().status,
            SessionStatus::Interrupted
        );
        assert!(!state.last_turn_abnormal("s"));
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
