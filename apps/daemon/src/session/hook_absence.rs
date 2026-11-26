//! The watch on a launch's hooks (`docs/plans/terminal-first-degradation.md`, D8): a launch whose
//! hook was due and did not come within [`HOOK_FALLBACK`] is read from then on as a launch of a
//! provider without hooks, and one that gets a hook is read as having them again.
//!
//! A hook is due at the launch where the provider says its first one comes as the agent starts,
//! and, where it comes with the first turn, once a prompt has gone in or the terminal has shown a
//! turn running ([`crate::adapter::FirstHook`], [`HookPresence::note_due`]). The bound neither
//! starts nor runs out while the agent's trust screen is up, since no hook runs while it waits for
//! a person ([`bound_passed`]). A bare launch is without hooks from its start. While a launch is
//! without them ([`LiveSession::hooks_absent`]):
//!
//! - its status follows the program's own reports (`LiveSession::status_from_reports`), from the
//!   root status record as it stands when the hooks are found absent, or, while the program has
//!   reported nothing, its terminal title (`crate::session::title_status`);
//! - the messages its trust watch held for a start hook are released at its input box with no trust
//!   screen up, as for an adopted session ([`trust::on_start_hook_absent`]);
//! - nothing waits for a turn message, a prompt reported sent, or a completion notice, and a cancel
//!   drops nothing that would be queued again;
//! - the user is told with a standing `hooks_not_received`, withdrawn when a hook arrives, and,
//!   where no conversation id is recorded for the session, with a standing
//!   `conversation_id_unknown`, withdrawn when one is recorded
//!   (`AppState::set_agent_session_id`).
//!
//! A hook that arrives late takes over without undoing anything: the state changes which source is
//! read, never a turn or a prompt a hook is waiting on. The watch is not stored: a daemon that
//! adopts the launch starts it again ([`watch`]), with a start hook taken to have run before it
//! (`HookPresence::note_adopted`).

use std::sync::{Arc, Weak};
use std::time::Duration;

use tokio::time::Instant;

use crate::adapter::FirstHook;
use crate::protocol::{notice_code, AgentId, Notice};
use crate::session::integration::{Hooks, Integration};
use crate::session::trust::{self, HOOK_FALLBACK};
use crate::session::LiveSession;
use crate::state::AppState;

/// Starts watching the hooks of `live`, a launch just registered or, where `adopted`, adopted from a
/// holder the previous daemon left running. A launch whose hook is expected is watched for it; one
/// that expects none is left alone. A bare launch is without hooks from its start: its launch notice
/// says so, its trust watch holds for its input box from the start, and its notice that the
/// conversation id is unknown goes with its other launch notices ([`launch_notice`]), or, adopted,
/// is raised here again, since it was raised after the spawn. Gone when the session is.
pub fn watch(state: &Arc<AppState>, live: &Arc<LiveSession>, adopted: bool) {
    match live.hooks.state() {
        Hooks::NotExpected | Hooks::Present => {}
        Hooks::Absent => {
            if adopted {
                tell_conversation_id_unknown(state, live);
            }
        }
        Hooks::Expected => {
            let state = state.clone();
            let due = live.hooks.due_wait();
            let weak = Arc::downgrade(live);
            tokio::spawn(async move {
                if !due.until_due().await {
                    return;
                }
                let Some(live) = bound_passed(&weak).await else {
                    return;
                };
                if !state.is_current(&live) || live.has_exited() || !live.hooks.enter_absent() {
                    return;
                }
                on_absent(&state, &live);
            });
        }
    }
}

/// How often the bound looks for a trust screen up while one can come up.
const TRUST_SCREEN_POLL: Duration = Duration::from_millis(250);

/// Returns once [`HOOK_FALLBACK`] has passed with no trust screen of the launch up
/// ([`trust::screen_up`]): the agent runs no hook while its screen waits for a person, however
/// long, so the bound starts only once no screen is up, and starts again once a screen that came
/// up meanwhile goes. `None` once the session is gone.
async fn bound_passed(weak: &Weak<LiveSession>) -> Option<Arc<LiveSession>> {
    let mut since = Instant::now();
    loop {
        let live = weak.upgrade()?;
        let watched = trust::screen_can_come_up(&live);
        if watched && trust::screen_up(&live) {
            since = Instant::now();
        }
        let left = HOOK_FALLBACK.saturating_sub(since.elapsed());
        if left.is_zero() {
            return Some(live);
        }
        drop(live);
        tokio::time::sleep(if watched {
            left.min(TRUST_SCREEN_POLL)
        } else {
            left
        })
        .await;
    }
}

/// The launch has just been found without hooks.
fn on_absent(state: &Arc<AppState>, live: &Arc<LiveSession>) {
    tracing::info!(
        session = %live.id,
        "no hook came within the bound after one was due; the session runs without its hooks"
    );
    trust::on_start_hook_absent(state, live);
    // What the program reported before is its status from now on.
    live.take_status_from_reports_now();
    state.raise_standing_notice(
        &live.id,
        hooks_not_received(live.agent),
        crate::time::now_millis(),
    );
    tell_conversation_id_unknown(state, live);
    if let Ok(session) = state.session_record(&live.id) {
        state.publish_session(&session);
        // What waited on the hooks may go in now.
        state.spawn_flush_outbox(&live.id, state.agent_status(&session));
    }
}

/// A hook of `live` reached the daemon: the launch has hooks from now on, and a notice that said
/// it had none is withdrawn.
pub fn on_hook(state: &AppState, live: &LiveSession) {
    if live.hooks.note_hook() == Hooks::Absent && live.integration().tier.integrated() {
        tracing::info!(session = %live.id, "a hook came after the session was read as without them");
        state.withdraw_standing_notice(&live.id, notice_code::HOOKS_NOT_RECEIVED, None);
        if let Ok(session) = state.session_record(&live.id) {
            state.publish_session(&session);
        }
    }
}

/// The notice a launch registered without hooks from its start (bare) raises with its other launch
/// notices: that its conversation has no id it can be resumed by, where the session has none
/// recorded. None for any other launch.
pub fn launch_notice(state: &AppState, live: &LiveSession) -> Option<Notice> {
    (live.hooks_absent() && !id_recorded(state, live)).then(|| conversation_id_unknown(live.agent))
}

/// The notice an integrated launch that expects no hook (`FirstHook::Never`: a provider's plan
/// that left its hooks out) raises with its other launch notices, where it records no conversation
/// id of its own (`records_an_id`: it resumes none, and its provider preallocated none): nothing
/// will report the id, so its conversation cannot be resumed by it. None for any other launch: a
/// bare one says so after its spawn ([`launch_notice`]), and one with hooks learns the id from
/// them. Withdrawn, as every such notice, when an id is recorded (`AppState::set_agent_session_id`).
pub(crate) fn unreported_id_notice(
    agent: AgentId,
    integration: &Integration,
    records_an_id: bool,
) -> Option<Notice> {
    (integration.tier.integrated() && integration.first_hook == FirstHook::Never && !records_an_id)
        .then(|| conversation_id_unknown(agent))
}

/// Whether the session of `live` has a conversation id recorded.
fn id_recorded(state: &AppState, live: &LiveSession) -> bool {
    state
        .session_record(&live.id)
        .is_ok_and(|session| session.agent_session_id.is_some())
}

/// Tells the user that the launch's conversation has no id it can be resumed by, where the session
/// has none recorded: its next relaunch opens a new conversation.
fn tell_conversation_id_unknown(state: &AppState, live: &LiveSession) {
    if id_recorded(state, live) {
        return;
    }
    state.raise_standing_notice(
        &live.id,
        conversation_id_unknown(live.agent),
        crate::time::now_millis(),
    );
}

/// The standing notice that `agent`'s hooks did not come.
pub(crate) fn hooks_not_received(agent: AgentId) -> Notice {
    Notice::new(
        notice_code::HOOKS_NOT_RECEIVED,
        format!(
            "{} has not reported to Octoboard through its hooks, so the session runs as it would \
             in a plain terminal: its status comes from what the program itself shows.",
            agent.label()
        ),
        &[("agent", agent.label())],
    )
    .standing()
}

/// The standing notice that the conversation of `agent`'s launch has no id it can be resumed by.
pub(crate) fn conversation_id_unknown(agent: AgentId) -> Notice {
    Notice::new(
        notice_code::CONVERSATION_ID_UNKNOWN,
        format!(
            "Octoboard does not know the id of this {} conversation, so it cannot be resumed by \
             it: the session's next start opens a new conversation.",
            agent.label()
        ),
        &[("agent", agent.label())],
    )
    .standing()
}

#[cfg(test)]
mod tests;
