//! Associating, changing and removing projects, what a removal takes along, and the refresh of
//! their git statuses.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use uuid::Uuid;

use crate::agents::launch_settings::{self, normalized_launches, trimmed_or_none, LaunchCheck};
use crate::blocking_pass::each_blocking;
use crate::branch_cleanup;
use crate::git_workflow::{GitWorkflowOverride, GitWorkflowOverrideChanges};
use crate::hostfs;
use crate::project_git;
use crate::project_trash;
use crate::protocol::{
    error_code, Agent, CodedError, Console, Event, Launch, LaunchChange, PerAgent, Present,
    Project, ProjectAccount, ProjectSource, Session, SessionBranchTarget,
};
use crate::state::AppState;
use crate::store::LOCAL_HOST_ID;

use super::account_choice::{agent_is_available, checked_project_account};
use super::bindings::binding_depth;
use super::lifecycle::{archive_session, ensure_unlocked, report_not_removed, session_ids_where};
use super::validate::{
    absolute_path, check_text_length, field_required, normalise_tags, required_text,
    PROJECT_DESCRIPTION_LIMIT, PROJECT_NAME_LIMIT,
};

/// One association request. A struct rather than a parameter list because the callers differ in
/// more than one field — the user's control-socket request, the console session's tool call, and a
/// configuration import — and most of the fields are optional strings that would otherwise be
/// positional.
pub struct AddProjectRequest {
    pub console_id: String,
    pub source: ProjectSource,
    pub path: Option<String>,
    pub remote_url: Option<String>,
    pub name: Option<String>,
    pub default_agent: Option<Agent>,
    pub settings: NewProjectSettings,
    /// Whether an absent `default_agent` is detected from each directory; false stores none.
    pub detect_default_agent: bool,
    pub tags: Option<Vec<String>>,
    /// Stored trimmed; blank is none. Like `name`, it applies only when a single project is added.
    pub description: Option<String>,
    /// Whether the settings' models and efforts are checked against what the agent offers. Not for
    /// an imported document, whose values were chosen on the machine it came from, and which must
    /// not lose a project because this one's agent no longer lists a model.
    pub check_launch: bool,
    /// Imported legacy text must survive configuration roundtrips, including overlong values.
    pub check_text_limits: bool,
}

/// What a project is associated with beyond where it is, as the request carries it: an account
/// reference is checked, and a model or effort trimmed with blank read as unset, before anything
/// is stored. Every field absent is the default, and what the console session's tool call sends.
#[derive(Default)]
pub struct NewProjectSettings {
    pub pinned: bool,
    pub accounts: PerAgent<ProjectAccount>,
    pub launch: PerAgent<Launch>,
    pub git_workflow: GitWorkflowOverride,
}

/// Checks the models and efforts a new project is given against what each agent offers, unless
/// `check` is false. Runs once the directory or repository is known to be usable and before
/// anything is cloned or stored, since asking an agent can take a while and a refusal must leave
/// nothing behind.
async fn check_new_project_launch(
    state: &Arc<AppState>,
    console: &Console,
    settings: &NewProjectSettings,
    accounts: &PerAgent<ProjectAccount>,
    check: bool,
) -> Result<()> {
    if !check {
        return Ok(());
    }
    for (agent, account) in accounts.iter() {
        let launch = settings.launch.get(agent);
        LaunchCheck::given(
            agent,
            console.resolve_project_account(agent, account),
            launch.model.as_deref(),
            launch.effort.as_deref(),
        )
        .behind(&console.project_session_launch.get(agent).model)
        .run(state)
        .await?;
    }
    Ok(())
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
        settings,
        detect_default_agent,
        tags,
        description,
        check_launch,
        check_text_limits,
    } = request;
    let console = state
        .store
        .get_console(&console_id)?
        .ok_or_else(|| CodedError::unknown_console(&console_id))?;
    // Blank is no name: it is ignored for a parent directory, and a single project takes its
    // directory's own.
    let name = name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty());
    let description = trimmed_or_none(description.as_deref());
    let check_text = || -> Result<()> {
        if check_text_limits {
            if let Some(name) = &name {
                check_text_length("name", name, PROJECT_NAME_LIMIT)?;
            }
            if let Some(description) = &description {
                check_text_length("description", description, PROJECT_DESCRIPTION_LIMIT)?;
            }
        }
        Ok(())
    };
    // A clone must not leave a directory behind after a text validation failure. Parent requests
    // first discover how many projects there are, because several ignore the supplied text.
    if source != ProjectSource::Parent {
        check_text()?;
    }
    // Checked before anything is cloned or stored, so a refusal leaves nothing behind.
    let accounts = PerAgent::try_from_fn(|agent| {
        checked_project_account(state, agent, settings.accounts.get(agent).clone())
    })?;

    let directories: Vec<PathBuf> = match source {
        ProjectSource::Local => {
            let path = absolute_path(&path.ok_or_else(|| field_required("path"))?)?;
            hostfs::require_directory(&path)?;
            check_new_project_launch(state, &console, &settings, &accounts, check_launch).await?;
            vec![path]
        }
        ProjectSource::Parent => {
            let parent = absolute_path(&path.ok_or_else(|| field_required("path"))?)?;
            let repos = hostfs::listing::discover_repos(&parent)?;
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
            if repos.len() == 1 {
                check_text()?;
            }
            check_new_project_launch(state, &console, &settings, &accounts, check_launch).await?;
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
            check_new_project_launch(state, &console, &settings, &accounts, check_launch).await?;
            let cloned =
                tokio::task::spawn_blocking(move || project_git::clone::clone_repo(&url, &parent))
                    .await??;
            vec![cloned]
        }
    };

    // A caller-supplied name only applies when a single project is being associated; a parent
    // directory yields many, and each takes its own directory's name.
    let explicit_name = if directories.len() == 1 { name } else { None };

    let tags = normalise_tags(tags.unwrap_or_default());
    let description = if directories.len() == 1 {
        description
    } else {
        None
    };

    let mut associated = associated_directories(state, &console_id).await?;
    let remote_urls = directory_remote_urls(
        &associated,
        source,
        &directories,
        project_git::remote::read_remote_url,
    )
    .await?;

    let mut added = Vec::new();
    for directory in directories {
        let path_text = directory.to_string_lossy().into_owned();
        if associated.contains(&directory) {
            continue;
        }
        // A clone keeps the URL it was given; the other sources take the one read above.
        let project_remote_url = match source {
            ProjectSource::Git => remote_url.clone(),
            ProjectSource::Local | ProjectSource::Parent => remote_urls.get(&directory).cloned(),
        };
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
            default_agent: default_agent.or_else(|| {
                detect_default_agent
                    .then(|| detect_project_agent(state, &directory))
                    .flatten()
            }),
            source,
            remote_url: project_remote_url,
            accounts: accounts.clone(),
            launch: normalized_launches(settings.launch.clone()),
            trust_consent: false,
            pinned: settings.pinned,
            tags: tags.clone(),
            description: description.clone(),
            git_workflow: settings.git_workflow,
        };
        state.store.insert_project(&project)?;
        associated.insert(&directory);
        project_git::watch::add(state, &project);
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

/// What `update_project` changes; a field left at its default is left alone.
#[derive(Default)]
pub struct ProjectChanges {
    pub name: Option<String>,
    /// `Some(None)` is back to following the console's project session agent (else its
    /// `default_agent`).
    pub default_agent: Present<Agent>,
    pub accounts: HashMap<Agent, ProjectAccount>,
    pub launch: HashMap<Agent, LaunchChange>,
    pub pinned: Option<bool>,
    /// Replaces the tags wholesale; refused beside `add_tags` or `remove_tags`.
    pub tags: Option<Vec<String>>,
    pub add_tags: Vec<String>,
    /// Matched ignoring case.
    pub remove_tags: Vec<String>,
    /// `Some(None)` clears it; blank text clears it too.
    pub description: Present<String>,
    pub git_workflow: GitWorkflowOverrideChanges,
}

/// Applies `changes` to the project `id`, broadcasts it and returns it as stored. Every reference
/// and setting is checked first, with nothing written yet; an absent one is not checked at all,
/// as for a console. The agent's models are asked for before the record is read for the write, so an
/// edit made in the meantime is not lost.
pub async fn update_project(
    state: &Arc<AppState>,
    id: &str,
    changes: ProjectChanges,
) -> Result<Project> {
    if changes.tags.is_some() && !(changes.add_tags.is_empty() && changes.remove_tags.is_empty()) {
        let other = if changes.add_tags.is_empty() {
            "remove_tags"
        } else {
            "add_tags"
        };
        return Err(CodedError::raised(
            error_code::CONFLICTING_FIELDS,
            format!("`tags` and `{other}` cannot both be given"),
            &[("first", "tags"), ("second", other)],
        ));
    }
    let name = changes
        .name
        .as_deref()
        .map(|name| required_text("name", name))
        .transpose()?;
    let description = changes
        .description
        .as_ref()
        .map(|description| trimmed_or_none(description.as_deref()));
    let mut accounts = HashMap::new();
    for (agent, account) in &changes.accounts {
        accounts.insert(
            *agent,
            checked_project_account(state, *agent, account.clone())?,
        );
    }
    let mut verified = Vec::new();
    loop {
        let mut project = state
            .store
            .get_project(id)?
            .ok_or_else(|| CodedError::unknown_project(id))?;
        let console = state
            .store
            .get_console(&project.console_id)?
            .ok_or_else(|| CodedError::unknown_console(&project.console_id))?;
        // Compare against the latest record on every pass: unchanged legacy text must not block
        // unrelated settings, including after a launch check has yielded.
        if let Some(name) = &name {
            if name != project.name.trim() {
                check_text_length("name", name, PROJECT_NAME_LIMIT)?;
                project.name = name.clone();
            }
        }
        if let Some(description) = &description {
            if description != &trimmed_or_none(project.description.as_deref()) {
                if let Some(description) = description {
                    check_text_length("description", description, PROJECT_DESCRIPTION_LIMIT)?;
                }
                project.description = description.clone();
            }
        }
        // Absent means "leave it alone", an explicit null means "go back to inheriting the
        // console's project session agent" — the two have to stay distinguishable, or a project's
        // own default could be set but never cleared.
        if let Some(default_agent) = changes.default_agent {
            project.default_agent = default_agent;
        }
        // Only sessions opened afterwards take a new reference, model or effort; each existing
        // one keeps the account and directory it was started with (see `Session::account_id`)
        // and the process it is running.
        for (agent, reference) in &accounts {
            *project.accounts.get_mut(*agent) = reference.clone();
        }
        let mut pending = Vec::new();
        for (agent, change) in &changes.launch {
            let account_id = console.resolve_project_account(*agent, project.accounts.get(*agent));
            let launch = project.launch.get_mut(*agent);
            let check = launch_settings::apply_change(*agent, account_id, launch, change)
                .behind(&console.project_session_launch.get(*agent).model);
            if !check.is_empty() && !verified.contains(&check) {
                pending.push(check);
            }
        }
        if !pending.is_empty() {
            for check in pending {
                check.run(state).await?;
                verified.push(check);
            }
            continue;
        }
        if let Some(pinned) = changes.pinned {
            project.pinned = pinned;
        }
        if let Some(tags) = &changes.tags {
            project.tags = normalise_tags(tags.clone());
        } else if !changes.add_tags.is_empty() || !changes.remove_tags.is_empty() {
            let removed: HashSet<String> = changes
                .remove_tags
                .iter()
                .map(|tag| tag.trim().to_lowercase())
                .collect();
            let mut tags = std::mem::take(&mut project.tags);
            tags.retain(|tag| !removed.contains(&tag.to_lowercase()));
            tags.extend(changes.add_tags.iter().cloned());
            project.tags = normalise_tags(tags);
        }
        project.git_workflow = changes.git_workflow.applied_to(project.git_workflow);
        state.store.update_project(&project)?;
        // Read back rather than sent as `project`: the stored record carries what this write
        // does not touch (the remote URL, the trust consent), which another path may have just
        // changed.
        let stored = state
            .store
            .get_project(id)?
            .ok_or_else(|| CodedError::unknown_project(id))?;
        state.broadcast(Event::ProjectUpserted {
            project: stored.clone(),
        });
        return Ok(stored);
    }
}

/// Removes the project `id` as a `delete_project` request asks: refused while one of its sessions
/// is locked, or is running or still starting (with `stop_sessions`, only one still starting).
/// With `trash_directory` its folder goes to the Trash first, and what goes along with the folder
/// is removed once the project's sessions are down, the locked worktrees in `force_remove`
/// included.
pub(super) async fn delete_project(
    state: &Arc<AppState>,
    id: String,
    stop_sessions: bool,
    trash_directory: bool,
    force_remove: Vec<SessionBranchTarget>,
    inspection: Option<String>,
) -> Result<()> {
    let _removal = state.claim_session_removal();
    let deleted = state
        .store
        .get_project(&id)?
        .ok_or_else(|| CodedError::unknown_project(&id))?;
    let in_project = |session: &Session| session.project_id.as_deref() == Some(&id);
    ensure_unlocked(state.store.list_sessions()?.into_iter().filter(in_project))?;
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
    // Checked before any session is stopped, so a refusal leaves everything as it was.
    let to_trash = if trash_directory {
        project_trash::directory_to_trash(
            state,
            &deleted,
            &state.store.list_projects()?,
            inspection.as_deref(),
        )
        .await?
    } else {
        None
    };
    // What goes along with the folder is found while the folder, and the records that say
    // what is bound to the project's sessions, are still there.
    // A failure to find it out leaves everything outside the folder, and is told once the
    // removal stands. The worktrees the inspection before held back stay held back.
    let (along, undetermined) = if to_trash.is_some() {
        let held_back = inspection.as_deref().map_or_else(Vec::new, |token| {
            state.unresolved_removals.held_back(token, &deleted.id)
        });
        let (state, project) = (Arc::clone(state), deleted.clone());
        match tokio::task::spawn_blocking(move || {
            branch_cleanup::project::plan(
                &state,
                &project,
                branch_cleanup::project::Reads::Removal,
                &held_back,
            )
        })
        .await?
        {
            Ok(along) => (along, None),
            Err(err) => (None, Some(format!("{err:#}"))),
        }
    } else {
        (None, None)
    };
    if stop_sessions {
        stop_project_sessions(state, in_project)?;
    }
    // Ended before the folder is moved, if it is, so no shell is left running in it (a
    // tool still writing there by absolute path could recreate the moved folder); one
    // started during the move is ended with the record below.
    crate::terminal::shell::close_project(state, &id).await;
    // The project's processes, which no session owns, go with it, as the confirmation
    // said (`inspect_project_removal`'s `processes`), the folder moved or not: stopped
    // here, before the move, and not after it, since removing the project stops them
    // whether or not its folder goes to the Trash.
    crate::processes::requests::stop_project_processes(state, std::slice::from_ref(&deleted)).await;
    // The directory goes first, once nothing runs in it, so a failure leaves the project
    // in place for a retry. Without `trash_directory` only the association goes away: the
    // directory is the user's. The repository is found first, as it cannot be once the
    // directory is gone.
    let repository = crate::git::repo_files::repository_of(Path::new(&deleted.path));
    if let Some(directory) = to_trash {
        // Off the runtime's worker: a move across volumes copies.
        let trash = state.host_tools.get().trash;
        tokio::task::spawn_blocking(move || trash(&directory))
            .await?
            .map_err(|failure| project_trash::trash_failed(&failure))?;
        // Requests run concurrently, so a session may have been started in the project
        // while the folder moved, and nothing held it back. The folder is gone by now, so
        // the project is removed regardless: such a session is ended here as well as it
        // can be, and one still being launched, which cannot be ended, is left to fail
        // once its record goes.
        if let Err(err) = stop_project_sessions(state, in_project) {
            tracing::warn!(project = %id, %err, "ending sessions started during the move to the Trash failed");
        }
    }
    // The project's sessions cascade with it, so their saved output is collected first.
    let session_ids = session_ids_where(state, in_project)?;
    state.store.delete_project(&id)?;
    state.forget_deleted_projects(&[id.as_str()]);
    state.forget_cascaded_sessions(&session_ids);
    crate::terminal::shell::close_project(state, &id).await;
    crate::branch_origin::project_deleted_in(state, repository);
    state.broadcast(Event::ProjectDeleted {
        project: id.clone(),
    });
    if let Some(detail) = undetermined {
        let failure = CodedError::raised(
            error_code::PROJECT_BRANCHES_NOT_DETERMINED,
            format!(
                "what goes with the project folder could not be determined, so the \
                 worktrees outside it, and its branches if the repository stays, were \
                 left: {detail}"
            ),
            &[("name", &deleted.name), ("detail", &detail)],
        );
        state.broadcast(Event::from_error(None, &failure));
    }
    if let Some(along) = along {
        let state = Arc::clone(state);
        tokio::spawn(async move {
            let missed = branch_cleanup::project::carry_out_once_stopped(
                state.clone(),
                session_ids,
                along,
                force_remove,
            )
            .await;
            report_not_removed(&state, &missed);
        });
    }
    Ok(())
}

/// Answers an `inspect_project_removal` request: what removing `project` with its folder would
/// take along, and the processes of the project the removal would stop.
pub(super) async fn inspect_project_removal(
    state: &Arc<AppState>,
    request_id: Option<String>,
    project: String,
) -> Result<Option<Event>> {
    let project = state
        .store
        .get_project(&project)?
        .ok_or_else(|| CodedError::unknown_project(&project))?;
    // A removal that would be refused takes nothing along: its refusal is the answer, and
    // a client lists nothing for it. One a path that could not be resolved decided is kept
    // for the removal to repeat, so it takes nothing along either.
    project_trash::inspected(state, &project, &state.store.list_projects()?).await?;
    let id = project.id.clone();
    let deleted_project = project.clone();
    let planning = Arc::clone(state);
    let (items, held_back) = tokio::task::spawn_blocking(move || {
        Ok::<_, anyhow::Error>(
            branch_cleanup::project::plan(
                &planning,
                &project,
                branch_cleanup::project::Reads::Inspection,
                &[],
            )?
            .map(|along| (along.items(), along.held_back()))
            .unwrap_or_default(),
        )
    })
    .await??;
    let processes = crate::processes::requests::project_processes(state, &deleted_project).await?;
    // The worktrees it held back, not listed as going, are kept for the removal confirmed
    // after it to hold back too.
    let inspection = if held_back.is_empty() {
        None
    } else {
        state
            .unresolved_removals
            .keep_held_back(state, &id, held_back)
    };
    Ok(Some(Event::ProjectRemovalInspected {
        id: request_id,
        items,
        processes,
        inspection,
    }))
}

/// The directories this console already has a project at, told apart by what they are on the file
/// system rather than by their paths' spelling: a project added through a link, or in another
/// case on a case-insensitive file system, is the same project as one added by the real path.
/// The stored paths are read under the store's lock and looked at on the file system after it is
/// released, on a blocking thread, so a directory on a stalled mount never holds the store.
async fn associated_directories(
    state: &AppState,
    console_id: &str,
) -> Result<hostfs::identity::DirectoryIdentities> {
    let paths = state.store.project_paths(console_id)?;
    Ok(identities_within(
        state,
        console_id,
        paths,
        ASSOCIATED_STAT_DEADLINE,
        |paths| hostfs::identity::DirectoryIdentities::from_paths(paths.iter().map(String::as_str)),
    )
    .await?)
}

/// How long looking at the stored project paths on the file system may take. Past it a project on
/// a mount that does not answer would hold every association to the console, so the set falls
/// back to the paths' text.
const ASSOCIATED_STAT_DEADLINE: Duration = Duration::from_secs(10);

/// The set `build` makes from `paths` on a blocking thread, or, once `deadline` passes, one made
/// from their text alone. The thread is left to finish on its own, and is recorded on the state
/// until it does: while it runs, later calls for the console take the text alone at once instead
/// of starting another thread that would be stuck the same way.
async fn identities_within(
    state: &AppState,
    console_id: &str,
    paths: Vec<String>,
    deadline: Duration,
    build: impl FnOnce(&[String]) -> hostfs::identity::DirectoryIdentities + Send + 'static,
) -> Result<hostfs::identity::DirectoryIdentities, tokio::task::JoinError> {
    let from_text = |paths: &[String]| {
        hostfs::identity::DirectoryIdentities::from_text(paths.iter().map(String::as_str))
    };
    if state.stalled_directory_passes.is_stuck(console_id) {
        return Ok(from_text(&paths));
    }
    let text_only = paths.clone();
    let alive = Arc::new(());
    let weak = Arc::downgrade(&alive);
    let built = tokio::task::spawn_blocking(move || {
        let identities = build(&paths);
        drop(alive);
        identities
    });
    match tokio::time::timeout(deadline, built).await {
        Ok(joined) => joined,
        Err(_) => {
            tracing::warn!(console = %console_id, "looking at the project directories timed out, comparing their paths as text");
            state
                .stalled_directory_passes
                .record(console_id.to_string(), weak);
            Ok(from_text(&text_only))
        }
    }
}

/// The remote URL `read` finds for each of `directories` that has one, for the sources whose
/// projects take it from their directory; a clone has the URL it was given and reads nothing.
///
/// Read only for the directories that are not already associated, so a parent directory whose
/// repositories mostly are does not pay a `git` read for each one it then skips; the check in
/// `add_project`'s loop stays the authority, this one only saves the reads. They run a few at a
/// time on blocking threads (see `blocking_pass::each_blocking`), so a directory on a stalled mount
/// holds up its own read and no other, and a parent directory holding hundreds of repositories
/// does not take over the blocking pool. A directory whose read did not come back, because its
/// thread panicked, is simply absent: a project stored without a URL, not a failed association.
async fn directory_remote_urls(
    associated: &hostfs::identity::DirectoryIdentities,
    source: ProjectSource,
    directories: &[PathBuf],
    read: impl Fn(&Path) -> Option<String> + Send + Sync + 'static,
) -> Result<HashMap<PathBuf, String>> {
    if source == ProjectSource::Git {
        return Ok(HashMap::new());
    }
    let unassociated: Vec<PathBuf> = directories
        .iter()
        .filter(|directory| !associated.contains(directory))
        .cloned()
        .collect();
    let urls = each_blocking(unassociated, move |directory| {
        let url = read(&directory);
        (directory, url)
    })
    .await;
    Ok(urls
        .into_iter()
        .filter_map(|(directory, url)| Some((directory, url?)))
        .collect())
}

/// The agent to store for a project associated with no default agent chosen: the one its
/// directory is set up for, unless that agent has been determined unavailable. Decided once, at
/// association, and stored like any chosen default; while availability is still undetermined the
/// detected agent is kept.
pub(super) fn detect_project_agent(state: &Arc<AppState>, directory: &Path) -> Option<Agent> {
    hostfs::listing::detect_agent(directory).filter(|agent| agent_is_available(state, *agent))
}

/// Archives the running sessions of the project `in_project` matches, the way the user's own
/// archive request does.
fn stop_project_sessions(
    state: &Arc<AppState>,
    in_project: impl Fn(&Session) -> bool,
) -> Result<()> {
    // A process still exiting when its rows are gone is tolerated by `watch_exit` and the hook paths.
    // The deepest level goes first: archiving an owner is refused while a session
    // bound to it still has a process, and all of them are in this project. So the
    // sessions bound to a lead session go before it, and it, bound to a console
    // session, before the unbound ones. A console session belongs to no project and
    // is not in this loop; a lead session's binding to one is left as any bound
    // session's is, and goes with its record.
    let mut in_this_project = Vec::new();
    for live in state.live_sessions() {
        if let Some(session) = state.store.get_session(&live.id)? {
            if in_project(&session) {
                in_this_project.push(session);
            }
        }
    }
    let levels = in_this_project
        .iter()
        .map(|session| Ok((session.id.clone(), binding_depth(state, session)?)))
        .collect::<Result<HashMap<String, usize>>>()?;
    in_this_project.sort_by_key(|session| std::cmp::Reverse(levels[&session.id]));
    for session in in_this_project {
        archive_session(state, &session.id)?;
    }
    Ok(())
}

/// Starts a git-status check of every project of `console_id`, concurrently; a project already
/// being checked is left alone. The statuses follow as `project_git_status` broadcasts, so this
/// returns as soon as the checks are started rather than waiting for them.
pub(super) fn refresh_git_status(state: &Arc<AppState>, console_id: &str) -> Result<()> {
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
    project_git::status::refresh(state, projects);
    Ok(())
}

#[cfg(test)]
mod tests;
