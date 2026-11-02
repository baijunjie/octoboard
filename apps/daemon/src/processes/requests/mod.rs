//! The process trees a client sees and acts on: `list_processes`, `stop_processes` and
//! `keep_processes`, and the trees an archive's confirmation lists. The work is the ledger's
//! (`crate::processes::ledger`); this module picks whose trees to ask it for, and keeps its
//! blocking calls off the runtime.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use crate::processes::ledger::{ProcessCount, ProcessTree};
use crate::protocol::{error_code, CodedError, ProcessTarget, Project};
use crate::state::AppState;
use crate::store::LOCAL_HOST_ID;

/// How long a tree's top process must have run before an archive asks the user about it: what
/// started a moment ago is a command still finishing, not something left running.
pub const ASK_AFTER: Duration = Duration::from_secs(30);

#[cfg(test)]
thread_local! {
    /// [`ASK_AFTER`] for a test that cannot wait that long, on the test's own thread.
    static ASK_AFTER_OVERRIDE: std::cell::Cell<Option<Duration>> = const { std::cell::Cell::new(None) };
}

/// [`ASK_AFTER`], or a test's override of it. Read on the thread that handles the request.
pub fn ask_after() -> Duration {
    #[cfg(test)]
    if let Some(after) = ASK_AFTER_OVERRIDE.get() {
        return after;
    }
    ASK_AFTER
}

/// How recently a list of the same session's or project's trees answers a `list_processes`
/// again, and the count of every project's and console session's trees a
/// `list_all_processes`: clients poll them on a timer of their own, every few seconds each, and
/// several may be connected. Short enough that a refresh the user asks for after acting still
/// reads the table; a stop or a keep through the daemon makes the next list read it whatever the
/// floor.
pub const LIST_FLOOR: Duration = Duration::from_secs(2);

#[cfg(test)]
thread_local! {
    /// [`LIST_FLOOR`] for a test, on the test's own thread.
    static LIST_FLOOR_OVERRIDE: std::cell::Cell<Option<Duration>> = const { std::cell::Cell::new(None) };
}

fn list_floor() -> Duration {
    #[cfg(test)]
    if let Some(floor) = LIST_FLOOR_OVERRIDE.get() {
        return floor;
    }
    LIST_FLOOR
}

/// How long a `list_all_processes` waits for the count another request is computing. Well past
/// what a computation takes when every look it makes runs to its deadline, so only one that is
/// stuck past those deadlines is given up on.
const COUNT_WAIT: Duration = Duration::from_secs(10);

#[cfg(test)]
thread_local! {
    /// [`COUNT_WAIT`] for a test, on the test's own thread.
    static COUNT_WAIT_OVERRIDE: std::cell::Cell<Option<Duration>> =
        const { std::cell::Cell::new(None) };
}

/// [`COUNT_WAIT`]; under test at least [`PATIENCE`](crate::test_support::PATIENCE), unless a
/// test overrides it, so a computation slowed by the suite's load is still waited for.
fn count_wait() -> Duration {
    #[cfg(test)]
    return COUNT_WAIT_OVERRIDE
        .get()
        .unwrap_or(COUNT_WAIT.max(crate::test_support::PATIENCE));
    #[cfg(not(test))]
    COUNT_WAIT
}

/// The trees `target` names. A session that has no process is refused; a project with none
/// running lists only its processes, the trees no session owns: its leftovers and the trees kept
/// under it. A list of the same target computed less than [`LIST_FLOOR`] ago answers again.
pub async fn list(state: &Arc<AppState>, target: ProcessTarget) -> Result<Vec<ProcessTree>> {
    let floor = list_floor();
    match target {
        ProcessTarget::Session { session_id } => {
            state.session_record(&session_id)?;
            if !state.has_process(&session_id) {
                return Err(CodedError::raised(
                    error_code::SESSION_NOT_RUNNING,
                    format!("session {session_id} has no running process"),
                    &[("session", &session_id)],
                ));
            }
            let key = format!("session:{session_id}");
            if let Some(trees) = state.process_ledger.listed_lately(&key, floor) {
                return Ok(trees);
            }
            let state = Arc::clone(state);
            Ok(tokio::task::spawn_blocking(move || {
                let generation = state.process_ledger.lists_generation();
                let trees = state
                    .process_ledger
                    .trees_of_sessions(&[session_id], Duration::ZERO);
                state
                    .process_ledger
                    .note_listed(key, generation, trees.clone());
                trees
            })
            .await?)
        }
        ProcessTarget::Project { project_id } => {
            let key = format!("project:{project_id}");
            if let Some(trees) = state.process_ledger.listed_lately(&key, floor) {
                return Ok(trees);
            }
            let project = state.store.require_project(&project_id)?;
            let sessions: Vec<String> = state
                .store
                .list_sessions()?
                .into_iter()
                .filter(|session| {
                    session.project_id.as_deref() == Some(project_id.as_str())
                        && state.has_process(&session.id)
                })
                .map(|session| session.id)
                .collect();
            let state = Arc::clone(state);
            Ok(tokio::task::spawn_blocking(move || {
                let dirs = project_dirs(&project);
                let generation = state.process_ledger.lists_generation();
                let trees = state
                    .process_ledger
                    .trees_of_project(&sessions, &project.id, &dirs);
                state
                    .process_ledger
                    .note_listed(key, generation, trees.clone());
                trees
            })
            .await?)
        }
    }
}

/// How many trees each project and each console session has: one entry per project the store
/// lists that has any, in the store's order, then one per console session that has any. Only
/// counted, never described, but counting still reads the whole table and looks at the working
/// directory of every process that may have been left in a project; clients poll it on a timer
/// of their own, so counts computed less than [`LIST_FLOOR`] ago answer again, and requests that
/// overlap wait for one computation and share it, for up to [`COUNT_WAIT`].
pub async fn list_all(state: &Arc<AppState>) -> Result<Vec<ProcessCount>> {
    let floor = list_floor();
    if let Some(counts) = state.process_ledger.counted_lately(floor) {
        return Ok(counts);
    }
    let claim = state.process_ledger.claim_counting();
    let Ok(claim) = tokio::time::timeout(count_wait(), claim).await else {
        // The computation under way is stuck, or every later count would wait on it with this
        // one. The counts noted last answer, however old. Without any, the request fails, as one
        // that cannot read the store does: a client keeps the counts it shows, where an empty
        // answer would tell it nothing runs, and a computation of this request's own would hold
        // another blocking thread on what is likely stuck the same way.
        if let Some(counts) = state.process_ledger.counted_lately(Duration::MAX) {
            return Ok(counts);
        }
        anyhow::bail!("counting the processes did not finish in time");
    };
    // What the computation this request waited for noted.
    if let Some(counts) = state.process_ledger.counted_lately(floor) {
        return Ok(counts);
    }
    let sessions: Vec<(String, Option<String>)> = state
        .store
        .list_sessions()?
        .into_iter()
        .filter(|session| state.has_process(&session.id))
        .map(|session| (session.id, session.project_id))
        .collect();
    let projects: Vec<(String, Option<PathBuf>)> = state
        .store
        .list_projects()?
        .into_iter()
        .map(|project| {
            let dir = (project.host_id == LOCAL_HOST_ID).then(|| PathBuf::from(project.path));
            (project.id, dir)
        })
        .collect();
    let state = Arc::clone(state);
    Ok(tokio::task::spawn_blocking(move || {
        // Held until the counts are noted, or the computation fails, whatever becomes of the
        // request that is waiting for it.
        let _claim = claim;
        let generation = state.process_ledger.lists_generation();
        let counts = state.process_ledger.trees_counted(&sessions, &projects);
        state
            .process_ledger
            .note_counted(generation, counts.clone());
        counts
    })
    .await?)
}

/// The directories a leftover of `project` is looked for in: its own, when it is local.
fn project_dirs(project: &Project) -> Vec<PathBuf> {
    if project.host_id == LOCAL_HOST_ID {
        vec![PathBuf::from(&project.path)]
    } else {
        Vec::new()
    }
}

/// The processes of `project`, the trees no session owns (left behind in it, or kept under it):
/// what its removal stops, and what its confirmation counts.
pub async fn project_processes(
    state: &Arc<AppState>,
    project: &Project,
) -> Result<Vec<ProcessTree>> {
    let (state, id, dirs) = (Arc::clone(state), project.id.clone(), project_dirs(project));
    Ok(
        tokio::task::spawn_blocking(move || state.process_ledger.project_processes(&id, &dirs))
            .await?,
    )
}

/// Stops the processes of each of `projects` (see [`project_processes`]), for their removal.
pub async fn stop_project_processes(state: &Arc<AppState>, projects: &[Project]) {
    let targets: Vec<(String, Vec<PathBuf>)> = projects
        .iter()
        .map(|project| (project.id.clone(), project_dirs(project)))
        .collect();
    let state = Arc::clone(state);
    let stopped = tokio::task::spawn_blocking(move || {
        for (id, dirs) in &targets {
            state.process_ledger.stop_project_processes(id, dirs);
        }
    })
    .await;
    if let Err(err) = stopped {
        tracing::warn!(%err, "stopping the processes of a removed project did not finish");
    }
}

/// The local projects by id, with their directories, where a leftover is looked for.
fn local_projects(state: &AppState) -> Result<Vec<(String, PathBuf)>> {
    Ok(state
        .store
        .list_projects()?
        .into_iter()
        .filter(|project| project.host_id == LOCAL_HOST_ID)
        .map(|project| (project.id, PathBuf::from(project.path)))
        .collect())
}

/// The directories of the local projects, where a leftover is looked for.
fn leftover_dirs(state: &AppState) -> Result<Vec<PathBuf>> {
    Ok(local_projects(state)?
        .into_iter()
        .map(|(_, dir)| dir)
        .collect())
}

/// Stops the trees `ids` name that are still a tree of a running launch, a leftover of a project
/// or a tree the user kept; returns the ids it stopped.
pub async fn stop(state: &Arc<AppState>, ids: Vec<String>) -> Result<Vec<String>> {
    let dirs = leftover_dirs(state)?;
    let state = Arc::clone(state);
    Ok(tokio::task::spawn_blocking(move || state.process_ledger.stop_trees(&ids, &dirs)).await?)
}

/// Makes the trees `ids` name that are still a tree of a running launch, or a leftover of a
/// project, the user's, so the ledger never stops them unless asked to; they stay listed among
/// the trees of their project that no session owns. Returns the ids it kept.
pub async fn keep(state: &Arc<AppState>, ids: Vec<String>) -> Result<Vec<String>> {
    let projects = local_projects(state)?;
    let session_projects: HashMap<String, String> = state
        .store
        .list_sessions()?
        .into_iter()
        .filter_map(|session| Some((session.id, session.project_id?)))
        .collect();
    let state = Arc::clone(state);
    Ok(tokio::task::spawn_blocking(move || {
        state
            .process_ledger
            .keep_trees(&ids, &projects, &session_projects)
    })
    .await?)
}

/// The trees of `sessions` whose top has run for at least `min_age`: what an archive of them
/// would stop and asks the user about. A session with no process has none.
pub async fn long_running(
    state: &Arc<AppState>,
    sessions: Vec<String>,
    min_age: Duration,
) -> Result<Vec<ProcessTree>> {
    let running: Vec<String> = sessions
        .into_iter()
        .filter(|session| state.has_process(session))
        .collect();
    if running.is_empty() {
        return Ok(Vec::new());
    }
    let state = Arc::clone(state);
    Ok(tokio::task::spawn_blocking(move || {
        state.process_ledger.trees_of_sessions(&running, min_age)
    })
    .await?)
}

#[cfg(test)]
mod tests;
