//! The records a client holds and the enums they carry: consoles, projects, sessions, accounts and
//! their availability, report pages, git statuses and shell terminals. See "Records" in
//! `apps/daemon/PROTOCOL.md`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::agents::{AgentId, Launch, PerAgent};
use super::fields::present_option;
use super::git_workflow::GitWorkflowOverride;

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionKind {
    Console,
    Project,
}

impl SessionKind {
    /// The name of this kind of session, as the tools and the messages to a session name it.
    pub fn name(self) -> &'static str {
        match self {
            SessionKind::Console => "console",
            SessionKind::Project => "project",
        }
    }
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Console,
    User,
}

/// A console session's badge colour, one of a fixed palette of sixteen colour families, in hue
/// order around the colour wheel and then the muted ones, assigned on creation and changed
/// afterwards only by the user (see `Session.colour`). The daemon only ever hands the label
/// around; what each one looks like in the light and the dark appearance is the UI's own
/// (`packages/ui/src/sessions/sessionColours.ts`), not here. A stored label must stay readable,
/// so once a release has stored it, one is renamed or removed only together with a schema version
/// step (`SCHEMA_VERSION` in `apps/daemon/src/store/schema.rs`).
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleSessionColour {
    Red,
    Orange,
    Amber,
    Yellow,
    Lime,
    Green,
    Teal,
    Cyan,
    Sky,
    Blue,
    Indigo,
    Violet,
    Fuchsia,
    Pink,
    Brown,
    Gray,
}

impl ConsoleSessionColour {
    /// The palette, in the order the UI lays it out.
    pub const PALETTE: [ConsoleSessionColour; 16] = [
        ConsoleSessionColour::Red,
        ConsoleSessionColour::Orange,
        ConsoleSessionColour::Amber,
        ConsoleSessionColour::Yellow,
        ConsoleSessionColour::Lime,
        ConsoleSessionColour::Green,
        ConsoleSessionColour::Teal,
        ConsoleSessionColour::Cyan,
        ConsoleSessionColour::Sky,
        ConsoleSessionColour::Blue,
        ConsoleSessionColour::Indigo,
        ConsoleSessionColour::Violet,
        ConsoleSessionColour::Fuchsia,
        ConsoleSessionColour::Pink,
        ConsoleSessionColour::Brown,
        ConsoleSessionColour::Gray,
    ];
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Recorded and shown, its agent process still being launched; the status of a session
    /// opened by the `open_session` request until the process is up (one opened for an agent's
    /// `open_session` tool or for an approved session request is recorded the same way, though
    /// its caller is answered only once the launch has settled). No agent has reported anything
    /// yet.
    Starting,
    Working,
    WaitingUser,
    /// A live session whose screen shows one of Claude Code's views of its background agents, which
    /// holds its messages and takes none: the person is looking at the agents, so it is neither
    /// busy nor waiting for an answer. Computed only in the status a client is shown
    /// (`AppState::shown_status`), over a stored `Working` or `Idle`; never stored.
    Dormant,
    Idle,
    Interrupted,
    Archived,
}

impl SessionStatus {
    /// True for the two statuses that mean "no process is running for this session". Both are
    /// resumable; the difference is only how the session got there.
    pub fn has_no_process(self) -> bool {
        matches!(self, SessionStatus::Interrupted | SessionStatus::Archived)
    }
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostKind {
    Local,
    Ssh,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectSource {
    Local,
    Parent,
    /// Clones any git remote, not only GitHub.
    Git,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Host {
    pub id: String,
    pub name: String,
    pub kind: HostKind,
    pub ssh_config: Option<String>,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct Console {
    pub id: String,
    pub name: String,
    pub workdir: String,
    /// The agent a console session of this console is of.
    pub default_agent: AgentId,
    /// The agent a project session falls back to when its project names none; `None` follows
    /// `default_agent`. Read it through [`Console::project_session_default`].
    pub project_session_agent: Option<AgentId>,
    /// The account each agent's sessions opened in this console read, by id; one setting per
    /// agent, and a session reads only its own agent's. `null` means that agent's default account
    /// — the state of pinning nothing. What a referenced account resolves to at launch is
    /// `Account.config_dir`, read through `crate::store`, not carried here.
    pub accounts: PerAgent<Option<String>>,
    /// The model and reasoning effort a console session of this console is launched with, in the
    /// agent's own spelling; one pair per agent, and a session reads only its own agent's. `null`
    /// leaves the choice to the agent. Read when a console session is opened or resumed, so one
    /// already running keeps what it started with; a project session never reads it.
    pub launch: PerAgent<Launch>,
    /// The model and reasoning effort a project session of this console is launched with when
    /// neither the session nor its project names one, in the agent's own spelling; one pair per
    /// agent, as for the console session pair. `null` leaves the choice to the agent. Read at every
    /// launch, so a session already running keeps what it started with.
    pub project_session_launch: PerAgent<Launch>,
    /// A custom avatar as an `image/*` `data:` URL of at most 256 KiB; unset shows the default
    /// glyph.
    pub icon: Option<String>,
    /// The user's own text added after the console session's built-in instructions, inside user
    /// tags; `None` adds nothing. Never reachable through the MCP tools.
    pub console_session_prompt: Option<String>,
    pub created_at: i64,
    /// The version of each field a form edits, by path (`crate::state::FieldVersions`): always
    /// every path of the record. A write names the version it was based on in `base_versions`.
    #[cfg_attr(test, ts(type = "Record<string, number>"))]
    pub versions: BTreeMap<String, u64>,
}

impl Console {
    /// The agent a project session of this console is of when neither the request nor its
    /// project names one.
    pub fn project_session_default(&self) -> AgentId {
        self.project_session_agent.unwrap_or(self.default_agent)
    }

    /// The account id a project's `setting` for `agent` resolves to under this console: its own
    /// account, `None` for the agent's default, and this console's reference when it inherits.
    pub fn resolve_project_account(
        &self,
        agent: AgentId,
        setting: &ProjectAccount,
    ) -> Option<String> {
        match setting {
            ProjectAccount::Account { id } => Some(id.clone()),
            ProjectAccount::Default => None,
            ProjectAccount::Inherit => self.accounts.get(agent).clone(),
        }
    }
}

/// A named config directory of one agent, kept once for the whole application and referred to by
/// id wherever a config directory is referred to — a console's per-agent setting, a session's own
/// copy of it. Every agent also has a *default* account, which is not a row here: it is the state
/// of pinning nothing. Its name is [`DEFAULT_ACCOUNT_NAME`] until the user gives it one of their
/// own (`Settings::default_account_names`, set by `set_default_account_name`), and it cannot be
/// created or removed.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub agent: AgentId,
    /// Required, and unique within this account's agent compared trimmed and case-insensitively —
    /// the default account's own name takes part in that comparison, which is why it is compared
    /// against [`DEFAULT_ACCOUNT_NAME`] rather than left unchecked. Names are not compared across
    /// agents.
    pub name: String,
    /// Absolute, lexically normalised, exactly as a console's pinned directory is stored today.
    /// Existence is not checked when this is set: the directory is created on first launch (by
    /// Claude Code itself, by the Codex adapter for Codex), so a user pointing an account at a
    /// directory they are about to create should not be stopped — this may therefore name a
    /// directory that does not exist yet.
    pub config_dir: String,
    /// Whether the account shares what its directory can share with the agent's default account's
    /// directory, by linking each item of `crate::accounts::shared_config::items` into it. The
    /// user's choice, set by `set_account_sharing`; kept local.
    #[serde(default)]
    pub share_default_config: bool,
    /// The last change toward `share_default_config` did not finish: it failed part-way, or the
    /// daemon stopped while it ran (set when a change starts, cleared when one succeeds). Only
    /// asking again for the recorded value or the opposite change clears it; nothing retries it
    /// by itself.
    #[serde(default)]
    pub share_default_config_incomplete: bool,
    /// The user turned the account off for new work: automatic account selection skips it and no
    /// session can be opened on, handed off to or switched to it (`account_disabled`). Sessions
    /// already on it are untouched, and no usage window is started on it automatically. Set by
    /// `set_account_disabled`, until the user turns it back on.
    #[serde(default)]
    pub disabled: bool,
    /// The version of each field a form edits, by path (`crate::state::FieldVersions`): always
    /// every path of the record once a client is sent it, as part of `Settings::accounts`.
    #[serde(default)]
    #[cfg_attr(test, ts(type = "Record<string, number>"))]
    pub versions: BTreeMap<String, u64>,
}

/// Whether an agent's binary resolves on the user's login shell `PATH`, the only test an agent is
/// held to: whether its config directory holds a login is not Octoboard's business (see
/// `crate::agents::availability`'s module doc). Three states, not two:
/// [`NotDetermined`](Availability::NotDetermined) is where every run of the daemon starts, and
/// only a completed login-shell snapshot ever moves an agent out of it — a snapshot that times out
/// or fails leaves it there, rather than being read as [`Unavailable`](Availability::Unavailable).
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    NotDetermined,
    Available,
    Unavailable,
}

/// One agent's availability and what its default account currently resolves to, derived once per
/// daemon start from one login-shell snapshot and held as its own derived state on `AppState`
/// (`crate::agents::availability`) — never as a field of the stored `Settings`, which is written
/// only by the user's own updates. Travels to a client inside `Event::Snapshot` and
/// `Event::AgentAvailabilityUpdated`, one entry per agent, always three of them: the
/// not-yet-determined state is carried by `availability` itself rather than by the entry's absence.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AgentAvailability {
    pub agent: AgentId,
    pub availability: Availability,
    /// The directory this agent's default account currently resolves to: the directory its own
    /// variable is exported to in the snapshot, else the agent's own usual default. For Grok Build
    /// this is the *source* home a session's per-session home would be built from, never the
    /// per-session home itself. `None` exactly while `availability` is `NotDetermined` — there is
    /// nothing to show yet.
    pub default_account_dir: Option<String>,
}

/// What the default account is called while the user has not named it: also the word every
/// requested account name is compared against, since the daemon carries no locale to compare the
/// user's own translated word against. A client shows its own translation of it, and the user's
/// own name for the account when there is one.
pub const DEFAULT_ACCOUNT_NAME: &str = "Default";

/// The name of `agent`'s default account in `names` (`Settings::default_account_names`): the
/// user's own, else [`DEFAULT_ACCOUNT_NAME`].
pub fn default_account_name_in(
    names: &std::collections::BTreeMap<String, String>,
    agent: AgentId,
) -> &str {
    names
        .get(agent.wire_name())
        .map_or(DEFAULT_ACCOUNT_NAME, String::as_str)
}

/// What a project says about one agent's account, which is one more state than a console's
/// reference has: a console's is an account or the agent's default account, while a project can
/// also decline to say and leave the choice to its console.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectAccount {
    /// Take the console's reference for the agent. What a project that was never given one, and
    /// one whose account was deleted, says.
    #[default]
    Inherit,
    /// The agent's default account, even where the console references a named account: pin
    /// nothing, so the agent uses whatever the login shell exports, else its own config
    /// directory.
    Default,
    /// The account with this id, which belongs to the agent the setting is for.
    Account { id: String },
}

impl ProjectAccount {
    /// The id of the account this names, `None` for the other two states.
    pub fn account_id(&self) -> Option<&str> {
        match self {
            Self::Account { id } => Some(id),
            _ => None,
        }
    }
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub console_id: String,
    pub host_id: String,
    pub name: String,
    pub path: String,
    pub default_agent: Option<AgentId>,
    pub source: ProjectSource,
    pub remote_url: Option<String>,
    /// The account each agent's sessions opened in this project read; one setting per agent, and
    /// a session reads only its own agent's. See [`ProjectAccount`] for the three states.
    #[serde(default)]
    pub accounts: PerAgent<ProjectAccount>,
    /// The model and reasoning effort a session of this project is launched with, in the agent's
    /// own spelling; one pair per agent, and a session reads only its own agent's. `null` leaves
    /// the choice to the agent: there is no console-level fallback, the console's own pairs being
    /// for its console sessions alone. Read when a session is opened or resumed, so one already
    /// running keeps what it started with.
    #[serde(default)]
    pub launch: PerAgent<Launch>,
    /// The user has agreed that Octoboard may press an agent's own trust confirmation for this
    /// project's directory without asking, by sending the keystrokes that accept it (see
    /// `crate::session::trust`). The project form of the one auto-trust permission every agent
    /// shares, a trusted directory being the other: recorded once "Trust and continue" in the
    /// dialog the daemon asks with has pressed a confirmation successfully, whichever agent showed
    /// it, and it covers every later session of any agent in the project. It is not the agent's own
    /// trust record, which each agent writes when its confirmation is pressed.
    pub trust_consent: bool,
    /// The user pinned this project to the top of its console's project list. Only the user's
    /// `update_project` changes it.
    pub pinned: bool,
    /// The user's free-form labels for this project, used to filter the project list. There is no
    /// tag registry: the tags in use are the distinct ones across projects. Stored trimmed,
    /// non-empty and without case-insensitive duplicates, in the order the user gave them.
    pub tags: Vec<String>,
    /// The user's own summary of what the project is for, read by agent sessions. Stored trimmed;
    /// `None` when blank.
    pub description: Option<String>,
    /// The project's own git workflow settings; an item left `null` follows the application-wide
    /// value in `Settings::git_workflow`.
    #[serde(default)]
    pub git_workflow: GitWorkflowOverride,
    /// The version of each field a form edits, by path (`crate::state::FieldVersions`): always
    /// every path of the record. A write names the version it was based on in `base_versions`.
    #[serde(default)]
    #[cfg_attr(test, ts(type = "Record<string, number>"))]
    pub versions: BTreeMap<String, u64>,
}

/// How much the main conversation's latest request carried, as the agent's own session record
/// shows it: a gauge of whether continuing the session is getting expensive, not a running total.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionContext {
    pub tokens: u64,
    /// The context-window limit, present only once the agent has given one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<u64>,
    /// The share of the window in use, in whole percent, by the agent's own formula. Present only
    /// together with `window`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_percent: Option<u8>,
}

/// One piece of work a session's agent left running in the background when its turn ended: a
/// shell command, a monitor, an MCP call or an agent, as the agent itself lists it.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackgroundTask {
    /// The agent's id for the task; the same id its completion notification names.
    pub id: String,
    /// The agent's name for the kind of task (`type` in its payload), absent when it gave none.
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// What the task is, as the agent describes it, absent when it gave none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Coordinator,
    Worker,
}

/// What an agent's own records say a session is called. Stored with the session, never sent: it
/// is here because `Session` holds it and `Session` is both the stored record and the one a client
/// receives.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecordTitles {
    /// A title the agent itself or the user named the conversation with.
    pub named: Option<String>,
    /// A stand-in taken from the conversation's first prompt, for one the agent left unnamed.
    pub prompt: Option<String>,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub agent: AgentId,
    pub agent_session_id: Option<String>,
    pub console_id: String,
    pub project_id: Option<String>,
    pub host_id: String,
    pub kind: SessionKind,
    /// Whether the session may own sessions (`Coordinator`) or reports to one and may not
    /// (`Worker`), independent of the parent in `bound_to`: a project coordinator under a console
    /// session keeps the coordinator role, including when it has no children. A console session
    /// and a session with no parent are always coordinators; a session created under a console
    /// session starts as a coordinator and one created under a project coordinator as a worker.
    /// Binding an independent coordinator to a console session keeps it a coordinator, and a
    /// transfer to another console session preserves that; a transfer to a project session makes
    /// it a worker, and a release makes it a coordinator again.
    pub role: Role,
    pub origin: Origin,
    pub title: String,
    /// Last title emitted by the terminal, retained when its process ends. The store keeps it as
    /// the agent sent it; the record a client receives carries the session's resolved title
    /// instead, which may come from the agent's records and so be set while no terminal title is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_title: Option<String>,
    /// The titles the agent's own records give the session. Stored, never sent.
    #[serde(skip)]
    pub record_titles: RecordTitles,
    /// The session's last known context size, retained after archiving. Absent until reported,
    /// and again after a compaction until the next request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<SessionContext>,
    /// The model the agent's records show the session running with, as the agent spells it. Unlike
    /// `model`, which is the launch override, this is what was observed; absent until reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_model: Option<String>,
    /// The reasoning effort the agent's records show the session running with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_effort: Option<String>,
    /// The reasoning effort the agent runs `observed_model` at by itself, where the records name
    /// that model but no effort and the agent's catalogue gives it a default
    /// (`crate::records::default_effort`). Never stored: it is filled in on the record a client
    /// receives (`AppState::wire_session`), and absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_effort: Option<String>,
    pub status: SessionStatus,
    /// The work the agent has left running in the background while the session is `Idle`: the
    /// session is then waiting for instructions and, at the same time, not finished. Never stored:
    /// it is filled in on the record a client receives (`AppState::wire_session`) from what the
    /// agent's last `Stop` listed, trimmed by completion notifications and cleared by a
    /// `StopFailure`, and is empty for every other status.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub background_tasks: Vec<BackgroundTask>,
    /// Whether an account switch (`switch_session_account`) holds the session right now, from the
    /// moment it takes its claim until it lets it go: the process is being ended, the conversation
    /// copied and the process relaunched. Never stored: it is filled in on the record a client
    /// receives (`AppState::wire_session`), and absent otherwise.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub switching_account: bool,
    /// Whether the session is at its prompt (`idle`) after a turn that ended abnormally: cancelled
    /// by the user or an owner's interrupt, a declined permission prompt or question that ended
    /// the turn, a usage-limit stop, a failure, or the process ending while the turn was in
    /// progress (the agent `working` or `waiting_user`). It lasts until the session's own status
    /// moves off its prompt (the next turn begins), a turn ends normally, a Claude Code
    /// `SessionStart` begins a fresh conversation, or the agent no longer has the conversation
    /// and the next launch opens a fresh one, and outlives the process: a session resumed
    /// after its process ended mid-turn comes back paused. The store keeps it whatever the
    /// status, as `last_turn_abnormal`; the record a client receives (`AppState::wire_session`)
    /// carries it only while the status shown is `idle`, and is absent otherwise.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub paused: bool,
    /// Whether a launch of this session that is pending or under way will hand the agent a
    /// prompt, so that it is `working` once it is up. A new open sets it from the first record
    /// that announces the session; a relaunch sets it inside `relaunch_session_with`, once the
    /// refusals are past and it is known the agent is handed a prompt (an instruction, a continue
    /// prompt, or an owed owner notice). It is cleared when the launch's claim is released. Never
    /// stored: it is filled in on the record a client receives (`AppState::wire_session`), and
    /// absent otherwise.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub prompt_on_launch: bool,
    /// Whether the screen a client is first painted was rebuilt, after a restart of the daemon
    /// that left the session's process running, from the last of the raw output a holder kept of
    /// it, rather than followed from the start: it may differ from what the agent has on its own
    /// screen until the agent redraws it. It lasts as long as the process does. Never stored: it
    /// is filled in on the record a client receives (`AppState::wire_session`), and absent
    /// otherwise.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub screen_inexact: bool,
    /// Whether the holder that runs the session's process can be reached: `pending` while it runs
    /// but cannot be linked to, and absent while it can, or no holder runs the process. While
    /// pending, the process goes on and the session stays claimed: it is neither interrupted nor
    /// resumed, and what is asked of its process fails. Never stored: it is filled in on the
    /// record a client receives (`AppState::wire_session`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub holder_link: Option<HolderLink>,
    /// Whether this session has a conversation on the agent's side: a turn taken, or one resumed
    /// at open. Until it has, the agent has stored no conversation to resume: `claude --resume`
    /// on such an id fails outright with "No conversation found", and a session the user opened
    /// and never typed into is the common case of that. A resume therefore starts a fresh
    /// conversation rather than failing.
    pub has_conversation: bool,
    /// The parent this session reports to, or `None` for an independent session. A parent is a
    /// console session of the same console or a coordinator of the same project. The hierarchy
    /// has at most two reporting edges. Binding and transfer set this id; release clears it.
    /// Console sessions have no parent. Reports route directly by this id, since a console can
    /// have several console sessions. The storage and wire field retain their existing name.
    pub bound_to: Option<String>,
    /// A console session's badge colour, assigned on creation and changed afterwards only by
    /// `set_session_colour` (see `ConsoleSessionColour`). `None` for a project session, which
    /// carries no colour of its own.
    pub colour: Option<ConsoleSessionColour>,
    /// A console session's place in its console's history, one past the highest ordinal ever
    /// used there. Never reused after archiving or deletion. `None` for a project session.
    pub ordinal: Option<i64>,
    /// The account this session's own agent reads, by id: the one chosen for the session, else the
    /// project's reference for that agent, else the console's, as they stood when the session was
    /// opened. `None` means the default account. Written when the session is opened and by a
    /// successful switch of its account, and by nothing else.
    pub account_id: Option<String>,
    /// The configuration directory of this session's own agent that it launches with:
    /// `account_id`'s directory at the time it was recorded, or unset when it names the default
    /// account. An agent keeps a conversation's transcript under that directory, so a resume finds
    /// it only when relaunched with the same one — which is why this is the session's own copy and
    /// a later edit of the account's directory never reaches a session that already exists.
    /// Written together with `account_id`, never alone.
    pub config_dir: Option<String>,
    /// This session's model override: absent inherits its project's (or console's) current
    /// setting, explicit null leaves the choice to the agent, and a value pins the model.
    #[serde(
        default,
        deserialize_with = "present_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(test, ts(optional))]
    pub model: Option<Option<String>>,
    /// The reasoning effort override, with the same inheritance rule as `model`.
    #[serde(
        default,
        deserialize_with = "present_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(test, ts(optional))]
    pub effort: Option<Option<String>>,
    /// The user pinned this session to the top of its list. Only `set_session_pinned` changes it,
    /// and a pinned session stays pinned across archiving and resuming.
    pub pinned: bool,
    /// Prevents archiving and deletion until the user unlocks the session.
    #[serde(default)]
    pub locked: bool,
    pub started_at: i64,
    pub ended_at: Option<i64>,
}

impl Session {
    pub fn is_worker(&self) -> bool {
        self.role == Role::Worker
    }
}

/// One page a session pushed to its report panel: a console session, or a project session that
/// faces the user (`show_page`). It belongs to that session, not to its console or project:
/// `session_id` is the session that pushed it, of either kind. Every page is kept, so the
/// panel can be paged back through.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct Page {
    pub id: String,
    pub session_id: String,
    pub html: String,
    pub created_at: i64,
}

/// One entry of a directory listing. Only directories are ever listed — a project is a directory —
/// so there is no "is this a directory" field to carry.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub is_git_repo: bool,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitActivity {
    Idle,
    Checking,
    Pulling,
    Pushing,
}

/// One project's live git state: its branch and how far it is from its upstream, and whether a
/// check, a fast-forward pull, or a push is in flight right now. Derived from the project's working
/// directory on disk, never stored in SQLite; held in memory by `AppState` and (re)built only when
/// `refresh_git_status` asks for it, with the local branch header also re-read when the project's
/// refs change — the daemon keeps no timer of its own for this (see `apps/daemon/PROTOCOL.md`'s
/// "Daemon behaviour, per project").
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct GitStatus {
    pub project: String,
    /// `false` when the project's directory is not a git repository; every field below is then at
    /// its empty value.
    pub repository: bool,
    /// The branch name; the short commit id when `detached`; `null` when it cannot be read (an
    /// empty repository with no commit yet, or a failed read).
    pub branch: Option<String>,
    pub detached: bool,
    /// The configured upstream ref, e.g. `origin/main`; `null` when the branch has none, which is
    /// also when `ahead` and `behind` are meaningless and both `0`.
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub activity: GitActivity,
    /// The verbatim, untranslatable `git` or operating-system message from the last failed step,
    /// cleared only by that same step succeeding again — a failed fetch does not stop the local
    /// read, so a status can carry both an error and usable numbers.
    pub error: Option<String>,
    /// The code of the failure `error` describes where the daemon has one, so a client words it
    /// itself: only `binary_not_found`, for a `git` that cannot be found on the login shell's
    /// `PATH`. Left out for any other failure, and with no `error`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub error_code: Option<super::codes::Code>,
}

/// Whether a shell terminal's shell is still running.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellTerminalState {
    Running,
    /// The shell has exited on its own. The record stays, with its output still replayed to a
    /// terminal client that attaches, until a client closes it.
    Exited,
}

/// A plain shell the daemon runs in a project's directory, beside its agent sessions. Live state
/// held in memory, not stored in SQLite but for the name the user gave it: the record lasts as
/// long as the shell, which its holder keeps running across a restart of the daemon.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShellTerminal {
    pub id: String,
    pub project_id: String,
    /// The worktree of the project's repository it was started in (`worktree` of the browse
    /// requests); null for the project's own directory.
    pub worktree: Option<String>,
    /// The directory the shell was started in, with its symbolic links resolved. Where the shell
    /// is now is its own business: a `cd` in it changes nothing here.
    pub cwd: String,
    /// What a tab shows: the title the shell last set with an OSC 0 or 2 sequence, the shell's
    /// name when it has set none (or set an empty one), or the user's name for it once renamed.
    pub title: String,
    /// The user renamed it, so the shell's own titles no longer change `title`.
    pub renamed: bool,
    pub state: ShellTerminalState,
    /// The shell is running something: the terminal's foreground process group is not the
    /// shell's own, or the shell has a child process (a job sent to the background included).
    /// Always false once exited.
    pub busy: bool,
    /// When it was created, in milliseconds since the epoch; a project's terminals are listed in
    /// this order.
    pub created_at: i64,
    /// The screen a client is first painted was rebuilt, after a restart of the daemon that left
    /// the shell running, from the last of the raw output a holder kept of it, rather than
    /// followed from the start: it may differ from what the shell has on its own screen until it
    /// redraws it. Absent otherwise; it lasts as long as the terminal does.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(test, ts(optional = nullable))]
    pub screen_inexact: bool,
    /// Whether the holder that runs the shell can be reached: `pending` while it runs but cannot
    /// be linked to, the shell going on unreachable, and absent while it can.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub holder_link: Option<HolderLink>,
}

/// How a holder that runs a session's process or a shell terminal's shell stands, when it cannot
/// be reached.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HolderLink {
    /// Its process runs but the daemon cannot link to it for now, and keeps trying.
    Pending,
}

#[cfg(test)]
mod tests {
    use crate::protocol::{
        AgentAvailability, AgentId, Availability, ConsoleSessionColour, Event, Request,
        RequestBody, Settings, ShellTerminal, ShellTerminalState,
    };

    /// An inexact screen is sent as `true` on a session and on a shell terminal, and left out
    /// otherwise.
    #[test]
    fn an_inexact_screen_is_sent_only_while_it_holds() {
        let mut terminal = ShellTerminal {
            id: "t1".to_string(),
            project_id: "p1".to_string(),
            worktree: None,
            cwd: "/work/p".to_string(),
            title: "zsh".to_string(),
            renamed: false,
            state: ShellTerminalState::Running,
            busy: false,
            created_at: 7,
            screen_inexact: false,
            holder_link: None,
        };
        let mut session =
            crate::test_support::bare_session("s1", crate::protocol::SessionKind::Project);
        let sent = |value: serde_json::Value| value.get("screen_inexact").cloned();
        assert_eq!(sent(serde_json::to_value(&terminal).unwrap()), None);
        assert_eq!(sent(serde_json::to_value(&session).unwrap()), None);
        terminal.screen_inexact = true;
        session.screen_inexact = true;
        assert_eq!(
            sent(serde_json::to_value(&terminal).unwrap()),
            Some(serde_json::json!(true))
        );
        assert_eq!(
            sent(serde_json::to_value(&session).unwrap()),
            Some(serde_json::json!(true))
        );
    }

    /// A holder that cannot be reached is sent as `pending` on a session and on a shell terminal,
    /// and left out otherwise.
    #[test]
    fn a_holder_that_cannot_be_reached_is_sent_only_while_it_cannot() {
        let mut terminal = ShellTerminal {
            id: "t1".to_string(),
            project_id: "p1".to_string(),
            worktree: None,
            cwd: "/work/p".to_string(),
            title: "zsh".to_string(),
            renamed: false,
            state: ShellTerminalState::Running,
            busy: false,
            created_at: 7,
            screen_inexact: false,
            holder_link: None,
        };
        let mut session =
            crate::test_support::bare_session("s1", crate::protocol::SessionKind::Project);
        let sent = |value: serde_json::Value| value.get("holder_link").cloned();
        assert_eq!(sent(serde_json::to_value(&terminal).unwrap()), None);
        assert_eq!(sent(serde_json::to_value(&session).unwrap()), None);
        terminal.holder_link = Some(crate::protocol::HolderLink::Pending);
        session.holder_link = Some(crate::protocol::HolderLink::Pending);
        assert_eq!(
            sent(serde_json::to_value(&terminal).unwrap()),
            Some(serde_json::json!("pending"))
        );
        assert_eq!(
            sent(serde_json::to_value(&session).unwrap()),
            Some(serde_json::json!("pending"))
        );
    }

    /// A shell terminal travels in the snapshot and in events of its own in the shape
    /// `PROTOCOL.md` and the generated `packages/ui/src/protocol.generated.ts` give it, and a
    /// terminal is created in the project's own directory unless a worktree is named.
    #[test]
    fn shell_terminals_travel_in_the_documented_shape() {
        let terminal = ShellTerminal {
            id: "t1".to_string(),
            project_id: "p1".to_string(),
            worktree: Some("w1".to_string()),
            cwd: "/work/p".to_string(),
            title: "zsh".to_string(),
            renamed: false,
            state: ShellTerminalState::Exited,
            busy: false,
            created_at: 7,
            screen_inexact: false,
            holder_link: None,
        };
        let record = serde_json::json!({
            "id": "t1", "project_id": "p1", "worktree": "w1", "cwd": "/work/p", "title": "zsh",
            "renamed": false, "state": "exited", "busy": false, "created_at": 7,
        });
        let event = |event: Event| serde_json::to_value(event).unwrap();
        assert_eq!(
            event(Event::ShellTerminalUpserted {
                shell_terminal: terminal.clone()
            }),
            serde_json::json!({"type": "shell_terminal_upserted", "shell_terminal": record})
        );
        assert_eq!(
            event(Event::ShellTerminalCreated {
                id: Some("r1".to_string()),
                shell_terminal: terminal.clone()
            }),
            serde_json::json!({"type": "shell_terminal_created", "id": "r1", "shell_terminal": record})
        );
        assert_eq!(
            event(Event::ShellTerminalDeleted {
                shell_terminal: "t1".to_string()
            }),
            serde_json::json!({"type": "shell_terminal_deleted", "shell_terminal": "t1"})
        );
        let snapshot = event(Event::Snapshot {
            hosts: vec![],
            consoles: vec![],
            projects: vec![],
            sessions: vec![],
            trusted_directories: vec![],
            settings: Settings::default(),
            providers: Vec::new(),
            git_statuses: vec![],
            agent_availability: vec![],
            account_quotas: vec![],
            account_logins: vec![],
            account_sharing: vec![],
            agent_cli_statuses: vec![],
            home_dir: None,
            host: crate::platform::HostFacts::current().into(),
            keep_awake_held: false,
            shell_terminals: vec![terminal],
        });
        assert_eq!(snapshot["shell_terminals"], serde_json::json!([record]));
        assert_eq!(
            serde_json::to_value(ShellTerminalState::Running).unwrap(),
            "running"
        );

        let worktree = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
            RequestBody::CreateShellTerminal { worktree, .. } => worktree,
            _ => panic!("parses as create_shell_terminal"),
        };
        assert_eq!(
            worktree(r#"{"type":"create_shell_terminal","project":"p"}"#),
            None
        );
        assert_eq!(
            worktree(r#"{"type":"create_shell_terminal","project":"p","worktree":"w"}"#),
            Some("w".to_string())
        );
        let path = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
            RequestBody::CreateShellTerminal { path, .. } => path,
            _ => panic!("parses as create_shell_terminal"),
        };
        assert_eq!(
            path(r#"{"type":"create_shell_terminal","project":"p"}"#),
            ""
        );
        assert_eq!(
            path(r#"{"type":"create_shell_terminal","project":"p","path":"src/a%25"}"#),
            "src/a%25"
        );
    }

    /// The not-yet-determined state has to be representable on the wire, not implied by an
    /// absent field: a client reading `availability` must be able to tell "not checked yet" from
    /// "checked and found unavailable" without inferring either from whether the entry exists at
    /// all or from `default_account_dir` being present.
    #[test]
    fn not_yet_determined_is_an_explicit_value_not_an_absent_entry() {
        let entry = AgentAvailability {
            agent: AgentId::GROK,
            availability: Availability::NotDetermined,
            default_account_dir: None,
        };
        let json = serde_json::to_value(&entry).unwrap();
        assert_eq!(json["agent"], "grok");
        assert_eq!(json["availability"], "not_determined");
        assert!(json["default_account_dir"].is_null());

        let determined = AgentAvailability {
            agent: AgentId::GROK,
            availability: Availability::Unavailable,
            default_account_dir: Some("/home/user/.grok".to_string()),
        };
        let json = serde_json::to_value(&determined).unwrap();
        assert_eq!(json["availability"], "unavailable");
        assert_eq!(json["default_account_dir"], "/home/user/.grok");
    }

    /// A colour is its snake_case family name on the wire, for each of the 16 entries and no other
    /// text, whether from the palette of an earlier release or a shade of the one before.
    #[test]
    fn a_console_session_colour_is_its_family_name_on_the_wire() {
        for colour in ConsoleSessionColour::PALETTE {
            let text = serde_json::to_string(&colour).unwrap();
            assert_eq!(
                serde_json::from_str::<ConsoleSessionColour>(&text).unwrap(),
                colour
            );
        }
        assert_eq!(
            serde_json::to_string(&ConsoleSessionColour::Fuchsia).unwrap(),
            r#""fuchsia""#
        );
        for text in [r#""olive""#, r#""purple""#, r#""red-2""#, r#""Red""#] {
            assert!(
                serde_json::from_str::<ConsoleSessionColour>(text).is_err(),
                "{text}"
            );
        }
    }
}
