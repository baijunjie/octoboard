//! The branches sessions are responsible for, grouped by the repository they are in.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

use super::{is_same_branch, repo_key, repository_of};
use crate::protocol::Session;
use crate::state::AppState;

/// One branch a session is responsible for: its name as a wire path, the session, and that
/// session's project.
#[derive(Clone)]
pub(super) struct BoundBranch {
    pub(super) branch: String,
    pub(super) session: String,
    pub(super) project: String,
}

/// One repository the sessions' projects are in, with the branches in it they are responsible
/// for.
#[derive(Clone)]
pub(super) struct Bound {
    pub(super) repo: PathBuf,
    pub(super) branches: Vec<BoundBranch>,
}

/// The repositories `sessions`' projects are in that hold a branch one of them is responsible
/// for. Read from the store and the reflog files only, so it starts no process.
pub(super) fn bound_in(state: &AppState, sessions: &[Session]) -> Result<Vec<Bound>> {
    let mut repos: Vec<(PathBuf, HashMap<&str, String>)> = Vec::new();
    // The repository of each project, found once however many of the sessions are in it.
    let mut found: HashMap<&str, Option<(String, PathBuf)>> = HashMap::new();
    for session in sessions {
        let Some(project_id) = &session.project_id else {
            continue;
        };
        let located = match found.get(project_id.as_str()) {
            Some(located) => located.clone(),
            None => {
                let located = match state.store.get_project(project_id)? {
                    Some(project) => {
                        repository_of(Path::new(&project.path)).map(|repo| (project.id, repo))
                    }
                    None => None,
                };
                found.insert(project_id, located.clone());
                located
            }
        };
        let Some((project, repo)) = located else {
            continue;
        };
        match repos.iter_mut().find(|(known, _)| *known == repo) {
            Some((_, of)) => {
                of.insert(&session.id, project);
            }
            None => repos.push((repo, HashMap::from([(session.id.as_str(), project)]))),
        }
    }
    let mut bound = Vec::new();
    for (repo, projects) in repos {
        let branches: Vec<BoundBranch> = state
            .store
            .branch_origins(&repo_key(&repo))?
            .into_iter()
            // The asked sessions' rows first: the reflog is read only for those.
            .filter(|row| projects.contains_key(row.responsible_session.as_str()))
            .filter(|row| is_same_branch(&repo, row))
            .map(|row| {
                let project = projects[row.responsible_session.as_str()].clone();
                BoundBranch {
                    branch: row.branch,
                    session: row.responsible_session,
                    project,
                }
            })
            .collect();
        if !branches.is_empty() {
            bound.push(Bound { repo, branches });
        }
    }
    Ok(bound)
}
