//! The wire types of the daemon's only external interface. `apps/daemon/PROTOCOL.md` is the
//! specification; this module is its Rust form.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

/// The named values a coded message is filled with. A parameter that names a console, a project or
/// a session is that record's id under the param name `console`, `project` or `session`, so the
/// client can show the record's current name; anything else is passed as text, verbatim.
pub type Params = BTreeMap<String, String>;

fn params(pairs: &[(&str, &str)]) -> Params {
    pairs
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Agent {
    Claude,
    Codex,
    Grok,
}

impl Agent {
    /// Every agent, in the order a per-agent listing keeps.
    pub const ALL: [Agent; 3] = [Agent::Claude, Agent::Codex, Agent::Grok];

    /// How the agent is named on the wire and in the console session's tools.
    pub fn wire_name(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
            Agent::Grok => "grok",
        }
    }

    /// The agent [`Self::wire_name`] names.
    pub fn from_wire_name(name: &str) -> Option<Agent> {
        Agent::ALL
            .into_iter()
            .find(|agent| agent.wire_name() == name)
    }

    /// How the agent is named in a message shown to the user.
    pub fn label(self) -> &'static str {
        match self {
            Agent::Claude => "Claude Code",
            Agent::Codex => "Codex",
            Agent::Grok => "Grok Build",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Console,
    Project,
}

impl Role {
    /// The kind of session this role makes, as the tools and the messages to a session name it.
    pub fn kind(self) -> &'static str {
        match self {
            Role::Console => "console",
            Role::Project => "project",
        }
    }
}

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
/// (`packages/ui/src/sessionColours.ts`), not here. The labels of the earlier palettes are
/// rewritten when the store opens (`Store::open`); from this one on a label is never renamed or
/// removed, so a database written under it stays readable.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Recorded and shown, its agent process still being launched; the status of a session
    /// opened by `open_session` until the process is up (one opened for an agent's own
    /// `start_session` or for an approved session request is recorded the same way, though
    /// its caller is answered only once the launch has settled). No agent has reported anything
    /// yet.
    Starting,
    Working,
    WaitingUser,
    Idle,
    Interrupted,
    Archived,
}

impl SessionStatus {
    /// True for the two statuses that mean "no process is running for this session". Both are
    /// resumable; the difference is only how the session got there.
    pub fn is_dormant(self) -> bool {
        matches!(self, SessionStatus::Interrupted | SessionStatus::Archived)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostKind {
    Local,
    Ssh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectSource {
    Local,
    Parent,
    /// Clones any git remote, not only GitHub. `github` is what this value was called before and is
    /// still read, in a request and in a stored project; it is never written.
    #[serde(alias = "github")]
    Git,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Host {
    pub id: String,
    pub name: String,
    pub kind: HostKind,
    pub ssh_config: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Console {
    pub id: String,
    pub name: String,
    pub workdir: String,
    /// Legacy copy of `default_agent`, kept equal on every save and reconciled on open for older
    /// databases. Session agent selection reads `default_agent` instead; nothing reads this field.
    /// Daemon-only: not part of the record clients are sent.
    #[serde(skip)]
    pub console_session_agent: Agent,
    pub default_agent: Agent,
    /// The account each agent's sessions opened in this console read, by id; one setting per
    /// agent, and a session reads only its own agent's. `None` means that agent's default account
    /// — the state of pinning nothing. What a referenced account resolves to at launch is
    /// `Account.config_dir`, read through `crate::store`, not carried here.
    pub claude_account_id: Option<String>,
    /// Codex's own reference; see `claude_account_id`.
    pub codex_account_id: Option<String>,
    /// Grok's own reference; see `claude_account_id`.
    pub grok_account_id: Option<String>,
    /// The model a console session of this console is launched with, in Claude Code's own
    /// spelling; one setting per agent, and a session reads only its own agent's. `None` leaves
    /// the choice to the agent. Read when a console session is opened or resumed, so one already
    /// running keeps what it started with; a project session never reads it.
    pub claude_console_session_model: Option<String>,
    /// The reasoning effort a console session is launched with; see
    /// `claude_console_session_model`.
    pub claude_console_session_effort: Option<String>,
    /// Codex's own model; see `claude_console_session_model`.
    pub codex_console_session_model: Option<String>,
    /// Codex's own effort; see `claude_console_session_effort`.
    pub codex_console_session_effort: Option<String>,
    /// Grok's own model; see `claude_console_session_model`.
    pub grok_console_session_model: Option<String>,
    /// Grok's own effort; see `claude_console_session_effort`.
    pub grok_console_session_effort: Option<String>,
    /// A custom avatar as an `image/*` `data:` URL of at most 256 KiB; unset shows the default
    /// glyph.
    pub icon: Option<String>,
    /// The user's own text added after the console session's built-in instructions, inside user
    /// tags; `None` adds nothing. Never reachable through the MCP tools.
    pub console_session_prompt: Option<String>,
    pub created_at: i64,
}

impl Console {
    /// The account this console references for `agent`; `None` is the agent's default account.
    pub fn account(&self, agent: Agent) -> &Option<String> {
        match agent {
            Agent::Claude => &self.claude_account_id,
            Agent::Codex => &self.codex_account_id,
            Agent::Grok => &self.grok_account_id,
        }
    }

    pub fn account_mut(&mut self, agent: Agent) -> &mut Option<String> {
        match agent {
            Agent::Claude => &mut self.claude_account_id,
            Agent::Codex => &mut self.codex_account_id,
            Agent::Grok => &mut self.grok_account_id,
        }
    }

    /// `agent`'s console session model and effort.
    pub fn launch(&self, agent: Agent) -> (&Option<String>, &Option<String>) {
        match agent {
            Agent::Claude => (
                &self.claude_console_session_model,
                &self.claude_console_session_effort,
            ),
            Agent::Codex => (
                &self.codex_console_session_model,
                &self.codex_console_session_effort,
            ),
            Agent::Grok => (
                &self.grok_console_session_model,
                &self.grok_console_session_effort,
            ),
        }
    }

    pub fn launch_mut(&mut self, agent: Agent) -> (&mut Option<String>, &mut Option<String>) {
        match agent {
            Agent::Claude => (
                &mut self.claude_console_session_model,
                &mut self.claude_console_session_effort,
            ),
            Agent::Codex => (
                &mut self.codex_console_session_model,
                &mut self.codex_console_session_effort,
            ),
            Agent::Grok => (
                &mut self.grok_console_session_model,
                &mut self.grok_console_session_effort,
            ),
        }
    }

    /// The account id a project's `setting` for `agent` resolves to under this console: its own
    /// account, `None` for the agent's default, and this console's reference when it inherits.
    pub fn resolve_project_account(
        &self,
        agent: Agent,
        setting: &ProjectAccount,
    ) -> Option<String> {
        match setting {
            ProjectAccount::Account { id } => Some(id.clone()),
            ProjectAccount::Default => None,
            ProjectAccount::Inherit => self.account(agent).clone(),
        }
    }
}

/// Who resolves Codex approval requests. An absent override follows the account configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalReviewer {
    User,
    AutoReview,
}

/// A named config directory of one agent, kept once for the whole application and referred to by
/// id wherever a config directory is referred to — a console's per-agent setting, a session's own
/// copy of it. Every agent also has a *default* account, which is not a row here: it is the state
/// of pinning nothing, its name is Octoboard's own untranslatable-here word for it (derived where
/// it is shown, not stored), and it cannot be created, renamed or removed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub agent: Agent,
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
    /// Codex-only launch override; never written into the agent's own configuration.
    #[serde(default)]
    pub approval_reviewer: Option<ApprovalReviewer>,
}

/// Whether an agent's binary resolves on the user's login shell `PATH`, the only test an agent is
/// held to: whether its config directory holds a login is not Octoboard's business (see
/// `crate::availability`'s module doc). Three states, not two:
/// [`NotDetermined`](Availability::NotDetermined) is where every run of the daemon starts, and
/// only a completed login-shell snapshot ever moves an agent out of it — a snapshot that times out
/// or fails leaves it there, rather than being read as [`Unavailable`](Availability::Unavailable).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    NotDetermined,
    Available,
    Unavailable,
}

/// One agent's availability and what its default account currently resolves to, derived once per
/// daemon start from one login-shell snapshot and held as its own derived state on `AppState`
/// (`crate::availability`) — never as a field of the stored `Settings`, which is written only by
/// the user's own updates. Travels to a client inside
/// `Event::Snapshot` and `Event::AgentAvailabilityUpdated`, one entry per agent, always three of
/// them: the not-yet-determined state is carried by `availability` itself rather than by the
/// entry's absence.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AgentAvailability {
    pub agent: Agent,
    pub availability: Availability,
    /// The directory this agent's default account currently resolves to: the directory its own
    /// variable is exported to in the snapshot, else the agent's own usual default. For Grok Build
    /// this is the *source* home a session's per-session home would be built from, never the
    /// per-session home itself. `None` exactly while `availability` is `NotDetermined` — there is
    /// nothing to show yet.
    pub default_account_dir: Option<String>,
}

/// The comparison key the daemon uses for the default account's name when checking a requested
/// account name for a collision — in English, since the daemon carries no locale and what the
/// default account is actually *shown* as is a client-side, per-locale concern derived separately
/// (see `Account`'s doc comment). Not sent to a client and not meant to be displayed; it exists
/// only so `claude_account_id: None` has something to compare a new name against.
pub const DEFAULT_ACCOUNT_NAME: &str = "Default";

/// What a project says about one agent's account, which is one more state than a console's
/// reference has: a console's is an account or the agent's default account, while a project can
/// also decline to say and leave the choice to its console.
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub console_id: String,
    pub host_id: String,
    pub name: String,
    pub path: String,
    pub default_agent: Option<Agent>,
    pub source: ProjectSource,
    pub remote_url: Option<String>,
    /// The account each agent's sessions opened in this project read; one setting per agent, and
    /// a session reads only its own agent's. See [`ProjectAccount`] for the three states.
    #[serde(default)]
    pub claude_account_id: ProjectAccount,
    /// Codex's own reference; see `claude_account_id`.
    #[serde(default)]
    pub codex_account_id: ProjectAccount,
    /// Grok's own reference; see `claude_account_id`.
    #[serde(default)]
    pub grok_account_id: ProjectAccount,
    /// The model a session of this project is launched with, in Claude Code's own spelling; one
    /// setting per agent, and a session reads only its own agent's. `None` leaves the choice to
    /// the agent: there is no console-level fallback, the console's own pair being for its console
    /// sessions alone. Read when a session is opened or resumed, so one already running keeps what
    /// it started with.
    pub claude_session_model: Option<String>,
    /// The reasoning effort a session of this project is launched with; see
    /// `claude_session_model`.
    pub claude_session_effort: Option<String>,
    /// Codex's own model; see `claude_session_model`.
    pub codex_session_model: Option<String>,
    /// Codex's own effort; see `claude_session_effort`.
    pub codex_session_effort: Option<String>,
    /// Grok's own model; see `claude_session_model`.
    pub grok_session_model: Option<String>,
    /// Grok's own effort; see `claude_session_effort`.
    pub grok_session_effort: Option<String>,
    /// The user has agreed that Octoboard may press an agent's own trust confirmation for this
    /// project's directory without asking, by sending the keystrokes that accept it (see
    /// `crate::trust`). The project form of the one auto-trust permission every agent shares, a
    /// trusted directory being the other: recorded once "Trust and continue" in the dialog the
    /// daemon asks with has pressed a confirmation successfully, whichever agent showed it, and it
    /// covers every later session of any agent in the project. It is not the agent's own trust
    /// record, which each agent writes when its confirmation is pressed.
    pub trust_consent: bool,
    /// The user pinned this project to the top of its console's project list. Only the user's
    /// `update_project` changes it.
    pub pinned: bool,
    /// The user's free-form labels for this project, used to filter the project list. There is no
    /// tag registry: the tags in use are the distinct ones across projects. Stored trimmed,
    /// non-empty and without case-insensitive duplicates, in the order the user gave them.
    pub tags: Vec<String>,
}

impl Project {
    /// The account setting for `agent`.
    pub fn account(&self, agent: Agent) -> &ProjectAccount {
        match agent {
            Agent::Claude => &self.claude_account_id,
            Agent::Codex => &self.codex_account_id,
            Agent::Grok => &self.grok_account_id,
        }
    }

    pub fn account_mut(&mut self, agent: Agent) -> &mut ProjectAccount {
        match agent {
            Agent::Claude => &mut self.claude_account_id,
            Agent::Codex => &mut self.codex_account_id,
            Agent::Grok => &mut self.grok_account_id,
        }
    }

    /// `agent`'s session model and effort.
    pub fn launch(&self, agent: Agent) -> (&Option<String>, &Option<String>) {
        match agent {
            Agent::Claude => (&self.claude_session_model, &self.claude_session_effort),
            Agent::Codex => (&self.codex_session_model, &self.codex_session_effort),
            Agent::Grok => (&self.grok_session_model, &self.grok_session_effort),
        }
    }

    pub fn launch_mut(&mut self, agent: Agent) -> (&mut Option<String>, &mut Option<String>) {
        match agent {
            Agent::Claude => (
                &mut self.claude_session_model,
                &mut self.claude_session_effort,
            ),
            Agent::Codex => (
                &mut self.codex_session_model,
                &mut self.codex_session_effort,
            ),
            Agent::Grok => (&mut self.grok_session_model, &mut self.grok_session_effort),
        }
    }
}

/// Cumulative usage reported by the agent's own session record. Cache counts are subsets of input.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write_input_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub agent: Agent,
    pub agent_session_id: Option<String>,
    pub console_id: String,
    pub project_id: Option<String>,
    pub host_id: String,
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
    pub record_titles: crate::session_title::RecordTitles,
    /// Last known native cumulative usage, retained after archiving. Absent until reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<SessionUsage>,
    pub status: SessionStatus,
    /// Whether this session has a conversation on the agent's side: a turn taken, or one resumed
    /// at open. Until it has, the agent has stored no conversation to resume: `claude --resume`
    /// on such an id fails outright with "No conversation found", and a session the user opened
    /// and never typed into is the common case of that. A resume therefore starts a fresh
    /// conversation rather than failing.
    pub has_conversation: bool,
    /// The session this session is bound to — its owner, which it reports to — or `None` for one
    /// with no owner, which the user owns. The owner is a console session of the same console, or
    /// a project session of the same project that can own sessions (see [`Self::has_bound_role`]),
    /// so orchestration is at most two levels deep. Set when the session is created, and changed
    /// afterwards only by the session binding itself to an owner
    /// (`crate::coordinator::bind_session`) or by its owner handing it to another
    /// (`crate::coordinator::transfer_session`); never cleared. A console session is never bound,
    /// so this is always `None` for one of those. Reports are routed by this field
    /// (`crate::reporting::deliver_report`): a binding names who to report to directly, rather
    /// than asking whether the console happens to have one console session to find by lookup.
    pub bound_to: Option<String>,
    /// Whether this is a lead session: a project session bound to a console session that can still
    /// own sessions of its own project, having been unbound when it was opened and bound only
    /// afterwards. It keeps the tools of an unbound project session, and reports to its console
    /// session. Stored rather than worked out from whether it owns sessions, because a lead
    /// session that has started none yet, or whose sessions have all been deleted, is still one.
    /// Set by binding to a console session, kept by a transfer to another console session, and
    /// cleared by a transfer to a project session, which leaves it unable to own any. Daemon-only:
    /// not part of the record clients are sent.
    #[serde(skip)]
    pub lead: bool,
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
    pub model: Option<Option<String>>,
    /// The reasoning effort override, with the same inheritance rule as `model`.
    #[serde(
        default,
        deserialize_with = "present_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub effort: Option<Option<String>>,
    /// The user pinned this session to the top of its list. Only `set_session_pinned` changes it,
    /// and a pinned session stays pinned across archiving and resuming.
    pub pinned: bool,
    pub started_at: i64,
    pub ended_at: Option<i64>,
}

impl Session {
    /// Whether this session has the role of a session bound to an owner: it reports, and can own
    /// no session of its own, so it is offered the bound project session's tools. A lead session
    /// is bound and does not have it. It follows the binding as it stands, so it can change while
    /// the session runs: binding to a project session, or being handed to one, gives it.
    pub fn has_bound_role(&self) -> bool {
        self.bound_to.is_some() && !self.lead
    }
}

/// One page a console session pushed to its report panel. It belongs to that console session, not
/// to its console. Every page is kept, so the panel can be paged back through; `anchor_message_id`
/// records the conversation position the page was pushed at and is stored only (see "Data model"
/// in `docs/architecture.md`), and no agent exposes a message id to put in it yet.
#[derive(Debug, Clone, Serialize)]
pub struct Page {
    pub id: String,
    pub console_session_id: String,
    pub html: String,
    pub anchor_message_id: Option<String>,
    pub created_at: i64,
}

/// One entry of a directory listing. Only directories are ever listed — a project is a directory —
/// so there is no "is this a directory" field to carry.
#[derive(Debug, Clone, Serialize)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub is_git_repo: bool,
}

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
}

/// The app-wide user settings the daemon stores, and the account list alongside them: a record
/// that already grows without a new request shape or a new event for every addition, which is
/// where the accounts belong too (see "Reusable capabilities" in `04-accounts-storage.md`).
#[derive(Debug, Clone, Default, Serialize)]
pub struct Settings {
    pub auto_pull_repositories: bool,
    /// The directory a `git` association clones into when none is named: the one the user set, or
    /// `~/Projects` (expanded) while none is. Always an absolute, lexically normalised path.
    pub default_clone_dir: String,
    /// Every account of every agent, application-wide. The default account of each agent is not
    /// among these — it is the state of pinning nothing, not a row (see `Account`).
    pub accounts: Vec<Account>,
    /// Launch override for Codex sessions that use the default account.
    pub codex_default_approval_reviewer: Option<ApprovalReviewer>,
}

/// Where a project's files are read from, as `project_source` reports it. See "Project sources" in
/// `apps/daemon/PROTOCOL.md`.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectSourceInfo {
    pub project: String,
    /// The project's path as stored.
    pub root: String,
    /// The directory that path resolves to, symbolic links followed, as a wire path.
    pub resolved_root: String,
    /// The resolved directory's identity; it changes when the directory is replaced.
    pub root_id: String,
    /// The repository holding the project's directory; `null` when it is in none.
    pub git: Option<GitSourceInfo>,
    /// Why the repository could not be read, verbatim, when its directory looks like one but `git`
    /// failed on it; `null` otherwise.
    pub git_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitSourceInfo {
    /// The repository's identity: that of its common git directory.
    pub repository: String,
    pub common_dir: String,
    /// The worktree holding the project's directory.
    pub worktree: String,
    /// The project's directory relative to that worktree's root, as a wire path; empty at the root.
    pub scope: String,
    pub worktrees: Vec<WorktreeInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorktreeInfo {
    pub id: String,
    pub root: String,
    pub main: bool,
    /// The commit checked out; `null` in a repository with no commit yet.
    pub head: Option<String>,
    /// The branch checked out, as a wire path; `null` when `HEAD` is detached or unborn.
    pub branch: Option<String>,
    /// Whether the project's scope exists as a directory in this worktree.
    pub scope_present: bool,
}

/// One entry of a project directory listing.
#[derive(Debug, Clone, Serialize)]
pub struct BrowseEntry {
    /// The entry's name, as a wire path.
    pub name: String,
    pub kind: EntryKind,
    /// A regular file's size in bytes; `null` for anything else.
    pub size: Option<u64>,
    /// A regular file's version, equal to the `version` a live read of it would report while it
    /// stays unchanged; `null` for anything else.
    pub version: Option<String>,
    /// What a symbolic link resolves to; `null` for anything else.
    pub target: Option<LinkTarget>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
    /// A FIFO, socket or device: listed, never read.
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkTarget {
    File,
    Directory,
    Other,
    /// The link resolves to nothing.
    Missing,
    /// The link resolves outside the scope, and is never followed.
    Outside,
}

/// Which content a `read_project_file` reads. Absent means `live`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReadFrom {
    #[default]
    Live,
    Index,
    /// A local branch's tip; `branch` is the name below `refs/heads/`, as a wire path.
    Branch {
        branch: String,
    },
    /// A commit, by its full object id.
    Commit {
        commit: String,
    },
}

/// What a file body was actually read from, so a reply identifies the content it carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContentSource {
    /// The file on disk; `version` changes whenever its content may have.
    /// `root_id` is the identity of the directory the read was scoped to.
    Live { root_id: String, version: String },
    /// The blob staged in a worktree's index.
    Index { worktree: String, blob: String },
    /// The blob in a commit; `branch` is the branch it was resolved from, when it was.
    Commit {
        commit: String,
        #[serde(default)]
        branch: Option<String>,
        blob: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentKind {
    /// Valid UTF-8 with no NUL byte; carried in `text`.
    Text,
    /// Anything else; carried base64-encoded in `data`.
    Binary,
}

/// A file body. Exactly one of `text` and `data` is set, by `kind`.
#[derive(Debug, Clone, Serialize)]
pub struct FileContent {
    pub size: u64,
    pub kind: ContentKind,
    /// The image type the bytes start like (`image/png`, …), a hint only; `null` when none.
    pub media_type: Option<&'static str>,
    pub text: Option<String>,
    pub data: Option<String>,
}

/// Which of a worktree's uncommitted changes an entry is: `HEAD` against the index (`staged`), the
/// index against the files on disk (`unstaged`), or a file on disk Git does not track and does not
/// ignore (`untracked`). Part of a change's identity, so one path can be a change in two groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeGroup {
    Staged,
    Unstaged,
    Untracked,
}

/// What one side of a change is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SideKind {
    File,
    Symlink,
    Submodule,
}

/// One side of a change in a change listing. See "Changes and comparisons" in
/// `apps/daemon/PROTOCOL.md`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ChangeSide {
    /// `path` is a wire path relative to the project's directory; `source` the version listed.
    Present {
        path: String,
        kind: SideKind,
        source: ContentSource,
    },
    /// No such side: the old side of an addition, the new side of a deletion.
    Absent,
    /// A side outside the project's scope; `repository_path` is a wire path relative to the
    /// repository's root, for display only.
    OutOfScope { repository_path: String },
}

/// How a path is in conflict, as `git status` tells the stages apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    BothModified,
    BothAdded,
    BothDeleted,
    AddedByUs,
    AddedByThem,
    DeletedByUs,
    DeletedByThem,
}

/// One entry of a change listing: a change between two sides, or a path in conflict, which is
/// not a two-sided change at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "group", rename_all = "snake_case")]
pub enum ChangeEntry {
    Staged {
        old: ChangeSide,
        new: ChangeSide,
    },
    Unstaged {
        old: ChangeSide,
        new: ChangeSide,
    },
    Untracked {
        old: ChangeSide,
        new: ChangeSide,
    },
    /// `path` is a wire path relative to the project's directory.
    Conflicted {
        path: String,
        conflict: ConflictKind,
    },
    /// A change between two branch tips: never part of a worktree's listing, only of a
    /// comparison's, where the old side is the left tip's and the new side the right tip's.
    Committed {
        old: ChangeSide,
        new: ChangeSide,
    },
}

/// How a `read_project_change` or a `read_project_comparison_change` names one side of the
/// change to read.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SideRef {
    /// `path` is a wire path relative to the project's directory.
    Present {
        path: String,
    },
    Absent,
    OutOfScope,
}

/// The change a `read_project_change` reads: its group and its two sides, as the listing gave them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ChangeRef {
    pub group: ChangeGroup,
    pub old: SideRef,
    pub new: SideRef,
}

/// One local branch of a project's repository, as `list_project_branches` lists it: its name below
/// `refs/heads/`, as a wire path, and the object id its reference named when it was listed — the
/// commit at its tip, unless the branch is broken (a missing object, or not a commit).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BranchInfo {
    pub name: String,
    pub commit: String,
    /// The session that created the branch, when the daemon can name one; absent otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<BranchOriginInfo>,
}

/// How the daemon came to attribute a branch to a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchOriginSource {
    /// The session's own git reported creating the branch, through the `reference-transaction`
    /// hook Octoboard points it at.
    RefHook,
}

/// The session a branch is attributed to having created, with the title that session had when the
/// attribution was made — the record outlives the session, which a project's deletion takes with
/// it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BranchOriginInfo {
    /// `ref_hook`: a branch with no session to name carries no origin at all.
    pub source: BranchOriginSource,
    /// The creating session's id. It may name a session that no longer exists.
    pub session: String,
    pub title: String,
    /// When the branch was created (milliseconds since the epoch): its reflog's first entry where
    /// it has one, else when the creation was reported.
    pub created_at: i64,
}

/// One end of a branch comparison: the branch, as a wire path, and the full id of the commit it was
/// resolved to. A comparison reply reports both; a read of one of its changes names both back, with
/// the comparison's `base`, and the commits alone decide what is read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComparisonEndpoint {
    pub branch: String,
    pub commit: String,
}

/// The change a `read_project_comparison_change` reads: its two sides, as the comparison gave them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ComparedChangeRef {
    pub old: SideRef,
    pub new: SideRef,
}

/// The version of one side of a change as a diff reply reported it, which a body request names back
/// so the bodies are read only from the sides the patch was made from.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SideVersion {
    Present { source: ContentSource },
    Absent,
}

/// One side of a change as `read_project_change` or `read_project_comparison_change` read it.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SideRead {
    /// `file` is the side's body, when the reply carries it: only a file side's, and only when no
    /// patch shows the change's content (a binary change, a side alone).
    ///
    /// `lines` is the number of lines the side's stored content holds (its newlines, and one more
    /// when the last line is unterminated), counted of the very blob the patch was made from;
    /// `None` where the daemon read no content for the side.
    Present {
        path: String,
        kind: SideKind,
        source: ContentSource,
        file: Option<FileContent>,
        lines: Option<u64>,
    },
    Absent,
    OutOfScope,
}

/// One browse request frame: the envelope's id, the optional slot a newer request supersedes an
/// older one in, and the request itself. Kept apart from [`Request`] because these are served
/// under their own budgets and on their own outbound lane — see "Browsing a project" in
/// `apps/daemon/PROTOCOL.md`.
#[derive(Debug, Deserialize)]
pub struct BrowseRequest {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub slot: Option<String>,
    #[serde(flatten)]
    pub body: BrowseBody,
}

/// The `type`s that are browse requests rather than [`RequestBody`] ones.
pub const BROWSE_REQUEST_TYPES: &[&str] = &[
    "get_project_source",
    "list_project_dir",
    "search_project_files",
    "read_project_file",
    "list_project_changes",
    "read_project_change",
    "list_project_branches",
    "compare_project_branches",
    "read_project_comparison_change",
    "read_project_change_bodies",
    "read_project_comparison_change_bodies",
];

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrowseBody {
    GetProjectSource {
        project: String,
    },
    ListProjectDir {
        project: String,
        #[serde(default)]
        worktree: Option<String>,
        path: String,
    },
    SearchProjectFiles {
        project: String,
        query: String,
    },
    ReadProjectFile {
        project: String,
        #[serde(default)]
        worktree: Option<String>,
        path: String,
        #[serde(default)]
        from: ReadFrom,
    },
    ListProjectChanges {
        project: String,
        #[serde(default)]
        worktree: Option<String>,
    },
    ReadProjectChange {
        project: String,
        #[serde(default)]
        worktree: Option<String>,
        change: ChangeRef,
    },
    ListProjectBranches {
        project: String,
    },
    /// `left` and `right` are branch names below `refs/heads/`, as wire paths; the changes are
    /// those the right branch has made since it forked from the left one.
    CompareProjectBranches {
        project: String,
        left: String,
        right: String,
    },
    ReadProjectComparisonChange {
        project: String,
        left: ComparisonEndpoint,
        right: ComparisonEndpoint,
        /// The commit the comparison's changes start from, as its reply gave it.
        base: String,
        change: ComparedChangeRef,
    },
    /// The full bodies of the two sides of a change `read_project_change` made a patch of; `old`
    /// and `new` are the versions that reply reported for its sides.
    ReadProjectChangeBodies {
        project: String,
        #[serde(default)]
        worktree: Option<String>,
        change: ChangeRef,
        old: SideVersion,
        new: SideVersion,
    },
    /// As `ReadProjectChangeBodies`, for a change of a comparison.
    ReadProjectComparisonChangeBodies {
        project: String,
        left: ComparisonEndpoint,
        right: ComparisonEndpoint,
        base: String,
        change: ComparedChangeRef,
        old: SideVersion,
        new: SideVersion,
    },
}

/// One control-socket frame from the client: an optional request id the daemon echoes back, plus
/// the request itself. The id lets the UI pair a reply with the request that caused it; state
/// changes are broadcast to every client instead and carry no id.
///
/// `id` belongs to the envelope alone. Inside a request, what it acts on is named for its kind —
/// `console`, `project`, `session` — because one flattened object carries both: a request field
/// named `id` would collide with the envelope's and silently lose one of the two, leaving the
/// daemon acting on the request id as if it were a record id.
#[derive(Debug, Deserialize)]
pub struct Request {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(flatten)]
    pub body: RequestBody,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RequestBody {
    CreateConsole {
        name: String,
        default_agent: Agent,
        /// Each agent's account, by id; absent means that agent's default account. A reference
        /// that names no account of that agent is refused with `unknown_account`.
        #[serde(default)]
        claude_account_id: Option<String>,
        #[serde(default)]
        codex_account_id: Option<String>,
        #[serde(default)]
        grok_account_id: Option<String>,
        /// Each agent's model and reasoning effort for the console's console sessions; absent or
        /// blank means the agent's own default.
        #[serde(default)]
        claude_console_session_model: Option<String>,
        #[serde(default)]
        claude_console_session_effort: Option<String>,
        #[serde(default)]
        codex_console_session_model: Option<String>,
        #[serde(default)]
        codex_console_session_effort: Option<String>,
        #[serde(default)]
        grok_console_session_model: Option<String>,
        #[serde(default)]
        grok_console_session_effort: Option<String>,
        #[serde(default)]
        icon: Option<String>,
        /// Absent or blank means no addition to the console session's instructions.
        #[serde(default)]
        console_session_prompt: Option<String>,
    },
    UpdateConsole {
        console: String,
        name: Option<String>,
        default_agent: Option<Agent>,
        /// Each agent's account: absent leaves the setting alone; an explicit `null` clears it
        /// back to that agent's default account. A reference that names no account of that agent
        /// is refused with `unknown_account`.
        #[serde(default, deserialize_with = "present_option")]
        claude_account_id: Option<Option<String>>,
        #[serde(default, deserialize_with = "present_option")]
        codex_account_id: Option<Option<String>>,
        #[serde(default, deserialize_with = "present_option")]
        grok_account_id: Option<Option<String>>,
        /// Each agent's console session model and effort: absent leaves the setting alone; an
        /// explicit `null` (or a blank string) clears it back to the agent's own default.
        #[serde(default, deserialize_with = "present_option")]
        claude_console_session_model: Option<Option<String>>,
        #[serde(default, deserialize_with = "present_option")]
        claude_console_session_effort: Option<Option<String>>,
        #[serde(default, deserialize_with = "present_option")]
        codex_console_session_model: Option<Option<String>>,
        #[serde(default, deserialize_with = "present_option")]
        codex_console_session_effort: Option<Option<String>>,
        #[serde(default, deserialize_with = "present_option")]
        grok_console_session_model: Option<Option<String>>,
        #[serde(default, deserialize_with = "present_option")]
        grok_console_session_effort: Option<Option<String>>,
        /// Absent leaves the avatar alone; an explicit `null` (or a blank string) clears it back
        /// to the default glyph.
        #[serde(default, deserialize_with = "present_option")]
        icon: Option<Option<String>>,
        /// Absent leaves the addition alone; an explicit `null` (or a blank string) clears it.
        #[serde(default, deserialize_with = "present_option")]
        console_session_prompt: Option<Option<String>>,
    },
    DeleteConsole {
        console: String,
    },
    AddProject {
        console_id: String,
        source: ProjectSource,
        path: Option<String>,
        remote_url: Option<String>,
        name: Option<String>,
        default_agent: Option<Agent>,
        /// Each agent's account as a [`ProjectAccount`]; absent or `null` means inheriting the
        /// console's reference for that agent. An account that names none of that agent's is
        /// refused with `unknown_account`.
        #[serde(default)]
        claude_account_id: Option<ProjectAccount>,
        #[serde(default)]
        codex_account_id: Option<ProjectAccount>,
        #[serde(default)]
        grok_account_id: Option<ProjectAccount>,
        /// Each agent's model and reasoning effort for the project's sessions; absent or blank
        /// means the agent's own default.
        #[serde(default)]
        claude_session_model: Option<String>,
        #[serde(default)]
        claude_session_effort: Option<String>,
        #[serde(default)]
        codex_session_model: Option<String>,
        #[serde(default)]
        codex_session_effort: Option<String>,
        #[serde(default)]
        grok_session_model: Option<String>,
        #[serde(default)]
        grok_session_effort: Option<String>,
        /// Whether an absent `default_agent` is filled in from the directory's marker files (see
        /// `hostfs::detect_agent`). Absent is true; a client that has already shown the user the
        /// detection, and so sends what it showed, sends false to have "inherit from console"
        /// kept as chosen.
        #[serde(default = "detect_by_default")]
        detect_default_agent: bool,
        /// Absent means no tags.
        tags: Option<Vec<String>>,
    },
    UpdateProject {
        project: String,
        name: Option<String>,
        /// Absent leaves the project's default agent alone; an explicit `null` clears it, which is
        /// how the project goes back to inheriting the console's default. The attribute is what
        /// keeps those two apart: serde otherwise reads a present `null` into the outer `Option` as
        /// `None`, exactly as if the field had not been sent.
        #[serde(default, deserialize_with = "present_option")]
        default_agent: Option<Option<Agent>>,
        /// Each agent's account as a [`ProjectAccount`]: absent leaves the setting alone; an
        /// explicit `null` puts it back to inheriting the console's reference. An account that
        /// names none of that agent's is refused with `unknown_account`.
        #[serde(default, deserialize_with = "present_option")]
        claude_account_id: Option<Option<ProjectAccount>>,
        #[serde(default, deserialize_with = "present_option")]
        codex_account_id: Option<Option<ProjectAccount>>,
        #[serde(default, deserialize_with = "present_option")]
        grok_account_id: Option<Option<ProjectAccount>>,
        /// Each agent's model and effort for the project's sessions: absent leaves the setting
        /// alone; an explicit `null` (or a blank string) clears it back to the agent's own default.
        #[serde(default, deserialize_with = "present_option")]
        claude_session_model: Option<Option<String>>,
        #[serde(default, deserialize_with = "present_option")]
        claude_session_effort: Option<Option<String>>,
        #[serde(default, deserialize_with = "present_option")]
        codex_session_model: Option<Option<String>>,
        #[serde(default, deserialize_with = "present_option")]
        codex_session_effort: Option<Option<String>>,
        #[serde(default, deserialize_with = "present_option")]
        grok_session_model: Option<Option<String>>,
        #[serde(default, deserialize_with = "present_option")]
        grok_session_effort: Option<Option<String>>,
        /// Absent leaves the project's pin alone.
        pinned: Option<bool>,
        /// Absent leaves the project's tags alone; a present array replaces them wholesale.
        tags: Option<Vec<String>>,
    },
    DeleteProject {
        project: String,
        /// Ends the project's running sessions first, as `archive_session` does, instead of
        /// refusing while there are any. Absent is false.
        #[serde(default)]
        stop_sessions: bool,
        /// Moves the project's directory to the system Trash once its sessions are stopped,
        /// and removes the project only if that worked. Absent is false.
        #[serde(default)]
        trash_directory: bool,
    },
    ListDir {
        path: String,
    },
    /// Which agent the directory at `path` is set up for, answered with `agent_detected`, or
    /// refused as `add_project` refuses a `local` path that is not an existing directory.
    DetectDirectoryAgent {
        path: String,
    },
    /// Whether the git remote `remote_url` can be read with the user's credentials, and which
    /// agent its top level is set up for, answered with `agent_detected` or, when it cannot be
    /// read, `git_remote_unreachable`.
    ProbeGitRemote {
        remote_url: String,
    },
    /// The models `agent` offers right now, answered with `agent_models`. `account` names the
    /// account whose catalogue to read, by id; absent is the default account. An account that
    /// does not exist or belongs to another agent is refused with `unknown_account`; anything
    /// that goes wrong while asking the agent is not a refusal but an empty answer with a reason.
    ListAgentModels {
        agent: Agent,
        #[serde(default)]
        account: Option<String>,
    },
    /// The configuration document of the console `console`, or, when it is absent, of every
    /// console and the application settings, answered with `configuration_exported`. A console
    /// that does not exist is refused with `unknown_console`. Reads what is stored; touches no
    /// session, runs no `git`.
    ExportConfiguration {
        #[serde(default)]
        console: Option<String>,
    },
    /// Reads a configuration document and answers with `configuration_inspected`: for each project
    /// which directories it could be placed in on this machine, and for each account reference
    /// whether it falls back to a default account. Changes nothing. `document` is the JSON a
    /// client read from a file and is checked here, not trusted: a `format` that is not a
    /// configuration document, a `version` newer than this daemon reads, or a body that cannot be
    /// read is refused with `configuration_not_recognised`, `configuration_version_unsupported`
    /// or `configuration_unreadable`.
    InspectConfiguration {
        document: serde_json::Value,
    },
    /// Applies a configuration document and answers with `configuration_imported`: creates a new
    /// console for each console in it, and associates each project with the directory its
    /// `resolutions` entry chooses. A project with no entry is left out. The document is refused
    /// as `inspect_configuration` refuses it, and a resolution naming a console or project the
    /// document does not have, naming one twice, or choosing a `directory` that is neither an
    /// absolute path nor starts with `~/`, is refused with `import_resolution_invalid`; all of
    /// that before anything is created.
    ImportConfiguration {
        document: serde_json::Value,
        #[serde(default)]
        resolutions: Vec<crate::config_import::ProjectResolution>,
        /// Whether to apply the document's application settings to this machine; absent means
        /// false. Asking for them of a document that carries none is refused with
        /// `import_settings_missing`.
        #[serde(default)]
        apply_settings: bool,
    },
    /// Previews the current console name without allocating a session ordinal.
    PreviewConsoleSessionTitle {
        console: String,
    },
    OpenSession {
        console_id: String,
        project_id: Option<String>,
        /// The session's agent; absent takes the project's default, else the console's.
        agent: Option<Agent>,
        /// The account this session's agent reads, by id. Absent leaves the choice to the
        /// project's reference for that agent, else the console's, else the default account; an
        /// explicit `null` chooses the default account outright, whatever the project or console
        /// refers to. A reference that names no account of the session's agent is refused with
        /// `unknown_account`.
        #[serde(default, deserialize_with = "present_option")]
        account: Option<Option<String>>,
        /// Absent inherits the project's (or console's) current setting; null or blank uses the
        /// agent's own default. A string is trimmed and kept for every later launch.
        #[serde(default, deserialize_with = "present_option")]
        model: Option<Option<String>>,
        #[serde(default, deserialize_with = "present_option")]
        effort: Option<Option<String>>,
        task: Option<String>,
        title: Option<String>,
        /// The session this (project) session should report to. Absent means none: a session the
        /// user opens by hand stays outside the orchestration unless they choose one. Ignored for
        /// the console session itself, which is never bound. Must name a console session of
        /// `console_id`, or an unbound project session of `project_id`, or the request is refused
        /// with `unknown_session`.
        #[serde(default)]
        bound_to: Option<String>,
        /// The agent's own id of a conversation started outside Octoboard, to continue in this
        /// session. Trimmed, blank meaning absent. Only a project session takes one, never
        /// together with `bound_to`; it must be a UUID (kept lowercase and hyphenated), found in
        /// the session's account directory and recorded by no session already. Each refusal has a
        /// code of its own (see `apps/daemon/PROTOCOL.md`'s "Coded messages" section) and leaves
        /// no session behind.
        #[serde(default)]
        resume_agent_session_id: Option<String>,
    },
    ResumeSession {
        session: String,
    },
    ArchiveSession {
        session: String,
    },
    /// Moves a session to another account of its own agent: ends its process, copies its
    /// conversation record into the target account's directory, records the account on the
    /// session and relaunches it as a resume does. `account` is required: an id, or an explicit
    /// `null` for the default account. See `coordinator::switch_session_account`.
    SwitchSessionAccount {
        session: String,
        #[serde(default, deserialize_with = "present_option")]
        account: Option<Option<String>>,
    },
    /// Fresh native-record presence for exactly the supplied sessions, aggregated across accounts.
    InspectSessionRecords {
        sessions: Vec<String>,
    },
    /// Removes Octoboard's record of one session, archived or never used. A session that is not
    /// archived is deleted only while it has had no turn and nothing is bound to it or from it, and
    /// its process is stopped. Unused sessions' agent records are moved to the Trash automatically;
    /// `trash_agent_records` also moves the records of used sessions.
    DeleteSession {
        session: String,
        /// Requires the atomic unused-session delete, including when the session is archived.
        /// Never falls back to an archived-session cascade.
        #[serde(default)]
        only_if_unused: bool,
        #[serde(default)]
        trash_agent_records: bool,
    },
    /// Removes every archived session of `project`, or every archived session under
    /// `console_session` (bound to it, or to an archived lead session bound to it), or, with
    /// neither, every archived console session of `console`. Naming both is refused as
    /// unreadable.
    DeleteArchivedSessions {
        console: String,
        project: Option<String>,
        #[serde(default)]
        console_session: Option<String>,
        /// Moves native records to the Trash for every session deleted; defaults to false.
        #[serde(default)]
        trash_agent_records: bool,
    },
    SetSessionPinned {
        session: String,
        pinned: bool,
    },
    /// Refused for a project session, which carries no colour.
    SetSessionColour {
        session: String,
        colour: ConsoleSessionColour,
    },
    SendMessage {
        session: String,
        text: String,
    },
    RenameSession {
        session: String,
        title: String,
    },
    ListPages {
        console_session: String,
    },
    /// What a report panel form was submitted with. The page it came from is named rather than the
    /// console session, because that is what the panel knows and it is also what decides whether the
    /// submission is allowed at all: only its console session's newest page is live. The page also
    /// names the console session the submission is delivered to.
    SubmitPage {
        page: String,
        data: serde_json::Value,
    },
    /// The user's answer to a `trust_prompt`: Octoboard may press that session's trust
    /// confirmation. `remember` also records the project's consent, so a later confirmation of any
    /// agent in the project is pressed without asking; `trust_parent_dir` records the project's
    /// parent directory as trusted instead, so every project under it is, and when set `remember`
    /// adds nothing. The directory is the daemon's to derive from the session's project, never the
    /// client's to name. Either is recorded only once the confirmation has been pressed.
    ConfirmTrust {
        session: String,
        remember: bool,
        #[serde(default)]
        trust_parent_dir: bool,
    },
    /// Stops trusting a directory. Projects' own consents are left as they are.
    RemoveTrustedDirectory {
        path: String,
    },
    /// The user's answer to a `session_request`. `approve` starts the session the request
    /// describes, unbound; anything else, dismissing the dialog included, refuses it and changes
    /// nothing. The first answer wins: one for a request already answered, timed out or withdrawn
    /// changes nothing and is refused. An approval is answered with `session_opened` for the
    /// session it started, a refusal with an ack.
    AnswerSessionRequest {
        request_id: String,
        approve: bool,
    },
    /// Each settable field absent means "leave it alone". Broadcasts `settings_updated` only when
    /// something actually changed, as `remove_trusted_directory` does.
    UpdateSettings {
        auto_pull_repositories: Option<bool>,
        /// A blank value goes back to the built-in default.
        default_clone_dir: Option<String>,
    },
    /// Both fields are required: an account always has a name and a directory. Broadcasts
    /// `settings_updated`.
    CreateAccount {
        agent: Agent,
        name: String,
        config_dir: String,
    },
    /// Renames the account, repoints it, or both, independently; either field absent leaves it
    /// alone. Broadcasts `settings_updated`.
    UpdateAccount {
        account: String,
        name: Option<String>,
        config_dir: Option<String>,
    },
    /// Clears the reference of every console that refers to this account, which puts each one back
    /// on its agent's default account, then removes the account. A session holding it is left
    /// alone — it already carries its own copy of the directory it launches with. Broadcasts
    /// `settings_updated`, and a `console_upserted` for every console whose reference was cleared.
    DeleteAccount {
        account: String,
    },
    /// Sets a Codex account's approval reviewer. A null account names the default account; a null
    /// reviewer follows its configuration. Takes effect on the next launch or resume.
    SetAccountApprovalReviewer {
        account: Option<String>,
        reviewer: Option<ApprovalReviewer>,
    },
    /// Checks every project of this console's current git status against its remote, concurrently.
    /// Answered with `ack` at once; the statuses follow as `project_git_status` broadcasts, one per
    /// project as its check finishes. A project already being checked — by an earlier call this
    /// client raced ahead of, or one from another client — is not checked again.
    RefreshGitStatus {
        console: String,
    },
    /// Checks one project against its remote and fast-forwards it right now, the same work
    /// `refresh_git_status` does for a console, but ignoring the one-minute floor and whether
    /// `auto_pull_repositories` is on. Answered with `ack` at once; the status follows as
    /// `project_git_status` broadcasts. A project already being checked gets one more forced check
    /// once that check ends.
    PullProjectBranch {
        project: String,
    },
    /// Opens the system terminal at the project's directory, on the daemon's host. Answered with
    /// `ack` once the terminal has been asked to open, not when its window closes. Refused with
    /// `terminal_unavailable` when the directory is missing, the operating system has none, or the
    /// launch fails.
    OpenProjectTerminal {
        project: String,
    },
    /// Opens the project's directory in the system file manager on the daemon's host. Answered
    /// with `ack` once `open` succeeds on macOS, or once `xdg-open` is spawned on Linux (later
    /// failures are not observed). Refused with `directory_unavailable` if validation or launch fails.
    OpenProjectDirectory {
        project: String,
    },
    /// Reveals an existing live path in the daemon host's file manager, selecting it.
    /// Uses the same canonical wire path and optional worktree identity as live browsing.
    RevealProjectPath {
        project: String,
        path: String,
        #[serde(default)]
        worktree: Option<String>,
    },
    /// Opens an existing live path with its default application on the daemon host.
    OpenProjectPath {
        project: String,
        path: String,
        #[serde(default)]
        worktree: Option<String>,
    },
    /// Pushes the project's current branch, and only that branch, never with `--force`. Answered
    /// with `ack` once `git push` has exited; a success is followed by `project_git_status` from a
    /// local re-read, with no fetch. Refused with `not_a_git_repository`, `not_on_a_branch`,
    /// `no_commits_to_push`, `no_push_remote`, `upstream_not_a_remote` or `git_push_failed`.
    PushProjectBranch {
        project: String,
    },
    /// Switches to an existing local branch in canonical wire form. A success publishes a local
    /// status re-read, without fetching or fast-forwarding. Answered after Git exits.
    SwitchProjectBranch {
        project: String,
        branch: String,
    },
    /// Discards this project's staged and unstaged tracked changes against HEAD, including
    /// staged additions. Untracked files, ignored files and submodules are preserved. Requires
    /// client confirmation. Answered with `ack` after Git exits; commits and branch stay put.
    RestoreProjectChanges {
        project: String,
    },
    Shutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionRecordPresence {
    Present,
    Absent,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    SessionRecordsInspected {
        id: Option<String>,
        presence: SessionRecordPresence,
    },
    Snapshot {
        hosts: Vec<Host>,
        consoles: Vec<Console>,
        projects: Vec<Project>,
        sessions: Vec<Session>,
        trusted_directories: Vec<String>,
        settings: Settings,
        /// Every git status the daemon currently holds; empty on a fresh start. Carried here, like
        /// the trusted directories, so a reconnecting client never has to ask for it separately.
        git_statuses: Vec<GitStatus>,
        /// One entry per supported agent, always three, each starting `not_determined` and
        /// replaced once the daemon's one-time login-shell snapshot lands; see
        /// `crate::availability`.
        agent_availability: Vec<AgentAvailability>,
        /// The daemon host's home directory, so a client can show a path under it as `~/...`; it
        /// is the daemon's home, not the browser's, since the two may be on different machines.
        /// Null when the daemon cannot determine one.
        home_dir: Option<String>,
    },
    /// Availability or a default account's resolved directory changed for one or more agents —
    /// the whole three-entry list, like every other upsert. Broadcast once, when the daemon's
    /// one-time determination lands; never again afterwards, since nothing re-determines it
    /// during a run.
    AgentAvailabilityUpdated {
        agent_availability: Vec<AgentAvailability>,
    },
    /// The directories whose projects Octoboard presses every agent's trust confirmation for
    /// changed: the whole list, like every other upsert.
    TrustedDirectoriesUpdated {
        trusted_directories: Vec<String>,
    },
    /// The user settings changed, whole: the single setting today, and whatever is added to
    /// `Settings` later.
    SettingsUpdated {
        settings: Settings,
    },
    ConsoleUpserted {
        console: Console,
    },
    ConsoleDeleted {
        console: String,
    },
    ProjectUpserted {
        project: Project,
    },
    ProjectDeleted {
        project: String,
    },
    SessionUpserted {
        session: Session,
    },
    SessionDeleted {
        session: String,
    },
    /// A project's `GitStatus` changed, including every transition of `activity` — the animated
    /// icon has something to follow. Nothing is sent when a project is removed; the client drops
    /// its status along with it.
    ProjectGitStatus {
        status: GitStatus,
    },
    /// Something about a session the user has to be told, which no status field carries: an
    /// injected capability that will not apply, a setting of theirs Octoboard had to work around.
    /// Broadcast once, when it is found; nothing stores it. `code` and `params` name the notice for
    /// a client to word in its own language; `message` is the English text for a client that does
    /// not know the code.
    SessionNotice {
        session: String,
        code: String,
        params: Params,
        message: String,
    },
    /// A session of a project the user has not yet agreed Octoboard may press trust confirmations
    /// for — not by its own consent and not through a trusted directory — is sitting at its
    /// agent's folder-trust confirmation; `agent` is the one asking. Broadcast once per
    /// confirmation, and sent again to a client after each `snapshot` while it waits. `trust_dir`
    /// is the directory `confirm_trust` with `trust_parent_dir` would trust, and null when there
    /// is none to offer: it would be the filesystem root, the home directory or one containing it,
    /// the home directory cannot be determined, or the project's path is not absolute.
    TrustPrompt {
        session: String,
        agent: Agent,
        project: String,
        path: String,
        trust_dir: Option<String>,
    },
    /// A session asked with `request_session` for a session that has no owner, and its call is
    /// waiting for the user's answer. Broadcast once, when the call is made, and sent again to a
    /// client after each `snapshot`, oldest first, for every request still waiting.
    SessionRequest {
        request_id: String,
        /// The requesting session.
        session: String,
        /// Whether the session asked for would be a console session or a project session.
        role: Role,
        /// The console the new session would be in.
        console: String,
        /// The project the new session would be in; null for a console session.
        project: Option<String>,
        /// The agent the new session would run.
        agent: Agent,
        /// Milliseconds since the epoch, which is what orders several requests oldest first.
        requested_at: i64,
    },
    /// A `session_request` is no longer waiting: every client closes its dialog.
    SessionRequestClosed {
        request_id: String,
        reason: SessionRequestEnding,
    },
    ConsoleSessionTitle {
        id: Option<String>,
        console: String,
        title: String,
    },
    /// The reply to `open_session`, and to an approving `answer_session_request`: the session that
    /// was opened. For `open_session` it is `starting`, the launch still going on; for an approving
    /// answer the launch has settled and it is running. The same record is broadcast as
    /// `session_upserted` as well, but a broadcast carries no request id, so this is the only way
    /// the client that asked can tell which of the sessions appearing in the tree is the one it
    /// just opened — and therefore the only way it can select it.
    SessionOpened {
        id: Option<String>,
        session: Session,
    },
    DirListing {
        id: Option<String>,
        path: String,
        entries: Vec<DirEntry>,
    },
    /// The reply to `detect_directory_agent` and `probe_git_remote`: the one agent the directory
    /// or remote is set up for, or `null` for none, several, or one known not to be installed.
    AgentDetected {
        id: Option<String>,
        agent: Option<Agent>,
    },
    /// The reply to `list_agent_models`: `agent` and `account` echo the request. `models` is
    /// empty exactly when `unavailable` says why; a client then offers a free-text field or the
    /// agent's own default rather than an error.
    AgentModels {
        id: Option<String>,
        agent: Agent,
        account: Option<String>,
        models: Vec<AgentModel>,
        unavailable: Option<ModelsUnavailable>,
    },
    /// The reply to `export_configuration`: the configuration document, which a client saves as
    /// the file the user keeps (see "Configuration export" in `apps/daemon/PROTOCOL.md`).
    ConfigurationExported {
        id: Option<String>,
        document: crate::config_export::ConfigurationDocument,
    },
    /// The reply to `inspect_configuration` (see "Configuration import" in
    /// `apps/daemon/PROTOCOL.md`): one entry per console of the document, in its order.
    ConfigurationInspected {
        id: Option<String>,
        settings: Option<crate::config_import::InspectedSettings>,
        consoles: Vec<crate::config_import::InspectedConsole>,
    },
    /// The reply to `import_configuration`: one entry per console of the document, in its order,
    /// each naming the console created for it.
    ConfigurationImported {
        id: Option<String>,
        settings_applied: Option<crate::config_export::DocumentSettings>,
        settings_error: Option<crate::config_import::ImportFailure>,
        consoles: Vec<crate::config_import::ImportedConsole>,
    },
    /// The reply to `list_pages`, oldest first. Pages are not in `snapshot`: a page carries a whole
    /// HTML document, and only the console session the user is looking at needs its pages, so the
    /// panel asks for them instead — and asks again after every `snapshot`, which is what keeps it
    /// correct across a `page_created` the client was too far behind to receive. A
    /// lagging client is sent a fresh snapshot in place of the events it missed, on the socket it
    /// already has, so nothing else tells it that its list is now short.
    PageList {
        id: Option<String>,
        console_session_id: String,
        pages: Vec<Page>,
    },
    /// A page the console session just pushed. The panel showing that console session refreshes to
    /// it.
    PageCreated {
        page: Page,
    },
    /// The reply to `get_project_source`.
    ProjectSource {
        id: Option<String>,
        source: ProjectSourceInfo,
    },
    /// The reply to `list_project_dir`. `complete` is false when the listing was cut at a budget.
    ProjectDir {
        id: Option<String>,
        project: String,
        worktree: Option<String>,
        path: String,
        /// The identity of the directory the listing was scoped to.
        root_id: String,
        entries: Vec<BrowseEntry>,
        complete: bool,
    },
    /// Recursive filename matches, possibly cut at a traversal or reply budget.
    ProjectFileMatches {
        id: Option<String>,
        project: String,
        query: String,
        paths: Vec<String>,
        complete: bool,
    },
    /// The reply to `read_project_file`; `project`, `worktree` and `path` echo the request.
    ProjectFile {
        id: Option<String>,
        project: String,
        worktree: Option<String>,
        path: String,
        source: ContentSource,
        file: FileContent,
    },
    /// The reply to `list_project_changes`. `head` is the commit the staged changes are against,
    /// `null` before the first commit; `complete` is false when the list was cut at a budget.
    ProjectChanges {
        id: Option<String>,
        project: String,
        worktree: Option<String>,
        head: Option<String>,
        changes: Vec<ChangeEntry>,
        complete: bool,
    },
    /// The reply to `read_project_change`. `head` is the commit a staged change was read against
    /// (`null` before the first commit, and for the other groups); `patch` is the change's unified
    /// patch as `git` writes it, `null` when the reply makes none.
    ProjectChange {
        id: Option<String>,
        project: String,
        worktree: Option<String>,
        group: ChangeGroup,
        head: Option<String>,
        // Boxed: two sides with their bodies would make every `Event` this large.
        old: Box<SideRead>,
        new: Box<SideRead>,
        patch: Option<FileContent>,
    },
    /// The reply to `list_project_branches`: the repository's local branches in byte order of
    /// their names; `complete` is false when the list was cut at a budget.
    ProjectBranches {
        id: Option<String>,
        project: String,
        branches: Vec<BranchInfo>,
        complete: bool,
    },
    /// The reply to `compare_project_branches`: the two branches with the commits they were
    /// resolved to, the commit they forked at (`base`, their merge base), and the changes from
    /// that commit to the right branch's that touch the project, each in the `committed` group;
    /// `complete` is false when the list was cut at a budget.
    ProjectComparison {
        id: Option<String>,
        project: String,
        left: ComparisonEndpoint,
        right: ComparisonEndpoint,
        base: String,
        changes: Vec<ChangeEntry>,
        complete: bool,
    },
    /// The reply to `read_project_comparison_change`: `left`, `right` and `base` echo the request,
    /// and both sides and the patch were read from exactly the base and the right commit.
    ProjectComparisonChange {
        id: Option<String>,
        project: String,
        left: ComparisonEndpoint,
        right: ComparisonEndpoint,
        base: String,
        old: Box<SideRead>,
        new: Box<SideRead>,
        patch: Option<FileContent>,
    },
    /// The reply to `read_project_change_bodies`: both sides with their whole bodies in `file`,
    /// each present or absent, never out of scope.
    ProjectChangeBodies {
        id: Option<String>,
        project: String,
        worktree: Option<String>,
        group: ChangeGroup,
        old: Box<SideRead>,
        new: Box<SideRead>,
    },
    /// The reply to `read_project_comparison_change_bodies`: `left`, `right` and `base` echo the
    /// request.
    ProjectComparisonChangeBodies {
        id: Option<String>,
        project: String,
        left: ComparisonEndpoint,
        right: ComparisonEndpoint,
        base: String,
        old: Box<SideRead>,
        new: Box<SideRead>,
    },
    Ack {
        id: Option<String>,
    },
    /// A failure, with a stable `code` and the `params` that fill it. A client words the failure from
    /// them in its own language and branches on the code where it has to act rather than just show;
    /// `message` is the English text, for a client that does not know the code.
    Error {
        id: Option<String>,
        code: String,
        params: Params,
        message: String,
    },
}

impl Event {
    /// The reply to a request the daemon could not read.
    pub fn unreadable_request(detail: &str) -> Self {
        Self::Error {
            id: None,
            code: error_code::UNREADABLE_REQUEST.to_string(),
            params: params(&[("detail", detail)]),
            message: format!("unreadable request: {detail}"),
        }
    }

    /// The reply to the failure `err`: its own code and params when it is a [`CodedError`], and
    /// otherwise [`error_code::INTERNAL_ERROR`] carrying the whole context chain.
    pub fn from_error(id: Option<String>, err: &anyhow::Error) -> Self {
        let (code, params, message) = error_parts(err);
        Self::Error {
            id,
            code,
            params,
            message,
        }
    }
}

/// One model an agent offers, as `list_agent_models` reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentModel {
    /// What Octoboard passes to the agent as its model: an alias Claude Code resolves itself, or
    /// the identifier the Codex or Grok Build catalogue lists.
    pub id: String,
    /// The name to show; the catalogue's own where it has one, else `id`.
    pub name: String,
    /// The reasoning-effort levels to offer for the model, in the agent's own spelling: the
    /// catalogue's own, in its order, where the model's entry gives them, else the agent's whole
    /// measured set; empty only for a model that takes none. Never unknown. A level the model
    /// does not advertise (the fallback is per agent, not per model) can still be refused by the
    /// agent, or quietly lowered, as Claude Code does with a level its model does not support, so
    /// a listed level is one to offer, not a promise that it is honoured.
    pub efforts: Vec<String>,
}

/// Why `list_agent_models` has no list, as a coded message of the same shape as an `error`. It
/// travels inside a reply because failing to ask the agent must never fail the form that asked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelsUnavailable {
    pub code: String,
    pub params: Params,
    pub message: String,
}

impl ModelsUnavailable {
    pub fn new(code: &'static str, message: impl Into<String>, pairs: &[(&str, &str)]) -> Self {
        Self {
            code: code.to_string(),
            params: params(pairs),
            message: message.into(),
        }
    }
}

/// Something a session's user has to be told, as a coded message. Carried from where it is found to
/// where it is broadcast as a `session_notice`.
#[derive(Debug, Clone)]
pub struct Notice {
    pub code: &'static str,
    pub message: String,
    pub params: Params,
}

impl Notice {
    pub fn new(code: &'static str, message: impl Into<String>, pairs: &[(&str, &str)]) -> Self {
        Self {
            code,
            message: message.into(),
            params: params(pairs),
        }
    }

    /// The `session_notice` event telling the user about this, for `session`.
    pub fn about(self, session: &str) -> Event {
        Event::SessionNotice {
            session: session.to_string(),
            code: self.code.to_string(),
            params: self.params,
            message: self.message,
        }
    }
}

/// Why a `session_request` stopped waiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionRequestEnding {
    Approved,
    /// Refused by the user, dismissing the dialog included.
    Refused,
    /// Nobody answered within the request's time limit.
    TimedOut,
    /// The requesting session stopped waiting: its process ended, its call's connection to the
    /// daemon was dropped, or its agent cancelled the call.
    Withdrawn,
}

/// Client-sent text frame on `/ws/term/:session`. Binary frames on that socket are raw PTY input
/// and never reach this type.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TermControl {
    Resize { cols: u16, rows: u16 },
}

fn detect_by_default() -> bool {
    true
}

/// A request field that is absent (`None`), cleared (`Some(None)`) or given.
pub type Present<T> = Option<Option<T>>;

/// Deserializes a present field — `null` included — as `Some`, leaving `None` to mean "the field
/// was not sent at all". Used for the fields where clearing a value and leaving it alone are
/// different requests.
fn present_option<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// An error that carries a code and its params, so a client can word it in its own language and
/// branch on it without matching prose. Travels inside `anyhow::Error` and is recovered by
/// downcasting where the reply is built.
#[derive(Debug)]
pub struct CodedError {
    pub code: &'static str,
    pub message: String,
    pub params: Params,
}

impl CodedError {
    /// Returns the `anyhow::Error` rather than the bare value: every caller wants it in that
    /// shape, and the one place that reads it back downcasts out of exactly this.
    pub fn raised(
        code: &'static str,
        message: impl Into<String>,
        pairs: &[(&str, &str)],
    ) -> anyhow::Error {
        anyhow::Error::new(Self {
            code,
            message: message.into(),
            params: params(pairs),
        })
    }

    /// The `error` reply to the request `id` that failed with this.
    pub fn reply(&self, id: Option<String>) -> Event {
        Event::Error {
            id,
            code: self.code.to_string(),
            params: self.params.clone(),
            message: self.message.clone(),
        }
    }

    pub fn unknown_console(id: &str) -> anyhow::Error {
        Self::raised(
            error_code::UNKNOWN_CONSOLE,
            format!("unknown console {id}"),
            &[("console", id)],
        )
    }

    pub fn unknown_project(id: &str) -> anyhow::Error {
        Self::raised(
            error_code::UNKNOWN_PROJECT,
            format!("unknown project {id}"),
            &[("project", id)],
        )
    }

    pub fn unknown_session(id: &str) -> anyhow::Error {
        Self::raised(
            error_code::UNKNOWN_SESSION,
            format!("unknown session {id}"),
            &[("session", id)],
        )
    }

    pub fn unknown_account(id: &str) -> anyhow::Error {
        Self::raised(
            error_code::UNKNOWN_ACCOUNT,
            format!("unknown account {id}"),
            &[("account", id)],
        )
    }
}

/// The `code`, `params` and `message` an `error` reply carries for `err`: a [`CodedError`]'s own,
/// or, for any other failure, [`error_code::INTERNAL_ERROR`] with the whole context chain as the
/// message and as the `detail` param.
pub fn error_parts(err: &anyhow::Error) -> (String, Params, String) {
    match err.downcast_ref::<CodedError>() {
        Some(coded) => (
            coded.code.to_string(),
            coded.params.clone(),
            coded.message.clone(),
        ),
        None => {
            let message = format!("{err:#}");
            (
                error_code::INTERNAL_ERROR.to_string(),
                params(&[("detail", &message)]),
                message,
            )
        }
    }
}

impl std::fmt::Display for CodedError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CodedError {}

/// Error codes carried by `Event::Error`; `PROTOCOL.md` lists each with its params. A code never
/// changes its meaning or its params, because a client words the failure from them. A new code also
/// needs a `daemon.<code>` message in the UI's catalogs (`packages/ui/src/i18n/messages/`): without
/// one nothing fails, and the UI just shows the English `message`.
pub mod error_code {
    // -- what a client acts on, rather than just shows ---------------------------------------

    /// A launch was asked for while one was already running or already starting for that session.
    /// The client's own double click is the ordinary cause, so it is shown as nothing at all.
    pub const SESSION_ALREADY_RUNNING: &str = "session_already_running";
    /// [`SESSION_ALREADY_RUNNING`], where the launch had begun but not yet registered.
    pub const SESSION_ALREADY_STARTING: &str = "session_already_starting";
    /// A directory to trust as a whole cannot be offered: it is the filesystem root, the user's home
    /// directory or one that contains it. Nothing was answered. The dialog the request came from
    /// stays open, because the user can still choose another way to answer, and so it does for the
    /// two codes below, which are the same refusal for other reasons.
    pub const TRUST_DIRECTORY_TOO_BROAD: &str = "trust_directory_too_broad";
    /// [`TRUST_DIRECTORY_TOO_BROAD`], where the project's path is not absolute.
    pub const TRUST_PATH_NOT_ABSOLUTE: &str = "trust_path_not_absolute";
    /// [`TRUST_DIRECTORY_TOO_BROAD`], where the home directory is unknown, so nothing can be
    /// checked against it.
    pub const TRUST_HOME_UNKNOWN: &str = "trust_home_unknown";
    /// A go-ahead for a trust confirmation that is no longer waiting for one: pressed already, by
    /// this or another client or in the terminal, gone with its session, or never shown. Nothing is
    /// wrong, so a client shows nothing.
    pub const TRUST_NOT_WAITING: &str = "trust_not_waiting";

    // -- for the user to read ----------------------------------------------------------------

    /// Octoboard could not press a trust confirmation it had accepted a go-ahead for, or meant to
    /// press without asking. `params` names the `agent`, and the `reason_code` param says why, one
    /// of [`trust_reason`].
    pub const TRUST_ANSWER_FAILED: &str = "trust_answer_failed";
    /// Grok Build's confirmation was accepted — pressed by Octoboard, or answered by the person in
    /// the terminal — but no trust entry for the folder reached the user's own store: Grok wrote
    /// none into the session's copy in time, or the user's store could not be updated. Nothing was
    /// written to the user's store and no permission was recorded. `params` names the `agent`, the
    /// user's store as `path`, the English `reason`, its `reason_code` (one of [`trust_reason`]'s
    /// carry codes) and, for some, a `detail`.
    pub const TRUST_NOT_CARRIED_OVER: &str = "trust_not_carried_over";
    /// A go-ahead for a console session's trust confirmation, which Octoboard presses without
    /// asking.
    pub const CONSOLE_SESSION_TRUST_NOT_ASKED: &str = "console_session_trust_not_asked";

    /// A request that is not valid JSON of a known shape.
    pub const UNREADABLE_REQUEST: &str = "unreadable_request";
    /// Anything that is not meant for the user to read in detail: an unexpected failure with no
    /// meaning of its own. The text is carried as the `detail` param.
    pub const INTERNAL_ERROR: &str = "internal_error";
    pub const UNKNOWN_CONSOLE: &str = "unknown_console";
    pub const UNKNOWN_PROJECT: &str = "unknown_project";
    pub const UNKNOWN_SESSION: &str = "unknown_session";
    pub const UNKNOWN_PAGE: &str = "unknown_page";
    /// A request names two fields that exclude each other. `params` names them as `first` and
    /// `second`, as on the wire.
    pub const CONFLICTING_FIELDS: &str = "conflicting_fields";
    /// A request lacks a field its kind needs.
    pub const FIELD_REQUIRED: &str = "field_required";
    /// A model or effort the agent does not offer. `params` names the `agent`, the `field`
    /// (`model` or `effort`) and the `value`.
    pub const INVALID_LAUNCH_SETTING: &str = "invalid_launch_setting";
    /// A repository URL that a clone is refused from: not one of the transports Octoboard
    /// clones over, or carrying credentials, which would be stored with the project. `params`
    /// names the `url` with any credentials removed.
    pub const INVALID_REMOTE_URL: &str = "invalid_remote_url";
    pub const CONSOLE_HAS_RUNNING_SESSIONS: &str = "console_has_running_sessions";
    pub const PROJECT_HAS_RUNNING_SESSIONS: &str = "project_has_running_sessions";
    /// Another project's directory is the one being moved to the Trash, lies inside it, or holds
    /// it, so trashing would take that project's files too. `params` names the other project as
    /// `project`.
    pub const PROJECT_DIRECTORY_SHARED: &str = "project_directory_shared";
    /// The project's directory is one the Trash is never offered for: the home directory, a
    /// root of a disk, or a directory holding Octoboard's own data. The project is kept.
    pub const PROJECT_DIRECTORY_PROTECTED: &str = "project_directory_protected";
    /// The project's directory could not be moved to the Trash. The project is kept. The reason
    /// is carried as the `detail` param.
    pub const PROJECT_DIRECTORY_TRASH_FAILED: &str = "project_directory_trash_failed";
    pub const PATH_NOT_ABSOLUTE: &str = "path_not_absolute";
    pub const PATH_NOT_FOUND: &str = "path_not_found";
    pub const PATH_NOT_A_DIRECTORY: &str = "path_not_a_directory";
    pub const PATH_ALREADY_EXISTS: &str = "path_already_exists";
    pub const DIRECTORY_UNREADABLE: &str = "directory_unreadable";
    /// A session's or a launch's working directory cannot be reached by the daemon.
    pub const DIRECTORY_UNREACHABLE: &str = "directory_unreachable";
    pub const NO_REPOSITORIES_FOUND: &str = "no_repositories_found";
    pub const ALL_PROJECTS_ALREADY_ADDED: &str = "all_projects_already_added";
    pub const REPOSITORY_NAME_MISSING: &str = "repository_name_missing";
    pub const GIT_CLONE_FAILED: &str = "git_clone_failed";
    pub const GIT_REMOTE_UNREACHABLE: &str = "git_remote_unreachable";
    /// Also used for an account's directory, which goes through the same normalisation minus the
    /// existence check below.
    pub const CONFIG_DIR_NOT_ABSOLUTE: &str = "config_dir_not_absolute";
    /// A session's pinned configuration directory has gone, which refuses the launch — narrowed to
    /// a session that has a conversation on the agent's side; see `crate::adapter::pinned_config_dir`.
    pub const CONFIG_DIR_UNREACHABLE: &str = "config_dir_unreachable";
    /// A Grok Build session's pinned source home exists but Grok has never been run against it, so
    /// it carries no login and no session history for the per-session home to link. `params` names
    /// `path`.
    pub const GROK_HOME_NOT_INITIALIZED: &str = "grok_home_not_initialized";
    /// The `format` of a configuration document is absent or names something else, so the file is
    /// not one.
    pub const CONFIGURATION_NOT_RECOGNISED: &str = "configuration_not_recognised";
    /// A configuration document is of a `version` this daemon does not read. `params` names the
    /// `version` and the newest it `supported`.
    pub const CONFIGURATION_VERSION_UNSUPPORTED: &str = "configuration_version_unsupported";
    /// A configuration document could not be read. `params` names the `detail`.
    pub const CONFIGURATION_UNREADABLE: &str = "configuration_unreadable";
    /// A resolution of `import_configuration` is not one the document can take. `params` names the
    /// `console` and `project` it gave, as indexes.
    pub const IMPORT_RESOLUTION_INVALID: &str = "import_resolution_invalid";
    /// `import_configuration` was asked to apply a document's application settings and the
    /// document carries none, as a single console's export does not.
    pub const IMPORT_SETTINGS_MISSING: &str = "import_settings_missing";
    /// A project of an import was left uncreated because the directory choice made for it cannot be
    /// used: the document gives it no usable path or remote URL for that choice. `params` names the
    /// `choice`.
    pub const IMPORT_CHOICE_UNAVAILABLE: &str = "import_choice_unavailable";
    pub const ICON_NOT_AN_IMAGE: &str = "icon_not_an_image";
    pub const ICON_TOO_LARGE: &str = "icon_too_large";
    pub const SESSION_NOT_RUNNING: &str = "session_not_running";
    /// Only an archived session, or one never used, can be deleted, and not while it is starting: a
    /// session that is not archived is refused once it has had a turn, is bound to an owner or has
    /// sessions bound to it.
    pub const SESSION_NOT_DELETABLE: &str = "session_not_deletable";
    /// A session cannot be archived while a session under it (bound to it, or to a lead session
    /// bound to it) has a process running, whatever its status. `params` names the `count` of
    /// those sessions and their `sessions`, by title, comma separated.
    pub const SESSION_HAS_RUNNING_SESSIONS: &str = "session_has_running_sessions";
    pub const SESSION_WAITING_FOR_USER: &str = "session_waiting_for_user";
    /// A message the user sent is refused because the session's agent is showing, or may still be
    /// about to show, its folder-trust confirmation, which the message's trailing Enter would
    /// answer. `params` names the `session`.
    pub const SESSION_TRUST_PENDING: &str = "session_trust_pending";
    pub const QUEUED_MESSAGES_LOST: &str = "queued_messages_lost";
    pub const PAGE_NOT_CURRENT: &str = "page_not_current";
    pub const BINARY_NOT_FOUND: &str = "binary_not_found";
    pub const SHELL_ENVIRONMENT_TIMEOUT: &str = "shell_environment_timeout";
    /// A session's resolved agent has been determined unavailable (its binary does not resolve on
    /// the login shell's `PATH`) — never raised while that determination is still pending.
    /// `params` names `agent`.
    pub const AGENT_NOT_AVAILABLE: &str = "agent_not_available";
    pub const UNKNOWN_ACCOUNT: &str = "unknown_account";
    /// A switch was asked to move a session to the account it is on already.
    pub const SESSION_ALREADY_ON_ACCOUNT: &str = "session_already_on_account";
    /// A switch was asked for an archived session. Reopening one is a resume's job, and a switch
    /// must not do it as a side effect.
    pub const SESSION_ARCHIVED: &str = "session_archived";
    /// A switch found no conversation record of the session in the account it is on. `params`
    /// names `agent` and the `path` of that account's directory.
    pub const CONVERSATION_NOT_FOUND: &str = "conversation_not_found";
    /// An `open_session` asked to resume a conversation found no record of it in the
    /// session's account directory. `params` names `agent`, the `path` of that directory and the
    /// `id` asked for.
    pub const CONVERSATION_TO_RESUME_NOT_FOUND: &str = "conversation_to_resume_not_found";
    /// An `open_session` asked to resume a conversation named an id that is not a UUID, which
    /// every agent's conversation ids are. `params` names the `id`.
    pub const RESUME_ID_INVALID: &str = "resume_id_invalid";
    /// An `open_session` asked to resume a conversation for a console session.
    pub const RESUME_FOR_CONSOLE_SESSION: &str = "resume_for_console_session";
    /// The conversation an `open_session` asked to resume is already recorded by a session
    /// outside the project (of another project, or a console session). `params` names the `id`.
    pub const CONVERSATION_OUTSIDE_PROJECT: &str = "conversation_outside_project";
    /// The conversation an `open_session` asked to resume is already recorded by a session of the
    /// same project, which resuming or reopening continues instead. `params` names the `id` and
    /// that `session`, by id.
    pub const CONVERSATION_IN_SESSION: &str = "conversation_in_session";
    /// Copying the conversation into the target account's directory did not complete. `params`
    /// names the target `path` and the `detail`.
    pub const RELOCATION_FAILED: &str = "relocation_failed";
    /// A switch ended the session's process and it was still not seen gone after a generous wait,
    /// so the switch stopped there: nothing was copied or recorded, and the process may still be
    /// running. `params` names the `session`.
    pub const SESSION_DID_NOT_STOP: &str = "session_did_not_stop";
    /// A switch relaunched the session and its process ended at once, so it is back on the
    /// account it had. `params` names the `session`.
    pub const SWITCH_DID_NOT_COME_UP: &str = "switch_did_not_come_up";
    /// A requested account name collides with an existing one of the same agent, trimmed and
    /// compared ignoring letter case — the default account's name takes part. `params` names
    /// `agent` and the `name` of the account it collides with.
    pub const ACCOUNT_NAME_TAKEN: &str = "account_name_taken";
    /// An approval of a `session_request` whose session is no longer waiting for it — the request
    /// was withdrawn or timed out — so nothing was started. `params` names the `session`.
    pub const SESSION_REQUEST_NOT_WAITING: &str = "session_request_not_waiting";
    /// An answer to a `session_request` that has already been answered, from this client or
    /// another, or that the daemon does not know. Nothing changed, and nothing is wrong.
    pub const SESSION_REQUEST_ANSWERED: &str = "session_request_answered";

    // -- browse requests ---------------------------------------------------------------------

    pub const INVALID_PATH: &str = "invalid_path";
    /// The directory a read is scoped to — the project's own, or its scope in another worktree —
    /// does not exist or is not a directory.
    pub const SOURCE_UNAVAILABLE: &str = "source_unavailable";
    pub const FILE_NOT_FOUND: &str = "file_not_found";
    pub const PERMISSION_DENIED: &str = "permission_denied";
    pub const OUTSIDE_SCOPE: &str = "outside_scope";
    pub const UNSUPPORTED_FILE_TYPE: &str = "unsupported_file_type";
    /// What was being read changed while it was read; reading again gets a consistent copy.
    pub const SOURCE_CHANGED: &str = "source_changed";
    pub const LIMIT_EXCEEDED: &str = "limit_exceeded";
    pub const NOT_A_GIT_REPOSITORY: &str = "not_a_git_repository";
    pub const GIT_UNAVAILABLE: &str = "git_unavailable";
    pub const WORKTREE_UNAVAILABLE: &str = "worktree_unavailable";
    pub const INVALID_BRANCH_NAME: &str = "invalid_branch_name";
    pub const UNKNOWN_BRANCH: &str = "unknown_branch";
    pub const INVALID_COMMIT: &str = "invalid_commit";
    pub const UNKNOWN_COMMIT: &str = "unknown_commit";
    /// A comparison of two branches whose histories share no commit, so there is no point they
    /// forked at to compare from. `params` names `left` and `right`, the two branches.
    pub const NO_MERGE_BASE: &str = "no_merge_base";
    /// As [`NO_MERGE_BASE`], in a shallow clone: the history that would show a common commit may
    /// simply not be there.
    pub const NO_MERGE_BASE_SHALLOW: &str = "no_merge_base_shallow";
    pub const GIT_FAILED: &str = "git_failed";
    /// The system terminal could not be opened at a project's directory. `params` names `detail`.
    pub const TERMINAL_UNAVAILABLE: &str = "terminal_unavailable";
    /// The system file manager could not open a project directory. `params` names `detail`.
    pub const DIRECTORY_UNAVAILABLE: &str = "directory_unavailable";
    pub const PATH_OPEN_FAILED: &str = "path_open_failed";
    /// A push was asked for while `HEAD` is detached. Nothing was pushed.
    pub const NOT_ON_A_BRANCH: &str = "not_on_a_branch";
    /// A push was asked for on a branch that has no commit yet. Nothing was pushed.
    pub const NO_COMMITS_TO_PUSH: &str = "no_commits_to_push";
    /// A branch with no upstream, and neither `origin` nor exactly one remote to send it to.
    /// Nothing was pushed.
    pub const NO_PUSH_REMOTE: &str = "no_push_remote";
    /// The branch's upstream does not name a remote of this repository. Nothing was pushed.
    pub const UPSTREAM_NOT_A_REMOTE: &str = "upstream_not_a_remote";
    /// `git push` failed, or the branch name cannot be placed in a refspec. `params` names
    /// `detail`, `git`'s own message when it ran.
    pub const GIT_PUSH_FAILED: &str = "git_push_failed";
    /// Switching to a local branch failed. `params` names `detail`, Git's message when it ran.
    pub const GIT_SWITCH_FAILED: &str = "git_switch_failed";
    pub const RESTORE_NOT_ON_BRANCH: &str = "restore_not_on_branch";
    pub const NO_COMMITS_TO_RESTORE: &str = "no_commits_to_restore";
    pub const GIT_OPERATION_IN_PROGRESS: &str = "git_operation_in_progress";
    /// Restoring tracked changes failed. `params` names `detail`, Git's message when it ran.
    pub const GIT_RESTORE_FAILED: &str = "git_restore_failed";
    /// A `read_project_change` whose change names no side its group can read.
    pub const INVALID_CHANGE: &str = "invalid_change";
    /// A body asked of a side on the disk whose path has a clean filter in effect: the patch was
    /// made from that filter's output, which cannot be read without writing an object. `params`
    /// names the `path` and the `filter` driver.
    pub const CLEAN_FILTERED: &str = "clean_filtered";
    /// A browse request was given up because a newer one took its slot. A client shows nothing.
    pub const REQUEST_SUPERSEDED: &str = "request_superseded";
}

/// Codes carried by `ModelsUnavailable`; `PROTOCOL.md` lists each with its params. Like an error
/// code, each needs a `daemon.<code>` message in the UI's catalogs.
pub mod models_unavailable_code {
    /// The agent's binary does not resolve on the login shell's `PATH`. `params` names `binary`.
    pub const BINARY_NOT_FOUND: &str = "models_binary_not_found";
    /// The login-shell snapshot the command runs in could not be taken. `params` names `detail`.
    pub const SHELL_ENVIRONMENT_FAILED: &str = "models_shell_environment_failed";
    /// The command did not finish in time. `params` names `agent`.
    pub const TIMED_OUT: &str = "models_timed_out";
    /// The command could not be run, or exited non-zero. `params` names `agent` and `detail`.
    pub const COMMAND_FAILED: &str = "models_command_failed";
    /// The command succeeded but what it printed holds no model that could be read. `params`
    /// names `agent`.
    pub const UNREADABLE_OUTPUT: &str = "models_unreadable_output";
}

/// The `reason_code` param of a failed press of a trust confirmation, in the `error` and in the
/// `session_notice` alike; `PROTOCOL.md` lists each. A client that does not know one shows the
/// English `reason` instead. A new reason also needs a `daemon.trust_reason.<code>` message in the
/// UI's catalogs.
pub mod trust_reason {
    pub const SCREEN_GONE: &str = "screen_gone";
    pub const CURSOR_NOT_AT_START: &str = "cursor_not_at_start";
    pub const CURSOR_DID_NOT_MOVE: &str = "cursor_did_not_move";
    pub const TERMINAL_NOT_SETTLED: &str = "terminal_not_settled";
    pub const CURSOR_MOVED_AWAY: &str = "cursor_moved_away";
    pub const SCREEN_NOT_DISMISSED: &str = "screen_not_dismissed";
    pub const SCREEN_REDRAWN: &str = "screen_redrawn";
    pub const INPUT_TOUCHED: &str = "input_touched";
    pub const TERMINAL_WRITE_FAILED: &str = "terminal_write_failed";
    // Of `trust_not_carried_over`: why Grok Build's trust entry did not reach the user's store.
    pub const NOT_RECORDED: &str = "not_recorded";
    pub const STORE_LOCKED: &str = "store_locked";
    pub const STORE_UNREADABLE: &str = "store_unreadable";
    pub const STORE_IO_FAILED: &str = "store_io_failed";
}

/// Codes carried by `Event::SessionNotice`; `PROTOCOL.md` lists each with its params. Like an error
/// code, each needs a `daemon.<code>` message in the UI's catalogs.
pub mod notice_code {
    pub const LAUNCH_FAILED: &str = "launch_failed";
    pub const CLAUDE_WORKSPACE_UNTRUSTED: &str = "claude_workspace_untrusted";
    pub const QUEUED_MESSAGES_DROPPED: &str = "queued_messages_dropped";
    pub const TRUST_ANSWER_FAILED: &str = "trust_answer_failed";
    pub const TRUST_NOT_CARRIED_OVER: &str = "trust_not_carried_over";
}

pub fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{ProjectAccount, ProjectSource, Request, RequestBody};

    #[test]
    fn an_absent_detect_default_agent_means_true() {
        let json = r#"{"type":"add_project","console_id":"c","source":"local"}"#;
        match serde_json::from_str::<Request>(json).unwrap().body {
            RequestBody::AddProject {
                detect_default_agent,
                ..
            } => assert!(detect_default_agent),
            _ => panic!("not add_project"),
        }
    }

    /// `github` is the old name of the `git` source; a request still naming it must be accepted,
    /// and the source is emitted as `git`.
    #[test]
    fn the_github_source_is_read_as_git_and_written_as_git() {
        for name in ["git", "github"] {
            let json = format!(r#"{{"type":"add_project","console_id":"c","source":"{name}"}}"#);
            match serde_json::from_str::<Request>(&json).unwrap().body {
                RequestBody::AddProject { source, path, .. } => {
                    assert_eq!(source, ProjectSource::Git);
                    assert!(path.is_none());
                }
                _ => panic!("not add_project"),
            }
        }
        assert_eq!(
            serde_json::to_value(ProjectSource::Git).unwrap(),
            serde_json::json!("git")
        );
    }

    /// The envelope's request id and the request's own fields arrive in one flat object, so a
    /// request field named `id` would be indistinguishable from the envelope's — and the daemon
    /// would act on the request id as if it were a record id. These are the requests that address
    /// a stored record; each must survive an envelope id sitting next to it.
    #[test]
    fn an_envelope_id_never_shadows_what_a_request_acts_on() {
        type Target = fn(RequestBody) -> Option<String>;
        let cases: &[(&str, Target)] = &[
            (
                r#"{"type":"delete_console","id":"request-1","console":"console-7"}"#,
                |body| match body {
                    RequestBody::DeleteConsole { console } => Some(console),
                    _ => None,
                },
            ),
            (
                r#"{"type":"delete_project","id":"request-1","project":"project-7"}"#,
                |body| match body {
                    RequestBody::DeleteProject { project, .. } => Some(project),
                    _ => None,
                },
            ),
            (
                r#"{"type":"update_project","id":"request-1","project":"project-7"}"#,
                |body| match body {
                    RequestBody::UpdateProject { project, .. } => Some(project),
                    _ => None,
                },
            ),
            (
                r#"{"type":"archive_session","id":"request-1","session":"session-7"}"#,
                |body| match body {
                    RequestBody::ArchiveSession { session } => Some(session),
                    _ => None,
                },
            ),
            (
                r#"{"type":"switch_session_account","id":"request-1","session":"session-7","account":null}"#,
                |body| match body {
                    RequestBody::SwitchSessionAccount { session, .. } => Some(session),
                    _ => None,
                },
            ),
            (
                r#"{"type":"delete_session","id":"request-1","session":"session-7"}"#,
                |body| match body {
                    RequestBody::DeleteSession { session, .. } => Some(session),
                    _ => None,
                },
            ),
            (
                r#"{"type":"set_session_pinned","id":"request-1","session":"session-7","pinned":true}"#,
                |body| match body {
                    RequestBody::SetSessionPinned { session, .. } => Some(session),
                    _ => None,
                },
            ),
            (
                r#"{"type":"set_session_colour","id":"request-1","session":"session-7","colour":"pink"}"#,
                |body| match body {
                    RequestBody::SetSessionColour { session, .. } => Some(session),
                    _ => None,
                },
            ),
            (
                r#"{"type":"submit_page","id":"request-1","page":"page-7","data":{}}"#,
                |body| match body {
                    RequestBody::SubmitPage { page, .. } => Some(page),
                    _ => None,
                },
            ),
            (
                r#"{"type":"confirm_trust","id":"request-1","session":"session-7","remember":true}"#,
                |body| match body {
                    RequestBody::ConfirmTrust { session, .. } => Some(session),
                    _ => None,
                },
            ),
            (
                r#"{"type":"list_pages","id":"request-1","console_session":"session-7"}"#,
                |body| match body {
                    RequestBody::ListPages { console_session } => Some(console_session),
                    _ => None,
                },
            ),
            (
                r#"{"type":"refresh_git_status","id":"request-1","console":"console-7"}"#,
                |body| match body {
                    RequestBody::RefreshGitStatus { console } => Some(console),
                    _ => None,
                },
            ),
            (
                r#"{"type":"pull_project_branch","id":"request-1","project":"project-7"}"#,
                |body| match body {
                    RequestBody::PullProjectBranch { project } => Some(project),
                    _ => None,
                },
            ),
            (
                r#"{"type":"open_project_terminal","id":"request-1","project":"project-7"}"#,
                |body| match body {
                    RequestBody::OpenProjectTerminal { project } => Some(project),
                    _ => None,
                },
            ),
            (
                r#"{"type":"reveal_project_path","id":"request-1","project":"project-7","path":"src/a%25.txt","worktree":"tree-1"}"#,
                |body| match body {
                    RequestBody::RevealProjectPath {
                        project,
                        path,
                        worktree,
                    } if path == "src/a%25.txt" && worktree.as_deref() == Some("tree-1") => {
                        Some(project)
                    }
                    _ => None,
                },
            ),
            (
                r#"{"type":"open_project_path","id":"request-1","project":"project-7","path":"src/a%25.txt"}"#,
                |body| match body {
                    RequestBody::OpenProjectPath {
                        project,
                        path,
                        worktree,
                    } if path == "src/a%25.txt" && worktree.is_none() => Some(project),
                    _ => None,
                },
            ),
            (
                r#"{"type":"open_project_directory","id":"request-1","project":"project-7"}"#,
                |body| match body {
                    RequestBody::OpenProjectDirectory { project } => Some(project),
                    _ => None,
                },
            ),
            (
                r#"{"type":"push_project_branch","id":"request-1","project":"project-7"}"#,
                |body| match body {
                    RequestBody::PushProjectBranch { project } => Some(project),
                    _ => None,
                },
            ),
            (
                r#"{"type":"switch_project_branch","id":"request-1","project":"project-7","branch":"feature/100%25"}"#,
                |body| match body {
                    RequestBody::SwitchProjectBranch { project, branch } => {
                        assert_eq!(branch, "feature/100%25");
                        Some(project)
                    }
                    _ => None,
                },
            ),
            (
                r#"{"type":"restore_project_changes","id":"request-1","project":"project-7"}"#,
                |body| match body {
                    RequestBody::RestoreProjectChanges { project } => Some(project),
                    _ => None,
                },
            ),
        ];

        for (json, target) in cases {
            let request: Request = serde_json::from_str(json).expect(json);
            assert_eq!(request.id.as_deref(), Some("request-1"), "{json}");
            let target = target(request.body).expect(json);
            assert!(target.ends_with("-7"), "{json} acted on `{target}`");
        }
    }

    /// A switch must name its target: an explicit null is the default account, and a request that
    /// names nothing is a different request, which the coordinator refuses rather than reading as
    /// the default.
    #[test]
    fn a_switch_names_the_default_account_with_an_explicit_null() {
        let account = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
            RequestBody::SwitchSessionAccount { account, .. } => account,
            _ => panic!("parses as switch_session_account"),
        };
        let kind = r#""type":"switch_session_account","session":"s""#;
        assert_eq!(account(&format!("{{{kind}}}")), None);
        assert_eq!(
            account(&format!(r#"{{{kind},"account":null}}"#)),
            Some(None)
        );
        assert_eq!(
            account(&format!(r#"{{{kind},"account":"a"}}"#)),
            Some(Some("a".to_string()))
        );
    }

    /// An absent optional field is "leave it alone" and an explicit null is "clear it", which is
    /// the only way a project can go back to inheriting its console's default agent.
    #[test]
    fn clearing_a_projects_default_agent_is_distinguishable_from_leaving_it() {
        let absent: Request =
            serde_json::from_str(r#"{"type":"update_project","project":"p"}"#).unwrap();
        let cleared: Request =
            serde_json::from_str(r#"{"type":"update_project","project":"p","default_agent":null}"#)
                .unwrap();
        match (absent.body, cleared.body) {
            (
                RequestBody::UpdateProject {
                    default_agent: absent,
                    ..
                },
                RequestBody::UpdateProject {
                    default_agent: cleared,
                    ..
                },
            ) => {
                assert_eq!(absent, None);
                assert_eq!(cleared, Some(None));
            }
            _ => panic!("both parse as update_project"),
        }
    }

    /// An absent `tags` leaves a project's tags alone, and a present array (even an empty one) is
    /// what replaces them.
    #[test]
    fn updating_a_projects_tags_is_distinguishable_from_leaving_them() {
        let parse = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
            RequestBody::UpdateProject { tags, .. } => tags,
            _ => panic!("parses as update_project"),
        };
        assert_eq!(parse(r#"{"type":"update_project","project":"p"}"#), None);
        assert_eq!(
            parse(r#"{"type":"update_project","project":"p","tags":[]}"#),
            Some(vec![])
        );
        assert_eq!(
            parse(r#"{"type":"update_project","project":"p","tags":["a","b"]}"#),
            Some(vec!["a".to_string(), "b".to_string()])
        );
    }

    /// A project's accounts, models and efforts are cleared the same way as a console's: absent
    /// leaves one alone, `null` clears it (an account back to inheriting), and each agent's is
    /// independent; a create request carries a model as a plain optional string and an account as
    /// a [`ProjectAccount`], absent or `null` being inheriting there too.
    #[test]
    fn clearing_a_projects_settings_is_distinguishable_from_leaving_them() {
        let parse = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
            RequestBody::UpdateProject {
                claude_account_id,
                grok_account_id,
                claude_session_model,
                codex_session_effort,
                ..
            } => (
                claude_account_id,
                grok_account_id,
                claude_session_model,
                codex_session_effort,
            ),
            _ => panic!("parses as update_project"),
        };
        assert_eq!(
            parse(r#"{"type":"update_project","project":"p"}"#),
            (None, None, None, None)
        );
        assert_eq!(
            parse(
                r#"{"type":"update_project","project":"p","claude_account_id":null,
                    "grok_account_id":{"kind":"account","id":"acct-1"},
                    "claude_session_model":"opus","codex_session_effort":null}"#
            ),
            (
                Some(None),
                Some(Some(ProjectAccount::Account {
                    id: "acct-1".to_string()
                })),
                Some(Some("opus".to_string())),
                Some(None)
            )
        );

        let RequestBody::AddProject {
            claude_account_id,
            codex_account_id,
            grok_account_id,
            grok_session_model,
            claude_session_effort,
            ..
        } = serde_json::from_str::<Request>(
            r#"{"type":"add_project","console_id":"c","source":"local","path":"/p",
                "codex_account_id":{"kind":"account","id":"acct-2"},
                "grok_account_id":{"kind":"default"},"grok_session_model":"grok-4"}"#,
        )
        .unwrap()
        .body
        else {
            panic!("parses as add_project");
        };
        assert_eq!(claude_account_id, None);
        assert_eq!(
            codex_account_id,
            Some(ProjectAccount::Account {
                id: "acct-2".to_string()
            })
        );
        assert_eq!(grok_account_id, Some(ProjectAccount::Default));
        assert_eq!(grok_session_model.as_deref(), Some("grok-4"));
        assert_eq!(claude_session_effort, None);
    }

    /// A project's account takes all three states on an update: `null` is inheriting, and an
    /// object says the agent's default account or a named one; absent leaves it alone.
    #[test]
    fn a_projects_account_is_inherit_default_or_named_on_an_update() {
        let parse = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
            RequestBody::UpdateProject {
                claude_account_id, ..
            } => claude_account_id,
            _ => panic!("parses as update_project"),
        };
        let update = |field: &str| format!(r#"{{"type":"update_project","project":"p"{field}}}"#);
        assert_eq!(parse(&update("")), None);
        assert_eq!(parse(&update(r#","claude_account_id":null"#)), Some(None));
        assert_eq!(
            parse(&update(r#","claude_account_id":{"kind":"inherit"}"#)),
            Some(Some(ProjectAccount::Inherit))
        );
        assert_eq!(
            parse(&update(r#","claude_account_id":{"kind":"default"}"#)),
            Some(Some(ProjectAccount::Default))
        );
        assert_eq!(
            parse(&update(
                r#","claude_account_id":{"kind":"account","id":"a"}"#
            )),
            Some(Some(ProjectAccount::Account {
                id: "a".to_string()
            }))
        );
        // A bare string is not an account any more: nothing reads an id without saying what it is.
        assert!(serde_json::from_str::<Request>(&update(r#","claude_account_id":"a""#)).is_err());
    }

    /// Absent versus null is the only way a console's account can be cleared as well as set, and
    /// each agent's is independent of the others.
    #[test]
    fn clearing_a_consoles_account_is_distinguishable_from_leaving_it() {
        let parse = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
            RequestBody::UpdateConsole {
                claude_account_id,
                codex_account_id,
                grok_account_id,
                ..
            } => (claude_account_id, codex_account_id, grok_account_id),
            _ => panic!("parses as update_console"),
        };
        assert_eq!(
            parse(r#"{"type":"update_console","console":"c"}"#),
            (None, None, None)
        );
        assert_eq!(
            parse(
                r#"{"type":"update_console","console":"c","claude_account_id":null,
                    "codex_account_id":"acct-1"}"#
            ),
            (Some(None), Some(Some("acct-1".to_string())), None)
        );
        assert_eq!(
            parse(r#"{"type":"update_console","console":"c","grok_account_id":"acct-2"}"#),
            (None, None, Some(Some("acct-2".to_string())))
        );
    }

    /// A session's own account is chosen, or deliberately the default one, or left to the console,
    /// by the same absent-versus-null distinction.
    #[test]
    fn choosing_the_default_account_for_a_session_is_distinguishable_from_choosing_none() {
        let parse = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
            RequestBody::OpenSession { account, .. } => account,
            _ => panic!("parses as open_session"),
        };
        assert_eq!(parse(r#"{"type":"open_session","console_id":"c"}"#), None);
        assert_eq!(
            parse(r#"{"type":"open_session","console_id":"c","account":null}"#),
            Some(None)
        );
        assert_eq!(
            parse(r#"{"type":"open_session","console_id":"c","account":"acct-1"}"#),
            Some(Some("acct-1".to_string()))
        );
    }

    /// The conversation to resume is optional on the wire: an `open_session` without it still parses.
    #[test]
    fn the_conversation_to_resume_is_optional() {
        let parse = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
            RequestBody::OpenSession {
                resume_agent_session_id,
                ..
            } => resume_agent_session_id,
            _ => panic!("parses as open_session"),
        };
        assert_eq!(parse(r#"{"type":"open_session","console_id":"c"}"#), None);
        assert_eq!(
            parse(r#"{"type":"open_session","console_id":"c","resume_agent_session_id":"u"}"#),
            Some("u".to_string())
        );
    }

    /// Each session launch setting distinguishes inheriting from deliberately using the agent's
    /// own default, and a record emitted on the wire keeps that distinction too.
    #[test]
    fn session_launch_settings_preserve_absent_null_and_values() {
        for (fields, expected) in [
            ("", (None, None)),
            (
                r#","model":null,"effort":"high""#,
                (Some(None), Some(Some("high".to_string()))),
            ),
            (
                r#","model":"opus","effort":null"#,
                (Some(Some("opus".to_string())), Some(None)),
            ),
        ] {
            let json = format!(r#"{{"type":"open_session","console_id":"c"{fields}}}"#);
            let RequestBody::OpenSession { model, effort, .. } =
                serde_json::from_str::<Request>(&json).unwrap().body
            else {
                panic!("parses as open_session")
            };
            assert_eq!((model, effort), expected, "{json}");

            let json = format!(
                r#"{{"id":"s","agent":"claude","agent_session_id":null,
                "console_id":"c","project_id":null,"host_id":"local","role":"console",
                "origin":"user","title":"Hub","status":"idle","has_conversation":false,
                "bound_to":null,"colour":null,"ordinal":null,"account_id":null,"config_dir":null,
                "pinned":false,"started_at":0,"ended_at":null{fields}}}"#
            );
            let session: super::Session = serde_json::from_str(&json).unwrap();
            assert_eq!((session.model.clone(), session.effort.clone()), expected);
            let emitted = serde_json::to_value(&session).unwrap();
            for (field, value) in [("model", expected.0), ("effort", expected.1)] {
                assert_eq!(emitted.get(field).is_some(), value.is_some());
                if let Some(value) = value {
                    assert_eq!(emitted[field], serde_json::to_value(value).unwrap());
                }
            }
        }
    }

    /// The avatar is cleared the same way as an account: an explicit `null`.
    #[test]
    fn clearing_a_consoles_icon_is_distinguishable_from_leaving_it() {
        let parse = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
            RequestBody::UpdateConsole { icon, .. } => icon,
            _ => panic!("parses as update_console"),
        };
        assert_eq!(parse(r#"{"type":"update_console","console":"c"}"#), None);
        assert_eq!(
            parse(r#"{"type":"update_console","console":"c","icon":null}"#),
            Some(None)
        );
        assert_eq!(
            parse(r#"{"type":"update_console","console":"c","icon":"data:image/png;base64,AA"}"#),
            Some(Some("data:image/png;base64,AA".to_string()))
        );
    }

    /// A console session's model is cleared the same way, and each agent's is independent.
    #[test]
    fn clearing_a_console_session_model_is_distinguishable_from_leaving_it() {
        let parse = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
            RequestBody::UpdateConsole {
                claude_console_session_model,
                codex_console_session_model,
                ..
            } => (claude_console_session_model, codex_console_session_model),
            _ => panic!("parses as update_console"),
        };
        assert_eq!(
            parse(r#"{"type":"update_console","console":"c"}"#),
            (None, None)
        );
        assert_eq!(
            parse(r#"{"type":"update_console","console":"c","claude_console_session_model":null}"#),
            (Some(None), None)
        );
        assert_eq!(
            parse(
                r#"{"type":"update_console","console":"c","codex_console_session_model":"gpt-5"}"#
            ),
            (None, Some(Some("gpt-5".to_string())))
        );
    }

    #[test]
    fn creating_a_console_needs_no_account() {
        let request: Request =
            serde_json::from_str(r#"{"type":"create_console","name":"n","default_agent":"codex"}"#)
                .unwrap();
        match request.body {
            RequestBody::CreateConsole {
                claude_account_id,
                codex_account_id,
                grok_account_id,
                ..
            } => assert_eq!(
                (claude_account_id, codex_account_id, grok_account_id),
                (None, None, None)
            ),
            _ => panic!("parses as create_console"),
        }
    }

    /// `trust_parent_dir` is newer than `remember`, so a request without it still reads, as "this
    /// project only".
    #[test]
    fn a_trust_confirmation_without_a_scope_means_this_project() {
        let parse = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
            RequestBody::ConfirmTrust {
                remember,
                trust_parent_dir,
                ..
            } => (remember, trust_parent_dir),
            _ => panic!("parses as confirm_trust"),
        };
        assert_eq!(
            parse(r#"{"type":"confirm_trust","session":"s","remember":true}"#),
            (true, false)
        );
        assert_eq!(
            parse(
                r#"{"type":"confirm_trust","session":"s","remember":false,"trust_parent_dir":true}"#
            ),
            (false, true)
        );
        match serde_json::from_str::<Request>(
            r#"{"type":"remove_trusted_directory","path":"/work"}"#,
        )
        .unwrap()
        .body
        {
            RequestBody::RemoveTrustedDirectory { path } => assert_eq!(path, "/work"),
            _ => panic!("parses as remove_trusted_directory"),
        }
    }

    #[test]
    fn the_trusted_directories_travel_under_their_own_names() {
        let updated = serde_json::to_value(super::Event::TrustedDirectoriesUpdated {
            trusted_directories: vec!["/work".to_string()],
        })
        .unwrap();
        assert_eq!(updated["type"], "trusted_directories_updated");
        assert_eq!(updated["trusted_directories"][0], "/work");
        let snapshot = serde_json::to_value(super::Event::Snapshot {
            hosts: vec![],
            consoles: vec![],
            projects: vec![],
            sessions: vec![],
            trusted_directories: vec!["/work".to_string()],
            settings: super::Settings::default(),
            git_statuses: vec![],
            agent_availability: [
                super::Agent::Claude,
                super::Agent::Codex,
                super::Agent::Grok,
            ]
            .into_iter()
            .map(|agent| super::AgentAvailability {
                agent,
                availability: super::Availability::NotDetermined,
                default_account_dir: None,
            })
            .collect(),
            home_dir: Some("/Users/dev".to_string()),
        })
        .unwrap();
        assert_eq!(snapshot["trusted_directories"][0], "/work");
        assert_eq!(snapshot["home_dir"], "/Users/dev");
    }

    /// A client words a failure and a notice from their code and params; the English message rides
    /// along for one that does not know the code.
    #[test]
    fn an_error_and_a_notice_carry_a_code_and_named_params() {
        let error = super::CodedError::unknown_console("console-7");
        let coded = error.downcast_ref::<super::CodedError>().expect("coded");
        let event = serde_json::to_value(coded.reply(Some("request-1".to_string()))).unwrap();
        assert_eq!(event["code"], "unknown_console");
        assert_eq!(event["params"]["console"], "console-7");
        assert_eq!(event["message"], "unknown console console-7");

        let event =
            serde_json::to_value(super::Event::from_error(None, &anyhow::anyhow!("boom"))).unwrap();
        assert_eq!(event["code"], "internal_error");
        assert_eq!(event["params"]["detail"], "boom");
        assert_eq!(event["message"], "boom");

        let notice = serde_json::to_value(
            super::Notice::new("a_notice", "English", &[("reason", "why")]).about("session-1"),
        )
        .unwrap();
        assert_eq!(notice["type"], "session_notice");
        assert_eq!(notice["session"], "session-1");
        assert_eq!(notice["code"], "a_notice");
        assert_eq!(notice["params"]["reason"], "why");
        assert_eq!(notice["message"], "English");
    }

    /// The not-yet-determined state has to be representable on the wire, not implied by an
    /// absent field: a client reading `availability` must be able to tell "not checked yet" from
    /// "checked and found unavailable" without inferring either from whether the entry exists at
    /// all or from `default_account_dir` being present.
    #[test]
    fn not_yet_determined_is_an_explicit_value_not_an_absent_entry() {
        let entry = super::AgentAvailability {
            agent: super::Agent::Grok,
            availability: super::Availability::NotDetermined,
            default_account_dir: None,
        };
        let json = serde_json::to_value(&entry).unwrap();
        assert_eq!(json["agent"], "grok");
        assert_eq!(json["availability"], "not_determined");
        assert!(json["default_account_dir"].is_null());

        let determined = super::AgentAvailability {
            agent: super::Agent::Grok,
            availability: super::Availability::Unavailable,
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
        use super::ConsoleSessionColour;

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
