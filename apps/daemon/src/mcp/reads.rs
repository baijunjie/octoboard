//! The reads every role has, with one scope: any session reads any session, and any console's
//! projects and archives, whichever its role. `get_session`, `list_sessions`, `list_projects`,
//! `list_consoles` and `list_archived` are here; the other reads every role has
//! (`read_session_history`, `get_session_branches`, `list_agent_models`, `get_account_quotas`)
//! have modules of their own. What a session may *do* to another is narrower, and is each write
//! tool's own rule in `exec`; reads are not limited to the caller's console, writes are.

use std::sync::Arc;

use anyhow::Result;
use serde_json::{json, Map, Value};

use crate::protocol::{Session, SessionStatus};
use crate::state::AppState;

use super::args::{
    console_projects, find_console, find_project, optional_bool, optional_count, optional_string,
    required_str,
};
use super::exec::{branch_names_of, describe_session, resolve_session};
use super::settings::describe_project;

/// How much of a session's output `get_session` hands back when the caller names no length. Enough
/// to see what it is doing and how it got there, short of handing the caller a transcript to wade
/// through.
const DEFAULT_OUTPUT_TAIL: usize = 8 * 1024;

/// The most output `get_session` hands back, however much is asked for.
const MAX_OUTPUT_TAIL: usize = 64 * 1024;

/// Every console's id and name, and which is the caller's own.
pub(super) fn list_consoles(state: &Arc<AppState>, caller: &Session) -> Result<Value> {
    let consoles: Vec<Value> = state
        .store
        .list_consoles()?
        .iter()
        .map(|console| {
            json!({
                "console": console.id,
                "name": console.name,
                "yours": console.id == caller.console_id,
            })
        })
        .collect();
    Ok(json!({ "consoles": consoles }))
}

/// The caller's own console's projects with their running sessions, or, for another console named
/// by `console`, only what is needed to target one of its projects: each project's id, name,
/// description and tags. Reads reach any console; what a call changes stays in the caller's own.
pub(super) async fn list_projects(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    if let Some(wanted) = optional_string(arguments, "console") {
        let console = find_console(state, &wanted)?;
        if console.id != caller.console_id {
            let projects: Vec<Value> = console_projects(state, &console.id)?
                .iter()
                .map(|project| {
                    json!({
                        "project": project.id,
                        "name": project.name,
                        "description": project.description,
                        "tags": project.tags,
                    })
                })
                .collect();
            return Ok(json!({ "console": console.id, "projects": projects }));
        }
    }
    let projects = console_projects(state, &caller.console_id)?;
    let sessions: Vec<Session> = state
        .store
        .list_sessions()?
        .into_iter()
        .filter(|session| {
            !session.status.is_dormant()
                && projects
                    .iter()
                    .any(|project| session.project_id.as_deref() == Some(&project.id))
        })
        .collect();
    let branches = branch_names_of(state, sessions.clone()).await?;
    let entries = projects
        .iter()
        .map(|project| {
            let live = sessions
                .iter()
                .filter(|session| session.project_id.as_deref() == Some(&project.id))
                .map(|session| describe_session(state, session, caller, &branches))
                .collect::<Result<Vec<Value>>>()?;
            let mut entry = describe_project(state, project)?;
            entry["sessions"] = json!(live);
            Ok(entry)
        })
        .collect::<Result<Vec<Value>>>()?;
    Ok(json!({ "console": caller.console_id, "projects": entries }))
}

pub(super) async fn get_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let target = resolve_session(state, required_str(arguments, "session")?)?;
    // Refused outside the bounds rather than clamped, so a caller that asked for more than it can
    // have is not left believing it read it all.
    let tail = optional_count(arguments, "output_bytes", 1, MAX_OUTPUT_TAIL)?
        .unwrap_or(DEFAULT_OUTPUT_TAIL);
    let output = recent_output(state, &target.id, tail).await;
    let branches = branch_names_of(state, vec![target.clone()]).await?;
    let mut description = describe_session(state, &target, caller, &branches)?;
    description["recent_output"] = json!(output);
    if state.shown_status(&target) == SessionStatus::WaitingUser {
        description["note"] = json!(
            "This session is waiting for the user, not for you. Leave it be until they have \
             answered."
        );
    }
    Ok(description)
}

/// The last `tail` bytes of what a session has printed, readable. A running session's comes from
/// its ring buffer; one whose process has ended has the ring buffer's last contents kept, which is
/// what `GET /ws/term/:session` replays, so the same path is read here. `None` when nothing was
/// kept — no process of it has printed anything yet, it ended with the daemon killed outright
/// before the output could be kept, or a new process took it over and discarded the old output at
/// launch.
async fn recent_output(state: &Arc<AppState>, id: &str, tail: usize) -> Option<String> {
    if let Some(live) = state.live_session(id) {
        return Some(readable_output(&live.recent_output(tail)));
    }
    // A file read of up to the whole ring buffer, so not on a runtime worker.
    let state = state.clone();
    let id = id.to_string();
    // A failed read is no output rather than an error: it must not take the session's status and
    // ownership down with it.
    let saved = tokio::task::spawn_blocking(move || state.saved_output(&id))
        .await
        .unwrap_or(None);
    saved.map(|output| {
        let from = output.len().saturating_sub(tail);
        readable_output(&output[from..])
    })
}

/// The archived sessions of a project of any console, for any role: reads are not limited to the
/// caller's console, as the writes on them are.
pub(super) async fn list_archived(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let project = find_project(state, caller, required_str(arguments, "project")?)?;
    let archived: Vec<Session> = state
        .store
        .list_sessions()?
        .into_iter()
        .filter(|session| {
            session.project_id.as_deref() == Some(&project.id)
                && session.status == SessionStatus::Archived
        })
        .collect();
    let branches = branch_names_of(state, archived.clone()).await?;
    let sessions = archived
        .iter()
        .map(|session| describe_session(state, session, caller, &branches))
        .collect::<Result<Vec<Value>>>()?;
    Ok(json!({ "sessions": sessions }))
}

/// Every running session but the caller (one a sharing change is restarting included), in every
/// console and project, console sessions included: information may go to any of them, and an
/// owner to hand a session to or bind to is one of them.
///
/// `console`, `project`, `owner` and `yours` narrow the list; each given one must hold, so they
/// combine as AND. A `console`, `project` or `owner` that names nothing is refused rather than
/// read as a filter that matches nothing.
///
/// `pub(crate)` for the tests of `shared_config`, which describe a session being restarted.
pub(crate) async fn list_sessions(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let console = optional_string(arguments, "console")
        .map(|wanted| find_console(state, &wanted))
        .transpose()?;
    let project = optional_string(arguments, "project")
        .map(|wanted| find_project(state, caller, &wanted))
        .transpose()?;
    let owner = optional_string(arguments, "owner")
        .map(|wanted| resolve_session(state, &wanted))
        .transpose()?;
    let yours = optional_bool(arguments, "yours")?;
    let running: Vec<Session> = state
        .store
        .list_sessions()?
        .into_iter()
        .filter(|session| session.id != caller.id && !state.shown_status(session).is_dormant())
        .filter(|session| {
            console
                .as_ref()
                .is_none_or(|console| session.console_id == console.id)
                && project
                    .as_ref()
                    .is_none_or(|project| session.project_id.as_deref() == Some(&project.id))
                && owner
                    .as_ref()
                    .is_none_or(|owner| session.bound_to.as_deref() == Some(&owner.id))
                && yours
                    .is_none_or(|yours| (session.bound_to.as_deref() == Some(&caller.id)) == yours)
        })
        .collect();
    let branches = branch_names_of(state, running.clone()).await?;
    let sessions = running
        .iter()
        .map(|session| {
            let mut description = describe_session(state, session, caller, &branches)?;
            description["your_owner"] = json!(caller.bound_to.as_deref() == Some(&session.id));
            Ok(description)
        })
        .collect::<Result<Vec<Value>>>()?;
    Ok(json!({ "your_owner": caller.bound_to, "sessions": sessions }))
}

/// Makes raw PTY output readable: escape sequences and the cursor-control bytes an agent's
/// renderer emits by the thousand carry no information once the frames are gone, and left in they
/// are most of what the console session would be reading.
fn readable_output(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\x1b' => match chars.peek() {
                // CSI: parameters and intermediates, then one final byte in `@`..`~`.
                Some('[') => {
                    chars.next();
                    while let Some(&next) = chars.peek() {
                        chars.next();
                        if ('@'..='~').contains(&next) {
                            break;
                        }
                    }
                }
                // OSC and the other string-introducing sequences run to a terminator rather than
                // to a single final byte.
                Some(']') | Some('P') | Some('X') | Some('^') | Some('_') => {
                    chars.next();
                    while let Some(next) = chars.next() {
                        if next == '\x07' {
                            break;
                        }
                        if next == '\x1b' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                // Anything else is a two-byte escape; drop both.
                Some(_) => {
                    chars.next();
                }
                None => {}
            },
            '\n' | '\t' => out.push(ch),
            '\r' => {}
            ch if (ch as u32) < 0x20 || ch as u32 == 0x7f => {}
            ch => out.push(ch),
        }
    }
    // Carriage-return redrawing leaves runs of blank lines behind once the escapes are gone.
    let mut collapsed = String::with_capacity(out.len());
    let mut blanks = 0;
    for line in out.lines() {
        if line.trim().is_empty() {
            blanks += 1;
            if blanks > 1 {
                continue;
            }
        } else {
            blanks = 0;
        }
        collapsed.push_str(line.trim_end());
        collapsed.push('\n');
    }
    collapsed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_sequences_and_control_bytes_are_stripped_from_the_output_tail() {
        let raw = b"\x1b[2J\x1b[1;32mbuilding\x1b[0m\r\n\x1b]0;title\x07done\n";
        assert_eq!(readable_output(raw), "building\ndone\n");
    }

    #[test]
    fn runs_of_blank_lines_collapse() {
        assert_eq!(readable_output(b"a\n\n\n\n b \n"), "a\n\n b\n");
    }

    /// Partial UTF-8 is normal: the tail starts at a byte offset into the ring buffer, not at a
    /// character boundary.
    #[test]
    fn a_tail_cut_mid_character_still_reads() {
        let mut raw = "ok ".as_bytes().to_vec();
        raw.extend_from_slice(&"日".as_bytes()[..2]);
        assert!(readable_output(&raw).starts_with("ok "));
    }
}
