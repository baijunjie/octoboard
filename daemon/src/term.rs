//! Launching an agent in a PTY, and writing messages into a running one.

use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use uuid::Uuid;

use crate::adapter::{self, LaunchSpec};
use crate::env_shell;
use crate::paths;
use crate::protocol::Agent;
use crate::ptyio;
use crate::session::{self, LiveSession};

/// Size the PTY starts at. The UI sends a `resize` as soon as it attaches; this only has to be
/// something sane for the output an agent produces before that.
const INITIAL_COLS: u16 = 120;
const INITIAL_ROWS: u16 = 32;

/// The terminal the UI actually renders with. An agent that inherits anything less — notably the
/// snapshot shell's `TERM=dumb` — downgrades its own renderer, colour and mouse handling.
const PTY_TERM: &str = "xterm-256color";

/// Size of the buffer the PTY reader thread reads into, which also bounds the size of every
/// daemon-to-client terminal frame. Not a tuning lever: sweeping it from 4 KiB to 256 KiB made no
/// measurable difference to throughput at two very different source rates.
const PTY_READ_BUF: usize = 8 * 1024;

/// Bracketed paste, then the text, then the paste end, then Enter — written as one buffer, which is
/// what makes a multi-line message arrive as a single submitted message with no delay needed
/// between the paste and the Enter. It must be `\r`: a raw `\n` inserts a newline instead of
/// submitting.
const PASTE_START: &[u8] = b"\x1b[200~";
const PASTE_END: &[u8] = b"\x1b[201~";
const SUBMIT: &[u8] = b"\r";

pub struct Launch {
    pub session: Arc<LiveSession>,
    /// The agent's own session id when the adapter could pre-allocate it. Codex cannot, and learns
    /// it from its first hook payload instead.
    pub agent_session_id: Option<String>,
}

/// Starts one agent process. `resume_agent_session_id` set means this relaunches an existing agent
/// session rather than starting a new one; the full injection is assembled either way, because
/// hooks are resolved from the launch arguments every time and a resume that omits them silently
/// produces a session nobody observes.
pub fn launch(
    session_id: &str,
    agent: Agent,
    cwd: &Path,
    task: Option<&str>,
    resume_agent_session_id: Option<&str>,
    daemon_port: u16,
    self_exe: &str,
) -> Result<Launch> {
    if !cwd.is_dir() {
        // `portable_pty` falls back to the home directory for a cwd that does not exist, which
        // would silently start a write-capable agent in `$HOME`.
        return Err(anyhow!(
            "`{}` is not a directory the daemon can reach. A packaged application needs file \
             access granted per volume, and the prompt may not have been answered yet.",
            cwd.display()
        ));
    }

    let shell_env = env_shell::snapshot().context("snapshotting the user's shell environment")?;

    let scratch = paths::session_scratch(session_id);
    if scratch.exists() {
        std::fs::remove_dir_all(&scratch).ok();
    }
    std::fs::create_dir_all(&scratch).with_context(|| format!("creating {}", scratch.display()))?;

    let hook_script = adapter::write_hook_script(&scratch, self_exe, session_id, daemon_port)
        .context("writing the session's hook script")?;

    let new_agent_session_id = Uuid::new_v4().to_string();
    let adapter = adapter::adapter_for(agent);
    let spec = LaunchSpec {
        new_agent_session_id: &new_agent_session_id,
        resume_agent_session_id,
        cwd,
        task,
        scratch: &scratch,
        hook_script: &hook_script,
        shell_env: &shell_env,
    };
    let plan = adapter.plan(&spec)?;

    let binary = env_shell::resolve_binary(adapter.binary(), &shell_env)?;

    let pty = native_pty_system()
        .openpty(PtySize {
            rows: INITIAL_ROWS,
            cols: INITIAL_COLS,
            pixel_width: 0,
            pixel_height: 0,
        })
        .context("opening a PTY")?;

    let mut cmd = CommandBuilder::new(&binary);
    cmd.args(&plan.args);
    cmd.env_clear();
    for (key, value) in &shell_env {
        cmd.env(key, value);
    }
    for (key, value) in &plan.env {
        cmd.env(key, value);
    }
    cmd.env("TERM", PTY_TERM);
    cmd.cwd(cwd);

    let child = pty
        .slave
        .spawn_command(cmd)
        .with_context(|| format!("spawning `{binary}`"))?;
    let pid = child
        .process_id()
        .ok_or_else(|| anyhow!("the spawned agent process has no pid"))?;

    let fd = pty
        .master
        .as_raw_fd()
        .ok_or_else(|| anyhow!("the PTY master has no file descriptor"))?;
    ptyio::set_nonblocking(fd).context("putting the PTY master into non-blocking mode")?;

    let agent_session_id = match resume_agent_session_id {
        Some(id) => Some(id.to_string()),
        None if adapter.preallocates_session_id() => Some(new_agent_session_id),
        None => None,
    };

    let live = Arc::new(LiveSession::new(
        session_id.to_string(),
        agent,
        pid,
        fd,
        pty.master,
        child,
        Some(scratch),
    ));
    session::spawn_reader_thread(live.clone(), PTY_READ_BUF);

    Ok(Launch {
        session: live,
        agent_session_id,
    })
}

/// Writes a message into a running session as bracketed paste + Enter.
///
/// The caller is responsible for the gate: nothing may be written while a modal dialog is up,
/// because the paste is discarded but the trailing Enter confirms whatever option is highlighted —
/// at Claude Code's trust dialog that exits the session, and at Grok's approval modal it selects
/// always-approve. Writing mid-turn is safe and is the better path: every agent queues the message
/// itself and consumes it when the turn ends.
pub fn send_message(session: &LiveSession, text: &str) -> Result<()> {
    // A message starting with `/` is executed as a slash command even inside a bracketed paste, so
    // it never reaches the model; a single leading space makes it arrive as ordinary text.
    let text = if text.starts_with('/') {
        format!(" {text}")
    } else {
        text.to_string()
    };

    let mut buf =
        Vec::with_capacity(PASTE_START.len() + text.len() + PASTE_END.len() + SUBMIT.len());
    buf.extend_from_slice(PASTE_START);
    buf.extend_from_slice(text.as_bytes());
    buf.extend_from_slice(PASTE_END);
    buf.extend_from_slice(SUBMIT);
    session.write_input(&buf)?;
    Ok(())
}
