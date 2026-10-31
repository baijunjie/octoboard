//! `archive_session` and `archive_self`: ending sessions, and what is left bound to them.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use crate::branch_cleanup;
use crate::coordinator;
use crate::mcp::args::{optional_bool, required_str};
use crate::protocol::{Session, SessionStatus};
use crate::state::AppState;

use super::resolve::{branch_names_of, resolve_owned_session};

/// Archives a session of the caller's, and once its process has ended removes what can go of the
/// branches and worktrees bound to it, never forcing anything, unless `keep`; what stays is named
/// in the answer, so the caller can deal with it.
pub(super) async fn archive_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let wanted = required_str(arguments, "session")?;
    if wanted == caller.id && caller.bound_to.is_none() {
        bail!(
            "session `{wanted}` is your own session, which `archive_session` does not archive; \
             use `archive_self` to archive yourself"
        );
    }
    let keep = optional_bool(arguments, "keep")?.unwrap_or(false);
    let target = resolve_owned_session(state, caller, wanted)?;
    let archived = coordinator::lifecycle::archive_session(state, &target.id)?;
    let mut answer = json!({ "session": target.id, "status": SessionStatus::Archived });
    if keep {
        // Nothing is removed, so nothing is waited for: the records stay bound to the archived
        // sessions, which `get_session_branches` lists and `handover_branches` can move.
        answer["branches_kept"] = json!(true);
        let kept = branch_names_of(state, archived.clone()).await?;
        answer["note"] = json!(keep_note(&target, &archived, &kept));
        return Ok(answer);
    }
    // A task of its own, so the removal finishes even if the call it answers is dropped.
    let left = tokio::spawn(branch_cleanup::remove::remove_once_stopped(
        state.clone(),
        archived,
        Vec::new(),
    ))
    .await
    .unwrap_or_else(|err| {
        tracing::warn!(%err, "removing the branches of an archived session did not finish");
        Vec::new()
    });
    if !left.is_empty() {
        answer["branches_left"] = branch_cleanup::report::for_agent(&left);
        answer["note"] = json!(
            "Archived. The git branches and worktrees in `branches_left` are bound to the sessions \
             archived and were not removed, for the reason given with each. If they are yours to \
             clean up, deal with what keeps each (commit or discard the changes, merge the \
             branch), then remove them with `git worktree remove` and `git branch -d`."
        );
    }
    Ok(answer)
}

/// What the caller is told of the branches `keep` left bound to the sessions an archive ended:
/// `kept` is the names by session id. An archive of a lead session takes the sessions under it too,
/// each of which keeps its own branches, and those are not the caller's to name as `from` to
/// `handover_branches`, which takes the caller itself or a session bound to it: the archived
/// session, once reopened, hands them to itself first.
fn keep_note(
    target: &Session,
    archived: &[Session],
    kept: &HashMap<String, Vec<String>>,
) -> String {
    let mut note = String::from(
        "Archived. Its git branches and worktrees were left as they are, as asked. They stay \
         bound to the sessions that are responsible for them (`get_session_branches` lists \
         them) and come back with those sessions when they are reopened; a session under a \
         project coordinator is reopened by that coordinator.",
    );
    let holders = |under: bool| {
        archived
            .iter()
            .filter(move |session| (session.id != target.id) == under)
            .filter(|session| kept.get(&session.id).is_some_and(|names| !names.is_empty()))
    };
    if holders(false).next().is_some() {
        note.push_str(&format!(
            " To move the branches of `{id}` itself to a session of yours now, use \
             `handover_branches` with `from` `{id}`.",
            id = target.id
        ));
    }
    let under: Vec<&Session> = holders(true).collect();
    if !under.is_empty() {
        let kept_by = under
            .iter()
            .map(|session| format!("`{}` ({})", session.id, kept[&session.id].join(", ")))
            .collect::<Vec<_>>()
            .join("; ");
        let ids = under
            .iter()
            .map(|session| format!("`{}`", session.id))
            .collect::<Vec<_>>()
            .join(", ");
        note.push_str(&format!(
            " The sessions archived with it keep theirs: {kept_by}. They are not yours to name \
             as `from`. To move them now, `reopen_session` `{id}` with a `text` asking it to \
             `handover_branches` them from each of {ids} to itself, then hand them on with \
             `handover_branches` `from` `{id}` once it is idle.",
            id = target.id
        ));
    }
    note
}

/// Archives the caller itself, which only a session that reports to nobody does: a lead session
/// wraps up with `report` instead. Its own sessions must all be archived already, since an owner
/// archives each once its task is done, so one that is not (running or interrupted alike) means
/// unfinished work and the call is refused, changing nothing. Once the record is archived the
/// process is ended off the caller's path, so this answer still reaches it.
///
/// The branches and worktrees bound to the caller are removed after its process has ended, where
/// they can go without forcing anything. What will stay is judged before the archive, since the
/// answer has to be on its way before the process is ended.
pub(super) async fn archive_self(state: &Arc<AppState>, caller: &Session) -> Result<Value> {
    let caller = archivable_self(state, caller)?;
    let left = branch_cleanup::survey::left_by(state, vec![caller.clone()]).await;
    // Taken only now, and the checks made again under it, so a lock set while the branches were
    // judged cannot slip past them; held across that wait, it would refuse every change of a lock
    // in the daemon for as long as the judging took.
    let archived = {
        let _removal = state.claim_session_removal();
        let caller = archivable_self(state, &caller)?;
        coordinator::lifecycle::archive_session(state, &caller.id)?
    };
    branch_cleanup::remove::remove_in_background(state, archived);
    let mut answer = json!({
        "session": caller.id,
        "status": SessionStatus::Archived,
        "note": "Archived; this session's process will now end.",
    });
    if !left.is_empty() {
        answer["branches_left"] = branch_cleanup::report::for_agent(&left);
        answer["note"] = json!(
            "Archived; this session's process will now end. The git branches and worktrees in \
             `branches_left` are bound to this session and will not be removed, for the reason \
             given with each; the rest that are bound to it are removed once the process has \
             ended."
        );
    }
    Ok(answer)
}

/// The caller's own record, once it is a session that may archive itself now; otherwise why not.
fn archivable_self(state: &Arc<AppState>, caller: &Session) -> Result<Session> {
    let caller = state.session_record(&caller.id)?;
    // A lead session is offered the tool with the unbound session's set, so being bound is checked
    // here.
    if caller.bound_to.is_some() {
        bail!(
            "this session reports to an owner, so it does not archive itself: call `report` with \
             status `done` and no open items, which archives it once the report is accepted"
        );
    }
    if caller.locked {
        bail!("this session is locked, so it cannot be archived; the user has to unlock it first");
    }
    let team = coordinator::bindings::team(state, &caller)?;
    let locked: Vec<Session> = team
        .iter()
        .filter(|session| session.locked)
        .cloned()
        .collect();
    if !locked.is_empty() {
        bail!(
            "this session cannot archive itself while sessions under it are locked: {}. The user \
             has to unlock them first",
            coordinator::lifecycle::session_labels(&locked)
        );
    }
    let unarchived: Vec<Session> = team
        .into_iter()
        .filter(|session| session.status != SessionStatus::Archived)
        .collect();
    if let Some(refusal) = unarchived_refusal(&caller.id, &unarchived) {
        bail!(refusal);
    }
    Ok(caller)
}

/// What refuses `archive_self` while sessions under the caller are not archived, or `None` when
/// there are none. A session bound to a lead session is not the caller's to archive, so it is named
/// under its lead with different advice from the caller's direct sessions.
fn unarchived_refusal(caller_id: &str, unarchived: &[Session]) -> Option<String> {
    let (direct, under_leads): (Vec<&Session>, Vec<&Session>) = unarchived
        .iter()
        .partition(|session| session.bound_to.as_deref() == Some(caller_id));
    let mut advice = Vec::new();
    if !direct.is_empty() {
        advice.push(format!(
            "{}: an owner archives its sessions once their tasks are done, so an unarchived one \
             means its task is not finished; finish it and archive it with `archive_session` first",
            coordinator::lifecycle::session_labels(direct)
        ));
    }
    let mut leads: Vec<&str> = under_leads
        .iter()
        .filter_map(|session| session.bound_to.as_deref())
        .collect();
    leads.sort_unstable();
    leads.dedup();
    for lead in leads {
        let named = coordinator::lifecycle::session_labels(
            under_leads
                .iter()
                .copied()
                .filter(|session| session.bound_to.as_deref() == Some(lead)),
        );
        advice.push(format!(
            "{} under the project coordinator `{lead}`: not yours to archive; wait for the \
             coordinator to finish them (its `done` report archives it), or archive `{lead}` with \
             `archive_session`, which takes along its sessions that have no process running and \
             is refused while any has one",
            named
        ));
    }
    (!advice.is_empty()).then(|| {
        format!(
            "this session cannot archive itself while sessions under it are not archived. {}. \
             Then archive yourself",
            advice.join(". ")
        )
    })
}

#[cfg(test)]
mod tests {
    use crate::mcp::exec::call;
    use crate::mcp::test_support::{
        arguments, lead_state, peer_state, project_session, with_status, RepoState,
    };
    use crate::protocol::{OrchestrationRole, Session, SessionStatus};
    use crate::state::AppState;
    use crate::test_support::running;
    use serde_json::{json, Map};
    use std::sync::Arc;

    async fn archive_self_error(state: &Arc<AppState>, caller: &str) -> String {
        call(state, caller, "archive_self", &Map::new())
            .await
            .expect_err("refused")
            .to_string()
    }

    /// An owner archives its sessions once their tasks are done, so one that is not archived —
    /// running or interrupted, under an unbound project session or two levels under a console
    /// session, below its lead session — refuses the call, names that session alone, and changes
    /// nothing.
    #[tokio::test]
    async fn a_session_with_an_unarchived_session_under_it_cannot_archive_itself() {
        let (peers, _peers_dir) = peer_state("archive-self-unarchived-peer");
        let (leads, _leads_dir) = lead_state("archive-self-unarchived-lead");
        // `a` owns `of-a` and the lead session `starter`, which owns `mine`: the other two are
        // archived, so `mine` is what is named.
        with_status(&leads, "of-a", SessionStatus::Archived);
        with_status(&leads, "starter", SessionStatus::Archived);
        for (state, caller) in [(&peers, "starter"), (&leads, "a")] {
            for status in [SessionStatus::Idle, SessionStatus::Interrupted] {
                with_status(state, "mine", status);
                let message = archive_self_error(state, caller).await;
                assert!(message.contains("`mine` (\"Work mine\")"), "{message}");
                assert!(!message.contains("`of-a`"), "{message}");
                if caller == "a" {
                    assert!(
                        message.contains("under the project coordinator `starter`"),
                        "{message}"
                    );
                } else {
                    assert!(message.contains("`archive_session`"), "{message}");
                }
                let after = state.store.get_session(caller).unwrap().unwrap();
                assert_ne!(after.status, SessionStatus::Archived, "{caller}");
                assert!(after.ended_at.is_none(), "{caller}");
            }
        }
    }

    /// A lead session is offered the tool with the unbound session's set, yet it reports to its
    /// console session and wraps up with `report`, so the call is refused in prose and changes
    /// nothing.
    #[tokio::test]
    async fn a_lead_session_is_told_to_report_done_instead_of_archiving_itself() {
        let (state, _dir) = lead_state("archive-self-lead");
        // Archived, so that being bound is the only thing that could refuse it.
        with_status(&state, "mine", SessionStatus::Archived);
        let message = archive_self_error(&state, "starter").await;
        assert!(message.contains("`report`"), "{message}");
        let after = state.store.get_session("starter").unwrap().unwrap();
        assert_ne!(after.status, SessionStatus::Archived);
    }

    /// A locked session is not archived, by itself either, and a locked session under it is
    /// named.
    #[tokio::test]
    async fn a_locked_session_cannot_archive_itself() {
        let (state, _dir) = peer_state("archive-self-locked");
        with_status(&state, "mine", SessionStatus::Archived);
        // Straight to the store: locking an archived session is refused, an old lock is not.
        state.store.set_session_locked("mine", true).unwrap();
        let message = archive_self_error(&state, "starter").await;
        assert!(message.contains("`mine` (\"Work mine\")"), "{message}");
        state.set_session_locked("mine", false).unwrap();
        state.set_session_locked("starter", true).unwrap();
        let message = archive_self_error(&state, "starter").await;
        assert!(message.contains("is locked"), "{message}");
        let after = state.store.get_session("starter").unwrap().unwrap();
        assert_ne!(after.status, SessionStatus::Archived);
    }

    /// `archive_session` on the caller's own session points to `archive_self`.
    #[tokio::test]
    async fn archiving_ones_own_session_with_archive_session_points_to_archive_self() {
        let (state, _dir) = peer_state("archive-self-pointer");
        let err = call(
            &state,
            "starter",
            "archive_session",
            &arguments(json!({ "session": "starter" })),
        )
        .await
        .expect_err("refused");
        assert!(err.to_string().contains("`archive_self`"), "{err}");
    }

    /// With every session under it archived, the call archives the caller and nobody else, and its
    /// process is ended, for a console session and an unbound project session alike.
    #[tokio::test]
    async fn archiving_oneself_archives_the_caller_alone() {
        let (state, _dir) = peer_state("archive-self-success");
        // `a` owns `of-a` and `archived-of-a`; `starter` owns `mine`.
        with_status(&state, "of-a", SessionStatus::Archived);
        with_status(&state, "mine", SessionStatus::Archived);
        let live = running(&state, "starter");
        for caller in ["a", "starter"] {
            let answer = call(&state, caller, "archive_self", &Map::new())
                .await
                .unwrap();
            assert_eq!(answer["session"], caller);
            assert_eq!(answer["status"], "archived");
            let after = state.store.get_session(caller).unwrap().unwrap();
            assert_eq!(after.status, SessionStatus::Archived, "{caller}");
            assert!(after.ended_at.is_some(), "{caller}");
        }
        for untouched in ["b", "of-b", "loose", "other-project"] {
            let session = state.store.get_session(untouched).unwrap().unwrap();
            assert_eq!(session.status, SessionStatus::Idle, "{untouched}");
        }
        // Ending the process is left to a blocking task; wait for it.
        let deadline = std::time::Instant::now() + crate::test_support::PATIENCE;
        while !live.poll_exit() && std::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(live.poll_exit());
    }

    /// `archive_session` with `keep` removes no branch or worktree, where without it a merged
    /// branch goes with its session.
    #[tokio::test]
    async fn archive_session_can_keep_the_branches() {
        let fixture = RepoState::new("archive-keep");
        let exists = |branch: &str| {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(fixture.repo.to_string_lossy().as_ref())
                .args(["branch", "--list", branch])
                .output()
                .unwrap();
            !String::from_utf8_lossy(&out.stdout).trim().is_empty()
        };
        fixture.branch("kept", "kid");
        fixture.branch("gone", "lead-kid");
        let archived = call(
            &fixture.state,
            "a",
            "archive_session",
            &arguments(json!({ "session": "kid", "keep": true })),
        )
        .await
        .unwrap();
        assert_eq!(archived["branches_kept"], true);
        assert!(archived["note"]
            .as_str()
            .unwrap()
            .contains("handover_branches"));
        assert!(exists("kept"));

        let archived = call(
            &fixture.state,
            "lead",
            "archive_session",
            &arguments(json!({ "session": "lead-kid" })),
        )
        .await
        .unwrap();
        assert!(archived.get("branches_kept").is_none());
        assert!(!exists("gone"));
    }

    /// `keep` on a lead session leaves its branches and those of the sessions under it, which
    /// are archived with it, and the note says which are not the caller's to name as `from`.
    #[tokio::test]
    async fn keeping_the_branches_of_an_archived_lead_names_those_of_its_sessions() {
        let fixture = RepoState::new("archive-keep-lead");
        for (id, owner) in [("mid", "a"), ("mid-kid", "mid"), ("mid-kid-2", "mid")] {
            fixture
                .state
                .store
                .insert_session(&Session {
                    project_id: Some("repo".to_string()),
                    orchestration_role: if id == "mid" {
                        OrchestrationRole::Coordinator
                    } else {
                        OrchestrationRole::Worker
                    },
                    ..project_session(id, "console-1", Some(owner))
                })
                .unwrap();
        }
        fixture.branch("mid-work", "mid");
        fixture.branch("kid-work", "mid-kid");
        fixture.branch("kid-2-work", "mid-kid-2");

        let archived = call(
            &fixture.state,
            "a",
            "archive_session",
            &arguments(json!({ "session": "mid", "keep": true })),
        )
        .await
        .unwrap();

        assert_eq!(archived["branches_kept"], true);
        let note = archived["note"].as_str().unwrap();
        assert!(
            note.contains("`handover_branches` with `from` `mid`"),
            "{note}"
        );
        assert!(note.contains("come back with those sessions"), "{note}");
        assert!(
            note.contains("`reopen_session` `mid` with a `text`")
                && note.contains("from each of `mid-kid`, `mid-kid-2` to itself"),
            "{note}"
        );
        assert!(
            note.contains("`mid-kid` (kid-work)") && note.contains("`mid-kid-2` (kid-2-work)"),
            "{note}"
        );
        assert!(note.contains("not yours to name as `from`"), "{note}");
        assert!(!note.contains("`mid` (mid-work)"), "{note}");
        for id in ["mid", "mid-kid", "mid-kid-2"] {
            assert_eq!(
                fixture.state.session_record(id).unwrap().status,
                SessionStatus::Archived
            );
        }
    }
}
