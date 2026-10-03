//! PTY lifecycle: spawning agent processes, the output-reader thread, and `send_message`.

use std::collections::HashMap;
use std::io::Read;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use uuid::Uuid;

use crate::adapter::{AgentAdapter, LaunchContext};
use crate::env_shell;
use crate::protocol::{Agent, Role, SessionState};
use crate::state::{register_child_pid, AppState, Session};

pub const INITIAL_COLS: u16 = 120;
pub const INITIAL_ROWS: u16 = 32;

/// Bytes `send_message` concatenates into one write: bracketed paste start, the message text,
/// bracketed paste end, then Enter. M1 verified this is read as one complete message by both
/// Claude Code and Grok — a single atomic write submits correctly, with no delay needed between
/// the paste and the Enter (see `docs/plans/20261001-octoboard-mvp/00-technical-validation.md`,
/// M1).
pub const BRACKETED_PASTE_START: &[u8] = b"\x1b[200~";
pub const BRACKETED_PASTE_END: &[u8] = b"\x1b[201~";
pub const SEND_MESSAGE_ENTER: &[u8] = b"\r";

// Milestone-01 debt (M1): a message whose text starts with `/` is read as a slash command even
// inside a bracketed paste, so the caller must prefix it with a single space before it reaches
// `send_message`. Not implemented here — the prototype has no caller that sends user-authored
// text starting with `/`.

/// Spawns one agent session in a PTY and starts its output-reader thread. Returns the new
/// `Session` (not yet registered in `AppState` — the caller does that once it holds an `Arc`).
pub fn spawn_session(
    state: &Arc<AppState>,
    session_id: Uuid,
    agent: Agent,
    role: Role,
    cwd: &str,
    task: Option<String>,
) -> Result<Arc<Session>> {
    let adapter: Box<dyn AgentAdapter> = match agent {
        Agent::Claude => Box::new(crate::adapter::claude::ClaudeAdapter),
        Agent::Grok => Box::new(crate::adapter::grok::GrokAdapter {
            injection: crate::adapter::grok::GrokInjection::None,
        }),
        Agent::Codex => Box::new(crate::adapter::codex::CodexAdapter {
            injection: crate::adapter::codex::CodexInjection::None,
        }),
    };

    let ctx = LaunchContext {
        session_id,
        role,
        task,
        daemon_port: state.port,
        self_exe: state.self_exe.clone(),
        resume_agent_session_id: None,
    };
    let args = adapter.build_args(&ctx);

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: INITIAL_ROWS,
            cols: INITIAL_COLS,
            pixel_width: 0,
            pixel_height: 0,
        })
        .context("opening PTY")?;

    let mut cmd = if state.login_shell_launch {
        // Fallback path, kept only so the two launch strategies can be compared directly; it
        // still runs the whole thing through one more login+interactive shell invocation instead
        // of exec'ing the resolved binary directly. The environment handling is deliberately kept
        // equivalent to the default path below: without this, a daemon started from inside an
        // agent session (the usual way this prototype is run) would hand its own session markers
        // straight to the child agent on this path alone.
        let shell = env_shell::shell_path();
        let mut full = shell_quote(adapter.binary());
        for a in &args {
            full.push(' ');
            full.push_str(&shell_quote(a));
        }
        let mut cmd = CommandBuilder::new(shell);
        cmd.args(["-l", "-i", "-c", &full]);
        for (k, _) in std::env::vars() {
            if env_shell::SNAPSHOT_ONLY_VARS.contains(&k.as_str())
                || env_shell::is_inherited_session_var(&k)
            {
                cmd.env_remove(&k);
            }
        }
        cmd.env("TERM", PTY_TERM);
        cmd
    } else {
        let env = env_shell::snapshot_shell_env().context("snapshotting shell environment")?;
        // Resolve to an absolute path ourselves rather than handing `portable_pty` the bare
        // name: its own search tries `cwd.join(exe)` first and accepts it on `.exists()` alone,
        // so a project file named e.g. `claude` would silently shadow the real CLI.
        let binary_path = resolve_binary(adapter.binary(), &env)?;
        let mut cmd = CommandBuilder::new(binary_path);
        cmd.args(&args);
        apply_env(&mut cmd, &env);
        cmd
    };
    cmd.cwd(cwd);

    let child = pair
        .slave
        .spawn_command(cmd)
        .with_context(|| format!("spawning agent `{}`", adapter.binary()))?;
    let pid = child
        .process_id()
        .ok_or_else(|| anyhow!("spawned child has no pid"))?;
    register_child_pid(pid);

    // From here on, any early return must kill the child and unregister its pid first: the
    // `Session` that would otherwise own that cleanup does not exist yet, so a failure in either
    // of these two steps would otherwise leave the agent running unattended with nothing tracking
    // it.
    let writer = match pair.master.take_writer() {
        Ok(writer) => writer,
        Err(err) => {
            kill_unregistered_child(pid);
            return Err(err).context("taking PTY writer");
        }
    };
    let mut reader = match pair.master.try_clone_reader() {
        Ok(reader) => reader,
        Err(err) => {
            kill_unregistered_child(pid);
            return Err(err).context("cloning PTY reader");
        }
    };

    let session = Arc::new(Session::new(
        session_id,
        agent,
        role,
        pid,
        child,
        writer,
        pair.master,
    ));

    // The reader is a blocking std::io::Read over the PTY, so it gets its own OS thread rather
    // than a tokio task. The buffer size bounds every daemon-to-client terminal frame (one
    // broadcast message per `read()`), which is why it is configurable (`state.pty_read_buf_bytes`,
    // `--pty-read-buf-kib` in main.rs) rather than a bare constant: T2's bench needs to vary
    // output framing independently of input chunking to measure it at all.
    let reader_session = session.clone();
    let read_buf_bytes = state.pty_read_buf_bytes;
    std::thread::spawn(move || {
        let mut buf = vec![0u8; read_buf_bytes];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => reader_session.on_pty_output(&buf[..n]),
                Err(_) => break,
            }
        }
        tracing::info!(session = %reader_session.id, "PTY reader thread exiting");
    });

    // Degraded-mode agents (grok, codex: no hooks wired yet) would otherwise never produce a
    // `status` event at all; polling the child's exit status is the one piece of G3 ("process
    // ended") that does not depend on hooks, so every adapter gets it for free.
    spawn_exit_watcher(state.clone(), session.clone());

    Ok(session)
}

fn spawn_exit_watcher(state: Arc<AppState>, session: Arc<Session>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            match session.try_wait() {
                Ok(Some(_exit_status)) => {
                    tracing::info!(session = %session.id, agent = ?session.agent, "agent process exited");
                    session.mark_exited();
                    crate::state::unregister_child_pid(session.pid);
                    state.set_status(
                        &session,
                        SessionState::Exited,
                        crate::protocol::StatusSource::Process,
                        serde_json::json!({ "note": "process exit detected by polling" }),
                    );
                    break;
                }
                Ok(None) => continue,
                Err(err) => {
                    // Give up watching, but unregister first: a pid left on the shutdown kill list
                    // with nobody watching it could later be aimed at a recycled process group.
                    tracing::warn!(session = %session.id, %err, "try_wait failed, stopping exit watcher");
                    session.mark_exited();
                    crate::state::unregister_child_pid(session.pid);
                    break;
                }
            }
        }
    });
}

/// The terminal the UI actually renders with. `xterm.js` speaks xterm, and an agent that
/// inherits anything less (notably the snapshot shell's `TERM=dumb`) downgrades its own renderer.
const PTY_TERM: &str = "xterm-256color";

fn apply_env(cmd: &mut CommandBuilder, env: &HashMap<String, String>) {
    cmd.env_clear();
    for (k, v) in env {
        if crate::env_shell::SNAPSHOT_ONLY_VARS.contains(&k.as_str())
            || crate::env_shell::is_inherited_session_var(k)
        {
            continue;
        }
        cmd.env(k, v);
    }
    cmd.env("TERM", PTY_TERM);
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Resolves a bare binary name to an absolute path by searching `env`'s `PATH`, mirroring what
/// the login shell snapshot would have found. Deliberately does not fall through to
/// `portable_pty`'s own resolution, whose `cwd.join(exe)` + `.exists()` search a bare name would
/// otherwise hit first.
fn resolve_binary(name: &str, env: &HashMap<String, String>) -> Result<String> {
    let path_var = env.get("PATH").map(String::as_str).unwrap_or_default();
    for dir in path_var.split(':') {
        if dir.is_empty() {
            continue;
        }
        let candidate = std::path::Path::new(dir).join(name);
        if candidate.is_file() {
            return Ok(candidate.to_string_lossy().into_owned());
        }
    }
    bail!("`{name}` not found on PATH in the snapshotted shell environment")
}

/// Kills a just-spawned child that has no `Session` yet to own its cleanup, and drops its
/// registration — used only on the narrow failure window in `spawn_session` between
/// `register_child_pid` and the `Session` existing.
fn kill_unregistered_child(pid: u32) {
    crate::state::unregister_child_pid(pid);
    // stderr is discarded the same way `state.rs`'s kills are: this is the "already dead" case
    // (the process has an unreaped zombie but no other group member, so `kill`'s signal to the
    // group prints macOS's "Operation not permitted") rather than an actual permission problem,
    // and surfacing it here would be the one kill call in the daemon that still does.
    let _ = std::process::Command::new("kill")
        .args(["-9", &format!("-{pid}")])
        .stderr(std::process::Stdio::null())
        .status();
}

/// Writes a message into the session's PTY as bracketed paste + Enter, or queues it if the
/// session is not currently "awaiting instructions" (`SessionState::Idle`) — PROTOCOL.md's
/// `message_queued`. Queued messages are flushed by `AppState::set_status` the next time the
/// session goes idle.
///
/// This gate is prototype-only; it is not the settled product rule. M1/M2 settled on delivering
/// while idle *or* mid-turn — every agent queues the message itself and consumes it when the turn
/// ends — holding only while a modal dialog is up or the session's state is unknown (see
/// `docs/mvp.md`'s `send_message` entry). The gate here is deliberately simpler, per
/// `PROTOCOL.md`, but it has a real consequence: `grok` and `codex` have no hooks wired in this
/// prototype, so their status never leaves `Working`, and a message sent to either queues forever
/// and is never flushed.
pub fn queue_or_send(state: &Arc<AppState>, session: &Arc<Session>, text: &str) -> Result<()> {
    let is_idle = *session.status.lock().expect("status mutex poisoned") == SessionState::Idle;
    if is_idle {
        send_message(session, text)?;
    } else {
        session
            .message_queue
            .lock()
            .expect("queue mutex poisoned")
            .push(text.to_string());
        state.broadcast(crate::protocol::ControlToClient::MessageQueued {
            session: session.id,
            reason: "session is not awaiting instructions".to_string(),
        });
    }
    Ok(())
}

/// The actual PTY write, unconditional on session state — callers (`queue_or_send`, the queue
/// flush in `state::AppState::flush_queue`) are responsible for only calling this when the
/// session is ready. Built as one buffer and issued as a single `write_input` call: M1 verified
/// that is what makes the CLI read it as one complete message (see the constants' doc comment
/// above).
pub fn send_message(session: &Arc<Session>, text: &str) -> Result<()> {
    let mut buf = Vec::with_capacity(
        BRACKETED_PASTE_START.len()
            + text.len()
            + BRACKETED_PASTE_END.len()
            + SEND_MESSAGE_ENTER.len(),
    );
    buf.extend_from_slice(BRACKETED_PASTE_START);
    buf.extend_from_slice(text.as_bytes());
    buf.extend_from_slice(BRACKETED_PASTE_END);
    buf.extend_from_slice(SEND_MESSAGE_ENTER);
    session.write_input(&buf)?;
    Ok(())
}
