//! The plugin cache and its registry stay together in the account because the registry lives
//! in `config.json` with sign-in state. Directory marketplaces in settings are used in place.

use crate::accounts::shared_config::{
    config_dir, config_file, history_dir, FileRegistry, SharedItem,
};
use crate::hostfs::json_paths::Select;
use crate::providers::copilot::LAYOUT;

const SETTINGS: &str = "settings.json";

pub(super) const REGISTRIES: &[FileRegistry] = &[FileRegistry {
    item: SETTINGS,
    select: Select::At(&["extraKnownMarketplaces", "*", "source", "path"]),
    source_path: None,
}];

pub(super) const ITEMS: &[SharedItem] = &[
    config_file(SETTINGS),
    config_file("mcp-config.json"),
    config_dir("instructions"),
    history_dir(LAYOUT.records_dir),
];

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::coordinator::account_sharing::set;
    use crate::protocol::{
        Account, AgentAvailability, AgentId, Availability, Session, SessionStatus,
    };
    use crate::test_support::app_state;

    fn write(root: &Path, name: &str, text: &str) {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[tokio::test]
    async fn shares_configuration_and_history_but_keeps_account_files_private() {
        if !crate::platform::fs::file_links_available() {
            return;
        }
        let (state, dir) = app_state("copilot-sharing");
        let (default, account) = (dir.join("default"), dir.join("account"));
        let agent = AgentId::COPILOT;
        state.set_agent_availability(vec![AgentAvailability {
            agent,
            availability: Availability::Available,
            default_account_dir: Some(default.to_string_lossy().into_owned()),
        }]);
        state
            .store
            .insert_account(&Account {
                id: "account".to_owned(),
                agent,
                name: "Account".to_owned(),
                config_dir: account.to_string_lossy().into_owned(),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
                versions: Default::default(),
            })
            .unwrap();

        let shared = [
            "instructions/example.instructions.md",
            "instructions/example-market/.github/plugin/marketplace.json",
            "mcp-config.json",
        ];
        let private = [
            "config.json",
            "installed-plugins/_direct/demo/manifest.json",
        ];
        for name in shared.iter().chain(&private) {
            write(&default, name, "default");
            write(&account, name, "account");
        }
        write(&account, SETTINGS, r#"{"footer":{"showQuota":false}}"#);
        let private_market = account.join("private-market");
        std::fs::create_dir(&private_market).unwrap();
        write(
            &default,
            SETTINGS,
            &serde_json::json!({
                "extraKnownMarketplaces": {
                    "private": {"source": {"source": "directory", "path": private_market}},
                },
            })
            .to_string(),
        );
        let old_record = format!("{}/existing/record.jsonl", LAYOUT.records_dir);
        write(&account, &old_record, "earlier history");

        let err = set(&state, Some("account".to_owned()), true)
            .await
            .unwrap_err();
        assert_eq!(
            crate::test_support::code_of(&err),
            crate::protocol::error_code::ACCOUNT_SHARING_FAILED
        );
        let states = state.account_sharing_states().unwrap();
        let failure = states[0].failure.as_ref().unwrap();
        assert_eq!(failure.items[0].item, SETTINGS);
        assert!(failure.items[0]
            .detail
            .contains("private to the sharing account"));
        assert!(!account.join(SETTINGS).is_symlink());

        write(&default, SETTINGS, r#"{"footer":{"showQuota":true}}"#);
        set(&state, Some("account".to_owned()), true).await.unwrap();

        for name in shared {
            assert_eq!(
                std::fs::read_to_string(account.join(name)).unwrap(),
                "default",
                "{name}"
            );
        }
        for name in private {
            assert_eq!(
                std::fs::read_to_string(account.join(name)).unwrap(),
                "account",
                "{name}"
            );
        }
        let market = account.join("instructions/example-market");
        let settings = serde_json::json!({
            "extraKnownMarketplaces": {"test": {"source": {"source": "directory", "path": market}}},
            "unrelatedPath": market,
        });
        assert_eq!(
            std::fs::read_to_string(account.join(SETTINGS)).unwrap(),
            r#"{"footer":{"showQuota":true}}"#
        );
        write(&account, SETTINGS, &settings.to_string());
        let ended = Session {
            account_id: Some("account".to_owned()),
            config_dir: Some(account.to_string_lossy().into_owned()),
            ..crate::test_support::project_session("ended", agent, SessionStatus::Interrupted)
        };
        crate::accounts::shared_config::keep_registries_after_exit(&state, &ended).await;
        let updated: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(default.join(SETTINGS)).unwrap())
                .unwrap();
        assert_eq!(
            updated["extraKnownMarketplaces"]["test"]["source"]["path"],
            serde_json::json!(crate::hostfs::canonical(&market).unwrap())
        );
        assert_eq!(updated["unrelatedPath"], settings["unrelatedPath"]);
        let new_record = format!("{}/new/record.jsonl", LAYOUT.records_dir);
        write(&account, &new_record, "later history");
        assert_eq!(
            std::fs::read_to_string(default.join(&old_record)).unwrap(),
            "earlier history"
        );
        assert_eq!(
            std::fs::read_to_string(default.join(&new_record)).unwrap(),
            "later history"
        );

        set(&state, Some("account".to_owned()), false)
            .await
            .unwrap();

        for name in shared {
            assert_eq!(
                std::fs::read_to_string(account.join(name)).unwrap(),
                "account",
                "{name}"
            );
        }
        assert_eq!(
            std::fs::read_to_string(account.join(SETTINGS)).unwrap(),
            r#"{"footer":{"showQuota":false}}"#
        );
        for name in [old_record, new_record] {
            assert!(default.join(&name).exists(), "{name}");
            assert!(!account.join(&name).exists(), "{name}");
        }
    }
}
