//! Launching an agent in a PTY, and writing messages into a running one.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use uuid::Uuid;

use crate::adapter::{self, LaunchSpec};
use crate::env_shell;
use crate::paths;
use crate::protocol::{error_code, Agent, CodedError, Notice, Role};
use crate::ptyio;
use crate::session::{self, LiveSession, NewSession};

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
    /// Something about this launch the user has to be told; see `adapter::LaunchPlan::notice`.
    pub notice: Option<Notice>,
}

/// One agent process to start. Owned rather than borrowed: launching snapshots the user's shell
/// environment and forks, so it runs on a blocking thread and the values have to travel to it.
pub struct LaunchRequest {
    pub session_id: String,
    pub agent: Agent,
    pub role: Role,
    /// Whether the session reports to a console session; see `LaunchSpec::bound`.
    pub bound: bool,
    pub cwd: PathBuf,
    /// Initial task, only on a fresh launch.
    pub task: Option<String>,
    /// Set when this relaunches an existing agent conversation rather than starting a new one.
    pub resume_agent_session_id: Option<String>,
    /// The configuration directory of the session's own agent that it is pinned to; see
    /// `protocol::Session::config_dir`.
    pub config_dir: Option<PathBuf>,
    /// A shell environment already snapshotted for this launch, used as it is instead of taking
    /// another. Only a caller that has already resolved something from one passes it — a switch
    /// to the default account, which resolved that account's directory from it to copy the
    /// conversation there and must launch against the same value.
    pub preresolved_shell_env: Option<HashMap<String, String>>,
    pub daemon_port: u16,
    pub self_exe: String,
    pub mcp_token: String,
}

/// Starts one agent process. A resume assembles the full injection just as a fresh launch does,
/// because hooks and the MCP server are resolved from the launch arguments every time and a resume
/// that omits them silently produces a session nobody observes.
pub fn launch(request: LaunchRequest) -> Result<Launch> {
    let LaunchRequest {
        session_id,
        agent,
        role,
        bound,
        cwd,
        task,
        resume_agent_session_id,
        config_dir,
        preresolved_shell_env,
        daemon_port,
        self_exe,
        mcp_token,
    } = request;
    let session_id = session_id.as_str();
    let cwd = cwd.as_path();
    if !cwd.is_dir() {
        // `portable_pty` falls back to the home directory for a cwd that does not exist, which
        // would silently start a write-capable agent in `$HOME`.
        return Err(CodedError::raised(
            error_code::DIRECTORY_UNREACHABLE,
            format!(
                "`{}` is not a directory the daemon can reach. A packaged application needs file \
                 access granted per volume, and the prompt may not have been answered yet.",
                cwd.display()
            ),
            &[("path", &cwd.to_string_lossy())],
        ));
    }

    let shell_env = match preresolved_shell_env {
        Some(shell_env) => shell_env,
        None => env_shell::snapshot().context("snapshotting the user's shell environment")?,
    };

    let scratch = paths::session_scratch(session_id);
    if scratch.exists() {
        std::fs::remove_dir_all(&scratch).ok();
    }
    std::fs::create_dir_all(&scratch).with_context(|| format!("creating {}", scratch.display()))?;

    let hook_script = adapter::write_hook_script(&scratch, &self_exe, session_id, daemon_port)
        .context("writing the session's hook script")?;

    let new_agent_session_id = Uuid::new_v4().to_string();
    let adapter = adapter::adapter_for(agent);
    let spec = LaunchSpec {
        session_id,
        role,
        bound,
        new_agent_session_id: &new_agent_session_id,
        resume_agent_session_id: resume_agent_session_id.as_deref(),
        cwd,
        task: task.as_deref(),
        scratch: &scratch,
        hook_script: &hook_script,
        shell_env: &shell_env,
        config_dir: config_dir.as_deref(),
        self_exe: &self_exe,
        daemon_port,
        mcp_token: &mcp_token,
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
        Some(id) => Some(id),
        None if adapter.preallocates_session_id() => Some(new_agent_session_id),
        None => None,
    };

    let live = Arc::new(LiveSession::new(NewSession {
        id: session_id.to_string(),
        agent,
        pid,
        fd,
        master: pty.master,
        child,
        scratch_dir: Some(scratch),
        resolves_approvals_itself: plan.resolves_approvals_itself,
    }));
    session::spawn_reader_thread(live.clone(), PTY_READ_BUF);

    Ok(Launch {
        session: live,
        agent_session_id,
        notice: plan.notice,
    })
}

/// What is said about a message the agent only partly accepted, to the sender whose call hit it and
/// to the user. One wording, because they are the same fact seen from two places.
///
/// It names no reader and promises no recovery: what state the input line is actually left in, and
/// what it takes to clear it, is not established for any of the three agents. What *is* known follows
/// from the bracketed-paste contract this whole mechanism already rests on — a receiver accumulates
/// until the end marker, which is why a multi-line message arrives as one — so the next message
/// Octoboard writes closes the fragment and is submitted along with it. The damage is one spoiled
/// message, not a session that stays broken.
pub const FRAGMENT_HAZARD: &str = concat!(
    "the session's input line may be holding part of a message ",
    env!("OCTOBOARD_APP_NAME"),
    " could not finish writing. The next message written into the session will be run \
     together with it, so check the session before sending anything else."
);

/// Writes a message into a running session as bracketed paste + Enter.
///
/// A failure's `written` counts bytes of the **framed** buffer, not of `text`, so `written == 0` is
/// the one case where nothing of the message reached the session. Any other count may have left an
/// unterminated paste in the input line, or — for a count short of the paste introducer — a
/// truncated escape sequence; either way the message may not be written again.
///
/// The caller is responsible for the gate: nothing may be written while a modal dialog is up,
/// because the paste is discarded but the trailing Enter confirms whatever option is highlighted —
/// at Claude Code's trust dialog that exits the session, and at Grok's approval modal it selects
/// always-approve. Writing mid-turn is safe and is the better path: every agent queues the message
/// itself and consumes it when the turn ends.
pub fn send_message(session: &LiveSession, text: &str) -> Result<(), crate::ptyio::PartialWrite> {
    session.write_input(&frame_message(text))
}

/// Shapes a message into the bracketed-paste buffer `send_message` writes: strip controls, guard
/// a leading slash, then frame. Pulled out as a pure function so the ordering between the two
/// steps — strip before guard, not after — is something a test can pin directly, rather than
/// re-deriving it from a copy of this logic; `send_message` itself needs a `LiveSession` and a PTY,
/// which is why it is not tested directly.
fn frame_message(text: &str) -> Vec<u8> {
    // `text` can be model-authored (a report panel submission, a project session's report summary)
    // and is never reviewed before it is written here. Without this, a value containing the
    // paste-end marker (`ESC[201~`) would close the bracketed paste early, and everything the
    // attacker put after it would be delivered to the agent's TUI as raw input — control
    // sequences and `\r` included, i.e. arbitrary keystrokes. Stripping every Cc control character
    // removes ESC along with it, so no embedded escape sequence can survive into the framed
    // buffer.
    //
    // This has to run before the slash-command check below: a control character prepended to
    // `/clear` makes the raw text not start with `/`, but stripping it bare would still leave
    // `/clear` for the agent to execute as a command. Checking the stripped text instead closes
    // that gap.
    let text = strip_control_chars(text);

    // A message starting with `/` is executed as a slash command even inside a bracketed paste, so
    // it never reaches the model; a single leading space makes it arrive as ordinary text.
    let text = if text.starts_with('/') {
        format!(" {text}")
    } else {
        text
    };

    let mut buf =
        Vec::with_capacity(PASTE_START.len() + text.len() + PASTE_END.len() + SUBMIT.len());
    buf.extend_from_slice(PASTE_START);
    buf.extend_from_slice(text.as_bytes());
    buf.extend_from_slice(PASTE_END);
    buf.extend_from_slice(SUBMIT);
    buf
}

/// Removes every Unicode `Cc` control character — C0, DEL and C1 — except `\n` and `\t`, which
/// prose legitimately uses. See the comment at `frame_message`'s call site for why this exists.
fn strip_control_chars(text: &str) -> String {
    text.chars()
        .filter(|&c| !c.is_control() || c == '\n' || c == '\t')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An embedded paste-end marker must not survive into the framed buffer at all — not just be
    /// absent from the stripped text — since the buffer written into the PTY is what an attacker
    /// would actually need to end the paste early. `frame_message` adds its own closing
    /// `PASTE_END`, so this counts occurrences in the whole buffer rather than just checking the
    /// marker's absence from the input.
    #[test]
    fn an_embedded_paste_end_marker_does_not_survive() {
        let text = "before\x1b[201~\rafter";
        let framed = frame_message(text);
        let occurrences = framed
            .windows(PASTE_END.len())
            .filter(|window| *window == PASTE_END)
            .count();
        assert_eq!(
            occurrences, 1,
            "exactly the trailing PASTE_END, none embedded"
        );
    }

    #[test]
    fn ordinary_multiline_text_with_tabs_survives_framing() {
        let text = "line one\n\tindented line two\nline three";
        let framed = frame_message(text);
        let mut expected = Vec::new();
        expected.extend_from_slice(PASTE_START);
        expected.extend_from_slice(text.as_bytes());
        expected.extend_from_slice(PASTE_END);
        expected.extend_from_slice(SUBMIT);
        assert_eq!(framed, expected);
    }

    /// A control character prepended to a slash command must not let it slip past the
    /// leading-space guard: `frame_message` strips controls before checking for `/`, so the
    /// stripped text still gets space-prefixed and reaches the model as text, not as a command.
    /// Asserting on the framed buffer, rather than re-deriving the strip-then-guard order in the
    /// test body, is what would actually catch that order being swapped back in `frame_message`.
    #[test]
    fn a_control_character_before_a_slash_command_still_gets_guarded() {
        let text = "\u{1}/clear";
        let framed = frame_message(text);
        let mut expected = Vec::new();
        expected.extend_from_slice(PASTE_START);
        expected.extend_from_slice(b" /clear");
        expected.extend_from_slice(PASTE_END);
        expected.extend_from_slice(SUBMIT);
        assert_eq!(framed, expected);
    }
}
