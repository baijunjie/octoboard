//! Bringing a configuration document of an older version up to [`CONFIGURATION_VERSION`] on the raw
//! JSON, before it is read as a [`ConfigurationDocument`](crate::protocol::ConfigurationDocument):
//! an inspection and an import both see the upgraded document, so they cannot disagree about what
//! an old one carries. `apps/daemon/PROTOCOL.md` ("Configuration export", paragraph "Version") says
//! what each version changed.

use serde_json::{json, Map, Value};

use crate::protocol::CONFIGURATION_VERSION;

/// The oldest version an import still reads: the first one released.
pub(crate) const OLDEST_VERSION: u32 = 1;

type Document = Map<String, Value>;

/// The steps that bring a document up one version: `STEPS[n - OLDEST_VERSION]` takes a document of
/// version `n` to `n + 1`.
const STEPS: [fn(&mut Document); 2] = [from_v1, from_v2];

/// Upgrades `document`, of `version`, to the current version, which it then names. The caller has
/// checked that `version` lies between [`OLDEST_VERSION`] and [`CONFIGURATION_VERSION`].
pub(crate) fn upgrade(document: &mut Document, version: u32) {
    for step in &STEPS[(version - OLDEST_VERSION) as usize..] {
        step(document);
    }
    document.insert("version".to_string(), json!(CONFIGURATION_VERSION));
}

/// Version 1 to 2.
///
/// - The settings of the Agent accounts section (the three switches, `agent_tags`,
///   `provider_settings`) move from `settings` to `accounts.agent_settings`. A document that has
///   them but no accounts region gets one with no agents, so they are kept and an import applies
///   them with the accounts, as the accounts region's settings now are.
/// - An account's `share_default_config` is dropped: an import no longer reads it.
/// - What version 1 never wrote is written as `null`, which reads as "the document does not say"
///   and leaves this machine's value as it is: the default clone directory, the default account's
///   `disabled` flag, and the account settings of a document with no `settings` region.
fn from_v1(document: &mut Document) {
    const MOVED: [&str; 5] = [
        "auto_select_account",
        "auto_switch_on_usage_limit",
        "auto_start_quota_windows",
        "agent_tags",
        "provider_settings",
    ];
    let mut agent_settings = Map::new();
    if let Some(Value::Object(settings)) = document.get_mut("settings") {
        for key in MOVED {
            if let Some(value) = settings.remove(key) {
                agent_settings.insert(key.to_string(), value);
            }
        }
        settings.entry("default_clone_dir").or_insert(Value::Null);
    }
    let agent_settings = if agent_settings.is_empty() {
        Value::Null
    } else {
        Value::Object(agent_settings)
    };
    match document.get_mut("accounts") {
        Some(Value::Object(accounts)) => {
            accounts.insert("agent_settings".to_string(), agent_settings);
            for agent in accounts
                .get_mut("agents")
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                if let Some(Value::Object(default_account)) = agent.get_mut("default_account") {
                    default_account.entry("disabled").or_insert(Value::Null);
                }
                for account in agent
                    .get_mut("accounts")
                    .and_then(Value::as_array_mut)
                    .into_iter()
                    .flatten()
                {
                    if let Some(account) = account.as_object_mut() {
                        account.remove("share_default_config");
                    }
                }
            }
        }
        _ if !agent_settings.is_null() => {
            document.insert(
                "accounts".to_string(),
                json!({ "agents": [], "agent_settings": agent_settings }),
            );
        }
        _ => {}
    }
}

/// Version 2 to 3.
///
/// - `settings.sidebar_order_by_status` is dropped: the sidebar's sort is a choice a client keeps,
///   so an import no longer reads it.
fn from_v2(document: &mut Document) {
    if let Some(Value::Object(settings)) = document.get_mut("settings") {
        settings.remove("sidebar_order_by_status");
    }
}
