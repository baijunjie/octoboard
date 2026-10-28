//! The tools that change who a session reports to: `transfer_session`, with which an owner hands
//! a session of its own to another, `release_session`, with which it lets one go, and
//! `bind_to_session`, with which an unbound project session binds itself to an owner. What may own
//! what is `coordinator::owner_refusal`'s; what is here is telling each session concerned what
//! became of it, since a session that was told at launch who it reports to is not told again by
//! anything else.

use std::sync::Arc;

use anyhow::Result;
use serde_json::{json, Map, Value};

use crate::coordinator::{self, Released};
use crate::protocol::{Agent, Role, Session};
use crate::reporting::{self, Delivery, ReportStatus, WhenBlocked, NOTICE_PREFIX};
use crate::sharing;
use crate::state::AppState;

use super::args::{optional_string, required_str};
use super::exec::resolve_owned_session;

/// The most characters of the parting note of a `release_session`.
const MAX_NOTE_CHARS: usize = 4000;

/// The tools a project session has, with the role of a bound one or not, by the names its agent
/// shows them, for telling a session whose binding has just changed what it has.
fn project_tools(agent: Agent, bound: bool) -> String {
    super::tools_for(Role::Project, bound)
        .iter()
        .map(|tool| format!("`{}`", super::qualified_tool_name(agent, tool.name)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// What a delivery to `who` came to, for the sender's result. `None` is a session with no process,
/// which is told when it next starts.
fn told(delivery: &Result<Option<Delivery>>, who: &str) -> String {
    match delivery {
        Ok(Some(Delivery::Written)) => format!("{who} was told."),
        Ok(Some(Delivery::Queued)) => {
            format!("{who} will be told as soon as it can take a message.")
        }
        Ok(Some(Delivery::Kept)) => format!("{who} is not running, and will be told once resumed."),
        Ok(Some(Delivery::Unsent)) => format!(
            "{who} was not told: the message was written into its input, but its agent did not \
             send it to the conversation (it is in the input box, or went to a view of its \
             background agents). The user has been told to check the session."
        ),
        Ok(None) => format!("{who} is not running, and will be told when it next starts."),
        Err(err) => format!("{who} could not be told ({err:#})."),
    }
}

/// The sentence that tells a session it can no longer start or drive sessions, with the tools it
/// has left.
fn lost_tools(agent: Agent) -> String {
    format!(
        "You can no longer start or drive sessions: of Octoboard's tools you now have {}, and a \
         call of any other is refused. Any pages you pushed to the report panel were withdrawn \
         with the tools for them.",
        project_tools(agent, true)
    )
}

/// The sentence that tells a session whose it is from now on, in the words of its `agent`'s tool
/// names.
pub(crate) fn owner_from_now_on(agent: Agent, owner: &str) -> String {
    format!(
        "{owner} is your owner from now on: take work only from it, or from the user typing in \
         this terminal, and report to it with `{report}`.",
        report = super::qualified_tool_name(agent, "report"),
    )
}

/// Whether `session` has a process to write into. Decided by the one who changes its owner, while
/// it holds the claim a launch needs, and handed to [`tell_now_or_leave`]; a session with none is
/// left owed a notice for its next launch (see [`owner_notice`]). Also what decides whether a
/// former owner is told of a release by the user ([`tell_former_owner`]).
pub(crate) fn is_running(state: &Arc<AppState>, session: &Session) -> bool {
    !session.status.is_dormant() && state.live_session(&session.id).is_some()
}

/// [`is_running`], marking a session with no process owed its notice now: for a change of owner
/// that is not a write of its own binding.
pub(crate) fn running_or_leave(state: &Arc<AppState>, session: &Session) -> Result<bool> {
    let running = is_running(state, session);
    if !running {
        state.store.mark_owner_notice(&session.id)?;
    }
    Ok(running)
}

/// Writes `text` into the session `id`, queued while it cannot take a message, when `running` says
/// it has a process (see [`running_or_leave`]); `None` is a session left to be told when it next
/// starts. A write that fails, a process that ended meanwhile say, leaves the notice for the next
/// launch too: it is built then from the binding as it stands, so telling twice does no harm. A
/// `note`, the parting words of a release, is kept with the notice that is left owed, as long as
/// the session is still unbound.
pub(crate) async fn tell_now_or_leave(
    state: &Arc<AppState>,
    id: &str,
    text: String,
    running: bool,
    note: Option<&str>,
) -> Result<Option<Delivery>> {
    if !running {
        return Ok(None);
    }
    match reporting::write_message_off_runtime(state, id, text, WhenBlocked::Queue).await {
        Ok(delivery) => Ok(Some(delivery)),
        Err(err) => {
            tracing::debug!(session = %id, %err, "telling a session of its owner failed");
            match note {
                Some(note) => state.store.mark_owner_notice_with_note(id, note)?,
                None => state.store.mark_owner_notice(id)?,
            }
            Ok(None)
        }
    }
}

/// What a session that was not running when its owner changed is told when it next starts, built
/// from its binding as it stands then, so a later change cannot leave it stale: who its owner is,
/// or that it has none, and, for one bound to a project session, the tools it has left. A session
/// is told at launch because Claude Code and Grok replay what they were first told of their owner
/// on every resume.
pub(crate) fn owner_notice(state: &Arc<AppState>, session: &Session) -> Result<String> {
    let Some(owner_id) = &session.bound_to else {
        // The note of the release is told only while the release is the latest change of owner.
        let note = state.store.owner_notice_note(&session.id)?;
        return Ok(released_text(session, note.as_deref()));
    };
    // An owner no longer on record is named by its id alone, as a report to it would be.
    let owner = match state.store.get_session(owner_id)? {
        Some(owner) => sharing::named_session(&owner),
        None => format!("`{owner_id}`"),
    };
    let tools = if session.has_bound_role() {
        format!(" {}", lost_tools(session.agent))
    } else {
        String::new()
    };
    Ok(format!(
        "{NOTICE_PREFIX}: your owner has changed while you were not running. {owner} This \
         replaces what you were told of your owner when you were started.{tools}",
        owner = owner_from_now_on(session.agent, &owner),
    ))
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
    let (moved, owner, running) =
        coordinator::transfer_session(state, &session.id, &caller.id, to)?;

    let tools = if !session.has_bound_role() && moved.has_bound_role() {
        format!(" {}", lost_tools(moved.agent))
    } else {
        String::new()
    };
    let to_session = format!(
        "{NOTICE_PREFIX}: your owner has changed. {from} handed you over to {to}. {owner_now} \
         This replaces what you were told of your owner when you were started; `{from_id}` no \
         longer drives you.{tools}",
        from = sharing::named_session(caller),
        to = sharing::named_session(&owner),
        owner_now = owner_from_now_on(moved.agent, "It"),
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
         yours, with `{instruct_session}`, `{archive_session}` and `{reopen_session}`.{lead} The \
         handover note:\n\n{note}\n— end of the handover note —",
        from = sharing::named_session(caller),
        id = moved.id,
        title = sharing::one_line(&moved.title),
        place = sharing::place_of(state, &moved),
        instruct_session = owner_tool("instruct_session"),
        archive_session = owner_tool("archive_session"),
        reopen_session = owner_tool("reopen_session"),
        note = sharing::quote(&note),
    );
    // Neither message credits the turn it starts: if the moved session or its new owner carries on
    // and stops without reporting, the report Octoboard synthesises is the safety net, and it is
    // wanted there.
    let child = tell_now_or_leave(state, &moved.id, to_session, running, None).await;
    let told_owner = reporting::write_notice_to_owner_off_runtime(state, &owner, &moved, to_owner)
        .await
        .map(Some);
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
            super::qualified_tool_name(caller.agent, "send_message")
        ));
    }
    Ok(json!({
        "session": moved.id,
        "owner": owner.id,
        "owner_kind": owner.role.kind(),
        "note": result_note,
    }))
}

/// Tells the project session `released`, just released from its owner, that it has none: its role
/// description, recorded when it was launched, still says whose it is. Queued while the session
/// cannot take a message; one with no process is told when it next starts (see [`owner_notice`]),
/// with the former owner's parting `note` if the release stored one.
pub(crate) async fn tell_released(
    state: &Arc<AppState>,
    released: &Session,
    running: bool,
    note: Option<&str>,
) -> Result<Option<Delivery>> {
    tell_now_or_leave(
        state,
        &released.id,
        released_text(released, note),
        running,
        note,
    )
    .await
}

/// Tells the former owner `owner_id` that the user released `released` from it, when it has a
/// process to be told in: one that has none is not told, and a later attempt by it to drive the
/// session is refused as not its own. Queued while it cannot take a message. Only the user's
/// release is told this way, since an owner that releases a session knows.
pub(crate) async fn tell_former_owner(
    state: &Arc<AppState>,
    owner_id: &str,
    released: &Session,
) -> Result<Option<Delivery>> {
    let Some(owner) = state.store.get_session(owner_id)? else {
        return Ok(None);
    };
    if !is_running(state, &owner) {
        return Ok(None);
    }
    let text = format!(
        "{NOTICE_PREFIX}: the user released {released} from you. It is no longer yours: it has \
         no owner now and answers only to the user, so you can no longer instruct, drive or \
         archive it, and it does not report to you. You can still tell it something with \
         `{send_message}`, which it weighs rather than obeys. Do not count on it any more, and \
         treat any work you handed it as left with the user.",
        released = sharing::named_session(released),
        send_message = super::qualified_tool_name(owner.agent, "send_message"),
    );
    reporting::write_message_off_runtime(state, owner_id, text, WhenBlocked::Queue)
        .await
        .map(Some)
}

/// The notice of a release, ending with the former owner's parting `note` when there is one.
fn released_text(released: &Session, note: Option<&str>) -> String {
    let tool = |name: &str| super::qualified_tool_name(released.agent, name);
    let note = match note {
        Some(note) => format!(
            " Your former owner left this note:\n\n{note}\n— end of the note —",
            note = sharing::quote(note),
        ),
        None => String::new(),
    };
    format!(
        "{NOTICE_PREFIX}: you have no owner now. You were released from the session you \
         reported to, and you are the user's own session: take work only from the user typing in \
         this terminal, and do not call `{report}`, which is refused for a session with no owner. \
         This replaces what you were told of your owner when you were started. You can start and \
         drive sessions of your own with `{new_session}` and the tools that go with it (call \
         `{get_session}` once with no arguments first, so that your tool list catches up), and \
         you can still tell your former owner something with `{send_message}`, which it weighs \
         rather than obeys; of Octoboard's tools you now have {tools}.{note}",
        report = tool("report"),
        get_session = tool("get_session"),
        new_session = tool("new_session"),
        send_message = tool("send_message"),
        tools = project_tools(released.agent, false),
    )
}

/// Releases a session of the caller's from its binding; see `coordinator::release_session`. The
/// session is told it has no owner now, with the caller's `note` if it gave one, and the caller
/// learns from the result what became of it.
pub(super) async fn release_session(
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
    } = coordinator::release_session(state, &session.id, Some(&caller.id), note.as_deref())?;
    let delivery = tell_released(state, &released, running, note.as_deref()).await;
    Ok(json!({
        "session": released.id,
        "note": format!(
            "Released: `{id}` has no owner now and is no longer yours. It is the user's own \
             session; you no longer instruct or drive it and it does not report to you, though \
             `{send_message}` can still tell it something. {told}",
            id = released.id,
            send_message = super::qualified_tool_name(caller.agent, "send_message"),
            told = told(&delivery, "It"),
        ),
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
        Ok((_, Delivery::Kept)) => "It is not running, so your summary is kept for it as your \
                                    first report and delivered once it is resumed; do not send it \
                                    again."
            .to_string(),
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
             leave it running. {}",
            lost_tools(bound.agent)
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
    use crate::test_support::{fake_live_session, heard_after_marker, output_containing, running};

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
        let note = result["note"].as_str().unwrap();
        assert!(note.contains("no longer yours"));
        assert!(
            note.contains(&super::super::qualified_tool_name(
                Agent::Claude,
                "send_message"
            )),
            "{note}"
        );
        let released = state.store.get_session("of-a").unwrap().unwrap();
        assert_eq!((released.bound_to, released.lead), (None, false));
        let told = output_containing(&of_a, "you have no owner now");
        assert!(told.contains("`mcp__octoboard__new_session`"), "{told}");
        assert!(
            told.contains("do not call `mcp__octoboard__report`"),
            "{told}"
        );
        assert!(
            told.contains("tell your former owner something with `mcp__octoboard__send_message`"),
            "{told}"
        );
        assert!(
            told.contains("call `mcp__octoboard__get_session` once with no arguments first,"),
            "{told}"
        );
        // A blank note adds nothing, and the owner that released it is not told what it did.
        assert!(!told.contains("former owner left"), "{told}");
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
            .set_binding("other-project", None, "a", false, false, false)
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
        assert!(told.contains("you have no owner now"), "{told}");
        assert!(told.contains("Your former owner left this note:"), "{told}");
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
        let note = result["note"].as_str().unwrap();
        assert!(
            note.contains("can no longer start or drive sessions"),
            "{note}"
        );
        assert!(note.contains("`mcp__octoboard__report`"), "{note}");
        assert!(note.contains("pages you pushed"), "{note}");
        // Its pages are withdrawn with the tools for them, and the panels are told.
        assert!(state.store.list_pages("loose").unwrap().is_empty());
        let mut withdrawn = Vec::new();
        while let Ok(event) = events.try_recv() {
            if let crate::protocol::Event::PagesDeleted {
                console_session_id,
                pages,
            } = event
            {
                assert_eq!(console_session_id, "loose");
                withdrawn.extend(pages);
            }
        }
        assert_eq!(withdrawn.len(), 1);
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
