//! The coordinator role: what each control-socket request does to the stored consoles, projects
//! and sessions, and which host-role work it triggers.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use uuid::Uuid;

use crate::hostfs;
use crate::paths;
use crate::protocol::{
    error_code, now_millis, Agent, CodedError, Console, Event, Origin, Project, ProjectSource,
    RequestBody, Role, Session, SessionStatus,
};
use crate::state::AppState;
use crate::store::LOCAL_HOST_ID;
use crate::term;

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
        } => {
            let id = Uuid::new_v4().to_string();
            let workdir = paths::console_workdir(&id);
            std::fs::create_dir_all(&workdir)?;
            // TODO(milestone 02): generate the hub's instruction file in this directory, named for
            // the console's hub agent — `CLAUDE.md` for Claude Code, `AGENTS.md` for Codex, either
            // for Grok, all matched by exact spelling. A Grok hub cannot be given one at all,
            // because Grok locates a project by walking up for a `.git` directory and reads no
            // instructions without one, so its role has to come from `--rules`.
            let console = Console {
                id,
                name,
                workdir: workdir.to_string_lossy().into_owned(),
                hub_agent,
                default_agent,
                created_at: now_millis(),
            };
            state.store.insert_console(&console)?;
            state.broadcast(Event::ConsoleUpserted { console });
            Ok(None)
        }

        RequestBody::UpdateConsole {
            console: id,
            name,
            hub_agent,
            default_agent,
        } => {
            let mut console = state
                .store
                .get_console(&id)?
                .ok_or_else(|| anyhow!("unknown console {id}"))?;
            if let Some(name) = name {
                console.name = name;
            }
            if let Some(hub_agent) = hub_agent {
                console.hub_agent = hub_agent;
            }
            if let Some(default_agent) = default_agent {
                console.default_agent = default_agent;
            }
            state.store.update_console(&console)?;
            state.broadcast(Event::ConsoleUpserted { console });
            Ok(None)
        }

        RequestBody::DeleteConsole { console: id } => {
            let console = state
                .store
                .get_console(&id)?
                .ok_or_else(|| anyhow!("unknown console {id}"))?;
            if state.has_live_sessions_where(|session| session.console_id == id)? {
                bail!("this console still has running sessions; archive them first");
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
        } => {
            let mut project = state
                .store
                .get_project(&id)?
                .ok_or_else(|| anyhow!("unknown project {id}"))?;
            if let Some(name) = name {
                project.name = name;
            }
            // Absent means "leave it alone", an explicit null means "go back to inheriting the
            // console's default" — the two have to stay distinguishable, or a project's own
            // default could be set but never cleared.
            if let Some(default_agent) = default_agent {
                project.default_agent = default_agent;
            }
            state.store.update_project(&project)?;
            state.broadcast(Event::ProjectUpserted { project });
            Ok(None)
        }

        RequestBody::DeleteProject { project: id } => {
            state
                .store
                .get_project(&id)?
                .ok_or_else(|| anyhow!("unknown project {id}"))?;
            if state
                .has_live_sessions_where(|session| session.project_id.as_deref() == Some(&id))?
            {
                bail!("this project still has running sessions; archive them first");
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
        } => {
            let session = open_session(state, console_id, project_id, agent, task, title).await?;
            Ok(Some(Event::SessionOpened {
                id: request_id,
                session,
            }))
        }

        RequestBody::ResumeSession { session } => {
            resume_session(state, &session).await?;
            Ok(None)
        }

        RequestBody::ArchiveSession { session } => {
            archive_session(state, &session)?;
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

        RequestBody::Shutdown => {
            state.request_shutdown();
            Ok(None)
        }
    }
}

async fn add_project(
    state: &Arc<AppState>,
    console_id: String,
    source: ProjectSource,
    path: Option<String>,
    remote_url: Option<String>,
    name: Option<String>,
    default_agent: Option<Agent>,
) -> Result<()> {
    state
        .store
        .get_console(&console_id)?
        .ok_or_else(|| anyhow!("unknown console {console_id}"))?;

    let directories: Vec<PathBuf> = match source {
        ProjectSource::Local => {
            let path = hostfs::expand(&path.ok_or_else(|| anyhow!("`path` is required"))?);
            if !path.is_dir() {
                bail!("`{}` is not a directory", path.display());
            }
            vec![path]
        }
        ProjectSource::Parent => {
            let parent = hostfs::expand(&path.ok_or_else(|| anyhow!("`path` is required"))?);
            let repos = hostfs::discover_repos(&parent)?;
            if repos.is_empty() {
                bail!(
                    "no git repositories were found directly under `{}`",
                    parent.display()
                );
            }
            repos
        }
        ProjectSource::Github => {
            let url = remote_url
                .clone()
                .ok_or_else(|| anyhow!("`remote_url` is required"))?;
            let parent = hostfs::expand(&path.ok_or_else(|| anyhow!("`path` is required"))?);
            let cloned =
                tokio::task::spawn_blocking(move || hostfs::clone_repo(&url, &parent)).await??;
            vec![cloned]
        }
    };

    // A caller-supplied name only applies when a single project is being associated; a parent
    // directory yields many, and each takes its own directory's name.
    let explicit_name = if directories.len() == 1 { name } else { None };

    let mut added = 0;
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
        };
        state.store.insert_project(&project)?;
        state.broadcast(Event::ProjectUpserted { project });
        added += 1;
    }
    if added == 0 {
        bail!("every directory found is already associated with this console");
    }
    Ok(())
}

async fn open_session(
    state: &Arc<AppState>,
    console_id: String,
    project_id: Option<String>,
    agent: Option<Agent>,
    task: Option<String>,
    title: Option<String>,
) -> Result<Session> {
    let console = state
        .store
        .get_console(&console_id)?
        .ok_or_else(|| anyhow!("unknown console {console_id}"))?;

    let (role, cwd, project, default_title) = match &project_id {
        Some(project_id) => {
            let project = state
                .store
                .get_project(project_id)?
                .ok_or_else(|| anyhow!("unknown project {project_id}"))?;
            let cwd = PathBuf::from(&project.path);
            let title = project.name.clone();
            (Role::Worker, cwd, Some(project), title)
        }
        None => {
            let workdir = PathBuf::from(&console.workdir);
            std::fs::create_dir_all(&workdir)?;
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
        // Everything this milestone opens is the user's own doing; the hub opening sessions is
        // milestone 02's.
        origin: Origin::User,
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
            Err(err)
        }
    }
}

async fn resume_session(state: &Arc<AppState>, id: &str) -> Result<()> {
    let mut session = state.session_record(id)?;
    // Claimed before anything else, so two overlapping relaunches cannot both get through.
    let claim = state.begin_launch(id)?;
    if !session.status.is_dormant() {
        // The ordinary cause is a second click landing after the first relaunch already moved the
        // session, so it carries the same code as a launch that is already under way.
        return Err(CodedError::raised(
            error_code::SESSION_ALREADY_RUNNING,
            "this session is not interrupted or archived",
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

    match start_process(state, claim, &session, &cwd, None, resume_id.as_deref()).await {
        Ok(()) => {
            state.publish_session(&state.session_record(&session.id)?);
            Ok(())
        }
        Err(err) => {
            // Back to where it was, archived included: a reopen that failed to launch must not
            // quietly move a session out of its project's Archive group.
            session.status = previous_status;
            session.ended_at = Some(now_millis());
            state.save_session(&session)?;
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
    let session_id = session.id.clone();
    let agent = session.agent;
    let cwd = cwd.to_path_buf();
    let task = task.map(str::to_string);
    let resume = resume_agent_session_id.map(str::to_string);
    let port = state.port;
    let self_exe = state.self_exe.clone();

    // Launching snapshots the user's shell environment and forks a process, so it goes off the
    // runtime rather than holding up the socket it was asked on.
    let launch = tokio::task::spawn_blocking(move || {
        term::launch(
            &session_id,
            agent,
            &cwd,
            task.as_deref(),
            resume.as_deref(),
            port,
            &self_exe,
        )
    })
    .await??;

    // Registering the session consumes the claim: from here on the live map is what says it is
    // running.
    state.register_live(launch.session.clone());
    drop(claim);
    state.watch_exit(launch.session.clone());
    if let Some(agent_session_id) = launch.agent_session_id {
        state.set_agent_session_id(&session.id, &agent_session_id)?;
    }
    Ok(())
}

fn archive_session(state: &Arc<AppState>, id: &str) -> Result<()> {
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

fn send_message(state: &Arc<AppState>, id: &str, text: &str) -> Result<()> {
    let session = state.session_record(id)?;
    let live = state
        .live_session(id)
        .ok_or_else(|| anyhow!("this session is not running"))?;

    // The gate is the session's hook-reported state, never the terminal: no agent signals its
    // modal state through terminal modes, and a write while a modal dialog is up has the trailing
    // Enter confirm whatever option is highlighted. Working and idle are both safe — every agent
    // queues a message written mid-turn and consumes it when the turn ends.
    match session.status {
        SessionStatus::Working | SessionStatus::Idle => term::send_message(&live, text),
        SessionStatus::WaitingUser => {
            bail!("this session is waiting for you — answer it in the terminal first")
        }
        SessionStatus::Interrupted | SessionStatus::Archived => {
            bail!("this session is not running")
        }
    }
}

fn session_cwd(state: &Arc<AppState>, session: &Session) -> Result<PathBuf> {
    match &session.project_id {
        Some(project_id) => {
            let project = state
                .store
                .get_project(project_id)?
                .ok_or_else(|| anyhow!("unknown project {project_id}"))?;
            Ok(PathBuf::from(project.path))
        }
        None => {
            let console = state
                .store
                .get_console(&session.console_id)?
                .ok_or_else(|| anyhow!("unknown console {}", session.console_id))?;
            Ok(PathBuf::from(console.workdir))
        }
    }
}
