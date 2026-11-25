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
    duties_under_new_parent, first_report_on_bind, tell_new_owner, tell_now_or_leave,
    tell_released, transferred_to_owner, transferred_to_you,
};
use crate::messaging::reporting::{self, ReportStatus};
use crate::messaging::texts::capitalised;
use crate::protocol::{Role, Session};
use crate::session_tools::catalogue::qualified_tool_name;
use crate::state::AppState;

use crate::mcp::lookup::{optional_string, required_str, resolve_owned_session};

/// The most characters of the parting note of a `release_session`.
const MAX_NOTE_CHARS: usize = 4000;

/// What became of `what`, a notice to `to`, for the sender's result. `None` is a session left to
/// be told when it next starts: one with no process (`running` false), or one whose notice could
/// not be written now.
fn told(delivery: &Result<Option<Delivery>>, running: bool, what: &str, to: &str) -> String {
    match delivery {
        Ok(Some(delivery)) => delivery.note(what, to, SentBy::Octoboard),
        Ok(None) if !running => format!(
            "{} is not running, and will be told when it next starts.",
            capitalised(to)
        ),
        Ok(None) => format!(
            "{} could not be told now, and will be told when it next starts.",
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
    // Neither message credits the turn it starts: if the moved session or its new owner carries on
    // and stops without reporting, the report Octoboard synthesises is the safety net, and it is
    // wanted there.
    let child = tell_now_or_leave(state, &moved.id, to_session, running, None).await;
    let text = {
        let (state, caller, moved, note) =
            (state.clone(), caller.clone(), moved.clone(), note.clone());
        move |owner: &Session| transferred_to_owner(&state, &caller, owner, &moved, &note)
    };
    // Named in the result as the owner that was told: one handed off while the notice waited
    // passes it to the owner the session is bound to now.
    let (told_owner, owner) = match tell_new_owner(state, &owner, &moved, text).await {
        Ok((delivery, told)) => (Ok(Some(delivery)), told),
        Err(err) => (Err(err), owner),
    };
    // Released, or given to another owner, while the notice waited: the transfer stood, but the
    // session has left that owner since, so the notice was dropped and is not for the caller to
    // send.
    let moved_on = delivery::is_no_longer_owned(&told_owner);
    let owner_told = if moved_on {
        "The notice of the transfer was not sent to its new parent session: the session was no \
         longer bound to it by the time it could be written."
            .to_string()
    } else {
        told(
            &told_owner,
            true,
            "The notice of the transfer",
            "its new parent session",
        )
    };
    let mut result_note = format!(
        "Transferred: `{id}` {went} and is no longer yours. {child} {owner_told}",
        id = moved.id,
        went = if moved_on {
            format!("went to {}, but has left it since,", named_session(&owner))
        } else {
            format!("now reports to {}", named_session(&owner))
        },
        child = told(&child, running, "The notice of the transfer", "the session"),
    );
    if told_owner.is_err() && !moved_on {
        result_note.push_str(&format!(
            " Tell the new parent session of the transfer yourself with `{}`.",
            qualified_tool_name(caller.agent, "inform_session")
        ));
    }
    Ok(json!({
        "session": moved.id,
        "owner": owner.id,
        "owner_kind": owner.kind.name(),
        "role": moved.role,
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
    let delivery = tell_released(state, &released, &caller.id, running, note.as_deref()).await;
    Ok(json!({
        "session": released.id,
        "role": released.role,
        "note": format!(
            "Released: `{id}` has no parent session now and is no longer yours. It is the \
             user's own session; you no longer instruct or drive it and it does not report to \
             you, though `{inform_session}` can still tell it something. {told}",
            id = released.id,
            inform_session = qualified_tool_name(caller.agent, "inform_session"),
            told = told(&delivery, running, "The notice of the release", "the session"),
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
    let (bound, owner, _running) =
        coordinator::bindings::bind_session(state, &caller.id, owner_id)?;
    let coordinator = bound.role == Role::Coordinator;

    // The binding is this round's report, so the round ending without another is not reported on
    // a second time.
    let delivered = reporting::deliver_own_report(
        state,
        &bound.id,
        summary.to_string(),
        ReportStatus::NeedsDecision,
        Vec::new(),
        Vec::new(),
        Some(first_report_on_bind(coordinator).to_string()),
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
        "owner_kind": owner.kind.name(),
        "role": bound.role,
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
        // It may already be there, and a second would repeat it.
        Err(err) if crate::messaging::delivery::acceptance_uncertain(err) => {
            "Your summary may already have been delivered to your new parent session as your \
             first report: its agent's queue did not confirm taking it. It will not be sent again, \
             and it counts as your report for this turn, so do not report it again."
                .to_string()
        }
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
    use crate::protocol::{error_code, AgentId};
    use crate::test_support::{
        code_of, fake_live_session, heard_after_marker, output_containing, running,
    };

    /// How the sessions of these tests, which run Claude Code, name one of the tools to their model.
    fn claude(tool: &str) -> String {
        qualified_tool_name(AgentId::CLAUDE, tool)
    }

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
        assert_eq!(result["role"], "worker");
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

    /// A new owner that is handed off, and archived, while the notice of the transfer waits for it
    /// is not the one told: the result names the owner the session is bound to now, which is the one
    /// that received the note.
    #[tokio::test(flavor = "current_thread")]
    async fn a_transfer_names_the_owner_it_told_when_the_new_owner_was_handed_off() {
        use crate::mcp::test_support::{console_session, with_status};
        let (state, _dir) = peer_state("transfer-owner-handed-off");
        state.store.insert_session(&console_session("c")).unwrap();
        let _a = running(&state, "a");
        let _of_a = running(&state, "of-a");
        let _b = running(&state, "b");
        let c = running(&state, "c");

        let transfer = {
            let state = state.clone();
            tokio::spawn(async move {
                call(
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
            })
        };
        // The transfer is written, and the notice to the session is under way: the task is away
        // from the runtime's one thread, which the test now has to itself. `b` is handed off to
        // `c` before the notice to `b` is written, as a handoff of it leaves things.
        let bound_to_b = || {
            state
                .store
                .get_session("of-a")
                .unwrap()
                .unwrap()
                .bound_to
                .as_deref()
                == Some("b")
        };
        while !bound_to_b() {
            tokio::task::yield_now().await;
        }
        with_status(&state, "b", crate::protocol::SessionStatus::Archived);
        state
            .store
            .set_binding(
                "of-a",
                Some("b"),
                "c",
                crate::protocol::Role::Worker,
                false,
                false,
            )
            .unwrap()
            .expect("rebound to the successor");

        let result = transfer.await.unwrap().unwrap();
        assert_eq!(result["owner"], "c", "{result}");
        assert_eq!(result["owner_kind"], "console", "{result}");
        let note = result["note"].as_str().unwrap();
        assert!(note.contains("`c`"), "{note}");
        assert!(!note.contains("`b`"), "{note}");
        let handed = output_containing(&c, "Half of the endpoints");
        assert!(
            handed.contains("> Half of the endpoints are done."),
            "{handed}"
        );
    }

    /// A session that leaves its new owner while the notice of the transfer is on its way is not
    /// told to the owner it left: the result says the notice was not sent and why, and does not
    /// send the caller to tell that owner itself.
    #[tokio::test(flavor = "current_thread")]
    async fn a_transfer_says_the_notice_was_not_sent_when_the_session_left_the_new_owner() {
        let (state, _dir) = peer_state("transfer-left-new-owner");
        let _a = running(&state, "a");
        let _of_a = running(&state, "of-a");
        let b = running(&state, "b");

        let transfer = {
            let state = state.clone();
            tokio::spawn(async move {
                call(
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
            })
        };
        // Written, with the notice to the session under way: the user releases it from `b` before
        // `b`'s notice is written (see
        // `a_transfer_names_the_owner_it_told_when_the_new_owner_was_handed_off`).
        let bound_to_b = || {
            state
                .store
                .get_session("of-a")
                .unwrap()
                .unwrap()
                .bound_to
                .as_deref()
                == Some("b")
        };
        while !bound_to_b() {
            tokio::task::yield_now().await;
        }
        state
            .store
            .clear_binding("of-a", "b", false, None)
            .unwrap()
            .expect("released");

        let result = transfer.await.unwrap().unwrap();
        assert_eq!(result["owner"], "b", "{result}");
        let note = result["note"].as_str().unwrap();
        assert!(
            note.contains(
                "The notice of the transfer was not sent to its new parent session: the session \
                 was no longer bound to it by the time it could be written."
            ),
            "{note}"
        );
        assert!(note.starts_with("Transferred: `of-a` went to "), "{note}");
        assert!(
            note.contains(", but has left it since, and is no longer yours."),
            "{note}"
        );
        assert!(!note.contains("now reports to"), "{note}");
        assert!(!note.contains("yourself"), "{note}");
        assert!(!note.contains("inform_session"), "{note}");
        let heard = crate::test_support::heard_after_marker(&state, &b);
        assert!(!heard.contains("Half of the endpoints"), "{heard}");
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
        assert_eq!(bound["role"], "coordinator");
        assert_eq!(
            bound["note"],
            format!(
                "Your role is coordinator. You now report to the console session `a` (\"Hub\"), your \
                 parent session: take work only from it, or from the user typing in this terminal, \
                 and report to it with `{report}` once per round of work. This \
                 replaces anything your role description says about being independent and not \
                 calling `{report}`. `done` with no open items archives you, and is \
                 refused while a session you started is not archived; `needs_decision` or open items \
                 leave you running. Items that are your parent session's to carry out rather than \
                 yours go in `owner_items`, not `open_items`. You keep your child sessions and go on \
                 driving them with `{open_session}` and the tools that go with it. Your \
                 summary, as your first report, was delivered to your new parent session. You have a \
                 parent session now, so do not call `{bind}` again.",
                report = claude("report"),
                open_session = claude("open_session"),
                bind = claude("bind_to_session"),
            )
        );
        for (caller, parent, expected, role_words) in [
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
            assert_eq!(moved["role"], expected);
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
            assert!(told.contains(role_words), "{told}");
            let record = call(&state, "loose", "get_session", &Map::new())
                .await
                .unwrap();
            assert_eq!(record["role"], expected);
            assert_eq!(record["owner"], parent);
            assert_eq!(record["owned_sessions"], 0);
            let wire = state.wire_session(state.session_record("loose").unwrap());
            assert_eq!(json!(wire.role), expected);
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
        assert_eq!(result["role"], "coordinator");
        let note = result["note"].as_str().unwrap();
        assert!(note.contains("no longer yours"));
        assert!(note.contains(&claude("inform_session")), "{note}");
        let released = state.store.get_session("of-a").unwrap().unwrap();
        assert_eq!(state.wire_session(released.clone()).role, Role::Coordinator);
        assert_eq!(released.bound_to, None);
        let told = output_containing(&of_a, "you have no parent session now");
        assert!(
            told.contains(&format!("`{}`", claude("open_session"))),
            "{told}"
        );
        assert!(
            told.contains(&format!("do not call `{}`", claude("report"))),
            "{told}"
        );
        assert!(
            told.contains(&format!(
                "tell your former parent session something with `{}`",
                claude("inform_session")
            )),
            "{told}"
        );
        assert!(
            told.contains(&format!(
                "call `{}` once with no arguments first,",
                claude("get_session")
            )),
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
            .set_binding("other-project", None, "a", Role::Worker, false, false)
            .unwrap()
            .unwrap();
        // A terminal holds no more than 1024 bytes of input that nothing reads, which this message
        // is longer than: this stand-in reads it all.
        let released = fake_live_session(
            "other-project",
            AgentId::CLAUDE,
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
            told.contains("You were released from the console session `a` (")
                && told.contains("), the session you reported to, and you are"),
            "{told}"
        );
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

    /// What a session is told about Octoboard's tools names them as its own agent does, whatever
    /// that agent puts between the server and the tool.
    #[tokio::test]
    async fn the_note_of_a_binding_names_tools_by_the_sessions_own_agents_spelling() {
        let (state, _dir) = peer_state("bind-names-by-agent");
        let _loose = running(&state, "loose");
        let _starter = running(&state, "starter");
        let session = state.store.get_session("loose").unwrap().unwrap();
        state
            .store
            .update_session(&Session {
                agent: AgentId::TEST,
                ..session
            })
            .unwrap();
        let result = call(
            &state,
            "loose",
            "bind_to_session",
            &arguments(json!({ "owner": "starter", "summary": "Auditing." })),
        )
        .await
        .unwrap();
        let note = result["note"].as_str().unwrap();
        let report = qualified_tool_name(AgentId::TEST, "report");
        assert_eq!(report, "mcp__octoboard.report");
        assert!(note.contains(&format!("`{report}`")), "{note}");
        assert!(!note.contains("octoboard__"), "{note}");
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
            &arguments(json!({ "body": [{ "type": "markdown", "text": "plan" }] })),
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
        assert_eq!(result["role"], "worker");
        let note = result["note"].as_str().unwrap();
        assert!(
            note.contains("can no longer start or drive sessions"),
            "{note}"
        );
        assert!(
            note.starts_with(&format!(
                "Your role is worker. You now report to the project session `starter` (\"Work \
                 starter\"), your parent session: take work only from it, or from the user typing \
                 in this terminal, and report to it with `{report}` once per round \
                 of work. This replaces anything your role description says about being \
                 independent and not calling `{report}`. `done` with no open items \
                 archives you; `needs_decision` or open items leave you running. Items that are \
                 your parent session's to carry out rather than yours go in `owner_items`, not \
                 `open_items`. You can no longer start or drive sessions",
                report = claude("report")
            )),
            "{note}"
        );
        assert!(note.contains(&format!("`{}`", claude("report"))), "{note}");
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
        let report = output_containing(&starter, "auditing the retries").replace("\r\n", "\n");
        assert!(
            report.contains(
                "Report from your child session, the project session `loose` (\"Work loose\") in \
                 the project \"project-1\"\nStatus: needs_decision\nThis project session is now a \
                 worker reporting to you: it is yours to drive from now on, and reports to you. \
                 Its summary of what it is doing and needs follows.\n\n> I am auditing the \
                 retries.\n— end of report from session `loose` —"
            ),
            "{report}"
        );

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

    /// The refusal of a bind for an owner that cannot own the session is coded like the user's
    /// (`owner_cannot_own`, naming the session and the owner), and the bind of a running owner
    /// still goes through.
    #[tokio::test]
    async fn a_refused_bind_is_coded_and_a_running_owner_still_takes_it() {
        let (state, _dir) = peer_state("bind-coded");
        let _loose = running(&state, "loose");
        let bind = |owner: &str| {
            let arguments = arguments(json!({ "owner": owner, "summary": "Ready." }));
            let state = &state;
            async move { call(state, "loose", "bind_to_session", &arguments).await }
        };

        // `a` is a console session with no process behind it.
        let err = bind("a").await.expect_err("not running");
        assert_eq!(code_of(&err), error_code::OWNER_CANNOT_OWN, "{err}");
        let coded = err.downcast_ref::<crate::protocol::CodedError>().unwrap();
        assert_eq!(
            coded.params.get("session").map(String::as_str),
            Some("loose")
        );
        assert_eq!(coded.params.get("owner").map(String::as_str), Some("a"));
        assert!(err.to_string().contains("not running"), "{err}");
        assert_eq!(
            state.store.get_session("loose").unwrap().unwrap().bound_to,
            None
        );

        let _a = running(&state, "a");
        let result = bind("a").await.unwrap();
        assert_eq!(result["owner"], "a");
        assert_eq!(
            state
                .store
                .get_session("loose")
                .unwrap()
                .unwrap()
                .bound_to
                .as_deref(),
            Some("a")
        );
    }

    /// What the caller of a change of owner is told became of each notice, and of its summary
    /// sent as its first report, word for word.
    #[test]
    fn the_caller_is_told_what_became_of_each_notice_word_for_word() {
        let notice = |delivery| {
            told(
                &delivery,
                false,
                "The notice of the transfer",
                "the session",
            )
        };
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
        // A session that had a process but whose notice could not be written is told at its next
        // start, which is not the same as having none.
        assert_eq!(
            told(&Ok(None), true, "The notice of the transfer", "the session"),
            "The session could not be told now, and will be told when it next starts."
        );
        assert_eq!(
            notice(Err(anyhow::anyhow!("the pipe broke"))),
            "The session could not be told (the pipe broke)."
        );

        let first = |delivery: Delivery| {
            first_report_note(&Ok((String::new(), delivery)), &claude("report"))
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
                &claude("report")
            ),
            format!(
                "Your summary could not be delivered to your new parent session (the pipe broke), so \
                 send it yourself with `{}`.",
                claude("report")
            )
        );
        // One that may already be there is not to be sent again.
        let uncertain = crate::protocol::CodedError::raised(
            crate::protocol::error_code::MESSAGE_ACCEPTANCE_UNCONFIRMED,
            "not confirmed",
            &[("agent", "Codex")],
        );
        let note = first_report_note(&Err(uncertain), &claude("report"));
        assert!(
            note.contains("may already have been delivered")
                && note.contains("do not report it again")
                && !note.contains("send it yourself"),
            "{note}"
        );
    }
}
