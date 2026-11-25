//! What applying a hook event does to the daemon: the queued events of a session applied in order,
//! and for each the session's status, turns, background tasks and record watches it moves.
//! What a payload says is read by the parent module.

use std::sync::Arc;

use crate::adapter::AgentProvider;
use crate::hooks::{self, BackgroundTasksChange, HookCarry, HookInterpreter, HookSession};
use crate::messaging::reporting;
use crate::protocol::{AgentId, SessionStatus};
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
    while let Some(hook) = state.hook_queues.next(session_id) {
        apply_hook(
            state,
            session_id,
            hook.launch.as_deref(),
            &hook.body,
            hook.event.as_deref(),
        );
        if !state.hook_queues.complete(session_id) {
            break;
        }
    }
    guard.drained = true;
}

/// The agent a hook's session runs, live or recorded.
fn hook_agent(state: &Arc<AppState>, session_id: &str) -> Option<AgentId> {
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

/// Applies a hook to its session. It may name the launch that posted it. The payload's event name
/// takes precedence over the name passed alongside it.
fn apply_hook(
    state: &Arc<AppState>,
    session_id: &str,
    launch: Option<&str>,
    body: &[u8],
    event_name: Option<&str>,
) {
    let payload: serde_json::Value =
        serde_json::from_slice(body).unwrap_or(serde_json::Value::Null);
    let event = hooks::event_name(&payload).or_else(|| event_name.map(str::to_string));

    // An event of the turn the end of the session's process cut short comes from the ended
    // process, posted after the end by a hook the agent did not wait for: it is that turn's
    // late word, and nothing of it reaches whatever runs in the session now.
    let provider = hook_agent(state, session_id).map(crate::adapter::provider);
    let interpreter = provider.map(|provider| provider.hooks());
    // What the interpreter may know of the session and keep for it. The process the event found
    // is held for the whole event, so what the interpreter keeps goes where it is looked for
    // next; a session with no process has nothing kept past this event.
    let found = state.live_session(session_id);
    let (scratch, unkept) = (
        found.as_ref().and_then(|live| live.scratch_dir()),
        HookCarry::default(),
    );
    let session = HookSession::new(
        session_id,
        scratch.as_deref(),
        found.as_ref().and_then(|live| live.config_dir()),
        found.as_ref().map_or(&unkept, |live| &live.hook_carry),
    );
    let session = &session;
    let is_subagent = interpreter.is_some_and(|interpreter| interpreter.is_subagent(&payload));
    let turn_key = interpreter
        .filter(|_| !is_subagent)
        .and_then(|interpreter| interpreter.turn_key(&payload));
    if let (Some(interpreter), Some(key)) = (interpreter, &turn_key) {
        let outcome = event
            .as_deref()
            .and_then(|event| interpreter.turn_outcome(event, &payload));
        if state.apply_late_turn_event(session_id, key, outcome) {
            return;
        }
    }

    // Any other report from a launch that has ended is that process's too, arriving once its end
    // was seen, perhaps after the session was launched again: it says nothing about what runs in
    // the session now, nor about the conversation it is on.
    if launch.is_some_and(|launch| state.launch_ended(session_id, launch)) {
        tracing::debug!(session = %session_id, "ignoring a hook report from a launch that has ended");
        return;
    }

    // Marked before anything of this event can release a queue: the first hook ends the trust
    // hold below, and its drain must not write into an error's prompt. A subagent's failure is
    // its own business, never the session's.
    if let (Some(interpreter), Some(event)) = (interpreter, event.as_deref()) {
        if !is_subagent && interpreter.stopped_on_agent_error(event, &payload) {
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

    // A hook that comes only once the session is past its trust confirmation means that screen is
    // gone, and anything on the terminal that looks like it is not it; which hooks those are is the
    // provider's to say (`HookInterpreter::past_startup`). A screen the person answered in the
    // terminal has its trust entry carried over from here (`trust::on_hook`).
    let past_startup = interpreter
        .zip(event.as_deref())
        .is_none_or(|(interpreter, event)| interpreter.past_startup(event, &payload));
    if let Some(live) = state.live_session(session_id) {
        // Noted first, so that the hook releases at once what waits for the agent to read its
        // input when its input box is already up.
        let announces_start =
            interpreter
                .zip(event.as_deref())
                .is_some_and(|(interpreter, event)| {
                    interpreter.announces_session_start(event, &payload)
                });
        if !is_subagent && announces_start {
            crate::session::trust::on_session_start(&live);
        }
        if past_startup {
            crate::session::trust::on_hook(state, &live);
        }
    }

    // A subagent's events are the agent's internal business and carry the subagent's own ids; the
    // only thing read from them is the background work they leave running, which never changes
    // the session's status.
    if is_subagent {
        let changes = event
            .as_deref()
            .zip(interpreter)
            .map(|(event, interpreter)| {
                interpreter.background_tasks_changes_in(session, event, &payload)
            })
            .unwrap_or_default();
        for change in changes {
            state.apply_background_tasks_change(session_id, change);
        }
        return;
    }

    // What the provider reads of the turn's message for the turn end its program's report closes.
    // The session takes it only where those reports give its status, and reads a source that has
    // to be opened off this queue.
    if let (Some(interpreter), Some(event), Some(live)) = (
        interpreter,
        event.as_deref(),
        state.live_session(session_id),
    ) {
        if let Some(message) = interpreter.turn_message_in(session, event, &payload) {
            live.supply_turn_message(message);
        }
    }

    // Kept only for a running process: it is what an end of that process remembers the turn by.
    if let Some(key) = turn_key
        .clone()
        .filter(|_| state.live_session(session_id).is_some())
    {
        state.note_turn_key(session_id, key);
    }

    // Whether the event is known to come from a process launched since the one that ended
    // (`AppState::mark_conversation_reported`): it names the session's launch under way
    // (`AppState::launch_is_current`), or else, from a hook that names no launch or one this run
    // does not know, a conversation other than the one the session named until now. Read before
    // the id is recorded below.
    let reported_id = interpreter.and_then(|interpreter| interpreter.agent_session_id(&payload));
    let launched_since = launch.is_some_and(|launch| state.launch_is_current(session_id, launch))
        || reported_id.as_deref().is_some_and(|reported| {
            state
                .store
                .get_session(session_id)
                .ok()
                .flatten()
                .is_some_and(|session| session.agent_session_id.as_deref() != Some(reported))
        });
    if let Some(agent_session_id) = reported_id {
        if let Err(err) = state.set_agent_session_id(session_id, &agent_session_id) {
            tracing::debug!(session = %session_id, %err, "recording the agent's session id failed");
        }
    }

    if let Some(provider) = provider {
        crate::records::on_hook(state, session_id, provider.records(), &payload);
        if let Some(launch) = launch.filter(|_| !is_subagent) {
            if let Some(report) = event
                .as_deref()
                .and_then(|event| provider.hooks().context_report(event, &payload))
            {
                if state.launch_is_current(session_id, launch) {
                    state.launch_context.report(session_id, launch, report);
                }
            }
        }
    }

    // The calls a prompt can be matched to: the current turn's, which a new prompt submission
    // begins.
    if let (Some(reader), Some(event), Some(live)) = (
        provider.and_then(|provider| provider.interactions()),
        event.as_deref(),
        state.live_session(session_id),
    ) {
        if interpreter.is_some_and(|interpreter| interpreter.starts_conversation(event, &payload)) {
            live.interaction.turn_started();
        }
        if let Some(call) = reader.call_started(event, &payload) {
            live.interaction.call_started(call);
        }
    }

    let Some(event) = event else {
        tracing::debug!(session = %session_id, "hook payload carried no event name");
        return;
    };
    let (Some(provider), Some(interpreter)) = (provider, interpreter) else {
        tracing::debug!(session = %session_id, %event, "hook callback for an unknown session");
        return;
    };
    let mut prompt_sent = false;
    if interpreter.starts_conversation(&event, &payload) {
        if let Err(err) = state.mark_conversation_reported(session_id, launched_since) {
            tracing::debug!(session = %session_id, %err, "recording the session's first turn failed");
        }
        let prompt = interpreter.submitted_prompt(&payload);
        // A background task's completion, and a subagent's hand-back, arrive as prompts the agent
        // sends itself, which say nothing about what is in its input box; unless the prompt is a
        // message Octoboard wrote, whatever it quotes, which the agent sending it has taken.
        prompt_sent = !prompt.is_some_and(|prompt| {
            interpreter.is_self_sent(prompt)
                && !state
                    .live_session(session_id)
                    .is_some_and(|live| live.wrote_message(prompt))
        });
        // The previous turn's watch goes before the new turn opens: a watch still current
        // when it opens could find the previous turn's failure (Codex) or cancel (Claude
        // Code, Grok) and close this one.
        state.end_record_watch(session_id);
        if interpreter.opening_prompt_may_follow_start() {
            state.turn_started_prompt_may_follow(session_id, prompt);
        } else {
            state.turn_started(session_id, prompt);
        }
        if prompt.is_some_and(|prompt| interpreter.is_completion_notice(prompt)) {
            state.note_completion_notice(session_id);
        }
    }
    if !read_turn_boundary(state, session, interpreter, &event, &payload) {
        return;
    }
    if let Some(live) = found.as_ref().filter(|live| {
        launch == Some(live.launch_id.as_str())
            && state.launch_is_current(session_id, &live.launch_id)
    }) {
        if let Some(record) = interpreter.program_status(&event, &payload) {
            live.note_hook_program_status(record);
        }
    }
    // A whole new list goes in before the status, so the record the status change publishes
    // already carries it. A completion notification goes in after: it arrives as a prompt while
    // the session is still idle, and trimming the list first would publish a plain idle record
    // before the turn starts. Of several changes, the first that finishes tasks and every one
    // after it go in after the status, so they are still applied in the order given.
    let mut changes = interpreter.background_tasks_changes_in(session, &event, &payload);
    let finished_from = changes
        .iter()
        .position(|change| matches!(change, BackgroundTasksChange::Finished(_)))
        .unwrap_or(changes.len());
    let after_status = changes.split_off(finished_from);
    for change in changes {
        state.apply_background_tasks_change(session_id, change);
    }
    apply_status(
        state,
        session,
        provider,
        &event,
        &payload,
        turn_key.as_deref(),
        prompt_sent,
    );
    for change in after_status {
        state.apply_background_tasks_change(session_id, change);
    }
}

/// Applies the status `event` brings, if it brings one: the pause, the status itself, and what the
/// status sets going. `turn_key` is the key of the turn the event belongs to, and `prompt_sent`
/// whether the event is a prompt the agent was sent. The session's `provider` reads the event
/// (`AgentProvider::hooks`) and arms the record watch it asks for (`AgentProvider::records`). A
/// status the interpreter holds back (`HookInterpreter::status_in`) is none: nothing of it is
/// published and nobody is told.
fn apply_status(
    state: &Arc<AppState>,
    session: &HookSession<'_>,
    provider: &dyn AgentProvider,
    event: &str,
    payload: &serde_json::Value,
    turn_key: Option<&str>,
    prompt_sent: bool,
) {
    let session_id = session.id;
    let interpreter = provider.hooks();
    let still_waiting = interpreter
        .tool_call_change(event, payload)
        .is_some_and(|change| state.apply_tool_call_change(session_id, change));
    let Some(status) = interpreter.status_in(session, event, payload) else {
        return;
    };
    // The end of one tool call, or the start of another, says nothing about a call of the same
    // turn that still waits on the user: the hand stays up, and nothing is armed for work.
    if status == SessionStatus::Working && still_waiting {
        return;
    }
    let status = suppress_auto_review_hook(state, session_id, status);
    // Before the status write, so the record it publishes already carries whether the turn that
    // ended left the session paused.
    let outcome = interpreter.turn_outcome(event, payload);
    if let Some(outcome) = outcome {
        state.set_paused(session_id, outcome == hooks::TurnOutcome::Abnormal);
    }
    // A hand Octoboard lowered itself, once the prompt it raised for left the screen
    // (`AppState::lower_hand_left_by_prompt`), is still the hooks' `WaitingUser`: what they arm on
    // a move off a prompt is owed to the first hook after it, as if the hand had stayed up. That
    // hook ends the mark.
    let lowered = state
        .live_session(session_id)
        .filter(|live| live.interaction.lowered());
    if let Some(live) = &lowered {
        live.interaction.clear_lowered();
    }
    // Read before the status lands: a watch armed on a move into a status, not on more of the
    // same, needs the status the event found.
    let previous = interpreter
        .watch_reads_previous_status()
        .then(|| {
            if lowered.is_some() {
                return Some(SessionStatus::WaitingUser);
            }
            state
                .session_record(session_id)
                .ok()
                .map(|session| state.agent_status(&session))
        })
        .flatten();
    // The prompt is read off the screen for as long as the session waits on the user.
    if status == SessionStatus::WaitingUser {
        if let Some(live) = state.live_session(session_id) {
            crate::session::interaction::watch::ensure(state, &live);
        }
    }
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
    if let Some(watch) = interpreter.record_watch(event, payload, previous, status) {
        provider.records().arm_watch(state, session_id, watch);
    }
    // Counted after the status it brings and before the drain that status releases: that drain
    // lets a queue held for an unsent message go on, and the agent reads as working by the time
    // the outbox sees the count move.
    if prompt_sent {
        if let Some(live) = state.live_session(session_id) {
            state.note_prompt_sent(&live, interpreter.submitted_prompt(payload));
        }
    }
    // Releasing a message queued while the session could not take one writes into its PTY, which
    // blocks; it must not sit on this response, where the adapters' few-second hook timeout would
    // surface as agent-visible noise.
    state.spawn_flush_outbox(session_id, status);
}

/// Acts on what this event says about the session's current turn, and says whether anything else it
/// claims about the session may be believed.
///
/// A turn boundary is read before the status is: Grok's one id-less turn-end signal also fires after
/// a turn that already ended, and taking its "idle" at face value would put a session that is
/// working back to idle.
fn read_turn_boundary(
    state: &Arc<AppState>,
    session: &HookSession<'_>,
    interpreter: &dyn HookInterpreter,
    event: &str,
    payload: &serde_json::Value,
) -> bool {
    let session_id = session.id;
    if interpreter.turn_cancelled(event) {
        // No report is owed — the user did it and knows — but the turn still has to be closed, or a
        // later clock-attributed end would find it open and invent one.
        let turn_was_open = state.turn_is_open(session_id);
        state.abandon_turn(session_id);
        if turn_was_open {
            state.write_dropped_messages_again(session_id);
        }
        start_own_queue(state, session_id);
        return true;
    }
    let status = || {
        state
            .store
            .get_session(session_id)
            .ok()
            .flatten()
            .map(|session| state.agent_status(&session))
    };
    match interpreter.turn_end_in(session, event, payload) {
        // A session with its hand up has not finished its turn — it is waiting for a person (the
        // agent's own hand: a session request's hand says nothing of its turn). Grok's
        // clock-attributed end would otherwise lower the hand and report "it said nothing" for any
        // prompt left unanswered longer than the backstop delay.
        Some(turn) if turn.backstop && status() == Some(SessionStatus::WaitingUser) => false,
        // A turn held on work still running is not over: Grok's `Stop` that listed the work left
        // it open at the prompt, and the `idle_prompt` it arms a minute later would otherwise close
        // it as if it had ended there, with nothing to show for it. The work's completion wakes a
        // turn of the agent's own, whose `Stop` closes it. Only while the session sits at its
        // prompt: a turn running since then that reports no `Stop` has the backstop as its only end.
        Some(turn)
            if turn.backstop
                && status() == Some(SessionStatus::Idle)
                && !state.background_tasks_of(session_id).is_empty() =>
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

/// Has the session's own server run the messages of Octoboard's that wait in its queue, which
/// its agent leaves there past a cancelled turn (Codex's), so they run as turns of their own. Off
/// the hook's response, since it asks the server.
pub(crate) fn start_own_queue(state: &Arc<AppState>, session_id: &str) {
    let Some(transport) = state
        .live_session(session_id)
        .and_then(|live| live.transport.clone())
    else {
        return;
    };
    let session_id = session_id.to_string();
    std::thread::spawn(move || {
        use crate::session::term::QueueStart;
        match transport.start_queue() {
            QueueStart::Nothing => {}
            QueueStart::Started(started) => {
                tracing::info!(session = %session_id, started, "started the messages a cancelled turn left queued");
            }
            QueueStart::BehindOthers(waiting) => {
                tracing::info!(session = %session_id, waiting, "left the messages a cancelled turn left queued behind others");
            }
            QueueStart::Failed(error) => {
                tracing::warn!(session = %session_id, %error, "could not start the messages a cancelled turn left queued");
            }
        }
    });
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
