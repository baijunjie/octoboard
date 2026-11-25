//! Pi's account sign-in and MCP inventory are independent of model-provider quota.

use std::collections::HashMap;
use std::path::Path;

use crate::accounts::quotas::sign_in::SignInReading;
use crate::accounts::AccountOps;

pub(super) struct PiAccounts;

impl AccountOps for PiAccounts {
    fn login(&self) -> Option<&'static dyn crate::accounts::login::flows::LoginFlow> {
        Some(&super::login::PiLogin)
    }

    fn mcp_inventory(
        &self,
    ) -> Option<&'static dyn crate::accounts::mcp_inventory::McpInventoryReader> {
        Some(&super::mcp_inventory::PiMcpInventory)
    }

    fn shared_items(&self) -> &'static [crate::accounts::shared_config::SharedItem] {
        super::shared_config::ITEMS
    }

    fn file_registries(&self) -> &'static [crate::accounts::shared_config::FileRegistry] {
        super::shared_config::REGISTRIES
    }

    fn check_shared_item(
        &self,
        item: &crate::accounts::shared_config::SharedItem,
        account: &Path,
        default: &Path,
    ) -> std::io::Result<()> {
        if item.name == super::shared_config::SETTINGS {
            super::shared_config::check_settings(account, default)?;
        }
        Ok(())
    }

    fn plugin_registries(&self) -> Option<&'static [(&'static str, &'static str)]> {
        Some(&[])
    }

    fn run_home(
        &self,
        _: &std::path::Path,
        _: crate::adapter::account_cli::BackgroundRun,
        _: &std::path::Path,
        _: &std::collections::HashMap<String, String>,
        _: &crate::adapter::account_cli::RunWait<'_>,
    ) -> anyhow::Result<Option<crate::adapter::account_cli::RunHome>> {
        Ok(None)
    }

    /// Read whether Pi has a usable model-provider credential for the account.
    fn sign_in_state(
        &self,
        config_dir: Option<&Path>,
        shell_env: &HashMap<String, String>,
    ) -> SignInReading {
        super::models::sign_in_state(config_dir, shell_env)
    }
}
