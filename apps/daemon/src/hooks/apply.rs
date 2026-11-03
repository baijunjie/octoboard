//! What applying a hook event does to the daemon: the queued events of a session applied in order,
//! and for each the session's status, turns, background tasks and record watches it moves.
//! What a payload says is read by the parent module.

use std::sync::Arc;

use crate::hooks::{self, BackgroundTasksChange};
use crate::messaging::reporting;
use crate::protocol::{Agent, SessionStatus};
use crate::records::watch::claude_transcript;
use crate::records::watch::codex_rollout;
use crate::state::{AppState, TurnClose};

/// Applies a session's queued hook events one after another until none is left. Applying one is
/// ordinary blocking work on the store.
pub fn drain_hook_queue(state: &Arc<AppState>, session_id: &str) {
    /// Releases the queue if applying an event panics, so the session's later events start a worker
    /// of their own instead of waiting on one that is gone.
    struct Abandon<'a> {
        state: &'a AppState,
        session_id: &'a str,
        drained: bool,
    }
    impl Drop for Abandon<'_> {
        fn drop(&mut self) {
            if !self.drained {
                self.state.hook_queues.abandon(self.session_id);
            }
        }
    }
    let mut guard = Abandon {
        state,
        session_id,
        drained: false,
    };
    while let Some(body) = state.hook_queues.next(session_id) {
        apply_hook(state, session_id, &body);
        if !state.hook_queues.complete(session_id) {
            break;
        }
    }
    guard.drained = true;
}

/// The agent a hook's session runs, live or recorded.
fn hook_agent(state: &Arc<AppState>, session_id: &str) -> Option<Agent> {
    state
        .live_session(session_id)
        .map(|live| live.agent)
        .or_else(|| {
            state
                .store
                .get_session(session_id)
                .ok()
                .flatten()
                .map(|session| session.agent)
        })
}

/// Applies one hook event to its session. The event name comes from the payload, because the hook
/// script is argument-free.
fn apply_hook(state: &Arc<AppState>, session_id: &str, body: &[u8]) {
    let payload: serde_json::Value =
        serde_json::from_slice(body).unwrap_or(serde_json::Value::Null);
    let event = hooks::event_name(&payload);

    // An event of the turn the end of the session's process cut short comes from the ended
    // process, posted after the end by a hook the agent did not wait for: it is that turn's
    // late word, and nothing of it reaches whatever runs in the session now.
    let agent = hook_agent(state, session_id);
    let is_subagent = hooks::is_subagent(agent, &payload);
    let turn_key = agent
        .filter(|_| !is_subagent)
        .and_then(|agent| hooks::turn_key(agent, &payload));
    if let (Some(agent), Some(key)) = (agent, &turn_key) {
        let outcome = event
            .as_deref()
            .and_then(|event| hooks::turn_outcome(agent, event, &payload));
        if state.apply_late_turn_event(session_id, key, outcome) {
            return;
        }
    }

    // Marked before anything of this event can release a queue: the first hook ends the trust
    // hold below, and its drain must not write into an error's prompt. A subagent's failure is
    // its own business, never the session's.
    if let (Some(agent), Some(event)) = (agent, event.as_deref()) {
        if !is_subagent && hooks::stopped_on_agent_error(agent, event, &payload) {
            if let Some(live) = state.live_session(session_id) {
                live.mark_agent_error();
                // Nothing else drains the queue once the hold has run out.
                let state = state.clone();
                let id = session_id.to_string();
                tokio::spawn(async move {
                    tokio::time::sleep(crate::session::AGENT_ERROR_SETTLE).await;
                    if let Ok(session) = state.session_record(&id) {
                        state.spawn_flush_outbox(&id, state.agent_status(&session));
                    }
                });
            }
        }
    }

    // Claude Code and Grok run no hook before their trust confirmation has been answered, so the
    // first one means that screen is gone — and anything on the terminal that looks like it is not
    // it. Codex runs none around its own, but its first hook still comes only once the session is
    // past it. A Grok screen the person answered in the terminal has its trust entry carried over
    // from here (`trust::on_hook`).
    if let Some(live) = state.live_session(session_id) {
        crate::session::trust::on_hook(state, &live);
    }

    // A subagent's events are the agent's internal business and carry the subagent's own ids; the
    // only thing read from them is the background work they leave running, which never changes
    // the session's status.
    if is_subagent {
        let change = event
            .as_deref()
            .zip(agent)
            .and_then(|(event, agent)| hooks::background_tasks_change(agent, event, &payload));
        if let Some(change) = change {
            state.apply_background_tasks_change(session_id, change);
        }
        return;
    }

    // Kept only for a running process: it is what an end of that process remembers the turn by.
    if let Some(key) = turn_key
        .clone()
        .filter(|_| state.live_session(session_id).is_some())
    {
        state.note_turn_key(session_id, key);
    }

    if let Some(agent_session_id) = hooks::agent_session_id(&payload) {
        if let Err(err) = state.set_agent_session_id(session_id, &agent_session_id) {
            tracing::debug!(session = %session_id, %err, "recording the agent's session id failed");
        }
    }

    crate::records::on_hook(state, session_id, &payload);

    let Some(event) = event else {
        tracing::debug!(session = %session_id, "hook payload carried no event name");
        return;
    };
    let mut prompt_sent = false;
    if hooks::starts_conversation(&event) {
        if let Err(err) = state.mark_conversation_started(session_id) {
            tracing::debug!(session = %session_id, %err, "recording the session's first turn failed");
        }
        let prompt = agent.and_then(|agent| hooks::submitted_prompt(agent, &payload));
        // A background task's completion, and a subagent's hand-back, arrive as prompts the agent
        // sends itself, which say nothing about what is in its input box.
        prompt_sent = !prompt.is_some_and(hooks::is_self_sent);
        // The previous turn's watch goes before the new turn opens: a watch still current
        // when it opens could find the previous turn's failure (Codex) or cancel (Claude
        // Code, Grok) and close this one.
        if agent.is_some() {
            state.end_record_watch(session_id);
        }
        state.turn_started(session_id, prompt);
    }
    let Some(agent) = agent else {
        tracing::debug!(session = %session_id, %event, "hook callback for an unknown session");
        return;
    };
    if !read_turn_boundary(state, session_id, agent, &event, &payload) {
        return;
    }
    // A whole new list goes in before the status, so the record the status change publishes
    // already carries it. A completion notification goes in after: it arrives as a prompt while
    // the session is still idle, and trimming the list first would publish a plain idle record
    // before the turn starts.
    let mut finished = None;
    match hooks::background_tasks_change(agent, &event, &payload) {
        Some(BackgroundTasksChange::Finished(ids)) => finished = Some(ids),
        Some(change) => state.apply_background_tasks_change(session_id, change),
        None => {}
    }
    apply_status(
        state,
        session_id,
        agent,
        &event,
        &payload,
        turn_key.as_deref(),
        prompt_sent,
    );
    if let Some(ids) = finished {
        state.apply_background_tasks_change(session_id, BackgroundTasksChange::Finished(ids));
    }
}

/// Applies the status `event` brings, if it brings one: the pause, the status itself, and what the
/// status sets going. `turn_key` is the key of the turn the event belongs to, and `prompt_sent`
/// whether the event is a prompt the agent was sent.
fn apply_status(
    state: &Arc<AppState>,
    session_id: &str,
    agent: Agent,
    event: &str,
    payload: &serde_json::Value,
    turn_key: Option<&str>,
    prompt_sent: bool,
) {
    let Some(status) = hooks::status_from_event(agent, event, payload) else {
        return;
    };
    let status = suppress_auto_review_hook(state, session_id, status);
    // Before the status write, so the record it publishes already carries whether the turn that
    // ended left the session paused.
    let outcome = hooks::turn_outcome(agent, event, payload);
    if let Some(outcome) = outcome {
        state.set_paused(session_id, outcome == hooks::TurnOutcome::Abnormal);
    }
    // Read before the status lands: a Claude Code watch, and a Grok one past the agent's own hand,
    // is armed on a move into a status, not on more of the same.
    let previous = matches!(agent, Agent::Claude | Agent::Grok)
        .then(|| state.session_record(session_id).ok())
        .flatten()
        .map(|session| state.agent_status(&session));
    match state.apply_hook_status(session_id, status) {
        // Delivery writes into the owner's PTY, which blocks.
        Ok(true) if status == SessionStatus::WaitingUser => {
            let (state, id) = (state.clone(), session_id.to_string());
            tokio::task::spawn_blocking(move || {
                crate::messaging::owner_notices::tell_owner_it_waits(&state, &id)
            });
        }
        Ok(_) => {}
        Err(err) => {
            tracing::debug!(session = %session_id, %event, %err, "applying the hook status failed");
        }
    }
    // The end of the process can land between the check at the top of `apply_hook` and these
    // writes, which a record without a process refuses: the turn's end is then its late word
    // after all.
    if let (Some(key), Some(outcome)) = (turn_key, outcome) {
        if state
            .session_record(session_id)
            .is_ok_and(|session| session.status.has_no_process())
        {
            state.apply_late_turn_event(session_id, key, Some(outcome));
        }
    }
    arm_record_watches(state, session_id, agent, event, previous, status, payload);
    // Counted after the status it brings and before the drain that status releases: that drain
    // lets a queue held for an unsent message go on, and the agent reads as working by the time
    // the outbox sees the count move.
    if prompt_sent {
        if let Some(live) = state.live_session(session_id) {
            live.note_prompt_sent(hooks::submitted_prompt(agent, payload));
        }
    }
    // Releasing a message queued while the session could not take one writes into its PTY, which
    // blocks; it must not sit on this response, where the adapters' few-second hook timeout would
    // surface as agent-visible noise.
    state.spawn_flush_outbox(session_id, status);
}

/// Starts watching the agent's own session record for what its hooks will not report after this
/// event, which reported `status` and found the agent at `previous` (read for Claude Code and Grok
/// Build alone). Claude Code reports neither a declined prompt nor an interrupted turn, so its
/// transcript is watched (`crate::records::watch::claude_transcript` says when); a Codex turn that
/// ends in an error fires nothing after its `UserPromptSubmit`, so its rollout is; a Grok turn
/// cancelled before its first token fires nothing at all, so its terminal title is watched from its
/// `UserPromptSubmit`, and again from each event that brings it back from waiting on the user
/// (`crate::session::title_interrupt`). Each watch is spawned, not awaited: it is expected to far
/// outlive this response, which the adapters' few-second hook timeout would not allow.
fn arm_record_watches(
    state: &Arc<AppState>,
    session_id: &str,
    agent: Agent,
    event: &str,
    previous: Option<SessionStatus>,
    status: SessionStatus,
    payload: &serde_json::Value,
) {
    let transcript_path = payload
        .get("transcript_path")
        .and_then(serde_json::Value::as_str);
    match agent {
        Agent::Claude => claude_transcript::watch_after_hook(
            state,
            session_id,
            event,
            previous,
            status,
            transcript_path,
        ),
        Agent::Codex if event == "UserPromptSubmit" => codex_rollout::watch_for_failure(
            state,
            session_id,
            transcript_path,
            payload.get("turn_id").and_then(serde_json::Value::as_str),
        ),
        // From the prompt, and again each time the hand comes down: whatever the title showed while
        // a card waited on the user must not end the turn that carries on after it.
        Agent::Grok
            if status == SessionStatus::Working
                && (hooks::starts_conversation(event)
                    || previous == Some(SessionStatus::WaitingUser)) =>
        {
            let generation = state.begin_record_watch(session_id);
            crate::session::title_interrupt::watch(state, session_id, generation);
        }
        _ => {}
    }
}

/// Acts on what this event says about the session's current turn, and says whether anything else it
/// claims about the session may be believed.
///
/// A turn boundary is read before the status is: Grok's one id-less turn-end signal also fires after
/// a turn that already ended, and taking its "idle" at face value would put a session that is
/// working back to idle.
fn read_turn_boundary(
    state: &Arc<AppState>,
    session_id: &str,
    agent: Agent,
    event: &str,
    payload: &serde_json::Value,
) -> bool {
    if hooks::turn_cancelled(agent, event) {
        // No report is owed — the user did it and knows — but the turn still has to be closed, or a
        // later clock-attributed end would find it open and invent one.
        state.abandon_turn(session_id);
        return true;
    }
    match hooks::turn_end(agent, event, payload) {
        // A session with its hand up has not finished its turn — it is waiting for a person (the
        // agent's own hand: a session request's hand says nothing of its turn). Grok's
        // clock-attributed end would otherwise lower the hand and report "it said nothing" for any
        // prompt left unanswered longer than the backstop delay.
        Some(turn)
            if turn.backstop
                && state
                    .store
                    .get_session(session_id)
                    .ok()
                    .flatten()
                    .is_some_and(|session| {
                        state.agent_status(&session) == SessionStatus::WaitingUser
                    }) =>
        {
            false
        }
        Some(turn) => match reporting::close_turn_and_report(state, session_id, turn) {
            // Talking about a turn that is already over, so nothing it says about the session
            // holds.
            TurnClose::NotThisTurn => false,
            TurnClose::NoTurn | TurnClose::Reported | TurnClose::OwesReport => true,
        },
        None => {
            // Anything that is not a turn boundary is the session still working on its turn, which
            // is what keeps a backstop from being attributed to it.
            state.touch_turn(session_id);
            true
        }
    }
}

/// Automatic reviews do not establish that a person is being asked to act.
///
/// With Codex's `approvals_reviewer` set to auto review, the permission hook fires, Codex resolves
/// the request itself. Visible action prompts override this hook status independently.
fn suppress_auto_review_hook(
    state: &Arc<AppState>,
    session_id: &str,
    status: crate::protocol::SessionStatus,
) -> crate::protocol::SessionStatus {
    use crate::protocol::SessionStatus;
    if status != SessionStatus::WaitingUser {
        return status;
    }
    match state.live_session(session_id) {
        Some(live) if live.resolves_approvals_itself => SessionStatus::Working,
        _ => status,
    }
}

#[cfg(test)]
mod tests;
