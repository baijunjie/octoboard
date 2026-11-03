//! `transfer_branches`: moving the responsibility for git branches from one session to another.
//! Only the branch records move; no session is replaced (that is `handoff_session`), and nothing in
//! git or on disk changes.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

use crate::branch_origin::session_branches::{self, Worktree};
use crate::branch_origin::transfer::{
    branch_name, branch_noun_and_pronoun, move_branches, named_branches, BranchTransfer,
    TransferOutcome,
};
use crate::git::repo_files::repository_of;
use crate::mcp::tools::session_branches::branch_entry;
use crate::messaging::naming::named_session;
use crate::messaging::texts::NOTICE_PREFIX;
use crate::protocol::{Project, Session, SessionKind, SessionStatus};
use crate::state::AppState;

use super::{branch_names, held_by, refuse_if_at_work, untold_note, Parties};

/// Moves the responsibility for `branches` from `from` to `to`, each the caller or a session it
/// owns (`from` may be archived, to recover what an archived session left behind). Needs no
/// approval: the caller owns every session but itself.
///
/// Refused, changing nothing, for a `from` that is another session and is in the middle of a turn,
/// since it would go on working on branches that are no longer its own, and for a branch `from`
/// is not responsible for. The sessions are claimed against a switch or handoff meanwhile, and
/// `from`'s messages are held, while the checks are made again and the branches move.
pub(in crate::mcp) async fn transfer_branches(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let branches = branch_names(arguments, "branches")?
        .ok_or_else(|| anyhow!("`branches` is required and must name at least one branch"))?;
    let mut parties = Parties::claim(state, caller, arguments)?;
    let (from, to) = (parties.from.clone(), parties.to.clone());
    if to.status == SessionStatus::Archived {
        bail!(
            "{} is archived and cannot be responsible for branches; reopen it first",
            named_session(&to)
        );
    }
    if from.id != caller.id {
        refuse_if_at_work(
            state,
            &from,
            "goes on writing to the worktrees of its branches. Transfer them once it is \
             awaiting instructions, or has been interrupted",
        )?;
    }
    let from_project = project_of(state, &from)?;
    let to_project = project_of(state, &to)?;
    let transfer = held_by(state, caller, &from, &from_project, branches).await?;
    refuse_unless_in_the_repository(&transfer, &to, &to_project).await?;

    let moved = match move_branches(state, &transfer, &from, &to).await {
        TransferOutcome::Moved(moved) if !moved.moved.is_empty() => moved,
        TransferOutcome::Moved(moved) => {
            let (noun, _) = branch_noun_and_pronoun(&moved.unmoved);
            let were = if moved.unmoved.len() == 1 {
                "it was"
            } else {
                "they were"
            };
            bail!(
                "Nothing was transferred: by then {} was no longer responsible for the {noun} {}, \
                 or {were} gone",
                named_session(&from),
                named_branches(&moved.unmoved)
            )
        }
        TransferOutcome::Failed(_) => {
            bail!("Octoboard could not record the transfer; nothing was transferred")
        }
    };

    let standing = standing_of(state, &transfer.repository, &moved.moved).await;
    let messages = messages(caller, &from, &to, &standing, &transfer.targets);
    let untold = parties.tell(state, caller, messages, "branch").await;

    let mut answer = json!({
        "from": from.id,
        "to": to.id,
        "branches": standing
            .iter()
            .map(|(_, branch, worktree)| branch_entry(branch, worktree))
            .collect::<Vec<_>>(),
        "not_transferred": moved
            .unmoved
            .iter()
            .map(|branch| branch_name(branch))
            .collect::<Vec<_>>(),
    });
    let (_, them) = branch_noun_and_pronoun(&moved.moved);
    let mut note = format!(
        "Transferred {} from {} to {}; the new session is the one responsible for {them} from now \
         on, and cleanup of {them} when a session is archived follows it. Nothing changed in git \
         or on disk: a branch checked out in a worktree stays checked out there, with whatever \
         uncommitted changes it holds, as `branches` lists.",
        named_branches(&moved.moved),
        named_session(&from),
        named_session(&to),
    );
    if !moved.unmoved.is_empty() {
        note.push_str(&format!(
            " Not transferred, because {} was no longer responsible for {them} by then: {}.",
            named_session(&from),
            named_branches(&moved.unmoved)
        ));
    }
    note.push_str(&untold_note(&untold));
    answer["note"] = json!(note);
    Ok(answer)
}

/// The project of `session`, whose repository the branches are in. A console session is in no
/// project and is responsible for no branch, so it can neither give nor take one.
fn project_of(state: &Arc<AppState>, session: &Session) -> Result<Project> {
    if session.kind == SessionKind::Console {
        bail!(
            "{} is a console session, which is responsible for no branch: name a project session \
             as `from` and `to`",
            named_session(session)
        );
    }
    session
        .project_id
        .as_deref()
        .map(|id| state.store.get_project(id))
        .transpose()?
        .flatten()
        .ok_or_else(|| {
            anyhow!(
                "the project of {} is no longer on record",
                named_session(session)
            )
        })
}

/// Refuses unless `project`, the project of `to`, is in the repository the branches are in.
async fn refuse_unless_in_the_repository(
    transfer: &BranchTransfer,
    to: &Session,
    project: &Project,
) -> Result<()> {
    let path = project.path.clone();
    let repository = tokio::task::spawn_blocking(move || repository_of(Path::new(&path))).await?;
    if repository.as_deref() != Some(transfer.repository.as_path()) {
        bail!(
            "{} is in the project `{}` (\"{}\"), which is not in the repository the branches are \
             in",
            named_session(to),
            project.id,
            project.name
        );
    }
    Ok(())
}

/// Where each of `branches` is checked out now, as its wire path, its name as text and its
/// worktree. Reading only; a failure leaves the worktrees unknown, since the branches have moved by
/// then.
async fn standing_of(
    state: &Arc<AppState>,
    repository: &Path,
    branches: &[String],
) -> Vec<(String, String, Worktree)> {
    let (state, repository, names) = (state.clone(), repository.to_path_buf(), branches.to_vec());
    let read = tokio::task::spawn_blocking(move || {
        session_branches::worktrees_of(&state, &repository, &names)
    })
    .await;
    read.unwrap_or_else(|err| {
        tracing::warn!(%err, "reading the worktrees of the branches transferred failed");
        branches
            .iter()
            .map(|wire| {
                (
                    wire.clone(),
                    branch_name(wire),
                    Worktree::Unknown(format!("{err:#}")),
                )
            })
            .collect()
    })
}

/// The branches with their worktrees and whether those hold uncommitted changes, as a message
/// names them, one per line, each with its target branch from `targets` (by wire path) where the
/// message is to give it. Each entry of `standing` carries the branch's wire path beside its
/// lossily decoded name, so the lookup in `targets` does not depend on the name being UTF-8.
fn standing_lines(
    standing: &[(String, String, Worktree)],
    targets: Option<&HashMap<String, String>>,
) -> String {
    standing
        .iter()
        .map(|(wire, branch, worktree)| {
            let place = match worktree {
                Worktree::CheckedOut { path, uncommitted } => format!(
                    "checked out in the worktree {}, {}",
                    path.display(),
                    match uncommitted {
                        Some(true) => "which holds uncommitted changes",
                        Some(false) => "which has no uncommitted changes",
                        None => "whose uncommitted changes could not be read",
                    }
                ),
                Worktree::Gone(path) => {
                    format!("last checked out in {}, which is gone", path.display())
                }
                Worktree::NotCheckedOut => "not checked out in any worktree".to_string(),
                Worktree::Unknown(_) => "in a worktree that could not be read".to_string(),
            };
            let target = targets
                .and_then(|targets| targets.get(wire))
                .map(|target| format!("; its target branch is `{target}`"))
                .unwrap_or_default();
            format!("\n- `{branch}`: {place}{target}")
        })
        .collect()
}

/// What `from` and `to` are told of the branches that moved, for [`Parties::tell`]. `to` is also
/// told each branch's target branch (`targets`, by wire path), the one it merges the branch into.
fn messages(
    caller: &Session,
    from: &Session,
    to: &Session,
    standing: &[(String, String, Worktree)],
    targets: &HashMap<String, String>,
) -> [String; 2] {
    let names: Vec<String> = standing
        .iter()
        .map(|(_, branch, _)| branch.clone())
        .collect();
    let (noun, them) = branch_noun_and_pronoun(&names);
    let named = names
        .iter()
        .map(|branch| format!("`{branch}`"))
        .collect::<Vec<_>>()
        .join(", ");
    let lines = standing_lines(standing, None);
    let lines_with_targets = standing_lines(standing, Some(targets));
    let owner = named_session(caller);
    [
        format!(
            "{NOTICE_PREFIX}: {owner}, your owner, transferred the {noun} {named} to \
             {to}. You are no longer responsible for {them}: do not touch {them} or the \
             worktrees {them} {is} checked out in, and leave any uncommitted changes there as \
             they are.{lines}",
            to = named_session(to),
            is = if names.len() == 1 { "is" } else { "are" },
        ),
        format!(
            "{NOTICE_PREFIX}: {owner}, your owner, transferred the {noun} {named} to \
             you from {from}. You are now the session responsible for {them}, and the previous \
             session no longer is. Your owner will tell you what to do with {them}.\
             {lines_with_targets}",
            from = named_session(from),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::branch_origin::repo_key;
    use crate::mcp::test_support::{arguments, project_session, shared_console_state};
    use crate::protocol::AgentId;
    use crate::test_support::{
        branch_made_by, git, heard_after_marker, output_containing, repo_with, running, ScratchDir,
    };
    use crate::wire_path;

    /// A line names a branch's target branch by its wire path, so a branch whose name is not UTF-8,
    /// shown lossily, still gets it.
    #[test]
    fn a_non_utf8_branch_line_names_its_target_branch() {
        let wire = wire_path::encode(b"f\xff");
        let standing = [(
            wire.clone(),
            "f\u{FFFD}".to_string(),
            Worktree::NotCheckedOut,
        )];
        let targets = HashMap::from([(wire, "main".to_string())]);
        let lines = standing_lines(&standing, Some(&targets));
        assert!(lines.contains("its target branch is `main`"), "{lines}");
        assert!(!standing_lines(&standing, None).contains("target branch"));
    }

    /// `shared_console_state` plus a project, `repo`, in a real repository, and sessions in it:
    /// `kid` and `other-kid` and an archived `old-kid` of the console session `a`, `busy-kid` of
    /// `a` in the middle of a turn, `waiting-kid` waiting for the user, `codex-kid` of `a`, a Codex
    /// session that is idle, `kid-of-b` of `b`, and `coordinator`, unbound, with `coordinator-kid`
    /// bound to it.
    struct Fixture {
        state: Arc<AppState>,
        repo: ScratchDir,
        _dir: ScratchDir,
    }

    fn fixture(name: &str) -> Fixture {
        let (state, dir) = shared_console_state(name);
        let repo = repo_with(&format!("transfer-{name}-repo"), &[(b"a", b"a")]);
        state
            .store
            .insert_project(&Project {
                id: "repo".to_string(),
                name: "repo".to_string(),
                path: repo.to_string_lossy().into_owned(),
                ..state.store.get_project("project-1").unwrap().unwrap()
            })
            .unwrap();
        for (id, owner, status) in [
            ("kid", Some("a"), SessionStatus::Idle),
            ("other-kid", Some("a"), SessionStatus::Idle),
            ("old-kid", Some("a"), SessionStatus::Archived),
            ("busy-kid", Some("a"), SessionStatus::Working),
            ("waiting-kid", Some("a"), SessionStatus::WaitingUser),
            ("codex-kid", Some("a"), SessionStatus::Idle),
            ("kid-of-b", Some("b"), SessionStatus::Idle),
            ("coordinator", None, SessionStatus::Idle),
            ("coordinator-kid", Some("coordinator"), SessionStatus::Idle),
        ] {
            state
                .store
                .insert_session(&Session {
                    project_id: Some("repo".to_string()),
                    status,
                    agent: if id == "codex-kid" {
                        AgentId::CODEX
                    } else {
                        AgentId::CLAUDE
                    },
                    ..project_session(id, "console-1", owner)
                })
                .unwrap();
        }
        Fixture {
            state,
            repo,
            _dir: dir,
        }
    }

    impl Fixture {
        /// Makes `branch`, recorded as `session`'s.
        fn branch(&self, branch: &str, session: &str) {
            branch_made_by(&self.state, &self.repo, branch, session);
        }

        /// Who is responsible for `branch`.
        fn responsible_for(&self, branch: &str) -> String {
            let key = repo_key(&repository_of(&self.repo).unwrap());
            self.state
                .store
                .branch_origins(&key)
                .unwrap()
                .into_iter()
                .find(|row| row.branch == branch)
                .unwrap()
                .responsible_session
        }

        async fn call(&self, caller: &str, input: Value) -> Result<Value> {
            transfer_branches(
                &self.state,
                &self.state.session_record(caller).unwrap(),
                &arguments(input),
            )
            .await
        }
    }

    /// The owner moves a branch between two of its own sessions: the records change hands, both
    /// are told, and the owner is not, since its result says it.
    #[tokio::test]
    async fn an_owner_transfers_a_branch_from_one_session_of_its_own_to_another() {
        let fixture = fixture("transfer-child-to-child");
        crate::test_support::branch_made_from(
            &fixture.state,
            &fixture.repo,
            "feature",
            "kid",
            Some("main"),
        );
        let caller = running(&fixture.state, "a");
        let from = running(&fixture.state, "kid");
        let to = running(&fixture.state, "other-kid");

        let result = fixture
            .call(
                "a",
                json!({ "branches": ["feature", " feature "], "from": "kid", "to": "other-kid" }),
            )
            .await
            .unwrap();

        assert_eq!(fixture.responsible_for("feature"), "other-kid");
        assert_eq!(result["from"], "kid");
        assert_eq!(result["to"], "other-kid");
        assert_eq!(result["branches"].as_array().unwrap().len(), 1);
        assert_eq!(result["branches"][0]["branch"], "feature");
        assert_eq!(result["not_transferred"], json!([]));
        let told = output_containing(&from, "do not touch");
        assert!(
            told.contains("You are no longer responsible for it")
                && told.contains("`feature`")
                && !told.contains("target branch"),
            "{told}"
        );
        // The session taking the branch is told which branch it merges into.
        let told = output_containing(&to, "Your owner will tell you what to do");
        assert!(
            told.contains(
                "`feature`: not checked out in any worktree; its target branch is `main`"
            ) && !told.contains("carry on"),
            "{told}"
        );
        assert!(
            !heard_after_marker(&fixture.state, &caller).contains("transferred the branch"),
            "the caller was told"
        );
        // Cleanup on archive follows the new responsible session.
        let record = |id: &str| fixture.state.session_record(id).unwrap();
        let branches_of = |id: &str| {
            session_branches::of_session(&fixture.state, &record(id))
                .unwrap()
                .into_iter()
                .map(|branch| branch.branch)
                .collect::<Vec<_>>()
        };
        assert_eq!(branches_of("other-kid"), ["feature"]);
        assert!(branches_of("kid").is_empty());
    }

    /// A project session takes a branch of a session it owns, `to` left out, and gives one of its
    /// own to a session it owns, `from` left out.
    #[tokio::test]
    async fn a_session_takes_a_branch_from_its_child_and_gives_one_to_it() {
        let fixture = fixture("transfer-self");
        fixture.branch("kids", "coordinator-kid");
        fixture.branch("mine", "coordinator");
        let caller = running(&fixture.state, "coordinator");
        let child = running(&fixture.state, "coordinator-kid");

        fixture
            .call(
                "coordinator",
                json!({ "branches": ["kids"], "from": "coordinator-kid" }),
            )
            .await
            .unwrap();
        assert_eq!(fixture.responsible_for("kids"), "coordinator");
        output_containing(&child, "do not touch");

        fixture
            .call(
                "coordinator",
                json!({ "branches": ["mine"], "to": "coordinator-kid" }),
            )
            .await
            .unwrap();
        assert_eq!(fixture.responsible_for("mine"), "coordinator-kid");
        output_containing(&child, "You are now the session responsible");
        assert!(!heard_after_marker(&fixture.state, &caller).contains("Message from Octoboard"));
    }

    /// The branches an archived session left behind can be taken over, and an archived session
    /// cannot be given any.
    #[tokio::test]
    async fn an_archived_session_gives_up_its_branches_but_takes_none() {
        let fixture = fixture("transfer-archived");
        fixture.branch("left", "old-kid");
        fixture.branch("mine", "coordinator");
        fixture
            .state
            .store
            .insert_session(&Session {
                project_id: Some("repo".to_string()),
                status: SessionStatus::Archived,
                ..project_session("old-coordinator-kid", "console-1", Some("coordinator"))
            })
            .unwrap();
        fixture.branch("left-of-coordinator", "old-coordinator-kid");

        let result = fixture
            .call(
                "coordinator",
                json!({ "branches": ["left-of-coordinator"], "from": "old-coordinator-kid" }),
            )
            .await
            .unwrap();
        assert_eq!(
            fixture.responsible_for("left-of-coordinator"),
            "coordinator"
        );
        // The archived session has no process to be told in, and the result says so.
        let note = result["note"].as_str().unwrap();
        assert!(
            note.ends_with(
                " Octoboard could not send the project session `old-coordinator-kid` (\"Work \
                 old-coordinator-kid\") a message about the transfer: tell it yourself once it can take \
                 a message."
            ),
            "{note}"
        );

        let err = fixture
            .call(
                "coordinator",
                json!({ "branches": ["mine"], "to": "old-coordinator-kid" }),
            )
            .await
            .expect_err("refused");
        assert!(err.to_string().contains("is archived"), "{err}");
        assert_eq!(fixture.responsible_for("mine"), "coordinator");
    }

    /// Everything that is refused changes no record.
    #[tokio::test]
    async fn a_refused_transfer_changes_nothing() {
        let fixture = fixture("transfer-refused");
        fixture.branch("kids", "kid");
        fixture.branch("busy", "busy-kid");
        fixture.branch("waiting", "waiting-kid");
        fixture.branch("prompted", "codex-kid");
        // The record says idle, while the terminal shows Codex's prompt for the user.
        let codex =
            crate::test_support::fake_live_session("codex-kid", AgentId::CODEX, 80, 24, "sleep 30");
        fixture.state.register_live(codex.clone());
        assert!(codex.observe_action_title(Some("[ ! ] Action Required | repo")));
        fixture.branch("theirs", "kid-of-b");
        fixture.branch("unowned", "loose");

        for (caller, input, reason) in [
            // Mid-turn.
            (
                "a",
                json!({ "branches": ["busy"], "from": "busy-kid", "to": "kid" }),
                "in the middle of a turn",
            ),
            (
                "a",
                json!({ "branches": ["waiting"], "from": "waiting-kid", "to": "kid" }),
                "waiting for the user",
            ),
            (
                "a",
                json!({ "branches": ["prompted"], "from": "codex-kid", "to": "kid" }),
                "waiting for the user",
            ),
            // A branch `from` is not responsible for, or that is not there.
            (
                "a",
                json!({ "branches": ["kids"], "from": "other-kid", "to": "kid" }),
                "is not a branch the project session `other-kid`",
            ),
            (
                "a",
                json!({ "branches": ["kids", "absent"], "from": "kid", "to": "other-kid" }),
                "`absent` is not a branch the project session `kid`",
            ),
            (
                "coordinator",
                json!({ "branches": ["kids"], "to": "coordinator-kid" }),
                "`kids` is not a branch you are responsible for",
            ),
            // Sessions that are not the caller's.
            (
                "a",
                json!({ "branches": ["theirs"], "from": "kid-of-b", "to": "kid" }),
                "bound to another console session, `b`",
            ),
            (
                "a",
                json!({ "branches": ["kids"], "from": "kid", "to": "loose" }),
                "not bound to any session",
            ),
            (
                "a",
                json!({ "branches": ["kids"], "from": "kid", "to": "coordinator-kid" }),
                "bound to a project session, `coordinator`",
            ),
            // The shape of the call.
            (
                "a",
                json!({ "branches": ["kids"], "from": "kid", "to": "kid" }),
                "are the same session",
            ),
            (
                "coordinator",
                json!({ "branches": ["kids"], "to": "coordinator" }),
                "are the same session",
            ),
            (
                "a",
                json!({ "branches": ["kids"], "to": "kid" }),
                "is a console session",
            ),
            (
                "a",
                json!({ "branches": [], "from": "kid", "to": "other-kid" }),
                "at least one branch",
            ),
            (
                "a",
                json!({ "branches": [" "], "from": "kid", "to": "other-kid" }),
                "blank name",
            ),
            (
                "a",
                json!({ "branches": "kids", "from": "kid", "to": "other-kid" }),
                "array of strings",
            ),
            (
                "a",
                json!({ "from": "kid", "to": "other-kid" }),
                "`branches` is required",
            ),
            // A session in a project of another repository.
            (
                "a",
                json!({ "branches": ["kids"], "from": "kid", "to": "of-a" }),
                "not in the repository the branches are in",
            ),
        ] {
            let err = fixture
                .call(caller, input.clone())
                .await
                .expect_err("refused");
            assert!(err.to_string().contains(reason), "{input}: {err}");
        }
        for (branch, holder) in [
            ("kids", "kid"),
            ("busy", "busy-kid"),
            ("waiting", "waiting-kid"),
            ("prompted", "codex-kid"),
            ("theirs", "kid-of-b"),
            ("unowned", "loose"),
        ] {
            assert_eq!(fixture.responsible_for(branch), holder);
        }
    }

    /// The caller itself is mid-turn whenever it calls, which does not stop it giving its own
    /// branches away.
    #[tokio::test]
    async fn the_caller_may_give_a_branch_while_it_works() {
        let fixture = fixture("transfer-caller-working");
        fixture
            .state
            .store
            .insert_session(&Session {
                project_id: Some("repo".to_string()),
                status: SessionStatus::Working,
                ..project_session("busy-coordinator", "console-1", None)
            })
            .unwrap();
        fixture
            .state
            .store
            .insert_session(&Session {
                project_id: Some("repo".to_string()),
                ..project_session(
                    "busy-coordinator-kid",
                    "console-1",
                    Some("busy-coordinator"),
                )
            })
            .unwrap();
        fixture.branch("mine", "busy-coordinator");
        fixture
            .call(
                "busy-coordinator",
                json!({ "branches": ["mine"], "to": "busy-coordinator-kid" }),
            )
            .await
            .unwrap();
        assert_eq!(fixture.responsible_for("mine"), "busy-coordinator-kid");
    }

    /// A worktree with changes in it does not stop the move, and the result names the worktree and
    /// says whether it holds uncommitted changes; a branch with no worktree has neither.
    #[tokio::test]
    async fn the_result_reports_each_worktree_and_whether_it_has_uncommitted_changes() {
        let fixture = fixture("transfer-worktrees");
        for branch in ["dirty", "clean", "bare"] {
            fixture.branch(branch, "kid");
        }
        let worktrees = ScratchDir::new("transfer-worktrees-checkouts");
        let (dirty, clean) = (
            worktrees.join("dirty-checkout"),
            worktrees.join("clean-checkout"),
        );
        git(
            &fixture.repo,
            &["worktree", "add", "-q", dirty.to_str().unwrap(), "dirty"],
        );
        git(
            &fixture.repo,
            &["worktree", "add", "-q", clean.to_str().unwrap(), "clean"],
        );
        std::fs::write(dirty.join("new.txt"), "unsaved").unwrap();
        let from = running(&fixture.state, "kid");

        let result = fixture
            .call(
                "a",
                json!({ "branches": ["dirty", "clean", "bare"], "from": "kid", "to": "other-kid" }),
            )
            .await
            .unwrap();

        let listed = result["branches"].as_array().unwrap();
        let find = |name: &str| listed.iter().find(|entry| entry["branch"] == name).unwrap();
        assert!(find("dirty")["worktree"]
            .as_str()
            .unwrap()
            .ends_with("dirty-checkout"));
        assert_eq!(find("dirty")["uncommitted_changes"], true);
        assert!(find("clean")["worktree"]
            .as_str()
            .unwrap()
            .ends_with("clean-checkout"));
        assert_eq!(find("clean")["uncommitted_changes"], false);
        assert!(find("bare")["worktree"].is_null());
        assert_eq!(find("bare")["worktree_status"], "none");
        assert_eq!(find("dirty")["worktree_status"], "checked_out");
        assert!(find("bare")["uncommitted_changes"].is_null());
        for branch in ["dirty", "clean", "bare"] {
            assert_eq!(fixture.responsible_for(branch), "other-kid");
        }
        let told = output_containing(&from, "no uncommitted changes");
        assert!(
            told.contains("dirty-checkout") && told.contains("holds uncommitted changes"),
            "{told}"
        );
    }

    /// A session another switch or handoff has claimed is not transferred from or to meanwhile.
    #[tokio::test]
    async fn a_session_that_is_being_switched_is_left_alone() {
        let fixture = fixture("transfer-claimed");
        fixture.branch("feature", "kid");
        let claim = fixture.state.begin_switch("other-kid").unwrap();

        let err = fixture
            .call(
                "a",
                json!({ "branches": ["feature"], "from": "kid", "to": "other-kid" }),
            )
            .await
            .expect_err("refused");
        assert!(err.to_string().contains("already being started"), "{err}");
        assert_eq!(fixture.responsible_for("feature"), "kid");

        drop(claim);
        fixture
            .call(
                "a",
                json!({ "branches": ["feature"], "from": "kid", "to": "other-kid" }),
            )
            .await
            .unwrap();
    }
}
