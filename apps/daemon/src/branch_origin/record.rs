//! Recording a branch a session's git reports it created.

use std::path::Path;
use std::time::Instant;

use anyhow::Result;

use super::{creation_of, recorded_title, repo_key, reported_repository, target, RefReport};
use crate::browse::budget;
use crate::git::object_id::is_object_id;
use crate::git::reflog;
use crate::git::repo_files::{repository_of, uses_reftable};
use crate::protocol::Session;
use crate::state::AppState;
use crate::store::BranchOrigin;
use crate::wire_path;

/// How far apart the time a report gives and the time of the reflog entry it is about may lie. The
/// report is made by a process git starts right after the entry is written, so anything further
/// apart is about another branch of the same name; the margin is for a machine whose process
/// starts stall for seconds.
const REPORT_SLACK_SECS: i64 = 60;

/// Records the branches a session's git reports it created. A report that cannot be taken — from
/// a session that is gone or is no project's, about a repository the session's project is not in,
/// or about a branch that is no longer the one reported — records nothing and says nothing: the
/// hook that sent it has long returned.
pub fn ref_created(state: &AppState, session_id: &str, report: &RefReport) {
    if let Err(err) = record_created(state, session_id, report) {
        tracing::debug!(session = %session_id, %err, "branch origins: recording a report failed");
    }
}

fn record_created(state: &AppState, session_id: &str, report: &RefReport) -> Result<()> {
    let Some(session) = state.store.get_session(session_id)? else {
        return Ok(());
    };
    let Some(project_id) = session.project_id.clone() else {
        return Ok(());
    };
    let Some(project) = state.store.get_project(&project_id)? else {
        return Ok(());
    };
    let Some(repo) = repository_of(Path::new(&project.path)) else {
        return Ok(());
    };
    if reported_repository(&report.cwd, report.git_dir.as_deref()).as_deref()
        != Some(repo.as_path())
    {
        tracing::debug!(session = %session_id, "branch origins: a report about another repository");
        return Ok(());
    }
    let key = repo_key(&repo);
    let responsible = Responsible::of(&session);
    let head = report.head.as_deref().and_then(branch_of);

    state.branch_origins.serialized(&repo, || {
        // The project was read before the gate: a deletion that has cleared the repository since
        // would leave what is written now with nothing to clear it.
        if state.store.get_project(&project_id)?.is_none() {
            return Ok(());
        }
        let rows = state.store.branch_origins(&key)?;
        let mut put: Vec<BranchOrigin> = Vec::new();
        for created in &report.refs {
            let Some(branch) = branch_of(&created.name) else {
                continue;
            };
            if !is_object_id(&created.oid) {
                continue;
            }
            let Some(birth_time) = birth_of(&repo, &branch, &created.oid, report.unix_time) else {
                continue;
            };
            let recorded = rows.iter().any(|row| {
                row.branch == branch
                    && row.birth_commit == created.oid
                    && row.birth_time == birth_time
            });
            if recorded {
                continue;
            }
            put.push(BranchOrigin {
                repository: key.clone(),
                target_branch: target::target_of(&repo, &branch, head.as_deref()),
                branch,
                created_at: birth_time * 1000,
                birth_commit: created.oid.clone(),
                birth_time,
                responsible_session: responsible.id.clone(),
                responsible_since: birth_time * 1000,
                responsible_title: responsible.title.clone(),
            });
        }
        if put.is_empty() {
            return Ok(());
        }
        state
            .store
            .apply_branch_origins(&key, &[], &put, budget::MAX_BRANCH_ORIGINS)?;
        state.branch_origins.wrote(&repo, Instant::now());
        Ok(())
    })
}

/// The branch a created reference is, as a wire path; `None` for a reference outside
/// `refs/heads/`.
fn branch_of(reference: &str) -> Option<String> {
    let name = wire_path::decode(reference)?;
    let branch = name.strip_prefix(b"refs/heads/")?;
    (!branch.is_empty()).then(|| wire_path::encode(branch))
}

/// When the reported branch was born, in seconds since the epoch, provided it is still the branch
/// the report is about: its reflog begins with a creation at the reported commit, at a time close
/// to the report's, and says nothing of the branch being a rename or a copy of another. A
/// repository whose references are files but keeps no reflog for the branch cannot tell it from a
/// later branch of the same name, and gives no birth. A reftable repository has no reflog files at
/// all, and the report's own time stands in.
pub(super) fn birth_of(repo: &Path, branch: &str, oid: &str, reported_secs: i64) -> Option<i64> {
    if uses_reftable(repo) {
        return Some(reported_secs);
    }
    let entry = creation_of(repo, branch)?;
    if entry.commit != oid || (entry.time - reported_secs).abs() > REPORT_SLACK_SECS {
        return None;
    }
    let name = wire_path::decode(branch)?;
    let moved_here = [b"Branch: renamed ".as_slice(), b"Branch: copied "]
        .iter()
        .any(|kind| reflog::has_message(repo, &name, |message| is_move_into(message, kind, &name)));
    (!moved_here).then_some(entry.time)
}

/// Whether a reflog message is git's record of a rename or copy (`kind`) that ends at the branch
/// `name`: `Branch: renamed refs/heads/<old> to refs/heads/<name>`.
fn is_move_into(message: &[u8], kind: &[u8], name: &[u8]) -> bool {
    let mut target = b" to refs/heads/".to_vec();
    target.extend_from_slice(name);
    message.starts_with(kind) && message.ends_with(&target)
}

/// A session as a row remembers it.
#[derive(Debug, Clone)]
struct Responsible {
    id: String,
    title: String,
}

impl Responsible {
    fn of(session: &Session) -> Self {
        Self {
            id: session.id.clone(),
            title: recorded_title(&session.title),
        }
    }
}
