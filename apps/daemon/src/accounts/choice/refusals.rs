//! The refusals of a session that cannot be opened on an agent or an account: an agent determined
//! unavailable or not added, an account whose quota is used up or that is not signed in.

use std::sync::Arc;

use anyhow::Result;

use super::standing::{agent_is_available, used_up_quota};
use crate::protocol::{error_code, AgentId, CodedError};
use crate::state::AppState;
use crate::time::now_millis;

/// Refuses to open a session whose resolved agent has been *determined* unavailable — never while
/// that determination is still pending, which the launch's own refusal of a missing binary already
/// covers; see "When the agent or the account cannot open a session" in
/// `docs/product/opening-sessions.md`. Checked here, ahead of everything that follows, so this is
/// also what a console session's own `open_session` tool is refused through — it calls this same
/// function.
pub(crate) fn refuse_if_agent_unavailable(state: &Arc<AppState>, agent: AgentId) -> Result<()> {
    if agent_is_available(state, agent) {
        return Ok(());
    }
    Err(CodedError::raised(
        error_code::AGENT_NOT_AVAILABLE,
        format!(
            "{} is not available: its binary does not resolve on your login shell's PATH. {} \
             supports {}; install one of them and make sure it is on your PATH to open a session.",
            agent.label(),
            crate::APP_NAME,
            supported_labels()
        ),
        &[("agent", agent.label())],
    ))
}

/// Refuses to put a session on an account whose plan quota is used up, going by the reading held
/// at this moment (see [`crate::accounts::quotas::AccountQuotas::exhausted`]): an account not read yet, or
/// of an agent that is not probed, is not refused, and nothing waits for a fresh reading. Applies
/// to every origin, the user's own request included. `config_dir` is the account's directory as
/// [`Store::account_dir`] resolves it, so a reading taken under another directory is ignored.
///
/// [`Store::account_dir`]: crate::store::Store::account_dir
pub(crate) fn refuse_if_account_quota_exhausted(
    state: &Arc<AppState>,
    agent: AgentId,
    account_id: Option<&str>,
    config_dir: Option<&str>,
) -> Result<()> {
    let now = now_millis();
    let Some(exhausted) = used_up_quota(state, agent, account_id, config_dir, now) else {
        return Ok(());
    };
    let account = state.store.account_display_name(agent, account_id)?;
    let resets = exhausted.resets_at.map(|at| at.to_string());
    let minutes = exhausted.window_minutes.map(|minutes| minutes.to_string());
    let mut params = vec![
        ("agent", agent.label()),
        ("account", account.as_str()),
        ("window", exhausted.window.as_str()),
        ("window_kind", exhausted.kind.wire_name()),
    ];
    if let Some(minutes) = minutes.as_deref() {
        params.push(("window_minutes", minutes));
    }
    if exhausted.has_model_weekly {
        params.push(("model_weekly", "true"));
    }
    if let Some(resets) = resets.as_deref() {
        params.push(("resets_at", resets));
    }
    let resets_in = exhausted
        .resets_at
        .map(|at| format!(", which resets in about {}", duration_label(at - now)))
        .unwrap_or_default();
    Err(CodedError::raised(
        error_code::ACCOUNT_QUOTA_EXHAUSTED,
        format!(
            "{account} has used up its {} quota ({}){resets_in}. Choose another account, or wait \
             for the quota to reset.",
            agent.label(),
            exhausted.named_for(agent)
        ),
        &params,
    ))
}

/// Refuses an account the held quota reading found not signed in (see
/// [`crate::accounts::quotas::AccountQuotas::signed_out`]); a reading that failed for another reason, or
/// none yet, never refuses. `config_dir` is the account's directory as [`Store::account_dir`]
/// resolves it, so a reading taken under another directory is ignored.
///
/// [`Store::account_dir`]: crate::store::Store::account_dir
pub(crate) fn refuse_if_account_not_signed_in(
    state: &Arc<AppState>,
    agent: AgentId,
    account_id: Option<&str>,
    config_dir: Option<&str>,
) -> Result<()> {
    if !state
        .account_quotas
        .signed_out(agent, account_id, config_dir)
    {
        return Ok(());
    }
    let account = state.store.account_display_name(agent, account_id)?;
    Err(CodedError::raised(
        error_code::ACCOUNT_NOT_SIGNED_IN,
        format!(
            "{account} ({}) is not logged in. Choose another account, or log in to it in Settings \
             → Agent accounts.",
            agent.label()
        ),
        &[("agent", agent.label()), ("account", account.as_str())],
    ))
}

/// Refuses to open a session of an agent that is not on the added list; see "When the agent or the
/// account cannot open a session" in `docs/product/opening-sessions.md`. An undecided list refuses
/// nothing. Only a session being opened is checked, so a live session of an agent removed since
/// keeps running, and resuming, restarting or reopening an existing session never comes here.
pub(crate) fn refuse_unless_agent_added(state: &Arc<AppState>, agent: AgentId) -> Result<()> {
    if state.store.agent_is_added(agent)? {
        return Ok(());
    }
    Err(CodedError::raised(
        error_code::AGENT_NOT_ADDED,
        format!(
            "{} is not added. Add it in Settings → Agent accounts to open a session with it.",
            agent.label()
        ),
        &[("agent", agent.label())],
    ))
}

/// Every registered provider's label, as a list in prose: "Claude Code, Codex and Grok Build".
fn supported_labels() -> String {
    let labels: Vec<&str> = crate::providers::descriptors()
        .map(|descriptor| descriptor.label)
        .collect();
    crate::text::list_names(&labels)
}

/// A span of milliseconds in the rough terms of a message to read: `3d 4h`, `5h 20m` or `7m`.
fn duration_label(millis: i64) -> String {
    let minutes = millis.max(0) / 60_000;
    let (days, hours, minutes) = (minutes / (24 * 60), minutes / 60 % 24, minutes % 60);
    match (days, hours) {
        (0, 0) => format!("{}m", minutes.max(1)),
        (0, _) => format!("{hours}h {minutes}m"),
        _ => format!("{days}d {hours}h"),
    }
}
