//! The provider registry: one static [`ProviderDescriptor`] per agent provider, holding the facts
//! about it that do not depend on running it — its names, where to install it, its binary, its
//! on-disk layout, how a project marks itself as set up for it, the names it gives its instruction
//! file and Octoboard's tools, and the capabilities it declares. It sits below the wire layer so
//! every layer can read a provider's facts and tell a provider by its id; how a provider behaves
//! is its `crate::adapter::AgentProvider`, registered in the agent sessions layer.
//!
//! Each provider's descriptor is in its own file. Code outside them reads a provider's facts from
//! its descriptor and branches on the capabilities it declares, never on which provider it is.
//!
//! [`AgentId`] names a registered provider; [`AgentId::ALL`] is the registry's order.

pub(crate) mod antigravity;
pub(crate) mod claude;
pub(crate) mod codex;
pub(crate) mod copilot;
pub(crate) mod grok;
pub(crate) mod layout;
pub(crate) mod opencode;
pub(crate) mod pi;
pub(crate) mod qwen;
#[cfg(test)]
pub(crate) mod test_provider;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use layout::AgentLayout;

use crate::platform::term::QuitKeys;

/// Every registered provider's descriptor, in the registry's order. The behavior registry
/// in `crate::adapter` registers the same providers.
const REGISTRY: &[&ProviderDescriptor] = &[
    &claude::DESCRIPTOR,
    &codex::DESCRIPTOR,
    &grok::DESCRIPTOR,
    &antigravity::DESCRIPTOR,
    &copilot::DESCRIPTOR,
    &opencode::DESCRIPTOR,
    &qwen::DESCRIPTOR,
    &pi::DESCRIPTOR,
    #[cfg(test)]
    &test_provider::DESCRIPTOR,
];

/// How many providers are registered.
pub const PROVIDER_COUNT: usize = REGISTRY.len();

/// An agent's version as major, minor, patch.
pub type AgentVersion = (u32, u32, u32);

/// A provider's id: the id of one of the registered providers, so it is only ever made from the
/// registry ([`AgentId::from_wire_name`], deserializing, or a provider's own constant). Ids that
/// name no registered provider are not `AgentId`s: the wire refuses them and the store keeps them
/// as text, leaving them out of what it loads.
///
/// Serialized as the id string, and generated for the clients as `Agent`, a plain `string`.
#[cfg_attr(
    test,
    derive(ts_rs::TS, schemars::JsonSchema),
    ts(rename = "Agent"),
    schemars(rename = "Agent")
)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct AgentId(&'static str);

impl AgentId {
    /// Every registered provider, in the registry's order.
    pub const ALL: [AgentId; PROVIDER_COUNT] = {
        let mut all = [AgentId(""); PROVIDER_COUNT];
        let mut index = 0;
        while index < PROVIDER_COUNT {
            all[index] = AgentId(REGISTRY[index].id);
            index += 1;
        }
        all
    };

    /// The provider's place in the registry's order.
    pub const fn index(self) -> usize {
        let mut index = 0;
        while index < PROVIDER_COUNT {
            if same_text(REGISTRY[index].id, self.0) {
                return index;
            }
            index += 1;
        }
        panic!("an AgentId names a registered provider")
    }

    /// The provider's descriptor.
    pub fn descriptor(self) -> &'static ProviderDescriptor {
        #[cfg(test)]
        if let Some(variant) = test_provider::variant_of(self) {
            return variant;
        }
        REGISTRY[self.index()]
    }

    /// How the provider is named on the wire, in the store and in the console session's tools:
    /// its id.
    pub fn wire_name(self) -> &'static str {
        self.0
    }

    /// The registered provider [`Self::wire_name`] names, `None` for an id no registered provider
    /// has.
    pub fn from_wire_name(name: &str) -> Option<AgentId> {
        AgentId::ALL.into_iter().find(|agent| agent.0 == name)
    }

    /// How the provider is named in a message shown to the user.
    pub fn label(self) -> &'static str {
        self.descriptor().label
    }
}

/// Whether `a` and `b` are the same text, where `==` on `str` cannot be called.
const fn same_text(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut index = 0;
    while index < a.len() {
        if a[index] != b[index] {
            return false;
        }
        index += 1;
    }
    true
}

impl std::fmt::Debug for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl Serialize for AgentId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0)
    }
}

impl<'de> Deserialize<'de> for AgentId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = std::borrow::Cow::<'de, str>::deserialize(deserializer)?;
        AgentId::from_wire_name(&name)
            .ok_or_else(|| serde::de::Error::custom(format_args!("unknown provider `{name}`")))
    }
}

/// Every registered provider's descriptor, in the registry's order.
pub fn descriptors() -> impl Iterator<Item = &'static ProviderDescriptor> {
    REGISTRY.iter().copied()
}

/// The static facts of one provider.
#[derive(Debug)]
pub struct ProviderDescriptor {
    /// The provider's id, as the wire, the store and Octoboard's tools name it.
    pub id: &'static str,
    /// The provider's product name, in text the daemon writes itself.
    pub label: &'static str,
    /// The shorter form of [`Self::label`], where the text around it already says which product
    /// is meant.
    pub short_name: &'static str,
    /// Where the user is sent to install the provider's CLI.
    pub install_url: &'static str,
    /// The CLI's binary name, resolved against the snapshotted shell `PATH`.
    pub binary: &'static str,
    /// Where the provider keeps its files, its configuration directory included.
    pub layout: &'static AgentLayout,
    /// The names at the top of a project that mark it as set up for this provider.
    pub detection: DetectionMarkers,
    /// Session marker variables removed from an inherited login-shell environment.
    /// Enumerate measured markers; user configuration and authentication must survive.
    pub inherited_session_vars: &'static [&'static str],
    /// What the CLI was measured to set, for the commands it runs, under names a user also sets:
    /// each name with the exact value. A daemon started from such a command would take the value
    /// for the user's, and a name like these cannot be filtered: the user's own setting would go
    /// with it. So a launch records each as it records what it sets itself
    /// (`crate::env_shell::session_environment_with`), and an inherited variable holding exactly
    /// this value is put back as the launch environment had it, while any other value is kept.
    /// Only values that are the same in every session belong here; a variable of the CLI's own
    /// that marks its session, or whose value varies, goes in
    /// [`Self::inherited_session_vars`]. The measurements are in `docs/agent-cli-reference.md`,
    /// "Whether a command the agent runs sees the launch's variables unchanged".
    pub command_environment: &'static [(&'static str, &'static str)],
    /// The instruction file read from the working directory, matched by exact spelling.
    pub instruction_file: &'static str,
    /// What a line of [`Self::instruction_file`] starts with, followed by the path of a file beside
    /// it, for the provider to read that file's text in the line's place; `None` where the provider
    /// is not known to read such a line.
    pub instruction_import_prefix: Option<&'static str>,
    /// What the provider puts before the server's name when it presents an MCP server's tool to
    /// its model.
    pub tool_name_prefix: &'static str,
    /// What the provider puts between the server's name and the tool's: the model sees
    /// `<prefix><server><separator><tool>`.
    pub tool_name_separator: &'static str,
    /// The provider's own tool for asking the user a question with options to pick from, when it
    /// has one whose name is known.
    pub ask_question_tool: Option<&'static str>,
    pub capabilities: Capabilities,
    /// The keys the provider's CLI quits on, for a host whose own request to end would end it
    /// without its shutdown work (the hook that fires on a clean teardown, the turn it is in): a
    /// stop presses them first. `None` for a CLI that finishes on the host's request.
    pub quit_keys: Option<QuitKeys>,
    /// How the provider's own interface names its quota windows.
    pub quota_windows: QuotaWindowNaming,
    /// How a model id of the provider is shown.
    pub model_display: ModelDisplay,
    /// Whether other providers can be made compatible with this one, reading its configuration
    /// (MCP servers, instructions) the way it does.
    pub compatibility_source: bool,
    /// Whether the agent keeps a memory of its own across sessions (the notes it keeps and reads
    /// back by itself, apart from the instruction files), and if so whether one launch can turn it
    /// off. Where it can, the provider's launch turns it off when
    /// [`crate::adapter::LaunchSpec::native_memory_off`] says so, which generic code sets for every
    /// session (`crate::adapter::turns_native_memory_off`). Served to clients, which name the
    /// agents whose memory is turned off and those whose memory is left as the user set it.
    pub native_memory: NativeMemory,
    /// The settings the user can make for the provider, in the order a client lists them. The
    /// provider reads its own at launch (`crate::adapter::LaunchSpec::settings`).
    pub settings: &'static [SettingDescriptor],
    /// The codes the provider's own failures and notices carry, each namespaced
    /// `provider.<id>.<code>`. Each is a `pub const` of the provider's file.
    pub codes: &'static [ProviderCode],
}

/// Whether a provider code names a failure of a request or a notice of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderCodeKind {
    /// Carried by an `error` reply or event, like `crate::protocol::error_code`.
    Error,
    /// Carried by a session notice, like `crate::protocol::notice_code`.
    Notice,
}

/// A code only one provider raises, declared by its descriptor rather than in
/// `crate::protocol::codes`. Core code makes it a wire `Code` through `Code::of_provider`. A UI
/// test reads each declaration from the provider's file by its shape (`pub const NAME: ProviderCode
/// = ProviderCode::<kind>("provider.<id>.<code>");`, which rustfmt may wrap after the `=`), so a
/// code is never made any other way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderCode {
    pub(crate) code: &'static str,
    pub kind: ProviderCodeKind,
}

impl ProviderCode {
    /// A code of an `error`.
    pub const fn error(code: &'static str) -> Self {
        Self {
            code,
            kind: ProviderCodeKind::Error,
        }
    }

    /// A code of a session notice.
    pub const fn notice(code: &'static str) -> Self {
        Self {
            code,
            kind: ProviderCodeKind::Notice,
        }
    }

    /// The code as it is on the wire.
    pub const fn as_str(self) -> &'static str {
        self.code
    }
}

impl ProviderDescriptor {
    /// Whether `tool` is the provider's own ask-the-user tool, a question waiting on a person.
    pub fn is_ask_question_tool(&self, tool: Option<&str>) -> bool {
        tool.is_some() && tool == self.ask_question_tool
    }

    /// Whether this setting can be held by a launch level.
    pub fn is_overridable(&self, id: &str) -> bool {
        self.setting(id).is_some_and(SettingDescriptor::overridable)
    }

    /// Held values limited to the compatibility switches this provider declares. Stored values
    /// from another build may name settings this build cannot use.
    pub fn known_compatibility(&self, mut held: BTreeMap<String, bool>) -> BTreeMap<String, bool> {
        held.retain(|setting, _| self.is_overridable(setting));
        held
    }

    /// The settings a launch level can hold, in descriptor order.
    pub fn overridable_settings(&self) -> impl Iterator<Item = &'static SettingDescriptor> + '_ {
        self.settings.iter().filter(|setting| setting.overridable())
    }

    /// The provider's setting with this id.
    pub fn setting(&self, id: &str) -> Option<&'static SettingDescriptor> {
        self.settings.iter().find(|setting| setting.id == id)
    }
}

/// Of `values`, by provider id and then setting id, the entries that name a setting a registered
/// provider declares and that `keep` accepts with its value; a provider left with none is left out.
/// What every view of the settings that only some of them reach (an export, an import, what agents
/// are shown) is cut down by, each with its own descriptor flag.
pub(crate) fn settings_where<V>(
    values: impl IntoIterator<Item = (String, BTreeMap<String, V>)>,
    keep: impl Fn(&SettingDescriptor, &V) -> bool,
) -> BTreeMap<String, BTreeMap<String, V>> {
    values
        .into_iter()
        .filter_map(|(provider, settings)| {
            let descriptor = AgentId::from_wire_name(&provider)?.descriptor();
            let kept: BTreeMap<String, V> = settings
                .into_iter()
                .filter(|(id, value)| descriptor.setting(id).is_some_and(|s| keep(s, value)))
                .collect();
            (!kept.is_empty()).then_some((provider, kept))
        })
        .collect()
}

/// One setting of a provider, declared by its descriptor. The values the user chose are kept
/// per provider and setting id, never as a field of their own.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SettingDescriptor {
    /// Unique within its provider; snake_case.
    pub id: &'static str,
    pub kind: SettingKind,
    /// Whether a configuration export carries the user's explicit choice. A setting whose
    /// enabling itself modifies files outside Octoboard's own directory stays out of one, for the
    /// user to turn on by hand; a setting that only becomes a launch flag travels.
    pub exported: bool,
    /// Whether the agents' `get_settings` tool returns the value in effect. Agents never write a
    /// provider setting.
    pub shown_to_agents: bool,
    /// The id of the provider whose configuration this switch makes the provider read, when it is
    /// such a switch. That provider declares itself a compatibility source.
    pub compatibility_with: Option<&'static str>,
}

/// What values a setting takes.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SettingKind {
    /// One of the choices; unset leaves configuration and resume behavior to the native agent.
    Choice { choices: &'static [SettingChoice] },
    /// On or off, by the user's choice or else by its default.
    Switch { default: SwitchDefault },
}

/// One choice of a [`SettingKind::Choice`] setting.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SettingChoice {
    pub value: &'static str,
    /// Whether a client asks the user to confirm before saving this choice. The daemon does not
    /// enforce it.
    pub confirm: bool,
}

/// What a switch is while the user has made no choice for it.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SwitchDefault {
    On,
    Off,
}

/// The value of a provider setting: a switch's on or off, or the chosen choice.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SettingValue {
    Switch(bool),
    Choice(String),
}

impl SettingDescriptor {
    /// Whether a console, a project or a session can hold its own value for the setting in its
    /// `Launch::compatibility`, which then wins over the application-wide one for the sessions it
    /// covers. The one rule everything that stores, validates, shows or exports such a value asks:
    /// the compatibility switches.
    pub fn overridable(&self) -> bool {
        self.compatibility_with.is_some()
    }

    /// Whether `value` is one this setting takes.
    pub fn accepts(&self, value: &SettingValue) -> bool {
        match (&self.kind, value) {
            (SettingKind::Switch { .. }, SettingValue::Switch(_)) => true,
            (SettingKind::Choice { choices }, SettingValue::Choice(chosen)) => {
                choices.iter().any(|choice| choice.value == chosen)
            }
            _ => false,
        }
    }

    /// The value in effect given the user's `chosen` one, if they made one this setting still
    /// takes: a switch falls back to its default, a choice to unset.
    pub fn in_effect(&self, chosen: Option<&SettingValue>) -> Option<SettingValue> {
        if let Some(value) = chosen.filter(|value| self.accepts(value)) {
            return Some(value.clone());
        }
        match self.kind {
            SettingKind::Choice { .. } => None,
            SettingKind::Switch { default } => Some(SettingValue::Switch(match default {
                SwitchDefault::On => true,
                SwitchDefault::Off => false,
            })),
        }
    }
}

/// One provider's settings in effect, as its launch reads them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderSettings(BTreeMap<String, SettingValue>);

impl ProviderSettings {
    /// The settings in effect, an unset choice left out.
    pub fn new(values: impl IntoIterator<Item = (String, Option<SettingValue>)>) -> Self {
        Self(
            values
                .into_iter()
                .filter_map(|(id, value)| Some((id, value?)))
                .collect(),
        )
    }

    /// These settings with the switches in `held` set to their value; an id that is not a switch
    /// of these settings is not read.
    pub fn with_switches(mut self, held: &BTreeMap<String, bool>) -> Self {
        for (id, value) in held {
            if let Some(known @ SettingValue::Switch(_)) = self.0.get_mut(id) {
                *known = SettingValue::Switch(*value);
            }
        }
        self
    }

    /// The chosen choice of the setting `id`; `None` while it is unset.
    pub fn choice(&self, id: &str) -> Option<&str> {
        match self.0.get(id) {
            Some(SettingValue::Choice(chosen)) => Some(chosen),
            _ => None,
        }
    }

    /// Whether the switch `id` is on.
    pub fn switch(&self, id: &str) -> bool {
        matches!(self.0.get(id), Some(SettingValue::Switch(true)))
    }
}

/// The names at a project's top level that point at a provider: its own, which point at it alone
/// (none for a provider no name points at alone), and generic ones it reads that other providers
/// may read as well. A generic name points at the
/// first registered provider that reads it, and at none while any provider reading it is marked by
/// its own names (`crate::agents::detect`).
#[derive(Debug)]
pub struct DetectionMarkers {
    pub own: &'static [&'static str],
    pub generic: &'static [&'static str],
}

/// How a provider's launch in a console's own directory comes to be trusted
/// (`Capabilities::console_trust`, `crate::session::trust`). A console's directory is Octoboard's
/// own, so trusting it is Octoboard's to do and nobody is asked; what differs is whether the agent
/// records it in the user's trust list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleTrust {
    /// The provider's launch plan trusts the console's directory for that launch alone, in a way
    /// measured to record nothing in the user's own trust list, so its confirmation does not come
    /// up for a console session. One that comes up all the same is answered like any other
    /// console's.
    PerLaunch,
    /// The provider has no such way: where its confirmation comes up for a console session,
    /// Octoboard answers it without asking, and the agent records the console's directory in its
    /// own trust list as it does for any answer.
    Answered,
}

/// What a provider can do that not every provider can, declared so that code and the UI branch
/// on it rather than on which provider it is.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Capabilities {
    /// Accepts an id for a new conversation, so Octoboard's session id can be the agent's;
    /// otherwise the agent's id becomes known from its first hook payload.
    pub preallocates_session_id: bool,
    /// Runs its hooks through the per-session hook script rather than an inline command.
    pub hook_script: bool,
    /// Shows a status line Octoboard can feed.
    pub status_line: bool,
    /// Offers credits that reset its usage limits early.
    pub reset_credits: bool,
    /// The URL scheme its desktop application registers, which is how the host is asked whether the
    /// application is installed; `Some` exactly when the provider can open a conversation in it
    /// (`AgentProvider::native_app`).
    pub native_app_scheme: Option<&'static str>,
    /// Keeps the trust its confirmation grants in a per-session home, from where Octoboard carries
    /// it to the user's own configuration.
    pub per_session_trust: bool,
    /// How a console's own directory comes to be trusted by the provider: the generic trust layer
    /// and the provider's own launch plan both follow it. Every descriptor sets it, from what was
    /// measured of the provider's CLI. Not sent to a client: nothing in a window follows it.
    #[serde(skip)]
    #[cfg_attr(test, ts(skip))]
    pub console_trust: ConsoleTrust,
    /// Its accounts can share the default account's configuration.
    pub config_sharing: bool,
    /// Its accounts beyond the default can each be given a directory of their own, the agent's
    /// configuration directory for that account. False for an agent that cannot be pointed at a
    /// second one without changing something every program it starts would inherit: it has the
    /// default account only, and the daemon refuses to add, import or point another.
    pub owned_accounts: bool,
    /// Can sign an account in without a terminal (`AccountOps::login`); otherwise the user signs
    /// in with the agent's own CLI, and a login request is refused.
    pub login_flow: bool,
    /// Can list an account's MCP servers and platform connectors (`read_account_mcp_inventory`).
    pub mcp_inventory: bool,
    /// Ends the process groups of the commands it still has running as it exits, so a process in
    /// such a group ends with the session whatever keeps it. Each command's group is led by a
    /// descendant of the process that runs the commands (the agent, or the server it runs them
    /// from, `crate::session::term::MessageTransport::command_host`), reached from the runner
    /// without another group leader in between.
    pub ends_command_groups_on_exit: bool,
    pub delivery: DeliveryQuirks,
    /// Which of the prompts its agent shows while a session waits on the user Octoboard publishes
    /// and answers (`Session.interaction`, `answer_interaction`;
    /// `AgentProvider::interactions`).
    pub interactions: InteractionKinds,
    /// Takes an image file as an attached image: images attached to a message (`send_message`'s
    /// `images`), and the path of an image saved for its session (`save_pasted_image`,
    /// `create_pasted_image_upload`), which a client pastes or drops into the session's terminal.
    /// Where it does not, each of those requests is refused.
    pub message_images: bool,
    /// Runs a slash command sent as one (`send_command`).
    pub slash_commands: bool,
}

/// The kinds of prompt a provider's sessions publish and answer.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct InteractionKinds {
    /// Tool permission prompts.
    pub permission: bool,
    /// Questions from the agent's own ask-the-user tool.
    pub question: bool,
}

impl InteractionKinds {
    /// Neither kind.
    pub const NONE: Self = Self {
        permission: false,
        question: false,
    };
    /// Both kinds.
    pub const ALL: Self = Self {
        permission: true,
        question: true,
    };
}

/// How writing a message into the provider's session differs from the plain case.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DeliveryQuirks {
    /// A write is confirmed only by the agent's report of the prompt it sent, and is marked unsent
    /// when no report comes.
    pub confirms_by_reported_prompt: bool,
    /// A report is held for the completion notice the agent sends after a turn.
    pub holds_report_for_completion_notice: bool,
    /// An instruction for a resumed conversation goes as the launch prompt, which keeps the agent
    /// from showing a resume dialog that would swallow it.
    pub resume_instruction_as_launch_prompt: bool,
}

/// Whether an agent keeps a memory of its own across sessions, and whether one launch can turn
/// it off without a change to the agent's or the user's configuration.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeMemory {
    /// The agent keeps no memory of its own.
    Absent,
    /// The agent keeps a memory of its own, which one launch can turn off.
    Switchable,
    /// The agent keeps a memory of its own, which no launch can turn off: it stays as the agent
    /// and the user set it.
    Unswitchable,
}

/// The kind of a quota window. Not every provider has every kind: Claude Code has `session`,
/// `weekly` and `weekly_model`, Codex `session` and `weekly`, Grok Build `credit_period`.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaWindowKind {
    /// The five-hour window.
    Session,
    /// The weekly window across all models.
    Weekly,
    /// A weekly window of one model, named by `label`.
    WeeklyModel,
    /// A pool of credits over a billing period.
    CreditPeriod,
}

impl QuotaWindowKind {
    /// How the kind is named on the wire, as it serializes.
    pub fn wire_name(self) -> &'static str {
        match self {
            QuotaWindowKind::Session => "session",
            QuotaWindowKind::Weekly => "weekly",
            QuotaWindowKind::WeeklyModel => "weekly_model",
            QuotaWindowKind::CreditPeriod => "credit_period",
        }
    }
}

/// What a provider's own interface calls the five-hour window.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionWindowName {
    /// "session".
    Session,
    /// "5h".
    FiveHour,
}

impl SessionWindowName {
    /// The name as it reads in a sentence.
    pub fn text(self) -> &'static str {
        match self {
            SessionWindowName::Session => "session",
            SessionWindowName::FiveHour => "5h",
        }
    }
}

/// A native name for a provider's credit pool.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CreditPeriodName {
    /// "Plan".
    Plan,
}

impl CreditPeriodName {
    pub fn text(self) -> &'static str {
        match self {
            Self::Plan => "Plan",
        }
    }
}

/// How the provider's own interface names its quota windows, which Octoboard's wording follows.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct QuotaWindowNaming {
    /// The kinds of window the provider reports at all. Empty: Octoboard reads no plan quota for
    /// the provider (`AccountOps::quota_probe` is `None`), and its accounts show none.
    pub kinds: &'static [QuotaWindowKind],
    /// What it calls the five-hour window.
    pub session_window: SessionWindowName,
    /// Whether an account-wide weekly window beside a model's own is named "Weekly · all models".
    pub weekly_all_models: bool,
    /// A native name for the credit pool. When absent, its period determines the name.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub credit_period: Option<CreditPeriodName>,
}

/// How a model id of the provider is shown.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ModelDisplay {
    /// Whether an id of the form `<prefix><family>-<major>-<minor>[-<patch>...][-<date>]` reads as
    /// "Family major.minor" (`claude-opus-5-5` as "Opus 5.5"), a bracketed suffix kept; any other
    /// id is shown as it is.
    pub family_version: bool,
    /// What such an id starts with, when only ids of one provider's spelling are read that way.
    pub id_prefix: Option<&'static str>,
    /// Aliases shown capitalized (`opus` as "Opus").
    pub aliases: &'static [&'static str],
    /// Stored spellings that read as no model at all (a retired alias).
    pub retired: &'static [&'static str],
}

impl ModelDisplay {
    /// A provider whose model ids are shown as they are.
    pub const PLAIN: ModelDisplay = ModelDisplay {
        family_version: false,
        id_prefix: None,
        aliases: &[],
        retired: &[],
    };
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    /// A provider has a status line exactly when its descriptor declares the capability, and the
    /// command its launch injects names the provider.
    #[test]
    fn the_status_line_capability_agrees_with_the_provider() {
        for agent in AgentId::ALL {
            assert_eq!(
                crate::adapter::provider(agent).status_line().is_some(),
                agent.descriptor().capabilities.status_line,
                "{}",
                agent.descriptor().id
            );
        }
    }

    /// A provider opens conversations in a desktop application exactly when its descriptor declares
    /// the scheme the application registers.
    #[test]
    fn the_native_app_capability_agrees_with_the_provider() {
        for agent in AgentId::ALL {
            assert_eq!(
                crate::adapter::provider(agent).native_app().is_some(),
                agent.descriptor().capabilities.native_app_scheme.is_some(),
                "{}",
                agent.descriptor().id
            );
        }
    }

    /// Claude Code, Codex and Grok Build declare that they end the process group of each command
    /// they still run as they exit; the test provider does not.
    #[test]
    fn the_real_agents_declare_that_they_end_command_groups() {
        for (agent, ends) in [
            (AgentId::CLAUDE, true),
            (AgentId::CODEX, true),
            (AgentId::GROK, true),
            (AgentId::TEST, false),
        ] {
            assert_eq!(
                agent.descriptor().capabilities.ends_command_groups_on_exit,
                ends,
                "{}",
                agent.descriptor().id
            );
        }
    }

    /// Claude Code, Codex and Grok Build give each account beyond the default a directory of its
    /// own, and so does the test provider unless a test takes the variant that does not; sharing the
    /// default account's configuration is something only such an account does.
    #[test]
    fn the_owned_accounts_capability_is_declared_where_accounts_have_directories() {
        for agent in [
            AgentId::CLAUDE,
            AgentId::CODEX,
            AgentId::GROK,
            AgentId::TEST,
        ] {
            assert!(
                agent.descriptor().capabilities.owned_accounts,
                "{}",
                agent.descriptor().id
            );
        }
        for descriptor in descriptors() {
            assert!(
                descriptor.capabilities.owned_accounts || !descriptor.capabilities.config_sharing,
                "{}: shares a configuration with no account of its own to share it",
                descriptor.id
            );
        }
        let _single = test_provider::without_owned_accounts();
        assert!(!AgentId::TEST.descriptor().capabilities.owned_accounts);
    }

    /// Every registered descriptor carries every fact a provider must have, and the registry's
    /// order, ids and lookups agree with each other.
    #[test]
    fn every_registered_descriptor_is_complete() {
        let mut ids = HashSet::new();
        for (agent, descriptor) in AgentId::ALL.into_iter().zip(descriptors()) {
            assert!(std::ptr::eq(agent.descriptor(), descriptor));
            assert!(
                ids.insert(descriptor.id),
                "{} registered twice",
                descriptor.id
            );
            assert_eq!(AgentId::from_wire_name(descriptor.id), Some(agent));
            assert_eq!(
                serde_json::to_value(agent).unwrap(),
                serde_json::json!(descriptor.id),
                "{}'s id is not its wire name",
                descriptor.id
            );
            for (field, value) in [
                ("id", descriptor.id),
                ("label", descriptor.label),
                ("short_name", descriptor.short_name),
                ("binary", descriptor.binary),
                ("instruction_file", descriptor.instruction_file),
                ("config_dir_var", descriptor.layout.config_dir_var),
                ("default_dir", descriptor.layout.default_dir),
            ] {
                assert!(
                    !value.trim().is_empty(),
                    "{}: {field} is blank",
                    descriptor.id
                );
            }
            assert!(
                descriptor.install_url.starts_with("https://"),
                "{}: install URL",
                descriptor.id
            );
            assert!(
                descriptor.label.contains(descriptor.short_name),
                "{}: the short name is not a form of the label",
                descriptor.id
            );
            if let Some(tool) = descriptor.ask_question_tool {
                assert!(!tool.trim().is_empty(), "{}: blank ask tool", descriptor.id);
            }
        }
        assert_eq!(ids.len(), AgentId::ALL.len());
    }

    #[test]
    fn a_providers_quota_naming_agrees_with_its_kinds() {
        for descriptor in descriptors() {
            let naming = descriptor.quota_windows;
            if naming.weekly_all_models {
                assert!(
                    naming.kinds.contains(&QuotaWindowKind::WeeklyModel),
                    "{}: names an all-models window without a model window",
                    descriptor.id
                );
            }
        }
    }

    /// Setting ids are unique within a provider, a choice setting offers distinct choices, and a
    /// compatibility switch names a registered source and is a switch.
    #[test]
    fn every_provider_setting_is_declared_consistently() {
        for descriptor in descriptors() {
            let mut ids = HashSet::new();
            for setting in descriptor.settings {
                let which = format!("{}.{}", descriptor.id, setting.id);
                assert!(
                    !setting.id.is_empty()
                        && setting
                            .id
                            .chars()
                            .all(|c| c.is_ascii_lowercase() || c == '_'),
                    "{which}: the id is not snake_case"
                );
                assert!(ids.insert(setting.id), "{which} declared twice");
                if let SettingKind::Choice { choices } = setting.kind {
                    assert!(!choices.is_empty(), "{which}: no choices");
                    let values: HashSet<_> = choices.iter().map(|c| c.value).collect();
                    assert_eq!(values.len(), choices.len(), "{which}: repeated choice");
                    assert!(
                        values.iter().all(|value| !value.is_empty()),
                        "{which}: blank choice"
                    );
                    assert!(
                        setting.compatibility_with.is_none(),
                        "{which}: a choice cannot be a compatibility switch"
                    );
                }
                if let Some(source) = setting.compatibility_with {
                    let source = AgentId::from_wire_name(source)
                        .unwrap_or_else(|| panic!("{which}: unregistered source {source}"));
                    assert!(
                        source.descriptor().compatibility_source,
                        "{which}: {} is no compatibility source",
                        source.descriptor().id
                    );
                    assert_ne!(
                        source.descriptor().id,
                        descriptor.id,
                        "{which}: reads itself"
                    );
                }
            }
        }
    }

    /// A provider offers a compatibility source exactly when its descriptor declares it.
    #[test]
    fn the_compatibility_source_capability_agrees_with_the_provider() {
        for agent in AgentId::ALL {
            assert_eq!(
                crate::adapter::provider(agent)
                    .compatibility_source()
                    .is_some(),
                agent.descriptor().compatibility_source,
                "{}",
                agent.descriptor().id
            );
        }
    }

    /// A switch with no choice of the user's follows its default; a value the setting does not
    /// take is not a choice at all.
    #[test]
    fn a_switch_with_no_choice_follows_its_default() {
        let compatibility =
            |agent: AgentId| agent.descriptor().setting("claude_compatibility").unwrap();
        let in_effect = |agent: AgentId, chosen: Option<SettingValue>| {
            compatibility(agent).in_effect(chosen.as_ref())
        };
        let on = Some(SettingValue::Switch(true));
        let off = Some(SettingValue::Switch(false));
        assert_eq!(in_effect(AgentId::GROK, None), off);
        assert_eq!(in_effect(AgentId::CODEX, None), off);
        assert_eq!(in_effect(AgentId::GROK, on.clone()), on);
        assert_eq!(in_effect(AgentId::CODEX, on.clone()), on);
        assert_eq!(in_effect(AgentId::GROK, off.clone()), off);
        let wrong = Some(SettingValue::Choice("on".into()));
        assert_eq!(in_effect(AgentId::GROK, wrong), off);

        let reviewer = AgentId::CODEX
            .descriptor()
            .setting("approval_reviewer")
            .unwrap();
        assert_eq!(reviewer.in_effect(None), None);
        let user = SettingValue::Choice("user".into());
        assert_eq!(reviewer.in_effect(Some(&user)), Some(user));
        assert!(!reviewer.accepts(&SettingValue::Choice("nobody".into())));
        assert!(!reviewer.accepts(&SettingValue::Switch(true)));
    }

    #[test]
    fn no_marker_of_a_providers_own_is_claimed_by_another() {
        let mut seen = HashSet::new();
        for descriptor in descriptors() {
            for marker in descriptor.detection.own {
                assert!(seen.insert(*marker), "{marker} is claimed twice");
            }
        }
        for descriptor in descriptors() {
            for marker in descriptor.detection.generic {
                assert!(!seen.contains(marker), "{marker} is both own and generic");
            }
        }
    }

    /// A provider's codes are namespaced by its id and none is another provider's or a core code.
    /// That each is listed in its descriptor and of the kind its use needs is checked beside it.
    #[test]
    fn every_provider_code_is_namespaced_and_unique() {
        let core = include_str!("../protocol/codes.rs");
        let mut seen = HashSet::new();
        for descriptor in descriptors() {
            let own = format!("provider.{}.", descriptor.id);
            for code in descriptor.codes {
                let wire = code.as_str();
                assert!(wire.starts_with(&own), "{wire} is not under {own}");
                assert!(wire.len() > own.len(), "{wire} has no name");
                assert!(seen.insert(wire), "{wire} declared twice");
                assert!(
                    !core.contains(&format!("Code(\"{wire}\")")),
                    "{wire} is also a core code"
                );
            }
        }
    }

    /// A provider's launch overrides are exactly the settings it declares as compatibility switches;
    /// each provider's own tests pin which those are.
    #[test]
    fn launch_overrides_accept_only_the_declared_compatibility_switches() {
        for agent in AgentId::ALL {
            let descriptor = agent.descriptor();
            let expected: Vec<_> = descriptor
                .settings
                .iter()
                .filter(|setting| setting.compatibility_with.is_some())
                .map(|setting| setting.id)
                .collect();
            let actual: Vec<_> = descriptor.overridable_settings().map(|s| s.id).collect();
            assert_eq!(actual, expected, "{}", agent.wire_name());
            for setting in descriptor.settings {
                assert_eq!(
                    descriptor.is_overridable(setting.id),
                    setting.compatibility_with.is_some()
                );
                assert_eq!(setting.overridable(), setting.compatibility_with.is_some());
                if matches!(setting.kind, SettingKind::Choice { .. }) {
                    assert!(!descriptor.is_overridable(setting.id));
                }
            }
            assert!(!descriptor.is_overridable("unknown_switch"));
        }
    }
}
