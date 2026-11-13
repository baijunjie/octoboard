//! The app-wide settings record.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::agents::AgentId;
use super::git_workflow::GitWorkflowSettings;
use super::records::Account;
use crate::providers::SettingValue;

/// When Prevent sleep holds the daemon's host awake (`Settings.keep_awake`).
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeepAwakeWhen {
    /// For as long as the setting is on.
    Always,
    /// While a session is working: starting, working or dormant, or idle with work left running in
    /// the background (any console's, any project's).
    #[default]
    Working,
}

impl KeepAwakeWhen {
    /// The name it has on the wire, which is what the serialization writes.
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::Working => "working",
        }
    }

    /// The value named `text` on the wire, or `None` for a name this build does not know.
    pub fn from_wire_name(text: &str) -> Option<Self> {
        [Self::Always, Self::Working]
            .into_iter()
            .find(|value| value.wire_name() == text)
    }
}

/// The app-wide user settings the daemon stores, and the account list alongside them: a record
/// that already grows without a new request shape or a new event for every addition, which is
/// why the accounts live here rather than in a record of their own.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize)]
pub struct Settings {
    /// Whether explicitly closing a project Terminal panel closes its idle shell terminals.
    /// Off unless the user turns it on; running programs are kept.
    pub auto_close_idle_shell_terminals: bool,
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
    /// Whether a project session is kept to its own project: its git is refused a ref update in
    /// the repository of another registered project, and its file-edit tools are refused a path
    /// in one. Off unless the user turned it on. The git check reads it at every ref update, so a
    /// change reaches running sessions at once; the edit check keeps the value a session was
    /// launched with, so a change reaches it only in sessions launched afterwards. Only the
    /// user's own client changes it.
    pub keep_sessions_in_own_project: bool,
    /// The tag of the interface language, one of the tags the interface offers; `null` until a
    /// client reports one. It is the one language every client shows (each keeps a copy only to
    /// render in before it has connected) and the one sessions are told to reply in, so a change
    /// reaches every window. Carried by a configuration export in its General section.
    pub interface_language: Option<String>,
    /// Prevent sleep: whether the daemon holds its host awake (see `crate::keep_awake`), when
    /// `keep_awake_when` says it should. Off unless the user turned it on. It acts only where the
    /// host can be held (`HostInfo::keep_awake`); elsewhere it is stored and does nothing.
    pub keep_awake: bool,
    /// When Prevent sleep holds. `working` unless the user chose `always`.
    pub keep_awake_when: KeepAwakeWhen,
    /// Whether a new session whose account was left out runs on the account of its agent whose
    /// weekly quota resets soonest and still has some left. Off unless the user turned it on.
    /// Without a weekly pick, try the inherited account, then the first usable in saved order.
    /// A new session with no named agent or account also tries other installed, added agents in
    /// added order when the preferred agent has no usable account. A resume, switch or handoff
    /// keeps its target agent.
    pub auto_select_account: bool,
    /// Whether Octoboard itself moves a session to another account when its turn stopped on the
    /// account's usage limit (see `crate::coordinator::auto_switch`). Off unless the user turned it
    /// on. Only that: it does not choose a new session's account (`auto_select_account`) or answer
    /// an agent's request for a switch (`auto_approve_account_switches`).
    pub auto_switch_on_usage_limit: bool,
    /// Whether the daemon starts an account's usage window that has reset but not begun counting,
    /// by sending one minimal request on that account. Off unless the user turned it on.
    pub auto_start_quota_windows: bool,
    /// Whether the daemon, once it has started, resumes the sessions that are interrupted then,
    /// whichever run interrupted them (see `coordinator::resume_on_start`). Off unless the user
    /// turned it on; read at each start.
    pub resume_sessions_on_start: bool,
    /// The directory a `git` association clones into when none is named: the one the user set, or
    /// `~/Projects` (expanded) while none is. Always an absolute, lexically normalised path.
    pub default_clone_dir: String,
    /// Every account of every agent, application-wide. The default account of each agent is not
    /// among these — it is the state of pinning nothing, not a row (see `Account`).
    pub accounts: Vec<Account>,
    /// The user's order of each agent's accounts, by agent wire name, which clients list them in
    /// and automatic account selection takes them in; null entries represent the default account.
    /// Missing accounts are appended and stale IDs are ignored.
    #[cfg_attr(test, ts(type = "Partial<Record<Agent, Array<string | null>>>"))]
    pub account_order: std::collections::BTreeMap<String, Vec<Option<String>>>,
    /// The agents the user has added, in the order the user set; a newly added agent goes last. Only these are offered for a
    /// new choice; removing one leaves its accounts here. `null` while the daemon has not yet
    /// decided the list (it waits for the agents' availability), which is not the same as the
    /// empty list of a user who removed them all.
    pub added_agents: Option<Vec<AgentId>>,
    /// The value in effect of every setting of every provider (see `ProviderInfo::settings`), by
    /// provider id and then setting id: each provider the daemon registers has an entry and each of
    /// its settings a key, and the type is `Partial` only because a client may index it by an id
    /// the daemon does not serve. A switch holds the user's choice, else its default; a choice
    /// holds the user's, else `null`, which follows each account's own configuration. Read when a
    /// session starts, resumes or switches account. The choices of a setting its provider does not
    /// export (`exported` false) are kept local to this machine; an exported one travels with a
    /// configuration export.
    #[cfg_attr(
        test,
        ts(type = "Partial<Record<Agent, Record<string, boolean | string | null>>>")
    )]
    pub provider_settings: BTreeMap<String, BTreeMap<String, Option<SettingValue>>>,
    /// The tags the user put on each agent (by wire name), which tell a session choosing an agent
    /// what it is good for. Application-wide, and they apply to every account of the agent. An
    /// agent without tags has no entry. An agent removed from `added_agents` keeps its entry, so
    /// its tags are back when it is added again; a client offers and shows only the added agents'.
    #[cfg_attr(test, ts(type = "Partial<Record<Agent, Array<string>>>"))]
    pub agent_tags: std::collections::BTreeMap<String, Vec<String>>,
    /// The name the user gave an agent's default account, by agent; an agent without an entry shows
    /// the client's own word for it. Not an account row.
    #[cfg_attr(test, ts(type = "Partial<Record<Agent, string>>"))]
    pub default_account_names: std::collections::BTreeMap<String, String>,
    /// The agents, by wire name, whose default account the user turned off for new work, as
    /// `Account::disabled` does for an account row.
    #[cfg_attr(test, ts(type = "Array<Agent>"))]
    pub disabled_default_accounts: std::collections::BTreeSet<String>,
    /// The application-wide git workflow settings; a project may override each item (see
    /// `Project::git_workflow`).
    pub git_workflow: GitWorkflowSettings,
    /// The version of each field a form edits, by path (`crate::state::FieldVersions`): always
    /// every path of the record. A write names the version it was based on in `base_versions`.
    #[cfg_attr(test, ts(type = "Record<string, number>"))]
    pub versions: BTreeMap<String, u64>,
}

impl Default for Settings {
    /// The settings of a fresh install: the cleanup of archived sessions is on, the rest empty.
    fn default() -> Self {
        Self {
            auto_close_idle_shell_terminals: false,
            auto_pull_repositories: false,
            auto_delete_unresumable_archives: true,
            auto_approve_session_requests: false,
            auto_approve_resume_requests: false,
            auto_approve_account_switches: false,
            auto_approve_handoffs: false,
            ask_questions_as_choices: true,
            keep_sessions_in_own_project: false,
            interface_language: None,
            keep_awake: false,
            keep_awake_when: KeepAwakeWhen::Working,
            auto_select_account: false,
            auto_switch_on_usage_limit: false,
            auto_start_quota_windows: false,
            resume_sessions_on_start: false,
            default_clone_dir: String::new(),
            accounts: Vec::new(),
            account_order: Default::default(),
            added_agents: None,
            provider_settings: BTreeMap::new(),
            agent_tags: std::collections::BTreeMap::new(),
            default_account_names: Default::default(),
            disabled_default_accounts: Default::default(),
            git_workflow: Default::default(),
            versions: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::protocol::{AgentAvailability, AgentId, Availability, Event, Settings};

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
            providers: Vec::new(),
            git_statuses: vec![],
            agent_availability: [AgentId::CLAUDE, AgentId::CODEX, AgentId::GROK]
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
            agent_cli_statuses: vec![],
            home_dir: Some("/Users/dev".to_string()),
            host: crate::platform::HostFacts::current().into(),
            keep_awake_held: false,
            shell_terminals: vec![],
        })
        .unwrap();
        assert_eq!(snapshot["trusted_directories"][0], "/work");
        assert_eq!(snapshot["home_dir"], "/Users/dev");
    }
}
