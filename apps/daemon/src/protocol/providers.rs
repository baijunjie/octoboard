//! The descriptor data the daemon serves for each registered provider, so a client reads a
//! provider's facts from the daemon instead of keeping its own table of them.

use serde::Serialize;

use crate::providers::{
    descriptors, AgentId, Capabilities, ModelDisplay, NativeMemory, ProviderDescriptor,
    QuotaWindowNaming, SettingDescriptor,
};

/// One registered provider, as `snapshot` carries it.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProviderInfo {
    pub id: AgentId,
    /// The provider's product name, for a client with no presentation of its own for it.
    pub label: &'static str,
    /// The shorter form of `label`, where the text around it already says which product is meant.
    pub short_name: &'static str,
    /// Where the user is sent to install the provider's CLI.
    pub install_url: &'static str,
    /// The configuration directory the provider uses when none is set, relative to the home
    /// directory (`.claude`).
    pub default_config_dir: &'static str,
    /// The names at a project's top level that point at the provider, as `DetectionMarkers` holds
    /// them: its own, then the generic ones it reads. A client shows them to say what detecting
    /// the agent of a repository looks for; several providers may carry the same generic name.
    pub detection_markers: Vec<&'static str>,
    pub capabilities: Capabilities,
    /// How the provider's own interface names its quota windows, which a client's wording follows.
    pub quota_windows: QuotaWindowNaming,
    /// How a model id of the provider is shown.
    pub model_display: ModelDisplay,
    /// Whether the agent keeps a memory of its own across sessions, and whether one launch can
    /// turn it off: a client names, beside the memory setting, the agents whose own memory a
    /// session launched while it is on has turned off and those whose own memory such a session
    /// leaves as the user set it in the agent, unable to turn it off.
    pub native_memory: NativeMemory,
    /// The settings the user can make for the provider, in the order a client lists them. Their
    /// values in effect are `Settings::provider_settings`.
    pub settings: &'static [SettingDescriptor],
}

impl ProviderInfo {
    fn of(agent: AgentId, descriptor: &'static ProviderDescriptor) -> Self {
        Self {
            id: agent,
            label: descriptor.label,
            short_name: descriptor.short_name,
            install_url: descriptor.install_url,
            default_config_dir: descriptor.layout.default_dir,
            detection_markers: descriptor
                .detection
                .own
                .iter()
                .chain(descriptor.detection.generic)
                .copied()
                .collect(),
            capabilities: descriptor.capabilities,
            quota_windows: descriptor.quota_windows,
            model_display: descriptor.model_display,
            native_memory: descriptor.native_memory,
            settings: descriptor.settings,
        }
    }
}

/// Every registered provider, in the registry's order.
pub fn registered_providers() -> Vec<ProviderInfo> {
    AgentId::ALL
        .into_iter()
        .zip(descriptors())
        .map(|(agent, descriptor)| ProviderInfo::of(agent, descriptor))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each provider serves its descriptor's own memory fact, under its wire name, so a client
    /// tells an agent whose memory a launch turns off from one whose memory no launch can turn off
    /// and from one with no memory of its own.
    #[test]
    fn every_provider_serves_its_own_memory_fact() {
        for (provider, descriptor) in registered_providers().iter().zip(descriptors()) {
            assert_eq!(
                provider.native_memory, descriptor.native_memory,
                "{}",
                descriptor.id
            );
            let wire = serde_json::to_value(provider).unwrap()["native_memory"].clone();
            let expected = match descriptor.native_memory {
                NativeMemory::Absent => "absent",
                NativeMemory::Switchable => "switchable",
                NativeMemory::Unswitchable => "unswitchable",
            };
            assert_eq!(wire, expected, "{}", descriptor.id);
        }
    }

    #[test]
    fn every_registered_provider_is_served_in_the_registrys_order() {
        let served = registered_providers();
        let ids: Vec<AgentId> = served.iter().map(|provider| provider.id).collect();
        assert_eq!(ids, AgentId::ALL);
        let claude = serde_json::to_value(&served[0]).unwrap();
        assert_eq!(claude["id"], "claude");
        assert_eq!(claude["label"], "Claude Code");
        assert_eq!(claude["default_config_dir"], ".claude");
        assert_eq!(
            claude["detection_markers"],
            serde_json::json!(["CLAUDE.md", ".claude"])
        );
        assert_eq!(
            serde_json::to_value(&served[1]).unwrap()["detection_markers"],
            serde_json::json!([".codex", "AGENTS.md", ".agents"])
        );
        assert_eq!(claude["capabilities"]["status_line"], true);
        assert_eq!(
            claude["capabilities"]["delivery"]["holds_report_for_completion_notice"],
            true
        );
        assert_eq!(claude["quota_windows"]["session_window"], "session");
        assert_eq!(claude["quota_windows"]["weekly_all_models"], true);
        assert_eq!(claude["quota_windows"]["kinds"][2], "weekly_model");
        assert_eq!(claude["model_display"]["id_prefix"], "claude-");
        assert_eq!(claude["model_display"]["retired"][0], "best");
        let grok = serde_json::to_value(&served[2]).unwrap();
        assert_eq!(grok["quota_windows"]["kinds"][0], "credit_period");
        assert_eq!(grok["model_display"]["family_version"], false);
        let copilot = served
            .iter()
            .find(|provider| provider.id == AgentId::COPILOT)
            .unwrap();
        assert_eq!(
            serde_json::to_value(copilot).unwrap()["quota_windows"]["credit_period"],
            "plan"
        );
        let reviewer = &serde_json::to_value(&served[1]).unwrap()["settings"][0];
        assert_eq!(reviewer["id"], "approval_reviewer");
        assert_eq!(reviewer["kind"]["type"], "choice");
        assert_eq!(reviewer["kind"]["choices"][0]["value"], "user");
        assert_eq!(reviewer["exported"], true);
        let bypass = &claude["settings"][0]["kind"]["choices"][4];
        assert_eq!(
            (bypass["value"].as_str(), bypass["confirm"].as_bool()),
            (Some("bypass"), Some(true))
        );
        let approval = &grok["settings"][0];
        assert_eq!(approval["id"], "approval_mode");
        assert_eq!(approval["exported"], true);
        assert_eq!(approval["shown_to_agents"], false);
        assert_eq!(
            approval["kind"]["choices"],
            serde_json::json!([
                {"value": "ask", "confirm": false},
                {"value": "auto", "confirm": false},
                {"value": "always_approve", "confirm": true}
            ])
        );
        let setting = AgentId::GROK.descriptor().setting("approval_mode").unwrap();
        for value in ["ask", "auto", "always_approve"] {
            assert!(setting.accepts(&crate::providers::SettingValue::Choice(value.into())));
        }
        assert!(!setting.accepts(&crate::providers::SettingValue::Choice("plan".into())));
        let compatibility = &grok["settings"][1];
        assert_eq!(
            compatibility["kind"],
            serde_json::json!({"type": "switch", "default": "off"})
        );
        assert_eq!(compatibility["compatibility_with"], "claude");
        let codex_compatibility = |provider: &serde_json::Value| {
            provider["settings"]
                .as_array()
                .unwrap()
                .iter()
                .find(|setting| setting["id"] == "codex_compatibility")
                .cloned()
                .unwrap()
        };
        for provider in [&claude, &grok] {
            let setting = codex_compatibility(provider);
            assert_eq!(
                setting["kind"],
                serde_json::json!({"type": "switch", "default": "off"})
            );
            assert_eq!(setting["compatibility_with"], "codex");
        }
    }

    /// Whether a provider's accounts beyond the default each have a directory of their own is
    /// served with its other capabilities, as the descriptor declares it.
    #[test]
    fn owned_accounts_is_served_as_the_descriptor_declares_it() {
        for provider in registered_providers() {
            let served = serde_json::to_value(&provider).unwrap();
            assert_eq!(
                served["capabilities"]["owned_accounts"],
                provider.id.descriptor().capabilities.owned_accounts,
                "{}",
                provider.id.wire_name()
            );
        }
        let _declared = crate::providers::test_provider::without_owned_accounts();
        let served =
            serde_json::to_value(ProviderInfo::of(AgentId::TEST, AgentId::TEST.descriptor()))
                .unwrap();
        assert_eq!(served["capabilities"]["owned_accounts"], false);
    }

    /// Claude Code's simple shell commands switch follows Codex compatibility: off until chosen,
    /// exported with the configuration, shown to agents (it is no permission setting), and not a compatibility switch, so no
    /// launch level can override it.
    #[test]
    fn claude_serves_the_simple_shell_commands_switch_after_codex_compatibility() {
        let claude = serde_json::to_value(&registered_providers()[0]).unwrap();
        let settings = claude["settings"].as_array().unwrap();
        let ids: Vec<_> = settings
            .iter()
            .map(|setting| setting["id"].as_str().unwrap())
            .collect();
        let at = ids.iter().position(|id| *id == "simple_shell_commands");
        assert_eq!(
            at,
            ids.iter()
                .position(|id| *id == "codex_compatibility")
                .map(|at| at + 1)
        );
        let setting = &settings[at.unwrap()];
        assert_eq!(
            setting["kind"],
            serde_json::json!({"type": "switch", "default": "off"})
        );
        assert_eq!(setting["exported"], true);
        assert_eq!(setting["shown_to_agents"], true);
        assert!(setting["compatibility_with"].is_null());
        let descriptor = AgentId::CLAUDE.descriptor();
        assert!(!descriptor.is_overridable("simple_shell_commands"));
        assert!(descriptor
            .setting("simple_shell_commands")
            .unwrap()
            .accepts(&crate::providers::SettingValue::Switch(true)));
    }
}
