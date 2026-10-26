//! Launching an agent in a PTY, and writing messages into a running one.

mod image_path;
mod invisible;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use portable_pty::CommandBuilder;
use uuid::Uuid;

use crate::adapter::{self, LaunchSpec};
use crate::branch_origin;
use crate::env_shell;
use crate::paths;
use crate::protocol::{
    error_code, Agent, ApprovalReviewer, ClaudeApprovalMode, CodedError, Notice, Role,
    TerminalAppearance,
};
use crate::pty;
use crate::session::{self, LiveSession, NewSession};

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
    /// Who the session reports to, if anyone; see `LaunchSpec::owner`.
    pub owner: Option<crate::mcp::Owner>,
    /// The project's effective worktree settings; see `LaunchSpec::worktrees`.
    pub worktrees: Option<crate::worktree_settings::WorktreeSettings>,
    /// See `LaunchSpec::ask_questions_as_choices`.
    pub ask_questions_as_choices: bool,
    /// See `LaunchSpec::user_language`.
    pub user_language: Option<String>,
    pub cwd: PathBuf,
    /// Initial task, only on a fresh launch.
    pub task: Option<String>,
    /// Set when this relaunches an existing agent conversation rather than starting a new one.
    pub resume_agent_session_id: Option<String>,
    /// The configuration directory of the session's own agent that it is pinned to; see
    /// `protocol::Session::config_dir`.
    pub config_dir: Option<PathBuf>,
    /// Codex approval reviewer override for this launch.
    pub approval_reviewer: Option<ApprovalReviewer>,
    /// Claude Code permission mode override for this launch.
    pub approval_mode: Option<ClaudeApprovalMode>,
    /// The default account's directory, fully resolved, when the session's account shares it;
    /// see `LaunchSpec::shared_default_dir`.
    pub shared_default_dir: Option<PathBuf>,
    /// The model to launch with; see `LaunchSpec::model`.
    pub model: Option<String>,
    /// The reasoning effort to launch with; see `LaunchSpec::effort`.
    pub effort: Option<String>,
    /// See `LaunchSpec::terminal_appearance`.
    pub terminal_appearance: Option<TerminalAppearance>,
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
        owner,
        worktrees,
        ask_questions_as_choices,
        user_language,
        cwd,
        task,
        resume_agent_session_id,
        config_dir,
        approval_reviewer,
        approval_mode,
        shared_default_dir,
        model,
        effort,
        terminal_appearance,
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
        None => {
            env_shell::launch_snapshot().context("snapshotting the user's shell environment")?
        }
    };

    let scratch = paths::session_scratch(session_id);
    if scratch.exists() {
        std::fs::remove_dir_all(&scratch).ok();
    }
    std::fs::create_dir_all(&scratch).with_context(|| format!("creating {}", scratch.display()))?;

    let adapter = adapter::adapter_for(agent);
    let hook_script = if adapter.uses_hook_script() {
        adapter::write_hook_script(&scratch, &self_exe, session_id, daemon_port)
            .context("writing the session's hook script")?
    } else {
        adapter::hook_script_path(&scratch)
    };

    let new_agent_session_id = Uuid::new_v4().to_string();
    // Resolved ahead of the plan because the agent's version, read from where the binary is
    // installed, decides how some adapters inject; a failure is raised only after the plan, so a
    // plan's own error (a missing pinned directory, say) is the one the user is told about.
    let binary = env_shell::resolve_binary(adapter.binary(), &shell_env);
    let spec = LaunchSpec {
        session_id,
        role,
        owner,
        worktrees,
        ask_questions_as_choices,
        user_language: user_language.as_deref(),
        new_agent_session_id: &new_agent_session_id,
        resume_agent_session_id: resume_agent_session_id.as_deref(),
        cwd,
        task: task.as_deref(),
        scratch: &scratch,
        hook_script: &hook_script,
        shell_env: &shell_env,
        config_dir: config_dir.as_deref(),
        approval_reviewer,
        approval_mode,
        shared_default_dir: shared_default_dir.as_deref(),
        model: model.as_deref(),
        effort: effort.as_deref(),
        terminal_appearance,
        agent_version: binary
            .as_ref()
            .ok()
            .and_then(|binary| adapter.installed_version(binary)),
        self_exe: &self_exe,
        daemon_port,
        mcp_token: &mcp_token,
    };
    let plan = adapter.plan(&spec)?;

    let binary = binary?;

    let mut cmd = CommandBuilder::new(&binary);
    cmd.args(&plan.args);
    cmd.env_clear();
    for (key, value) in &shell_env {
        cmd.env(key, value);
    }
    for (key, value) in &plan.env {
        cmd.env(key, value);
    }
    // A project session's git reports the branches it creates. A console session works in a
    // directory of Octoboard's own and has no project a branch could be attributed in.
    if role == Role::Project {
        let report =
            branch_origin::git_hooks::session_env(&shell_env, &self_exe, session_id, daemon_port);
        for (key, value) in report {
            cmd.env(key, value);
        }
    }
    cmd.env("TERM", pty::TERM);
    cmd.cwd(cwd);

    let spawned = pty::spawn(cmd, pty::INITIAL_ROWS, pty::INITIAL_COLS)
        .with_context(|| format!("spawning `{binary}`"))?;

    let agent_session_id = match resume_agent_session_id {
        Some(id) => Some(id),
        None if adapter.preallocates_session_id() => Some(new_agent_session_id),
        None => None,
    };

    let live = Arc::new(LiveSession::new(NewSession {
        id: session_id.to_string(),
        agent,
        spawned,
        scratch_dir: Some(scratch),
        resolves_approvals_itself: plan.resolves_approvals_itself,
        carried_trust: plan.carried_trust,
        before_scratch_removal: plan
            .shared_config_writeback
            .map(|writeback| Box::new(move || writeback.publish()) as Box<dyn FnOnce() + Send>),
    }));
    session::spawn_reader_thread(live.clone(), pty::READ_BUF);

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
    session.write_input(&frame_message(session.agent, text))
}

/// Shapes a message into the bracketed-paste buffer `send_message` writes: see [`shaped`], then
/// whatever the receiving agent needs on top, then frame. Pulled out as a pure function so the
/// ordering — both strips before the slash guard, not after — is something a test can pin
/// directly, rather than re-deriving it from a copy of this logic; `send_message` itself needs a
/// `LiveSession` and a PTY, which is why it is not tested directly.
pub(crate) fn frame_message(agent: Agent, text: &str) -> Vec<u8> {
    let text = shaped(text);
    // Claude Code reads a pasted piece that ends in an image extension as an image and loses the
    // Enter that follows (see `image_path`). Codex and Grok Build were not measured for it.
    let text = if agent == Agent::Claude {
        image_path::guard_image_paths(&text)
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

/// A message as the agent should read it: control and invisible characters stripped, then a
/// leading slash guarded.
fn shaped(text: &str) -> String {
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
    //
    // The invisible characters go for a different reason: Claude Code holds back a prompt that
    // contains any, and the Enter written after the paste does not send it (see `invisible`).
    // They are stripped before the slash check for the same reason as the controls.
    let text = stripped(text);

    // A message starting with `/` is executed as a slash command even inside a bracketed paste, so
    // it never reaches the model; a single leading space makes it arrive as ordinary text.
    if text.starts_with('/') {
        format!(" {text}")
    } else {
        text
    }
}

/// A message as the command line of an agent launch takes it, or `None` when nothing is left of it.
/// It gets [`shaped`] (the invisible-character strip and the slash guard) but not the image-path
/// guard of `frame_message`, which is for a paste: argv is not one. A prompt that began with `/`
/// would run a slash command, as a pasted one would, and one that began with `-` would be read as
/// a flag; a leading space keeps either as text.
pub(crate) fn launch_prompt(text: &str) -> Option<String> {
    if is_blank_once_stripped(text) {
        return None;
    }
    let text = shaped(text);
    Some(if text.starts_with('-') {
        format!(" {text}")
    } else {
        text
    })
}

/// Whether nothing but whitespace is left of a message once its control and invisible characters
/// are stripped. Such a message would be written as an empty prompt, which no agent sends.
pub(crate) fn is_blank_once_stripped(text: &str) -> bool {
    stripped(text).trim().is_empty()
}

/// The text of a message with what `frame_message` strips taken out.
fn stripped(text: &str) -> String {
    invisible::strip_invisible(&strip_control_chars(text))
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
        let framed = frame_message(Agent::Claude, text);
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
    fn a_message_of_only_controls_invisible_characters_and_spaces_is_blank() {
        assert!(is_blank_once_stripped(
            "\u{2068}\u{2069}\u{1b}\u{7f} \n\t\u{200B}"
        ));
        assert!(!is_blank_once_stripped("\u{2068}ok\u{2069}"));
        assert!(!is_blank_once_stripped("\u{1F9D1}"));
    }

    #[test]
    fn ordinary_multiline_text_with_tabs_survives_framing() {
        let text = "line one\n\tindented line two\nline three";
        let framed = frame_message(Agent::Claude, text);
        let mut expected = Vec::new();
        expected.extend_from_slice(PASTE_START);
        expected.extend_from_slice(text.as_bytes());
        expected.extend_from_slice(PASTE_END);
        expected.extend_from_slice(SUBMIT);
        assert_eq!(framed, expected);
    }

    /// Claude Code holds back a prompt holding bidirectional isolates instead of sending it, so
    /// none may reach the framed buffer — and an isolate in front of a slash command must not keep
    /// it from being guarded, any more than a control character may.
    #[test]
    fn invisible_characters_are_stripped_before_the_slash_guard() {
        let framed = frame_message(Agent::Claude, "\u{2068}/clear\u{2069} the cache");
        let mut expected = Vec::new();
        expected.extend_from_slice(PASTE_START);
        expected.extend_from_slice(b" /clear the cache");
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
        let framed = frame_message(Agent::Claude, text);
        let mut expected = Vec::new();
        expected.extend_from_slice(PASTE_START);
        expected.extend_from_slice(b" /clear");
        expected.extend_from_slice(PASTE_END);
        expected.extend_from_slice(SUBMIT);
        assert_eq!(framed, expected);
    }

    #[test]
    fn only_a_claude_code_message_has_its_image_paths_wrapped() {
        let text = "done\n- /a/shot.png";
        let framed = |agent| String::from_utf8(frame_message(agent, text)).unwrap();
        assert!(framed(Agent::Claude).contains("- `/a/shot.png`\x1b[201~"));
        assert!(framed(Agent::Codex).contains("- /a/shot.png\x1b[201~"));
        assert!(framed(Agent::Grok).contains("- /a/shot.png\x1b[201~"));
    }
}
