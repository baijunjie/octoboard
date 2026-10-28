//! The tool with which a session reads the git branches a session is responsible for,
//! `get_session_branches`, each with the worktree it stands in, where it came from (its target
//! branch and fork point) and what archiving the session would do with it. It reads only: the
//! responsibility is `handover_branches`' to move, and the branch names alone are part of every
//! session record. The worktree part of one branch's entry is shared with `handover_branches`'
//! answer.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use serde_json::{json, Map, Value};

use crate::branch_origin::cleanup;
use crate::branch_origin::session_branches::{self, Worktree};
use crate::protocol::Session;
use crate::state::AppState;

use super::exec::session_or_caller;

/// The branches `session` (any session; the caller's own when it is left out) is responsible for,
/// each with the project it is in, its worktree, the target branch and fork point recorded for it,
/// since when the session has been responsible, and what archiving the session would do with it
/// (`archive`, `null` when that could not be judged). A console session is in no project and is
/// responsible for none, so its list is empty. Reading only; git runs on the blocking pool, and
/// only when the session is responsible for a branch.
pub(super) async fn get_session_branches(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let target = session_or_caller(state, caller, arguments)?;
    let (read, inspected) = {
        let (state, target) = (state.clone(), target.clone());
        tokio::task::spawn_blocking(move || {
            let read = session_branches::responsible_for(&state, &target)?;
            // What archiving would leave, judged as the archive judges it; a branch is still
            // listed when that cannot be.
            let inspected = if read.is_empty() {
                None
            } else {
                cleanup::inspect_unforced(&state, std::slice::from_ref(&target))
                    .map_err(
                        |err| tracing::warn!(%err, "judging what archiving would remove failed"),
                    )
                    .ok()
            };
            anyhow::Ok((read, inspected))
        })
        .await??
    };
    let branches: Vec<Value> = read
        .iter()
        .map(|responsible| {
            let mut entry = branch_entry(&responsible.branch, &responsible.worktree);
            entry["project"] = json!(responsible.project);
            entry["target_branch"] = json!(responsible.target_branch);
            entry["fork_point"] = json!(responsible.fork_point);
            entry["responsible_since"] = json!(responsible.since);
            entry["archive"] = inspected.as_deref().map_or(Value::Null, |items| {
                cleanup::archive_outlook(items, &responsible.wire)
            });
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

    /// Each branch says where it came from, and what archiving the session would do with it and
    /// why: a merged one goes, with its clean worktree, one in a worktree with changes stays
    /// checked out, one with commits of its own is unmerged, and one checked out in the main
    /// working copy has no worktree to remove.
    #[tokio::test]
    async fn a_branch_says_where_it_came_from_and_what_archiving_would_do() {
        let fixture = RepoState::new("session-branches-archive");
        let repo = &fixture.repo;
        crate::test_support::branch_made_from(&fixture.state, repo, "feat", "lead", Some("main"));
        for branch in ["plain", "dirty", "tidy", "unmerged", "inmain"] {
            fixture.branch(branch, "lead");
        }
        let checkouts = ScratchDir::new("session-branches-archive-checkouts");
        let tidy = checkouts.join("tidy-checkout");
        git(
            repo,
            &["worktree", "add", "-q", tidy.to_str().unwrap(), "tidy"],
        );
        let dirty = checkouts.join("dirty-checkout");
        git(
            repo,
            &["worktree", "add", "-q", dirty.to_str().unwrap(), "dirty"],
        );
        // Unmerged as well: the worktree that stays keeps it, not its own commits.
        std::fs::write(dirty.join("mine.txt"), "mine").unwrap();
        git(&dirty, &["add", "mine.txt"]);
        git(&dirty, &["commit", "-q", "-m", "mine"]);
        std::fs::write(dirty.join("new.txt"), "unsaved").unwrap();
        let temporary = checkouts.join("unmerged-checkout");
        git(
            repo,
            &[
                "worktree",
                "add",
                "-q",
                temporary.to_str().unwrap(),
                "unmerged",
            ],
        );
        std::fs::write(temporary.join("own.txt"), "own").unwrap();
        git(&temporary, &["add", "own.txt"]);
        git(&temporary, &["commit", "-q", "-m", "own work"]);
        git(repo, &["worktree", "remove", temporary.to_str().unwrap()]);
        git(repo, &["switch", "-q", "inmain"]);

        let own = read(&fixture, "lead", json!({})).await;
        let feat = entry(&own, "feat");
        assert_eq!(feat["target_branch"], "main");
        assert_eq!(feat["fork_point"].as_str().unwrap().len(), 40);
        assert!(feat["responsible_since"].as_i64().unwrap() > 0);
        assert_eq!(feat["archive"]["removed"], true);
        assert_eq!(feat["archive"]["branch"]["state"], "removable");
        assert!(feat["archive"]["worktree"].is_null());
        assert!(entry(&own, "plain")["target_branch"].is_null());

        // A clean worktree goes with its merged branch.
        let tidy = &entry(&own, "tidy")["archive"];
        assert_eq!(tidy["removed"], true);
        assert_eq!(tidy["branch"]["state"], "removable");
        assert_eq!(tidy["worktree"]["state"], "removable");

        // The main working copy is never removed, so it is no worktree of the archive's.
        let in_main = entry(&own, "inmain");
        assert_eq!(in_main["worktree_status"], "checked_out");
        assert!(in_main["archive"]["worktree"].is_null());
        assert_eq!(in_main["archive"]["removed"], false);
        assert_eq!(in_main["archive"]["branch"]["state"], "checked_out");

        let dirty = &entry(&own, "dirty")["archive"];
        assert_eq!(dirty["removed"], false);
        assert_eq!(dirty["worktree"]["state"], "uncommitted_changes");
        assert_eq!(dirty["branch"]["state"], "checked_out");
        assert_eq!(
            dirty["branch"]["reason"],
            "it is checked out in a worktree that stays"
        );
        assert_eq!(dirty["worktree"]["reason"], "it has uncommitted changes");

        let unmerged = &entry(&own, "unmerged")["archive"];
        assert_eq!(unmerged["removed"], false);
        assert_eq!(unmerged["branch"]["state"], "unmerged");
        assert_eq!(unmerged["branch"]["reason"], "it is not merged");
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
            assert!(entry(&own, name)["archive"].is_null());
        }
    }
}
