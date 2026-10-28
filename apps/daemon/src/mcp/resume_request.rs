//! The tool with which a session asks the user to resume a conversation by its id,
//! `request_resume_session`: the capability the new-session dialog's Resume conversation tab gives
//! the user, with the same limits, behind the user's approval.
//!
//! What is here is the caller's side: reading the arguments, finding where the conversation is
//! held with the lookup the tab asks the daemon for (`coordinator::locate_conversation`), and
//! refusing before the user is asked whatever the approval could never carry out. The asking, the
//! waiting and what an approval does are `crate::session_request`'s, shared with `request_session`.

use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use serde_json::{Map, Value};

use crate::coordinator::{self, OpenRequest};
use crate::protocol::{ConversationLocation, Origin, Project, Role, Session, SessionStatus};
use crate::session_request::{self, Action};
use crate::sharing::{named_session, one_line, place_of, reports_to, ReportsTo};
use crate::state::{AppState, ClaimKind};

use super::args::{find_project, optional_string, own_project, present_text, required_str};

/// Asks the user to resume the conversation `conversation`, and waits for the answer; see
/// `crate::session_request`. The project is the one the call names, else the caller's own, and
/// plays the part of the project whose dialog the tab is in.
pub(super) async fn request_resume_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let action = plan(
        state,
        caller,
        arguments,
        async |project, conversation| {
            coordinator::locate_conversation(state, project, conversation).await
        },
        async |id| coordinator::check_can_resume(state, id).await,
    )
    .await?;
    // Read once, as `request_session` reads its own setting: the call acts on it.
    let automatic = state.store.auto_approve_resume_requests()?;
    session_request::ask_to_approve(
        state,
        caller,
        action,
        automatic,
        session_request::ANSWER_TIME_LIMIT,
    )
    .await
}

/// What approving the call would do, settled from its arguments, or the reason it can never be
/// carried out. `locate` finds where the conversation is held, for a project and an id, and
/// `check_resumable` refuses a session whose conversation its agent no longer has and returns the
/// archived owners that would come back with it; both are `coordinator`'s outside the tests, which
/// cannot read the user's disk and shell.
async fn plan(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
    locate: impl AsyncFnOnce(&str, &str) -> Result<ConversationLocation>,
    check_resumable: impl AsyncFnOnce(&str) -> Result<Vec<String>>,
) -> Result<Action> {
    let project = match optional_string(arguments, "project") {
        Some(wanted) => find_project(state, caller, &wanted)?,
        None if caller.role == Role::Project => own_project(state, caller, None)?,
        None => bail!("`project` is required for a console session"),
    };
    let wanted = required_str(arguments, "conversation")?;
    let title = optional_string(arguments, "title");
    let model = present_text(arguments, "model")?;
    let effort = present_text(arguments, "effort")?;
    // Left blank, a model or effort asks for nothing: a model that fills every field in is not
    // refused for the ones it had nothing to say in.
    let overrides_given = [&model, &effort]
        .into_iter()
        .any(|value| matches!(value, Some(Some(text)) if !text.trim().is_empty()));

    let id = coordinator::parse_conversation_id(wanted)?;
    let location = locate(&project.id, &id).await?;
    let holder = match location {
        ConversationLocation::ArchivedSession { session }
        | ConversationLocation::LiveSession { session } => read_holder(state, &session)?,
        ConversationLocation::OutsideProject { session } => {
            return Err(held_elsewhere(
                state,
                &read_holder(state, &session)?,
                &project,
            ))
        }
        ConversationLocation::Record { agent, account } => {
            return Ok(Action::ResumeRecord {
                open: OpenRequest {
                    console_id: project.console_id.clone(),
                    project_id: Some(project.id.clone()),
                    agent: Some(agent),
                    account: Some(account),
                    model,
                    effort,
                    task: None,
                    // Left out, the session is named for its project, as the daemon names one
                    // opened with no title.
                    title,
                    // The user's own, as a session of the Resume tab: nobody owns it but them.
                    origin: Origin::User,
                    bound_to: None,
                    resume_agent_session_id: Some(id.clone()),
                    session_id: None,
                    terminal_appearance: None,
                },
                conversation: id,
            });
        }
        ConversationLocation::NotFound => bail!(
            "no conversation `{id}` was found: no session of Octoboard holds it, and no agent that \
             can be started here has a record of it in any of its accounts"
        ),
    };

    // As the session stands now, which is what decides what an approval would do.
    let archived = holder.status == SessionStatus::Archived;
    let named = named_session(&holder);
    if holder.id == caller.id {
        bail!("this conversation is your own: {named} is the session you are in");
    }
    if holder.bound_to.as_deref() == Some(&caller.id) {
        bail!(
            "{named} holds this conversation and is yours, so it needs no approval: reopen it with \
             `reopen_session` while it is archived or interrupted, and instruct it with \
             `instruct_session` while it runs"
        );
    }
    if !holder.status.is_dormant() {
        bail!(
            "{named} holds this conversation and is running, so there is nothing to resume. It \
             reports to {}; `send_message` tells it something",
            reports_to_words(state, &holder)
        );
    }
    match state.claim_kind(&holder.id) {
        Some(ClaimKind::Switch) => bail!(
            "{named} is being switched to another account, handed off, rebound or having its \
             branches handed over right now: ask again in a moment"
        ),
        Some(ClaimKind::Launch) => {
            bail!("{named} is being launched right now: ask again in a moment")
        }
        None => {}
    }
    if overrides_given {
        bail!(
            "`model` and `effort` are for a conversation no session holds, which opens a new \
             session; {named} holds this one and keeps its own launch settings"
        );
    }
    let rename = if archived {
        // A name that is the session's own changes nothing, so it is not asked about.
        title.filter(|title| title != &holder.title)
    } else if title.is_some() {
        bail!(
            "`title` cannot rename {named}: it is interrupted, and an interrupted session keeps \
             its name"
        );
    } else {
        None
    };
    let reopened_owners = check_resumable(&holder.id).await?;
    Ok(Action::Resume {
        target: holder.id,
        archived,
        owner: holder.bound_to,
        reopened_owners,
        rename,
    })
}

/// The session a lookup named, as it stands now.
fn read_holder(state: &Arc<AppState>, id: &str) -> Result<Session> {
    state.store.get_session(id)?.ok_or_else(|| {
        anyhow!("the session `{id}` that holds this conversation is gone: ask again")
    })
}

/// Who `session` reports to, as a sentence part.
fn reports_to_words(state: &Arc<AppState>, session: &Session) -> String {
    match reports_to(state, session) {
        ReportsTo::Session(owner) => named_session(&owner),
        ReportsTo::Unreadable => "its owner".to_string(),
        ReportsTo::User => "the user".to_string(),
    }
}

/// The refusal of a conversation held by a session outside `asked`, the project the call is about:
/// a project's other session can be asked for with that project, a console session cannot be
/// resumed from any project.
fn held_elsewhere(state: &Arc<AppState>, holder: &Session, asked: &Project) -> anyhow::Error {
    let named = named_session(holder);
    let place = place_of(state, holder);
    match &holder.project_id {
        Some(project) if holder.role == Role::Project => anyhow!(
            "{named}{place} holds this conversation, and it is not in the project \"{}\" you asked \
             in. Ask again with `project` set to `{project}` to resume it there",
            one_line(&asked.name)
        ),
        _ => anyhow!(
            "{named}{place} holds this conversation, and a console session cannot be resumed this \
             way"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::exec::tests::{arguments, shared_console_state};
    use crate::protocol::Agent;
    use serde_json::json;

    const ID: &str = "11111111-2222-3333-4444-555555555555";

    /// What a lookup answers, whatever it is asked.
    fn found(
        location: ConversationLocation,
    ) -> impl AsyncFnOnce(&str, &str) -> Result<ConversationLocation> {
        async move |_, _| Ok(location)
    }

    /// A check that finds the conversation resumable and the owners `owners` to come back with it.
    fn resumable(owners: &[&str]) -> impl AsyncFnOnce(&str) -> Result<Vec<String>> {
        let owners: Vec<String> = owners.iter().map(|owner| owner.to_string()).collect();
        async move |_| Ok(owners)
    }

    async fn planned(
        state: &Arc<AppState>,
        caller: &str,
        args: Value,
        location: ConversationLocation,
    ) -> Result<Action> {
        let caller = state.session_record(caller).unwrap();
        plan(
            state,
            &caller,
            &arguments(args),
            found(location),
            resumable(&[]),
        )
        .await
    }

    fn held_by(session: &str, archived: bool) -> ConversationLocation {
        let session = session.to_string();
        match archived {
            true => ConversationLocation::ArchivedSession { session },
            false => ConversationLocation::LiveSession { session },
        }
    }

    fn make(state: &Arc<AppState>, id: &str, status: SessionStatus) {
        let mut session = state.store.get_session(id).unwrap().unwrap();
        session.status = status;
        state.store.delete_session(id).unwrap();
        state.store.insert_session(&session).unwrap();
    }

    fn refusal(result: Result<Action>) -> String {
        match result {
            Ok(_) => panic!("expected a refusal"),
            Err(err) => err.to_string(),
        }
    }

    /// What the tab cannot do the call cannot: a conversation held outside the project is
    /// refused, naming the holder and the project to ask in, and a console session's cannot be
    /// resumed at all; one that nobody holds is refused as not found.
    #[tokio::test]
    async fn a_conversation_the_tab_could_not_resume_is_refused_before_asking() {
        let (state, _dir) = shared_console_state("resume-request-elsewhere");
        let args = json!({ "conversation": ID });

        let outside = ConversationLocation::OutsideProject {
            session: "foreign".to_string(),
        };
        let reason = refusal(planned(&state, "loose", args.clone(), outside).await);
        assert!(
            reason.contains("`foreign`") && reason.contains("`project-2`"),
            "{reason}"
        );

        let console = ConversationLocation::OutsideProject {
            session: "a".to_string(),
        };
        let reason = refusal(planned(&state, "loose", args.clone(), console).await);
        assert!(
            reason.contains("console session cannot be resumed"),
            "{reason}"
        );

        let reason = refusal(planned(&state, "loose", args, ConversationLocation::NotFound).await);
        assert!(reason.contains("no conversation"), "{reason}");
    }

    /// The project is the caller's own unless it names one, a console session has to name one, and
    /// the id has to be a conversation id.
    #[tokio::test]
    async fn the_project_and_the_conversation_id_are_checked() {
        let (state, _dir) = shared_console_state("resume-request-arguments");
        let reason = refusal(
            planned(
                &state,
                "a",
                json!({ "conversation": ID }),
                ConversationLocation::NotFound,
            )
            .await,
        );
        assert!(reason.contains("`project` is required"), "{reason}");
        let reason = refusal(
            planned(
                &state,
                "loose",
                json!({ "conversation": ID, "project": "nowhere" }),
                ConversationLocation::NotFound,
            )
            .await,
        );
        assert!(reason.contains("no project `nowhere`"), "{reason}");
        let reason = refusal(
            planned(
                &state,
                "loose",
                json!({ "conversation": "my-title" }),
                ConversationLocation::NotFound,
            )
            .await,
        );
        assert!(reason.contains("not a conversation id"), "{reason}");
    }

    /// A conversation held by a session that is the caller's, the caller's own, or running has
    /// nothing to approve; each is refused saying what to do instead.
    #[tokio::test]
    async fn a_session_that_needs_no_resuming_or_no_approval_is_refused() {
        let (state, _dir) = shared_console_state("resume-request-held");
        let args = json!({ "conversation": ID });

        let reason = refusal(planned(&state, "loose", args.clone(), held_by("loose", false)).await);
        assert!(reason.contains("your own"), "{reason}");

        // Archived and bound to the caller: `reopen_session` needs no approval.
        let reason = refusal(
            planned(
                &state,
                "a",
                json!({ "conversation": ID, "project": "project-1" }),
                held_by("archived-of-a", true),
            )
            .await,
        );
        assert!(reason.contains("`reopen_session`"), "{reason}");

        // Running, owned by somebody else: nothing to resume, and who it reports to is named.
        let reason = refusal(planned(&state, "loose", args.clone(), held_by("of-a", false)).await);
        assert!(
            reason.contains("running") && reason.contains("`a`"),
            "{reason}"
        );

        // Starting counts as running.
        make(&state, "of-b", SessionStatus::Starting);
        let reason = refusal(planned(&state, "loose", args, held_by("of-b", false)).await);
        assert!(reason.contains("nothing to resume"), "{reason}");
    }

    /// A holder whose claim is held for a switch, a handoff, a handover of branches or a binding
    /// change is refused as being switched, naming the handover too; one held for a launch is
    /// refused as being launched.
    #[tokio::test]
    async fn a_holder_whose_claim_is_held_is_refused_by_the_kind_of_claim() {
        let (state, _dir) = shared_console_state("resume-request-claimed");
        let args = json!({ "conversation": ID });

        let claim = state.begin_switch("archived-of-a").unwrap();
        let reason = refusal(
            planned(
                &state,
                "loose",
                args.clone(),
                held_by("archived-of-a", true),
            )
            .await,
        );
        assert!(
            reason.contains("being switched")
                && reason.contains("handed off")
                && reason.contains("branches handed over"),
            "{reason}"
        );
        drop(claim);

        let _claim = state.begin_launch("archived-of-a").unwrap();
        let reason = refusal(planned(&state, "loose", args, held_by("archived-of-a", true)).await);
        assert!(reason.contains("being launched"), "{reason}");
    }

    /// An archived session is reopened with the name asked for when it differs, along with the
    /// owners the check says come back; an interrupted one is resumed and keeps its name.
    #[tokio::test]
    async fn a_dormant_holder_is_resumed_in_place() {
        let (state, _dir) = shared_console_state("resume-request-dormant");
        let args =
            |title: Option<&str>| json!({ "conversation": ID.to_uppercase(), "title": title });

        let caller = state.session_record("loose").unwrap();
        let action = plan(
            &state,
            &caller,
            &arguments(args(Some("Refund flow"))),
            found(held_by("archived-of-a", true)),
            resumable(&["a"]),
        )
        .await
        .unwrap();
        let Action::Resume {
            target,
            archived,
            owner,
            reopened_owners,
            rename,
        } = action
        else {
            panic!("expected a resume");
        };
        assert_eq!(
            (
                target.as_str(),
                archived,
                owner.as_deref(),
                reopened_owners,
                rename.as_deref()
            ),
            (
                "archived-of-a",
                true,
                Some("a"),
                vec!["a".to_string()],
                Some("Refund flow")
            )
        );

        // A name that is its own changes nothing.
        let own = state.session_record("archived-of-a").unwrap().title;
        let action = planned(
            &state,
            "loose",
            args(Some(&own)),
            held_by("archived-of-a", true),
        )
        .await
        .unwrap();
        assert!(matches!(action, Action::Resume { rename: None, .. }));

        // Interrupted: resumed, and a name is refused.
        make(&state, "archived-of-a", SessionStatus::Interrupted);
        let action = planned(&state, "loose", args(None), held_by("archived-of-a", false))
            .await
            .unwrap();
        assert!(matches!(
            action,
            Action::Resume {
                archived: false,
                rename: None,
                ..
            }
        ));
        let reason = refusal(
            planned(
                &state,
                "loose",
                args(Some("Refund flow")),
                held_by("archived-of-a", false),
            )
            .await,
        );
        assert!(reason.contains("keeps its name"), "{reason}");
    }

    /// A session that keeps its launch settings is not given others, and one whose launch is under
    /// way is not resumed meanwhile; a model or effort left blank is no override.
    #[tokio::test]
    async fn a_holder_refuses_launch_settings_and_a_launch_under_way() {
        let (state, _dir) = shared_console_state("resume-request-settings");
        for key in ["model", "effort"] {
            let reason = refusal(
                planned(
                    &state,
                    "loose",
                    json!({ "conversation": ID, key: "x" }),
                    held_by("archived-of-a", true),
                )
                .await,
            );
            assert!(reason.contains("keeps its own launch settings"), "{reason}");
        }
        let blank = planned(
            &state,
            "loose",
            json!({ "conversation": ID, "model": " ", "effort": "" }),
            held_by("archived-of-a", true),
        )
        .await;
        assert!(blank.is_ok());

        let claim = state.begin_launch("archived-of-a").unwrap();
        let reason = refusal(
            planned(
                &state,
                "loose",
                json!({ "conversation": ID }),
                held_by("archived-of-a", true),
            )
            .await,
        );
        assert!(reason.contains("being launched"), "{reason}");
        drop(claim);
    }

    /// A refusal of the check (the agent no longer has the conversation) stops the call before
    /// the user is asked.
    #[tokio::test]
    async fn an_unresumable_conversation_is_refused_before_asking() {
        let (state, _dir) = shared_console_state("resume-request-unresumable");
        let caller = state.session_record("loose").unwrap();
        let result = plan(
            &state,
            &caller,
            &arguments(json!({ "conversation": ID })),
            found(held_by("archived-of-a", true)),
            async |_: &str| -> Result<Vec<String>> { bail!("Claude Code no longer has it") },
        )
        .await;
        assert!(refusal(result).contains("no longer has it"));
    }

    /// A conversation only an agent's own record holds opens a new session: the agent and account
    /// that hold it, the user's own origin, no owner, the title and launch settings as given.
    #[tokio::test]
    async fn a_record_opens_an_unbound_session_on_the_account_that_holds_it() {
        let (state, _dir) = shared_console_state("resume-request-record");
        let location = ConversationLocation::Record {
            agent: Agent::Codex,
            account: Some("work".to_string()),
        };
        let action = planned(
            &state,
            "loose",
            json!({
                "conversation": ID.to_uppercase(),
                "title": "Refunds",
                "model": "gpt-x",
                "effort": "high",
            }),
            location,
        )
        .await
        .unwrap();
        let Action::ResumeRecord { open, conversation } = action else {
            panic!("expected a record");
        };
        assert_eq!(conversation, ID);
        assert_eq!(
            (
                open.console_id.as_str(),
                open.project_id.as_deref(),
                open.agent,
                open.account.clone()
            ),
            (
                "console-1",
                Some("project-1"),
                Some(Agent::Codex),
                Some(Some("work".to_string()))
            )
        );
        assert_eq!(
            (
                open.title.as_deref(),
                open.model.clone(),
                open.effort.clone(),
                open.origin,
                open.bound_to.is_none(),
                open.resume_agent_session_id.as_deref(),
                open.task.is_none()
            ),
            (
                Some("Refunds"),
                Some(Some("gpt-x".to_string())),
                Some(Some("high".to_string())),
                Origin::User,
                true,
                Some(ID),
                true
            )
        );
    }

    /// The tool is reachable through the dispatch every call takes, which reads its arguments.
    #[tokio::test]
    async fn the_tool_is_dispatched_and_needs_a_conversation() {
        let (state, _dir) = shared_console_state("resume-request-dispatch");
        let err = crate::mcp::exec::call(
            &state,
            "loose",
            "request_resume_session",
            &arguments(json!({})),
        )
        .await
        .expect_err("refused");
        assert!(
            err.to_string().contains("`conversation` is required"),
            "{err}"
        );
    }
}
