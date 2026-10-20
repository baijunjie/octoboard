//! The tool with which an owner moves the responsibility for git branches between sessions,
//! `handover_branches`: from itself to a session of its own, from one to itself, or from one of its
//! sessions to another. Only the branch records move; no session is replaced (that is
//! `handoff_session`), and nothing in git or on disk changes. The checks on the branches named,
//! which `new_session` and `request_session` make too, are here as well.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

use crate::branch_origin::handover::{
    branch_name, branch_noun_and_pronoun, move_branches, named_branches, Handed, Handover,
};
use crate::branch_origin::session_branches::{self, Worktree};
use crate::branch_origin::{self, repository_of};
use crate::browse::wire_path;
use crate::protocol::{error_code, CodedError, Project, Role, Session, SessionStatus};
use crate::reporting::{self, WhenBlocked};
use crate::sharing::named_session;
use crate::state::AppState;

use super::args::{optional_string, optional_string_list};
use super::exec::resolve_owned_session;

/// The names in the list argument `key`, each trimmed, as wire paths and named once. `None` when
/// the argument is absent or names none; a blank name is refused.
pub(super) fn branch_names(
    arguments: &Map<String, Value>,
    key: &str,
) -> Result<Option<Vec<String>>> {
    let Some(names) = optional_string_list(arguments, key)? else {
        return Ok(None);
    };
    let mut branches: Vec<String> = Vec::new();
    for name in &names {
        let name = name.trim();
        if name.is_empty() {
            bail!("`{key}` must not hold a blank name");
        }
        let wire = wire_path::encode(name.as_bytes());
        if !branches.contains(&wire) {
            branches.push(wire);
        }
    }
    Ok((!branches.is_empty()).then_some(branches))
}

/// The repository `project` is in and `branches` (wire paths) of it, checked to be all `holder`'s
/// to give. The refusal speaks to `caller`: of its own branches when it is the holder, else of the
/// holder's.
pub(super) async fn held_by(
    state: &Arc<AppState>,
    caller: &Session,
    holder: &Session,
    project: &Project,
    branches: Vec<String>,
) -> Result<Handover> {
    // Finding the repository reads the filesystem and the holder check reads the store.
    let (repository, not_held) = {
        let state = state.clone();
        let path = project.path.clone();
        let (branches, holder) = (branches.clone(), holder.id.clone());
        tokio::task::spawn_blocking(move || {
            let Some(repository) = repository_of(Path::new(&path)) else {
                return Ok(None);
            };
            let not_held =
                branch_origin::first_not_held_by(&state, &repository, &branches, &holder)?;
            Ok::<_, anyhow::Error>(Some((repository, not_held)))
        })
        .await??
        .ok_or_else(|| {
            anyhow!(
                "the project `{}` (\"{}\") is not in a git repository, so there is no branch to \
                 hand to a session there",
                project.id,
                project.name
            )
        })?
    };
    if let Some(branch) = not_held {
        let place = format!(
            "in the repository of the project `{}` (\"{}\"): it does not exist there, or it was \
             not created by",
            project.id, project.name
        );
        let branch = named_branches(&[branch]);
        if holder.id == caller.id {
            bail!(
                "{branch} is not a branch you are responsible for {place} you or handed to you, or \
                 another session is responsible for it now. Only a branch you are responsible for \
                 can be handed over"
            );
        }
        bail!(
            "{branch} is not a branch {holder} is responsible for {place} that session or handed \
             to it, or another session is responsible for it now. Only a branch that session is \
             responsible for can be handed over",
            holder = named_session(holder)
        );
    }
    Ok(Handover {
        repository,
        branches,
    })
}

/// Moves the responsibility for `branches` from `from` to `to`, each the caller or a session it
/// owns (`from` may be archived, to recover what an archived session left behind). Needs no
/// approval: the caller owns every session but itself.
///
/// Refused, changing nothing, for a `from` that is another session and is in the middle of a turn,
/// since it would go on working on branches that are no longer its own, and for a branch `from`
/// is not responsible for. The sessions are claimed against a switch or handoff meanwhile, and
/// `from`'s messages are held, while the checks are made again and the branches move.
pub(super) async fn handover_branches(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let branches = branch_names(arguments, "branches")?
        .ok_or_else(|| anyhow!("`branches` is required and must name at least one branch"))?;
    let from_id = party(state, caller, arguments, "from")?.id;
    let to_id = party(state, caller, arguments, "to")?.id;
    if from_id == to_id {
        bail!("`from` and `to` are the same session, `{from_id}`; there is nothing to hand over");
    }

    // Everything above can be stale by the time the branches move: a claim keeps another switch
    // or handoff off a session, and `from` takes no message that starts a turn, while everything
    // is read and checked again.
    let _from_claim = (from_id != caller.id)
        .then(|| state.begin_switch(&from_id))
        .transpose()?;
    let _to_claim = (to_id != caller.id)
        .then(|| state.begin_switch(&to_id))
        .transpose()?;
    let hold = (from_id != caller.id).then(|| state.hold_outbox(&from_id));
    let from = party(state, caller, arguments, "from")?;
    let to = party(state, caller, arguments, "to")?;
    if to.status == SessionStatus::Archived {
        bail!(
            "{} is archived and cannot be responsible for branches; reopen it first",
            named_session(&to)
        );
    }
    if from.id != caller.id {
        refuse_if_at_work(state, &from)?;
    }
    let from_project = project_of(state, &from)?;
    let to_project = project_of(state, &to)?;
    let handover = held_by(state, caller, &from, &from_project, branches).await?;
    refuse_unless_in_the_repository(&handover, &to, &to_project).await?;

    let moved = match move_branches(state, &handover, &from, &to).await {
        Handed::Moved(moved) if !moved.moved.is_empty() => moved,
        Handed::Moved(moved) => {
            let (noun, _) = branch_noun_and_pronoun(&moved.unmoved);
            let were = if moved.unmoved.len() == 1 {
                "it was"
            } else {
                "they were"
            };
            bail!(
                "Nothing was handed over: by then {} was no longer responsible for the {noun} {}, \
                 or {were} gone",
                named_session(&from),
                named_branches(&moved.unmoved)
            )
        }
        Handed::Failed(_) => {
            bail!("Octoboard could not record the handover; nothing was handed over")
        }
    };

    let standing = standing_of(state, &handover.repository, &moved.moved).await;
    let untold = tell(state, caller, &from, &to, &standing).await;
    drop(hold);

    let mut answer = json!({
        "from": from.id,
        "to": to.id,
        "branches": standing.iter().map(|(branch, worktree)| json!({
            "branch": branch,
            "worktree": worktree_path(worktree),
            "worktree_status": worktree_status(worktree),
            "uncommitted_changes": uncommitted(worktree),
        })).collect::<Vec<_>>(),
        "not_handed_over": moved
            .unmoved
            .iter()
            .map(|branch| branch_name(branch))
            .collect::<Vec<_>>(),
    });
    let (_, them) = branch_noun_and_pronoun(&moved.moved);
    let mut note = format!(
        "Handed over {} from {} to {}; the new session is the one responsible for {them} from now \
         on, and cleanup of {them} when a session is archived follows it. Nothing changed in git \
         or on disk: a branch checked out in a worktree stays checked out there, with whatever \
         uncommitted changes it holds, as `branches` lists.",
        named_branches(&moved.moved),
        named_session(&from),
        named_session(&to),
    );
    if !moved.unmoved.is_empty() {
        note.push_str(&format!(
            " Not handed over, because {} was no longer responsible for {them} by then: {}.",
            named_session(&from),
            named_branches(&moved.unmoved)
        ));
    }
    for session in &untold {
        note.push_str(&format!(
            " Octoboard could not send {} a message about the handover: tell it yourself once it \
             can take a message.",
            named_session(session)
        ));
    }
    answer["note"] = json!(note);
    Ok(answer)
}

/// The session `key` names — the caller itself when it is left out — as the caller may act on
/// it: itself, or a session it owns.
fn party(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
    key: &str,
) -> Result<Session> {
    match optional_string(arguments, key) {
        Some(wanted) if wanted != caller.id => resolve_owned_session(state, caller, &wanted),
        _ => state.session_record(&caller.id),
    }
}

/// The project of `session`, whose repository the branches are in. A console session is in no
/// project and is responsible for no branch, so it can neither give nor take one.
fn project_of(state: &Arc<AppState>, session: &Session) -> Result<Project> {
    if session.role == Role::Console {
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
    handover: &Handover,
    to: &Session,
    project: &Project,
) -> Result<()> {
    let path = project.path.clone();
    let repository = tokio::task::spawn_blocking(move || repository_of(Path::new(&path))).await?;
    if repository.as_deref() != Some(handover.repository.as_path()) {
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

/// A session in the middle of a turn — working, starting, or held up for the user (a permission
/// prompt, a question, a session request waiting) — goes on writing to the worktrees of its
/// branches, so it is not asked to give one up. Both the record and the status the agent is at
/// count: a prompt the agent shows (Codex's "Action Required" terminal title) can be seen before
/// the record has caught up with it.
fn refuse_if_at_work(state: &Arc<AppState>, from: &Session) -> Result<()> {
    let busy = [from.status, state.agent_status(from)]
        .into_iter()
        .find(|status| {
            matches!(
                status,
                SessionStatus::Working | SessionStatus::Starting | SessionStatus::WaitingUser
            )
        });
    let Some(status) = busy else {
        return Ok(());
    };
    Err(CodedError::raised(
        error_code::SESSION_WORKING,
        format!(
            "{} is {}: a session in the middle of a turn goes on writing to the worktrees of its \
             branches. Hand them over once it is awaiting instructions, or has been interrupted",
            named_session(from),
            match status {
                SessionStatus::Working => "in the middle of a turn",
                SessionStatus::Starting => "still starting",
                _ => "waiting for the user, so in the middle of a turn",
            }
        ),
        &[("session", &from.id)],
    ))
}

/// Where each of `branches` is checked out now. Reading only; a failure is logged and leaves the
/// worktrees unknown, since the branches have moved by then.
async fn standing_of(
    state: &Arc<AppState>,
    repository: &Path,
    branches: &[String],
) -> Vec<(String, Worktree)> {
    let (state, repository, names) = (state.clone(), repository.to_path_buf(), branches.to_vec());
    let read = tokio::task::spawn_blocking(move || {
        session_branches::worktrees_of(&state, &repository, &names)
    })
    .await
    .map_err(anyhow::Error::from)
    .and_then(|read| read);
    read.unwrap_or_else(|err| {
        tracing::warn!(%err, "reading the worktrees of the branches handed over failed");
        branches
            .iter()
            .map(|branch| (branch_name(branch), Worktree::Unknown(format!("{err:#}"))))
            .collect()
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

/// The branches with their worktrees and whether those hold uncommitted changes, as a message
/// names them, one per line.
fn standing_lines(standing: &[(String, Worktree)]) -> String {
    standing
        .iter()
        .map(|(branch, worktree)| {
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
            format!("\n- `{branch}`: {place}")
        })
        .collect()
}

/// Tells `from` and `to` what moved, unless one is the caller, which knows from the result.
/// Returns the sessions that could not be told, for the result to name: one with no process, or
/// one that took no message. The records are right either way.
async fn tell(
    state: &Arc<AppState>,
    caller: &Session,
    from: &Session,
    to: &Session,
    standing: &[(String, Worktree)],
) -> Vec<Session> {
    let names: Vec<String> = standing.iter().map(|(branch, _)| branch.clone()).collect();
    let (noun, them) = branch_noun_and_pronoun(&names);
    let named = names
        .iter()
        .map(|branch| format!("`{branch}`"))
        .collect::<Vec<_>>()
        .join(", ");
    let lines = standing_lines(standing);
    let owner = named_session(caller);
    let messages = [
        (
            from,
            format!(
                "Message from Octoboard: {owner}, your owner, handed the {noun} {named} over to \
                 {to}. You are no longer responsible for {them}: do not touch {them} or the \
                 worktrees {them} {is} checked out in, and leave any uncommitted changes there as \
                 they are.{lines}",
                to = named_session(to),
                is = if names.len() == 1 { "is" } else { "are" },
            ),
        ),
        (
            to,
            format!(
                "Message from Octoboard: {owner}, your owner, handed the {noun} {named} over to \
                 you from {from}. You are now the session responsible for {them}, and the previous \
                 session no longer is. Your owner will tell you what to do with {them}.{lines}",
                from = named_session(from),
            ),
        ),
    ];
    let mut untold = Vec::new();
    for (session, text) in messages {
        if session.id == caller.id {
            continue;
        }
        // Read again: the status in hand is from before the move.
        let current = state
            .store
            .get_session(&session.id)
            .ok()
            .flatten()
            .unwrap_or_else(|| session.clone());
        if current.status.is_dormant() {
            untold.push(current);
            continue;
        }
        if let Err(err) =
            reporting::write_message_off_runtime(state, &session.id, text, WhenBlocked::Queue).await
        {
            tracing::warn!(
                session = %session.id,
                %err,
                "telling a session of a branch handover failed"
            );
            untold.push(current);
        }
    }
    untold
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::branch_origin::repo_key;
    use crate::mcp::exec::tests::{arguments, project_session, shared_console_state};
    use crate::protocol::Agent;
    use crate::test_support::{
        branch_made_by, git, heard_after_marker, output_containing, repo_with, running, ScratchDir,
    };

    /// `shared_console_state` plus a project, `repo`, in a real repository, and sessions in it:
    /// `kid` and `other-kid` and an archived `old-kid` of the console session `a`, `busy-kid` of
    /// `a` in the middle of a turn, `waiting-kid` waiting for the user, `codex-kid` of `a`, a
    /// Codex session that is idle, `kid-of-b` of `b`, and `lead`, unbound, with `lead-kid` bound to
    /// it.
    struct Fixture {
        state: Arc<AppState>,
        repo: ScratchDir,
        _dir: ScratchDir,
    }

    fn fixture(name: &str) -> Fixture {
        let (state, dir) = shared_console_state(name);
        let repo = repo_with(&format!("handover-{name}-repo"), &[(b"a", b"a")]);
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
            ("lead", None, SessionStatus::Idle),
            ("lead-kid", Some("lead"), SessionStatus::Idle),
        ] {
            state
                .store
                .insert_session(&Session {
                    project_id: Some("repo".to_string()),
                    status,
                    agent: if id == "codex-kid" {
                        Agent::Codex
                    } else {
                        Agent::Claude
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
            handover_branches(
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
    async fn an_owner_hands_a_branch_from_one_session_of_its_own_to_another() {
        let fixture = fixture("handover-child-to-child");
        fixture.branch("feature", "kid");
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
        let told = output_containing(&from, "do not touch");
        assert!(
            told.contains("You are no longer responsible for it") && told.contains("`feature`"),
            "{told}"
        );
        let told = output_containing(&to, "Your owner will tell you what to do");
        assert!(
            told.contains("`feature`") && !told.contains("carry on"),
            "{told}"
        );
        assert!(
            !heard_after_marker(&fixture.state, &caller).contains("handed the branch"),
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
        let fixture = fixture("handover-self");
        fixture.branch("kids", "lead-kid");
        fixture.branch("mine", "lead");
        let caller = running(&fixture.state, "lead");
        let child = running(&fixture.state, "lead-kid");

        fixture
            .call("lead", json!({ "branches": ["kids"], "from": "lead-kid" }))
            .await
            .unwrap();
        assert_eq!(fixture.responsible_for("kids"), "lead");
        output_containing(&child, "do not touch");

        fixture
            .call("lead", json!({ "branches": ["mine"], "to": "lead-kid" }))
            .await
            .unwrap();
        assert_eq!(fixture.responsible_for("mine"), "lead-kid");
        output_containing(&child, "You are now the session responsible");
        assert!(!heard_after_marker(&fixture.state, &caller).contains("Message from Octoboard"));
    }

    /// The branches an archived session left behind can be taken over, and an archived session
    /// cannot be given any.
    #[tokio::test]
    async fn an_archived_session_gives_up_its_branches_but_takes_none() {
        let fixture = fixture("handover-archived");
        fixture.branch("left", "old-kid");
        fixture.branch("mine", "lead");
        fixture
            .state
            .store
            .insert_session(&Session {
                project_id: Some("repo".to_string()),
                status: SessionStatus::Archived,
                ..project_session("old-lead-kid", "console-1", Some("lead"))
            })
            .unwrap();
        fixture.branch("left-of-lead", "old-lead-kid");

        fixture
            .call(
                "lead",
                json!({ "branches": ["left-of-lead"], "from": "old-lead-kid" }),
            )
            .await
            .unwrap();
        assert_eq!(fixture.responsible_for("left-of-lead"), "lead");

        let err = fixture
            .call(
                "lead",
                json!({ "branches": ["mine"], "to": "old-lead-kid" }),
            )
            .await
            .expect_err("refused");
        assert!(err.to_string().contains("is archived"), "{err}");
        assert_eq!(fixture.responsible_for("mine"), "lead");
    }

    /// Everything that is refused changes no record.
    #[tokio::test]
    async fn a_refused_handover_changes_nothing() {
        let fixture = fixture("handover-refused");
        fixture.branch("kids", "kid");
        fixture.branch("busy", "busy-kid");
        fixture.branch("waiting", "waiting-kid");
        fixture.branch("prompted", "codex-kid");
        // The record says idle, while the terminal shows Codex's prompt for the user.
        let codex =
            crate::test_support::fake_live_session("codex-kid", Agent::Codex, 80, 24, "sleep 30");
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
                "lead",
                json!({ "branches": ["kids"], "to": "lead-kid" }),
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
                json!({ "branches": ["kids"], "from": "kid", "to": "lead-kid" }),
                "bound to a project session, `lead`",
            ),
            // The shape of the call.
            (
                "a",
                json!({ "branches": ["kids"], "from": "kid", "to": "kid" }),
                "are the same session",
            ),
            (
                "lead",
                json!({ "branches": ["kids"], "to": "lead" }),
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
        let fixture = fixture("handover-caller-working");
        fixture
            .state
            .store
            .insert_session(&Session {
                project_id: Some("repo".to_string()),
                status: SessionStatus::Working,
                ..project_session("busy-lead", "console-1", None)
            })
            .unwrap();
        fixture
            .state
            .store
            .insert_session(&Session {
                project_id: Some("repo".to_string()),
                ..project_session("busy-lead-kid", "console-1", Some("busy-lead"))
            })
            .unwrap();
        fixture.branch("mine", "busy-lead");
        fixture
            .call(
                "busy-lead",
                json!({ "branches": ["mine"], "to": "busy-lead-kid" }),
            )
            .await
            .unwrap();
        assert_eq!(fixture.responsible_for("mine"), "busy-lead-kid");
    }

    /// A worktree with changes in it does not stop the move, and the result names the worktree and
    /// says whether it holds uncommitted changes; a branch with no worktree has neither.
    #[tokio::test]
    async fn the_result_reports_each_worktree_and_whether_it_has_uncommitted_changes() {
        let fixture = fixture("handover-worktrees");
        for branch in ["dirty", "clean", "bare"] {
            fixture.branch(branch, "kid");
        }
        let worktrees = ScratchDir::new("handover-worktrees-checkouts");
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

    /// A session another switch or handoff has claimed is not handed over from or to meanwhile.
    #[tokio::test]
    async fn a_session_that_is_being_switched_is_left_alone() {
        let fixture = fixture("handover-claimed");
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
