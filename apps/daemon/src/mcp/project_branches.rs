//! The tool with which a session lists a project's git branches, `list_project_branches`, each with
//! its tip, upstream and worktree and the session recorded as responsible for it, where it came
//! from (target branch and fork point) and since when. The reads go through the browse layer as
//! `list_project_changes`' do (`project_browse`), so the same reach, gate and budgets hold.

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use crate::branch_origin;
use crate::browse::budget;
use crate::browse::git::GitEnv;
use crate::protocol::{BrowseBody, Event, Session, SessionStatus};
use crate::state::AppState;

use super::args::optional_count;
use super::project_browse::{browse, git_source, plain, readable_project, MakeGit};

/// How many branches `list_project_branches` returns when the caller names no `limit`.
const DEFAULT_BRANCHES_LIMIT: usize = 200;

/// The largest `limit` it takes; a larger one is refused.
const MAX_BRANCHES_LIMIT: usize = 1000;

pub(super) async fn list_project_branches(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    list_branches_with(state, caller, arguments, GitEnv::from_shell).await
}

/// [`list_project_branches`] with the `git` environment made by `make_git`.
async fn list_branches_with(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
    make_git: MakeGit,
) -> Result<Value> {
    let project = readable_project(state, caller, arguments)?;
    let limit = optional_count(arguments, "limit", 1, MAX_BRANCHES_LIMIT)?
        .unwrap_or(DEFAULT_BRANCHES_LIMIT);
    let offset = optional_count(arguments, "offset", 0, usize::MAX)?.unwrap_or(0);
    let git = git_source(state, make_git, &project).await?;
    let body = BrowseBody::ListProjectBranches {
        project: project.id.clone(),
    };
    let (
        Event::ProjectBranches {
            branches,
            complete,
            upstreams_complete,
            default_branch,
            ..
        },
        _admission,
    ) = browse(state, make_git, body, budget::BRANCH_LIST_RESERVATION).await?
    else {
        bail!("the daemon answered something other than the branches");
    };
    let end = offset.saturating_add(limit).min(branches.len());
    let start = offset.min(end);
    let page = &branches[start..end];
    // The listing has just reconciled the repository's records, so the page's are read as they
    // stand: who is responsible comes with the listing (`origin`), and the rows add where each
    // branch came from. Whether those sessions are still there is read once for each.
    let (forks, session_states) = {
        let (state, path) = (state.clone(), project.path.clone());
        let wires: Vec<String> = page.iter().map(|branch| branch.name.clone()).collect();
        let sessions: Vec<String> = page
            .iter()
            .filter_map(|branch| branch.origin.as_ref())
            .map(|origin| origin.responsible_session.clone())
            .collect();
        tokio::task::spawn_blocking(move || {
            let forks = branch_origin::forks_in_project(&state, &path, &wires)?;
            let mut session_states: HashMap<String, &'static str> = HashMap::new();
            for id in sessions {
                if let Entry::Vacant(slot) = session_states.entry(id) {
                    let session_state = session_state(&state, slot.key())?;
                    slot.insert(session_state);
                }
            }
            anyhow::Ok((forks, session_states))
        })
        .await??
    };
    let listed = page
        .iter()
        .map(|branch| {
            let worktree = git
                .worktrees
                .iter()
                .find(|worktree| worktree.branch.as_deref() == Some(branch.name.as_str()))
                .map(|worktree| {
                    json!({
                        "path": plain(&worktree.root),
                        "main": worktree.main,
                        "holds_project_directory": worktree.id == git.worktree,
                    })
                });
            let origin = branch.origin.as_ref().map(|origin| {
                let fork = forks.get(&branch.name);
                json!({
                    "session": origin.responsible_session,
                    "title": origin.responsible_title,
                    "session_state": session_states[&origin.responsible_session],
                    "since": origin.responsible_since,
                    "target_branch": fork.and_then(|fork| fork.target_branch.as_ref()),
                    "fork_point": fork.map(|fork| &fork.fork_point),
                })
            });
            json!({
                "branch": plain(&branch.name),
                "tip": branch.commit,
                "upstream": branch.upstream.as_ref().map(|upstream| json!({
                    "branch": plain(&upstream.name),
                    "commit": upstream.commit,
                })),
                "worktree": worktree,
                "origin": origin,
            })
        })
        .collect::<Vec<Value>>();
    Ok(json!({
        "project": project.id,
        "branches": listed,
        "total": branches.len(),
        "offset": offset,
        "next_offset": (end < branches.len()).then_some(end),
        "complete": complete,
        "upstreams_complete": upstreams_complete,
        "default_branch": default_branch.map(|default| json!({
            "branch": plain(&default.name),
            "commit": default.commit,
        })),
    }))
}

/// Where the session `id` stands for a reader of a branch's record: `live` (it has a process),
/// `interrupted`, `archived`, or `gone` (no record of it is left).
fn session_state(state: &AppState, id: &str) -> Result<&'static str> {
    Ok(match state.store.get_session(id)? {
        None => "gone",
        Some(session) => match state.shown_status(&session) {
            SessionStatus::Archived => "archived",
            SessionStatus::Interrupted => "interrupted",
            _ => "live",
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::exec::tests::{arguments, RepoState};
    use crate::mcp::project_browse::test_git;
    use crate::test_support::{git, ScratchDir};

    async fn list_branches(fixture: &RepoState, caller: &str, input: Value) -> Result<Value> {
        let session = fixture.state.session_record(caller)?;
        list_branches_with(&fixture.state, &session, &arguments(input), test_git).await
    }

    fn branch_of<'a>(answer: &'a Value, name: &str) -> &'a Value {
        answer["branches"]
            .as_array()
            .unwrap()
            .iter()
            .find(|branch| branch["branch"] == name)
            .unwrap_or_else(|| panic!("{name} in {answer}"))
    }

    /// The branches of a project with their tips, worktrees and the recorded origin, including a
    /// responsible session in each state it can be in, read by every kind of session.
    #[tokio::test]
    async fn the_branches_of_a_project_are_listed_with_who_is_responsible() {
        let fixture = RepoState::new("project-branches");
        let state = &fixture.state;
        let set_status = |id: &str, status| {
            let mut session = state.session_record(id).unwrap();
            session.status = status;
            state.save_session(&session).unwrap();
        };
        set_status("kid", SessionStatus::Archived);
        set_status("lead-kid", SessionStatus::Interrupted);
        crate::test_support::branch_made_from(state, &fixture.repo, "live", "lead", Some("main"));
        fixture.branch("archived", "kid");
        fixture.branch("interrupted", "lead-kid");
        fixture.branch("gone", "vanished");
        git(&fixture.repo, &["branch", "unrecorded"]);
        let checkouts = ScratchDir::new("project-branches-checkouts");
        let side = checkouts.join("live-checkout");
        git(
            &fixture.repo,
            &["worktree", "add", "-q", side.to_str().unwrap(), "live"],
        );

        for caller in ["a", "lead", "lead-kid"] {
            let listed = list_branches(&fixture, caller, json!({ "project": "repo" }))
                .await
                .unwrap();
            assert_eq!(listed["project"], "repo", "{caller}");
            assert_eq!(listed["total"], 6, "{caller}");
            assert_eq!(listed["complete"], true, "{caller}");
            assert_eq!(listed["next_offset"], Value::Null, "{caller}");
            let main = branch_of(&listed, "main");
            assert_eq!(main["worktree"]["main"], true, "{caller}");
            assert_eq!(
                main["worktree"]["holds_project_directory"], true,
                "{caller}"
            );
            assert!(main["origin"].is_null(), "{caller}");
            assert_eq!(main["tip"].as_str().unwrap().len(), 40, "{caller}");
            assert!(
                branch_of(&listed, "unrecorded")["origin"].is_null(),
                "{caller}"
            );
            assert!(
                branch_of(&listed, "unrecorded")["worktree"].is_null(),
                "{caller}"
            );
        }
        let listed = list_branches(&fixture, "lead", json!({})).await.unwrap();
        let live = branch_of(&listed, "live");
        assert_eq!(live["worktree"]["main"], false);
        assert_eq!(live["worktree"]["holds_project_directory"], false);
        assert!(live["worktree"]["path"]
            .as_str()
            .unwrap()
            .ends_with("live-checkout"));
        assert_eq!(live["origin"]["session"], "lead");
        assert_eq!(live["origin"]["title"], "Work lead");
        assert_eq!(live["origin"]["session_state"], "live");
        assert_eq!(live["origin"]["target_branch"], "main");
        assert_eq!(live["origin"]["fork_point"].as_str().unwrap().len(), 40);
        assert!(live["origin"]["since"].as_i64().unwrap() > 0);
        for (branch, session_state) in [
            ("archived", "archived"),
            ("interrupted", "interrupted"),
            ("gone", "gone"),
        ] {
            let origin = &branch_of(&listed, branch)["origin"];
            assert_eq!(origin["session_state"], session_state, "{branch}");
            assert!(origin["target_branch"].is_null(), "{branch}");
        }

        // A page of it.
        let first = list_branches(&fixture, "a", json!({ "project": "repo", "limit": 2 }))
            .await
            .unwrap();
        assert_eq!(first["branches"].as_array().unwrap().len(), 2);
        assert_eq!(first["next_offset"], 2);
        let last = list_branches(
            &fixture,
            "a",
            json!({ "project": "repo", "limit": 2, "offset": 4 }),
        )
        .await
        .unwrap();
        assert_eq!(last["branches"].as_array().unwrap().len(), 2);
        assert_eq!(last["next_offset"], Value::Null);

        let err = list_branches(&fixture, "a", json!({})).await.unwrap_err();
        assert!(err.to_string().contains("`project` is required"), "{err}");
    }

    /// The main working copy's branch carries its `upstream`; with none it is given the remote's
    /// default branch instead, and a lookup the daemon cut short says so.
    #[tokio::test]
    async fn the_upstream_and_the_remotes_default_branch_are_those_of_the_listing() {
        let fixture = RepoState::new("project-branches-upstream");
        let repo = &fixture.repo;
        let main = String::from_utf8(git(repo, &["rev-parse", "main"]))
            .unwrap()
            .trim()
            .to_string();
        git(
            repo,
            &["remote", "add", "origin", "https://example.invalid/r.git"],
        );
        fixture.branch("other", "lead");
        let listed = |input: Value| list_branches(&fixture, "a", input);

        // Nothing on the remote: no upstream, no default branch, and the lookup was whole.
        let answer = listed(json!({ "project": "repo" })).await.unwrap();
        assert!(branch_of(&answer, "main")["upstream"].is_null());
        assert!(answer["default_branch"].is_null());
        assert_eq!(answer["upstreams_complete"], true);

        // The main working copy's branch has an upstream, and no other branch does.
        git(repo, &["update-ref", "refs/remotes/origin/main", &main]);
        git(repo, &["update-ref", "refs/remotes/origin/other", &main]);
        let answer = listed(json!({ "project": "repo" })).await.unwrap();
        assert_eq!(
            branch_of(&answer, "main")["upstream"],
            json!({ "branch": "origin/main", "commit": main })
        );
        assert!(branch_of(&answer, "other")["upstream"].is_null());
        // With an upstream it is given no default branch.
        git(
            repo,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/other",
            ],
        );
        let answer = listed(json!({ "project": "repo" })).await.unwrap();
        assert!(answer["default_branch"].is_null());

        // Without one, the remote's default branch stands in.
        git(repo, &["update-ref", "-d", "refs/remotes/origin/main"]);
        let answer = listed(json!({ "project": "repo" })).await.unwrap();
        assert!(branch_of(&answer, "main")["upstream"].is_null());
        assert_eq!(
            answer["default_branch"],
            json!({ "branch": "origin/other", "commit": main })
        );
        assert_eq!(answer["upstreams_complete"], true);

        // Past the daemon's budget the lookup is cut short, and the answer says it may be missing.
        let mut config = std::fs::read_to_string(repo.join(".git/config")).unwrap();
        for n in 0..1_800 {
            config.push_str(&format!("[remote \"r{n:05}\"]\n\turl = /nowhere\n"));
        }
        std::fs::write(repo.join(".git/config"), config).unwrap();
        git(repo, &["update-ref", "refs/remotes/r00001/trunk", &main]);
        git(
            repo,
            &[
                "symbolic-ref",
                "refs/remotes/r00001/HEAD",
                "refs/remotes/r00001/trunk",
            ],
        );
        git(repo, &["update-ref", "-d", "refs/remotes/origin/other"]);
        git(repo, &["symbolic-ref", "-d", "refs/remotes/origin/HEAD"]);
        let answer = listed(json!({ "project": "repo" })).await.unwrap();
        assert_eq!(answer["upstreams_complete"], false);
        assert!(answer["default_branch"].is_null());
    }
}
