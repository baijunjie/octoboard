//! The notices Octoboard writes about who a session reports to: to an owner about one of its
//! sessions — it ended on its own, it waits for the user, Octoboard switched it to another account
//! at its usage limit or resumed it once its quota recovered, or could not — and to a session
//! whose owner changed, or to the owner that lost it, with whose it is now and the tools that
//! leaves it. A session that was told at launch who it reports to is not told again by anything
//! else, so one with no process when its owner changes is left owed its notice, built at its next
//! launch from the binding as it stands then. The notice that a session stayed idle after its last
//! report is [`crate::messaging::idle_children`]'s, written through [`tell_owner`] here.

use std::sync::Arc;

use anyhow::Result;

use crate::messaging::delivery::{
    self, is_archived_refusal, rebound_owner, write_notice_to_owner, Delivery, WhenBlocked,
};
use crate::messaging::naming::{label, named_session, one_line, place_of, quote};
use crate::messaging::texts::{capitalised, session_notice_opening, NOTICE_PREFIX, WAITING_PHRASE};
use crate::protocol::{AgentId, Role, Session, SessionKind, SessionStatus};
use crate::session_tools::catalogue;
use crate::state::AppState;

/// Tells the owner of `session` that its process ended without anyone asking it to, so that the
/// work it was doing is not waited for, as [`tell_owner`] does.
///
/// **Blocks** on writing into the owner.
pub fn tell_owner_it_ended(state: &Arc<AppState>, session: &Session) {
    tell_owner(state, session, |owner| {
        format!(
            "{NOTICE_PREFIX}: {session} ended on its own: its process exited although nobody had \
             archived it, interrupted it or moved it to another account, whether it crashed or the \
             user closed it in its terminal. It is recorded as interrupted, and the work it was \
             doing stopped with it. Look at it with `{get_session}`, and relaunch it with \
             `{resume_session}` if the work should go on.",
            session = named_session(session),
            get_session = catalogue::qualified_tool_name(owner.agent, "get_session"),
            resume_session = catalogue::qualified_tool_name(owner.agent, "resume_session"),
        )
    });
}

/// [`tell_owner_it_waits`] on a blocking thread, for a caller on the runtime that has just seen
/// the session wait for the user by a route other than a hook: a prompt on its terminal, a
/// dialog of its agent, or a request of its own.
pub fn spawn_tell_owner_it_waits(state: &Arc<AppState>, id: &str) {
    let (state, id) = (state.clone(), id.to_string());
    tokio::task::spawn_blocking(move || tell_owner_it_waits(&state, &id));
}

/// Tells the owner of `session` that Octoboard's own switch of it to another account failed after
/// its process was ended, and that bringing it back up on the account it had failed too, so that it
/// is interrupted and the work it was doing stopped, as [`tell_owner`] does.
///
/// **Blocks** on writing into the owner.
pub fn tell_owner_the_switch_failed(state: &Arc<AppState>, session: &Session, reason: &str) {
    tell_owner(state, session, |owner| {
        format!(
            "{NOTICE_PREFIX}: {session} reached the usage limit of its account, and Octoboard \
             could neither switch it to another account nor bring it back up on the one it had \
             ({reason}). It is interrupted, and the work it was doing stopped with it. Look at it \
             with `{get_session}`, and relaunch it with `{resume_session}` if the work should go \
             on.",
            session = named_session(session),
            get_session = catalogue::qualified_tool_name(owner.agent, "get_session"),
            resume_session = catalogue::qualified_tool_name(owner.agent, "resume_session"),
        )
    });
}

/// Tells the owner of `session` that Octoboard switched it to the account `to` by itself when its
/// account `from` reached its usage limit, as [`tell_owner`] does. `continued` is whether its
/// relaunch carried a prompt to continue the task its turn stopped on.
///
/// **Blocks** on writing into the owner.
pub fn tell_owner_it_was_switched(
    state: &Arc<AppState>,
    session: &Session,
    from: &str,
    to: &str,
    continued: bool,
) {
    tell_owner(state, session, |owner| {
        it_was_switched(owner, session, from, to, continued)
    });
}

/// The notice of [`tell_owner_it_was_switched`] for `owner`.
fn it_was_switched(
    owner: &Session,
    session: &Session,
    from: &str,
    to: &str,
    continued: bool,
) -> String {
    format!(
        "{NOTICE_PREFIX}: {named} reached the usage limit of its {} account `{from}`, so \
         Octoboard switched it to `{to}` by itself. Its conversation came with it. {}",
        session.agent.label(),
        if continued {
            "Its turn had stopped on the limit, so it was sent a prompt to continue that task: \
             do not send it one yourself."
                .to_string()
        } else {
            format!(
                "It is back awaiting instructions, so send it the next step with `{}` if it \
                 has one pending.",
                catalogue::qualified_tool_name(owner.agent, "instruct_session")
            )
        },
        named = named_session(session),
    )
}

/// Tells the owner of `session` that it resumed by itself on its own account once that account's
/// usage quota recovered, as [`tell_owner`] does. `continued` is whether it was sent a prompt to
/// continue.
///
/// **Blocks** on writing into the owner.
pub fn tell_owner_it_resumed_on_recovery(
    state: &Arc<AppState>,
    session: &Session,
    continued: bool,
) {
    tell_owner(state, session, |_| resumed_on_recovery(session, continued));
}

/// The notice of [`tell_owner_it_resumed_on_recovery`].
fn resumed_on_recovery(session: &Session, continued: bool) -> String {
    let continuation = if continued {
        "It was sent a prompt to continue; do not send it one yourself."
    } else {
        "It is awaiting instructions."
    };
    format!(
        "{NOTICE_PREFIX}: {} resumed automatically after its account's usage quota \
         recovered. {continuation}",
        named_session(session)
    )
}

/// Tells the owner of `session` that its model was at capacity and that retry number `attempt` of
/// `of` is sent in `seconds`, in place of the failed report, as [`tell_owner`] does.
///
/// **Blocks** on writing into the owner.
pub fn tell_owner_it_will_be_retried(
    state: &Arc<AppState>,
    session: &Session,
    attempt: usize,
    of: usize,
    seconds: u64,
) {
    tell_owner(state, session, |_| {
        will_be_retried(session, attempt, of, seconds)
    });
}

/// The notice of [`tell_owner_it_will_be_retried`].
fn will_be_retried(session: &Session, attempt: usize, of: usize, seconds: u64) -> String {
    format!(
        "{NOTICE_PREFIX}: the model of {} was at capacity, so its turn stopped. {} will retry \
         automatically in {seconds} seconds (retry {attempt} of {of}) in the same session; do \
         not send it a continue prompt yourself.",
        named_session(session),
        crate::APP_NAME
    )
}

/// Tells the owner of `session` that it could not be resumed on its own account once that
/// account's usage quota recovered, for `reason`, as [`tell_owner`] does.
///
/// **Blocks** on writing into the owner.
pub fn tell_owner_the_recovery_failed(state: &Arc<AppState>, session: &Session, reason: &str) {
    tell_owner(state, session, |_| recovery_failed(session, reason));
}

/// The notice of [`tell_owner_the_recovery_failed`].
fn recovery_failed(session: &Session, reason: &str) -> String {
    format!(
        "{NOTICE_PREFIX}: {} could not resume after its account's usage quota \
         recovered. It is interrupted and its work is stopped. Reason: {reason}",
        named_session(session)
    )
}

/// Tells the owner of the session `id` that it waits for the user, which no report says: the
/// fallback report at the end of a turn is held back while the session waits, and a report it sent
/// earlier in the turn does not say it then stopped for a person. Once a turn (see
/// [`AppState::claim_waiting_notice`]). Nothing when the session is not shown as waiting by now,
/// by whatever route.
///
/// **Blocks** on writing into the owner.
pub fn tell_owner_it_waits(state: &Arc<AppState>, id: &str) {
    let session = match state.store.get_session(id) {
        Ok(Some(session))
            if session.bound_to.is_some()
                && state.shown_status(&session) == SessionStatus::WaitingUser =>
        {
            session
        }
        _ => return,
    };
    if !state.claim_waiting_notice(id) {
        return;
    }
    tell_owner(state, &session, |_| {
        format!(
            "{opening}{title}{WAITING_PHRASE}: it is at a permission prompt or has asked the \
             user something, and only they can clear it. Do not chase it or send it anything to \
             get it going: a message to it is held until they are done. Carry on with the rest of \
             your work.",
            opening = session_notice_opening(&session),
            title = one_line(&session.title),
        )
    });
}

/// Writes the notice `text` builds, for the owner it is given, into the owner of `session`, or
/// keeps it for an owner that is interrupted (see [`delivery::write_to_owner`]). A failure is
/// logged and goes no further: an owner that is archived, or cannot be written to for another
/// reason, is not owed it later.
///
/// **Blocks** on writing into the owner.
pub(crate) fn tell_owner(
    state: &Arc<AppState>,
    session: &Session,
    text: impl Fn(&Session) -> String,
) {
    let Some(owner_id) = &session.bound_to else {
        return;
    };
    let owner = match state.store.get_session(owner_id) {
        Ok(Some(owner)) => owner,
        _ => return,
    };
    let mut told = write_notice_to_owner(state, &owner, session, &text(&owner));
    // As a report is: the owner waited for may have been handed off and archived meanwhile.
    if is_archived_refusal(&told) {
        if let Ok(Some(new)) = rebound_owner(state, &session.id, &owner.id) {
            told = write_notice_to_owner(state, &new, session, &text(&new));
        }
    }
    if let Err(err) = told {
        tracing::debug!(session = %session.id, owner = %owner.id, %err, "telling the owner failed");
    }
}

/// The tools a project session has, as a worker or not, by the names its agent shows them, for
/// telling a session whose binding has just changed what it has.
fn project_tools(agent: AgentId, worker: bool) -> String {
    catalogue::tools_for(SessionKind::Project, worker)
        .iter()
        .map(|tool| format!("`{}`", catalogue::qualified_tool_name(agent, tool.name)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The sentence that tells a session that has just become a worker that it can no longer start or
/// drive sessions, with the tools it has left; what comes before it says it is a worker.
fn lost_tools(agent: AgentId) -> String {
    format!(
        "You can no longer start or drive sessions: of Octoboard's tools you now have {}, and a \
         call of any other is refused. Any pages you pushed to the report panel were withdrawn \
         with the tools for them.",
        project_tools(agent, true)
    )
}

/// The sentence that tells a session whose parent session has just changed what its role is now:
/// the tools it lost, when the change made a worker of a coordinator, else that its role stays.
/// `was_worker` is whether it was a worker before, which one told at its next launch is not known
/// to have been, so a worker is then told of the tools as one that lost them.
pub(crate) fn role_after_owner_change(session: &Session, was_worker: bool) -> String {
    if !session.is_worker() {
        "Your role remains coordinator: you can create and manage child sessions.".to_string()
    } else if was_worker {
        "Your role remains worker: you cannot create or manage child sessions.".to_string()
    } else {
        format!("Your role is worker. {}", lost_tools(session.agent))
    }
}

/// What a session launched with no parent session is to do now that it has one, in the words of its
/// `agent`'s tool names, to follow a colon after the parent session is named: take work only from
/// it and report to it, what ending a round with `done` does and where items for the parent go,
/// and, for a `coordinator`, that it keeps its child sessions, else the tools it lost. Its role
/// description, recorded at launch, says it is independent, so this says it replaces that.
pub(crate) fn duties_under_new_parent(agent: AgentId, coordinator: bool) -> String {
    let tool = |name: &str| catalogue::qualified_tool_name(agent, name);
    let report = tool("report");
    let ending = if coordinator {
        format!(
            "`done` with no open items archives you, and is refused while a session you started \
             is not archived; `needs_decision` or open items leave you running. {OWNER_ITEMS} You \
             keep your child sessions and go on driving them with `{}` and the tools that go \
             with it.",
            tool("open_session"),
        )
    } else {
        format!(
            "`done` with no open items archives you; `needs_decision` or open items leave you \
             running. {OWNER_ITEMS} {}",
            lost_tools(agent)
        )
    };
    format!(
        "take work only from it, or from the user typing in this terminal, and report to it with \
         `{report}` once per round of work. This replaces anything your role description says \
         about being independent and not calling `{report}`. {ending}"
    )
}

/// Where a session under a parent puts what is the parent's to carry out, said after what ending
/// a round with `done` does.
const OWNER_ITEMS: &str = "Items that are your parent session's to carry out rather than yours \
                           go in `owner_items`, not `open_items`.";

/// The clause that tells a coordinator that its reporting relationship is independent, for a
/// notice to go on to say what that means for it.
pub(crate) const INDEPENDENT_COORDINATOR_ROLE: &str =
    "Your role is coordinator and your reporting relationship is independent";

/// The sentence that tells a session whose it is from now on, in the words of its `agent`'s tool
/// names; `owner` names the parent session, and the sentence begins with it.
pub(crate) fn owner_from_now_on(agent: AgentId, owner: &str) -> String {
    format!(
        "{owner} is your parent session from now on: take work only from it, or from the user \
         typing in this terminal, and report to it with `{report}`.",
        owner = capitalised(owner),
        report = catalogue::qualified_tool_name(agent, "report"),
    )
}

/// The notice to a session whose parent session has changed, the one place that words the part of
/// it every path shares: that the parent session changed, whose session it is from now on, and
/// that this replaces what the session was told when it was started.
///
/// `happened` follows "has changed" with its own punctuation (what became of the change, or " while
/// you were not running."); `owner` names the new parent session, as [`owner_from_now_on`] takes
/// it; `after` follows "when you were started" with its own punctuation and ends the notice (the
/// role the session now has, for one).
pub(crate) fn parent_changed_notice(
    happened: &str,
    agent: AgentId,
    owner: &str,
    after: &str,
) -> String {
    format!(
        "{NOTICE_PREFIX}: your parent session has changed{happened} {owner_now} This replaces \
         what you were told of your parent session when you were started{after}",
        owner_now = owner_from_now_on(agent, owner),
    )
}

/// The notice to an owner that a session reports to it from now on, the one place that words how
/// it drives that session, in the words of `owner_agent`'s tool names.
///
/// `lead` is what the notice says first, ending where "you drive it" follows on; `rest` follows
/// the sentence, with its own leading space.
pub(crate) fn new_owner_notice(owner_agent: AgentId, lead: &str, rest: &str) -> String {
    let tool = |name: &str| catalogue::qualified_tool_name(owner_agent, name);
    format!(
        "{NOTICE_PREFIX}: {lead} you drive it like any session of yours, with \
         `{instruct_session}`, `{archive_session}` and `{resume_session}`.{rest}",
        instruct_session = tool("instruct_session"),
        archive_session = tool("archive_session"),
        resume_session = tool("resume_session"),
    )
}

/// The notice to `moved`, a session its owner `from` transferred to `to`, which is no longer
/// `from`'s. `was_worker` is whether it was a worker before (see [`role_after_owner_change`]).
pub(crate) fn transferred_to_you(
    from: &Session,
    to: &Session,
    moved: &Session,
    was_worker: bool,
) -> String {
    parent_changed_notice(
        &format!(
            ". {from} transferred you to {to}.",
            from = capitalised(&named_session(from)),
            to = named_session(to),
        ),
        moved.agent,
        "It",
        &format!(
            "; `{}` no longer drives you. {}",
            from.id,
            role_after_owner_change(moved, was_worker)
        ),
    )
}

/// The notice to `owner`, the session `from` transferred `moved` to, with `from`'s `note`, quoted.
pub(crate) fn transferred_to_owner(
    state: &Arc<AppState>,
    from: &Session,
    owner: &Session,
    moved: &Session,
    note: &str,
) -> String {
    let role_note = if moved.role == Role::Coordinator {
        " Its role is coordinator. It keeps its child sessions, which you can read but not drive."
    } else {
        " Its role is worker and it cannot manage child sessions."
    };
    new_owner_notice(
        owner.agent,
        &format!(
            "{from} transferred its session {moved}{place} to you. It is yours from now on: it \
             reports to you, and",
            from = named_session(from),
            moved = label(moved),
            place = place_of(state, moved),
        ),
        &format!(
            "{role_note} The transfer note:\n\n{}\n— end of the transfer note —",
            quote(note)
        ),
    )
}

/// What Octoboard says with the first report a project session that bound itself to an owner
/// sends it, ahead of the session's own summary: what it now is to the owner. `coordinator` is
/// whether it is a coordinator, which keeps the sessions it started, else a worker.
pub(crate) fn first_report_on_bind(coordinator: bool) -> &'static str {
    if coordinator {
        "This project coordinator set you as its parent session: it is yours to drive from now \
         on, and reports to you. It keeps the sessions it started in its own project and drives \
         them itself; you can read those but not drive them. Its summary of what it is doing and \
         needs follows."
    } else {
        "This project session is now a worker reporting to you: it is yours to drive from now \
         on, and reports to you. Its summary of what it is doing and needs follows."
    }
}

/// Whether `session` has a process to write into. Decided by the one who changes its owner, while
/// it holds the claim a launch needs, and handed to [`tell_now_or_leave`]; a session with none is
/// left owed a notice for its next launch (see [`owner_notice`]). Also what decides whether a
/// former owner is told of a release by the user ([`tell_former_owner`]).
pub(crate) fn is_running(state: &Arc<AppState>, session: &Session) -> bool {
    !session.status.has_no_process() && state.live_session(&session.id).is_some()
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

/// What a release leaves with the notice it owes: who the former owner was and what it said in
/// parting, if anything.
#[derive(Clone, Copy)]
pub(crate) struct Parting<'a> {
    pub(crate) former_owner: &'a str,
    pub(crate) note: Option<&'a str>,
}

/// Writes `text` into the session `id`, queued while it cannot take a message, when `running` says
/// it has a process (see [`running_or_leave`]); `None` is a session left to be told when it next
/// starts. A write that fails, a process that ended meanwhile say, leaves the notice for the next
/// launch too: it is built then from the binding as it stands, so telling twice does no harm. The
/// `parting` of a release is kept with the notice that is left owed, as long as the session is
/// still unbound. One its agent's own queue may have taken without confirming it
/// ([`delivery::acceptance_uncertain`]) is returned as that error and left owed by nothing.
pub(crate) async fn tell_now_or_leave(
    state: &Arc<AppState>,
    id: &str,
    text: String,
    running: bool,
    parting: Option<Parting<'_>>,
) -> Result<Option<Delivery>> {
    if !running {
        return Ok(None);
    }
    match delivery::write_message_off_runtime(state, id, text, WhenBlocked::Queue).await {
        Ok(delivery) => Ok(Some(delivery)),
        // It may have arrived, so it is not told again at the next launch; the caller says what is
        // known of it.
        Err(err) if delivery::acceptance_uncertain(&err) => Err(err),
        Err(err) => {
            tracing::debug!(session = %id, %err, "telling a session of its parent session failed");
            match parting {
                Some(parting) => {
                    state
                        .store
                        .mark_released_notice(id, parting.former_owner, parting.note)?
                }
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
        let former_owner = state
            .store
            .owner_notice_former_owner(&session.id)?
            .map(|id| named_or_id(state, &id));
        return Ok(released_text(
            session,
            former_owner.as_deref(),
            note.as_deref(),
        ));
    };
    // An owner no longer on record is named by its id alone, as a report to it would be.
    let owner = match state.store.get_session(owner_id)? {
        Some(owner) => named_session(&owner),
        None => format!("`{owner_id}`"),
    };
    Ok(parent_changed_notice(
        " while you were not running.",
        session.agent,
        &owner,
        &format!(". {}", role_after_owner_change(session, false)),
    ))
}

/// `id` as a message names a session: by its kind, id and title when its record can be read, else
/// by its id alone.
fn named_or_id(state: &Arc<AppState>, id: &str) -> String {
    match state.store.get_session(id) {
        Ok(Some(session)) => named_session(&session),
        _ => format!("the session `{id}`"),
    }
}

/// Tells the project session `released`, just released from its owner `former_owner_id`, that it
/// has none: its role description, recorded when it was launched, still says whose it is. Queued
/// while the session cannot take a message; one with no process is told when it next starts (see
/// [`owner_notice`]), with the former owner's parting `note` if the release stored one. The notice
/// sent now names the former parent session, by its id alone when its record cannot be read.
pub(crate) async fn tell_released(
    state: &Arc<AppState>,
    released: &Session,
    former_owner_id: &str,
    running: bool,
    note: Option<&str>,
) -> Result<Option<Delivery>> {
    let former_owner = running.then(|| named_or_id(state, former_owner_id));
    tell_now_or_leave(
        state,
        &released.id,
        released_text(released, former_owner.as_deref(), note),
        running,
        Some(Parting {
            former_owner: former_owner_id,
            note,
        }),
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
         no parent session now and answers only to the user, so you can no longer instruct, \
         drive or archive it, and it does not report to you. You can still tell it something \
         with `{inform_session}`, which it weighs rather than obeys. Do not count on it any more, \
         and treat any work you handed it as left with the user.",
        released = named_session(released),
        inform_session = catalogue::qualified_tool_name(owner.agent, "inform_session"),
    );
    delivery::write_message_off_runtime(state, owner_id, text, WhenBlocked::Queue)
        .await
        .map(Some)
}

/// The notice of a release, naming the `former_owner` (as [`named_or_id`] words it) when it is
/// known and ending with its parting `note` when there is one.
fn released_text(released: &Session, former_owner: Option<&str>, note: Option<&str>) -> String {
    let tool = |name: &str| catalogue::qualified_tool_name(released.agent, name);
    let note = match note {
        Some(note) => format!(
            " Your former parent session left this note:\n\n{note}\n— end of the note —",
            note = quote(note),
        ),
        None => String::new(),
    };
    let former = match former_owner {
        Some(owner) => format!("{owner}, the session"),
        None => "the session".to_string(),
    };
    format!(
        "{NOTICE_PREFIX}: you have no parent session now. {INDEPENDENT_COORDINATOR_ROLE}. You \
         were released from {former} you reported to, and you are the user's own session: take \
         work only from the user typing in this terminal, and do not call `{report}`, which is \
         refused for a session with no parent session. This replaces what you were told of your \
         parent session when you were started. You can start and drive sessions of your own with \
         `{open_session}` and the tools that go with it (call `{get_session}` once with no \
         arguments first, so that your tool list catches up), and you can still tell your former \
         parent session something with `{inform_session}`, which it weighs rather than obeys; of \
         Octoboard's tools you now have {tools}.{note}",
        report = tool("report"),
        get_session = tool("get_session"),
        open_session = tool("open_session"),
        inform_session = tool("inform_session"),
        tools = project_tools(released.agent, false),
    )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::messaging::texts::{is_waiting_notice, waiting_notice_session};
    use crate::protocol::Role;
    use crate::test_support::owner_team::{
        hub_heard, hub_heard_of, interrupted_hub_state, kept_texts, project_session, team_state,
    };

    /// A stand-in process for the session `worker` of `team_state`, registered and watched for its
    /// exit as a launched one is.
    fn watched_worker(state: &Arc<AppState>) -> crate::test_support::StandIn {
        let live = crate::test_support::idle_stand_in("worker");
        state.register_live(live.clone());
        crate::coordinator::exit_watch::watch_exit(state, live.clone());
        live
    }

    /// Waits until the exit watcher has recorded the end of the session `id`.
    async fn recorded_ended(state: &Arc<AppState>, id: &str) {
        let (state, id) = (state.clone(), id.to_string());
        let ended = crate::test_support::within(crate::test_support::PATIENCE, move || {
            state
                .store
                .get_session(&id)
                .unwrap()
                .is_some_and(|session| session.ended_at.is_some())
        })
        .await;
        assert!(ended, "the exit was recorded");
    }

    /// A process that ends by itself, with nobody having archived the session, is told to its
    /// owner as an end nobody asked for, naming the session and the tools to look at it and
    /// relaunch it by.
    #[tokio::test]
    async fn an_owner_is_told_when_its_session_ends_on_its_own() {
        let (state, _dir, hub) = team_state(
            "reporting-unexpected-exit",
            &[("worker", "hub", false, SessionStatus::Working)],
        );
        let worker = watched_worker(&state);

        worker.terminate();
        recorded_ended(&state, "worker").await;
        // The notice is written off the watcher after the record, so a marker could overtake it.
        hub_heard_of(&hub, "ended on its own").await;

        let heard = hub_heard(&state, &hub).await;
        assert!(heard.contains("ended on its own"), "{heard}");
        assert!(heard.contains("`worker`"), "{heard}");
        assert!(
            heard.contains("`mcp__octoboard__resume_session`"),
            "{heard}"
        );
        assert_eq!(heard.matches("ended on its own").count(), 1, "{heard}");
        assert_eq!(
            state.session_record("worker").unwrap().status,
            SessionStatus::Interrupted
        );
    }

    /// An end somebody asked for is not told: an archive, a daemon going down (which has marked
    /// the session interrupted already) and an operation that stops the process itself, as an
    /// account switch does. A session that reports to nobody has no one to tell.
    #[tokio::test]
    async fn an_owner_is_not_told_of_an_end_somebody_asked_for() {
        let (state, _dir, hub) = team_state(
            "reporting-deliberate-exit",
            &[
                ("worker", "hub", false, SessionStatus::Working),
                ("claimed", "hub", false, SessionStatus::Working),
                ("going-down", "hub", false, SessionStatus::Working),
            ],
        );
        let alone = Session {
            id: "alone".to_string(),
            bound_to: None,
            status: SessionStatus::Working,
            ..project_session("alone")
        };
        state.store.insert_session(&alone).unwrap();
        let mut ending = Vec::new();
        for id in ["worker", "claimed", "going-down", "alone"] {
            let live = crate::test_support::idle_stand_in(id);
            state.register_live(live.clone());
            crate::coordinator::exit_watch::watch_exit(&state, live.clone());
            ending.push(live);
        }
        let mut archived = state.session_record("worker").unwrap();
        archived.status = SessionStatus::Archived;
        state.store.update_session(&archived).unwrap();
        let _claim = state.begin_account_switch("claimed", false).unwrap();
        let mut interrupted = state.session_record("going-down").unwrap();
        interrupted.status = SessionStatus::Interrupted;
        state.store.update_session(&interrupted).unwrap();

        for live in &ending {
            // What an account switch's stop does; archive and shutdown mark the record first.
            live.end();
        }
        for live in &ending {
            crate::test_support::exit_settled(live).await;
        }

        let heard = hub_heard(&state, &hub).await;
        assert!(!heard.contains("ended on its own"), "{heard}");
    }

    /// A claim on the session says nothing of how its process ended: one that exits by itself
    /// while an operation holds the claim, including one that stops no process (a transfer of
    /// branches, a binding change), is told to the owner as an end nobody asked for, and so is a
    /// process the account switch found already gone when it stopped it.
    #[tokio::test]
    async fn an_owner_is_told_of_a_process_that_ended_by_itself_under_a_claim() {
        let (state, _dir, hub) = team_state(
            "reporting-exit-under-claim",
            &[("worker", "hub", false, SessionStatus::Working)],
        );
        let worker = watched_worker(&state);
        let _claim = state.begin_switch("worker").unwrap();

        worker.terminate();
        recorded_ended(&state, "worker").await;

        // The notice is written off the watcher after the record, so a marker could overtake it.
        let heard = hub_heard_of(&hub, "ended on its own").await;
        assert!(heard.contains("ended on its own"), "{heard}");
        assert!(!worker.end(), "the process was gone before the stop");
        assert!(!worker.was_ended_on_purpose());
    }

    /// An owner is told once a turn that its session waits for the user, and told it is not to
    /// chase it; the next turn tells it again.
    #[tokio::test]
    async fn an_owner_is_told_once_a_turn_that_its_session_waits_for_the_user() {
        let (state, _dir, hub) = team_state(
            "reporting-waiting",
            &[("worker", "hub", false, SessionStatus::WaitingUser)],
        );

        tell_owner_it_waits(&state, "worker");
        tell_owner_it_waits(&state, "worker");
        let heard = hub_heard(&state, &hub).await;
        assert!(heard.contains("is waiting for the user"), "{heard}");
        assert!(heard.contains("Do not chase it"), "{heard}");
        assert_eq!(
            heard.matches("is waiting for the user").count(),
            1,
            "{heard}"
        );

        state.turn_started("worker", Some("go on"));
        tell_owner_it_waits(&state, "worker");
        let heard = hub_heard(&state, &hub).await;
        assert_eq!(
            heard.matches("is waiting for the user").count(),
            2,
            "{heard}"
        );
    }

    /// A session that reported earlier in its turn and then stops at a permission prompt is told
    /// to its owner as waiting: its report did not say so. A wait before the first turn, a trust
    /// prompt say, does not use up the notice of the turn that follows.
    #[tokio::test]
    async fn a_wait_after_a_report_or_before_the_first_turn_is_told() {
        let (state, _dir, hub) = team_state(
            "reporting-waiting-after-report",
            &[("worker", "hub", false, SessionStatus::WaitingUser)],
        );

        tell_owner_it_waits(&state, "worker");
        state.turn_started("worker", Some("go on"));
        state.mark_reported("worker");
        tell_owner_it_waits(&state, "worker");

        let heard = hub_heard(&state, &hub).await;
        assert_eq!(
            heard.matches("is waiting for the user").count(),
            2,
            "{heard}"
        );
    }

    /// A wait the terminal shows (a dialog of the agent), with the record still at its prompt, is
    /// told to the owner as well, and the notice is not repeated for the same turn.
    #[tokio::test]
    async fn an_owner_is_told_of_a_wait_only_the_screen_shows() {
        let (state, _dir, hub) = team_state(
            "reporting-waiting-screen",
            &[("worker", "hub", false, SessionStatus::Idle)],
        );
        let worker = crate::test_support::running(&state, "worker");

        tell_owner_it_waits(&state, "worker");
        let heard = hub_heard(&state, &hub).await;
        assert!(!heard.contains("is waiting for the user"), "{heard}");

        worker.on_output(
            "\n  Resume this conversation?\n  This conversation has been inactive for 1h and is \
             42k tokens long.\n  \u{276f} Resume\n    Start a new conversation\n\n  Enter to \
             confirm \u{b7} Esc to dismiss\n"
                .as_bytes(),
        );
        assert!(worker.look_for_dialog().current.is_some());
        tell_owner_it_waits(&state, "worker");
        tell_owner_it_waits(&state, "worker");
        let heard = hub_heard(&state, &hub).await;
        assert_eq!(
            heard.matches("is waiting for the user").count(),
            1,
            "{heard}"
        );
    }

    /// The routes by which a session comes to wait for the user without a hook saying so each tell
    /// its owner: a dialog the agent puts on the screen, and Codex's "Action Required" title.
    #[tokio::test]
    async fn an_owner_is_told_when_a_dialog_or_an_action_title_makes_its_session_wait() {
        let (state, _dir, hub) = team_state("reporting-waiting-routes", &[]);
        for (id, agent) in [("dialog", AgentId::CLAUDE), ("title", AgentId::CODEX)] {
            state
                .store
                .insert_session(&Session {
                    id: id.to_string(),
                    agent,
                    bound_to: Some("hub".to_string()),
                    role: Role::Worker,
                    status: SessionStatus::Idle,
                    ..project_session(id)
                })
                .unwrap();
        }
        let dialog = crate::test_support::running(&state, "dialog");
        let title =
            crate::test_support::fake_live_session("title", AgentId::CODEX, 80, 24, "sleep 30");
        state.register_live(title.clone());
        crate::session::spawn_reader_thread(title.clone(), 8 * 1024);

        crate::session::dialogs::watch(&state, &dialog);
        dialog.on_output(
            "\n  Resume this conversation?\n  This conversation has been inactive for 1h and is \
             42k tokens long.\n  \u{276f} Resume\n    Start a new conversation\n\n  Enter to \
             confirm \u{b7} Esc to dismiss\n"
                .as_bytes(),
        );
        let heard = hub_heard_of(&hub, "`dialog`").await;
        assert!(heard.contains("`dialog`"), "{heard}");

        title.on_output(b"\x1b]0;[ ! ] Action Required | repo\x07");
        let heard = hub_heard_of(&hub, "`title`").await;
        assert!(heard.contains("`title`"), "{heard}");
    }

    /// Nothing is said of a wait when the session has no owner or no process to wait in.
    #[tokio::test]
    async fn an_owner_is_not_told_of_a_wait_that_is_not_one() {
        let (state, _dir, hub) = team_state(
            "reporting-waiting-not-told",
            &[("down", "hub", false, SessionStatus::Interrupted)],
        );
        let alone = Session {
            id: "alone".to_string(),
            bound_to: None,
            status: SessionStatus::WaitingUser,
            ..project_session("alone")
        };
        state.store.insert_session(&alone).unwrap();

        for id in ["alone", "down"] {
            tell_owner_it_waits(&state, id);
        }

        let heard = hub_heard(&state, &hub).await;
        assert!(!heard.contains("is waiting for the user"), "{heard}");
    }

    /// An owner under a claim takes nothing kept and has no process, but what is sent to it is an
    /// event that will not come again: the notice waits out the claim and is then kept.
    #[test]
    fn a_notice_waits_out_a_claim_on_an_interrupted_owner_and_is_kept() {
        let (state, _dir) = interrupted_hub_state("reporting-notice-under-claim");
        let claim = state.begin_launch("hub").unwrap();
        let releasing = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            drop(claim);
        });

        let worker = state.session_record("worker").unwrap();
        tell_owner_it_ended(&state, &worker);
        releasing.join().unwrap();

        let kept = kept_texts(&state);
        assert_eq!(kept.len(), 1, "{kept:?}");
        assert!(kept[0].contains("ended on its own"), "{kept:?}");
    }

    /// The kept wait of a session is found again whatever its title was then, so a rename between
    /// two waits does not leave the older one behind.
    #[test]
    fn a_renamed_sessions_second_wait_drops_the_first() {
        let (state, _dir) = interrupted_hub_state("reporting-wait-renamed");

        tell_owner_it_waits(&state, "worker");
        let mut worker = state.session_record("worker").unwrap();
        worker.title = "Renamed".to_string();
        state.store.update_session(&worker).unwrap();
        state.turn_started("worker", Some("go on"));
        tell_owner_it_waits(&state, "worker");

        let kept = kept_texts(&state);
        assert_eq!(kept.len(), 1, "{kept:?}");
        assert!(kept[0].contains("(\"Renamed\") is waiting"), "{kept:?}");
        // What the notice is built of is what the matching looks for.
        assert!(is_waiting_notice(&kept[0], &worker));
        assert_eq!(waiting_notice_session(&kept[0]), Some("worker"));
        assert_eq!(
            waiting_notice_session("Message from Octoboard: other"),
            None
        );
    }

    /// The notices of an automatic account switch and of a quota recovery, word for word: an
    /// owner reads them for what to do next, and whether to send the session a prompt itself. The
    /// tool the owner is pointed to is named in its own agent's words (a Grok owner, a Claude
    /// session), not the session's.
    #[test]
    fn the_notices_of_an_automatic_switch_and_a_recovery_are_worded_as_fixed() {
        let session = project_session("Fix the \"login\"\nretry");
        let owner = Session {
            id: "hub".to_string(),
            agent: AgentId::GROK,
            ..project_session("hub")
        };
        assert_eq!(
            it_was_switched(&owner, &session, "Work A", "Work B", true),
            "Message from Octoboard: the project session `session-7` (\"Fix the \"login\" retry\") \
             reached the usage limit of its Claude Code account `Work A`, so Octoboard switched it \
             to `Work B` by itself. Its conversation came with it. Its turn had stopped on the \
             limit, so it was sent a prompt to continue that task: do not send it one yourself."
        );
        assert_eq!(
            it_was_switched(&owner, &session, "Work A", "Work B", false),
            "Message from Octoboard: the project session `session-7` (\"Fix the \"login\" retry\") \
             reached the usage limit of its Claude Code account `Work A`, so Octoboard switched it \
             to `Work B` by itself. Its conversation came with it. It is back awaiting \
             instructions, so send it the next step with `octoboard__instruct_session` if it has \
             one pending."
        );
        assert_eq!(
            resumed_on_recovery(&session, true),
            "Message from Octoboard: the project session `session-7` (\"Fix the \"login\" retry\") \
             resumed automatically after its account's usage quota recovered. It was sent a prompt \
             to continue; do not send it one yourself."
        );
        assert_eq!(
            resumed_on_recovery(&session, false),
            "Message from Octoboard: the project session `session-7` (\"Fix the \"login\" retry\") \
             resumed automatically after its account's usage quota recovered. It is awaiting \
             instructions."
        );
        assert_eq!(
            will_be_retried(&session, 2, 5, 30),
            "Message from Octoboard: the model of the project session `session-7` (\"Fix the \
             \"login\" retry\") was at capacity, so its turn stopped. Octoboard will retry \
             automatically in 30 seconds (retry 2 of 5) in the same session; do not send it a \
             continue prompt yourself."
        );
        assert_eq!(
            recovery_failed(&session, "the launch failed"),
            "Message from Octoboard: the project session `session-7` (\"Fix the \"login\" retry\") \
             could not resume after its account's usage quota recovered. It is interrupted and \
             its work is stopped. Reason: the launch failed"
        );
    }

    /// What a session is told of its role when its parent session changed while it was not
    /// running, or when it was released, word for word as far as the role goes.
    #[tokio::test]
    async fn a_session_told_of_a_new_parent_or_a_release_is_told_its_role_word_for_word() {
        let (state, _dir, _hub) = team_state(
            "owner-notice-role-words",
            &[("coordinator", "hub", true, SessionStatus::Interrupted)],
        );
        let coordinator = state.session_record("coordinator").unwrap();
        assert_eq!(
            owner_notice(&state, &coordinator).unwrap(),
            "Message from Octoboard: your parent session has changed while you were not running. \
             The console session `hub` (\"hub\") is your parent session from now on: take work \
             only from it, or from the user typing in this terminal, and report to it with \
             `mcp__octoboard__report`. This replaces what you were told of your parent session \
             when you were started. Your role remains coordinator: you can create and manage \
             child sessions."
        );
        let released = Session {
            bound_to: None,
            ..coordinator
        };
        let text = released_text(&released, None, None);
        assert!(
            text.starts_with(
                "Message from Octoboard: you have no parent session now. Your role is coordinator \
                 and your reporting relationship is independent. You were released from the \
                 session you reported to, and you are the user's own session: "
            ),
            "{text}"
        );
        let hub = state.session_record("hub").unwrap();
        let text = released_text(&released, Some(&named_session(&hub)), None);
        assert!(
            text.starts_with(
                "Message from Octoboard: you have no parent session now. Your role is coordinator \
                 and your reporting relationship is independent. You were released from the \
                 console session `hub` (\"hub\"), the session you reported to, and you are the \
                 user's own session: "
            ),
            "{text}"
        );
        let text = released_text(&released, Some("the session `gone`"), None);
        assert!(
            text.starts_with(
                "Message from Octoboard: you have no parent session now. Your role is coordinator \
                 and your reporting relationship is independent. You were released from the \
                 session `gone`, the session you reported to, and you are the user's own session: "
            ),
            "{text}"
        );
    }

    /// The parent-changed notice is worded the same on every path that builds it: a handoff of the
    /// session's parent and the wording of a transfer by the parent, each ending as its own path
    /// does.
    #[test]
    fn the_parent_changed_notice_is_worded_as_fixed_for_each_path() {
        let moved = project_session("child");
        assert_eq!(
            parent_changed_notice(
                ". the console session `a` (\"A\") was handed off to the console session `b` \
                 (\"B\"), which took its place.",
                moved.agent,
                "It",
                ". `a` no longer drives you; it is archived.",
            ),
            "Message from Octoboard: your parent session has changed. the console session `a` \
             (\"A\") was handed off to the console session `b` (\"B\"), which took its place. It \
             is your parent session from now on: take work only from it, or from the user typing \
             in this terminal, and report to it with `mcp__octoboard__report`. This replaces what \
             you were told of your parent session when you were started. `a` no longer drives \
             you; it is archived."
        );
        assert_eq!(
            parent_changed_notice(
                ". the console session `a` (\"A\") transferred you to the console session `b` \
                 (\"B\").",
                moved.agent,
                "It",
                "; `a` no longer drives you. Your role remains coordinator: you can create and \
                 manage child sessions.",
            ),
            "Message from Octoboard: your parent session has changed. the console session `a` \
             (\"A\") transferred you to the console session `b` (\"B\"). It is your parent session \
             from now on: take work only from it, or from the user typing in this terminal, and \
             report to it with `mcp__octoboard__report`. This replaces what you were told of your \
             parent session when you were started; `a` no longer drives you. Your role remains \
             coordinator: you can create and manage child sessions."
        );
    }

    /// The new-owner notice is worded the same on every path that builds it, with the owner's
    /// agent's tool names.
    #[test]
    fn the_new_owner_notice_is_worded_as_fixed_for_each_path() {
        assert_eq!(
            new_owner_notice(
                AgentId::GROK,
                "`a` transferred its session `s` (\"S\") to you. It is yours from now on: it \
                 reports to you, and",
                " Its role is worker and it cannot manage child sessions.",
            ),
            "Message from Octoboard: `a` transferred its session `s` (\"S\") to you. It is yours \
             from now on: it reports to you, and you drive it like any session of yours, with \
             `octoboard__instruct_session`, `octoboard__archive_session` and \
             `octoboard__resume_session`. Its role is worker and it cannot manage child sessions."
        );
        assert_eq!(
            new_owner_notice(
                AgentId::CLAUDE,
                "`a`, which reported to you, was handed off to a session of Codex. `b` took its \
                 place and reports to you from now on:",
                " The original is archived.",
            ),
            "Message from Octoboard: `a`, which reported to you, was handed off to a session of \
             Codex. `b` took its place and reports to you from now on: you drive it like any \
             session of yours, with `mcp__octoboard__instruct_session`, \
             `mcp__octoboard__archive_session` and `mcp__octoboard__resume_session`. The original \
             is archived."
        );
    }

    /// The notices of a transfer by the owner are worded as fixed, to the session and to its new
    /// owner, and a bind's first report likewise.
    #[test]
    fn the_notices_of_a_transfer_and_a_bind_are_worded_as_fixed() {
        let (state, _dir, _hub) = team_state("owner-notice-transfer-words", &[]);
        let console_session = |id: &str, title: &str| Session {
            id: id.to_string(),
            kind: SessionKind::Console,
            bound_to: None,
            ..project_session(title)
        };
        let (from, owner) = (console_session("a", "A"), console_session("b", "B"));
        let moved = Session {
            title: "Fix\nthe build".to_string(),
            role: Role::Coordinator,
            ..project_session("S")
        };
        assert_eq!(
            transferred_to_you(&from, &owner, &moved, true),
            "Message from Octoboard: your parent session has changed. The console session `a` \
             (\"A\") transferred you to the console session `b` (\"B\"). It is your parent \
             session from now on: take work only from it, or from the user typing in this \
             terminal, and report to it with `mcp__octoboard__report`. This replaces what you \
             were told of your parent session when you were started; `a` no longer drives you. \
             Your role remains coordinator: you can create and manage child sessions."
        );
        assert_eq!(
            transferred_to_owner(&state, &from, &owner, &moved, "Half done.\nNext: tests."),
            "Message from Octoboard: the console session `a` (\"A\") transferred its session \
             `session-7` (\"Fix the build\") in the console \"Console\" to you. It is yours \
             from now on: it reports to you, and you drive it like any session of yours, with \
             `mcp__octoboard__instruct_session`, `mcp__octoboard__archive_session` and \
             `mcp__octoboard__resume_session`. Its role is coordinator. It keeps its child \
             sessions, which you can read but not drive. The transfer note:\n\n> Half done.\n> \
             Next: tests.\n— end of the transfer note —"
        );
        let worker = Session {
            role: Role::Worker,
            ..moved
        };
        assert!(transferred_to_owner(&state, &from, &owner, &worker, "n")
            .contains("Its role is worker and it cannot manage child sessions. The transfer note"));
        assert_eq!(
            first_report_on_bind(true),
            "This project coordinator set you as its parent session: it is yours to drive from \
             now on, and reports to you. It keeps the sessions it started in its own project and \
             drives them itself; you can read those but not drive them. Its summary of what it \
             is doing and needs follows."
        );
        assert_eq!(
            first_report_on_bind(false),
            "This project session is now a worker reporting to you: it is yours to drive from now \
             on, and reports to you. Its summary of what it is doing and needs follows."
        );
    }
}
