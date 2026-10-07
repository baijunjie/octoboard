//! The coordinator role: what each control-socket request does to the stored consoles, projects and
//! sessions, and which host-role work it triggers — including the launch flow every way of starting
//! a session goes through.
//!
//! Writing into a running session and everything built on it (the hub's reports, synthesis,
//! automatic archiving) is `crate::reporting`'s, because the hub's tools and the hook callback reach
//! it without going through a control-socket request at all.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use uuid::Uuid;

use crate::hostfs;
use crate::mcp;
use crate::paths;
use crate::protocol::{
    error_code, now_millis, Agent, CodedError, Console, Event, Origin, Project, ProjectSource,
    RequestBody, Role, Session, SessionStatus,
};
use crate::reporting;
use crate::state::AppState;
use crate::store::LOCAL_HOST_ID;
use crate::term;
use crate::trust;

/// Handles one request. The returned event, when there is one, is the answer to that request and
/// goes to the asking socket only; everything that changed state has already been broadcast.
pub async fn handle(
    state: &Arc<AppState>,
    request_id: Option<String>,
    body: RequestBody,
) -> Result<Option<Event>> {
    match body {
        RequestBody::CreateConsole {
            name,
            hub_agent,
            default_agent,
            claude_config_dir,
            codex_config_dir,
            grok_config_dir,
            icon,
        } => {
            // Validated before anything is created, so a refused request leaves no working
            // directory behind.
            let claude_config_dir =
                normalize_config_dir(Agent::Claude, claude_config_dir.as_deref())?;
            let codex_config_dir = normalize_config_dir(Agent::Codex, codex_config_dir.as_deref())?;
            let grok_config_dir = normalize_config_dir(Agent::Grok, grok_config_dir.as_deref())?;
            let icon = normalize_icon(icon.as_deref())?;
            let id = Uuid::new_v4().to_string();
            let workdir = paths::console_workdir(&id);
            std::fs::create_dir_all(&workdir)?;
            let console = Console {
                id,
                name,
                workdir: workdir.to_string_lossy().into_owned(),
                hub_agent,
                default_agent,
                claude_config_dir,
                codex_config_dir,
                grok_config_dir,
                icon,
                created_at: now_millis(),
            };
            mcp::role::write_hub_instructions(&console)?;
            state.store.insert_console(&console)?;
            state.broadcast(Event::ConsoleUpserted { console });
            Ok(None)
        }

        RequestBody::UpdateConsole {
            console: id,
            name,
            hub_agent,
            default_agent,
            claude_config_dir,
            codex_config_dir,
            grok_config_dir,
            icon,
        } => {
            let mut console = state
                .store
                .get_console(&id)?
                .ok_or_else(|| CodedError::unknown_console(&id))?;
            if let Some(name) = name {
                console.name = name;
            }
            if let Some(hub_agent) = hub_agent {
                console.hub_agent = hub_agent;
            }
            if let Some(default_agent) = default_agent {
                console.default_agent = default_agent;
            }
            // Only sessions opened afterwards take a new value; each existing one keeps the
            // directory it was started with (see `Session::config_dir`).
            if let Some(dir) = claude_config_dir {
                console.claude_config_dir = normalize_config_dir(Agent::Claude, dir.as_deref())?;
            }
            if let Some(dir) = codex_config_dir {
                console.codex_config_dir = normalize_config_dir(Agent::Codex, dir.as_deref())?;
            }
            if let Some(dir) = grok_config_dir {
                console.grok_config_dir = normalize_config_dir(Agent::Grok, dir.as_deref())?;
            }
            if let Some(icon) = icon {
                console.icon = normalize_icon(icon.as_deref())?;
            }
            // Rewritten rather than left alone: the file is named for the hub's agent, so a
            // console that changed agents would otherwise keep reading the old one's.
            mcp::role::write_hub_instructions(&console)?;
            state.store.update_console(&console)?;
            state.broadcast(Event::ConsoleUpserted { console });
            Ok(None)
        }

        RequestBody::DeleteConsole { console: id } => {
            let console = state
                .store
                .get_console(&id)?
                .ok_or_else(|| CodedError::unknown_console(&id))?;
            if state.has_live_sessions_where(|session| session.console_id == id)? {
                return Err(CodedError::raised(
                    error_code::CONSOLE_HAS_RUNNING_SESSIONS,
                    "this console still has running or starting sessions; archive the running ones, or wait for the starting ones to finish",
                    &[],
                ));
            }
            state.store.delete_console(&id)?;
            // The working directory is Octoboard's own, under `~/.octoboard/consoles/`; no project
            // directory is ever touched by this.
            if let Err(err) = std::fs::remove_dir_all(&console.workdir) {
                if err.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(workdir = %console.workdir, %err, "removing the console working directory failed");
                }
            }
            state.broadcast(Event::ConsoleDeleted { console: id });
            Ok(None)
        }

        RequestBody::AddProject {
            console_id,
            source,
            path,
            remote_url,
            name,
            default_agent,
        } => {
            add_project(
                state,
                console_id,
                source,
                path,
                remote_url,
                name,
                default_agent,
            )
            .await?;
            Ok(None)
        }

        RequestBody::UpdateProject {
            project: id,
            name,
            default_agent,
            pinned,
        } => {
            let mut project = state
                .store
                .get_project(&id)?
                .ok_or_else(|| CodedError::unknown_project(&id))?;
            if let Some(name) = name {
                project.name = name;
            }
            // Absent means "leave it alone", an explicit null means "go back to inheriting the
            // console's default" — the two have to stay distinguishable, or a project's own
            // default could be set but never cleared.
            if let Some(default_agent) = default_agent {
                project.default_agent = default_agent;
            }
            if let Some(pinned) = pinned {
                project.pinned = pinned;
            }
            state.store.update_project(&project)?;
            state.broadcast(Event::ProjectUpserted { project });
            Ok(None)
        }

        RequestBody::DeleteProject {
            project: id,
            stop_sessions,
        } => {
            state
                .store
                .get_project(&id)?
                .ok_or_else(|| CodedError::unknown_project(&id))?;
            let in_project = |session: &Session| session.project_id.as_deref() == Some(&id);
            // A session still being launched has no process to stop, so it refuses either way.
            let refused = if stop_sessions {
                state.has_launching_sessions_where(in_project)?
            } else {
                state.has_live_sessions_where(in_project)?
            };
            if refused {
                return Err(CodedError::raised(
                    error_code::PROJECT_HAS_RUNNING_SESSIONS,
                    "this project still has running or starting sessions; archive the running ones, or wait for the starting ones to finish",
                    &[],
                ));
            }
            if stop_sessions {
                // Archived the way the user's own archive request does it; a process still
                // exiting when its rows are gone is tolerated by `watch_exit` and the hook paths.
                for live in state.live_sessions() {
                    if state
                        .store
                        .get_session(&live.id)?
                        .is_some_and(|s| in_project(&s))
                    {
                        archive_session(state, &live.id)?;
                    }
                }
            }
            // Only the association goes away. The directory is the user's.
            state.store.delete_project(&id)?;
            state.broadcast(Event::ProjectDeleted { project: id });
            Ok(None)
        }

        RequestBody::ListDir { path } => {
            let expanded = hostfs::expand(&path);
            let entries = hostfs::list_dir(&expanded)?;
            Ok(Some(Event::DirListing {
                id: request_id,
                path: expanded.to_string_lossy().into_owned(),
                entries,
            }))
        }

        RequestBody::OpenSession {
            console_id,
            project_id,
            agent,
            task,
            title,
            include_in_hub,
        } => {
            let session = open_session(
                state,
                OpenRequest {
                    console_id,
                    project_id,
                    agent,
                    task,
                    title,
                    origin: Origin::User,
                    include_in_hub,
                },
            )
            .await?;
            Ok(Some(Event::SessionOpened {
                id: request_id,
                session,
            }))
        }

        RequestBody::ResumeSession { session } => {
            resume_session(state, &session, None).await?;
            Ok(None)
        }

        RequestBody::ArchiveSession { session } => {
            archive_session(state, &session)?;
            Ok(None)
        }

        RequestBody::DeleteSession { session } => {
            delete_session(state, &session)?;
            Ok(None)
        }

        RequestBody::DeleteArchivedSessions { console, project } => {
            delete_archived_sessions(state, &console, project.as_deref())?;
            Ok(None)
        }

        RequestBody::SetSessionPinned {
            session: id,
            pinned,
        } => {
            state.store.set_session_pinned(&id, pinned)?;
            state.publish_session(&state.session_record(&id)?);
            Ok(None)
        }

        RequestBody::SendMessage { session, text } => {
            send_message(state, &session, &text)?;
            Ok(None)
        }

        RequestBody::RenameSession { session: id, title } => {
            let mut session = state.session_record(&id)?;
            session.title = title;
            state.save_session(&session)?;
            Ok(None)
        }

        RequestBody::ListPages { console } => {
            state
                .store
                .get_console(&console)?
                .ok_or_else(|| CodedError::unknown_console(&console))?;
            let pages = state.store.list_pages(&console)?;
            Ok(Some(Event::PageList {
                id: request_id,
                console_id: console,
                pages,
            }))
        }

        RequestBody::SubmitPage {
            page: page_id,
            data,
        } => {
            submit_page(state, &page_id, data).await?;
            Ok(None)
        }

        RequestBody::ConfirmClaudeTrust {
            session,
            remember,
            trust_parent_dir,
        } => {
            trust::confirm(state, &session, remember, trust_parent_dir).await?;
            Ok(None)
        }

        RequestBody::RemoveTrustedDirectory { path } => {
            trust::remove_trusted_directory(state, &path)?;
            Ok(None)
        }

        RequestBody::Shutdown => {
            state.request_shutdown();
            Ok(None)
        }
    }
}

/// A request that lacks a field its kind needs. `field` is the wire name, passed as is.
fn field_required(field: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::FIELD_REQUIRED,
        format!("`{field}` is required"),
        &[("field", field)],
    )
}

/// What the user typed for a project's directory, as the absolute, lexically normalised path that is
/// stored. A relative path is refused: it would mean a different directory whenever the daemon's own
/// working directory differed, and nothing that compares project paths — the trusted directories
/// among them — could read it.
fn absolute_path(text: &str) -> Result<PathBuf> {
    let path = hostfs::expand(text);
    if !path.is_absolute() {
        return Err(CodedError::raised(
            error_code::PATH_NOT_ABSOLUTE,
            format!("`{text}` is not an absolute path; give the full path or start it with `~/`"),
            &[("path", text)],
        ));
    }
    Ok(hostfs::lexically_normalise(&path))
}

pub async fn add_project(
    state: &Arc<AppState>,
    console_id: String,
    source: ProjectSource,
    path: Option<String>,
    remote_url: Option<String>,
    name: Option<String>,
    default_agent: Option<Agent>,
) -> Result<Vec<Project>> {
    state
        .store
        .get_console(&console_id)?
        .ok_or_else(|| CodedError::unknown_console(&console_id))?;

    let directories: Vec<PathBuf> = match source {
        ProjectSource::Local => {
            let path = absolute_path(&path.ok_or_else(|| field_required("path"))?)?;
            if !path.is_dir() {
                return Err(hostfs::not_a_directory(&path));
            }
            vec![path]
        }
        ProjectSource::Parent => {
            let parent = absolute_path(&path.ok_or_else(|| field_required("path"))?)?;
            let repos = hostfs::discover_repos(&parent)?;
            if repos.is_empty() {
                return Err(CodedError::raised(
                    error_code::NO_REPOSITORIES_FOUND,
                    format!(
                        "no git repositories were found directly under `{}`",
                        parent.display()
                    ),
                    &[("path", &parent.to_string_lossy())],
                ));
            }
            repos
        }
        ProjectSource::Github => {
            let url = remote_url
                .clone()
                .ok_or_else(|| field_required("remote_url"))?;
            let parent = absolute_path(&path.ok_or_else(|| field_required("path"))?)?;
            let cloned =
                tokio::task::spawn_blocking(move || hostfs::clone_repo(&url, &parent)).await??;
            vec![cloned]
        }
    };

    // A caller-supplied name only applies when a single project is being associated; a parent
    // directory yields many, and each takes its own directory's name.
    let explicit_name = if directories.len() == 1 { name } else { None };

    let mut added = Vec::new();
    for directory in directories {
        let path_text = directory.to_string_lossy().into_owned();
        if state.store.project_exists_at(&console_id, &path_text)? {
            continue;
        }
        let project_name = explicit_name.clone().unwrap_or_else(|| {
            directory
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path_text.clone())
        });
        let project = Project {
            id: Uuid::new_v4().to_string(),
            console_id: console_id.clone(),
            host_id: LOCAL_HOST_ID.to_string(),
            name: project_name,
            path: path_text,
            default_agent,
            source,
            remote_url: remote_url.clone(),
            claude_trust_consent: false,
            pinned: false,
        };
        state.store.insert_project(&project)?;
        state.broadcast(Event::ProjectUpserted {
            project: project.clone(),
        });
        added.push(project);
    }
    if added.is_empty() {
        return Err(CodedError::raised(
            error_code::ALL_PROJECTS_ALREADY_ADDED,
            "every directory found is already associated with this console",
            &[],
        ));
    }
    Ok(added)
}

/// One session to open. A struct rather than a parameter list because the two callers differ in
/// more than one field — the user opening a session by hand, and the hub dispatching one — and the
/// fields that differ are all optional strings that would otherwise be positional.
pub struct OpenRequest {
    pub console_id: String,
    pub project_id: Option<String>,
    pub agent: Option<Agent>,
    pub task: Option<String>,
    pub title: Option<String>,
    pub origin: Origin,
    pub include_in_hub: bool,
}

pub async fn open_session(state: &Arc<AppState>, request: OpenRequest) -> Result<Session> {
    let OpenRequest {
        console_id,
        project_id,
        agent,
        task,
        title,
        origin,
        include_in_hub,
    } = request;
    let console = state
        .store
        .get_console(&console_id)?
        .ok_or_else(|| CodedError::unknown_console(&console_id))?;

    // Held for the rest of this function where a hub is involved, so the one-live-hub check and the
    // insert that follows it cannot interleave with another open.
    let _hub_claim;
    let (role, cwd, project, default_title) = match &project_id {
        Some(project_id) => {
            let project = state
                .store
                .get_project(project_id)?
                .ok_or_else(|| CodedError::unknown_project(project_id))?;
            let cwd = PathBuf::from(&project.path);
            let title = project.name.clone();
            _hub_claim = None;
            (Role::Worker, cwd, Some(project), title)
        }
        None => {
            // One live hub per console. Reports route to the console's hub by lookup, and the menu
            // has one Hub row, so a second live hub would be both unreachable and able to swallow
            // reports meant for the first. The UI guards against it too, but the rule belongs here:
            // the application is only a client. The claim is what makes the check mean anything —
            // every request runs in its own task, so reading the store and then inserting would
            // otherwise let two concurrent opens both through.
            _hub_claim = Some(state.claim_hub(&console_id)?);
            if let Some(existing) = state.store.list_sessions()?.into_iter().find(|session| {
                session.console_id == console_id
                    && session.role == Role::Hub
                    && !session.status.is_dormant()
            }) {
                return Err(CodedError::raised(
                    error_code::HUB_ALREADY_RUNNING,
                    format!("this console already has a hub session ({})", existing.id),
                    &[("session", &existing.id)],
                ));
            }
            let workdir = PathBuf::from(&console.workdir);
            // Refreshed right before the hub launches, so the file it reads is the one for the
            // agent this console currently uses whatever happened to it since.
            mcp::role::write_hub_instructions(&console)?;
            (Role::Hub, workdir, None, "Hub".to_string())
        }
    };

    // Agent selection, in descending priority: what this launch asked for, the project's default,
    // then the console's. A hub session uses the console's hub agent instead.
    let agent = agent.unwrap_or(match (&role, &project) {
        (Role::Hub, _) => console.hub_agent,
        (_, Some(project)) => project.default_agent.unwrap_or(console.default_agent),
        (_, None) => console.default_agent,
    });

    let session = Session {
        id: Uuid::new_v4().to_string(),
        agent,
        agent_session_id: None,
        console_id,
        project_id,
        host_id: LOCAL_HOST_ID.to_string(),
        role,
        origin,
        title: title.unwrap_or(default_title),
        // A session opened without a task is sitting at its prompt, not working. This also
        // matters for Codex specifically: its `SessionStart` hook does not fire until the first
        // prompt submission, so an optimistic `working` here would stay until the user typed.
        status: if task.is_some() {
            SessionStatus::Working
        } else {
            SessionStatus::Idle
        },
        has_conversation: false,
        // Taken from the console now and kept: a resume must find the transcript where the first
        // launch put it, whatever the console's setting says by then.
        config_dir: session_config_dir(agent, &console),
        // A hub session is the recipient of reports, never a sender of them.
        include_in_hub: role == Role::Worker && (origin == Origin::Hub || include_in_hub),
        pinned: false,
        started_at: now_millis(),
        ended_at: None,
    };
    state.store.insert_session(&session)?;
    let claim = state.begin_launch(&session.id)?;

    match start_process(state, claim, &session, &cwd, task.as_deref(), None).await {
        Ok(()) => {
            // Re-read rather than publish the record we built: launching may have learned the
            // agent's own session id, and publishing the stale copy would tell every client the
            // session has none.
            let started = state.session_record(&session.id)?;
            state.publish_session(&started);
            Ok(started)
        }
        Err(err) => {
            // Nothing ran, so there is nothing to resume: drop the record rather than leave a
            // session in the tree that never existed.
            state.store.delete_session(&session.id)?;
            state.revoke_mcp_tokens(&session.id);
            Err(err)
        }
    }
}

/// Relaunches a dormant session. `instruction` is written into it once it is running — queued here
/// rather than by the caller, because whichever hook releases it can fire the moment the agent
/// starts, so it has to be in the queue before the launch and out again if the launch never happens.
pub async fn resume_session(
    state: &Arc<AppState>,
    id: &str,
    instruction: Option<&str>,
) -> Result<()> {
    // Claimed before anything else, so two overlapping relaunches cannot both get through, and
    // before the record is read, so a delete cannot slip in between reading and launching.
    let claim = state.begin_launch(id)?;
    let mut session = state.session_record(id)?;
    if !session.status.is_dormant() {
        // The ordinary cause is a second click landing after the first relaunch already moved the
        // session, so it carries the same code as a launch that is already under way.
        return Err(CodedError::raised(
            error_code::SESSION_ALREADY_RUNNING,
            "this session is not interrupted or archived",
            &[("session", id)],
        ));
    }

    // Same claim as `open_session`: reading the store and then relaunching is two steps.
    let _hub_claim = if session.role == Role::Hub {
        let claim = state.claim_hub(&session.console_id)?;
        if let Some(existing) = state.store.list_sessions()?.into_iter().find(|other| {
            other.console_id == session.console_id
                && other.role == Role::Hub
                && other.id != session.id
                && !other.status.is_dormant()
        }) {
            return Err(CodedError::raised(
                error_code::HUB_REOPEN_BLOCKED,
                format!(
                    "this console already has a hub session ({}); archive it before reopening this \
                     one",
                    existing.id
                ),
                &[("session", &existing.id)],
            ));
        }
        Some(claim)
    } else {
        None
    };

    let previous_status = session.status;
    let cwd = session_cwd(state, &session)?;
    // Only a session that has had a turn has something to resume. Without one the agent stored no
    // conversation, and resuming by id fails with "No conversation found" — so this launches a
    // fresh conversation instead, in the same project and under the same session.
    let resume_id = if session.has_conversation {
        session.agent_session_id.clone()
    } else {
        None
    };
    // A relaunch carries no task, so the agent comes up at its prompt rather than working. On
    // Codex that is also the only correct value available: its `SessionStart` hook does not fire
    // until the first prompt submission, so an optimistic `working` would never be corrected.
    session.status = SessionStatus::Idle;
    session.ended_at = None;
    state.store.update_session(&session)?;

    // Queued with every refusal above already past, so a call that never launched leaves nothing
    // behind for a later launch to deliver.
    if let Some(instruction) = instruction {
        state.queue_message(&session.id, instruction);
    }

    match start_process(state, claim, &session, &cwd, None, resume_id.as_deref()).await {
        Ok(()) => {
            state.publish_session(&state.session_record(&session.id)?);
            reporting::release_after_relaunch(state, &session);
            Ok(())
        }
        Err(err) => {
            // Back to where it was, archived included: a reopen that failed to launch must not
            // quietly move a session out of its project's Archive group.
            session.status = previous_status;
            session.ended_at = Some(now_millis());
            state.save_session(&session)?;
            state.revoke_mcp_tokens(&session.id);
            // Nothing will ever release what was queued for this relaunch, and leaving it would
            // deliver a stale instruction to whatever launch comes next.
            state.discard_outbox(&session.id);
            Err(err)
        }
    }
}

/// Launches the agent for an already-stored session and starts watching it.
async fn start_process(
    state: &Arc<AppState>,
    claim: crate::state::LaunchClaim,
    session: &Session,
    cwd: &Path,
    task: Option<&str>,
    resume_agent_session_id: Option<&str>,
) -> Result<()> {
    let request = term::LaunchRequest {
        session_id: session.id.clone(),
        agent: session.agent,
        role: session.role,
        cwd: cwd.to_path_buf(),
        task: task.map(str::to_string),
        resume_agent_session_id: resume_agent_session_id.map(str::to_string),
        config_dir: session.config_dir.as_ref().map(PathBuf::from),
        daemon_port: state.port,
        self_exe: state.self_exe.clone(),
        // Issued per launch: the previous process is gone, and a token outliving it would let a
        // stale child act on this session.
        mcp_token: state.issue_mcp_token(&session.id),
    };

    // Launching snapshots the user's shell environment and forks a process, so it goes off the
    // runtime rather than holding up the socket it was asked on.
    let launch = tokio::task::spawn_blocking(move || term::launch(request)).await??;

    // Registering the session consumes the claim: from here on the live map is what says it is
    // running.
    state.register_live(launch.session.clone());
    drop(claim);
    state.watch_exit(launch.session.clone());
    trust::supervise(state, &launch.session);
    if let Some(agent_session_id) = launch.agent_session_id {
        state.set_agent_session_id(&session.id, &agent_session_id)?;
    }
    if let Some(notice) = launch.notice {
        state.broadcast(notice.about(&session.id));
    }
    Ok(())
}

pub fn archive_session(state: &Arc<AppState>, id: &str) -> Result<()> {
    let mut session = state.session_record(id)?;
    session.status = SessionStatus::Archived;
    session.ended_at = Some(now_millis());
    state.save_session(&session)?;
    if let Some(live) = state.live_session(id) {
        // Ending the process waits out the graceful period, which the caller should not.
        tokio::task::spawn_blocking(move || live.terminate());
    }
    Ok(())
}

/// Removes Octoboard's record of one archived session. Nothing else of Octoboard's hangs off a
/// session: its queue, MCP token and turn bookkeeping go when its process does, and so does its
/// scratch directory (clearing the run directory at startup is only the fallback). The agent's own
/// transcript and the project are never touched.
fn delete_session(state: &Arc<AppState>, id: &str) -> Result<()> {
    state.session_record(id)?;
    if !state.delete_if_archived(id)? {
        return Err(CodedError::raised(
            error_code::SESSION_NOT_ARCHIVED,
            "only an archived session can be deleted",
            &[("session", id)],
        ));
    }
    Ok(())
}

/// Deletes every archived session of `project`, or with no project every archived hub session of
/// `console`. One that stopped being archived meanwhile (a resume got there first) is skipped.
fn delete_archived_sessions(
    state: &Arc<AppState>,
    console_id: &str,
    project_id: Option<&str>,
) -> Result<()> {
    state
        .store
        .get_console(console_id)?
        .ok_or_else(|| CodedError::unknown_console(console_id))?;
    if let Some(project_id) = project_id {
        let project = state
            .store
            .get_project(project_id)?
            .ok_or_else(|| CodedError::unknown_project(project_id))?;
        if project.console_id != console_id {
            return Err(CodedError::unknown_project(project_id));
        }
    }
    for session in state.store.list_sessions()? {
        let in_scope = session.console_id == console_id
            && match project_id {
                Some(project_id) => session.project_id.as_deref() == Some(project_id),
                None => session.role == Role::Hub,
            };
        if in_scope {
            state.delete_if_archived(&session.id)?;
        }
    }
    Ok(())
}

/// The user's own write into a session. Refused rather than queued while the session is waiting for
/// them: they are the one who has to answer the prompt that is blocking it, and they can be told so.
fn send_message(state: &Arc<AppState>, id: &str, text: &str) -> Result<()> {
    reporting::write_message(state, id, text, reporting::WhenBlocked::Refuse)?;
    Ok(())
}

/// A report panel form submission. Refused unless `page_id` names that console's newest page —
/// history pages are read-only, and this is where that is actually enforced; a panel that disables
/// its own submit button on a history page is only reflecting the rule, not the source of it.
async fn submit_page(state: &Arc<AppState>, page_id: &str, data: serde_json::Value) -> Result<()> {
    let page = state.store.get_page(page_id)?.ok_or_else(|| {
        CodedError::raised(
            error_code::UNKNOWN_PAGE,
            format!("unknown page {page_id}"),
            &[("page", page_id)],
        )
    })?;
    let newest = state.store.newest_page_id(&page.console_id)?;
    if newest.as_deref() != Some(page.id.as_str()) {
        return Err(CodedError::raised(
            error_code::PAGE_NOT_CURRENT,
            "this page is no longer current; its form can no longer be submitted",
            &[],
        ));
    }
    let hub = reporting::hub_session(state, &page.console_id)?.ok_or_else(|| {
        CodedError::raised(
            error_code::HUB_MISSING,
            "this console has no hub session, so there is nobody to submit to",
            &[],
        )
    })?;

    let message = reporting::render_page_submission(&page.id, &data);
    let owned_state = state.clone();
    // The write blocks on the PTY. Queued rather than refused: the user is the one submitting, so
    // there is nobody to tell to answer a prompt first, and the submission must not be dropped.
    tokio::task::spawn_blocking(move || {
        reporting::write_message(
            &owned_state,
            &hub.id,
            &message,
            reporting::WhenBlocked::Queue,
        )
    })
    .await??;
    Ok(())
}

/// The directory a new session is pinned to: the console's setting for the session's own agent.
fn session_config_dir(agent: Agent, console: &Console) -> Option<String> {
    match agent {
        Agent::Claude => console.claude_config_dir.clone(),
        Agent::Codex => console.codex_config_dir.clone(),
        Agent::Grok => console.grok_config_dir.clone(),
    }
}

/// What the user typed for one agent's config directory on a console, as the absolute path that is
/// stored. Blank means unset. The directory has to exist: the agent would create a missing one
/// and start logged out in it, so a mistyped path is better refused here, where the dialog can
/// show why, than discovered as a session that has lost its login.
fn normalize_config_dir(agent: Agent, text: Option<&str>) -> Result<Option<String>> {
    let Some(text) = text.map(str::trim).filter(|text| !text.is_empty()) else {
        return Ok(None);
    };
    let dir = hostfs::expand(text);
    if !dir.is_absolute() {
        return Err(CodedError::raised(
            error_code::CONFIG_DIR_NOT_ABSOLUTE,
            format!(
                "the {} config directory must be an absolute path or start with `~/`",
                agent.label()
            ),
            &[("agent", agent.label())],
        ));
    }
    // Lexical only: a symlink stays as typed, so the stored path is what the user chose.
    let dir = hostfs::lexically_normalise(&dir);
    if !dir.is_dir() {
        return Err(CodedError::raised(
            error_code::CONFIG_DIR_NOT_A_DIRECTORY,
            format!(
                "the {} config directory `{}` is not a directory",
                agent.label(),
                dir.display()
            ),
            &[("agent", agent.label()), ("path", &dir.to_string_lossy())],
        ));
    }
    Ok(Some(dir.to_string_lossy().into_owned()))
}

/// The longest avatar `data:` URL a console may carry. The UI sends a 128x128 image, which is
/// far smaller; the bound is there because the value goes to every client with each console.
const MAX_ICON_BYTES: usize = 256 * 1024;

/// A console's avatar as it is stored. Blank means unset. Checked lightly: it has to be an image
/// `data:` URL and not too long; whether it decodes is left to the client that draws it.
fn normalize_icon(text: Option<&str>) -> Result<Option<String>> {
    let Some(text) = text.map(str::trim).filter(|text| !text.is_empty()) else {
        return Ok(None);
    };
    if !text.starts_with("data:image/") {
        return Err(CodedError::raised(
            error_code::ICON_NOT_AN_IMAGE,
            "the console avatar must be an image `data:` URL",
            &[],
        ));
    }
    if text.len() > MAX_ICON_BYTES {
        return Err(CodedError::raised(
            error_code::ICON_TOO_LARGE,
            format!(
                "the console avatar is larger than {} KiB",
                MAX_ICON_BYTES / 1024
            ),
            &[("limit_kib", &(MAX_ICON_BYTES / 1024).to_string())],
        ));
    }
    Ok(Some(text.to_string()))
}

fn session_cwd(state: &Arc<AppState>, session: &Session) -> Result<PathBuf> {
    match &session.project_id {
        Some(project_id) => {
            let project = state
                .store
                .get_project(project_id)?
                .ok_or_else(|| CodedError::unknown_project(project_id))?;
            Ok(PathBuf::from(project.path))
        }
        None => {
            let console = state
                .store
                .get_console(&session.console_id)?
                .ok_or_else(|| CodedError::unknown_console(&session.console_id))?;
            Ok(PathBuf::from(console.workdir))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Console;

    fn console(hub_agent: Agent, workdir: &Path) -> Console {
        Console {
            id: "console-1".to_string(),
            name: "Console".to_string(),
            workdir: workdir.to_string_lossy().into_owned(),
            hub_agent,
            default_agent: Agent::Claude,
            claude_config_dir: None,
            codex_config_dir: None,
            grok_config_dir: None,
            icon: None,
            created_at: 0,
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "octoboardd-coordinator-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).expect("temporary directory");
        dir
    }

    /// Every agent matches its instruction filename by exact spelling, and Grok reads several of
    /// them — so a file left behind by a console that changed agents would be loaded alongside the
    /// right one.
    #[test]
    fn the_hub_instruction_file_is_the_only_one_left_in_the_working_directory() {
        let workdir = temp_dir("hub-instructions");

        mcp::role::write_hub_instructions(&console(Agent::Claude, &workdir)).expect("written");
        assert!(workdir.join("CLAUDE.md").is_file());
        assert!(!workdir.join("AGENTS.md").exists());

        mcp::role::write_hub_instructions(&console(Agent::Codex, &workdir)).expect("written");
        assert!(workdir.join("AGENTS.md").is_file());
        assert!(!workdir.join("CLAUDE.md").exists());

        // Grok reads no project instructions without a git root, and a console's working directory
        // is not a repository — so it gets none, and the previous agent's file goes.
        mcp::role::write_hub_instructions(&console(Agent::Grok, &workdir)).expect("written");
        assert!(!workdir.join("AGENTS.md").exists());
        assert!(!workdir.join("CLAUDE.md").exists());

        std::fs::remove_dir_all(&workdir).ok();
    }

    #[test]
    fn a_config_dir_is_stored_absolute_or_not_at_all() {
        let dir = temp_dir("config-dir");
        let text = dir.to_string_lossy().into_owned();
        let normalize = |text: Option<&str>| normalize_config_dir(Agent::Codex, text);

        assert_eq!(normalize(None).unwrap(), None);
        assert_eq!(normalize(Some("  ")).unwrap(), None);
        assert_eq!(
            normalize(Some(&format!("  {text} "))).unwrap(),
            Some(text.clone())
        );

        assert_eq!(
            normalize(Some(&format!("{text}/"))).unwrap(),
            Some(text.clone())
        );
        assert_eq!(
            normalize(Some(&format!("{text}/./sub/.."))).unwrap(),
            Some(text.clone())
        );

        // The refusal names the agent whose field it was.
        let relative = normalize(Some(".codex-alt")).unwrap_err();
        assert!(relative.to_string().contains("absolute"), "{relative}");
        assert!(relative.to_string().contains("Codex"), "{relative}");
        let missing = normalize(Some(&format!("{text}/missing"))).unwrap_err();
        assert!(missing.to_string().contains("not a directory"), "{missing}");
        assert!(missing.to_string().contains("Codex"), "{missing}");
        let file = dir.join("file");
        std::fs::write(&file, "").expect("file");
        assert!(normalize(Some(&file.to_string_lossy())).is_err());

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Only an archived session can be deleted, and the bulk delete takes exactly its scope: a
    /// project's archived sessions, or with no project the console's own archived hubs.
    #[test]
    fn deleting_sessions_is_limited_to_archived_ones_in_scope() {
        let state = Arc::new(crate::state::tests::app_state("delete-sessions"));
        let dir = temp_dir("delete-sessions");
        for id in ["console-1", "console-2"] {
            let mut other = console(Agent::Claude, &dir);
            other.id = id.to_string();
            state.store.insert_console(&other).unwrap();
        }
        state
            .store
            .insert_project(&Project {
                id: "project-1".to_string(),
                console_id: "console-1".to_string(),
                host_id: LOCAL_HOST_ID.to_string(),
                name: "Project".to_string(),
                path: "/tmp/project-1".to_string(),
                default_agent: None,
                source: ProjectSource::Local,
                remote_url: None,
                claude_trust_consent: false,
                pinned: false,
            })
            .unwrap();
        let add = |id: &str, console: &str, role: Role, project: Option<&str>, status| {
            state
                .store
                .insert_session(&Session {
                    id: id.to_string(),
                    agent: Agent::Claude,
                    agent_session_id: None,
                    console_id: console.to_string(),
                    project_id: project.map(str::to_string),
                    host_id: LOCAL_HOST_ID.to_string(),
                    role,
                    origin: Origin::User,
                    title: id.to_string(),
                    status,
                    has_conversation: false,
                    include_in_hub: false,
                    config_dir: None,
                    pinned: false,
                    started_at: 0,
                    ended_at: None,
                })
                .unwrap();
        };
        let archived = SessionStatus::Archived;
        add(
            "worker-archived",
            "console-1",
            Role::Worker,
            Some("project-1"),
            archived,
        );
        add(
            "worker-idle",
            "console-1",
            Role::Worker,
            Some("project-1"),
            SessionStatus::Idle,
        );
        add("hub-archived", "console-1", Role::Hub, None, archived);
        add("other-hub-archived", "console-2", Role::Hub, None, archived);
        let remaining = || -> Vec<String> {
            let mut ids: Vec<String> = state
                .store
                .list_sessions()
                .unwrap()
                .into_iter()
                .map(|session| session.id)
                .collect();
            ids.sort();
            ids
        };

        // A resume in flight holds the session's launch claim, which keeps it from being deleted.
        let claim = state.begin_launch("worker-archived").unwrap();
        assert!(!state.delete_if_archived("worker-archived").unwrap());
        drop(claim);

        let refused = delete_session(&state, "worker-idle").expect_err("not archived");
        let coded = refused.downcast_ref::<CodedError>().expect("a coded error");
        assert_eq!(coded.code, error_code::SESSION_NOT_ARCHIVED);

        delete_archived_sessions(&state, "console-1", Some("project-1")).unwrap();
        assert_eq!(
            remaining(),
            ["hub-archived", "other-hub-archived", "worker-idle"]
        );

        delete_archived_sessions(&state, "console-1", None).unwrap();
        assert_eq!(remaining(), ["other-hub-archived", "worker-idle"]);

        std::fs::remove_dir_all(dir).ok();
    }

    /// A project's deletion can be told to stop its sessions, but never goes ahead under a session
    /// that is being launched; with nothing in flight it removes the project and its sessions.
    #[tokio::test]
    async fn deleting_a_project_with_stop_sessions_refuses_a_launch_in_flight() {
        let state = Arc::new(crate::state::tests::app_state("delete-project-stop"));
        let dir = temp_dir("delete-project-stop");
        state
            .store
            .insert_console(&console(Agent::Claude, &dir))
            .unwrap();
        state
            .store
            .insert_project(&Project {
                id: "project-1".to_string(),
                console_id: "console-1".to_string(),
                host_id: LOCAL_HOST_ID.to_string(),
                name: "Project".to_string(),
                path: "/tmp/project-1".to_string(),
                default_agent: None,
                source: ProjectSource::Local,
                remote_url: None,
                claude_trust_consent: false,
                pinned: false,
            })
            .unwrap();
        state
            .store
            .insert_session(&Session {
                id: "worker".to_string(),
                agent: Agent::Claude,
                agent_session_id: None,
                console_id: "console-1".to_string(),
                project_id: Some("project-1".to_string()),
                host_id: LOCAL_HOST_ID.to_string(),
                role: Role::Worker,
                origin: Origin::User,
                title: "Worker".to_string(),
                status: SessionStatus::Archived,
                has_conversation: false,
                include_in_hub: false,
                config_dir: None,
                pinned: false,
                started_at: 0,
                ended_at: None,
            })
            .unwrap();
        let delete = || RequestBody::DeleteProject {
            project: "project-1".to_string(),
            stop_sessions: true,
        };

        let claim = state.begin_launch("worker").unwrap();
        let refused = handle(&state, None, delete()).await.expect_err("launching");
        let coded = refused.downcast_ref::<CodedError>().expect("a coded error");
        assert_eq!(coded.code, error_code::PROJECT_HAS_RUNNING_SESSIONS);
        drop(claim);

        handle(&state, None, delete()).await.unwrap();
        assert!(state.store.get_project("project-1").unwrap().is_none());
        assert!(state.store.get_session("worker").unwrap().is_none());

        std::fs::remove_dir_all(dir).ok();
    }

    /// `~` is the home directory of the host the daemon runs on, like every other path it takes.
    #[test]
    fn a_leading_tilde_in_a_config_dir_is_expanded() {
        // `$HOME` always exists as a directory, so this needs no fixture and no environment edit.
        let home = paths::home_dir();
        assert_eq!(
            normalize_config_dir(Agent::Grok, Some("~")).unwrap(),
            Some(home.to_string_lossy().into_owned())
        );
    }

    #[test]
    fn a_console_icon_is_an_image_data_url_of_bounded_size() {
        assert_eq!(normalize_icon(None).unwrap(), None);
        assert_eq!(normalize_icon(Some("  ")).unwrap(), None);
        assert_eq!(
            normalize_icon(Some(" data:image/webp;base64,AAAA ")).unwrap(),
            Some("data:image/webp;base64,AAAA".to_string())
        );
        assert!(normalize_icon(Some("https://example.com/a.png")).is_err());
        assert!(normalize_icon(Some("data:text/html;base64,AAAA")).is_err());
        let huge = format!("data:image/png;base64,{}", "A".repeat(MAX_ICON_BYTES));
        assert!(normalize_icon(Some(&huge)).is_err());
    }

    #[test]
    fn a_session_is_pinned_to_its_own_agents_config_dir() {
        let mut console = console(Agent::Claude, Path::new("/tmp/unused"));
        console.claude_config_dir = Some("/home/u/.claude-alt".to_string());
        console.codex_config_dir = Some("/home/u/.codex-alt".to_string());
        assert_eq!(
            session_config_dir(Agent::Claude, &console).as_deref(),
            Some("/home/u/.claude-alt")
        );
        assert_eq!(
            session_config_dir(Agent::Codex, &console).as_deref(),
            Some("/home/u/.codex-alt")
        );
        // Nothing is set for Grok, and another agent's directory is never borrowed for it.
        assert_eq!(session_config_dir(Agent::Grok, &console), None);
    }

    /// A project's path is stored absolute and written one way, so the trusted directories can be
    /// compared against it; a relative one is refused.
    #[test]
    fn a_project_path_is_stored_absolute_and_normalised() {
        assert_eq!(
            absolute_path("/work/a/../project/./").unwrap(),
            PathBuf::from("/work/project")
        );
        assert_eq!(
            absolute_path("~/code/app/").unwrap(),
            paths::home_dir().join("code/app")
        );
        let err = absolute_path("work/project").expect_err("relative");
        assert!(err.to_string().contains("not an absolute path"), "{err}");
        assert!(absolute_path("").is_err());
    }
}
