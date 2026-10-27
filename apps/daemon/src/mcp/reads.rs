//! The reads every role has, with one scope: any session reads any session, and any console's
//! projects and archives, whichever its role. `get_session`, `list_sessions`, `list_projects`,
//! `list_consoles` and `list_archived` are here; the other reads every role has
//! (`read_session_history`, `get_session_branches`, `list_agent_models`, `get_account_quotas`)
//! have modules of their own. What a session may *do* to another is narrower, and is each write
//! tool's own rule in `exec`; reads are not limited to the caller's console, writes are.

use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

use crate::protocol::{Console, Project, Role, Session, SessionStatus};
use crate::state::AppState;

use super::args::{
    console_projects, find_console, find_project_in, optional_bool, optional_count,
    optional_string, own_console,
};
use super::exec::{
    branch_names_of, describe_session, owned_count_of, owned_counts_of, resolve_session,
};
use super::settings::{describe_project, refresh_git_status};

/// How much of a session's output `get_session` hands back when the caller names no length. Enough
/// to see what it is doing and how it got there, short of handing the caller a transcript to wade
/// through.
const DEFAULT_OUTPUT_TAIL: usize = 8 * 1024;

/// The most output `get_session` hands back, however much is asked for.
const MAX_OUTPUT_TAIL: usize = 64 * 1024;

/// How many archived sessions `list_archived` returns when the caller names no `limit`: each comes
/// with its whole description, so an unfiltered list of a long-lived install is not handed over
/// whole.
const DEFAULT_ARCHIVED_LIMIT: usize = 50;

/// The most `list_archived` returns, however much is asked for.
const MAX_ARCHIVED_LIMIT: usize = 200;

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
///
/// `project` narrows either to the one project it names (an id, or a name that is unambiguous in
/// that console). `refresh` checks the git status of the projects listed first, and is refused for
/// another console, whose entries carry none.
pub(super) async fn list_projects(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let console = match optional_string(arguments, "console") {
        Some(wanted) => find_console(state, &wanted)?,
        None => own_console(state, caller)?,
    };
    let mut projects = console_projects(state, &console.id)?;
    if let Some(wanted) = optional_string(arguments, "project") {
        let project = find_project_in(state, caller, Some(&console), &wanted)?;
        projects.retain(|listed| listed.id == project.id);
    }
    let refresh = optional_bool(arguments, "refresh")?.unwrap_or(false);
    if console.id != caller.console_id {
        if refresh {
            bail!(
                "`refresh` checks git status, which the projects of another console are listed \
                 without; it applies to your own console's projects only"
            );
        }
        let projects: Vec<Value> = projects
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
    let refreshed = if refresh {
        Some(refresh_git_status(state, projects.clone()).await)
    } else {
        None
    };
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
    let owned = owned_counts_of(state)?;
    let entries = projects
        .iter()
        .map(|project| {
            let live = sessions
                .iter()
                .filter(|session| session.project_id.as_deref() == Some(&project.id))
                .map(|session| describe_session(state, session, caller, &branches, &owned))
                .collect::<Result<Vec<Value>>>()?;
            let mut entry = describe_project(state, project)?;
            entry["sessions"] = json!(live);
            Ok(entry)
        })
        .collect::<Result<Vec<Value>>>()?;
    let mut answer = json!({ "console": caller.console_id, "projects": entries });
    if let Some(refreshed) = refreshed {
        answer["git_refresh"] = refreshed;
    }
    Ok(answer)
}

pub(super) async fn get_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    // Left out, the caller itself: the way a session learns its own Octoboard id and record.
    let target = match optional_string(arguments, "session") {
        Some(wanted) if wanted != caller.id => resolve_session(state, &wanted)?,
        _ => state.session_record(&caller.id)?,
    };
    // Refused outside the bounds rather than clamped, so a caller that asked for more than it can
    // have is not left believing it read it all.
    let tail = optional_count(arguments, "output_bytes", 1, MAX_OUTPUT_TAIL)?
        .unwrap_or(DEFAULT_OUTPUT_TAIL);
    let output = recent_output(state, &target.id, tail).await;
    let branches = branch_names_of(state, vec![target.clone()]).await?;
    let owned = owned_count_of(state, &target)?;
    let mut description = describe_session(state, &target, caller, &branches, &owned)?;
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

/// The `console`, `project` and `owner` filters `list_sessions` and `list_archived` share, resolved
/// once from the call's arguments. Each given one must hold, so they combine as AND; one that
/// names nothing is refused rather than read as a filter that matches nothing.
struct SessionFilters {
    console: Option<Console>,
    project: Option<Project>,
    owner: Option<Session>,
}

impl SessionFilters {
    /// A `project` named beside a `console` is looked for in that console.
    fn parse(
        state: &Arc<AppState>,
        caller: &Session,
        arguments: &Map<String, Value>,
    ) -> Result<Self> {
        let console = optional_string(arguments, "console")
            .map(|wanted| find_console(state, &wanted))
            .transpose()?;
        let project = optional_string(arguments, "project")
            .map(|wanted| find_project_in(state, caller, console.as_ref(), &wanted))
            .transpose()?;
        let owner = optional_string(arguments, "owner")
            .map(|wanted| resolve_session(state, &wanted))
            .transpose()?;
        Ok(Self {
            console,
            project,
            owner,
        })
    }

    fn matches(&self, session: &Session) -> bool {
        self.console
            .as_ref()
            .is_none_or(|console| session.console_id == console.id)
            && self
                .project
                .as_ref()
                .is_none_or(|project| session.project_id.as_deref() == Some(&project.id))
            && self
                .owner
                .as_ref()
                .is_none_or(|owner| session.bound_to.as_deref() == Some(&owner.id))
    }
}

/// The archived sessions of any console, for any role: reads are not limited to the caller's
/// console, as the writes on them are.
///
/// `console`, `project`, `owner`, `yours` and `kind` narrow the list; each given one must hold, so
/// they combine as AND. A `console`, `project` or `owner` that names nothing is refused rather
/// than read as a filter that matches nothing, as is a `kind` of console sessions beside a
/// `project`, which holds only project sessions. The newest archived come first and at most
/// `limit` are described; `total` counts all that match, and `older` those that match and come
/// after the page, which `before` reaches: the `next` the previous page answered with, a position
/// in the order ([`archive_cursor`]) rather than a session, so it holds whatever has become of the
/// session it was taken from.
pub(super) async fn list_archived(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let filters = SessionFilters::parse(state, caller, arguments)?;
    let kind = match optional_string(arguments, "kind").as_deref() {
        None => None,
        Some("console") => Some(Role::Console),
        Some("project") => Some(Role::Project),
        Some(other) => bail!("`kind` must be `console` or `project`, not `{other}`"),
    };
    if filters.project.is_some() && kind == Some(Role::Console) {
        bail!("`kind: console` and `project` cannot both hold: a console session is in no project");
    }
    let yours = optional_bool(arguments, "yours")?;
    let limit = optional_count(arguments, "limit", 1, MAX_ARCHIVED_LIMIT)?
        .unwrap_or(DEFAULT_ARCHIVED_LIMIT);
    let before = optional_string(arguments, "before")
        .map(|wanted| {
            parse_archive_cursor(&wanted).ok_or_else(|| {
                anyhow!(
                    "`before` is not a cursor: pass the `next` of the previous answer unchanged, \
                     or leave `before` out to start from the newest"
                )
            })
        })
        .transpose()?;
    let mut archived: Vec<Session> = state
        .store
        .list_sessions()?
        .into_iter()
        .filter(|session| session.status == SessionStatus::Archived)
        .filter(|session| filters.matches(session))
        .filter(|session| kind.is_none_or(|kind| session.role == kind))
        .filter(|session| {
            yours.is_none_or(|yours| (session.bound_to.as_deref() == Some(&caller.id)) == yours)
        })
        .collect();
    archived.sort_by(archived_order);
    let total = archived.len();
    // The cursor is a position in the order, not a session, so sessions archived, reopened or
    // deleted between two calls move nothing that was already read.
    if let Some((ended_at, id)) = &before {
        archived.retain(|session| {
            archived_position(session.ended_at, &session.id, *ended_at, id).is_gt()
        });
    }
    let older = archived.len().saturating_sub(limit);
    archived.truncate(limit);
    let next = (older > 0)
        .then(|| archived.last().map(archive_cursor))
        .flatten();
    let branches = branch_names_of(state, archived.clone()).await?;
    let owned = owned_counts_of(state)?;
    let sessions = archived
        .iter()
        .map(|session| describe_session(state, session, caller, &branches, &owned))
        .collect::<Result<Vec<Value>>>()?;
    Ok(json!({ "sessions": sessions, "total": total, "older": older, "next": next }))
}

/// The order of the archive: newest archived first (`ended_at` is when it was archived). The id
/// breaks a tie so the order does not move between calls.
fn archived_order(a: &Session, b: &Session) -> std::cmp::Ordering {
    archived_position(a.ended_at, &a.id, b.ended_at, &b.id)
}

/// [`archived_order`] on the two keys it goes by.
fn archived_position(
    a_ended_at: Option<i64>,
    a_id: &str,
    b_ended_at: Option<i64>,
    b_id: &str,
) -> std::cmp::Ordering {
    b_ended_at.cmp(&a_ended_at).then_with(|| a_id.cmp(b_id))
}

/// The position of `session` in the archive's order as a token: its `ended_at` (`-` for none), a
/// colon, and its id. Callers pass it back as `before` without reading into it.
fn archive_cursor(session: &Session) -> String {
    let ended_at = session
        .ended_at
        .map_or_else(|| "-".to_string(), |at| at.to_string());
    format!("{ended_at}:{}", session.id)
}

/// The `ended_at` and id [`archive_cursor`] made `token` of, `None` for a token it did not make.
fn parse_archive_cursor(token: &str) -> Option<(Option<i64>, String)> {
    let (ended_at, id) = token.split_once(':')?;
    let ended_at = match ended_at {
        "-" => None,
        digits => Some(digits.parse().ok()?),
    };
    (!id.is_empty()).then(|| (ended_at, id.to_string()))
}

/// Every running session but the caller (one a sharing change is restarting included), in every
/// console and project, console sessions included: information may go to any of them, and an
/// owner to hand a session to or bind to is one of them. `status` widens or changes that to the
/// interrupted sessions, which have no process but are resumable; archived ones are
/// `list_archived`'s.
///
/// `console`, `project`, `owner` and `yours` narrow the list; each given one must hold, so they
/// combine as AND.
///
/// `pub(crate)` for the tests of `shared_config`, which describe a session being restarted.
pub(crate) async fn list_sessions(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let filters = SessionFilters::parse(state, caller, arguments)?;
    let yours = optional_bool(arguments, "yours")?;
    let (live, interrupted) = match optional_string(arguments, "status").as_deref() {
        None | Some("live") => (true, false),
        Some("interrupted") => (false, true),
        Some("any") => (true, true),
        Some(other) => bail!("`status` must be `live`, `interrupted` or `any`, not `{other}`"),
    };
    let running: Vec<Session> = state
        .store
        .list_sessions()?
        .into_iter()
        .filter(|session| {
            session.id != caller.id
                && match state.shown_status(session) {
                    SessionStatus::Archived => false,
                    SessionStatus::Interrupted => interrupted,
                    _ => live,
                }
        })
        .filter(|session| {
            filters.matches(session)
                && yours
                    .is_none_or(|yours| (session.bound_to.as_deref() == Some(&caller.id)) == yours)
        })
        .collect();
    let branches = branch_names_of(state, running.clone()).await?;
    let owned = owned_counts_of(state)?;
    let sessions = running
        .iter()
        .map(|session| {
            let mut description = describe_session(state, session, caller, &branches, &owned)?;
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
