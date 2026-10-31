//! The notices Octoboard writes about who a session reports to: to an owner about one of its
//! sessions — it ended on its own, it waits for the user, its switch to another account failed —
//! and to a session whose owner changed, or to the owner that lost it, with whose it is now and the
//! tools that leaves it. A session that was told at launch who it reports to is not told again by
//! anything else, so one with no process when its owner changes is left owed its notice, built at
//! its next launch from the binding as it stands then.

use std::sync::Arc;

use anyhow::Result;

use crate::delivery::{
    self, is_archived_refusal, rebound_owner, write_notice_to_owner, Delivery, WhenBlocked,
};
use crate::protocol::{Agent, Role, Session, SessionStatus};
use crate::session_texts::{waiting_opening, NOTICE_PREFIX, WAITING_PHRASE};
use crate::state::AppState;
use crate::{mcp, sharing};

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
             `{reopen_session}` if the work should go on.",
            session = sharing::named_session(session),
            get_session = mcp::qualified_tool_name(owner.agent, "get_session"),
            reopen_session = mcp::qualified_tool_name(owner.agent, "reopen_session"),
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
             with `{get_session}`, and relaunch it with `{reopen_session}` if the work should go \
             on.",
            session = sharing::named_session(session),
            get_session = mcp::qualified_tool_name(owner.agent, "get_session"),
            reopen_session = mcp::qualified_tool_name(owner.agent, "reopen_session"),
        )
    });
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
            opening = waiting_opening(&session),
            title = sharing::one_line(&session.title),
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
fn project_tools(agent: Agent, worker: bool) -> String {
    mcp::tools_for(Role::Project, worker)
        .iter()
        .map(|tool| format!("`{}`", mcp::qualified_tool_name(agent, tool.name)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The sentence that tells a session it can no longer start or drive sessions, with the tools it
/// has left.
pub(crate) fn lost_tools(agent: Agent) -> String {
    format!(
        "Your role is worker. You can no longer start or drive sessions: of Octoboard's tools \
         you now have {}, and a call of any other is refused. Any pages you pushed to the \
         report panel were withdrawn with the tools for them.",
        project_tools(agent, true)
    )
}

/// The sentence that tells a session whose it is from now on, in the words of its `agent`'s tool
/// names.
pub(crate) fn owner_from_now_on(agent: Agent, owner: &str) -> String {
    format!(
        "{owner} is your parent session from now on: take work only from it, or from the user \
         typing in this terminal, and report to it with `{report}`.",
        report = mcp::qualified_tool_name(agent, "report"),
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
    match delivery::write_message_off_runtime(state, id, text, WhenBlocked::Queue).await {
        Ok(delivery) => Ok(Some(delivery)),
        Err(err) => {
            tracing::debug!(session = %id, %err, "telling a session of its parent session failed");
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
    let tools = if session.is_worker() {
        format!(" {}", lost_tools(session.agent))
    } else {
        " Your role is coordinator: you can create and manage child sessions.".to_string()
    };
    Ok(format!(
        "{NOTICE_PREFIX}: your parent session has changed while you were not running. {owner} \
         This replaces what you were told of your parent session when you were started.{tools}",
        owner = owner_from_now_on(session.agent, &owner),
    ))
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
         no parent session now and answers only to the user, so you can no longer instruct, \
         drive or archive it, and it does not report to you. You can still tell it something \
         with `{send_message}`, which it weighs rather than obeys. Do not count on it any more, \
         and treat any work you handed it as left with the user.",
        released = sharing::named_session(released),
        send_message = mcp::qualified_tool_name(owner.agent, "send_message"),
    );
    delivery::write_message_off_runtime(state, owner_id, text, WhenBlocked::Queue)
        .await
        .map(Some)
}

/// The notice of a release, ending with the former owner's parting `note` when there is one.
fn released_text(released: &Session, note: Option<&str>) -> String {
    let tool = |name: &str| mcp::qualified_tool_name(released.agent, name);
    let note = match note {
        Some(note) => format!(
            " Your former parent session left this note:\n\n{note}\n— end of the note —",
            note = sharing::quote(note),
        ),
        None => String::new(),
    };
    format!(
        "{NOTICE_PREFIX}: you have no parent session now. Your role is coordinator and your \
         reporting relationship is independent. You were released from the session you reported \
         to, and you are the user's own session: take work only from the user typing in this \
         terminal, and do not call `{report}`, which is refused for a session with no parent \
         session. This replaces what you were told of your parent session when you were \
         started. You can start and drive sessions of your own with `{new_session}` and the \
         tools that go with it (call `{get_session}` once with no arguments first, so that your \
         tool list catches up), and you can still tell your former parent session something \
         with `{send_message}`, which it weighs rather than obeys; of Octoboard's tools you now \
         have {tools}.{note}",
        report = tool("report"),
        get_session = tool("get_session"),
        new_session = tool("new_session"),
        send_message = tool("send_message"),
        tools = project_tools(released.agent, false),
    )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::protocol::OrchestrationRole;
    use crate::session_texts::{is_waiting_notice, waiting_notice_session};
    use crate::test_support::owner_team::{
        hub_heard, hub_heard_of, interrupted_hub_state, kept_texts, project_session, team_state,
    };

    /// A stand-in process for the session `worker` of `team_state`, registered and watched for its
    /// exit as a launched one is.
    fn watched_worker(state: &Arc<AppState>) -> crate::test_support::StandIn {
        let live = crate::test_support::idle_stand_in("worker");
        state.register_live(live.clone());
        state.watch_exit(live.clone());
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
            heard.contains("`mcp__octoboard__reopen_session`"),
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
            state.watch_exit(live.clone());
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
    /// while an operation holds the claim, including one that stops no process (a handover of
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
        for (id, agent) in [("dialog", Agent::Claude), ("title", Agent::Codex)] {
            state
                .store
                .insert_session(&Session {
                    id: id.to_string(),
                    agent,
                    bound_to: Some("hub".to_string()),
                    orchestration_role: OrchestrationRole::Worker,
                    status: SessionStatus::Idle,
                    ..project_session(id)
                })
                .unwrap();
        }
        let dialog = crate::test_support::running(&state, "dialog");
        let title =
            crate::test_support::fake_live_session("title", Agent::Codex, 80, 24, "sleep 30");
        state.register_live(title.clone());
        crate::session::spawn_reader_thread(title.clone(), 8 * 1024);

        crate::agent_dialogs::watch(&state, &dialog);
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
}
