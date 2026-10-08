//! The coordinator role: what each control-socket request does to the stored consoles, projects and
//! sessions, and which host-role work it triggers — including the launch flow every way of starting
//! a session goes through.
//!
//! Writing into a running session and everything built on it (the console session's reports,
//! synthesis, automatic archiving) is `crate::reporting`'s, because the console session's tools and
//! the hook callback reach it without going through a control-socket request at all.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use uuid::Uuid;

use crate::git_status;
use crate::hostfs;
use crate::mcp;
use crate::paths;
use crate::protocol::{
    error_code, now_millis, Account, Agent, Availability, CodedError, Console, Event, Origin,
    Project, ProjectSource, RequestBody, Role, Session, SessionStatus,
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
            console_session_agent,
            default_agent,
            claude_account_id,
            codex_account_id,
            grok_account_id,
            icon,
        } => {
            // Every field checked first, so a refusal on a later one leaves no working directory
            // behind.
            let icon = normalize_icon(icon.as_deref())?;
            let claude_account_id =
                checked_account_reference(state, Agent::Claude, claude_account_id)?;
            let codex_account_id =
                checked_account_reference(state, Agent::Codex, codex_account_id)?;
            let grok_account_id = checked_account_reference(state, Agent::Grok, grok_account_id)?;
            let id = Uuid::new_v4().to_string();
            let workdir = paths::console_workdir(&id);
            std::fs::create_dir_all(&workdir)?;
            let console = Console {
                id,
                name,
                workdir: workdir.to_string_lossy().into_owned(),
                console_session_agent,
                default_agent,
                claude_account_id,
                codex_account_id,
                grok_account_id,
                icon,
                created_at: now_millis(),
            };
            mcp::role::write_console_session_instructions(&console)?;
            state.store.insert_console(&console)?;
            state.broadcast(Event::ConsoleUpserted { console });
            Ok(None)
        }

        RequestBody::UpdateConsole {
            console: id,
            name,
            console_session_agent,
            default_agent,
            claude_account_id,
            codex_account_id,
            grok_account_id,
            icon,
        } => {
            let mut console = state
                .store
                .get_console(&id)?
                .ok_or_else(|| CodedError::unknown_console(&id))?;
            // Every field checked first, with nothing written yet. An absent reference is not
            // checked at all: saving a console with a field untouched succeeds whatever has
            // happened to the account it names since.
            let icon = icon
                .map(|icon| normalize_icon(icon.as_deref()))
                .transpose()?;
            let claude_account_id = claude_account_id
                .map(|reference| checked_account_reference(state, Agent::Claude, reference))
                .transpose()?;
            let codex_account_id = codex_account_id
                .map(|reference| checked_account_reference(state, Agent::Codex, reference))
                .transpose()?;
            let grok_account_id = grok_account_id
                .map(|reference| checked_account_reference(state, Agent::Grok, reference))
                .transpose()?;
            if let Some(name) = name {
                console.name = name;
            }
            if let Some(console_session_agent) = console_session_agent {
                console.console_session_agent = console_session_agent;
            }
            if let Some(default_agent) = default_agent {
                console.default_agent = default_agent;
            }
            // Only sessions opened afterwards take a new reference; each existing one keeps the
            // account and the directory it was started with (see `Session::account_id`).
            if let Some(reference) = claude_account_id {
                console.claude_account_id = reference;
            }
            if let Some(reference) = codex_account_id {
                console.codex_account_id = reference;
            }
            if let Some(reference) = grok_account_id {
                console.grok_account_id = reference;
            }
            if let Some(icon) = icon {
                console.icon = icon;
            }
            // Rewritten rather than left alone: the file is named for the console session's agent,
            // so a console that changed agents would otherwise keep reading the old one's.
            mcp::role::write_console_session_instructions(&console)?;
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
            // Read before the delete below: `projects.console_id` is `ON DELETE CASCADE`, so the
            // store drops this console's projects with it, and nothing else would ever clean up
            // their git statuses afterwards — the daemon keeps no timer of its own that would
            // notice they are gone.
            let project_ids: Vec<String> = state
                .store
                .list_projects()?
                .into_iter()
                .filter(|project| project.console_id == id)
                .map(|project| project.id)
                .collect();
            state.store.delete_console(&id)?;
            for project_id in &project_ids {
                state.remove_git_status(project_id);
            }
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
            tags,
        } => {
            add_project(
                state,
                AddProjectRequest {
                    console_id,
                    source,
                    path,
                    remote_url,
                    name,
                    default_agent,
                    tags,
                },
            )
            .await?;
            Ok(None)
        }

        RequestBody::UpdateProject {
            project: id,
            name,
            default_agent,
            pinned,
            tags,
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
            if let Some(tags) = tags {
                project.tags = normalise_tags(tags);
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
            state.remove_git_status(&id);
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
            account,
            task,
            title,
            bound_to,
        } => {
            let session = open_session(
                state,
                OpenRequest {
                    console_id,
                    project_id,
                    agent,
                    account,
                    task,
                    title,
                    origin: Origin::User,
                    bound_to,
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

        RequestBody::UpdateSettings {
            auto_sync_repositories,
        } => {
            update_settings(state, auto_sync_repositories)?;
            Ok(None)
        }

        RequestBody::RefreshGitStatus { console } => {
            refresh_git_status(state, &console)?;
            Ok(None)
        }

        RequestBody::CreateAccount {
            agent,
            name,
            config_dir,
        } => {
            create_account(state, agent, name, config_dir)?;
            Ok(None)
        }

        RequestBody::UpdateAccount {
            account,
            name,
            config_dir,
        } => {
            update_account(state, &account, name, config_dir)?;
            Ok(None)
        }

        RequestBody::DeleteAccount { account } => {
            delete_account(state, &account)?;
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

/// A project's tags as stored: each trimmed, the empty ones dropped, and a later tag dropped when
/// an earlier one matches it ignoring case (the first spelling wins). The order is the user's.
fn normalise_tags(tags: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    tags.into_iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty() && seen.insert(tag.to_lowercase()))
        .collect()
}

/// One association request. A struct rather than a parameter list because the two callers differ
/// in more than one field — the user's control-socket request, and the console session's tool call,
/// which has no tags — and most of the fields are optional strings that would otherwise be
/// positional.
pub struct AddProjectRequest {
    pub console_id: String,
    pub source: ProjectSource,
    pub path: Option<String>,
    pub remote_url: Option<String>,
    pub name: Option<String>,
    pub default_agent: Option<Agent>,
    pub tags: Option<Vec<String>>,
}

pub async fn add_project(
    state: &Arc<AppState>,
    request: AddProjectRequest,
) -> Result<Vec<Project>> {
    let AddProjectRequest {
        console_id,
        source,
        path,
        remote_url,
        name,
        default_agent,
        tags,
    } = request;
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

    let tags = normalise_tags(tags.unwrap_or_default());

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
            tags: tags.clone(),
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

/// What a new session should be bound to: always `None` for a console session, which is never
/// bound; for a project session, whatever `requested` names, which must be a console session of
/// `console_id` or the request is refused with `unknown_session` — a binding is fixed for the
/// session's lifetime once set, so naming one that does not exist, or belongs to another console,
/// must not be let through to be discovered later.
fn resolve_bound_to(
    state: &Arc<AppState>,
    console_id: &str,
    role: Role,
    requested: Option<String>,
) -> Result<Option<String>> {
    if role == Role::Console {
        return Ok(None);
    }
    let Some(target_id) = requested else {
        return Ok(None);
    };
    let target = state
        .store
        .get_session(&target_id)?
        .filter(|target| target.console_id == console_id && target.role == Role::Console)
        .ok_or_else(|| CodedError::unknown_session(&target_id))?;
    Ok(Some(target.id))
}

/// One session to open. A struct rather than a parameter list because the two callers differ in
/// more than one field — the user opening a session by hand, and the console session dispatching
/// one — and the fields that differ are all optional strings that would otherwise be positional.
pub struct OpenRequest {
    pub console_id: String,
    pub project_id: Option<String>,
    pub agent: Option<Agent>,
    /// The account chosen for this session: `None` leaves it to the console's reference for the
    /// session's agent, `Some(None)` chooses the default account outright. See
    /// [`session_account`].
    pub account: Option<Option<String>>,
    pub task: Option<String>,
    pub title: Option<String>,
    pub origin: Origin,
    /// The console session this (project) session should report to, or `None` for one outside the
    /// orchestration. Ignored when a console session itself is being opened, which is never bound.
    pub bound_to: Option<String>,
}

pub async fn open_session(state: &Arc<AppState>, request: OpenRequest) -> Result<Session> {
    let OpenRequest {
        console_id,
        project_id,
        agent,
        account,
        task,
        title,
        origin,
        bound_to,
    } = request;
    let console = state
        .store
        .get_console(&console_id)?
        .ok_or_else(|| CodedError::unknown_console(&console_id))?;

    let (role, cwd, project, default_title) = match &project_id {
        Some(project_id) => {
            let project = state
                .store
                .get_project(project_id)?
                .ok_or_else(|| CodedError::unknown_project(project_id))?;
            let cwd = PathBuf::from(&project.path);
            let title = project.name.clone();
            (Role::Project, cwd, Some(project), title)
        }
        None => {
            let workdir = PathBuf::from(&console.workdir);
            // Refreshed right before the console session launches, so the file it reads is the one
            // for the agent this console currently uses whatever happened to it since. A console
            // may hold any number of live console sessions, so nothing here checks for one already
            // running.
            mcp::role::write_console_session_instructions(&console)?;
            // Overwritten by `Store::insert_console_session` below, which decides the real default
            // ("Hub <ordinal>") once it knows the ordinal; nothing here reads this value.
            (Role::Console, workdir, None, String::new())
        }
    };

    let bound_to = resolve_bound_to(state, &console_id, role, bound_to)?;

    // Agent selection, in descending priority: what this launch asked for, the project's default,
    // then the console's. A console session uses the console's console session agent instead.
    let agent = agent.unwrap_or(match (&role, &project) {
        (Role::Console, _) => console.console_session_agent,
        (_, Some(project)) => project.default_agent.unwrap_or(console.default_agent),
        (_, None) => console.default_agent,
    });
    refuse_if_agent_unavailable(state, agent)?;

    let (account_id, config_dir) = session_account(state, agent, &console, account)?;

    let session = Session {
        id: Uuid::new_v4().to_string(),
        agent,
        agent_session_id: None,
        console_id,
        project_id,
        host_id: LOCAL_HOST_ID.to_string(),
        role,
        origin,
        title: title.clone().unwrap_or_else(|| default_title.clone()),
        // A session opened without a task is sitting at its prompt, not working. This also
        // matters for Codex specifically: its `SessionStart` hook does not fire until the first
        // prompt submission, so an optimistic `working` here would stay until the user typed.
        status: if task.is_some() {
            SessionStatus::Working
        } else {
            SessionStatus::Idle
        },
        has_conversation: false,
        // Resolved now and kept: a resume must find the transcript where the first launch put it,
        // whatever the console's reference says by then.
        account_id,
        config_dir,
        bound_to,
        // Assigned by `insert_console_session` below for a console session; a project session
        // carries no colour or ordinal of its own.
        colour: None,
        ordinal: None,
        pinned: false,
        started_at: now_millis(),
        ended_at: None,
    };
    let session = if role == Role::Console {
        state.store.insert_console_session(session, title)?
    } else {
        state.store.insert_session(&session)?;
        session
    };
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

/// Deletes every archived session of `project`, or with no project every archived console session
/// of `console`. One that stopped being archived meanwhile (a resume got there first) is skipped.
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
                None => session.role == Role::Console,
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
    let console_session = newest_console_session(state, &page.console_id)?.ok_or_else(|| {
        CodedError::raised(
            error_code::CONSOLE_SESSION_MISSING,
            "this console has no console session, so there is nobody to submit to",
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
            &console_session.id,
            &message,
            reporting::WhenBlocked::Queue,
        )
    })
    .await??;
    Ok(())
}

/// One console session to address a report-panel submission to, preferring one with a process
/// behind it: a console may hold several, but a page still belongs to the console as a whole
/// rather than to one of them, so which one gets the submission is a guess.
///
/// TODO(docs/plans/20261008-console-sessions-and-agent-accounts/09-report-panel-scope.md): remove
/// this once a page carries the id of the console session it belongs to.
fn newest_console_session(state: &Arc<AppState>, console_id: &str) -> Result<Option<Session>> {
    let mut candidates: Vec<Session> = state
        .store
        .list_sessions()?
        .into_iter()
        .filter(|session| session.console_id == console_id && session.role == Role::Console)
        .collect();
    candidates.sort_by_key(|session| (session.status.is_dormant(), -session.started_at));
    Ok(candidates.into_iter().next())
}

/// Refuses to open a session whose resolved agent has been *determined* unavailable — never while
/// that determination is still pending, which the launch's own refusal of a missing binary already
/// covers; see "Opening a session" in `docs/product/sessions.md`. Checked here, ahead of
/// everything that follows, so this is also what a console session's own `start_session` tool is
/// refused through — it calls this same function.
fn refuse_if_agent_unavailable(state: &Arc<AppState>, agent: Agent) -> Result<()> {
    if state.agent_availability_of(agent).availability != Availability::Unavailable {
        return Ok(());
    }
    Err(CodedError::raised(
        error_code::AGENT_NOT_AVAILABLE,
        format!(
            "{} is not available: its binary does not resolve on your login shell's PATH. {} \
             supports Claude Code, Codex and Grok Build; install one of them and make sure it is \
             on your PATH to open a session.",
            agent.label(),
            crate::APP_NAME
        ),
        &[("agent", agent.label())],
    ))
}

/// The account id and the config directory a session of `agent` should launch with, and that
/// account's directory at this moment — `None` for both when it is the default account (the state
/// of pinning nothing). In descending priority: the choice made for this session (`chosen`, where
/// `Some(None)` is a choice of the default account), the console's account for the agent, the
/// agent's default account. Mirrors the agent chain in [`open_session`]; the project contributes
/// the agent alone.
fn session_account(
    state: &Arc<AppState>,
    agent: Agent,
    console: &Console,
    chosen: Option<Option<String>>,
) -> Result<(Option<String>, Option<String>)> {
    let account_id = match chosen {
        Some(choice) => checked_account_reference(state, agent, choice)?,
        None => console_account_field(console, agent).clone(),
    };
    let config_dir = match &account_id {
        None => None,
        Some(id) => state
            .store
            .get_account(id)?
            .map(|account| account.config_dir),
    };
    Ok((account_id, config_dir))
}

/// A reference to an account of `agent` as stored, or the `unknown_account` refusal when `id`
/// names no account or one of another agent: an account belongs to one agent, and a reference
/// across agents would launch it with another agent's directory.
fn checked_account_reference(
    state: &Arc<AppState>,
    agent: Agent,
    id: Option<String>,
) -> Result<Option<String>> {
    let Some(id) = id else { return Ok(None) };
    match state.store.get_account(&id)? {
        Some(account) if account.agent == agent => Ok(Some(id)),
        _ => Err(CodedError::unknown_account(&id)),
    }
}

/// What the user typed for one agent's config directory, as the absolute, lexically normalised
/// path an account's own directory is stored as. Required and non-blank: an account always has a
/// directory. Existence is
/// not checked: the agent creates its own config directory on first run, so a user pointing an
/// account at a directory they are about to create should not be stopped; the agent that needs one
/// to already exist (Grok Build, against a *pinned* source home) checks that itself, at launch.
fn normalize_account_dir(agent: Agent, text: &str) -> Result<String> {
    let text = text.trim();
    if text.is_empty() {
        return Err(field_required("config_dir"));
    }
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
    Ok(dir.to_string_lossy().into_owned())
}

/// The comparison key for an account name collision: trimmed and lower-cased, so "Work" and
/// " work " read as the same name.
fn account_name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// The name of the account whose name collides with `name` within `agent`, if any — the default
/// account's name takes part, compared against [`crate::protocol::DEFAULT_ACCOUNT_NAME`] since the
/// daemon carries no locale to compare the user's own translated word against. `exclude_id` lets a
/// rename compare against every *other* account without colliding with itself.
fn colliding_account_name(
    state: &Arc<AppState>,
    agent: Agent,
    name: &str,
    exclude_id: Option<&str>,
) -> Result<Option<String>> {
    let wanted = account_name_key(name);
    if wanted == account_name_key(crate::protocol::DEFAULT_ACCOUNT_NAME) {
        return Ok(Some(crate::protocol::DEFAULT_ACCOUNT_NAME.to_string()));
    }
    for existing in state.store.list_accounts()? {
        if existing.agent == agent
            && Some(existing.id.as_str()) != exclude_id
            && account_name_key(&existing.name) == wanted
        {
            return Ok(Some(existing.name));
        }
    }
    Ok(None)
}

/// Refuses a name that collides within its agent, naming the account it collides with.
fn check_account_name_unique(
    state: &Arc<AppState>,
    agent: Agent,
    name: &str,
    exclude_id: Option<&str>,
) -> Result<()> {
    if let Some(existing) = colliding_account_name(state, agent, name, exclude_id)? {
        return Err(CodedError::raised(
            error_code::ACCOUNT_NAME_TAKEN,
            format!(
                "the name `{name}` is already used by `{existing}` for {}",
                agent.label()
            ),
            &[("agent", agent.label()), ("name", &existing)],
        ));
    }
    Ok(())
}

/// A name as stored: trimmed, and refused if that leaves nothing.
fn normalize_account_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(field_required("name"));
    }
    Ok(trimmed.to_string())
}

/// Broadcasts the current settings record — the pattern every settings-shaped mutation (the
/// trusted directories, `auto_sync_repositories`, and now the accounts) follows.
fn broadcast_settings(state: &Arc<AppState>) -> Result<()> {
    state.broadcast(Event::SettingsUpdated {
        settings: state.store.get_settings()?,
    });
    Ok(())
}

fn create_account(
    state: &Arc<AppState>,
    agent: Agent,
    name: String,
    config_dir: String,
) -> Result<()> {
    let name = normalize_account_name(&name)?;
    check_account_name_unique(state, agent, &name, None)?;
    let config_dir = normalize_account_dir(agent, &config_dir)?;
    let account = Account {
        id: Uuid::new_v4().to_string(),
        agent,
        name,
        config_dir,
    };
    state.store.insert_account(&account)?;
    broadcast_settings(state)?;
    Ok(())
}

fn update_account(
    state: &Arc<AppState>,
    id: &str,
    name: Option<String>,
    config_dir: Option<String>,
) -> Result<()> {
    let mut account = state
        .store
        .get_account(id)?
        .ok_or_else(|| CodedError::unknown_account(id))?;
    let mut changed = false;
    if let Some(name) = name {
        let name = normalize_account_name(&name)?;
        if name != account.name {
            check_account_name_unique(state, account.agent, &name, Some(id))?;
            account.name = name;
            changed = true;
        }
    }
    if let Some(dir) = config_dir {
        let dir = normalize_account_dir(account.agent, &dir)?;
        if dir != account.config_dir {
            account.config_dir = dir;
            changed = true;
        }
    }
    if !changed {
        return Ok(());
    }
    state.store.update_account(&account)?;
    broadcast_settings(state)?;
    Ok(())
}

fn delete_account(state: &Arc<AppState>, id: &str) -> Result<()> {
    state
        .store
        .get_account(id)?
        .ok_or_else(|| CodedError::unknown_account(id))?;
    let affected: Vec<String> = state
        .store
        .list_consoles()?
        .into_iter()
        .filter(|console| console_references_account(console, id))
        .map(|console| console.id)
        .collect();
    state.store.delete_account(id)?;
    for console_id in affected {
        if let Some(console) = state.store.get_console(&console_id)? {
            state.broadcast(Event::ConsoleUpserted { console });
        }
    }
    broadcast_settings(state)?;
    Ok(())
}

fn console_references_account(console: &Console, account_id: &str) -> bool {
    [
        &console.claude_account_id,
        &console.codex_account_id,
        &console.grok_account_id,
    ]
    .iter()
    .any(|referenced| referenced.as_deref() == Some(account_id))
}

/// The field of `console` that would hold a reference to an account of `agent`.
fn console_account_field(console: &Console, agent: Agent) -> &Option<String> {
    match agent {
        Agent::Claude => &console.claude_account_id,
        Agent::Codex => &console.codex_account_id,
        Agent::Grok => &console.grok_account_id,
    }
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

/// Applies the settable fields of `update_settings`: absent means "leave it alone", as in
/// `update_project`. Broadcasts `settings_updated` only when something actually changed, the
/// trusted-directories pattern.
fn update_settings(state: &Arc<AppState>, auto_sync_repositories: Option<bool>) -> Result<()> {
    let Some(auto_sync_repositories) = auto_sync_repositories else {
        return Ok(());
    };
    if !state
        .store
        .set_auto_sync_repositories(auto_sync_repositories)?
    {
        return Ok(());
    }
    state.broadcast(Event::SettingsUpdated {
        settings: state.store.get_settings()?,
    });
    // Turning the sync on takes effect at once rather than at the client's next five-minute sweep:
    // the branches already known to be behind are fast-forwarded now (see
    // `git_status::sync_behind_projects`).
    if auto_sync_repositories {
        git_status::sync_behind_projects(state);
    }
    Ok(())
}

/// Starts a git-status check of every project of `console_id`, concurrently; a project already
/// being checked is left alone. The statuses follow as `project_git_status` broadcasts, so this
/// returns as soon as the checks are started rather than waiting for them.
fn refresh_git_status(state: &Arc<AppState>, console_id: &str) -> Result<()> {
    state
        .store
        .get_console(console_id)?
        .ok_or_else(|| CodedError::unknown_console(console_id))?;
    let projects: Vec<Project> = state
        .store
        .list_projects()?
        .into_iter()
        .filter(|project| project.console_id == console_id)
        .collect();
    git_status::refresh(state, projects);
    Ok(())
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

    #[test]
    fn tags_are_trimmed_and_deduplicated_ignoring_case_keeping_the_first_spelling() {
        let tags = |tags: &[&str]| normalise_tags(tags.iter().map(|tag| tag.to_string()).collect());
        assert_eq!(
            tags(&["  Rust ", "", "   ", "backend", "rust", "BACKEND", "web"]),
            ["Rust", "backend", "web"]
        );
        assert!(tags(&[]).is_empty());
    }

    fn console(console_session_agent: Agent, workdir: &Path) -> Console {
        Console {
            id: "console-1".to_string(),
            name: "Console".to_string(),
            workdir: workdir.to_string_lossy().into_owned(),
            console_session_agent,
            default_agent: Agent::Claude,
            claude_account_id: None,
            codex_account_id: None,
            grok_account_id: None,
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
    fn the_console_session_instruction_file_is_the_only_one_left_in_the_working_directory() {
        let workdir = temp_dir("console-session-instructions");

        mcp::role::write_console_session_instructions(&console(Agent::Claude, &workdir))
            .expect("written");
        assert!(workdir.join("CLAUDE.md").is_file());
        assert!(!workdir.join("AGENTS.md").exists());

        mcp::role::write_console_session_instructions(&console(Agent::Codex, &workdir))
            .expect("written");
        assert!(workdir.join("AGENTS.md").is_file());
        assert!(!workdir.join("CLAUDE.md").exists());

        // Grok reads no project instructions without a git root, and a console's working directory
        // is not a repository — so it gets none, and the previous agent's file goes.
        mcp::role::write_console_session_instructions(&console(Agent::Grok, &workdir))
            .expect("written");
        assert!(!workdir.join("AGENTS.md").exists());
        assert!(!workdir.join("CLAUDE.md").exists());

        std::fs::remove_dir_all(&workdir).ok();
    }

    /// An account directory is stored absolute and lexically normalised or not at all; unlike a
    /// console's pinned path before accounts existed, it is accepted even when it does not exist,
    /// since the agent (Grok Build excepted, checked at launch instead) creates it on first run.
    #[test]
    fn an_account_dir_is_stored_absolute_or_not_at_all_and_existence_is_not_checked() {
        let dir = temp_dir("config-dir");
        let text = dir.to_string_lossy().into_owned();
        let normalize = |text: &str| normalize_account_dir(Agent::Codex, text);

        assert_eq!(normalize(&format!("  {text} ")).unwrap(), text.clone());
        assert_eq!(normalize(&format!("{text}/")).unwrap(), text.clone());
        assert_eq!(
            normalize(&format!("{text}/./sub/..")).unwrap(),
            text.clone()
        );

        // Now intentionally accepted, where a console's pinned path used to refuse it.
        let missing = format!("{text}/missing");
        assert_eq!(normalize(&missing).unwrap(), missing);

        // The refusal names the agent whose field it was.
        let relative = normalize(".codex-alt").unwrap_err();
        assert!(relative.to_string().contains("absolute"), "{relative}");
        assert!(relative.to_string().contains("Codex"), "{relative}");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// An account always requires a non-blank directory.
    #[test]
    fn an_account_directory_may_not_be_blank() {
        assert!(normalize_account_dir(Agent::Codex, "").is_err());
        assert!(normalize_account_dir(Agent::Codex, "   ").is_err());
    }

    /// Only an archived session can be deleted, and the bulk delete takes exactly its scope: a
    /// project's archived sessions, or with no project the console's own archived console sessions.
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
                tags: Vec::new(),
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
                    bound_to: None,
                    colour: None,
                    ordinal: None,
                    account_id: None,
                    config_dir: None,
                    pinned: false,
                    started_at: 0,
                    ended_at: None,
                })
                .unwrap();
        };
        let archived = SessionStatus::Archived;
        add(
            "project-archived",
            "console-1",
            Role::Project,
            Some("project-1"),
            archived,
        );
        add(
            "project-idle",
            "console-1",
            Role::Project,
            Some("project-1"),
            SessionStatus::Idle,
        );
        add(
            "console-archived",
            "console-1",
            Role::Console,
            None,
            archived,
        );
        add(
            "other-console-archived",
            "console-2",
            Role::Console,
            None,
            archived,
        );
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
        let claim = state.begin_launch("project-archived").unwrap();
        assert!(!state.delete_if_archived("project-archived").unwrap());
        drop(claim);

        let refused = delete_session(&state, "project-idle").expect_err("not archived");
        let coded = refused.downcast_ref::<CodedError>().expect("a coded error");
        assert_eq!(coded.code, error_code::SESSION_NOT_ARCHIVED);

        delete_archived_sessions(&state, "console-1", Some("project-1")).unwrap();
        assert_eq!(
            remaining(),
            ["console-archived", "other-console-archived", "project-idle"]
        );

        delete_archived_sessions(&state, "console-1", None).unwrap();
        assert_eq!(remaining(), ["other-console-archived", "project-idle"]);

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
                tags: Vec::new(),
            })
            .unwrap();
        state
            .store
            .insert_session(&Session {
                id: "project".to_string(),
                agent: Agent::Claude,
                agent_session_id: None,
                console_id: "console-1".to_string(),
                project_id: Some("project-1".to_string()),
                host_id: LOCAL_HOST_ID.to_string(),
                role: Role::Project,
                origin: Origin::User,
                title: "Project".to_string(),
                status: SessionStatus::Archived,
                has_conversation: false,
                bound_to: None,
                colour: None,
                ordinal: None,
                account_id: None,
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

        let claim = state.begin_launch("project").unwrap();
        let refused = handle(&state, None, delete()).await.expect_err("launching");
        let coded = refused.downcast_ref::<CodedError>().expect("a coded error");
        assert_eq!(coded.code, error_code::PROJECT_HAS_RUNNING_SESSIONS);
        drop(claim);

        handle(&state, None, delete()).await.unwrap();
        assert!(state.store.get_project("project-1").unwrap().is_none());
        assert!(state.store.get_session("project").unwrap().is_none());

        std::fs::remove_dir_all(dir).ok();
    }

    /// A session whose resolved agent has been determined unavailable is refused before anything
    /// is launched or written — no record, no claim, no process.
    #[tokio::test]
    async fn opening_a_session_is_refused_when_its_resolved_agent_is_unavailable() {
        let state = Arc::new(crate::state::tests::app_state(
            "open-session-agent-unavailable",
        ));
        let dir = temp_dir("open-session-agent-unavailable");
        state
            .store
            .insert_console(&console(Agent::Claude, &dir))
            .unwrap();
        state.set_agent_availability(vec![crate::protocol::AgentAvailability {
            agent: Agent::Claude,
            availability: Availability::Unavailable,
            default_account_dir: Some("/home/user/.claude".to_string()),
        }]);

        let err = open_session(
            &state,
            OpenRequest {
                console_id: "console-1".to_string(),
                project_id: None,
                agent: None,
                account: None,
                task: None,
                title: None,
                origin: Origin::User,
                bound_to: None,
            },
        )
        .await
        .expect_err("refused");
        let coded = err.downcast_ref::<CodedError>().expect("a coded error");
        assert_eq!(coded.code, error_code::AGENT_NOT_AVAILABLE);
        assert!(err.to_string().contains("Claude Code"), "{err}");
        // Refused ahead of the write that would have recorded a console session.
        assert!(state.store.list_sessions().unwrap().is_empty());

        std::fs::remove_dir_all(dir).ok();
    }

    /// While availability has not yet been determined — the state every run begins in — nothing is
    /// refused on that account: the launch's own refusal already covers a binary that is not
    /// there.
    #[test]
    fn nothing_is_refused_for_unavailability_while_it_is_not_yet_determined() {
        let state = Arc::new(crate::state::tests::app_state("agent-not-yet-determined"));
        assert_eq!(
            state.agent_availability_of(Agent::Claude).availability,
            Availability::NotDetermined
        );
        assert!(refuse_if_agent_unavailable(&state, Agent::Claude).is_ok());
    }

    /// `~` is the home directory of the host the daemon runs on, like every other path it takes.
    #[test]
    fn a_leading_tilde_in_a_config_dir_is_expanded() {
        // `$HOME` always exists, so this needs no fixture and no environment edit.
        let home = paths::home_dir();
        assert_eq!(
            normalize_account_dir(Agent::Grok, "~").unwrap(),
            home.to_string_lossy().into_owned()
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
    fn a_session_is_pinned_to_its_own_agents_account() {
        let state = Arc::new(crate::state::tests::app_state("session-account"));
        state
            .store
            .insert_account(&Account {
                id: "claude-acct".to_string(),
                agent: Agent::Claude,
                name: "Claude work".to_string(),
                config_dir: "/home/u/.claude-alt".to_string(),
            })
            .unwrap();
        state
            .store
            .insert_account(&Account {
                id: "claude-other".to_string(),
                agent: Agent::Claude,
                name: "Claude other".to_string(),
                config_dir: "/home/u/.claude-other".to_string(),
            })
            .unwrap();
        state
            .store
            .insert_account(&Account {
                id: "codex-acct".to_string(),
                agent: Agent::Codex,
                name: "Codex work".to_string(),
                config_dir: "/home/u/.codex-alt".to_string(),
            })
            .unwrap();
        let mut console = console(Agent::Claude, Path::new("/tmp/unused"));
        console.claude_account_id = Some("claude-acct".to_string());
        console.codex_account_id = Some("codex-acct".to_string());

        let resolved = |agent: Agent, chosen: Option<Option<&str>>| {
            session_account(
                &state,
                agent,
                &console,
                chosen.map(|choice| choice.map(str::to_string)),
            )
            .unwrap()
        };
        let account = |id: &str, dir: &str| (Some(id.to_string()), Some(dir.to_string()));

        // (agent, the choice made for the session, what it launches under)
        let cases = [
            // No choice: the console's account for the session's own agent.
            (
                Agent::Claude,
                None,
                account("claude-acct", "/home/u/.claude-alt"),
            ),
            (
                Agent::Codex,
                None,
                account("codex-acct", "/home/u/.codex-alt"),
            ),
            // Nothing referenced for Grok, and another agent's account is never borrowed for it:
            // the default account, which pins nothing.
            (Agent::Grok, None, (None, None)),
            // A choice wins over the console's reference...
            (
                Agent::Claude,
                Some(Some("claude-other")),
                account("claude-other", "/home/u/.claude-other"),
            ),
            // ...and a choice of the default account wins over it too.
            (Agent::Claude, Some(None), (None, None)),
        ];
        for (agent, chosen, expected) in cases {
            assert_eq!(resolved(agent, chosen), expected, "{agent:?} {chosen:?}");
        }

        // A choice naming no account, or an account of another agent, is refused.
        for chosen in ["missing", "codex-acct"] {
            let err = session_account(
                &state,
                Agent::Claude,
                &console,
                Some(Some(chosen.to_string())),
            )
            .expect_err(chosen);
            let coded = err.downcast_ref::<CodedError>().expect("a coded error");
            assert_eq!(coded.code, error_code::UNKNOWN_ACCOUNT, "{chosen}");
        }
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

    fn bare_session(id: &str, role: Role) -> Session {
        Session {
            id: id.to_string(),
            agent: Agent::Claude,
            agent_session_id: None,
            console_id: "console-1".to_string(),
            project_id: None,
            host_id: LOCAL_HOST_ID.to_string(),
            role,
            origin: Origin::User,
            title: "Session".to_string(),
            status: SessionStatus::Idle,
            has_conversation: false,
            bound_to: None,
            colour: None,
            ordinal: None,
            account_id: None,
            config_dir: None,
            pinned: false,
            started_at: 0,
            ended_at: None,
        }
    }

    /// A console session is never bound, whatever the request named — `resolve_bound_to` does not
    /// even look at `requested` for one.
    #[test]
    fn a_console_session_is_never_bound() {
        let state = Arc::new(crate::state::tests::app_state("resolve-bound-to-console"));
        let bound = resolve_bound_to(&state, "console-1", Role::Console, Some("anything".into()))
            .expect("resolved");
        assert_eq!(bound, None);
    }

    /// With nothing named, a project session opens unbound — the ordinary case for one the user
    /// starts by hand without checking a box.
    #[test]
    fn a_project_session_named_nothing_to_bind_to_opens_unbound() {
        let state = Arc::new(crate::state::tests::app_state("resolve-bound-to-none"));
        let bound = resolve_bound_to(&state, "console-1", Role::Project, None).expect("resolved");
        assert_eq!(bound, None);
    }

    /// Naming a real console session of the same console is honoured.
    #[test]
    fn a_project_session_binds_to_the_console_session_it_names() {
        let state = Arc::new(crate::state::tests::app_state("resolve-bound-to-valid"));
        state
            .store
            .insert_console(&console(Agent::Claude, Path::new("/tmp/unused")))
            .unwrap();
        state
            .store
            .insert_session(&bare_session("console-session-1", Role::Console))
            .unwrap();

        let bound = resolve_bound_to(
            &state,
            "console-1",
            Role::Project,
            Some("console-session-1".to_string()),
        )
        .expect("resolved");
        assert_eq!(bound, Some("console-session-1".to_string()));
    }

    /// A binding is fixed for the session's lifetime once set, so naming one that turns out not to
    /// exist, to belong to another console, or to be a project session rather than a console
    /// session must be refused outright rather than silently dropped or discovered later.
    #[test]
    fn binding_to_an_invalid_target_is_refused() {
        let state = Arc::new(crate::state::tests::app_state("resolve-bound-to-invalid"));
        state
            .store
            .insert_console(&console(Agent::Claude, Path::new("/tmp/unused")))
            .unwrap();
        let mut other_console = console(Agent::Claude, Path::new("/tmp/unused"));
        other_console.id = "console-2".to_string();
        state.store.insert_console(&other_console).unwrap();

        let mut elsewhere = bare_session("console-session-elsewhere", Role::Console);
        elsewhere.console_id = "console-2".to_string();
        state.store.insert_session(&elsewhere).unwrap();
        state
            .store
            .insert_session(&bare_session("project-session-1", Role::Project))
            .unwrap();

        // (requested target, why it is not a valid binding)
        let cases = [
            ("does-not-exist", "no such session at all"),
            (
                "console-session-elsewhere",
                "a console session of another console",
            ),
            ("project-session-1", "not a console session at all"),
        ];
        for (target, why) in cases {
            let err =
                resolve_bound_to(&state, "console-1", Role::Project, Some(target.to_string()))
                    .expect_err(why);
            let coded = err.downcast_ref::<CodedError>().expect("a coded error");
            assert_eq!(coded.code, error_code::UNKNOWN_SESSION, "{why}");
        }
    }

    /// Two console sessions of the same console coexist on record and both come back from
    /// `list_sessions` — the storage layer that `Store::insert_console_session` and listing sit on
    /// never treated a second one as a conflict (that refusal lived in `open_session`, not here).
    ///
    /// This does not cover the actual removal of the one-live-console-session rule: that refusal
    /// used to sit in `open_session`, ahead of launching the agent process, and exercising its
    /// absence means actually launching a second agent for the same console, which needs a real
    /// agent binary and is impractical to do here. Nothing in this test suite currently covers that
    /// seam; `open_session`'s own refusal path is gone and read rather than tested at the daemon
    /// level.
    #[test]
    fn a_consoles_several_console_sessions_both_come_back_from_listing() {
        let state = Arc::new(crate::state::tests::app_state("several-console-sessions"));
        state
            .store
            .insert_console(&console(Agent::Claude, Path::new("/tmp/unused")))
            .unwrap();
        let first = state
            .store
            .insert_console_session(bare_session("session-0", Role::Console), None)
            .expect("first console session");
        let second = state
            .store
            .insert_console_session(bare_session("session-1", Role::Console), None)
            .expect("second console session");

        let live_console_sessions: Vec<String> = state
            .store
            .list_sessions()
            .unwrap()
            .into_iter()
            .filter(|s| s.role == Role::Console && !s.status.is_dormant())
            .map(|s| s.id)
            .collect();
        assert_eq!(live_console_sessions, [first.id, second.id]);
    }

    // -- accounts --------------------------------------------------------------

    fn create_account_request(agent: Agent, name: &str, config_dir: &str) -> RequestBody {
        RequestBody::CreateAccount {
            agent,
            name: name.to_string(),
            config_dir: config_dir.to_string(),
        }
    }

    fn only_account(state: &Arc<AppState>) -> Account {
        let accounts = state.store.list_accounts().unwrap();
        assert_eq!(accounts.len(), 1, "{accounts:?}");
        accounts.into_iter().next().unwrap()
    }

    /// Creating, renaming, repointing and removing an account over the protocol, each change
    /// visible afterwards through the settings record the account list travels with.
    #[tokio::test]
    async fn an_account_can_be_created_renamed_repointed_and_removed_over_the_protocol() {
        let state = Arc::new(crate::state::tests::app_state("account-crud"));

        handle(
            &state,
            None,
            create_account_request(Agent::Codex, "Work", "/home/u/.codex-work"),
        )
        .await
        .expect("created");
        let created = only_account(&state);
        assert_eq!(created.name, "Work");
        assert_eq!(created.config_dir, "/home/u/.codex-work");
        assert_eq!(created.agent, Agent::Codex);

        handle(
            &state,
            None,
            RequestBody::UpdateAccount {
                account: created.id.clone(),
                name: Some("Work (renamed)".to_string()),
                config_dir: None,
            },
        )
        .await
        .expect("renamed");
        let renamed = only_account(&state);
        assert_eq!(renamed.name, "Work (renamed)");
        assert_eq!(renamed.config_dir, "/home/u/.codex-work", "untouched");

        handle(
            &state,
            None,
            RequestBody::UpdateAccount {
                account: created.id.clone(),
                name: None,
                config_dir: Some("/home/u/.codex-work-2".to_string()),
            },
        )
        .await
        .expect("repointed");
        let repointed = only_account(&state);
        assert_eq!(repointed.name, "Work (renamed)", "untouched");
        assert_eq!(repointed.config_dir, "/home/u/.codex-work-2");

        handle(
            &state,
            None,
            RequestBody::DeleteAccount {
                account: created.id.clone(),
            },
        )
        .await
        .expect("removed");
        assert!(state.store.list_accounts().unwrap().is_empty());
    }

    /// A name colliding within its agent is refused, naming the account it collides with — against
    /// an existing account, against the default account's own name, and not at all across agents.
    #[tokio::test]
    async fn a_colliding_account_name_is_refused_within_its_agent_only() {
        let state = Arc::new(crate::state::tests::app_state("account-name-collision"));
        handle(
            &state,
            None,
            create_account_request(Agent::Claude, "Work", "/home/u/.claude-work"),
        )
        .await
        .expect("created");

        // Trimmed and case-insensitive, against an existing account of the same agent.
        let refused = handle(
            &state,
            None,
            create_account_request(Agent::Claude, "  work ", "/home/u/.claude-other"),
        )
        .await
        .expect_err("collides");
        let coded = refused.downcast_ref::<CodedError>().expect("coded");
        assert_eq!(coded.code, error_code::ACCOUNT_NAME_TAKEN);
        assert_eq!(coded.params.get("name").map(String::as_str), Some("Work"));

        // The default account's own name takes part in the comparison too.
        let refused_default = handle(
            &state,
            None,
            create_account_request(Agent::Claude, "default", "/home/u/.claude-other"),
        )
        .await
        .expect_err("collides with the default account");
        let coded = refused_default.downcast_ref::<CodedError>().expect("coded");
        assert_eq!(coded.code, error_code::ACCOUNT_NAME_TAKEN);

        // The same name is free for another agent: names are not compared across agents.
        handle(
            &state,
            None,
            create_account_request(Agent::Codex, "Work", "/home/u/.codex-work"),
        )
        .await
        .expect("a different agent, so no collision");
        assert_eq!(state.store.list_accounts().unwrap().len(), 2);
    }

    /// A non-absolute directory is refused with nothing saved; a directory that does not exist at
    /// all is accepted — the two rules this milestone changes about a config directory.
    #[tokio::test]
    async fn a_non_absolute_account_dir_is_refused_and_a_non_existent_one_is_accepted() {
        let state = Arc::new(crate::state::tests::app_state("account-dir-rules"));

        let refused = handle(
            &state,
            None,
            create_account_request(Agent::Claude, "Relative", "relative/path"),
        )
        .await
        .expect_err("not absolute");
        let coded = refused.downcast_ref::<CodedError>().expect("coded");
        assert_eq!(coded.code, error_code::CONFIG_DIR_NOT_ABSOLUTE);
        assert!(
            state.store.list_accounts().unwrap().is_empty(),
            "nothing saved"
        );

        handle(
            &state,
            None,
            create_account_request(
                Agent::Claude,
                "Not yet created",
                "/tmp/octoboard-does-not-exist",
            ),
        )
        .await
        .expect("a missing directory is accepted");
        assert_eq!(
            only_account(&state).config_dir,
            "/tmp/octoboard-does-not-exist"
        );
    }

    /// Removing an account a console refers to clears that console's reference, putting it back on
    /// the agent's default account; a console that never referred to it is untouched.
    #[tokio::test]
    async fn removing_an_account_a_console_refers_to_clears_the_reference() {
        let state = Arc::new(crate::state::tests::app_state(
            "account-delete-clears-console",
        ));
        handle(
            &state,
            None,
            create_account_request(Agent::Claude, "Work", "/home/u/.claude-work"),
        )
        .await
        .expect("created");
        let account_id = only_account(&state).id;

        let mut referring = console(Agent::Claude, Path::new("/tmp/unused"));
        referring.id = "console-referring".to_string();
        referring.claude_account_id = Some(account_id.clone());
        state.store.insert_console(&referring).unwrap();
        let mut untouched = console(Agent::Claude, Path::new("/tmp/unused"));
        untouched.id = "console-untouched".to_string();
        state.store.insert_console(&untouched).unwrap();

        handle(
            &state,
            None,
            RequestBody::DeleteAccount {
                account: account_id,
            },
        )
        .await
        .expect("removed");

        let after = state
            .store
            .get_console("console-referring")
            .unwrap()
            .unwrap();
        assert_eq!(after.claude_account_id, None, "back on the default account");
        assert_eq!(
            state
                .store
                .get_console("console-untouched")
                .unwrap()
                .unwrap()
                .claude_account_id,
            None,
            "a console that never referred to it is unaffected either way"
        );
    }
}
