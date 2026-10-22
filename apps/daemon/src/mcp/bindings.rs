//! The tools that change who a session reports to: `transfer_session`, with which an owner hands
//! a session of its own to another, and `bind_to_session`, with which an unbound project session
//! binds itself to an owner. What may own what is `coordinator::owner_refusal`'s; what is here is
//! telling each session concerned what became of it, since a session that was told at launch who
//! it reports to is not told again by anything else.

use std::sync::Arc;

use anyhow::Result;
use serde_json::{json, Map, Value};

use crate::coordinator;
use crate::protocol::{Agent, Role, Session};
use crate::reporting::{self, Delivery, ReportStatus, WhenBlocked, NOTICE_PREFIX};
use crate::sharing;
use crate::state::AppState;

use super::args::required_str;
use super::exec::resolve_owned_session;

/// The tools a session with the role of a bound one has, by the names its agent shows them, for
/// telling a session whose binding has just changed what it has left.
fn bound_tools(agent: Agent) -> String {
    super::tools_for(Role::Project, true)
        .iter()
        .map(|tool| format!("`{}`", super::qualified_tool_name(agent, tool.name)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// What a delivery to `who` came to, for the sender's result.
fn told(delivery: &Result<Delivery>, who: &str) -> String {
    match delivery {
        Ok(Delivery::Written) => format!("{who} was told."),
        Ok(Delivery::Queued) => format!("{who} will be told as soon as it can take a message."),
        Ok(Delivery::Unsent) => format!(
            "{who} was not told: the message was written into its input, but its agent did not \
             send it to the conversation (it is in the input box, or went to a view of its \
             background agents). The user has been told to check the session."
        ),
        Err(err) => format!("{who} could not be told ({err:#})."),
    }
}

/// Hands a session of the caller's to another owner; see `coordinator::transfer_session` for what
/// may own what. The session is told who it now reports to, the new owner is told it has the
/// session with the caller's note, and the caller learns from the result what became of both.
pub(super) async fn transfer_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let session = resolve_owned_session(state, caller, required_str(arguments, "session")?)?;
    let to = required_str(arguments, "to")?;
    let note = required_str(arguments, "note")?.to_string();
    let (moved, owner) = coordinator::transfer_session(state, &session.id, &caller.id, to)?;

    let tool = |name: &str| super::qualified_tool_name(moved.agent, name);
    let tools = if !session.has_bound_role() && moved.has_bound_role() {
        format!(
            " You can no longer start or drive sessions: of Octoboard's tools you now have {}, \
             and a call of any other is refused.",
            bound_tools(moved.agent)
        )
    } else {
        String::new()
    };
    let to_session = format!(
        "{NOTICE_PREFIX}: your owner has changed. {from} handed you over to {to}, which is \
         your owner from now on: take work only from it, or from the user typing in this \
         terminal, and report to it with `{report}`. This replaces what you were told of your \
         owner when you were started; `{from_id}` no longer drives you.{tools}",
        from = sharing::named_session(caller),
        to = sharing::named_session(&owner),
        report = tool("report"),
        from_id = caller.id,
    );
    let owner_tool = |name: &str| super::qualified_tool_name(owner.agent, name);
    let lead = if moved.lead {
        " It keeps the sessions it started, which you can read but not drive."
    } else {
        ""
    };
    let to_owner = format!(
        "{NOTICE_PREFIX}: {from} handed its session `{id}` (\"{title}\"){place} over to \
         you. It is yours from now on: it reports to you, and you drive it like any session of \
         yours, with `{send_message}`, `{archive_session}` and `{reopen_session}`.{lead} The \
         handover note:\n\n{note}\n— end of the handover note —",
        from = sharing::named_session(caller),
        id = moved.id,
        title = sharing::one_line(&moved.title),
        place = sharing::place_of(state, &moved),
        send_message = owner_tool("send_message"),
        archive_session = owner_tool("archive_session"),
        reopen_session = owner_tool("reopen_session"),
        note = sharing::quote(&note),
    );
    // Neither message credits the turn it starts: if the moved session or its new owner carries on
    // and stops without reporting, the report Octoboard synthesises is the safety net, and it is
    // wanted there.
    let child =
        reporting::write_message_off_runtime(state, &moved.id, to_session, WhenBlocked::Queue)
            .await;
    let told_owner =
        reporting::write_message_off_runtime(state, &owner.id, to_owner, WhenBlocked::Queue).await;
    let mut result_note = format!(
        "Handed over: `{id}` now reports to {to} and is no longer yours. {child} {owner}",
        id = moved.id,
        to = sharing::named_session(&owner),
        child = told(&child, "It"),
        owner = told(&told_owner, "Its new owner"),
    );
    if told_owner.is_err() {
        result_note.push_str(&format!(
            " Tell the new owner of the handover yourself with `{}`.",
            super::qualified_tool_name(caller.agent, "share_info")
        ));
    }
    Ok(json!({
        "session": moved.id,
        "owner": owner.id,
        "owner_kind": owner.role.kind(),
        "note": result_note,
    }))
}

/// Binds the unbound caller to an owner; see `coordinator::bind_session` for what may own what.
/// The owner is sent the caller's summary as its first report, and the caller is told in the
/// result what it now is and which tools it has: its role description, recorded when it was
/// launched unbound, says otherwise.
pub(super) async fn bind_to_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let owner_id = required_str(arguments, "owner")?;
    let summary = required_str(arguments, "summary")?;
    let (bound, owner) = coordinator::bind_session(state, &caller.id, owner_id)?;
    let lead = bound.lead;

    let first_report = if lead {
        format!(
            "This project session bound itself to you, as a lead session: it is yours to drive \
             from now on, and reports to you. It keeps the sessions it started in its own project \
             and drives them itself; you can read those but not drive them.\n\
             \n\
             What it is doing and needs:\n\
             {summary}"
        )
    } else {
        format!(
            "This project session bound itself to you: it is yours to drive from now on, and \
             reports to you.\n\
             \n\
             What it is doing and needs:\n\
             {summary}"
        )
    };
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
    let tool = |name: &str| super::qualified_tool_name(bound.agent, name);
    // Nothing after the binding fails the call: the session is bound by now, so a first report
    // that was not delivered, for whatever reason, is the session's to send.
    let first = match delivered {
        Ok((_, Delivery::Unsent)) => {
            "Your summary was written into its input as your first report, but its agent did not \
             send it to the conversation, so it has not read it; the user has been told. Do not \
             send it again."
                .to_string()
        }
        Ok(_) => "Your summary was sent to it as your first report.".to_string(),
        Err(err) => format!(
            "Your summary could not be delivered to it ({err:#}), so send it yourself with `{}`.",
            tool("report")
        ),
    };
    let what_now = if lead {
        format!(
            "`done` with no open items archives this session, and is refused while a session you \
             started is not archived; `needs_decision` or open items leave it running. You keep \
             the sessions you started and go on driving them with `{new_session}` and the \
             tools that go with it.",
            new_session = tool("new_session"),
        )
    } else {
        format!(
            "`done` with no open items archives this session; `needs_decision` or open items \
             leave it running. You can no longer start or drive sessions: of Octoboard's tools \
             you now have {}, and a call of any other is refused.",
            bound_tools(bound.agent)
        )
    };
    let note = format!(
        "You are now bound to {owner}{as_lead}. {first} From now on it is your owner: take work \
         from it, or from the user typing in this terminal, and report to it with `{report}` once \
         per round of work. This replaces anything your role description says about being \
         unbound and not calling `{report}`. {what_now} You are bound now, so do not call \
         `{bind}` again.",
        owner = sharing::named_session(&owner),
        as_lead = if lead { ", as its lead session" } else { "" },
        report = tool("report"),
        bind = tool("bind_to_session"),
    );
    Ok(json!({
        "owner": owner.id,
        "owner_kind": owner.role.kind(),
        "note": note,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::exec::call;
    use crate::mcp::exec::tests::{arguments, peer_state};
    use crate::test_support::{output_containing, running};

    /// A handover moves the session to its new owner, tells it whose it now is, and sends the new
    /// owner the note, quoted; the session is the new owner's to drive from then on, not the old
    /// one's.
    #[tokio::test]
    async fn a_handover_tells_the_session_and_its_new_owner() {
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
        let told = output_containing(&of_a, "your owner has changed");
        assert!(told.contains("your owner has changed"), "{told}");
        let handed = output_containing(&b, "Half of the endpoints");
        assert!(
            handed.contains("> Half of the endpoints are done."),
            "{handed}"
        );

        let err = call(
            &state,
            "a",
            "send_message",
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

    /// A session that binds itself to a project session becomes an ordinary bound one: the owner
    /// gets its summary as its first report, its result says which tools it has left, and the
    /// ones it had for owning sessions are refused from then on.
    #[tokio::test]
    async fn binding_to_a_project_session_takes_the_tools_for_owning_sessions_away() {
        let (state, _dir) = peer_state("bind-to-project");
        let _loose = running(&state, "loose");
        let starter = running(&state, "starter");
        let result = call(
            &state,
            "loose",
            "bind_to_session",
            &arguments(json!({ "owner": "starter", "summary": "I am auditing the retries." })),
        )
        .await
        .unwrap();
        let note = result["note"].as_str().unwrap();
        assert!(
            note.contains("can no longer start or drive sessions"),
            "{note}"
        );
        assert!(note.contains("`mcp__octoboard__report`"), "{note}");
        let report = output_containing(&starter, "auditing the retries");
        assert!(report.contains("Report from session loose"), "{report}");

        let err = call(
            &state,
            "loose",
            "new_session",
            &arguments(json!({ "brief": { "goal": "x" } })),
        )
        .await
        .expect_err("no longer offered");
        assert!(err.to_string().contains("is not a tool"), "{err}");
    }
}
