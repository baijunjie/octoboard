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

pub(crate) mod claude;
pub(crate) mod codex;
pub(crate) mod grok;
pub(crate) mod layout;
#[cfg(test)]
pub(crate) mod test_provider;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use layout::AgentLayout;

/// Every registered provider's descriptor, in the registry's order, which every per-provider
/// listing keeps. Registering a provider is one line here and one in `crate::adapter`'s registry.
const REGISTRY: &[&ProviderDescriptor] = &[
    &claude::DESCRIPTOR,
    &codex::DESCRIPTOR,
    &grok::DESCRIPTOR,
    #[cfg(test)]
    &test_provider::DESCRIPTOR,
];

/// How many providers are registered.
pub const PROVIDER_COUNT: usize = REGISTRY.len();

/// A provider's id: the id of one of the registered providers, so it is only ever made from the
/// registry ([`AgentId::from_wire_name`], deserializing, or a provider's own constant). Ids that
/// name no registered provider are not `AgentId`s: the wire refuses them and the store keeps them
/// as text, leaving them out of what it loads.
///
/// Serialized as the id string, and generated for the clients as `Agent`, a plain `string`.
#[cfg_attr(test, derive(ts_rs::TS), ts(rename = "Agent"))]
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
    pub const fn descriptor(self) -> &'static ProviderDescriptor {
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
    /// The instruction file the provider reads out of its working directory, matched by exact
    /// spelling (see "Injecting Octoboard into each agent" in `docs/agent-cli-reference.md`).
    pub instruction_file: &'static str,
    /// What the provider puts before `<server>__<tool>` when it presents an MCP server's tool to
    /// its model.
    pub tool_name_prefix: &'static str,
    /// The provider's own tool for asking the user a question with options to pick from, when it
    /// has one whose name is known.
    pub ask_question_tool: Option<&'static str>,
    pub capabilities: Capabilities,
    /// How the provider's own interface names its quota windows.
    pub quota_windows: QuotaWindowNaming,
    /// How a model id of the provider is shown.
    pub model_display: ModelDisplay,
    /// Whether other providers can be made compatible with this one, reading its configuration
    /// (MCP servers, instructions) the way it does.
    pub compatibility_source: bool,
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
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SettingDescriptor {
    /// Unique within its provider; snake_case.
    pub id: &'static str,
    pub kind: SettingKind,
    /// Whether a configuration export carries the user's explicit choice. A setting that lets
    /// agents act without asking stays out of one.
    pub exported: bool,
    /// Whether the agents' `get_settings` tool returns the value in effect. Agents never write a
    /// provider setting.
    pub shown_to_agents: bool,
    /// The id of the provider whose configuration this switch makes the provider read, when it is
    /// such a switch. That provider declares itself a compatibility source.
    pub compatibility_with: Option<&'static str>,
}

/// What values a setting takes.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SettingKind {
    /// One of the choices; unset follows each account's own configuration.
    Choice { choices: &'static [SettingChoice] },
    /// On or off, by the user's choice or else by its default rule.
    Switch { default: SwitchDefault },
}

/// One choice of a [`SettingKind::Choice`] setting.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SettingChoice {
    pub value: &'static str,
    /// Whether a client asks the user to confirm before saving this choice. The daemon does not
    /// enforce it.
    pub confirm: bool,
}

/// What a switch is while the user has made no choice for it.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SwitchDefault {
    On,
    Off,
    /// On while the provider with this id is among the added agents; the undecided list counts as
    /// not containing it.
    WhileAdded(&'static str),
}

/// The value of a provider setting: a switch's on or off, or the chosen choice.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SettingValue {
    Switch(bool),
    Choice(String),
}

impl SettingDescriptor {
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
    /// takes, and the `added` agents (`None` while undecided): a switch falls back to its default
    /// rule, a choice to unset.
    pub fn in_effect(
        &self,
        chosen: Option<&SettingValue>,
        added: Option<&[AgentId]>,
    ) -> Option<SettingValue> {
        if let Some(value) = chosen.filter(|value| self.accepts(value)) {
            return Some(value.clone());
        }
        match self.kind {
            SettingKind::Choice { .. } => None,
            SettingKind::Switch { default } => Some(SettingValue::Switch(match default {
                SwitchDefault::On => true,
                SwitchDefault::Off => false,
                SwitchDefault::WhileAdded(id) => AgentId::from_wire_name(id)
                    .is_some_and(|agent| added.is_some_and(|added| added.contains(&agent))),
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

/// The names at a project's top level that point at a provider: its own, which point at it alone,
/// and generic ones it reads that other providers may read as well. A generic name points at the
/// first registered provider that reads it, and at none while any provider reading it is marked by
/// its own names (`crate::agents::detect`).
#[derive(Debug)]
pub struct DetectionMarkers {
    pub own: &'static [&'static str],
    pub generic: &'static [&'static str],
}

/// What a provider can do that not every provider can, declared so that code and the UI branch
/// on it rather than on which provider it is.
#[cfg_attr(test, derive(ts_rs::TS))]
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
    /// Keeps the trust its confirmation grants in a per-session home, from where Octoboard carries
    /// it to the user's own configuration.
    pub per_session_trust: bool,
    /// Its accounts can share the default account's configuration.
    pub config_sharing: bool,
    pub delivery: DeliveryQuirks,
}

/// How writing a message into the provider's session differs from the plain case.
#[cfg_attr(test, derive(ts_rs::TS))]
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

/// The kind of a quota window. Not every provider has every kind: Claude Code has `session`,
/// `weekly` and `weekly_model`, Codex `session` and `weekly`, Grok Build `credit_period`.
#[cfg_attr(test, derive(ts_rs::TS))]
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
#[cfg_attr(test, derive(ts_rs::TS))]
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

/// How the provider's own interface names its quota windows, which Octoboard's wording follows.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct QuotaWindowNaming {
    /// The kinds of window the provider reports at all.
    pub kinds: &'static [QuotaWindowKind],
    /// What it calls the five-hour window.
    pub session_window: SessionWindowName,
    /// Whether an account-wide weekly window beside a model's own is named "Weekly · all models".
    pub weekly_all_models: bool,
}

/// How a model id of the provider is shown.
#[cfg_attr(test, derive(ts_rs::TS))]
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
                ("records_dir", descriptor.layout.records_dir),
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
            assert!(
                !descriptor.detection.own.is_empty(),
                "{}: no detection marker of its own",
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
            assert!(
                !naming.kinds.is_empty(),
                "{}: reports no quota window",
                descriptor.id
            );
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
    /// compatibility switch names a registered source and is a switch whose default rule names a
    /// registered provider.
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
                match setting.kind {
                    SettingKind::Choice { choices } => {
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
                    SettingKind::Switch { default } => {
                        if let SwitchDefault::WhileAdded(id) = default {
                            assert!(
                                AgentId::from_wire_name(id).is_some(),
                                "{which}: defaults to the presence of unregistered {id}"
                            );
                        }
                    }
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

    /// A switch with no choice of the user's follows its default rule over the added agents; a
    /// value the setting does not take is not a choice at all.
    #[test]
    fn a_switch_with_no_choice_follows_its_default_rule() {
        let compatibility =
            |agent: AgentId| agent.descriptor().setting("claude_compatibility").unwrap();
        let in_effect =
            |agent: AgentId, chosen: Option<SettingValue>, added: Option<&[AgentId]>| {
                compatibility(agent).in_effect(chosen.as_ref(), added)
            };
        let on = Some(SettingValue::Switch(true));
        let off = Some(SettingValue::Switch(false));
        assert_eq!(in_effect(AgentId::GROK, None, None), on);
        assert_eq!(in_effect(AgentId::CODEX, None, None), off);
        assert_eq!(
            in_effect(AgentId::CODEX, None, Some(&[AgentId::CODEX])),
            off
        );
        assert_eq!(
            in_effect(AgentId::CODEX, None, Some(&[AgentId::CLAUDE])),
            on
        );
        assert_eq!(in_effect(AgentId::GROK, off.clone(), None), off);
        assert_eq!(in_effect(AgentId::CODEX, on.clone(), None), on);
        let wrong = Some(SettingValue::Choice("on".into()));
        assert_eq!(in_effect(AgentId::GROK, wrong, None), on);

        let reviewer = AgentId::CODEX
            .descriptor()
            .setting("approval_reviewer")
            .unwrap();
        assert_eq!(reviewer.in_effect(None, None), None);
        let user = SettingValue::Choice("user".into());
        assert_eq!(reviewer.in_effect(Some(&user), None), Some(user));
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
}
