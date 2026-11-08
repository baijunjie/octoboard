//! Reading an account's MCP servers and platform connectors: what a provider's
//! [`McpInventoryReader`] does through its CLI, and [`read_inventory`], which runs it off the
//! request path with a timeout and lets a second request for the same account join the reading
//! under way.
//! Only `unknown_account` refuses a request; every other failure is the inventory's `unavailable`.

use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::OnceCell;

use super::choice::agent_is_available;
use super::quotas::ShellSource;
use crate::adapter::provider;
use crate::protocol::{
    AccountMcpInventory, AgentId, LocalMcpServer, McpInventoryFailure, McpInventoryUnavailable,
    PlatformConnector,
};
use crate::state::AppState;
use crate::time::now_millis;

/// How long one reading may take: it connects to every server, and a refetch reaches the network.
const READ_TIMEOUT: Duration = Duration::from_secs(60);

/// What a reading found: the platform's connectors and the configured servers.
pub(crate) type Listing = (Vec<PlatformConnector>, Vec<LocalMcpServer>);

/// A provider's way of listing an account's MCP servers and connectors through its CLI. A reading
/// never writes the account's configuration, and a state value of the agent that Octoboard does not
/// know is `unknown`, never a failed reading.
pub(crate) trait McpInventoryReader: Send + Sync {
    /// Blocking. `config_dir` is the account's directory, `None` for the default account;
    /// `refresh` asks the agent to refetch what it caches, where it has such a thing. Setting
    /// `cancel` stops it.
    fn read(
        &self,
        shell_env: &HashMap<String, String>,
        config_dir: Option<&Path>,
        refresh: bool,
        timeout: Duration,
        cancel: &AtomicBool,
    ) -> Result<Listing, McpInventoryUnavailable>;
}

/// Which reading: the agent, the account's id (`None` for the default account) and the account's
/// directory as stored (`None` for the default account), so that a request made after the account
/// is pointed at another directory never joins a reading of the old one.
type Key = (AgentId, Option<String>, Option<String>);

/// The readings running now, one per account and directory. Held in memory only.
#[derive(Default)]
pub struct McpInventoryReadings {
    running: Mutex<HashMap<Key, Arc<OnceCell<AccountMcpInventory>>>>,
}

impl McpInventoryReadings {
    /// The inventory `work` makes for `key`, or, while another call for `key` is running its own
    /// work, that call's inventory: the work runs once and every caller gets its result. A caller
    /// that is dropped while it runs the work leaves it to the next waiting one.
    async fn joined<F: Future<Output = AccountMcpInventory>>(
        &self,
        key: Key,
        work: F,
    ) -> AccountMcpInventory {
        let cell = self
            .running
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(key.clone())
            .or_default()
            .clone();
        let inventory = cell.get_or_init(|| work).await.clone();
        let mut running = self
            .running
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if running
            .get(&key)
            .is_some_and(|held| Arc::ptr_eq(held, &cell))
        {
            running.remove(&key);
        }
        inventory
    }
}

/// Reads the MCP servers and connectors of the `agent` account `account` (an account id, `None`
/// for the default account) in the shell environment `shell` gives. A reading of the same
/// account and directory already running is joined, whatever `refresh` each asked for. Refused
/// with `unknown_account` for an account that is no longer stored.
pub(crate) async fn read_inventory(
    state: &Arc<AppState>,
    agent: AgentId,
    account: Option<String>,
    refresh: bool,
    shell: ShellSource,
) -> anyhow::Result<AccountMcpInventory> {
    let config_dir = state.store.account_dir(&account)?;
    if let (Some(id), None) = (&account, &config_dir) {
        return Err(crate::protocol::CodedError::unknown_account(id));
    }
    let key = (agent, account.clone(), config_dir.clone());
    let stopping = state.account_quotas.stopping_flag();
    let held = state.clone();
    let inventory = state
        .mcp_inventories
        .joined(key, async move {
            let outcome = attempt(
                &held,
                agent,
                config_dir,
                refresh,
                shell,
                READ_TIMEOUT,
                stopping,
            )
            .await;
            let (connectors, servers, unavailable) = match outcome {
                Ok((connectors, servers)) => (connectors, servers, None),
                Err(unavailable) => (vec![], vec![], Some(unavailable)),
            };
            AccountMcpInventory {
                agent,
                account,
                read_at: now_millis(),
                connectors,
                servers,
                unavailable,
            }
        })
        .await;
    Ok(inventory)
}

async fn attempt(
    state: &Arc<AppState>,
    agent: AgentId,
    config_dir: Option<String>,
    refresh: bool,
    shell: ShellSource,
    timeout: Duration,
    stopping: Arc<AtomicBool>,
) -> Result<Listing, McpInventoryUnavailable> {
    let unavailable = |reason, detail| McpInventoryUnavailable::new(reason, detail);
    let Some(reader) = provider(agent)
        .accounts()
        .and_then(|ops| ops.mcp_inventory())
    else {
        return Err(unavailable(McpInventoryFailure::UnsupportedAgent, None));
    };
    if !agent_is_available(state, agent) {
        return Err(unavailable(McpInventoryFailure::CliMissing, None));
    }
    tokio::task::spawn_blocking(move || {
        let shell_env = shell()
            .map_err(|err| unavailable(McpInventoryFailure::Failed, Some(format!("{err:#}"))))?;
        reader.read(
            &shell_env,
            config_dir.as_deref().map(Path::new),
            refresh,
            timeout,
            &stopping,
        )
    })
    .await
    .unwrap_or_else(|_| {
        Err(unavailable(
            McpInventoryFailure::Failed,
            Some("the reading stopped unexpectedly".to_string()),
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn inventory(read_at: i64) -> AccountMcpInventory {
        AccountMcpInventory {
            agent: AgentId::CODEX,
            account: None,
            read_at,
            connectors: vec![],
            servers: vec![],
            unavailable: None,
        }
    }

    /// A provider declares `capabilities.mcp_inventory`, which clients show the inventory by,
    /// exactly when it has a reader; any other is answered as unsupported.
    #[test]
    fn the_capability_matches_whether_a_reader_exists() {
        for agent in AgentId::ALL {
            let reads = provider(agent)
                .accounts()
                .and_then(|ops| ops.mcp_inventory())
                .is_some();
            assert_eq!(
                agent.descriptor().capabilities.mcp_inventory,
                reads,
                "{}",
                agent.wire_name()
            );
        }
    }

    #[tokio::test]
    async fn a_second_request_for_the_same_account_joins_the_reading_under_way() {
        let readings = McpInventoryReadings::default();
        let runs = AtomicUsize::new(0);
        let work = |read_at| {
            let runs = &runs;
            async move {
                runs.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(50)).await;
                inventory(read_at)
            }
        };
        let key = || (AgentId::CODEX, None, None);
        let (first, second) = tokio::join!(
            readings.joined(key(), work(1)),
            readings.joined(key(), work(2))
        );
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        assert_eq!(first, second);

        // Once it is done the next request reads again, and another account is its own reading.
        let third = readings.joined(key(), work(3)).await;
        assert_eq!(third.read_at, 3);
        let account =
            |id: &str, dir: &str| (AgentId::CODEX, Some(id.to_string()), Some(dir.to_string()));
        let (a, b) = tokio::join!(
            readings.joined(account("a", "/a"), work(4)),
            readings.joined(account("b", "/b"), work(5)),
        );
        assert_eq!((a.read_at, b.read_at), (4, 5));
        // Nor does an account pointed at another directory join the reading of the old one.
        let (old, new) = tokio::join!(
            readings.joined(account("a", "/a"), work(6)),
            readings.joined(account("a", "/elsewhere"), work(7)),
        );
        assert_eq!((old.read_at, new.read_at), (6, 7));
        assert_eq!(runs.load(Ordering::SeqCst), 6);
    }
}
