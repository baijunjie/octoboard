//! The tool with which a session reads the git branches a session is responsible for,
//! `get_session_branches`, each with the worktree it stands in. It reads only: the responsibility
//! is `handover_branches`' to move, and the branch names alone are part of every session record.
//! The shape of one branch's entry is shared with `handover_branches`' answer.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use serde_json::{json, Map, Value};

use crate::branch_origin::session_branches::{self, Worktree};
use crate::protocol::Session;
use crate::state::AppState;

use super::args::optional_string;
use super::exec::resolve_session;

/// The branches `session` (any session; the caller's own when it is left out) is responsible for,
/// each with the project it is in and its worktree. A console session is in no project and is
/// responsible for none, so its list is empty. Reading only; git runs on the blocking pool, and
/// only when the session is responsible for a branch.
pub(super) async fn get_session_branches(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let target = match optional_string(arguments, "session") {
        Some(wanted) if wanted != caller.id => resolve_session(state, &wanted)?,
        _ => state.session_record(&caller.id)?,
    };
    let read = {
        let (state, target) = (state.clone(), target.clone());
        tokio::task::spawn_blocking(move || session_branches::responsible_for(&state, &target))
            .await??
    };
    let branches: Vec<Value> = read
        .iter()
        .map(|responsible| {
            let mut entry = branch_entry(&responsible.branch, &responsible.worktree);
            entry["project"] = json!(responsible.project);
            entry
        })
        .collect();
    Ok(json!({ "session": target.id, "branches": branches }))
}

/// One branch and where it stands, as an answer lists it: `worktree` (its path when it has one),
/// `worktree_status` and `uncommitted_changes`.
pub(super) fn branch_entry(branch: &str, worktree: &Worktree) -> Value {
    json!({
        "branch": branch,
        "worktree": worktree_path(worktree),
        "worktree_status": worktree_status(worktree),
        "uncommitted_changes": uncommitted(worktree),
    })
}

/// The worktree a branch is checked out in, when it has one.
fn worktree_path(worktree: &Worktree) -> Option<&PathBuf> {
    match worktree {
        Worktree::CheckedOut { path, .. } | Worktree::Gone(path) => Some(path),
        Worktree::NotCheckedOut | Worktree::Unknown(_) => None,
    }
}

/// How the result says a branch stands: `checked_out`, `gone` (git's record of the worktree is
/// left, the directory is not), `none` (no worktree has it) or `unknown` (it could not be read).
fn worktree_status(worktree: &Worktree) -> &'static str {
    match worktree {
        Worktree::CheckedOut { .. } => "checked_out",
        Worktree::Gone(_) => "gone",
        Worktree::NotCheckedOut => "none",
        Worktree::Unknown(_) => "unknown",
    }
}

/// Whether the branch's worktree holds uncommitted changes; `None` with no worktree or when that
/// could not be read.
fn uncommitted(worktree: &Worktree) -> Option<bool> {
    match worktree {
        Worktree::CheckedOut { uncommitted, .. } => *uncommitted,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::exec::call;
    use crate::mcp::exec::tests::{arguments, RepoState};
    use crate::test_support::{git, ScratchDir};

    async fn read(fixture: &RepoState, caller: &str, input: Value) -> Value {
        call(
            &fixture.state,
            caller,
            "get_session_branches",
            &arguments(input),
        )
        .await
        .unwrap()
    }

    fn entry<'a>(read: &'a Value, name: &str) -> &'a Value {
        read["branches"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["branch"] == name)
            .unwrap()
    }

    /// Each kind of caller reads a session's branches with their worktrees: its own by default,
    /// another session's by id; a checked-out worktree says whether it holds uncommitted changes,
    /// and a console session has none.
    #[tokio::test]
    async fn the_branches_of_a_session_are_read_with_their_worktrees() {
        let fixture = RepoState::new("session-branches");
        for branch in ["dirty", "clean", "bare"] {
            fixture.branch(branch, "lead");
        }
        fixture.branch("kids", "lead-kid");
        let checkouts = ScratchDir::new("session-branches-checkouts");
        let (dirty, clean) = (
            checkouts.join("dirty-checkout"),
            checkouts.join("clean-checkout"),
        );
        for (path, branch) in [(&dirty, "dirty"), (&clean, "clean")] {
            git(
                &fixture.repo,
                &["worktree", "add", "-q", path.to_str().unwrap(), branch],
            );
        }
        std::fs::write(dirty.join("new.txt"), "unsaved").unwrap();

        // The caller's own, `session` left out: an unbound project session.
        let own = read(&fixture, "lead", json!({})).await;
        assert_eq!(own["session"], "lead");
        assert_eq!(own["branches"].as_array().unwrap().len(), 3);
        assert_eq!(entry(&own, "dirty")["worktree_status"], "checked_out");
        assert!(entry(&own, "dirty")["worktree"]
            .as_str()
            .unwrap()
            .ends_with("dirty-checkout"));
        assert_eq!(entry(&own, "dirty")["uncommitted_changes"], true);
        assert_eq!(entry(&own, "clean")["worktree_status"], "checked_out");
        assert_eq!(entry(&own, "clean")["uncommitted_changes"], false);
        assert_eq!(entry(&own, "bare")["worktree_status"], "none");
        assert!(entry(&own, "bare")["worktree"].is_null());
        assert!(entry(&own, "bare")["uncommitted_changes"].is_null());
        assert_eq!(entry(&own, "bare")["project"], "repo");

        // Another session's, read by a console session, by its owner, and by a bound project
        // session that owns nobody.
        for caller in ["a", "lead", "lead-kid"] {
            let theirs = read(&fixture, caller, json!({ "session": "lead-kid" })).await;
            assert_eq!(theirs["session"], "lead-kid", "{caller}");
            assert_eq!(theirs["branches"].as_array().unwrap().len(), 1, "{caller}");
            assert_eq!(
                entry(&theirs, "kids")["worktree_status"],
                "none",
                "{caller}"
            );
        }
        // A bound project session reads its own by default.
        let kid = read(&fixture, "lead-kid", json!({})).await;
        assert_eq!(kid["branches"][0]["branch"], "kids");

        // A console session is responsible for no branch, and neither is a project session with
        // none.
        assert_eq!(read(&fixture, "a", json!({})).await["branches"], json!([]));
        for session in ["b", "kid"] {
            let none = read(&fixture, "lead", json!({ "session": session })).await;
            assert_eq!(none["branches"], json!([]), "{session}");
        }

        let err = call(
            &fixture.state,
            "a",
            "get_session_branches",
            &arguments(json!({ "session": "absent" })),
        )
        .await
        .expect_err("no such session");
        assert!(err.to_string().contains("there is no session"), "{err}");
    }

    /// A worktree whose directory was deleted is `gone`, with the path git remembers and no word
    /// on its changes.
    #[tokio::test]
    async fn a_worktree_whose_directory_is_gone_says_so() {
        let fixture = RepoState::new("session-branches-gone");
        fixture.branch("lost", "lead");
        let checkouts = ScratchDir::new("session-branches-gone-checkouts");
        let path = checkouts.join("lost-checkout");
        git(
            &fixture.repo,
            &["worktree", "add", "-q", path.to_str().unwrap(), "lost"],
        );
        std::fs::remove_dir_all(&path).unwrap();

        let own = read(&fixture, "lead", json!({})).await;
        let lost = entry(&own, "lost");
        assert_eq!(lost["worktree_status"], "gone");
        assert!(lost["worktree"]
            .as_str()
            .unwrap()
            .ends_with("lost-checkout"));
        assert!(lost["uncommitted_changes"].is_null());
    }

    /// With no `git` to run the branches are still listed, each with an unknown worktree.
    #[tokio::test]
    async fn without_git_every_worktree_is_unknown() {
        let fixture = RepoState::new("session-branches-no-git");
        fixture.branch("one", "lead");
        fixture.branch("two", "lead");
        fixture.state.host_tools.replace(crate::host_tools::Tools {
            git: || Err(anyhow::anyhow!("git must not run")),
            ..crate::test_support::host_tools()
        });

        let own = read(&fixture, "lead", json!({})).await;
        assert_eq!(own["branches"].as_array().unwrap().len(), 2);
        for name in ["one", "two"] {
            assert_eq!(entry(&own, name)["worktree_status"], "unknown");
            assert!(entry(&own, name)["worktree"].is_null());
            assert!(entry(&own, name)["uncommitted_changes"].is_null());
        }
    }
}
