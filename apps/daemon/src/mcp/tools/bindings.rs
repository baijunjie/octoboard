//! The tools that change who a session reports to: `transfer_session`, with which an owner hands a
//! session of its own to another, `release_session`, with which it lets one go, and
//! `bind_to_session`, with which an unbound project session binds itself to an owner. What may own
//! what is `coordinator::bindings::owner_refusal`'s, and the notices that tell a session its owner
//! changed are [`crate::messaging::owner_notices`]'s; what is here is reading each tool's
//! arguments and telling the caller what became of each session concerned.

use std::sync::Arc;

use anyhow::Result;
use serde_json::{json, Map, Value};

use crate::coordinator::{self, bindings::Released};
use crate::messaging::delivery::{self, Delivery, SentBy};
use crate::messaging::naming::named_session;
use crate::messaging::owner_notices::{
    duties_under_new_parent, first_report_on_bind, tell_now_or_leave, tell_released,
    transferred_to_owner, transferred_to_you,
};
use crate::messaging::reporting::{self, ReportStatus};
use crate::messaging::texts::capitalised;
use crate::protocol::{OrchestrationRole, Session};
use crate::session_tools::catalogue::qualified_tool_name;
use crate::state::AppState;

use crate::mcp::lookup::{optional_string, required_str, resolve_owned_session};

/// The most characters of the parting note of a `release_session`.
const MAX_NOTE_CHARS: usize = 4000;

/// What became of `what`, a notice to `to`, for the sender's result. `None` is a session with no
/// process, which is told when it next starts.
fn told(delivery: &Result<Option<Delivery>>, what: &str, to: &str) -> String {
    match delivery {
        Ok(Some(delivery)) => delivery.note(what, to, SentBy::Octoboard),
        Ok(None) => format!(
            "{} is not running, and will be told when it next starts.",
            capitalised(to)
        ),
        Err(err) => format!("{} could not be told ({err:#}).", capitalised(to)),
    }
}

/// Hands a session of the caller's to another owner; see `coordinator::bindings::transfer_session`
/// for what may own what. The session is told who it now reports to, the new owner is told it has
/// the session with the caller's note, and the caller learns from the result what became of both.
pub(in crate::mcp) async fn transfer_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let session = resolve_owned_session(state, caller, required_str(arguments, "session")?)?;
    let to = required_str(arguments, "to")?;
    let note = required_str(arguments, "note")?.to_string();
    let (moved, owner, running) =
        coordinator::bindings::transfer_session(state, &session.id, &caller.id, to)?;

    let to_session = transferred_to_you(caller, &owner, &moved, session.is_worker());
    let to_owner = transferred_to_owner(state, caller, &owner, &moved, &note);
    // Neither message credits the turn it starts: if the moved session or its new owner carries on
    // and stops without reporting, the report Octoboard synthesises is the safety net, and it is
    // wanted there.
    let child = tell_now_or_leave(state, &moved.id, to_session, running, None).await;
    let told_owner = delivery::write_notice_to_owner_off_runtime(state, &owner, &moved, to_owner)
        .await
        .map(Some);
    let mut result_note = format!(
        "Transferred: `{id}` now reports to {to} and is no longer yours. {child} {owner}",
        id = moved.id,
        to = named_session(&owner),
        child = told(&child, "The notice of the transfer", "the session"),
        owner = told(
            &told_owner,
            "The notice of the transfer",
            "its new parent session"
        ),
    );
    if told_owner.is_err() {
        result_note.push_str(&format!(
            " Tell the new parent session of the transfer yourself with `{}`.",
            qualified_tool_name(caller.agent, "inform_session")
        ));
    }
    Ok(json!({
        "session": moved.id,
        "owner": owner.id,
        "owner_kind": owner.role.kind(),
        "orchestration_role": moved.orchestration_role,
        "note": result_note,
    }))
}

/// Releases a session of the caller's from its binding; see
/// `coordinator::bindings::release_session`. The session is told it has no owner now, with the
/// caller's `note` if it gave one, and the caller learns from the result what became of it.
pub(in crate::mcp) async fn release_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let session = resolve_owned_session(state, caller, required_str(arguments, "session")?)?;
    // Normalised here, once: trimmed, and none when blank.
    let note = optional_string(arguments, "note");
    if note
        .as_ref()
        .is_some_and(|note| note.chars().count() > MAX_NOTE_CHARS)
    {
        anyhow::bail!("`note` is longer than {MAX_NOTE_CHARS} characters; shorten it");
    }
    let Released {
        session: released,
        running,
        ..
    } = coordinator::bindings::release_session(
        state,
        &session.id,
        Some(&caller.id),
        note.as_deref(),
    )?;
    let delivery = tell_released(state, &released, running, note.as_deref()).await;
    Ok(json!({
        "session": released.id,
        "orchestration_role": released.orchestration_role,
        "note": format!(
            "Released: `{id}` has no parent session now and is no longer yours. It is the \
             user's own session; you no longer instruct or drive it and it does not report to \
             you, though `{inform_session}` can still tell it something. {told}",
            id = released.id,
            inform_session = qualified_tool_name(caller.agent, "inform_session"),
            told = told(&delivery, "The notice of the release", "the session"),
        ),
    }))
}

/// Binds the unbound caller to an owner; see `coordinator::bindings::bind_session` for what may own
/// what. The owner is sent the caller's summary as its first report, and the caller is told in the
/// result what it now is and which tools it has: its role description, recorded when it was
/// launched unbound, says otherwise.
pub(in crate::mcp) async fn bind_to_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let owner_id = required_str(arguments, "owner")?;
    let summary = required_str(arguments, "summary")?;
    let (bound, owner) = coordinator::bindings::bind_session(state, &caller.id, owner_id)?;
    let coordinator = bound.orchestration_role == OrchestrationRole::Coordinator;

    let first_report = first_report_on_bind(coordinator, summary);
    // The binding is this round's report, so the round ending without another is not reported on
    // a second time.
    let delivered = reporting::deliver_own_report(
        state,
        &bound.id,
        first_report,
        ReportStatus::NeedsDecision,
        Vec::new(),
    )
    .await;
    let tool = |name: &str| qualified_tool_name(bound.agent, name);
    // Nothing after the binding fails the call: the session is bound by now, so a first report
    // that was not delivered, for whatever reason, is the session's to send.
    let first = first_report_note(&delivered, &tool("report"));
    let note = format!(
        "Your role is {role}. You now report to {owner}, your parent session: {duties} {first} \
         You have a parent session now, so do not call `{bind}` again.",
        owner = named_session(&owner),
        role = if coordinator { "coordinator" } else { "worker" },
        duties = duties_under_new_parent(bound.agent, coordinator),
        bind = tool("bind_to_session"),
    );
    Ok(json!({
        "owner": owner.id,
        "owner_kind": owner.role.kind(),
        "orchestration_role": bound.orchestration_role,
        "note": note,
    }))
}

/// What the result of `bind_to_session` says of the caller's summary, delivered to its new owner as
/// its first report; `report` is the tool by which the caller sends one itself.
fn first_report_note(delivered: &Result<(String, Delivery)>, report: &str) -> String {
    match delivered {
        Ok((_, delivery)) => delivery.note(
            "Your summary, as your first report,",
            "your new parent session",
            SentBy::Reader,
        ),
        Err(err) => format!(
            "Your summary could not be delivered to your new parent session ({err:#}), so send it \
             yourself with `{report}`."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::dispatch::call;
    use crate::mcp::test_support::{arguments, peer_state};
    use crate::protocol::Agent;
    use crate::test_support::{fake_live_session, heard_after_marker, output_containing, running};

    /// A transfer moves the session to its new owner, tells it whose it now is, and sends the new
    /// owner the note, quoted; the session is the new owner's to drive from then on, not the old
    /// one's.
    #[tokio::test]
    async fn a_transfer_tells_the_session_and_its_new_owner() {
        let (state, _dir) = peer_state("transfer");
        let of_a = running(&state, "of-a");
        let _a = running(&state, "a");
        let b = running(&state, "b");
        let result = call(
            &state,
            "a",
            "transfer_session",
            &arguments(json!({
                "session": "of-a",
                "to": "b",
                "note": "Half of the endpoints are done.",
            })),
        )
        .await
        .unwrap();
        assert_eq!(result["owner"], "b");
        assert_eq!(result["orchestration_role"], "worker");
        assert!(result["note"].as_str().unwrap().contains("no longer yours"));
        assert_eq!(
            state
                .store
                .get_session("of-a")
                .unwrap()
                .unwrap()
                .bound_to
                .as_deref(),
            Some("b")
        );
        let told = output_containing(&of_a, "your parent session has changed");
        assert!(
            told.contains(" Your role remains worker: you cannot create or manage child sessions."),
            "{told}"
        );
        let handed = output_containing(&b, "Half of the endpoints");
        assert!(
            handed.contains("> Half of the endpoints are done."),
            "{handed}"
        );

        let err = call(
            &state,
            "a",
            "instruct_session",
            &arguments(json!({ "session": "of-a", "text": "hello" })),
        )
        .await
        .expect_err("no longer a's");
        assert!(
            err.to_string()
                .contains("bound to another console session, `b`"),
            "{err}"
        );
    }

    #[tokio::test]
    async fn an_empty_coordinators_role_follows_binding_and_transfer() {
        let (state, _dir) = peer_state("coordinator-role-transfer");
        let _a = running(&state, "a");
        let _b = running(&state, "b");
        let _starter = running(&state, "starter");
        let loose = running(&state, "loose");
        let bound = call(
            &state,
            "loose",
            "bind_to_session",
            &arguments(json!({
                "owner": "a", "summary": "Ready to coordinate."
            })),
        )
        .await
        .unwrap();
        assert_eq!(bound["orchestration_role"], "coordinator");
        assert_eq!(
            bound["note"],
            "Your role is coordinator. You now report to the console session `a` (\"Hub\"), your \
             parent session: take work only from it, or from the user typing in this terminal, \
             and report to it with `mcp__octoboard__report` once per round of work. This \
             replaces anything your role description says about being independent and not \
             calling `mcp__octoboard__report`. `done` with no open items archives you, and is \
             refused while a session you started is not archived; `needs_decision` or open items \
             leave you running. You keep your child sessions and go on driving them with \
             `mcp__octoboard__open_session` and the tools that go with it. Your summary, as your \
             first report, was delivered to your new parent session. You have a parent session \
             now, so do not call `mcp__octoboard__bind_to_session` again."
        );
        for (caller, parent, expected, role) in [
            (
                "a",
                "b",
                "coordinator",
                " Your role remains coordinator: you can create and manage child sessions.",
            ),
            (
                "b",
                "starter",
                "worker",
                " Your role is worker. You can no longer start or drive sessions",
            ),
        ] {
            let moved = call(
                &state,
                caller,
                "transfer_session",
                &arguments(json!({
                    "session": "loose", "to": parent, "note": "Continue the task."
                })),
            )
            .await
            .unwrap();
            assert_eq!(moved["orchestration_role"], expected);
            let told = output_containing(
                &loose,
                &format!(
                    "transferred you to the {}",
                    if parent == "starter" {
                        "project session `starter`"
                    } else {
                        "console session `b`"
                    }
                ),
            );
            assert!(told.contains(role), "{told}");
            let record = call(&state, "loose", "get_session", &Map::new())
                .await
                .unwrap();
            assert_eq!(record["orchestration_role"], expected);
            assert_eq!(record["owner"], parent);
            assert_eq!(record["owned_sessions"], 0);
            let wire = state.wire_session(state.session_record("loose").unwrap());
            assert_eq!(json!(wire.orchestration_role), expected);
        }
    }

    /// A release unbinds the session and tells it, in a message, that it has no owner and which
    /// tools it has; it is the user's own afterwards, so its former owner no longer drives it, and
    /// only the owner's own sessions can be released.
    #[tokio::test]
    async fn a_release_tells_the_session_it_has_no_owner() {
        let (state, _dir) = peer_state("release");
        let of_a = running(&state, "of-a");
        let a = running(&state, "a");
        let _starter = running(&state, "starter");
        let _mine = running(&state, "mine");

        let err = call(
            &state,
            "a",
            "release_session",
            &arguments(json!({ "session": "mine" })),
        )
        .await
        .expect_err("not a's");
        assert!(
            err.to_string().contains("bound to a project session"),
            "{err}"
        );

        let result = call(
            &state,
            "a",
            "release_session",
            &arguments(json!({ "session": "of-a", "note": "  " })),
        )
        .await
        .unwrap();
        assert_eq!(result["session"], "of-a");
        assert_eq!(result["orchestration_role"], "coordinator");
        let note = result["note"].as_str().unwrap();
        assert!(note.contains("no longer yours"));
        assert!(
            note.contains(&qualified_tool_name(Agent::Claude, "inform_session")),
            "{note}"
        );
        let released = state.store.get_session("of-a").unwrap().unwrap();
        assert_eq!(
            state.wire_session(released.clone()).orchestration_role,
            OrchestrationRole::Coordinator
        );
        assert_eq!(released.bound_to, None);
        let told = output_containing(&of_a, "you have no parent session now");
        assert!(told.contains("`mcp__octoboard__open_session`"), "{told}");
        assert!(
            told.contains("do not call `mcp__octoboard__report`"),
            "{told}"
        );
        assert!(
            told.contains(
                "tell your former parent session something with `mcp__octoboard__inform_session`"
            ),
            "{told}"
        );
        assert!(
            told.contains("call `mcp__octoboard__get_session` once with no arguments first,"),
            "{told}"
        );
        // A blank note adds nothing, and the owner that released it is not told what it did.
        assert!(!told.contains("former parent session left"), "{told}");
        assert!(!heard_after_marker(&state, &a).contains("released"));

        let err = call(
            &state,
            "a",
            "instruct_session",
            &arguments(json!({ "session": "of-a", "text": "hello" })),
        )
        .await
        .expect_err("nobody's now");
        assert!(err.to_string().contains("not yours"), "{err}");
    }

    /// A note given with a release reaches the released session, quoted and marked as its former
    /// owner's, in the message that tells it it has no owner now.
    #[tokio::test]
    async fn a_release_passes_the_owners_note_on() {
        let (state, _dir) = peer_state("release-note");
        state
            .store
            .set_binding(
                "other-project",
                None,
                "a",
                OrchestrationRole::Worker,
                false,
                false,
            )
            .unwrap()
            .unwrap();
        // A terminal holds no more than 1024 bytes of input that nothing reads, which this message
        // is longer than: this stand-in reads it all.
        let released = fake_live_session(
            "other-project",
            Agent::Claude,
            80,
            24,
            "stty -icanon; echo ready; cat",
        );
        state.register_live(released.clone());
        crate::session::spawn_reader_thread(released.clone(), 8 * 1024);
        output_containing(&released, "ready");
        let _a = running(&state, "a");

        call(
            &state,
            "a",
            "release_session",
            &arguments(
                json!({ "session": "other-project", "note": " Finish the retries.\nThen stop. " }),
            ),
        )
        .await
        .unwrap();
        let told = output_containing(&released, "Then stop.");
        assert!(told.contains("you have no parent session now"), "{told}");
        assert!(
            told.contains("Your former parent session left this note:"),
            "{told}"
        );
        assert!(told.contains("> Finish the retries."), "{told}");
        assert!(told.contains("> Then stop."), "{told}");
        // Told in the message, so nothing is left owed.
        assert_eq!(
            state.store.owner_notice_pending("other-project").unwrap(),
            0
        );
        assert_eq!(
            state.store.owner_notice_note("other-project").unwrap(),
            None
        );
    }

    /// A note over the cap is refused in prose before anything changes; one at the cap goes
    /// through.
    #[tokio::test]
    async fn a_release_refuses_a_note_that_is_too_long() {
        let (state, _dir) = peer_state("release-note-cap");
        let _a = running(&state, "a");
        let _of_a = running(&state, "of-a");

        let too_long = "x".repeat(MAX_NOTE_CHARS + 1);
        let err = call(
            &state,
            "a",
            "release_session",
            &arguments(json!({ "session": "of-a", "note": too_long })),
        )
        .await
        .expect_err("too long");
        assert!(
            err.to_string()
                .contains(&format!("{MAX_NOTE_CHARS} characters")),
            "{err}"
        );
        let session = state.store.get_session("of-a").unwrap().unwrap();
        assert_eq!(session.bound_to.as_deref(), Some("a"));

        let at_cap = "x".repeat(MAX_NOTE_CHARS);
        call(
            &state,
            "a",
            "release_session",
            &arguments(json!({ "session": "of-a", "note": at_cap })),
        )
        .await
        .unwrap();
        assert_eq!(
            state.store.get_session("of-a").unwrap().unwrap().bound_to,
            None
        );
    }

    /// A session that binds itself to a project session becomes an ordinary bound one: the owner
    /// gets its summary as its first report, its result says which tools it has left, and the
    /// ones it had for owning sessions are refused from then on.
    #[tokio::test]
    async fn binding_to_a_project_session_takes_the_tools_for_owning_sessions_away() {
        let (state, _dir) = peer_state("bind-to-project");
        let _loose = running(&state, "loose");
        let starter = running(&state, "starter");
        call(
            &state,
            "loose",
            "show_page",
            &arguments(json!({ "html": "<p>plan</p>" })),
        )
        .await
        .unwrap();
        assert_eq!(state.store.list_pages("loose").unwrap().len(), 1);
        let mut events = state.subscribe();
        let result = call(
            &state,
            "loose",
            "bind_to_session",
            &arguments(json!({ "owner": "starter", "summary": "I am auditing the retries." })),
        )
        .await
        .unwrap();
        assert_eq!(result["orchestration_role"], "worker");
        let note = result["note"].as_str().unwrap();
        assert!(
            note.contains("can no longer start or drive sessions"),
            "{note}"
        );
        assert!(
            note.starts_with(
                "Your role is worker. You now report to the project session `starter` (\"Work \
                 starter\"), your parent session: take work only from it, or from the user typing \
                 in this terminal, and report to it with `mcp__octoboard__report` once per round \
                 of work. This replaces anything your role description says about being \
                 independent and not calling `mcp__octoboard__report`. `done` with no open items \
                 archives you; `needs_decision` or open items leave you running. You can no \
                 longer start or drive sessions"
            ),
            "{note}"
        );
        assert!(note.contains("`mcp__octoboard__report`"), "{note}");
        assert!(note.contains("pages you pushed"), "{note}");
        // Its pages are withdrawn with the tools for them, and the panels are told.
        assert!(state.store.list_pages("loose").unwrap().is_empty());
        let mut withdrawn = Vec::new();
        while let Ok(event) = events.try_recv() {
            if let crate::protocol::Event::PagesDeleted { session_id, pages } = event {
                assert_eq!(session_id, "loose");
                withdrawn.extend(pages);
            }
        }
        assert_eq!(withdrawn.len(), 1);
        let report = output_containing(&starter, "auditing the retries");
        assert!(report.contains("Report from session loose"), "{report}");

        let err = call(
            &state,
            "loose",
            "open_session",
            &arguments(json!({ "brief": { "goal": "x" } })),
        )
        .await
        .expect_err("no longer offered");
        assert!(err.to_string().contains("is not a tool"), "{err}");
    }

    /// What the caller of a change of owner is told became of each notice, and of its summary
    /// sent as its first report, word for word.
    #[test]
    fn the_caller_is_told_what_became_of_each_notice_word_for_word() {
        let notice = |delivery| told(&delivery, "The notice of the transfer", "the session");
        assert_eq!(
            notice(Ok(Some(Delivery::Written))),
            "The notice of the transfer was delivered to the session."
        );
        assert_eq!(
            notice(Ok(Some(Delivery::Queued))),
            "The notice of the transfer is queued: the session cannot take a message right now. \
             It will be delivered as soon as the session can take one."
        );
        assert_eq!(
            notice(Ok(Some(Delivery::Kept))),
            "The notice of the transfer is kept for the session, which is interrupted with no \
             process running: it will be delivered once the session is resumed."
        );
        assert_eq!(
            notice(Ok(Some(Delivery::Unsent))),
            "The notice of the transfer was written into the input of the session, but the \
             session's agent did not send it to the conversation (it is in the input box, or went \
             to a view of its background agents), so the session has not read it. The user has \
             been told to check the session."
        );
        assert_eq!(
            notice(Ok(None)),
            "The session is not running, and will be told when it next starts."
        );
        assert_eq!(
            notice(Err(anyhow::anyhow!("the pipe broke"))),
            "The session could not be told (the pipe broke)."
        );

        let first = |delivery: Delivery| {
            first_report_note(&Ok((String::new(), delivery)), "mcp__octoboard__report")
        };
        assert_eq!(
            first(Delivery::Written),
            "Your summary, as your first report, was delivered to your new parent session."
        );
        assert_eq!(
            first(Delivery::Queued),
            "Your summary, as your first report, is queued: your new parent session cannot take a \
             message right now. It will be delivered as soon as your new parent session can take \
             one, so do not send it again."
        );
        assert_eq!(
            first(Delivery::Unsent),
            "Your summary, as your first report, was written into the input of your new parent \
             session, but your new parent session's agent did not send it to the conversation (it \
             is in the input box, or went to a view of its background agents), so your new parent \
             session has not read it. The user has been told to check your new parent session; do \
             not send it again."
        );
        assert_eq!(
            first(Delivery::Kept),
            "Your summary, as your first report, is kept for your new parent session, which is \
             interrupted with no process running: it will be delivered once your new parent \
             session is resumed, so do not send it again."
        );
        assert_eq!(
            first_report_note(
                &Err(anyhow::anyhow!("the pipe broke")),
                "mcp__octoboard__report"
            ),
            "Your summary could not be delivered to your new parent session (the pipe broke), so \
             send it yourself with `mcp__octoboard__report`."
        );
    }
}
