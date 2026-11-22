//! What a provider's accounts can be asked beyond launching: their quota, the email they sign in
//! with, their reset credits, signing in (to the agent, and to its MCP servers), reading their MCP
//! servers and connectors, and the configuration they can share with the default account's.
//! `AgentProvider::accounts` is `None` for a provider that has no accounts in this sense, and every
//! method here has a default, so a provider implements only what it has.

use std::collections::HashMap;
use std::path::Path;

use super::login::flows::LoginFlow;
use super::mcp_inventory::McpInventoryReader;
use super::mcp_sign_in::McpServerAuth;
use super::quotas::kick::KickCommand;
use super::quotas::probe::QuotaProbe;
use super::quotas::reset_credit::ResetCredits;
use super::shared_config::SharedItem;
use super::thread_registry::ThreadRegistry;
use crate::adapter::account_cli::{BackgroundRun, RunHome, RunWait};

/// A provider's accounts, beyond launching a session against one.
pub(crate) trait AccountOps: Send + Sync {
    /// The quota probe: the conversation with the CLI and how its reply reads. `None`: the quota is
    /// never read, and the provider's accounts are left out of the quota poll.
    fn quota_probe(&self) -> Option<&'static dyn QuotaProbe> {
        None
    }

    /// The request that starts a usage window that has reset but not begun counting. `None`: no
    /// kick is sent.
    fn kick_command(&self) -> Option<KickCommand> {
        None
    }

    /// A directory of Octoboard's own, under `run_homes`, to make `run` against in place of the
    /// account's directory `account_dir`, for a CLI measured to change the account's configuration
    /// when that run is made against the account's own directory; `None` makes it against the
    /// account's. Every provider answers, from what was measured of its CLI: running against the
    /// account's directory is what lets a CLI write the user's files, so it is never assumed.
    /// Asked once the account's directory is admitted for the run. A directory another run holds
    /// is waited for as `wait` says. `Err` is why the directory could not be made ready, and the
    /// run is then not made at all: the account's own directory is not fallen back to.
    fn run_home(
        &self,
        run_homes: &Path,
        run: BackgroundRun,
        account_dir: &Path,
        shell_env: &HashMap<String, String>,
        wait: &RunWait<'_>,
    ) -> anyhow::Result<Option<RunHome>>;

    /// Removes what [`Self::run_home`] keeps under `run_homes` for the account at `account_dir`,
    /// when the account is removed: such a directory can hold a copy of the account's
    /// configuration. The default keeps nothing, so removes nothing.
    fn forget_run_homes(&self, _run_homes: &Path, _account_dir: &Path) {}

    /// The email the account under `config_dir` (`None` for the default account, whose directory
    /// follows `shell_env` as the CLI's would) signs in with, read from the provider's own sign-in
    /// record; `None` when there is none or it cannot be read.
    fn email(
        &self,
        _config_dir: Option<&Path>,
        _shell_env: &HashMap<String, String>,
    ) -> Option<String> {
        None
    }

    /// Using a rate limit reset credit, for a provider declaring `Capabilities::reset_credits`.
    fn reset_credits(&self) -> Option<&'static dyn ResetCredits> {
        None
    }

    /// Reading the account's MCP servers and platform connectors. `None`: the provider has no
    /// way of listing them, and a reading is answered as unsupported.
    fn mcp_inventory(&self) -> Option<&'static dyn McpInventoryReader> {
        None
    }

    /// Signing an account in to and out of an MCP server through the agent's own commands. `None`:
    /// the provider has no such commands, and the requests are refused as unsupported.
    fn mcp_server_auth(&self) -> Option<&'static dyn McpServerAuth> {
        None
    }

    /// Signing an account in without a terminal. `None`: a login ends at once as failed.
    fn login(&self) -> Option<&'static dyn LoginFlow> {
        None
    }

    /// The entries of the configuration directory an account may share with the default
    /// account's, for a provider declaring `Capabilities::config_sharing`.
    fn shared_items(&self) -> &'static [SharedItem] {
        &[]
    }

    /// The plugin registries in the shared entries that record the path a plugin was installed
    /// under, by the shared item they sit in and their path inside it: the files sharing keeps
    /// naming the default account's directory. A provider that shares entries states them,
    /// `Some(&[])` when it was measured to record no such path; `None` is for a provider that
    /// shares nothing.
    fn plugin_registries(&self) -> Option<&'static [(&'static str, &'static str)]> {
        None
    }

    /// The shared directory item in which the provider keeps per-project memory, which a launch
    /// of an account that shares it points at the default account's copy
    /// (`LaunchSpec::shared_default_dir`).
    fn shared_memory_item(&self) -> Option<&'static str> {
        None
    }

    /// The provider's own registry of conversation threads, which has to be kept in step with
    /// where their records are when records move between directories; `None` for a provider that
    /// finds records by name.
    fn thread_registry(&self) -> Option<&'static dyn ThreadRegistry> {
        None
    }
}
