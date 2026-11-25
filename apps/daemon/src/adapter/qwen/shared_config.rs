//! Qwen Code writes extension settings into the installation's `.env` through directory links.
//! Keep each extension and its enablement in the account with that file.

use crate::accounts::shared_config::{config_dir, config_file, history_dir, SharedItem};
use crate::providers::qwen::LAYOUT;

pub(super) const ITEMS: &[SharedItem] = &[
    config_file("QWEN.md"),
    config_dir("skills"),
    history_dir(LAYOUT.records_dir),
];

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::coordinator::account_sharing::set;
    use crate::protocol::{Account, AgentAvailability, AgentId, Availability};
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
        let (state, dir) = app_state("qwen-sharing");
        let (default, account) = (dir.join("default"), dir.join("account"));
        let agent = AgentId::QWEN;
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

        let shared = ["skills/example/SKILL.md", "QWEN.md"];
        let private = [
            "settings.json",
            ".env",
            "subscription_credentials.json",
            "extensions/demo/.qwen-extension-install.json",
            "extension-store/state.json",
        ];
        for name in shared.iter().chain(&private) {
            write(&default, name, "default");
            write(&account, name, "account");
        }
        let old_record = format!("{}/existing/record.jsonl", LAYOUT.records_dir);
        write(&account, &old_record, "earlier history");

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
        for name in [old_record, new_record] {
            assert!(default.join(&name).exists(), "{name}");
            assert!(!account.join(&name).exists(), "{name}");
        }
    }
}
