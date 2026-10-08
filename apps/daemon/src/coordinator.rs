//! The coordinator role: what each control-socket request does to the stored consoles, projects and
//! sessions, and which host-role work it triggers — including the launch flow every way of starting
//! a session goes through.
//!
//! Writing into a running session and everything built on it (the console session's reports,
//! synthesis, automatic archiving) is `crate::reporting`'s, because the console session's tools and
//! the hook callback reach it without going through a control-socket request at all.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use uuid::Uuid;

use crate::adapter;
use crate::env_shell;
use crate::git_status;
use crate::hostfs;
use crate::mcp;
use crate::paths;
use crate::protocol::{
    error_code, now_millis, Account, Agent, Availability, CodedError, Console, Event, Origin,
    Project, ProjectSource, RequestBody, Role, Session, SessionStatus,
};
use crate::relocate;
use crate::reporting;
use crate::session::LiveSession;
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
                // These are project sessions, which have nothing bound to them, so none of
                // archiving's cascade along the binding applies; a console session belongs to no
                // project and is not in this loop.
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

        RequestBody::SwitchSessionAccount { session, account } => {
            switch_session_account(state, &session, account, env_shell::snapshot).await?;
            Ok(None)
        }

        RequestBody::DeleteSession { session } => {
            delete_session(state, &session)?;
            Ok(None)
        }

        RequestBody::DeleteArchivedSessions {
            console,
            project,
            console_session,
        } => {
            delete_archived_sessions(
                state,
                &console,
                project.as_deref(),
                console_session.as_deref(),
            )?;
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

        RequestBody::ListPages { console_session } => {
            let session = state.session_record(&console_session)?;
            if session.role != Role::Console {
                return Err(CodedError::unknown_session(&console_session));
            }
            let pages = state.store.list_pages(&console_session)?;
            Ok(Some(Event::PageList {
                id: request_id,
                console_session_id: console_session,
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
            default_clone_dir,
        } => {
            update_settings(state, auto_sync_repositories, default_clone_dir)?;
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
        ProjectSource::Git => {
            let url = remote_url
                .clone()
                .ok_or_else(|| field_required("remote_url"))?;
            let parent = match path {
                Some(path) => absolute_path(&path)?,
                None => PathBuf::from(state.store.default_clone_dir()?),
            };
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

    match start_process(state, &claim, &session, &cwd, task.as_deref(), None, None).await {
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

/// The agent's id of the conversation a relaunch of this session resumes, or none when it has not
/// got one yet. Only a session that has had a turn has something to resume: without one the agent
/// stored no conversation, and resuming by id fails with "No conversation found" — so a launch
/// without an id opens a fresh conversation instead, in the same project and under the same
/// session. A switch relocates exactly the conversation a resume would ask for.
fn resumable_agent_session_id(session: &Session) -> Option<String> {
    if session.has_conversation {
        session.agent_session_id.clone()
    } else {
        None
    }
}

/// Relaunches a dormant session. `instruction` is written into it once it is running — queued here
/// rather than by the caller, because whichever hook releases it can fire the moment the agent
/// starts, so it has to be in the queue before the launch and out again if the launch never happens.
///
/// A session bound to an archived console session brings that console session back first, so what
/// it reports reaches a process; if the console session cannot be relaunched the whole reopen
/// fails and the session stays as it was. The console session stays reopened when it is the
/// session's own relaunch that then fails. Reopening a console session reopens nothing bound to
/// it: the group comes back one session at a time, from the one the user asked for.
pub async fn resume_session(
    state: &Arc<AppState>,
    id: &str,
    instruction: Option<&str>,
) -> Result<()> {
    // Claimed before anything else, so two overlapping relaunches cannot both get through, and
    // before the record is read, so a delete cannot slip in between reading and launching.
    let claim = state.begin_launch(id)?;
    if let Some(owner) = archived_owner(state, id)? {
        // A console session is bound to nothing, so this never goes round again. Two bound
        // sessions reopened at once are kept apart by the window, not by anything held: nothing
        // awaits between this claim and the owner's status write, so the second reopen reads an
        // owner that is no longer archived and skips this step. One that does land inside the
        // window is refused on the owner's claim, and the user's next click goes through.
        let owner_claim = state.begin_launch(&owner)?;
        relaunch_session(state, &owner_claim, &owner, None, true, None).await?;
    }
    relaunch_session(state, &claim, id, instruction, true, None).await
}

/// The console session a dormant session is bound to, when that console session is archived.
fn archived_owner(state: &Arc<AppState>, id: &str) -> Result<Option<String>> {
    let session = state.session_record(id)?;
    let Some(owner) = session.bound_to.filter(|_| session.status.is_dormant()) else {
        return Ok(None);
    };
    Ok(state
        .store
        .get_session(&owner)?
        .filter(|owner| owner.status == SessionStatus::Archived)
        .map(|owner| owner.id))
}

/// The relaunch a resume and a switch of the session's account both end in: everything Octoboard
/// injects is reassembled and every launch refusal applies, whichever of the two asked. `claim` is
/// the caller's, taken before it read anything and dropped by it when it is done; `reopen` says
/// whether an archived session may be relaunched (a resume reopens one, a switch must not);
/// `shell_env` is a snapshot the caller already resolved something from (see
/// [`term::LaunchRequest::preresolved_shell_env`]).
async fn relaunch_session(
    state: &Arc<AppState>,
    claim: &crate::state::LaunchClaim,
    id: &str,
    instruction: Option<&str>,
    reopen: bool,
    shell_env: Option<HashMap<String, String>>,
) -> Result<()> {
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

    if !reopen && session.status == SessionStatus::Archived {
        return Err(session_archived(id));
    }

    let previous_status = session.status;
    let cwd = session_cwd(state, &session)?;
    let resume_id = resumable_agent_session_id(&session);
    // A relaunch carries no task, so the agent comes up at its prompt rather than working. On
    // Codex that is also the only correct value available: its `SessionStart` hook does not fire
    // until the first prompt submission, so an optimistic `working` would never be corrected.
    session.status = SessionStatus::Idle;
    session.ended_at = None;
    if !reopen
        && state
            .store
            .update_session_status_if(id, previous_status, SessionStatus::Idle)?
            .is_none()
    {
        // Archived since it was read, which can land without the claim: an archive that landed
        // before this point stands rather than being undone by a relaunch the user did not ask
        // for. One landing after it is not excluded; closing that takes a claim on archiving, and
        // a resume has always had the same gap.
        return Err(session_archived(id));
    }
    state.store.update_session(&session)?;

    // Queued with every refusal above already past, so a call that never launched leaves nothing
    // behind for a later launch to deliver.
    if let Some(instruction) = instruction {
        state.queue_message(&session.id, instruction);
    }

    match start_process(
        state,
        claim,
        &session,
        &cwd,
        None,
        resume_id.as_deref(),
        shell_env,
    )
    .await
    {
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
    _claim: &crate::state::LaunchClaim,
    session: &Session,
    cwd: &Path,
    task: Option<&str>,
    resume_agent_session_id: Option<&str>,
    shell_env: Option<HashMap<String, String>>,
) -> Result<()> {
    let request = term::LaunchRequest {
        session_id: session.id.clone(),
        agent: session.agent,
        role: session.role,
        bound: session.bound_to.is_some(),
        cwd: cwd.to_path_buf(),
        task: task.map(str::to_string),
        resume_agent_session_id: resume_agent_session_id.map(str::to_string),
        config_dir: session.config_dir.as_ref().map(PathBuf::from),
        preresolved_shell_env: shell_env,
        daemon_port: state.port,
        self_exe: state.self_exe.clone(),
        // Issued per launch: the previous process is gone, and a token outliving it would let a
        // stale child act on this session.
        mcp_token: state.issue_mcp_token(&session.id),
    };

    // Launching snapshots the user's shell environment and forks a process, so it goes off the
    // runtime rather than holding up the socket it was asked on.
    let launch = tokio::task::spawn_blocking(move || term::launch(request)).await??;

    // The live map now says the session is running; `claim` stays with the caller, which drops it
    // when it is done with the launch.
    state.register_live(launch.session.clone());
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

/// Ends the session's process and archives it. What archiving means beyond ending the process
/// belongs here and not in [`stop_process`], which a switch of the session's account shares.
///
/// Archiving a console session is a group operation along the binding, decided by whether a
/// process is running and not by status: refused, with nothing changed, while any session bound to
/// it has one (working, awaiting instructions or waiting for the user alike; the refusal counts and
/// names them), and otherwise archiving every bound session that is not archived yet, before the
/// console session itself. Nothing bound to it is left outside the archive. The order is
/// deliberate: bound sessions first and the console session last, so a failure part way leaves
/// only a state the user could have produced by hand, never an archived console session with a
/// bound session still outside the archive. A project session has nothing bound to it, so archiving
/// one — the user's request, the console session's tool, a `done` report with no open items, a
/// `delete_project` that stops sessions — never reaches another session.
pub fn archive_session(state: &Arc<AppState>, id: &str) -> Result<()> {
    let session = state.session_record(id)?;
    if session.role == Role::Console {
        // "Not running" is taken to mean interrupted by exclusion: a status that is neither
        // archived nor backed by a process would be archived here without anyone having decided so.
        // One gap is left open: `open_session` inserts the session record, `bound_to` included,
        // before it takes the launch claim, so in between a bound session reads as neither archived
        // nor process-backed. Nothing awaits in that gap and the UI binds only to a live console
        // session, so it is not closed.
        let (running, dormant): (Vec<Session>, Vec<Session>) = bound_sessions(state, id)?
            .into_iter()
            .filter(|bound| bound.status != SessionStatus::Archived)
            .partition(|bound| state.has_process(&bound.id));
        if !running.is_empty() {
            return Err(CodedError::raised(
                error_code::CONSOLE_SESSION_HAS_RUNNING_SESSIONS,
                format!(
                    "this console session cannot be archived while sessions bound to it have a \
                     process running: {}; archive them or wait for them to finish first",
                    quoted_titles(&running)
                ),
                &[
                    ("count", &running.len().to_string()),
                    ("sessions", &quoted_titles(&running)),
                ],
            ));
        }
        for bound in dormant {
            archive_record(state, &bound.id)?;
        }
    }
    archive_record(state, id)
}

/// The sessions bound to the console session `id`, in any status.
fn bound_sessions(state: &Arc<AppState>, id: &str) -> Result<Vec<Session>> {
    Ok(state
        .store
        .list_sessions()?
        .into_iter()
        .filter(|session| session.bound_to.as_deref() == Some(id))
        .collect())
}

/// The titles of `sessions`, each in quotes, comma separated. The separator is half-width and
/// English whatever the reading language; that is accepted, because a client with the message in
/// its catalogue words the list itself and this serves the fallback text, which needs the titles.
fn quoted_titles(sessions: &[Session]) -> String {
    sessions
        .iter()
        .map(|session| format!("\u{201c}{}\u{201d}", session.title))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Marks one session archived and ends its process, if it has one.
fn archive_record(state: &Arc<AppState>, id: &str) -> Result<()> {
    let mut session = state.session_record(id)?;
    session.status = SessionStatus::Archived;
    session.ended_at = Some(now_millis());
    state.save_session(&session)?;
    if let Some(live) = state.live_session(id) {
        // Ending the process waits out the graceful period, which the caller should not.
        stop_process(live);
    }
    Ok(())
}

/// Ends one session's process, waiting out the graceful period the agents' shutdown hooks depend
/// on, on a blocking thread. This is the whole of "ending a process" and nothing more: it decides
/// no status, archives nothing and reaches no other session. What archiving adds around it — the
/// status, and whatever later follows an archived session to the sessions bound to it — is
/// `archive_session`'s, which is why a switch of the session's account stops the process through
/// this and not through archiving.
fn stop_process(live: Arc<LiveSession>) -> tokio::task::JoinHandle<()> {
    tokio::task::spawn_blocking(move || live.terminate())
}

/// How long a relaunched session has to stay up before a switch counts it as having come up. An
/// agent that cannot find the conversation it was asked to resume says so and exits within a
/// moment, each in its own words, which the switch does not read: its process ending is the signal,
/// as it is everywhere else the daemon decides a session is interrupted.
///
/// One failing relaunch could outlive the window: Grok Build restores a session it cannot find
/// locally from a remote registry, a network round trip. That is harmless here, because the
/// switch's own copy means Grok finds the session locally; and if a relaunch did fail after the
/// window, the session would be interrupted on the new account, where the copy is, and so
/// resumable.
///
/// An account that is not signed in is not caught here, for any of the three agents: each comes up
/// on its own sign-in instead of exiting — Grok Build included, which waits on a browser
/// device-code approval — so a switch to such an account counts as having come up.
const SWITCH_SETTLE: Duration = Duration::from_secs(4);

/// How long a switch waits for the process it ended to be seen gone and recorded as interrupted,
/// which the exit watcher does on its own schedule. Far above that schedule plus the graceful
/// period: reaching it means the process would not die, which is not a state to carry on from
/// (see [`error_code::SESSION_DID_NOT_STOP`]).
const SWITCH_STOP_TIMEOUT: Duration = Duration::from_secs(30);

/// What a switch worked out before it ended anything.
struct SwitchPlan {
    /// The snapshot the default account was resolved from, handed on to the relaunch so the copy
    /// and the launch cannot disagree about where the default account is.
    shell_env: Option<HashMap<String, String>>,
    /// The conversation to copy: from the source account's directory to the target's, at the
    /// path relative to both. None for a session with no conversation on the agent's side.
    relocation: Option<(PathBuf, PathBuf, PathBuf)>,
}

/// Everything a switch can refuse before it ends the session's process: the launch rule on the
/// target directory, and the conversation record being where it is expected. The default account
/// pins nothing, so each side of it resolves from one shell snapshot taken here, once, and kept
/// for the relaunch. `source` and `target` are the pinned directories, `None` for the default
/// account. Blocking.
fn prepare_switch(
    agent: Agent,
    resume_id: Option<&str>,
    source: Option<&str>,
    target: Option<&str>,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>>,
) -> Result<SwitchPlan> {
    // Before anything is copied: the copy would create the directory's `sessions/` and so make a
    // Grok home that was never initialized pass the same check at launch.
    adapter::refuse_unlaunchable_account(agent, target.map(Path::new))?;
    let shell_env = if source.is_none() || target.is_none() {
        Some(snapshot().context("snapshotting the user's shell environment")?)
    } else {
        None
    };
    let Some(resume_id) = resume_id else {
        return Ok(SwitchPlan {
            shell_env,
            relocation: None,
        });
    };
    let resolve = |pinned: Option<&str>| match (pinned, &shell_env) {
        (Some(dir), _) => PathBuf::from(dir),
        (None, Some(shell_env)) => adapter::default_account_dir(agent, shell_env),
        (None, None) => unreachable!("a default side takes the snapshot"),
    };
    let (from, to) = (resolve(source), resolve(target));
    let relative = relocate::locate(agent, &from, resume_id)?;
    Ok(SwitchPlan {
        shell_env,
        relocation: Some((from, to, relative)),
    })
}

/// Moves a session to another account of its own agent: ends its process the way archiving does,
/// copies its conversation into the target account's directory, records the account and the
/// directory on the session, and relaunches it as a resume does. The user's action alone —
/// nothing calls this on a usage signal.
///
/// `account` is the target by id, `None` for the default account. Refused, with nothing recorded
/// and (where it can be told beforehand) the process left running: a launch, resume or switch of
/// the session under way, an archived session, the account it is on already, an account that is
/// not one of the session's agent, a Grok target that is not an initialized home, and a
/// conversation record that is not where it is expected. Once the process has been ended, a
/// relocation that does not complete or a relaunch that is refused or does not come up leaves
/// the session interrupted on the account it had, with the conversation still in both
/// directories.
///
/// Two windows break "always back on the account it had", neither harmful. A daemon that dies
/// between recording the new account and the relaunch leaves the session on the new account, which
/// already holds the copy, so the conversation resumes there. And a switch to the *default* Grok
/// account is not checked for being an initialized home (`refuse_unlaunchable_account` is a no-op
/// for an account that pins nothing, by design: that directory is the user's own setup), so an
/// uninitialized `~/.grok` gets the copy, and the relaunch either comes up on Grok's own sign-in,
/// which counts as a switch made like any target that is not signed in, or dies, which is reported
/// as a failed switch and put back.
///
/// It archives nothing: the process is ended by [`stop_process`], not by [`archive_session`], and
/// the status the session passes through is the interrupted one the exit watcher gives any
/// process that ends without the session having been archived.
pub async fn switch_session_account(
    state: &Arc<AppState>,
    id: &str,
    account: Option<Option<String>>,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
) -> Result<()> {
    let account = account.ok_or_else(|| field_required("account"))?;
    // Held until this function returns, after the come-up verdict and any revert, so nothing else
    // can launch, resume, switch or delete the session while its process is down, while the
    // relaunched one is being watched, or while the account is put back; refused here when one
    // of those is under way.
    let claim = state.begin_switch(id)?;
    let before = state.session_record(id)?;
    if before.status == SessionStatus::Archived {
        return Err(session_archived(id));
    }
    let target_account = checked_account_reference(state, before.agent, account)?;
    if target_account == before.account_id {
        return Err(CodedError::raised(
            error_code::SESSION_ALREADY_ON_ACCOUNT,
            "this session is on that account already",
            &[("session", id)],
        ));
    }
    let target_dir = account_dir(state, &target_account)?;

    let plan = {
        let (agent, resume_id) = (before.agent, resumable_agent_session_id(&before));
        let (source, target) = (before.config_dir.clone(), target_dir.clone());
        tokio::task::spawn_blocking(move || {
            prepare_switch(
                agent,
                resume_id.as_deref(),
                source.as_deref(),
                target.as_deref(),
                snapshot,
            )
        })
        .await??
    };

    if let Some(live) = state.live_session(id) {
        stop_process(live).await?;
    }
    wait_until_down(state, id).await?;
    // Archiving it meanwhile is the user's later word on the matter.
    if state.session_record(id)?.status == SessionStatus::Archived {
        return Err(session_archived(id));
    }

    if let Some((from, to, relative)) = plan.relocation {
        tokio::task::spawn_blocking(move || relocate::copy(&from, &to, &relative)).await??;
    }
    state.set_session_account(id, target_account.as_deref(), target_dir.as_deref())?;

    if let Err(err) = relaunch_session(state, &claim, id, None, false, plan.shell_env).await {
        put_back_on_account(state, &before);
        return Err(err);
    }
    let came_up = match state.live_session(id) {
        Some(live) => came_up(&live, SWITCH_SETTLE).await,
        None => false,
    };
    if !came_up {
        put_back_once_down(state, &before).await;
        return Err(CodedError::raised(
            error_code::SWITCH_DID_NOT_COME_UP,
            "the session ended as soon as it was relaunched, so it stays on the account it had",
            &[("session", id)],
        ));
    }
    Ok(())
}

/// Records the account and directory a switched session had before, `before`'s. A revert that
/// fails is logged, never allowed to hide the reason the switch failed.
fn put_back_on_account(state: &Arc<AppState>, before: &Session) {
    if let Err(revert) = state.set_session_account(
        &before.id,
        before.account_id.as_deref(),
        before.config_dir.as_deref(),
    ) {
        tracing::warn!(session = %before.id, %revert, "putting the session back on its account failed");
    }
}

/// [`put_back_on_account`] for a relaunched process that has already ended, once the exit watcher
/// has recorded the session interrupted. The watcher does that on its own schedule, so reverting
/// first would let every client see the session idle on its old account with no process until the
/// watcher caught up — after the reply saying the switch failed — and a watcher that read the
/// record just before the revert would publish it with the new account. Waiting first means the
/// restored record is broadcast before the caller replies.
async fn put_back_once_down(state: &Arc<AppState>, before: &Session) {
    if let Err(err) = wait_until_down(state, &before.id).await {
        tracing::warn!(session = %before.id, %err, "the relaunched process was not recorded as ended");
    }
    put_back_on_account(state, before);
}

fn session_archived(id: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::SESSION_ARCHIVED,
        "an archived session cannot be switched; reopen it first",
        &[("session", id)],
    )
}

/// Waits for the exit watcher to have seen the session's process gone and recorded the session as
/// dormant — the state a relaunch starts from, and the one thing that must be true before the
/// watcher can no longer write over what the relaunch records.
async fn wait_until_down(state: &Arc<AppState>, id: &str) -> Result<()> {
    let deadline = Instant::now() + SWITCH_STOP_TIMEOUT;
    while state.live_session(id).is_some() || !state.session_record(id)?.status.is_dormant() {
        if Instant::now() >= deadline {
            // The process may still be running, with nothing recorded and nothing copied.
            return Err(CodedError::raised(
                error_code::SESSION_DID_NOT_STOP,
                "the session's process did not end in time, so it was not switched",
                &[("session", id)],
            ));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Ok(())
}

/// Whether the relaunched process is still running after `settle`.
async fn came_up(live: &LiveSession, settle: Duration) -> bool {
    let deadline = Instant::now() + settle;
    while Instant::now() < deadline {
        if live.poll_exit() {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    !live.poll_exit()
}

/// Removes Octoboard's record of one archived session, and with an archived console session the
/// archived sessions bound to it: left behind, reopening one would have no console session to come
/// back with. Deleting an archived bound session on its own leaves its console session alone.
/// Nothing else of Octoboard's hangs off a session: its queue, MCP token and turn bookkeeping go
/// when its process does, and so does its scratch directory (clearing the run directory at startup
/// is only the fallback). The agent's own transcript and the project are never touched.
fn delete_session(state: &Arc<AppState>, id: &str) -> Result<()> {
    let session = state.session_record(id)?;
    if !delete_archived_group(state, &session)? {
        return Err(CodedError::raised(
            error_code::SESSION_NOT_ARCHIVED,
            "only an archived session can be deleted",
            &[("session", id)],
        ));
    }
    Ok(())
}

/// Deletes `session` if it is archived and says whether it did; a console session takes the
/// archived sessions bound to it along. Deleting the console session first means a refusal, which
/// only that first step can give, deletes nothing. It does not make the whole atomic: if a later
/// step is skipped (a session being launched right now) or fails, the console session is already
/// gone. That is left, because a bound session can only be mid-launch through a resume, which
/// reopens the console session first, so the console session was no longer archived and the
/// delete was refused. One that stopped being archived meanwhile is skipped.
fn delete_archived_group(state: &Arc<AppState>, session: &Session) -> Result<bool> {
    if !state.delete_if_archived(&session.id)? {
        return Ok(false);
    }
    if session.role == Role::Console {
        for bound in bound_sessions(state, &session.id)? {
            state.delete_if_archived(&bound.id)?;
        }
    }
    Ok(true)
}

/// Deletes every archived session of `project`, or every archived session bound to the console
/// session `console_session_id`, or with neither every archived console session of `console` along
/// with the archived sessions bound to them. One that stopped being archived meanwhile (a resume
/// got there first) is skipped.
fn delete_archived_sessions(
    state: &Arc<AppState>,
    console_id: &str,
    project_id: Option<&str>,
    console_session_id: Option<&str>,
) -> Result<()> {
    state
        .store
        .get_console(console_id)?
        .ok_or_else(|| CodedError::unknown_console(console_id))?;
    if project_id.is_some() && console_session_id.is_some() {
        return Err(CodedError::raised(
            error_code::CONFLICTING_FIELDS,
            "`project` and `console_session` cannot both be given",
            &[("first", "project"), ("second", "console_session")],
        ));
    }
    if let Some(project_id) = project_id {
        let project = state
            .store
            .get_project(project_id)?
            .ok_or_else(|| CodedError::unknown_project(project_id))?;
        if project.console_id != console_id {
            return Err(CodedError::unknown_project(project_id));
        }
    }
    if let Some(console_session_id) = console_session_id {
        let owner = state.session_record(console_session_id)?;
        if owner.console_id != console_id || owner.role != Role::Console {
            return Err(CodedError::unknown_session(console_session_id));
        }
    }
    for session in state.store.list_sessions()? {
        let in_scope = session.console_id == console_id
            && match (project_id, console_session_id) {
                (Some(project_id), _) => session.project_id.as_deref() == Some(project_id),
                (None, Some(owner)) => session.bound_to.as_deref() == Some(owner),
                (None, None) => session.role == Role::Console,
            };
        if in_scope {
            delete_archived_group(state, &session)?;
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

/// A report panel form submission. Refused unless `page_id` names its console session's newest page —
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
    let newest = state.store.newest_page_id(&page.console_session_id)?;
    if newest.as_deref() != Some(page.id.as_str()) {
        return Err(CodedError::raised(
            error_code::PAGE_NOT_CURRENT,
            "this page is no longer current; its form can no longer be submitted",
            &[],
        ));
    }
    let console_session_id = page.console_session_id;

    let message = reporting::render_page_submission(&page.id, &data);
    let owned_state = state.clone();
    // The write blocks on the PTY. Queued rather than refused: the user is the one submitting, so
    // there is nobody to tell to answer a prompt first, and the submission must not be dropped.
    tokio::task::spawn_blocking(move || {
        reporting::write_message(
            &owned_state,
            &console_session_id,
            &message,
            reporting::WhenBlocked::Queue,
        )
    })
    .await??;
    Ok(())
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
    let config_dir = account_dir(state, &account_id)?;
    Ok((account_id, config_dir))
}

/// The directory of the stored account `account_id` names, `None` for the default account, which
/// pins nothing.
fn account_dir(state: &Arc<AppState>, account_id: &Option<String>) -> Result<Option<String>> {
    Ok(match account_id {
        None => None,
        Some(id) => state
            .store
            .get_account(id)?
            .map(|account| account.config_dir),
    })
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
/// directory. Existence is not checked: the directory is created on first launch (by Claude Code
/// itself, by the Codex adapter for Codex), so a user pointing an account at a directory they are
/// about to create should not be stopped; the agent that needs one to already exist (Grok Build,
/// against a *pinned* source home) checks that itself, at launch.
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
fn update_settings(
    state: &Arc<AppState>,
    auto_sync_repositories: Option<bool>,
    default_clone_dir: Option<String>,
) -> Result<()> {
    // Checked before anything is written, so a refused directory leaves the other field unapplied
    // too rather than half the request taking effect.
    let default_clone_dir = match default_clone_dir {
        Some(text) if text.trim().is_empty() => Some(None),
        Some(text) => Some(Some(absolute_path(&text)?.to_string_lossy().into_owned())),
        None => None,
    };
    let mut changed = false;
    let mut sync_turned_on = false;
    if let Some(value) = auto_sync_repositories {
        let updated = state.store.set_auto_sync_repositories(value)?;
        sync_turned_on = updated && value;
        changed |= updated;
    }
    if let Some(value) = default_clone_dir {
        changed |= state.store.set_default_clone_dir(value.as_deref())?;
    }
    if !changed {
        return Ok(());
    }
    state.broadcast(Event::SettingsUpdated {
        settings: state.store.get_settings()?,
    });
    // Turning the sync on takes effect at once rather than at the client's next five-minute sweep:
    // the branches already known to be behind are fast-forwarded now (see
    // `git_status::sync_behind_projects`).
    if sync_turned_on {
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
    use crate::test_support::{fake_live_session, idle_stand_in, ScratchDir};

    #[test]
    fn tags_are_trimmed_and_deduplicated_ignoring_case_keeping_the_first_spelling() {
        let tags = |tags: &[&str]| normalise_tags(tags.iter().map(|tag| tag.to_string()).collect());
        assert_eq!(
            tags(&["  Rust ", "", "   ", "backend", "rust", "BACKEND", "web"]),
            ["Rust", "backend", "web"]
        );
        assert!(tags(&[]).is_empty());
    }

    #[test]
    fn update_settings_applies_all_or_nothing_and_a_blank_clone_dir_clears_it() {
        let (state, _dir) = crate::test_support::app_state("coordinator-update-settings");
        let built_in = state.store.default_clone_dir().unwrap();

        update_settings(&state, None, Some("/work/repos/".to_string())).unwrap();
        assert_eq!(state.store.default_clone_dir().unwrap(), "/work/repos");

        // A refused directory leaves the other field of the same request unapplied too.
        assert!(update_settings(&state, Some(true), Some("relative".to_string())).is_err());
        assert!(!state.store.get_settings().unwrap().auto_sync_repositories);
        assert_eq!(state.store.default_clone_dir().unwrap(), "/work/repos");

        update_settings(&state, None, Some("  ".to_string())).unwrap();
        assert_eq!(state.store.default_clone_dir().unwrap(), built_in);
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

    /// A console workdir inside a store's scratch directory, kept apart from the database, which
    /// deleting the console would otherwise wipe.
    fn console_workdir(dir: &Path) -> PathBuf {
        let workdir = dir.join("workdir");
        std::fs::create_dir(&workdir).expect("workdir");
        workdir
    }

    fn temp_dir(name: &str) -> ScratchDir {
        ScratchDir::new(&format!("coordinator-{name}"))
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
    }

    /// An account directory is stored absolute and lexically normalised or not at all; unlike a
    /// console's pinned path before accounts existed, it is accepted even when it does not exist,
    /// since it is created on first launch (Grok Build excepted, checked at launch instead).
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
        let (state, dir) = crate::test_support::app_state("coordinator-delete-sessions");
        let workdir = console_workdir(&dir);
        for id in ["console-1", "console-2"] {
            let mut other = console(Agent::Claude, &workdir);
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

        delete_archived_sessions(&state, "console-1", Some("project-1"), None).unwrap();
        assert_eq!(
            remaining(),
            ["console-archived", "other-console-archived", "project-idle"]
        );

        delete_archived_sessions(&state, "console-1", None, None).unwrap();
        assert_eq!(remaining(), ["other-console-archived", "project-idle"]);
    }

    /// A project's deletion can be told to stop its sessions, but never goes ahead under a session
    /// that is being launched; with nothing in flight it removes the project and its sessions.
    #[tokio::test]
    async fn deleting_a_project_with_stop_sessions_refuses_a_launch_in_flight() {
        let (state, dir) = crate::test_support::app_state("coordinator-delete-project-stop");
        let workdir = console_workdir(&dir);
        state
            .store
            .insert_console(&console(Agent::Claude, &workdir))
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
    }

    /// A session whose resolved agent has been determined unavailable is refused before anything
    /// is launched or written — no record, no claim, no process.
    #[tokio::test]
    async fn opening_a_session_is_refused_when_its_resolved_agent_is_unavailable() {
        let (state, dir) =
            crate::test_support::app_state("coordinator-open-session-agent-unavailable");
        let workdir = console_workdir(&dir);
        state
            .store
            .insert_console(&console(Agent::Claude, &workdir))
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
    }

    /// While availability has not yet been determined — the state every run begins in — nothing is
    /// refused on that account: the launch's own refusal already covers a binary that is not
    /// there.
    #[test]
    fn nothing_is_refused_for_unavailability_while_it_is_not_yet_determined() {
        let (state, _dir) = crate::test_support::app_state("coordinator-agent-not-yet-determined");
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
        let (state, _dir) = crate::test_support::app_state("coordinator-session-account");
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
        let (state, _dir) = crate::test_support::app_state("coordinator-resolve-bound-to-console");
        let bound = resolve_bound_to(&state, "console-1", Role::Console, Some("anything".into()))
            .expect("resolved");
        assert_eq!(bound, None);
    }

    /// With nothing named, a project session opens unbound — the ordinary case for one the user
    /// starts by hand without checking a box.
    #[test]
    fn a_project_session_named_nothing_to_bind_to_opens_unbound() {
        let (state, _dir) = crate::test_support::app_state("coordinator-resolve-bound-to-none");
        let bound = resolve_bound_to(&state, "console-1", Role::Project, None).expect("resolved");
        assert_eq!(bound, None);
    }

    /// Naming a real console session of the same console is honoured.
    #[test]
    fn a_project_session_binds_to_the_console_session_it_names() {
        let (state, _dir) = crate::test_support::app_state("coordinator-resolve-bound-to-valid");
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
        let (state, _dir) = crate::test_support::app_state("coordinator-resolve-bound-to-invalid");
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
        let (state, _dir) = crate::test_support::app_state("coordinator-several-console-sessions");
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
        let (state, _dir) = crate::test_support::app_state("coordinator-account-crud");

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
        let (state, _dir) = crate::test_support::app_state("coordinator-account-name-collision");
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
        let (state, _dir) = crate::test_support::app_state("coordinator-account-dir-rules");

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
        let (state, _dir) =
            crate::test_support::app_state("coordinator-account-delete-clears-console");
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

    /// A console and a project (whose directory does not exist, so a relaunch is refused at the
    /// launch's first check before it writes anything), and one account of each agent whose
    /// directory is under `dir`.
    fn switch_fixture(name: &str) -> (Arc<AppState>, ScratchDir) {
        let (state, dir) = crate::test_support::app_state(&format!("coordinator-{name}"));
        state
            .store
            .insert_console(&console(Agent::Claude, &dir.join("no-such-workdir")))
            .unwrap();
        state
            .store
            .insert_project(&Project {
                id: "project-1".to_string(),
                console_id: "console-1".to_string(),
                host_id: LOCAL_HOST_ID.to_string(),
                name: "Project".to_string(),
                path: dir.join("no-such-project").to_string_lossy().into_owned(),
                default_agent: None,
                source: ProjectSource::Local,
                remote_url: None,
                claude_trust_consent: false,
                pinned: false,
                tags: Vec::new(),
            })
            .unwrap();
        for (id, agent) in [
            ("claude-a", Agent::Claude),
            ("codex-a", Agent::Codex),
            ("grok-a", Agent::Grok),
        ] {
            state
                .store
                .insert_account(&Account {
                    id: id.to_string(),
                    agent,
                    name: id.to_string(),
                    config_dir: dir.join(id).to_string_lossy().into_owned(),
                })
                .unwrap();
        }
        (state, dir)
    }

    fn project_session(id: &str, agent: Agent, status: SessionStatus) -> Session {
        Session {
            agent,
            project_id: Some("project-1".to_string()),
            role: Role::Project,
            status,
            ..bare_session(id, Role::Project)
        }
    }

    fn code_of(err: &anyhow::Error) -> &'static str {
        err.downcast_ref::<CodedError>()
            .expect("a coded error")
            .code
    }

    fn shell_env(dir: &Path) -> impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static {
        let env = HashMap::from([(
            "CLAUDE_CONFIG_DIR".to_string(),
            dir.join("default-claude").to_string_lossy().into_owned(),
        )]);
        move || Ok(env)
    }

    /// Every refusal that can be told before the process is ended names its reason and records
    /// nothing: the session keeps its account, its directory and its status, and a Grok home that
    /// was never initialized is not given a `sessions/` directory by the refused attempt.
    #[tokio::test]
    async fn a_refused_switch_records_nothing() {
        let (state, dir) = switch_fixture("switch-refused");
        std::fs::create_dir_all(dir.join("grok-a")).unwrap();

        // (what is refused, the session, the request's account, the code it is refused with)
        let archived = project_session("s", Agent::Claude, SessionStatus::Archived);
        let mut with_conversation = project_session("s", Agent::Claude, SessionStatus::Interrupted);
        with_conversation.has_conversation = true;
        with_conversation.agent_session_id = Some("agent-1".to_string());
        let cases = [
            (
                "an account that does not exist",
                project_session("s", Agent::Claude, SessionStatus::Interrupted),
                Some(Some("nope".to_string())),
                error_code::UNKNOWN_ACCOUNT,
            ),
            (
                "another agent's account",
                project_session("s", Agent::Claude, SessionStatus::Interrupted),
                Some(Some("codex-a".to_string())),
                error_code::UNKNOWN_ACCOUNT,
            ),
            (
                "the account it is on",
                project_session("s", Agent::Claude, SessionStatus::Interrupted),
                Some(None),
                error_code::SESSION_ALREADY_ON_ACCOUNT,
            ),
            (
                "no account named at all",
                project_session("s", Agent::Claude, SessionStatus::Interrupted),
                None,
                error_code::FIELD_REQUIRED,
            ),
            (
                "an archived session",
                archived,
                Some(Some("claude-a".to_string())),
                error_code::SESSION_ARCHIVED,
            ),
            (
                "a conversation that is not in its account",
                with_conversation,
                Some(Some("claude-a".to_string())),
                error_code::CONVERSATION_NOT_FOUND,
            ),
            (
                "a Grok home that was never initialized",
                project_session("s", Agent::Grok, SessionStatus::Interrupted),
                Some(Some("grok-a".to_string())),
                error_code::GROK_HOME_NOT_INITIALIZED,
            ),
        ];
        for (why, session, account, code) in cases {
            state.store.delete_session("s").unwrap();
            state.store.insert_session(&session).unwrap();

            let err = switch_session_account(&state, "s", account, shell_env(&dir))
                .await
                .expect_err(why);
            assert_eq!(code_of(&err), code, "{why}");

            let after = state.store.get_session("s").unwrap().unwrap();
            assert_eq!(after.account_id, session.account_id, "{why}");
            assert_eq!(after.config_dir, session.config_dir, "{why}");
            assert_eq!(after.status, session.status, "{why}");
        }
        assert!(!dir.join("grok-a/sessions").exists());
        assert!(!dir.join("claude-a").exists());

        // A launch, resume or switch of the session under way refuses another switch of it.
        state.store.delete_session("s").unwrap();
        state
            .store
            .insert_session(&project_session(
                "s",
                Agent::Claude,
                SessionStatus::Interrupted,
            ))
            .unwrap();
        let claim = state.begin_launch("s").unwrap();
        let err = switch_session_account(
            &state,
            "s",
            Some(Some("claude-a".to_string())),
            shell_env(&dir),
        )
        .await
        .expect_err("launching");
        assert_eq!(code_of(&err), error_code::SESSION_ALREADY_STARTING);
        drop(claim);
    }

    /// The default account resolves from one shell snapshot, taken once, and that same snapshot is
    /// what the relaunch is handed; an account with a directory of its own needs none. A session
    /// with no conversation has nothing to relocate whichever side is the default.
    #[test]
    fn the_default_account_is_resolved_once_for_the_copy_and_the_relaunch() {
        let dir = temp_dir("switch-prepare");
        let default_dir = dir.join("default-claude");
        let record = default_dir.join("projects/slug/agent-1.jsonl");
        std::fs::create_dir_all(record.parent().unwrap()).unwrap();
        std::fs::write(&record, "conversation").unwrap();
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let snapshot = || {
            calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(HashMap::from([(
                "CLAUDE_CONFIG_DIR".to_string(),
                default_dir.to_string_lossy().into_owned(),
            )]))
        };
        let target = dir.join("claude-a").to_string_lossy().into_owned();

        let plan = prepare_switch(
            Agent::Claude,
            Some("agent-1"),
            None,
            Some(&target),
            snapshot,
        )
        .unwrap();
        let (from, to, relative) = plan.relocation.expect("a conversation to relocate");
        assert_eq!(
            (from, to, relative),
            (
                default_dir.clone(),
                PathBuf::from(&target),
                PathBuf::from("projects/slug/agent-1.jsonl")
            )
        );
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            plan.shell_env.expect("kept for the relaunch")["CLAUDE_CONFIG_DIR"],
            default_dir.to_string_lossy()
        );

        // Both sides pinned: no default to resolve, so no snapshot.
        let plan = prepare_switch(Agent::Claude, None, Some(&target), Some(&target), || {
            panic!("no snapshot needed")
        })
        .unwrap();
        assert!(plan.shell_env.is_none() && plan.relocation.is_none());

        // The default account as the target, for a session with no conversation: still one
        // snapshot, for the launch, and nothing to copy.
        let plan = prepare_switch(Agent::Claude, None, Some(&target), None, snapshot).unwrap();
        assert!(plan.shell_env.is_some() && plan.relocation.is_none());
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    /// A relaunch that is refused after the conversation was copied and the account recorded puts
    /// the account back, leaves the session interrupted rather than archived, and leaves the
    /// conversation in the account it came from as well as the one it was copied to. The
    /// switched session here is a live console session, whose process is ended on the way: the
    /// session bound to it is not touched, which is what keeps a switch from ever being an
    /// archive.
    #[tokio::test]
    async fn a_switch_whose_relaunch_fails_is_back_on_its_account_and_archives_nothing() {
        let (state, dir) = switch_fixture("switch-relaunch-refused");
        let source = dir.join("default-claude");
        let record = "projects/slug/agent-1.jsonl";
        std::fs::create_dir_all(source.join(record).parent().unwrap()).unwrap();
        std::fs::write(source.join(record), "conversation").unwrap();

        let mut hub = bare_session("hub", Role::Console);
        hub.has_conversation = true;
        hub.agent_session_id = Some("agent-1".to_string());
        hub.status = SessionStatus::Working;
        state.store.insert_session(&hub).unwrap();
        let mut bound = project_session("bound", Agent::Claude, SessionStatus::Interrupted);
        bound.bound_to = Some("hub".to_string());
        state.store.insert_session(&bound).unwrap();
        let live = idle_stand_in("hub");
        state.register_live(live.clone());
        state.watch_exit(live.clone());

        let err = switch_session_account(
            &state,
            "hub",
            Some(Some("claude-a".into())),
            shell_env(&dir),
        )
        .await
        .expect_err("the console's working directory does not exist");
        assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);

        assert!(live.poll_exit(), "its process was ended");
        let after = state.store.get_session("hub").unwrap().unwrap();
        assert_eq!(after.status, SessionStatus::Interrupted);
        assert_eq!((after.account_id, after.config_dir), (None, None));
        assert_eq!(
            state.store.get_session("bound").unwrap().unwrap().status,
            SessionStatus::Interrupted
        );
        assert!(source.join(record).exists(), "copied, not moved");
        assert_eq!(
            std::fs::read_to_string(dir.join("claude-a").join(record)).unwrap(),
            "conversation"
        );
    }

    /// The other half of the seam: ending a console session's process for a switch is not refused
    /// because sessions bound to it are running. That refusal belongs to archiving alone, so a
    /// switch of a console session with a working bound session goes on to the relaunch, which
    /// fails here only on the missing working directory, and leaves the bound session running.
    #[tokio::test]
    async fn a_switch_is_not_refused_for_a_running_bound_session() {
        let (state, dir) = switch_fixture("switch-running-bound");
        state
            .store
            .insert_session(&console_session("hub", SessionStatus::Working))
            .unwrap();
        state
            .store
            .insert_session(&bound_session("worker", SessionStatus::Working, "hub"))
            .unwrap();
        let hub = idle_stand_in("hub");
        let worker = idle_stand_in("worker");
        state.register_live(hub.clone());
        state.watch_exit(hub.clone());
        state.register_live(worker.clone());

        let err = switch_session_account(
            &state,
            "hub",
            Some(Some("claude-a".into())),
            shell_env(&dir),
        )
        .await
        .expect_err("the console's working directory does not exist");

        assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);
        assert!(hub.poll_exit(), "its process was ended");
        assert!(!worker.poll_exit(), "the bound session was left running");
        assert_eq!(status_of(&state, "worker"), Some(SessionStatus::Working));
    }

    /// A relaunched process that has ended by the end of the settling time did not come up; one
    /// still running did.
    #[tokio::test]
    async fn a_relaunched_process_that_ends_at_once_did_not_come_up() {
        let settle = Duration::from_millis(400);
        let gone = fake_live_session("gone", Agent::Claude, 80, 24, "exit 1");
        let up = idle_stand_in("up");

        assert!(!came_up(&gone, settle).await);
        assert!(came_up(&up, settle).await);
    }

    /// A switched session whose relaunched process ended at once is broadcast interrupted on its
    /// old account before the switch returns its error, and nothing the exit watcher publishes
    /// afterwards puts the new account back in a client's copy.
    #[tokio::test]
    async fn a_switch_that_did_not_come_up_is_restored_before_it_returns() {
        let (state, _dir) = switch_fixture("switch-restored-first");
        let before = project_session("s", Agent::Claude, SessionStatus::Interrupted);
        state.store.insert_session(&before).unwrap();
        state
            .set_session_account("s", Some("claude-a"), Some("/elsewhere"))
            .unwrap();
        state
            .store
            .update_session(&Session {
                status: SessionStatus::Idle,
                ..before.clone()
            })
            .unwrap();
        let mut events = state.subscribe();
        let live = fake_live_session("s", Agent::Claude, 80, 24, "exit 1");
        state.register_live(live.clone());
        state.watch_exit(live.clone());

        put_back_once_down(&state, &before).await;

        let mut last = None;
        while let Ok(event) = events.try_recv() {
            if let Event::SessionUpserted { session } = event {
                last = Some(session);
            }
        }
        let last = last.expect("the session was published");
        assert_eq!(last.status, SessionStatus::Interrupted);
        assert_eq!((last.account_id, last.config_dir), (None, None));
    }

    /// The claim a switch holds outlives the relaunched process being registered as live, and it
    /// outlives that process's exit: while the come-up verdict is pending and the account is put
    /// back, a resume (what the terminal's Resume button asks for) and a second switch are both
    /// refused.
    #[tokio::test]
    async fn a_switch_claim_is_held_after_the_relaunched_process_is_live_and_after_it_exits() {
        let (state, _dir) = crate::test_support::app_state("coordinator-switch-claim");
        let claim = state.begin_switch("s").unwrap();
        let live = fake_live_session("s", Agent::Claude, 80, 24, "exit 1");
        state.register_live(live.clone());
        state.watch_exit(live.clone());
        let deadline = Instant::now() + Duration::from_secs(5);
        while state.live_session("s").is_some() {
            assert!(Instant::now() < deadline, "the exit was never seen");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        for refused in [state.begin_launch("s"), state.begin_switch("s")] {
            let err = refused.err().expect("still claimed");
            assert_eq!(code_of(&err), error_code::SESSION_ALREADY_STARTING);
        }
        drop(claim);
        assert!(state.begin_launch("s").is_ok());
    }

    /// An archive that lands before the relaunch writes anything is not undone by the switch: the
    /// relaunch refuses an archived session unless it is a resume reopening one, and leaves it
    /// archived.
    #[tokio::test]
    async fn a_relaunch_for_a_switch_never_reopens_an_archived_session() {
        let (state, _dir) = switch_fixture("switch-archived-late");
        state
            .store
            .insert_session(&project_session(
                "s",
                Agent::Claude,
                SessionStatus::Archived,
            ))
            .unwrap();
        let claim = state.begin_switch("s").unwrap();

        let err = relaunch_session(&state, &claim, "s", None, false, None)
            .await
            .expect_err("archived");
        assert_eq!(code_of(&err), error_code::SESSION_ARCHIVED);
        assert_eq!(
            state.store.get_session("s").unwrap().unwrap().status,
            SessionStatus::Archived
        );
    }

    /// A project session bound to the console session `owner`, titled by its id so a refusal that
    /// names it can be read back.
    fn bound_session(id: &str, status: SessionStatus, owner: &str) -> Session {
        Session {
            title: id.to_string(),
            bound_to: Some(owner.to_string()),
            ..project_session(id, Agent::Claude, status)
        }
    }

    fn console_session(id: &str, status: SessionStatus) -> Session {
        Session {
            title: id.to_string(),
            status,
            ..bare_session(id, Role::Console)
        }
    }

    fn status_of(state: &Arc<AppState>, id: &str) -> Option<SessionStatus> {
        state.store.get_session(id).unwrap().map(|s| s.status)
    }

    /// The sessions on record, sorted by id.
    fn session_ids(state: &Arc<AppState>) -> Vec<String> {
        let mut ids: Vec<String> = state
            .store
            .list_sessions()
            .unwrap()
            .into_iter()
            .map(|session| session.id)
            .collect();
        ids.sort();
        ids
    }

    /// The line is the process, not the status: any bound session with one refuses the archive,
    /// working, awaiting instructions or waiting for the user alike, and so does one being
    /// launched right now. The refusal counts them and names them, and nothing was archived.
    #[tokio::test]
    async fn archiving_a_console_session_is_refused_while_a_bound_process_runs() {
        use SessionStatus::{Idle, Interrupted, WaitingUser, Working};
        let (state, _dir) = switch_fixture("archive-refused");
        state
            .store
            .insert_session(&console_session("hub", Idle))
            .unwrap();
        state
            .store
            .insert_session(&bound_session("dormant", Interrupted, "hub"))
            .unwrap();

        // (what runs, the statuses of the bound sessions that have a process)
        let cases: [(&str, &[SessionStatus]); 4] = [
            ("working", &[Working]),
            ("awaiting instructions", &[Idle]),
            ("waiting for the user", &[WaitingUser]),
            ("several", &[Working, Idle]),
        ];
        for (name, statuses) in cases {
            let mut live = Vec::new();
            let ids: Vec<String> = (0..statuses.len()).map(|n| format!("run-{n}")).collect();
            for (id, status) in ids.iter().zip(statuses) {
                state
                    .store
                    .insert_session(&bound_session(id, *status, "hub"))
                    .unwrap();
                let fake = idle_stand_in(id);
                state.register_live(fake.clone());
                live.push(fake);
            }

            let err = archive_session(&state, "hub").expect_err(name);
            assert_eq!(
                code_of(&err),
                error_code::CONSOLE_SESSION_HAS_RUNNING_SESSIONS
            );
            let coded = err.downcast_ref::<CodedError>().unwrap();
            assert_eq!(coded.params["count"], statuses.len().to_string(), "{name}");
            for id in &ids {
                assert!(coded.params["sessions"].contains(id.as_str()), "{name}");
                assert!(coded.message.contains(id.as_str()), "{name}");
            }
            assert!(!coded.params["sessions"].contains("dormant"), "{name}");
            assert_eq!(status_of(&state, "hub"), Some(Idle), "{name}");
            assert_eq!(status_of(&state, "dormant"), Some(Interrupted), "{name}");

            drop(live); // ends the stand-in processes
            for id in &ids {
                state.store.delete_session(id).unwrap();
            }
        }

        // A bound session being launched has no registered process yet, and still counts.
        let claim = state.begin_launch("dormant").unwrap();
        let err = archive_session(&state, "hub").expect_err("launching");
        assert_eq!(
            code_of(&err),
            error_code::CONSOLE_SESSION_HAS_RUNNING_SESSIONS
        );
        assert_eq!(status_of(&state, "hub"), Some(Idle));
        drop(claim);
    }

    /// With no bound process running, archiving a console session archives every bound session
    /// that is not archived yet, and only those: not an unbound session, not one bound to another
    /// console session, and an archived one keeps its own ended time.
    #[tokio::test]
    async fn archiving_a_console_session_archives_its_interrupted_bound_sessions() {
        use SessionStatus::{Archived, Idle, Interrupted};
        let (state, _dir) = switch_fixture("archive-cascade");
        for session in [
            console_session("hub", Idle),
            console_session("other-hub", Idle),
            bound_session("a", Interrupted, "hub"),
            bound_session("b", Interrupted, "hub"),
            Session {
                ended_at: Some(5),
                ..bound_session("old", Archived, "hub")
            },
            bound_session("elsewhere", Interrupted, "other-hub"),
            project_session("unbound", Agent::Claude, Interrupted),
        ] {
            state.store.insert_session(&session).unwrap();
        }

        archive_session(&state, "hub").unwrap();

        let statuses: Vec<(String, SessionStatus)> = session_ids(&state)
            .into_iter()
            .map(|id| {
                let status = status_of(&state, &id).unwrap();
                (id, status)
            })
            .collect();
        assert_eq!(
            statuses,
            [
                ("a".to_string(), Archived),
                ("b".to_string(), Archived),
                ("elsewhere".to_string(), Interrupted),
                ("hub".to_string(), Archived),
                ("old".to_string(), Archived),
                ("other-hub".to_string(), Idle),
                ("unbound".to_string(), Interrupted),
            ]
        );
        assert_eq!(
            state.store.get_session("old").unwrap().unwrap().ended_at,
            Some(5)
        );
    }

    /// Archiving a project session reaches no other session: the automatic archive of a `done`
    /// report (see `reporting.rs`) and a `delete_project` that stops sessions both end here, and
    /// neither takes the console session the session is bound to.
    #[tokio::test]
    async fn archiving_a_bound_project_session_leaves_its_console_session_alone() {
        let (state, _dir) = switch_fixture("archive-bound-alone");
        state
            .store
            .insert_session(&console_session("hub", SessionStatus::Idle))
            .unwrap();
        state
            .store
            .insert_session(&bound_session("worker", SessionStatus::Idle, "hub"))
            .unwrap();

        archive_session(&state, "worker").unwrap();

        assert_eq!(status_of(&state, "worker"), Some(SessionStatus::Archived));
        assert_eq!(status_of(&state, "hub"), Some(SessionStatus::Idle));
    }

    /// Which console session a relaunch brings back first: the archived one a dormant session is
    /// bound to, and nothing else — not an owner that is only interrupted or already running, not
    /// for an unbound session, and never for a console session, so reopening one reopens nothing
    /// bound to it.
    #[test]
    fn a_reopen_brings_back_only_an_archived_owner() {
        use SessionStatus::{Archived, Idle, Interrupted};
        let (state, _dir) = switch_fixture("archived-owner");
        for session in [
            console_session("archived-hub", Archived),
            console_session("interrupted-hub", Interrupted),
            console_session("running-hub", Idle),
        ] {
            state.store.insert_session(&session).unwrap();
        }
        // (the session's status, what it is bound to, the owner it brings back)
        let cases = [
            (
                "archived",
                Archived,
                Some("archived-hub"),
                Some("archived-hub"),
            ),
            (
                "interrupted",
                Interrupted,
                Some("archived-hub"),
                Some("archived-hub"),
            ),
            ("owner interrupted", Archived, Some("interrupted-hub"), None),
            ("owner running", Archived, Some("running-hub"), None),
            ("owner gone", Archived, Some("no-such-hub"), None),
            ("unbound", Archived, None, None),
            ("not dormant", Idle, Some("archived-hub"), None),
        ];
        for (name, status, owner, expected) in cases {
            let session = Session {
                bound_to: owner.map(str::to_string),
                ..project_session("s", Agent::Claude, status)
            };
            state.store.insert_session(&session).unwrap();
            assert_eq!(
                archived_owner(&state, "s").unwrap().as_deref(),
                expected,
                "{name}"
            );
            state.store.delete_session("s").unwrap();
        }
        assert_eq!(archived_owner(&state, "archived-hub").unwrap(), None);
    }

    /// The console session is relaunched before the session asked for, and the reopen fails as a
    /// whole when it cannot be: the session is never attempted (its ended time is untouched) and
    /// both stay archived. The console's working directory is missing here and the project's is
    /// not, so the failure can only be the console session's.
    #[tokio::test]
    async fn a_bound_session_is_not_reopened_when_its_console_session_cannot_be() {
        let (state, dir) = switch_fixture("reopen-owner-fails");
        std::fs::create_dir_all(dir.join("no-such-project")).unwrap();
        state
            .store
            .insert_session(&console_session("hub", SessionStatus::Archived))
            .unwrap();
        state
            .store
            .insert_session(&Session {
                ended_at: Some(5),
                ..bound_session("worker", SessionStatus::Archived, "hub")
            })
            .unwrap();

        let err = resume_session(&state, "worker", None)
            .await
            .expect_err("the console session cannot launch");

        assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);
        assert_eq!(status_of(&state, "hub"), Some(SessionStatus::Archived));
        let worker = state.store.get_session("worker").unwrap().unwrap();
        assert_eq!(
            (worker.status, worker.ended_at),
            (SessionStatus::Archived, Some(5))
        );
        assert!(!state.has_process("hub") && !state.has_process("worker"));
    }

    /// Deleting an archived console session takes the archived sessions bound to it and no others;
    /// one bound session on its own leaves the console session; a console session that is not
    /// archived is refused and takes nothing with it.
    #[tokio::test]
    async fn deleting_an_archived_console_session_deletes_its_archived_bound_sessions() {
        use SessionStatus::{Archived, Idle, Interrupted};
        let (state, _dir) = switch_fixture("delete-cascade");
        for session in [
            console_session("hub", Archived),
            bound_session("a", Archived, "hub"),
            bound_session("b", Archived, "hub"),
            bound_session("still-here", Interrupted, "hub"),
            console_session("other-hub", Archived),
            bound_session("c", Archived, "other-hub"),
            project_session("unbound", Agent::Claude, Archived),
            console_session("live-hub", Idle),
            bound_session("d", Archived, "live-hub"),
        ] {
            state.store.insert_session(&session).unwrap();
        }

        delete_session(&state, "hub").unwrap();
        assert_eq!(
            session_ids(&state),
            ["c", "d", "live-hub", "other-hub", "still-here", "unbound"]
        );

        delete_session(&state, "c").unwrap();
        assert_eq!(
            session_ids(&state),
            ["d", "live-hub", "other-hub", "still-here", "unbound"],
            "the bound session alone leaves its console session"
        );

        let err = delete_session(&state, "live-hub").expect_err("not archived");
        assert_eq!(code_of(&err), error_code::SESSION_NOT_ARCHIVED);
        assert_eq!(
            session_ids(&state),
            ["d", "live-hub", "other-hub", "still-here", "unbound"]
        );
    }

    /// Deleting every archived console session of a console takes their archived bound sessions
    /// too; deleting a project's archived sessions takes the bound ones among them and leaves the
    /// console sessions; deleting a console session's archive takes its archived bound sessions
    /// and nothing else, and a project together with a console session is refused.
    #[tokio::test]
    async fn deleting_archived_sessions_in_bulk_follows_the_binding_for_console_sessions_only() {
        use SessionStatus::Archived;
        let seed = |name: &str| {
            let (state, dir) = switch_fixture(name);
            for session in [
                console_session("hub", Archived),
                bound_session("a", Archived, "hub"),
                project_session("unbound", Agent::Claude, Archived),
            ] {
                state.store.insert_session(&session).unwrap();
            }
            (state, dir)
        };

        let (state, _dir) = seed("bulk-console-scope");
        delete_archived_sessions(&state, "console-1", None, None).unwrap();
        assert_eq!(session_ids(&state), ["unbound"]);

        let (state, _dir) = seed("bulk-project-scope");
        delete_archived_sessions(&state, "console-1", Some("project-1"), None).unwrap();
        assert_eq!(session_ids(&state), ["hub"]);

        let (state, _dir) = seed("bulk-bound-scope");
        delete_archived_sessions(&state, "console-1", None, Some("hub")).unwrap();
        assert_eq!(session_ids(&state), ["hub", "unbound"]);
        for (project, owner, code) in [
            (
                Some("project-1"),
                Some("hub"),
                error_code::CONFLICTING_FIELDS,
            ),
            (None, Some("unbound"), error_code::UNKNOWN_SESSION),
            (None, Some("nobody"), error_code::UNKNOWN_SESSION),
        ] {
            let refused = delete_archived_sessions(&state, "console-1", project, owner)
                .expect_err("not a scope");
            assert_eq!(code_of(&refused), code);
        }
        assert_eq!(session_ids(&state), ["hub", "unbound"]);
    }

    fn page_of(id: &str, console_session_id: &str, created_at: i64) -> crate::protocol::Page {
        crate::protocol::Page {
            id: id.to_string(),
            console_session_id: console_session_id.to_string(),
            html: "<p>page</p>".to_string(),
            anchor_message_id: None,
            created_at,
        }
    }

    /// Two console sessions in one console each keep their own page history: a submission is
    /// written into the console session that pushed the page and no other, and "the newest page"
    /// is the newest of that console session, so a later page from the other one does not make an
    /// earlier page read-only.
    #[tokio::test]
    async fn a_page_submission_reaches_the_console_session_that_pushed_it() {
        let (state, dir) =
            crate::test_support::app_state("coordinator-submit-page-routes-by-owner");
        let workdir = console_workdir(&dir);
        state
            .store
            .insert_console(&console(Agent::Claude, &workdir))
            .unwrap();
        let mut live = HashMap::new();
        for id in ["owner-a", "owner-b"] {
            state
                .store
                .insert_session(&bare_session(id, Role::Console))
                .unwrap();
            let session = idle_stand_in(id);
            state.register_live(session.clone());
            crate::session::spawn_reader_thread(session.clone(), 8 * 1024);
            live.insert(id, session);
        }
        for page in [
            page_of("a-old", "owner-a", 1),
            page_of("a-new", "owner-a", 3),
            page_of("b-new", "owner-b", 4),
        ] {
            state.store.insert_page(&page).unwrap();
        }

        // Pages are listed by console session only: a project session is not one.
        state
            .store
            .insert_session(&bare_session("worker", Role::Project))
            .unwrap();
        let listing = |session: &str| {
            handle(
                &state,
                None,
                RequestBody::ListPages {
                    console_session: session.to_string(),
                },
            )
        };
        assert!(matches!(
            listing("owner-a").await.unwrap(),
            Some(Event::PageList { ref pages, .. }) if pages.len() == 2
        ));
        let refused = listing("worker").await.expect_err("not a console session");
        assert_eq!(code_of(&refused), error_code::UNKNOWN_SESSION);

        let refused = submit_page(&state, "a-old", serde_json::json!({}))
            .await
            .expect_err("a history page of its console session");
        assert_eq!(code_of(&refused), error_code::PAGE_NOT_CURRENT);

        submit_page(&state, "a-new", serde_json::json!({}))
            .await
            .expect("owner-a's newest page, though owner-b has a later one");

        let output_of = |id: &str| {
            let deadline = Instant::now() + Duration::from_millis(500);
            loop {
                let output = live[id].recent_output(8 * 1024);
                if !output.is_empty() || Instant::now() >= deadline {
                    return String::from_utf8_lossy(&output).into_owned();
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        assert!(output_of("owner-a").contains("a-new"));
        // Read once, after owner-a's delivery has landed. A mistaken write to owner-b would most
        // likely have arrived by now; this cannot rule out one that is still on its way.
        assert!(!live["owner-b"]
            .recent_output(8 * 1024)
            .windows(5)
            .any(|window| window == b"a-new"));
    }
}
