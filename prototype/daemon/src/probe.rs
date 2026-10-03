//! `obd-proto --probe` — a tiny self-test exercising the whole chain without a UI: start a
//! `claude` session in a scratch directory, wait for a hook event, send a message, wait for the
//! resulting MCP tool call, print one pass/fail line per step. Uses the user's real Claude Code
//! quota, so it is kept to exactly one round trip.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use crate::protocol::{Agent, ControlToClient, Role, SessionState, StatusSource};
use crate::state::AppState;
use crate::term;

const STEP_TIMEOUT: Duration = Duration::from_secs(90);

pub async fn run(login_shell_launch: bool, pty_read_buf_bytes: usize) -> Result<()> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .context("binding probe listener")?;
    let port = listener.local_addr()?.port();
    let self_exe = std::env::current_exe()?.to_string_lossy().into_owned();
    let state = Arc::new(AppState::new(
        port,
        self_exe,
        login_shell_launch,
        pty_read_buf_bytes,
    ));
    let mut events = state.control_tx.subscribe();

    let router = crate::server::router(state.clone());
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });

    let scratch = std::env::temp_dir().join("obd-proto-probe");
    std::fs::create_dir_all(&scratch).context("creating probe scratch dir")?;
    let scratch = scratch
        .to_str()
        .context("probe scratch dir path is not valid UTF-8")?;

    println!("== obd-proto --probe ==");

    // Step 1: launch.
    let session_id = Uuid::new_v4();
    let task = "You are in a throwaway automated probe with no real task. Do not call any \
                tools yet. Just say \"ready\" and stop."
        .to_string();
    let session = match term::spawn_session(
        &state,
        session_id,
        Agent::Claude,
        Role::Worker,
        scratch,
        Some(task),
    ) {
        Ok(session) => {
            println!(
                "PASS  launch: claude session {session_id} started (pid {})",
                session.pid
            );
            session
        }
        Err(err) => {
            println!("FAIL  launch: {err}");
            bail!("launch failed");
        }
    };
    state
        .sessions
        .write()
        .expect("sessions lock poisoned")
        .insert(session_id, session.clone());
    state.broadcast(ControlToClient::SessionStarted {
        session: session_id,
        agent: Agent::Claude,
        pid: session.pid,
        agent_session_id: None,
    });

    // Step 2: any hook event at all (proves `--settings` hooks fired and reached the daemon).
    if wait_for(&mut events, STEP_TIMEOUT, |e| {
        matches!(e, ControlToClient::Status { session, source: StatusSource::Hook, .. } if *session == session_id)
    })
    .await
    {
        println!("PASS  hooks: received a hook-sourced status event");
    } else {
        println!("FAIL  hooks: no hook-sourced status event within {STEP_TIMEOUT:?}");
        cleanup(&state, &session);
        bail!("no hook event");
    }

    // Step 3: wait for the session to go idle (Stop fired), then send it a message.
    wait_for(&mut events, STEP_TIMEOUT, |e| {
        matches!(e, ControlToClient::Status { session, state: SessionState::Idle, .. } if *session == session_id)
    })
    .await;
    let message = "Call the `report` tool from the `octoboard` MCP server now, with summary \
                   \"probe ok\" and outcome \"done\"."
        .to_string();
    match term::queue_or_send(&state, &session, &message) {
        Ok(_) => println!("PASS  send_message: message written to session {session_id}"),
        Err(err) => {
            println!("FAIL  send_message: {err}");
            cleanup(&state, &session);
            bail!("send_message failed");
        }
    }

    // Step 4: the MCP tool call the message asked for.
    if wait_for(&mut events, STEP_TIMEOUT, |e| {
        matches!(e, ControlToClient::ToolCall { session, tool, .. } if *session == session_id && tool == "report")
    })
    .await
    {
        println!("PASS  mcp: received tool_call `report` for session {session_id}");
    } else {
        println!("FAIL  mcp: no `report` tool_call within {STEP_TIMEOUT:?}");
        cleanup(&state, &session);
        bail!("no tool_call");
    }

    cleanup(&state, &session);
    println!("== probe passed ==");
    Ok(())
}

fn cleanup(state: &Arc<AppState>, session: &Arc<crate::state::Session>) {
    let _ = session.kill();
    crate::state::unregister_child_pid(session.pid);
    state
        .sessions
        .write()
        .expect("sessions lock poisoned")
        .remove(&session.id);
}

/// Drains the control-event broadcast until `predicate` matches or `timeout` elapses.
async fn wait_for(
    events: &mut tokio::sync::broadcast::Receiver<ControlToClient>,
    timeout: Duration,
    predicate: impl Fn(&ControlToClient) -> bool,
) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return false;
        }
        match tokio::time::timeout(remaining, events.recv()).await {
            Ok(Ok(event)) => {
                if predicate(&event) {
                    return true;
                }
            }
            Ok(Err(RecvError::Lagged(_))) => continue,
            Ok(Err(RecvError::Closed)) => return false,
            Err(_) => return false,
        }
    }
}
