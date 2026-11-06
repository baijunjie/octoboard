//! The descriptor data the daemon serves for each registered provider, so a client reads a
//! provider's facts from the daemon instead of keeping its own table of them.

use serde::Serialize;

use crate::providers::{
    descriptors, AgentId, Capabilities, ModelDisplay, ProviderDescriptor, QuotaWindowNaming,
    SettingDescriptor,
};

/// One registered provider, as `snapshot` carries it.
#[cfg_attr(test, derive(ts_rs::TS))]
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
    pub capabilities: Capabilities,
    /// How the provider's own interface names its quota windows, which a client's wording follows.
    pub quota_windows: QuotaWindowNaming,
    /// How a model id of the provider is shown.
    pub model_display: ModelDisplay,
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
            capabilities: descriptor.capabilities,
            quota_windows: descriptor.quota_windows,
            model_display: descriptor.model_display,
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

    #[test]
    fn every_registered_provider_is_served_in_the_registrys_order() {
        let served = registered_providers();
        let ids: Vec<AgentId> = served.iter().map(|provider| provider.id).collect();
        assert_eq!(ids, AgentId::ALL);
        let claude = serde_json::to_value(&served[0]).unwrap();
        assert_eq!(claude["id"], "claude");
        assert_eq!(claude["label"], "Claude Code");
        assert_eq!(claude["default_config_dir"], ".claude");
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
        let reviewer = &serde_json::to_value(&served[1]).unwrap()["settings"][0];
        assert_eq!(reviewer["id"], "approval_reviewer");
        assert_eq!(reviewer["kind"]["type"], "choice");
        assert_eq!(reviewer["kind"]["choices"][0]["value"], "auto_review");
        assert_eq!(reviewer["exported"], true);
        let bypass = &claude["settings"][0]["kind"]["choices"][2];
        assert_eq!(
            (bypass["value"].as_str(), bypass["confirm"].as_bool()),
            (Some("bypass"), Some(true))
        );
        let compatibility = &grok["settings"][0];
        assert_eq!(
            compatibility["kind"],
            serde_json::json!({"type": "switch", "default": "on"})
        );
        assert_eq!(compatibility["compatibility_with"], "claude");
        assert_eq!(
            serde_json::to_value(&served[1]).unwrap()["settings"][1]["kind"]["default"],
            serde_json::json!({"while_added": "claude"})
        );
    }
}
