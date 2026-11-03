//! What the tools that relaunch a session on its conversation (`switch_session_account`,
//! `set_session_model`, `handoff_session`) tell the caller in the same words.

use crate::coordinator::resume::resumable_agent_session_id;
use crate::protocol::{AgentId, Session};
use crate::session_tools::catalogue::qualified_tool_name;

/// How a setting the user chose is told: its value, or the default for an unset one (for a
/// model, `list_agent_models`' `default_model`; for an effort, the agent's own).
pub(super) fn setting_text(value: &Option<String>) -> String {
    value
        .as_deref()
        .map_or_else(|| "the default".to_string(), |text| format!("`{text}`"))
}

/// What the caller (a session of `caller`'s agent) is told of the state a relaunched session came
/// back in: whether the relaunch was sent a prompt to carry on with the turn that `cut_off_by` (a
/// word for what relaunched it) cut off, or is awaiting instructions.
pub(super) fn relaunch_note(caller: AgentId, cut_off_by: &str, continued: bool) -> String {
    if continued {
        format!(
            "Its turn had been cut off by the {cut_off_by} or stopped on a usage limit, so it was \
             sent a prompt to continue that task: do not send it one yourself."
        )
    } else {
        format!(
            "It is back awaiting instructions: a turn it was in the middle of is cut off, so send \
             it the next step with `{}`.",
            qualified_tool_name(caller, "instruct_session")
        )
    }
}

/// What the caller is told of a relaunched `session` that had no conversation to continue, or
/// nothing when it had one. `place` is where it starts fresh, with its leading space, or empty.
pub(super) fn no_conversation_note(session: &Session, place: &str) -> String {
    if resumable_agent_session_id(session).is_some() {
        String::new()
    } else {
        format!(" It had no conversation yet, so it starts fresh{place}.")
    }
}
