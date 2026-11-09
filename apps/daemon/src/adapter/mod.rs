//! The provider behavior registry: one [`AgentProvider`] per agent CLI, beside the static facts of
//! its descriptor (`crate::providers`). Each provider's module assembles the launch of its CLI so
//! Octoboard's capabilities are injected *additionally*, without modifying project files or the
//! user's own configuration, and reads the titles it shows and records; [`provider`] gives an
//! agent's. Knowledge of how one provider behaves lives in its provider: its descriptor and its
//! module here, how its hook events are read, how its records are found, read and watched, how its
//! screens, dialogs and terminal title read, what its accounts can be asked (quota, sign-in, reset
//! credits, shared configuration, thread registry), how its models are listed, and its status line
//! and trust carrying included.
//!
//! Three things are injected into every agent: the status hooks, the Octoboard MCP server, and a
//! role description, plus a status line for a provider that declares one
//! (`Capabilities::status_line`; "What is injected on every launch" in
//! `docs/product/launching-agents.md`). The mechanism for each differs per agent and is the
//! adapter's own business; the conditions the shared ones meet are below.
//!
//! Three rules hold for every adapter, each of which fails silently if broken:
//!
//! - **Resume re-injects everything.** For every provider, hooks and the MCP server are resolved
//!   from the launch arguments every time and are lost on a resume that omits them, leaving a
//!   session nobody observes. Adapters therefore assemble the full injection on every launch,
//!   resume included.
//! - **A role cannot be changed after the first turn.** Claude Code records an appended system
//!   prompt on the conversation's first request and replays it verbatim afterwards, so different
//!   text passed on a later launch is silently ignored; Grok persists `--rules` into the session
//!   record. The role text is therefore a function of the session's kind, the owner it had at
//!   launch and its agent alone, and names that owner: a later change of owner is told to the
//!   session in prose and replaces what the text says (a coordinator that sets a console parent
//!   only after it was launched independent is launched with that parent named from then on). It
//!   is still passed on every launch, because Claude Code re-renders its snapshot from whatever
//!   *that* launch passed after a compaction.
//! - **Hooks must fail fast and silently.** Every agent surfaces a failing hook to the user, and a
//!   hook with no timeout blocks the turn for its full duration. Octoboard owns the hook command
//!   (`hooks::hook_command`; Codex and Grok run it as the per-session `hook_script`, Claude Code
//!   inline, or not at all when its version posts hooks over HTTP itself), which exits 0
//!   unconditionally and writes nothing.
//!
//! The hook command takes no event argument: it derives the event from the payload on stdin, so
//! one constant command serves every event, and adding or removing an event never changes what an
//! adapter has to generate beyond the event name itself.

pub(crate) mod account_cli;
pub mod claude;
pub mod codex;
mod compatibility_offer;
pub(crate) mod files;
pub mod grok;
mod hooks;
#[cfg(test)]
mod test_provider;

pub use account_cli::{
    account_command_of, account_dir, default_account_dir, resolve_account_binary, Unstartable,
};
pub use hooks::{
    hook_command, hook_script_path, hook_url, write_hook_script, COMMAND_HOOK_TIMEOUT_SECS,
    HTTP_HOOK_TIMEOUT_SECS,
};

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;

use crate::agents::models::{ModelCatalogue, NoCatalogue};
use crate::hooks::{HookInterpreter, NoHooks};
use crate::platform::shell::ResolvedBinary;
use crate::protocol::{
    error_code, AgentId, CodedError, GitWorkflowSettings, Notice, SessionKind, TerminalAppearance,
};
use crate::providers::{ProviderDescriptor, ProviderSettings};
use crate::records::{NoRecords, RecordStore};
use crate::session::screen::{NoScreen, ScreenReader};
use crate::session_tools::Owner;

/// Everything an adapter needs to assemble one launch.
pub struct LaunchSpec<'a> {
    /// Octoboard's own session id. Not the agent's — it identifies the session to the daemon, and
    /// is what the injected MCP server is launched with.
    pub session_id: &'a str,
    /// Whether this is a console or a project session, which decides both the role text and the
    /// tools its MCP server announces.
    pub kind: SessionKind,
    /// Who this session reports to, if anyone, which decides what its role text tells it is
    /// waiting on it and which tools its MCP server announces. Never set for a console session.
    pub owner: Option<Owner>,
    /// The project's effective git workflow settings, read when the launch was assembled, which
    /// shape the development rule in the role text. Never set for a console session.
    pub git_workflow: Option<GitWorkflowSettings>,
    /// The application setting `ask_questions_as_choices` as it stood when the launch was
    /// assembled, which decides whether a project session's role text asks it to put questions
    /// to the user as options to pick from.
    pub ask_questions_as_choices: bool,
    /// The tag of the language the user's interface is set to, once a client has reported one; it
    /// decides whether a project session's role text names that language.
    pub user_language: Option<&'a str>,
    /// The id to give an agent that can pre-allocate one for a *new* conversation. Freshly
    /// generated per launch: both Claude Code and Grok refuse an id that already has a stored
    /// conversation, so reusing one would make relaunching a session that was opened and never
    /// typed into fail with an opaque launch error.
    pub new_agent_session_id: &'a str,
    /// The agent's own session id, set when this launch resumes an existing conversation.
    pub resume_agent_session_id: Option<&'a str>,
    pub cwd: &'a Path,
    /// Initial task, rendered as the agent's positional prompt argument. Only on a fresh launch.
    pub task: Option<&'a str>,
    /// Per-session scratch directory, already created. Adapters may write into it whatever their
    /// injection needs; it is removed when the session's process is gone.
    pub scratch: &'a Path,
    /// Absolute path of the hook script for this session. Written only for a provider that
    /// declares the `hook_script` capability; for any other it is where the script would be,
    /// and nothing is there.
    pub hook_script: &'a Path,
    /// The filtered snapshot of the user's shell environment this launch will use. Adapters read
    /// the user's own settings from here rather than from the daemon's own environment, which may
    /// be that of an agent session the daemon happens to have been started from.
    pub shell_env: &'a HashMap<String, String>,
    /// The configuration directory of this session's own agent that it is pinned to, when it has
    /// one. Each adapter exposes it through its agent's own mechanism (`CLAUDE_CONFIG_DIR`,
    /// `CODEX_HOME`, or the directory Grok's per-session home is built from), layered over whatever
    /// the shell snapshot carries; absent leaves that snapshot untouched. An adapter reads it only
    /// through [`pinned_config_dir`], which refuses a directory that has vanished, and passes the
    /// result on to whatever else needs it.
    pub config_dir: Option<&'a Path>,
    /// The session's own provider's settings in effect when the launch was assembled
    /// (`Store::provider_settings_of`). A provider reads only the ids its descriptor declares.
    pub settings: &'a ProviderSettings,
    /// The compatibility sources this session's provider reads, one for each of its compatibility
    /// switches (`SettingDescriptor::compatibility_with`) that is on, keyed by the agent the switch
    /// names: the MCP servers and instructions of that provider, offered through its
    /// [`CompatibilitySource`]. Read one through [`LaunchSpec::compatibility_source`].
    pub compatibility_sources: Vec<(AgentId, &'static dyn CompatibilitySource)>,
    /// The default account's directory, fully resolved, when the session's account shares the
    /// default configuration (`crate::accounts::shared_config`); `None` for every other launch. An
    /// adapter whose agent keeps per-project memory in its conversation directory points that
    /// memory here, so it lands where the default account keeps it.
    pub shared_default_dir: Option<&'a Path>,
    /// The model to launch with, in the agent's own spelling; `None` leaves it to the agent. Read
    /// from the setting for this agent of the session's console (a console session) or of its
    /// project (a project session), and `None` when that setting is unset.
    pub model: Option<&'a str>,
    /// The reasoning effort to launch with, in the agent's own spelling; `None` leaves it to the
    /// agent. Set under the same conditions as [`Self::model`].
    pub effort: Option<&'a str>,
    /// The appearance of the terminal this launch will be shown in. Only the Grok adapter reads
    /// it, to set `GROK_THEME`; absent means follow the operating system.
    pub terminal_appearance: Option<TerminalAppearance>,
    /// The version of the agent binary this launch starts, when it could be told without running
    /// it (see [`AgentProvider::installed_version`]); `None` makes an adapter fall back to what
    /// every version takes.
    pub agent_version: Option<AgentVersion>,
    /// The daemon binary, which is also the MCP server and the hook forwarder.
    pub self_exe: &'a str,
    /// Where the daemon is listening, for the MCP server this launch registers.
    pub daemon_port: u16,
    /// The key the daemon requires of every connection, for every client this launch starts or
    /// configures: the MCP server, the hooks and the status line.
    pub daemon_key: &'a str,
    /// The token that session's MCP server authenticates to the daemon with.
    pub mcp_token: &'a str,
}

impl LaunchSpec<'_> {
    /// The compatibility source of `agent` this launch reads: `Some` while the switch naming it is
    /// on.
    pub fn compatibility_source(&self, agent: AgentId) -> Option<&'static dyn CompatibilitySource> {
        self.compatibility_sources
            .iter()
            .find(|(source, _)| *source == agent)
            .map(|(_, source)| *source)
    }

    /// What the launch adds to the role description beyond the role and owner.
    pub fn role_options(&self) -> crate::session_tools::role::RoleOptions<'_> {
        crate::session_tools::role::RoleOptions {
            git_workflow: self.git_workflow.as_ref(),
            ask_as_choices: self.ask_questions_as_choices,
            user_language: self.user_language,
        }
    }
}

/// What an agent's own time limit on a call to Octoboard's tools is set to, for every agent that
/// has such a limit: `session_tools::CALL_TIMEOUT` plus a margin, so Octoboard's forwarding limit is
/// the one that ends a call rather than the agent's own default, whichever side of it that default
/// falls on. Derived from `CALL_TIMEOUT` so it stays above it if that is raised. The margin covers
/// a call in normal operation, not a daemon that stalls partway through a response: `CALL_TIMEOUT`
/// bounds each read, not the call. Why the ordering matters is in apps/daemon/PROTOCOL.md's
/// "`POST /mcp/:token` — Octoboard's MCP tools" section; what each agent does when its own limit
/// passes, and so what the setting prevents, is in that agent's section of
/// `docs/agent-cli-reference.md`. Every agent takes it per server, so no other MCP server of the
/// user's is touched. Claude Code has none to set: it moves a long call to the background rather
/// than ending it.
///
/// Each adapter's tests pin the resulting number, so a change to `CALL_TIMEOUT` is not finished by
/// updating those tests: the new value has to hold for every agent, against what that agent's
/// section records about its own limit.
pub const MCP_TOOL_TIMEOUT_SECS: u64 =
    crate::session_tools::CALL_TIMEOUT.as_secs() + MCP_TOOL_TIMEOUT_MARGIN.as_secs();

const MCP_TOOL_TIMEOUT_MARGIN: Duration = Duration::from_secs(60);

const _: () = assert!(MCP_TOOL_TIMEOUT_SECS > crate::session_tools::CALL_TIMEOUT.as_secs());

/// The command and arguments every adapter registers as this session's MCP server. The session's
/// identity travels in argv: Claude Code and Grok do export a session id to an MCP child, but
/// Codex exports nothing and Grok's `{{session_id}}` templating does not work, so argv is the one
/// route all three agree on.
pub fn mcp_server_command(spec: &LaunchSpec<'_>) -> (String, Vec<String>) {
    let mut args = vec![
        "mcp".to_string(),
        "--session".to_string(),
        spec.session_id.to_string(),
        "--kind".to_string(),
        spec.kind.name().to_string(),
        "--port".to_string(),
        spec.daemon_port.to_string(),
        "--key".to_string(),
        spec.daemon_key.to_string(),
        "--token".to_string(),
        spec.mcp_token.to_string(),
    ];
    if spec.owner.as_ref().is_some_and(Owner::of_worker) {
        args.push("--worker".to_string());
    }
    (spec.self_exe.to_string(), args)
}

/// The session's pinned configuration directory, refused when it has gone and this launch is
/// resuming a conversation the agent actually has — `spec.resume_agent_session_id` is set only
/// then (see `protocol::Session::has_conversation` and `coordinator::resume::resume_session`),
/// never for a fresh launch or for resuming a session that never had a turn. Those two launch into
/// the missing directory instead: Claude Code creates it itself, as it would for an unpinned
/// launch, and the Codex adapter creates it before launching.
///
/// Checked here rather than left to the agent: Claude Code would quietly create the missing
/// directory and come up logged out, without the conversation the resume was meant to continue —
/// an outcome that looks like success — and Codex would exit.
pub fn pinned_config_dir<'a>(spec: &LaunchSpec<'a>, agent: AgentId) -> Result<Option<&'a Path>> {
    match spec.config_dir {
        Some(dir) if !dir.is_dir() && spec.resume_agent_session_id.is_some() => {
            Err(CodedError::raised(
                error_code::CONFIG_DIR_UNREACHABLE,
                format!(
                    "the {} config directory `{}` is not a directory the daemon can reach; \
                     recreate it, or point the account at a different directory",
                    agent.label(),
                    dir.display()
                ),
                &[("agent", agent.label()), ("path", &dir.to_string_lossy())],
            ))
        }
        dir => Ok(dir),
    }
}

#[derive(Default)]
pub struct LaunchPlan {
    pub args: Vec<String>,
    /// Environment entries layered on top of the shell snapshot. The launch sets `TERM` after
    /// these, and in a project session `GIT_CONFIG_*`, `OCTOBOARD_SESSION_ID`,
    /// `OCTOBOARD_DAEMON_PORT` and `OCTOBOARD_DAEMON_KEY` too, so an adapter can neither see nor
    /// override their final values.
    pub env: Vec<(String, String)>,
    /// Codex's user configuration or account override can resolve permissions automatically, so
    /// a permission hook alone does not establish a user interaction. Visible action prompts are
    /// observed separately.
    pub resolves_approvals_itself: bool,
    /// Something about this launch the user has to be told, because the agent will not tell them
    /// in a way they can act on. Surfaced once, when the session starts.
    pub notice: Option<Notice>,
    /// Where the agent records the trust its confirmation grants when that record cannot reach the
    /// user's own configuration by itself, and so has to be carried there once the confirmation is
    /// accepted, by Octoboard's press or in the terminal. Only Grok, whose trust store lives in the
    /// per-session home.
    pub carried_trust: Option<std::sync::Arc<dyn crate::session::trust::TrustCarrier>>,
    /// Work the launch leaves for once its process has exited, done before the session's scratch
    /// directory is removed (Grok's copy of the settings a session changed onto the configuration
    /// file its account shares).
    pub after_exit: Option<Box<dyn crate::session::term::AfterExit>>,
}

/// A title an agent's own record gives its conversation. Of the titles one record gives, the
/// highest `precedence` wins and the last of equal precedence wins.
pub struct NamedTitle {
    pub text: String,
    pub precedence: u8,
}

/// Whether `text` is nothing but a spinner's glyphs, the braille patterns agents animate.
pub fn is_spinner(text: &str) -> bool {
    let mut glyphs = text.chars().filter(|c| !c.is_whitespace()).peekable();
    glyphs.peek().is_some() && glyphs.all(crate::terminal::title::is_braille_spinner_glyph)
}

/// What a provider offers the providers made compatible with it (`ProviderDescriptor::
/// compatibility_source`): its configuration, read the way it reads it. A consuming provider
/// takes these through this interface and knows nothing of the source's files.
pub trait CompatibilitySource: Send + Sync {
    /// The source's MCP servers in effect for `cwd`, by name, as its own configuration declares
    /// them (scopes merged, approvals and disables applied), each in the common MCP configuration
    /// format: `command`, `args` and `env` for a local server, `type: "http"` or `"sse"` with `url`
    /// and `headers` for a remote one. A `${` in an `args` entry, an `env` value or a header value is
    /// always a reference to an environment variable of the consumer's process (`${NAME}`, or
    /// `${NAME:-default}` where the source's own agent expands that): a source leaves out a server
    /// whose literal value holds one, which a consumer would otherwise expand where the source's
    /// agent does not.
    fn mcp_servers(
        &self,
        shell_env: &HashMap<String, String>,
        cwd: &Path,
    ) -> BTreeMap<String, serde_json::Value>;

    /// The guidance a consuming agent, named `consumer_label` in it, is given to read the source's
    /// instruction files, skills, rules and settings, in the source's own layout.
    fn instructions(&self, consumer_label: &str, shell_env: &HashMap<String, String>) -> String;

    /// The directories of the source's skills in effect for `cwd`, each holding a `SKILL.md`, a
    /// skill named once at its nearest place. Empty for a source that leaves skills to the consumer
    /// to find (`instructions`).
    fn skill_dirs(&self, _shell_env: &HashMap<String, String>, _cwd: &Path) -> Vec<PathBuf> {
        Vec::new()
    }

    /// The source's custom prompts, one Markdown file each, named by the file. Empty for a source
    /// that leaves them to the consumer to find (`instructions`).
    fn command_files(&self, _shell_env: &HashMap<String, String>) -> Vec<PathBuf> {
        Vec::new()
    }
}

/// A provider's status line, the one its launch injects as `octoboardd status-line --provider <id>
/// …` (`Capabilities::status_line`). Core only dispatches the command to it.
pub trait StatusLine: Send + Sync {
    /// Runs one update: reads what the provider wrote on stdin, reports the session's context
    /// window to the daemon at `port` with `key`, runs the user's own status line in its place and
    /// returns the exit status to leave with. `args` are the provider's own arguments after the
    /// fixed ones.
    fn run(&self, session: &str, port: u16, key: &str, args: &[String]) -> i32;
}

/// How one provider behaves: launching it and reading what it shows and writes, beside the static
/// facts of its [`ProviderDescriptor`]. Every method but the core has a default, so a provider
/// implements only what it has.
pub trait AgentProvider: Send + Sync {
    /// The provider's static facts.
    fn descriptor(&self) -> &'static ProviderDescriptor;

    /// How the provider's hook events are read.
    fn hooks(&self) -> &'static dyn HookInterpreter {
        &NoHooks
    }

    /// How the provider's own records of a session are found, read and watched.
    fn records(&self) -> &'static dyn RecordStore {
        &NoRecords
    }

    /// How the provider's terminal screens, input box and terminal title read.
    fn screen(&self) -> &'static dyn ScreenReader {
        &NoScreen
    }

    /// What the provider's accounts can be asked beyond launching: their quota, the email they
    /// sign in with, signing in and the configuration they share. `None` for a provider whose
    /// accounts are none of that.
    fn accounts(&self) -> Option<&'static dyn crate::accounts::AccountOps> {
        None
    }

    /// How the provider's models are listed, and which of them a session gets by default.
    fn catalogue(&self) -> &'static dyn ModelCatalogue {
        &NoCatalogue
    }

    /// The provider's status line, when it shows one Octoboard can feed
    /// (`Capabilities::status_line`).
    fn status_line(&self) -> Option<&'static dyn StatusLine> {
        None
    }

    /// What the provider offers a provider made compatible with it; `Some` exactly when its
    /// descriptor declares `compatibility_source`.
    fn compatibility_source(&self) -> Option<&'static dyn CompatibilitySource> {
        None
    }

    fn plan(&self, spec: &LaunchSpec<'_>) -> Result<LaunchPlan>;

    /// Refuses an account directory this provider cannot launch against, ahead of anything being
    /// done in it: the same rule `plan` applies at launch, asked beforehand so a switch can refuse
    /// before it copies a conversation into the directory. The copy would otherwise create files
    /// that make a directory that was never set up pass the check at launch (a Grok home's
    /// `sessions/`), which is exactly the silent failure that check exists to prevent. `pinned` is
    /// the account's own directory; the default account pins nothing and is not checked.
    fn refuse_unlaunchable_account(&self, _pinned: Option<&Path>) -> Result<()> {
        Ok(())
    }

    /// The version of the resolved `binary`, read from where it is installed instead of by running
    /// it: a process start is exactly what the caller is trying to avoid, and on a machine whose
    /// endpoint security stalls every exec a `--version` probe costs seconds. `None` when this
    /// agent's launch does not depend on its version, or when the install does not tell it; the
    /// caller then asks the host to run the binary where the host does (`platform::shell::probe_version`).
    fn installed_version(&self, _binary: &str) -> Option<AgentVersion> {
        None
    }

    /// How the provider's CLI is installed and updated, which `crate::agents::cli` reads to tell
    /// an install's method and to run its update. A provider that does not say gets
    /// [`CliPackaging::NONE`]: its CLI's install is not recognised and no update of it is offered.
    fn cli_packaging(&self) -> &'static CliPackaging {
        &CliPackaging::NONE
    }

    /// The program that starts the provider's CLI in place of the resolved `binary`, when `binary`
    /// is a launcher the CLI should not be started through. `None` starts `binary` itself; an
    /// error is a launcher whose native program cannot be found.
    fn native_binary(&self, _binary: &ResolvedBinary) -> Result<Option<ResolvedBinary>> {
        Ok(None)
    }

    /// The task title a raw terminal title carries, or `None` when it carries none: agents decorate
    /// their titles with status glyphs, activity and their own or the project's name.
    /// `project_names` are the names the project may appear under, given only to an agent that
    /// `names_project`. An agent nobody has taught the shape of gets the title as sent.
    fn task_title(&self, raw: &str, _project_names: &[String]) -> Option<String> {
        let title = raw.trim();
        (!title.is_empty()).then(|| title.to_owned())
    }

    /// Whether the agent's terminal title carries the project's name, which `task_title` has to
    /// know to take it off.
    fn names_project(&self) -> bool {
        false
    }

    /// The title one line of the conversation's own append-only record names it with, if it is
    /// such a line.
    fn named_title(&self, _line: &str) -> Option<NamedTitle> {
        None
    }

    /// The conversation and its title named by one line of a file shared by all of an account's
    /// conversations, if it is such a line.
    fn indexed_title(&self, _line: &str) -> Option<(String, String)> {
        None
    }

    /// The prompt the user typed, when one line of the conversation's own record is it. Only for an
    /// agent that can leave a conversation without a title of its own to fall back on.
    fn prompt_text(&self, _line: &str) -> Option<String> {
        None
    }

    /// The title a conversation's summary file names it with.
    fn summary_title(&self, _summary: &serde_json::Value) -> Option<String> {
        None
    }
}

/// The behavior registry: each registered provider's behavior, by id. Registering a provider is
/// one line here and one in `crate::providers`' descriptor registry; a test checks the two list
/// the same providers.
static PROVIDERS: &[(AgentId, &dyn AgentProvider)] = &[
    (AgentId::CLAUDE, &claude::ClaudeProvider),
    (AgentId::CODEX, &codex::CodexProvider),
    (AgentId::GROK, &grok::GrokProvider),
    #[cfg(test)]
    (AgentId::TEST, &test_provider::TestProvider),
];

/// `agent`'s behavior.
pub fn provider(agent: AgentId) -> &'static dyn AgentProvider {
    PROVIDERS
        .iter()
        .find(|(id, _)| *id == agent)
        .map(|(_, provider)| *provider)
        .expect("every registered provider has a behavior")
}

/// What starts `agent`'s CLI: the binary its name resolves to on `shell_env`'s `PATH`, or the
/// native program the provider starts in place of it ([`AgentProvider::native_binary`]).
pub fn resolve_binary(
    agent: AgentId,
    shell_env: &HashMap<String, String>,
) -> Result<ResolvedBinary> {
    let binary = crate::env_shell::resolve_binary(agent.descriptor().binary, shell_env)?;
    starting_program(agent, binary)
}

/// What starts `agent`'s CLI where its binary resolved to `binary`: `binary` itself, or the native
/// program the provider starts in place of it, or the provider's coded refusal of a launcher whose
/// native program is missing.
pub(crate) fn starting_program(agent: AgentId, binary: ResolvedBinary) -> Result<ResolvedBinary> {
    Ok(provider(agent).native_binary(&binary)?.unwrap_or(binary))
}

/// The compatibility sources `agent` reads under `settings`: one for each of its compatibility
/// switches (`SettingDescriptor::compatibility_with`) that is on, keyed by the agent it names;
/// none while none is.
pub fn compatibility_sources_of(
    agent: AgentId,
    settings: &ProviderSettings,
) -> Vec<(AgentId, &'static dyn CompatibilitySource)> {
    agent
        .descriptor()
        .settings
        .iter()
        .filter(|setting| settings.switch(setting.id))
        .filter_map(|setting| {
            let source = AgentId::from_wire_name(setting.compatibility_with?)?;
            Some((source, provider(source).compatibility_source()?))
        })
        .collect()
}

/// The facts of how a provider's CLI is installed and updated, from which an install's method is
/// told by where the resolved binary lies and the command that updates it is chosen
/// (`crate::agents::cli`).
#[derive(Debug)]
pub struct CliPackaging {
    /// Directory names, outermost first, that mark the layout the CLI's own installer leaves, each
    /// layout a run of consecutive components of the binary's path once its links are followed,
    /// matched ignoring case (`.local/share/claude/versions`).
    pub native_layouts: &'static [&'static [&'static str]],
    /// The arguments that make a CLI of that layout update itself, after its binary; empty where
    /// it does not.
    pub native_update: &'static [&'static str],
    /// How a CLI of that layout reports the latest version itself, when it can.
    pub native_latest: Option<NativeLatest>,
    /// The CLI's package on the npm registry, which names what an npm install is updated to and
    /// what the latest version is read from.
    pub npm_package: Option<&'static str>,
    /// The Homebrew formula and the cask that install the CLI, each when one exists.
    pub homebrew_formula: Option<&'static str>,
    pub homebrew_cask: Option<&'static str>,
}

/// A CLI's own check for the latest version: a command whose stdout is a JSON object holding it.
#[derive(Debug)]
pub struct NativeLatest {
    /// The arguments after the CLI's binary; the command only checks and changes nothing.
    pub args: &'static [&'static str],
    /// The field of the JSON object that holds the version.
    pub version_field: &'static str,
}

impl CliPackaging {
    /// A provider whose install is not recognised and whose CLI is not updated.
    pub const NONE: CliPackaging = CliPackaging {
        native_layouts: &[],
        native_update: &[],
        native_latest: None,
        npm_package: None,
        homebrew_formula: None,
        homebrew_cask: None,
    };

    /// Whether this says nothing about how the CLI is installed.
    pub fn is_none(&self) -> bool {
        self.native_layouts.is_empty()
            && self.npm_package.is_none()
            && self.homebrew_formula.is_none()
            && self.homebrew_cask.is_none()
    }
}

/// An agent's version as major, minor, patch.
pub type AgentVersion = (u32, u32, u32);

/// The version a binary's `--version` output names: the first word that is a version, with or
/// without a leading `v` (`2.1.318 (Claude Code)`, `codex-cli 0.166.0`).
pub fn version_from_output(text: &str) -> Option<AgentVersion> {
    text.split_whitespace()
        .find_map(|word| parse_version(word.strip_prefix('v').unwrap_or(word)))
}

/// Reads a version that is exactly `major.minor.patch` and nothing else.
fn parse_version(text: &str) -> Option<AgentVersion> {
    let mut parts = text.split('.');
    let version = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(version)
}

#[cfg(test)]
pub mod tests {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    use super::LaunchSpec;
    use crate::protocol::TerminalAppearance;
    use crate::session_tools::Owner;

    /// The file cases [`agent_file`] makes on this host: a FIFO only where there are FIFOs.
    pub(crate) const FILE_CASES: &[&str] = &[
        "regular",
        "linked",
        #[cfg(unix)]
        "fifo",
        "oversized",
    ];

    /// Runs each file case in a killable process: a regressed FIFO open must not strand a test
    /// thread. The parent owns the temporary root, including fixtures a killed child cannot drop.
    pub(crate) fn bounded_file_cases(name: &str, cases: &[&str], check: impl Fn(&str)) {
        const CASE: &str = "OCTOBOARDD_TEST_FILE_CASE";
        const DONE: &str = "OCTOBOARDD_TEST_FILE_DONE";
        if let Ok(case) = std::env::var(CASE) {
            check(&case);
            std::fs::write(std::env::var_os(DONE).unwrap(), "done").unwrap();
            return;
        }
        let mut failures = Vec::new();
        for case in cases {
            let root = crate::test_support::ScratchDir::new("bounded-file-case");
            let done = root.join("done");
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", name, "--nocapture", "--test-threads=1"])
                .env(CASE, case)
                .env(DONE, &done)
                .env("TMPDIR", &*root);
            let bounds = crate::processes::subprocess::Bounds {
                timeout: crate::test_support::PATIENCE,
                stdout_limit: 64 * 1024,
                stderr_limit: 64 * 1024,
                reach: crate::platform::proc::Reach::Group,
                term_grace: std::time::Duration::ZERO,
            };
            match crate::processes::subprocess::run_bounded(
                &mut command,
                &bounds,
                &std::sync::atomic::AtomicBool::new(false),
            ) {
                Ok(output) if output.status.success() && done.is_file() => {}
                Ok(output) => failures.push(format!(
                    "{case}: {}\n{}\n{}",
                    output.status,
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                )),
                Err(error) => failures.push(format!("{case}: {error}")),
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// Replaces a fixture with readable text, a regular symlink, a FIFO or valid oversized text.
    pub(crate) fn agent_file(path: &std::path::Path, case: &str, text: &str, limit: usize) {
        std::fs::remove_file(path).ok();
        match case {
            #[cfg(unix)]
            "fifo" => crate::test_support::mkfifo(path),
            "oversized" => std::fs::write(path, format!("{text}{}", " ".repeat(limit))).unwrap(),
            "linked" => {
                let target = path.with_extension("target");
                std::fs::write(&target, text).unwrap();
                crate::test_support::symlink(target, path).unwrap();
            }
            "regular" => std::fs::write(path, text).unwrap(),
            _ => panic!("unknown file case {case}"),
        }
    }

    /// A launch to plan against, with a real temporary directory standing in for the session's
    /// scratch space — the Grok adapter writes into it, so it cannot be a fiction.
    pub struct SpecFixture {
        pub session_id: String,
        pub kind: crate::protocol::SessionKind,
        pub owner: Option<Owner>,
        pub git_workflow: Option<crate::protocol::GitWorkflowSettings>,
        pub ask_questions_as_choices: bool,
        pub user_language: Option<String>,
        pub new_agent_session_id: String,
        pub cwd: PathBuf,
        pub scratch: crate::test_support::ScratchDir,
        pub hook_script: PathBuf,
        pub shell_env: HashMap<String, String>,
        pub config_dir: Option<PathBuf>,
        pub settings: crate::providers::ProviderSettings,
        pub compatibility_sources: Vec<(
            crate::protocol::AgentId,
            &'static dyn super::CompatibilitySource,
        )>,
        pub shared_default_dir: Option<PathBuf>,
        pub model: Option<String>,
        pub effort: Option<String>,
        pub terminal_appearance: Option<TerminalAppearance>,
        pub agent_version: Option<super::AgentVersion>,
        pub self_exe: String,
        pub mcp_token: String,
    }

    impl SpecFixture {
        pub fn spec(&self) -> LaunchSpec<'_> {
            LaunchSpec {
                session_id: &self.session_id,
                kind: self.kind,
                owner: self.owner.clone(),
                git_workflow: self.git_workflow,
                ask_questions_as_choices: self.ask_questions_as_choices,
                user_language: self.user_language.as_deref(),
                new_agent_session_id: &self.new_agent_session_id,
                resume_agent_session_id: None,
                cwd: &self.cwd,
                task: None,
                scratch: &self.scratch,
                hook_script: &self.hook_script,
                shell_env: &self.shell_env,
                config_dir: self.config_dir.as_deref(),
                settings: &self.settings,
                compatibility_sources: self.compatibility_sources.clone(),
                shared_default_dir: self.shared_default_dir.as_deref(),
                model: self.model.as_deref(),
                effort: self.effort.as_deref(),
                terminal_appearance: self.terminal_appearance,
                agent_version: self.agent_version,
                self_exe: &self.self_exe,
                daemon_port: 4321,
                daemon_key: "feedface",
                mcp_token: &self.mcp_token,
            }
        }
    }

    /// Whether a Codex launch for the account whose directory is `config_dir` (`None` for the
    /// default account), resumed when `resume` names a thread, keeps MCP OAuth tokens in the
    /// account's own home: the overrides `-c mcp_oauth_credentials_store="file"` adds to its plan,
    /// counted. The default account's home is a scratch one, so no real Codex home is read.
    pub fn codex_file_credential_overrides(
        config_dir: Option<&Path>,
        resume: Option<&str>,
    ) -> usize {
        let mut fixture = spec_fixture();
        let default_home = fixture.scratch.join("default-codex-home");
        std::fs::create_dir_all(&default_home).expect("default codex home");
        fixture.shell_env.insert(
            "CODEX_HOME".to_string(),
            default_home.to_string_lossy().into_owned(),
        );
        fixture.config_dir = config_dir.map(Path::to_path_buf);
        let mut spec = fixture.spec();
        spec.resume_agent_session_id = resume;
        let args = super::provider(crate::protocol::AgentId::CODEX)
            .plan(&spec)
            .expect("a plan")
            .args;
        args.windows(2)
            .filter(|pair| pair[0] == "-c" && pair[1] == super::codex::MCP_FILE_CREDENTIALS_STORE)
            .count()
    }

    pub fn spec_fixture() -> SpecFixture {
        let root = crate::test_support::ScratchDir::new("adapter-test");
        let scratch = root.join("scratch");
        let cwd = root.join("project");
        std::fs::create_dir_all(&scratch).expect("scratch directory");
        std::fs::create_dir_all(&cwd).expect("project directory");
        let hook_script = scratch.join("hook");
        std::fs::write(&hook_script, "#!/bin/sh\nexit 0\n").expect("hook script");

        SpecFixture {
            session_id: "session-1".to_string(),
            kind: crate::protocol::SessionKind::Project,
            owner: Some(Owner::Console),
            git_workflow: Some(crate::protocol::GitWorkflowSettings::default()),
            ask_questions_as_choices: true,
            user_language: None,
            new_agent_session_id: format!("agent-{}", std::process::id()),
            cwd,
            scratch: root,
            hook_script,
            shell_env: HashMap::new(),
            config_dir: None,
            settings: Default::default(),
            compatibility_sources: Vec::new(),
            shared_default_dir: None,
            model: None,
            effort: None,
            terminal_appearance: None,
            agent_version: None,
            self_exe: "/opt/octoboard/octoboardd".to_string(),
            mcp_token: "token-1".to_string(),
        }
    }

    /// Every registered provider's behavior is registered under the descriptor of its own agent.
    #[test]
    fn every_registered_provider_answers_with_its_own_descriptor() {
        for agent in crate::protocol::AgentId::ALL {
            let provider = super::provider(agent);
            assert!(std::ptr::eq(provider.descriptor(), agent.descriptor()));
        }
    }

    /// Every registered provider that keeps records reads them through a store of its own: each
    /// names the share of a full context window used, which the store of a provider without
    /// records (`NoRecords`) does not, and the test-only provider is the one that has none.
    #[test]
    fn every_registered_provider_reads_its_records_through_its_own_store() {
        use crate::records::RecordStore;
        assert_eq!(crate::records::NoRecords.used_percent(1_000, 1_000), None);
        for agent in crate::protocol::AgentId::ALL {
            let share = super::provider(agent)
                .records()
                .used_percent(1_000_000, 1_000_000);
            if agent == crate::protocol::AgentId::TEST {
                assert_eq!(share, None, "{agent:?}");
            } else {
                assert_eq!(share, Some(100), "{agent:?}");
            }
        }
    }

    /// A provider reads a compatibility source only through a switch of its descriptor that names
    /// one: with none on, or none declared (the test-only provider declares none), it reads none.
    #[test]
    fn a_compatibility_source_is_read_only_through_a_switch_naming_one() {
        use crate::providers::{ProviderSettings, SettingValue};
        for agent in crate::protocol::AgentId::ALL {
            let none = ProviderSettings::default();
            assert!(
                super::compatibility_sources_of(agent, &none).is_empty(),
                "{agent:?}"
            );
            let all_on = ProviderSettings::new(
                agent
                    .descriptor()
                    .settings
                    .iter()
                    .map(|setting| (setting.id.to_string(), Some(SettingValue::Switch(true)))),
            );
            let declares = agent
                .descriptor()
                .settings
                .iter()
                .any(|setting| setting.compatibility_with.is_some());
            assert_eq!(
                !super::compatibility_sources_of(agent, &all_on).is_empty(),
                declares,
                "{agent:?}"
            );
        }
        let test = crate::protocol::AgentId::TEST;
        assert!(!test.descriptor().compatibility_source);
        assert!(super::provider(test).compatibility_source().is_none());
    }

    /// A provider with only a launch plan launches with nothing of the optional kinds: no carried
    /// trust, no work after its exit, no notice, no status line and no hook script.
    #[test]
    fn the_test_provider_launches_without_any_optional_part() {
        let fixture = spec_fixture();
        let test = super::provider(crate::protocol::AgentId::TEST);
        let plan = test.plan(&fixture.spec()).expect("a plan");
        assert!(plan.args.is_empty() && plan.env.is_empty());
        assert!(plan.carried_trust.is_none());
        assert!(plan.after_exit.is_none());
        assert!(plan.notice.is_none());
        assert!(!plan.resolves_approvals_itself);
        assert!(test.status_line().is_none());
        assert!(test.accounts().is_none());
        assert!(test
            .refuse_unlaunchable_account(Some(Path::new("/nowhere")))
            .is_ok());
        assert_eq!(test.installed_version("octoboard-test-agent"), None);
        assert_eq!(
            test.task_title("  a title ", &[]).as_deref(),
            Some("a title")
        );
        assert_eq!(test.task_title("   ", &[]), None);
    }

    /// The two registries list the same providers in the same order.
    #[test]
    fn the_descriptor_and_behavior_registries_agree() {
        let behaviors: Vec<_> = super::PROVIDERS.iter().map(|(agent, _)| *agent).collect();
        assert_eq!(behaviors, crate::protocol::AgentId::ALL);
        let descriptors: Vec<_> = crate::providers::descriptors().map(|d| d.id).collect();
        let ids: Vec<_> = crate::protocol::AgentId::ALL
            .iter()
            .map(|a| a.wire_name())
            .collect();
        assert_eq!(descriptors, ids);
    }

    /// The child starts its catalogue from its own arguments, so a worker has to say so there, and
    /// a coordinator, with a console parent or without a parent, must not; after launch it follows
    /// the daemon's answers.
    #[test]
    fn the_mcp_child_is_told_whether_the_session_is_a_worker() {
        let mut fixture = spec_fixture();
        let (_, owned) = super::mcp_server_command(&fixture.spec());
        assert!(owned.contains(&"--worker".to_string()));
        // The child authenticates to the daemon with the key.
        let key = owned.iter().position(|arg| arg == "--key").expect("--key");
        assert_eq!(owned[key + 1], "feedface");
        fixture.owner = Some(Owner::ConsoleOfCoordinator);
        let (_, coordinator) = super::mcp_server_command(&fixture.spec());
        assert!(!coordinator.contains(&"--worker".to_string()));
        fixture.owner = None;
        let (_, independent) = super::mcp_server_command(&fixture.spec());
        assert!(!independent.contains(&"--worker".to_string()));
    }

    #[test]
    fn a_version_is_the_first_word_of_the_output_that_is_one() {
        for (text, version) in [
            ("2.1.318 (Claude Code)\n", Some((2, 1, 318))),
            ("codex-cli 0.166.0\r\n", Some((0, 166, 0))),
            ("grok v1.2.3", Some((1, 2, 3))),
            ("tool 1.2 beta", None),
            ("", None),
        ] {
            assert_eq!(super::version_from_output(text), version, "{text:?}");
        }
    }
}
