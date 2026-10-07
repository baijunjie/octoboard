//! The daemon side of the MCP tools: running one call against the real consoles, projects and
//! sessions.
//!
//! The calling session is resolved from the token the call arrived with, never from an argument,
//! so a child that rewrote its own arguments still cannot act on another session. Which tools it
//! may call follows from that session's role, checked here as well as in the child: the child is a
//! separate process and its announcement is not something the daemon can rely on.
//!
//! Every tool goes through the same coordinator functions the control socket uses. The hub is a
//! second client of the same operations, not a second implementation of them.

use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::coordinator::{self, OpenRequest};
use crate::protocol::{
    now_millis, Agent, Event, Origin, Page, Project, ProjectSource, Role, Session, SessionStatus,
};
use crate::reporting::{self, Delivery, Report, ReportStatus, WhenBlocked};
use crate::state::AppState;

/// How much of a session's output `get_session` hands back. Enough to see what it is doing and how
/// it got there, short of handing the hub a transcript to wade through.
const OUTPUT_TAIL: usize = 8 * 1024;

/// Runs one tool call. The `Err` case is what the model is shown as the reason the call failed, so
/// the messages are written for it rather than for a log.
pub async fn call(
    state: &Arc<AppState>,
    session_id: &str,
    tool: &str,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let session = state.session_record(session_id)?;
    if super::tool_by_name(session.role, tool).is_none() {
        bail!("`{tool}` is not a tool this session can call");
    }

    match tool {
        "list_projects" => list_projects(state, &session),
        "add_project" => add_project(state, &session, arguments).await,
        "start_session" => start_session(state, &session, arguments).await,
        "send_message" => send_message(state, &session, arguments).await,
        "get_session" => get_session(state, &session, arguments),
        "archive_session" => archive_session(state, &session, arguments),
        "list_archived" => list_archived(state, &session, arguments),
        "reopen_session" => reopen_session(state, &session, arguments).await,
        "show_page" => show_page(state, &session, arguments),
        "report" => report(state, &session, arguments).await,
        // Unreachable while the catalogue and this dispatch agree; a tool added to one and not the
        // other should say so rather than look like a refusal.
        _ => bail!("`{tool}` is announced but not implemented"),
    }
}

// -- hub tools ---------------------------------------------------------------

fn list_projects(state: &Arc<AppState>, hub: &Session) -> Result<Value> {
    let projects = console_projects(state, &hub.console_id)?;
    let sessions = state.store.list_sessions()?;
    let entries: Vec<Value> = projects
        .iter()
        .map(|project| {
            let live: Vec<Value> = sessions
                .iter()
                .filter(|session| {
                    session.project_id.as_deref() == Some(&project.id)
                        && !session.status.is_dormant()
                })
                .map(describe_session)
                .collect();
            json!({
                "project": project.id,
                "name": project.name,
                "host": project.host_id,
                "path": project.path,
                "default_agent": project.default_agent,
                "sessions": live,
            })
        })
        .collect();
    Ok(json!({ "projects": entries }))
}

async fn add_project(
    state: &Arc<AppState>,
    hub: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let source = match required_str(arguments, "source")? {
        "local" => ProjectSource::Local,
        "parent" => ProjectSource::Parent,
        "github" => ProjectSource::Github,
        other => bail!("`{other}` is not one of `local`, `parent` or `github`"),
    };
    let added = coordinator::add_project(
        state,
        coordinator::AddProjectRequest {
            console_id: hub.console_id.clone(),
            source,
            path: optional_string(arguments, "path"),
            remote_url: optional_string(arguments, "remote_url"),
            name: optional_string(arguments, "name"),
            default_agent: optional_agent(arguments, "default_agent")?,
            tags: None,
        },
    )
    .await?;
    Ok(json!({
        "projects": added
            .iter()
            .map(|project| json!({
                "project": project.id,
                "name": project.name,
                "path": project.path,
            }))
            .collect::<Vec<_>>(),
    }))
}

async fn start_session(
    state: &Arc<AppState>,
    hub: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let project = resolve_project(state, &hub.console_id, required_str(arguments, "project")?)?;
    let brief = arguments
        .get("brief")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("`brief` is required and must be an object"))?;
    let goal = brief
        .get("goal")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|goal| !goal.is_empty())
        .ok_or_else(|| anyhow!("`brief.goal` is required"))?;
    let task = reporting::render_brief(
        goal,
        brief.get("context").and_then(Value::as_str),
        brief.get("acceptance").and_then(Value::as_str),
        brief.get("constraints").and_then(Value::as_str),
    );

    let session = coordinator::open_session(
        state,
        OpenRequest {
            console_id: hub.console_id.clone(),
            project_id: Some(project.id),
            agent: optional_agent(arguments, "agent")?,
            task: Some(task),
            // Named for the task, not the project: several sessions dispatched into one project
            // would otherwise all carry the project's name and be indistinguishable in the menu.
            title: Some(reporting::title_from_goal(goal)),
            origin: Origin::Hub,
            include_in_hub: true,
        },
    )
    .await?;
    Ok(json!({ "session": session.id, "agent": session.agent }))
}

async fn send_message(
    state: &Arc<AppState>,
    hub: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let target = resolve_session(state, hub, required_str(arguments, "session")?)?;
    if target.role == Role::Hub {
        bail!("this is the hub's own session; say it to the user instead");
    }
    let text = required_str(arguments, "text")?.to_string();
    let delivery = write_off_runtime(state, &target.id, text, WhenBlocked::Queue).await?;
    Ok(json!({
        "delivered": delivery == Delivery::Written,
        "note": match delivery {
            Delivery::Written => "Delivered.",
            // The hub is told rather than refused: queuing is the designed behaviour for a session
            // that is waiting for the user, and nagging it is exactly what it must not do.
            Delivery::Queued =>
                "Queued: this session cannot take a message right now. It will be delivered as \
                 soon as it can, so do not send it again.",
        },
    }))
}

/// Writes into a session from the runtime. The write itself blocks on the PTY for as long as the
/// child is not draining, so it goes to a blocking thread rather than a runtime worker.
async fn write_off_runtime(
    state: &Arc<AppState>,
    id: &str,
    text: String,
    when_blocked: WhenBlocked,
) -> Result<Delivery> {
    let state = state.clone();
    let id = id.to_string();
    tokio::task::spawn_blocking(move || reporting::write_message(&state, &id, &text, when_blocked))
        .await?
}

fn get_session(
    state: &Arc<AppState>,
    hub: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let target = resolve_session(state, hub, required_str(arguments, "session")?)?;
    let output = state
        .live_session(&target.id)
        .map(|live| readable_output(&live.recent_output(OUTPUT_TAIL)));
    let mut description = describe_session(&target);
    description["recent_output"] = json!(output);
    if target.status == SessionStatus::WaitingUser {
        description["note"] = json!(
            "This session is waiting for the user, not for you. Leave it be until they have \
             answered."
        );
    }
    Ok(description)
}

fn archive_session(
    state: &Arc<AppState>,
    hub: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let target = resolve_session(state, hub, required_str(arguments, "session")?)?;
    if target.role == Role::Hub {
        // Including its own: a hub that ended itself would leave its console's project sessions
        // reporting to nothing.
        bail!("a hub session is not the hub's to archive");
    }
    coordinator::archive_session(state, &target.id)?;
    Ok(json!({ "session": target.id, "status": SessionStatus::Archived }))
}

fn list_archived(
    state: &Arc<AppState>,
    hub: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let project = resolve_project(state, &hub.console_id, required_str(arguments, "project")?)?;
    let sessions: Vec<Value> = state
        .store
        .list_sessions()?
        .iter()
        .filter(|session| {
            session.project_id.as_deref() == Some(&project.id)
                && session.status == SessionStatus::Archived
        })
        .map(describe_session)
        .collect();
    Ok(json!({ "sessions": sessions }))
}

async fn reopen_session(
    state: &Arc<AppState>,
    hub: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let target = resolve_session(state, hub, required_str(arguments, "session")?)?;
    // The instruction travels with the relaunch rather than being written after it: whichever hook
    // would release it can fire the moment the agent starts, so the queueing belongs inside the
    // launch, where a refusal also undoes it.
    coordinator::resume_session(
        state,
        &target.id,
        arguments.get("text").and_then(Value::as_str),
    )
    .await?;
    Ok(json!({ "session": target.id }))
}

fn show_page(
    state: &Arc<AppState>,
    hub: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let html = required_str(arguments, "html")?.to_string();
    let page = Page {
        id: Uuid::new_v4().to_string(),
        console_id: hub.console_id.clone(),
        html,
        // No agent exposes a message id to put here yet; the rewind linkage that would read it is
        // not built.
        anchor_message_id: None,
        created_at: now_millis(),
    };
    state.store.insert_page(&page)?;
    state.broadcast(Event::PageCreated { page: page.clone() });
    Ok(json!({ "page": page.id }))
}

// -- project session tools ---------------------------------------------------

async fn report(
    state: &Arc<AppState>,
    session: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let summary = required_str(arguments, "summary")?;
    let status_text = required_str(arguments, "status")?;
    let status = ReportStatus::parse(status_text).ok_or_else(|| {
        anyhow!("`{status_text}` is not one of `done`, `failed` or `needs_decision`")
    })?;
    let open_items: Vec<String> = arguments
        .get("open_items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    // Recorded before delivery: the session is credited with having reported even if the hub cannot
    // take the message yet, so its stop does not also produce a synthesised report.
    state.mark_reported(&session.id);

    let summary = summary.to_string();
    let worker_id = session.id.clone();
    let owned_state = state.clone();
    // Delivery writes into the hub's PTY, which blocks.
    let note = tokio::task::spawn_blocking(move || {
        reporting::deliver_report(
            &owned_state,
            &worker_id,
            Report {
                summary: &summary,
                status,
                open_items: &open_items,
                synthesised: false,
            },
        )
    })
    .await?;
    // Taken back whenever `deliver_report` failed, so the hub gets a synthesised report instead of
    // neither. A failure after the hub already has the report would make that a duplicate, which is
    // the better way round.
    let note = note.inspect_err(|_| state.clear_reported(&session.id))?;
    Ok(json!({ "note": note }))
}

// -- argument and record helpers ---------------------------------------------

fn required_str<'a>(arguments: &'a Map<String, Value>, key: &str) -> Result<&'a str> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("`{key}` is required"))
}

fn optional_string(arguments: &Map<String, Value>, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn optional_agent(arguments: &Map<String, Value>, key: &str) -> Result<Option<Agent>> {
    match optional_string(arguments, key) {
        None => Ok(None),
        Some(value) => match value.as_str() {
            "claude" => Ok(Some(Agent::Claude)),
            "codex" => Ok(Some(Agent::Codex)),
            "grok" => Ok(Some(Agent::Grok)),
            other => bail!("`{other}` is not one of `claude`, `codex` or `grok`"),
        },
    }
}

fn console_projects(state: &Arc<AppState>, console_id: &str) -> Result<Vec<Project>> {
    Ok(state
        .store
        .list_projects()?
        .into_iter()
        .filter(|project| project.console_id == console_id)
        .collect())
}

/// Finds a project by id, or by name when that names exactly one. The hub works from what
/// `list_projects` told it, which is both, and a name it half-remembers must not resolve to
/// whichever project happens to sort first.
fn resolve_project(state: &Arc<AppState>, console_id: &str, wanted: &str) -> Result<Project> {
    let projects = console_projects(state, console_id)?;
    if let Some(project) = projects.iter().find(|project| project.id == wanted) {
        return Ok(project.clone());
    }
    let matches: Vec<&Project> = projects
        .iter()
        .filter(|project| project.name == wanted)
        .collect();
    match matches.as_slice() {
        [project] => Ok((*project).clone()),
        [] => bail!("this console has no project `{wanted}`"),
        _ => bail!(
            "`{wanted}` names {} projects in this console; use the project's id instead",
            matches.len()
        ),
    }
}

/// Finds a session the hub is allowed to act on: one of its own console's. A session id from
/// another console is refused rather than acted on, so one console's hub cannot reach into
/// another's.
fn resolve_session(state: &Arc<AppState>, hub: &Session, wanted: &str) -> Result<Session> {
    let session = state
        .store
        .get_session(wanted)?
        .ok_or_else(|| anyhow!("there is no session `{wanted}`"))?;
    if session.console_id != hub.console_id {
        bail!("session `{wanted}` belongs to another console");
    }
    Ok(session)
}

fn describe_session(session: &Session) -> Value {
    json!({
        "session": session.id,
        "title": session.title,
        "agent": session.agent,
        "status": session.status,
        "project": session.project_id,
        // Whether this session reports to the hub. A session the user opened by hand and kept out
        // of the orchestration is theirs, not the hub's to drive.
        "include_in_hub": session.include_in_hub,
        "started_at": session.started_at,
        "ended_at": session.ended_at,
    })
}

/// Makes raw PTY output readable: escape sequences and the cursor-control bytes an agent's
/// renderer emits by the thousand carry no information once the frames are gone, and left in they
/// are most of what the hub would be reading.
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
