//! The refusals of a session that cannot be put on an agent or an account: an agent determined
//! unavailable or not added, an account that is not signed in (which the user's own open is not
//! refused for, but told of), and one whose quota is used up, which only a switch or a handoff is
//! refused for.

use std::sync::Arc;

use anyhow::Result;

use super::standing::{agent_is_available, used_up_quota};
use crate::protocol::{error_code, notice_code, AgentId, CodedError, Notice, Session};
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

/// Refuses to put a session on an account whose plan quota is used up, going by the reading held at
/// this moment (see [`crate::accounts::quotas::AccountQuotas::exhausted`]): an account not read
/// yet, or of an agent that is not probed, is not refused, and nothing waits for a fresh reading.
/// Applies to a switch and a handoff's receiving session, whatever their origin; a new session is
/// not refused for it. `config_dir` is the account's directory as [`Store::account_dir`] resolves
/// it, so a reading taken under another directory is ignored.
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
    let Some(signed_out) = signed_out_account(state, agent, account_id, config_dir)? else {
        return Ok(());
    };
    Err(CodedError::raised(
        error_code::ACCOUNT_NOT_SIGNED_IN,
        format!(
            "{} ({}) is not logged in. Choose another account, or log in to it in Settings \
             → Agent accounts.",
            signed_out.account,
            agent.label()
        ),
        &signed_out.params(agent),
    ))
}

/// The standing notice for a session the user opens on an account the held quota reading found
/// not signed in, which [`refuse_if_account_not_signed_in`] would refuse to anyone else: the
/// reading may be wrong, and the user is at the terminal to sign in if it is not. `None` when the
/// reading does not find the account signed out. Withdrawn by
/// [`withdraw_sign_in_unconfirmed`] once a reading finds the account signed in.
pub(crate) fn sign_in_unconfirmed_notice(
    state: &Arc<AppState>,
    agent: AgentId,
    account_id: Option<&str>,
    config_dir: Option<&str>,
) -> Result<Option<Notice>> {
    let Some(signed_out) = signed_out_account(state, agent, account_id, config_dir)? else {
        return Ok(None);
    };
    Ok(Some(
        Notice::new(
            notice_code::ACCOUNT_SIGN_IN_UNCONFIRMED,
            format!(
                "{} ({}) was last read as logged out, so this session may not be logged in. If {} \
                 asks for a login, log in in the terminal.",
                signed_out.account,
                agent.label(),
                agent.label()
            ),
            &signed_out.params(agent),
        )
        .standing(),
    ))
}

/// Withdraws [`sign_in_unconfirmed_notice`] from every session of `agent` on the account
/// `account_id` read under `config_dir`, as a reading that found it signed in was recorded for.
/// The account and directory are the session's own, as the notice was raised for them.
pub(crate) fn withdraw_sign_in_unconfirmed(
    state: &Arc<AppState>,
    agent: AgentId,
    account_id: Option<&str>,
    config_dir: Option<&str>,
) {
    for id in state.sessions_with_standing_notice(notice_code::ACCOUNT_SIGN_IN_UNCONFIRMED) {
        let Ok(session) = state.session_record(&id) else {
            continue;
        };
        if is_sessions_account(&session, agent, account_id, config_dir) {
            state.withdraw_standing_notice(
                &session.id,
                notice_code::ACCOUNT_SIGN_IN_UNCONFIRMED,
                None,
            );
        }
    }
}

/// Whether `session` runs on the account `account_id` of `agent` read under `config_dir`. The
/// directories are compared by their key (`hostfs::dir_key`), never as text: one directory has two
/// spellings on a host whose file system folds case.
fn is_sessions_account(
    session: &Session,
    agent: AgentId,
    account_id: Option<&str>,
    config_dir: Option<&str>,
) -> bool {
    let dir = |dir: Option<&str>| dir.map(crate::hostfs::dir_key);
    session.agent == agent
        && session.account_id.as_deref() == account_id
        && dir(session.config_dir.as_deref()) == dir(config_dir)
}

/// An account the held quota reading found not signed in, as its refusal and its notice name it.
struct SignedOut {
    /// The account's name.
    account: String,
    /// The directory the account's sign-in lives in, for a client that tells the user how to sign
    /// in with the agent's own CLI: the account's own, else the default account's as resolved.
    dir: Option<String>,
}

impl SignedOut {
    fn params(&self, agent: AgentId) -> Vec<(&'static str, &str)> {
        let mut params = vec![
            ("agent", agent.label()),
            ("agent_id", agent.wire_name()),
            ("account", self.account.as_str()),
        ];
        if let Some(dir) = self.dir.as_deref() {
            params.push(("config_dir", dir));
        }
        params
    }
}

/// The account `account_id` of `agent`, read under `config_dir`, when the held quota reading
/// found it not signed in.
fn signed_out_account(
    state: &Arc<AppState>,
    agent: AgentId,
    account_id: Option<&str>,
    config_dir: Option<&str>,
) -> Result<Option<SignedOut>> {
    if !state
        .account_quotas
        .signed_out(agent, account_id, config_dir)
    {
        return Ok(None);
    }
    let account = state.store.account_display_name(agent, account_id)?;
    let default_dir = state.agent_availability_of(agent).default_account_dir;
    let dir = config_dir.map(str::to_string).or(default_dir);
    Ok(Some(SignedOut { account, dir }))
}

/// Refuses to open a session of an agent that is not on the added list; see "When the agent or the
/// account cannot open a session" in `docs/product/opening-sessions.md`. An undecided list refuses
/// nothing. Only a session being opened is checked, so a live session of an agent removed since
/// keeps running, and resuming, restarting or reopening an existing session never comes here.
pub(crate) fn refuse_unless_agent_added(state: &Arc<AppState>, agent: AgentId) -> Result<()> {
    refuse_unless_agent_added_to(state, agent, "open a session with it")
}

/// [`refuse_unless_agent_added`] for a request that is not a session being opened: `purpose` ends
/// the message, "Add it in Settings → Agent accounts to <purpose>."
pub(crate) fn refuse_unless_agent_added_to(
    state: &Arc<AppState>,
    agent: AgentId,
    purpose: &str,
) -> Result<()> {
    if state.store.agent_is_added(agent)? {
        return Ok(());
    }
    Err(CodedError::raised(
        error_code::AGENT_NOT_ADDED,
        format!(
            "{} is not added. Add it in Settings → Agent accounts to {purpose}.",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::SessionStatus;
    use crate::test_support::project_session;

    /// A reading recorded for an account withdraws the notice from the sessions on it: the same
    /// agent, account and directory, the directory compared by its key, so a spelling that differs
    /// only in case is the same directory where the host folds case and another one elsewhere.
    #[test]
    fn a_sessions_account_is_matched_by_agent_account_and_directory_key() {
        let session = Session {
            account_id: Some("work".to_string()),
            config_dir: Some("/Users/me/.claude-work".to_string()),
            ..project_session("s", AgentId::CLAUDE, SessionStatus::Idle)
        };
        let dir = Some("/Users/me/.claude-work");
        assert!(is_sessions_account(
            &session,
            AgentId::CLAUDE,
            Some("work"),
            dir
        ));
        assert!(!is_sessions_account(
            &session,
            AgentId::CODEX,
            Some("work"),
            dir
        ));
        assert!(!is_sessions_account(&session, AgentId::CLAUDE, None, dir));
        assert!(!is_sessions_account(
            &session,
            AgentId::CLAUDE,
            Some("work"),
            None
        ));
        assert!(!is_sessions_account(
            &session,
            AgentId::CLAUDE,
            Some("work"),
            Some("/Users/me/.claude-home")
        ));
        assert_eq!(
            is_sessions_account(
                &session,
                AgentId::CLAUDE,
                Some("work"),
                Some("/Users/me/.Claude-Work")
            ),
            crate::platform::names::FOLDS_CASE
        );

        let default = project_session("d", AgentId::CLAUDE, SessionStatus::Idle);
        assert!(is_sessions_account(&default, AgentId::CLAUDE, None, None));
    }
}
