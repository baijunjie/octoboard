//! The app-wide settings record, and the approval overrides it carries.

use serde::{Deserialize, Serialize};

use super::agents::Agent;
use super::records::Account;

/// Who resolves Codex approval requests. An absent override follows the account configuration.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalReviewer {
    User,
    AutoReview,
}

/// The permission mode a Claude Code launch is asked for. An absent override follows the account
/// configuration and passes no flag.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaudeApprovalMode {
    /// Claude Code's classifier approves what it judges safe and blocks the rest.
    Auto,
    /// Every tool call that is not already allowed asks the user.
    Manual,
    /// Every tool call runs without asking.
    Bypass,
}

/// The app-wide user settings the daemon stores, and the account list alongside them: a record
/// that already grows without a new request shape or a new event for every addition, which is
/// why the accounts live here rather than in a record of their own.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct Settings {
    pub auto_pull_repositories: bool,
    /// Whether the daemon's scan deletes the archived sessions whose conversation is proven gone
    /// (see `coordinator/archive_cleanup.rs`). On unless the user turned it off.
    pub auto_delete_unresumable_archives: bool,
    /// Whether a session's `request_session` call is approved without asking the user (see
    /// `session_request`). Off unless the user turned it on; only the user's own client changes it.
    pub auto_approve_session_requests: bool,
    /// Whether a session's `request_resume_session` call is approved without asking the user, as
    /// `auto_approve_session_requests` does for `request_session`, independently of it. Off unless
    /// the user turned it on; only the user's own client changes it.
    pub auto_approve_resume_requests: bool,
    /// Whether an owner's `switch_session_account` calls run without asking the user, as
    /// `auto_approve_session_requests` does for `request_session`, independently of the other
    /// approvals. Off unless the user turned it on; only the user's own client changes it.
    pub auto_approve_account_switches: bool,
    /// Whether an owner's `handoff_session` calls run without asking the user, as
    /// `auto_approve_session_requests` does for `request_session`, independently of the other
    /// approvals. Off unless the user turned it on; only the user's own client changes it.
    pub auto_approve_handoffs: bool,
    /// Whether the instructions given to every session at launch ask it to put each question that
    /// needs the user's decision or answer as options to pick from. On unless the user turned it
    /// off. Fixed per session when it starts, so a change reaches only sessions begun afterwards
    /// (a console session's file is rewritten at each launch, so it follows at the next one). Only
    /// the user's own client changes it.
    pub ask_questions_as_choices: bool,
    /// The tag of the language the user last picked for the interface, one of the tags the
    /// interface offers; `null` until a client reports one. The user's language, not a client's:
    /// the clients keep their own choice locally, and this holds the one most recently reported.
    /// Not carried by an export; not visible to agents.
    pub interface_language: Option<String>,
    /// Whether a new session whose account was left out runs on the account of its agent whose
    /// weekly quota resets soonest and still has some left. Off unless the user turned it on.
    /// Without a weekly pick, try the inherited account, then the first usable in saved order.
    /// A new session with no named agent or account also tries other installed, added agents in
    /// added order when the preferred agent has no usable account. A resume, switch or handoff
    /// keeps its target agent.
    pub auto_select_account: bool,
    /// Whether Octoboard itself moves a session to another account when its turn stopped on the
    /// account's usage limit (see `crate::accounts::auto_switch`). Off unless the user turned it
    /// on. Only that: it does not choose a new session's account (`auto_select_account`) or answer
    /// an agent's request for a switch (`auto_approve_account_switches`).
    pub auto_switch_on_usage_limit: bool,
    /// Whether the client's sidebar orders sessions and projects by status. On unless the user
    /// turned it off; the daemon only stores it, the ordering is the client's.
    pub sidebar_order_by_status: bool,
    /// Whether the daemon starts an account's usage window that has reset but not begun counting,
    /// by sending one minimal request on that account. Off unless the user turned it on.
    pub auto_start_quota_windows: bool,
    /// The directory a `git` association clones into when none is named: the one the user set, or
    /// `~/Projects` (expanded) while none is. Always an absolute, lexically normalised path.
    pub default_clone_dir: String,
    /// Every account of every agent, application-wide. The default account of each agent is not
    /// among these — it is the state of pinning nothing, not a row (see `Account`).
    pub accounts: Vec<Account>,
    /// The user's order of each agent's accounts, by agent wire name, which clients list them in
    /// and automatic account selection takes them in; null entries represent the default account.
    /// Missing accounts are appended and stale IDs are ignored. Kept local to this machine.
    #[cfg_attr(test, ts(type = "Partial<Record<Agent, Array<string | null>>>"))]
    pub account_order: std::collections::BTreeMap<String, Vec<Option<String>>>,
    /// The agents the user has added, in the order the user set; a newly added agent goes last. Only these are offered for a
    /// new choice; removing one leaves its accounts here. `null` while the daemon has not yet
    /// decided the list (it waits for the agents' availability), which is not the same as the
    /// empty list of a user who removed them all.
    pub added_agents: Option<Vec<Agent>>,
    /// Launch override for every Codex session, whichever account it uses; never written into
    /// Codex's own configuration. Null follows each account's configuration. Kept local.
    pub codex_approval_reviewer: Option<ApprovalReviewer>,
    /// Launch override for every Claude Code session, whichever account it uses; never written
    /// into Claude Code's own configuration. Null follows each account's configuration. Kept local.
    pub claude_approval_mode: Option<ClaudeApprovalMode>,
    /// Whether each agent other than Claude Code (by wire name) loads Claude Code's instructions
    /// (CLAUDE.md and rules), skills, agents, commands, MCP servers, hooks and permissions in its
    /// sessions: one value per agent, whichever account a session uses. It is the value in effect:
    /// the user's choice for the agent, else on for Grok Build and, for Codex, on while Claude Code
    /// is an added agent. Read when a session starts, resumes or switches account.
    #[cfg_attr(test, ts(type = "Partial<Record<Agent, boolean>>"))]
    pub claude_compatibility: std::collections::BTreeMap<String, bool>,
    /// The name the user gave an agent's default account, by agent; an agent without an entry shows
    /// the client's own word for it. Not an account row and not carried by an export.
    #[cfg_attr(test, ts(type = "Partial<Record<Agent, string>>"))]
    pub default_account_names: std::collections::BTreeMap<String, String>,
    /// The agents, by wire name, whose default account the user turned off for new work, as
    /// `Account::disabled` does for an account row. Not carried by an export.
    #[cfg_attr(test, ts(type = "Array<Agent>"))]
    pub disabled_default_accounts: std::collections::BTreeSet<String>,
    /// The application-wide git workflow settings; a project may override each item (see
    /// `Project::git_workflow`).
    pub git_workflow: crate::git_workflow::GitWorkflowSettings,
}

impl Default for Settings {
    /// The settings of a fresh install: the cleanup of archived sessions is on, the rest empty.
    fn default() -> Self {
        Self {
            auto_pull_repositories: false,
            auto_delete_unresumable_archives: true,
            auto_approve_session_requests: false,
            auto_approve_resume_requests: false,
            auto_approve_account_switches: false,
            auto_approve_handoffs: false,
            ask_questions_as_choices: true,
            interface_language: None,
            auto_select_account: false,
            auto_switch_on_usage_limit: false,
            sidebar_order_by_status: true,
            auto_start_quota_windows: false,
            default_clone_dir: String::new(),
            accounts: Vec::new(),
            account_order: Default::default(),
            added_agents: None,
            codex_approval_reviewer: None,
            claude_approval_mode: None,
            claude_compatibility: std::collections::BTreeMap::new(),
            default_account_names: Default::default(),
            disabled_default_accounts: Default::default(),
            git_workflow: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::protocol::{Agent, AgentAvailability, Availability, Event, Settings};

    #[test]
    fn the_trusted_directories_travel_under_their_own_names() {
        let updated = serde_json::to_value(Event::TrustedDirectoriesUpdated {
            trusted_directories: vec!["/work".to_string()],
        })
        .unwrap();
        assert_eq!(updated["type"], "trusted_directories_updated");
        assert_eq!(updated["trusted_directories"][0], "/work");
        let snapshot = serde_json::to_value(Event::Snapshot {
            hosts: vec![],
            consoles: vec![],
            projects: vec![],
            sessions: vec![],
            trusted_directories: vec!["/work".to_string()],
            settings: Settings::default(),
            git_statuses: vec![],
            agent_availability: [Agent::Claude, Agent::Codex, Agent::Grok]
                .into_iter()
                .map(|agent| AgentAvailability {
                    agent,
                    availability: Availability::NotDetermined,
                    default_account_dir: None,
                })
                .collect(),
            account_quotas: vec![],
            account_logins: vec![],
            account_sharing: vec![],
            home_dir: Some("/Users/dev".to_string()),
            shell_terminals: vec![],
        })
        .unwrap();
        assert_eq!(snapshot["trusted_directories"][0], "/work");
        assert_eq!(snapshot["home_dir"], "/Users/dev");
    }
}
