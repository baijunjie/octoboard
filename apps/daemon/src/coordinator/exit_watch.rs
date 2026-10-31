//! The exit watcher: what follows when a session's process ends — the output it left saved, what
//! it started outside its terminal session stopped, its exit recorded, and its owner told of an
//! end nobody asked for.

use std::sync::Arc;
use std::time::Duration;

use crate::protocol::now_millis;
use crate::session::LiveSession;
use crate::state::AppState;

/// How often a session's process is polled for having exited. The agent's own exit (the user
/// typing `/exit`, a crash) has no other signal: `SessionEnd` hooks are not reliable for this,
/// since a killed process fires nothing at all.
const EXIT_POLL_INTERVAL: Duration = Duration::from_millis(500);

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
pub fn watch_exit(state: &Arc<AppState>, live: Arc<LiveSession>) {
    let state = state.clone();
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
            tokio::task::spawn_blocking(move || state.process_ledger.end_launch(&live.launch_id))
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
}

#[cfg(test)]
mod tests {
    use super::watch_exit;
    use crate::protocol::{Agent, SessionStatus};
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
        let live = fake_live_session("s", Agent::Claude, 80, 24, "printf 'last words'; sleep 0.2");
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
        let live = fake_live_session("s", Agent::Claude, 80, 24, "printf 'gone'; sleep 0.2");
        spawn_reader_thread(live.clone(), 8 * 1024);
        state.register_live(live.clone());
        assert!(state.delete_if_archived("s").unwrap());
        watch_exit(&state, live.clone());

        until_unregistered(&state, "s").await;
        assert_eq!(state.saved_output("s"), None);
        assert!(!dir.join("output").join("s").exists());
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
