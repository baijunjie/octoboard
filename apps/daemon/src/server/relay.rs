//! The daemon's end of the requests holders relay from their helpers: they are handled by the
//! routes' own code ([`super::callbacks::handle`]), on the daemon's runtime, once the sessions the
//! holders kept are adopted: a request that names one of them cannot be answered before.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use bytes::Bytes;
use tokio::task::JoinHandle;

use super::callbacks;
use crate::holder::client::{RelayHandler, RelayReply, RelayRequest};
use crate::state::AppState;

/// How long a request waits for the adoption to finish before it is handled all the same.
const ADOPTION_WAIT: Duration = Duration::from_secs(20);

pub struct DaemonRelay {
    state: Arc<AppState>,
    runtime: tokio::runtime::Handle,
    /// The reports that arrived before the adoption was done, in the order they arrived; `None`
    /// once they are handled, from when a report is handled as it arrives.
    held: Arc<Mutex<Option<Vec<RelayRequest>>>>,
}

impl DaemonRelay {
    /// Handles on the runtime this is called in, which is the one the daemon's routes run on.
    pub fn new(state: Arc<AppState>) -> Self {
        let runtime = tokio::runtime::Handle::current();
        let held = Arc::new(Mutex::new(None));
        if !state.adoption_done() {
            *locked(&held) = Some(Vec::new());
            // Handled under the lock, so a report arriving meanwhile follows them, as the adoption
            // ends, before it reads as done, or once the wait for it runs out, whichever is first.
            let release = {
                let (state, held, runtime) = (state.clone(), held.clone(), runtime.clone());
                move || {
                    let _runtime = runtime.enter();
                    let mut held = locked(&held);
                    for request in held.take().unwrap_or_default() {
                        handle_report(&state, request);
                    }
                }
            };
            state.after_adoption(release.clone());
            let state = state.clone();
            runtime.spawn(async move {
                state.await_adoption(ADOPTION_WAIT).await;
                release();
            });
        }
        Self {
            state,
            runtime,
            held,
        }
    }
}

/// The reports held back, locked even if a thread panicked while holding it: it stays a list of
/// requests either way.
fn locked(held: &Mutex<Option<Vec<RelayRequest>>>) -> MutexGuard<'_, Option<Vec<RelayRequest>>> {
    held.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Handles a report on the calling thread, which is the reader of a holder's link: a panic in the
/// handler is logged and must not take the reader, and with it the link, down.
fn handle_report(state: &Arc<AppState>, request: RelayRequest) {
    let RelayRequest { route, param, body } = request;
    let handled = catch_unwind(AssertUnwindSafe(|| {
        callbacks::report(state.clone(), route, param, Bytes::from(body));
    }));
    if handled.is_err() {
        tracing::error!(
            ?route,
            "handling a relayed report panicked; the report is dropped"
        );
    }
}

impl RelayHandler for DaemonRelay {
    fn report(&self, request: RelayRequest) {
        let mut held = locked(&self.held);
        if let Some(held) = held.as_mut() {
            held.push(request);
            return;
        }
        // Entered, not spawned: what a report starts goes on the daemon's runtime, while the
        // report itself is handled here, before the next frame of the link is read.
        let _runtime = self.runtime.enter();
        handle_report(&self.state, request);
    }

    fn relay(&self, request: RelayRequest) -> JoinHandle<RelayReply> {
        let state = self.state.clone();
        self.runtime.spawn(async move {
            state.await_adoption(ADOPTION_WAIT).await;
            let RelayRequest { route, param, body } = request;
            let reply = callbacks::handle(state, route, param, Bytes::from(body)).await;
            RelayReply {
                status: reply.status.as_u16(),
                body: reply.body,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::holder::client::in_process_relaying;
    use crate::protocol::SessionStatus;
    use crate::test_support::{app_state, PATIENCE};

    /// A helper's hook, posted to the holder's port with the launch's key, reaches the daemon's
    /// own hook handling: the Stop event it carries is applied to the session.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_hook_posted_to_the_holder_reaches_the_daemons_hook_handling() {
        let (state, dir) = app_state("relay-hook");
        state
            .store
            .insert_console(&crate::test_support::console(&dir))
            .unwrap();
        state
            .store
            .insert_session(&crate::test_support::console_session(
                "s",
                SessionStatus::Idle,
            ))
            .unwrap();
        let relay = Arc::new(DaemonRelay::new(state.clone()));
        let holders = in_process_relaying(dir.join("holders"), relay).expect("holders");

        let (holders, issued) = tokio::task::spawn_blocking(move || {
            let issued = holders
                .issue_launch("s", None)
                .expect("the holder issues a key");
            (holders, issued)
        })
        .await
        .unwrap();
        assert_ne!(
            issued.port, state.port,
            "the holder's port, not the daemon's"
        );
        let (port, key) = (issued.port, issued.key.clone());
        let posted = tokio::task::spawn_blocking(move || {
            let stop = br#"{"hook_event_name": "Stop", "background_tasks": [{"id": "b1", "type": "shell"}]}"#;
            let accepted = crate::loopback::post(port, &key, "/hook/s", stop, PATIENCE);
            // Another session's hook is refused by the holder and never reaches the daemon.
            let refused = crate::loopback::post(port, &key, "/hook/other", stop, PATIENCE);
            (accepted, refused)
        })
        .await
        .unwrap();
        assert!(posted.0.expect("answered").is_empty());
        let refused = posted.1.expect_err("refused");
        assert_eq!(crate::loopback::error_status(&refused), Some(401));

        let deadline = Instant::now() + PATIENCE;
        while state.background_tasks_of("s").len() != 1 {
            assert!(
                Instant::now() < deadline,
                "the hook never reached the daemon"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        tokio::task::spawn_blocking(move || drop((issued, holders)))
            .await
            .unwrap();
    }

    /// A report that arrives while the adopted sessions are not there yet is held, and applied
    /// once they are; a question waits for them the same way.
    #[tokio::test(start_paused = true)]
    async fn requests_wait_for_the_adoption_to_finish() {
        let (state, dir) = app_state("relay-adoption");
        state
            .store
            .insert_console(&crate::test_support::console(&dir))
            .unwrap();
        state
            .store
            .insert_session(&crate::test_support::console_session(
                "s",
                SessionStatus::Idle,
            ))
            .unwrap();
        state.begin_adoption();
        let relay = DaemonRelay::new(state.clone());
        let stop =
            br#"{"hook_event_name": "Stop", "background_tasks": [{"id": "b1", "type": "shell"}]}"#;
        relay.report(RelayRequest {
            route: crate::holder::protocol::RelayRoute::Hook,
            param: "s".to_string(),
            body: stop.to_vec(),
        });
        let question = relay.relay(RelayRequest {
            route: crate::holder::protocol::RelayRoute::Mcp,
            param: "no-such-token".to_string(),
            body: br#"{"tool":"x"}"#.to_vec(),
        });
        // Everything the report and the question could do before the adoption ends has run by
        // then, short of the wait for it running out.
        crate::test_support::tasks_idle().await;
        assert!(state.background_tasks_of("s").is_empty());
        assert!(!question.is_finished());

        state.end_adoption();
        let deadline = Instant::now() + PATIENCE;
        while state.background_tasks_of("s").len() != 1 {
            assert!(
                Instant::now() < deadline,
                "the held report was never applied"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let reply = tokio::time::timeout(PATIENCE, question)
            .await
            .expect("answered")
            .unwrap();
        assert!(reply.body.contains("no longer valid"));
    }

    /// A question the helper waits on gets the daemon's own answer: an unknown MCP token is told
    /// its connection is no longer valid, as the daemon's HTTP route tells it.
    #[tokio::test(flavor = "multi_thread")]
    async fn an_mcp_call_posted_to_the_holder_gets_the_daemons_answer() {
        let (state, dir) = app_state("relay-mcp");
        let relay = Arc::new(DaemonRelay::new(state.clone()));
        let holders = in_process_relaying(dir.join("holders"), relay).expect("holders");
        let answer = tokio::task::spawn_blocking(move || {
            let issued = holders
                .issue_launch("s", None)
                .expect("the holder issues a key");
            let body = crate::loopback::post(
                issued.port,
                &issued.key,
                "/mcp/no-such-token",
                br#"{"tool":"x"}"#,
                PATIENCE,
            )
            .expect("answered");
            drop((issued, holders));
            body
        })
        .await
        .unwrap();
        let answer: serde_json::Value = serde_json::from_slice(&answer).unwrap();
        assert_eq!(answer["ok"], false);
        assert!(answer["error"]
            .as_str()
            .unwrap()
            .contains("no longer valid"));
    }

    /// What a launch's helpers are given, once they are built from the holder's credentials: the
    /// MCP command, the HTTP hook URL and the git hooks' environment all name the holder's port and
    /// the launch's key, and the MCP command and git hooks run the holder's executable.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_helpers_of_a_launch_in_a_holder_are_given_the_holders_port_and_the_launch_key() {
        let (state, dir) = app_state("relay-wiring");
        let relay = Arc::new(DaemonRelay::new(state.clone()));
        let holders = in_process_relaying(dir.join("holders"), relay).expect("holders");
        let (holders, issued) = tokio::task::spawn_blocking(move || {
            let issued = holders.issue_launch("session-1", None).expect("a key");
            (holders, issued)
        })
        .await
        .unwrap();
        let entry = std::fs::read_dir(dir.join("holders"))
            .unwrap()
            .find_map(|file| {
                let id = file.ok()?.path().file_stem()?.to_str()?.to_string();
                crate::holder::registry::read(&dir.join("holders"), &id)
            })
            .expect("the holder's registry file");
        assert_eq!(issued.port, entry.port);
        assert_eq!(issued.exe, entry.exe);
        assert_ne!(issued.key, state.key);

        let fixture = crate::adapter::tests::spec_fixture();
        let spec = crate::adapter::LaunchSpec {
            self_exe: &issued.exe,
            daemon_port: issued.port,
            daemon_key: &issued.key,
            ..fixture.spec()
        };
        let (program, args) = crate::adapter::mcp_server_command(&spec);
        assert_eq!(program, issued.exe);
        let port = issued.port.to_string();
        for wanted in [&port, &issued.key] {
            assert!(args.contains(wanted), "{args:?}");
        }
        let url = crate::adapter::hook_url("session-1", issued.port, &issued.key);
        assert_eq!(
            url,
            format!(
                "http://127.0.0.1:{}/hook/session-1?key={}",
                issued.port, issued.key
            )
        );
        // The installed git, and dispatchers made in the test's own directory, not the user's.
        let env = crate::session_git_hooks::session_env_under(
            &dir.join("githooks"),
            &crate::test_support::isolated_git_env(),
            &issued.exe,
            "session-1",
            issued.port,
            &issued.key,
        );
        assert!(env.contains(&("OCTOBOARD_DAEMON_PORT".to_string(), port)));
        assert!(env.contains(&("OCTOBOARD_DAEMON_KEY".to_string(), issued.key.clone())));

        tokio::task::spawn_blocking(move || drop((issued, holders)))
            .await
            .unwrap();
    }
}
