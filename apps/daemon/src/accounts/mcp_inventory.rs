//! Reading an account's MCP servers and platform connectors: what a provider's
//! [`McpInventoryReader`] does through its CLI, and [`read_inventory`], which runs it off the
//! request path with a timeout and lets a second request for the same account join the reading
//! under way.
//! Only `unknown_account` refuses a request; every other failure is the inventory's `unavailable`.

use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::OnceCell;

use super::choice::agent_is_available;
use super::quotas::diagnostics::{redact, truncate, LINE_LIMIT};
use super::quotas::ShellSource;
use crate::adapter::account_cli::{NotRun, QueryFailure};
use crate::adapter::provider;
use crate::processes::subprocess::BoundedFailure;
use crate::protocol::{
    AccountMcpInventory, AgentId, LocalMcpServer, McpInventoryFailure, McpInventoryUnavailable,
    PlatformConnector,
};
use crate::state::AppState;
use crate::time::now_millis;

/// How long one reading may take: it connects to every server, and a refetch reaches the network.
const READ_TIMEOUT: Duration = Duration::from_secs(60);

/// How long a request waits for a reading in all, whatever step it is on. A reading that keeps
/// to its own bounds takes at most the shell snapshot (`SNAPSHOT_TIMEOUT`), the wait to be
/// admitted under the account's directory, the CLI runs within [`READ_TIMEOUT`] and, after a run
/// that times out, the CLI's stop (`AGENT_TERM_GRACE`). This leaves room beyond that sum, so the
/// reader reports its own timeout first; only a step that does not honour its bound reaches this
/// one, and the request is then answered as timed out with the CLI stopped.
const OVERALL_TIMEOUT: Duration = Duration::from_secs(90);

/// How often a reading under way is checked for the daemon stopping.
const STOP_POLL: Duration = Duration::from_millis(250);

/// A reading that takes longer than this is logged even when it succeeds.
const SLOW_READING: Duration = Duration::from_secs(20);

/// What a reading is doing, for saying where it was when it timed out.
const STEP_SHELL: &str = "waiting for the shell environment";
const STEP_CLI: &str = "waiting for the agent's CLI";

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

/// The unavailability for a query that was not started: a missing or refused binary is
/// `cli_missing`, anything else `failed`.
pub(crate) fn not_run_unavailable(refused: &NotRun) -> McpInventoryUnavailable {
    let reason = match refused {
        NotRun::BinaryNotFound(_) | NotRun::BinaryRefused(_) => McpInventoryFailure::CliMissing,
        NotRun::DirMissing(_) => McpInventoryFailure::Failed,
    };
    McpInventoryUnavailable::new(reason, Some(refused.detail()))
}

/// The unavailability for a query of an agent CLI that produced no output: a timeout is
/// `timed_out`, any other stop `failed` with its words.
pub(crate) fn query_unavailable(failure: QueryFailure) -> McpInventoryUnavailable {
    match failure {
        QueryFailure::NotRun(refused) => not_run_unavailable(&refused),
        QueryFailure::Bounded(BoundedFailure::TimedOut(_)) => {
            McpInventoryUnavailable::new(McpInventoryFailure::TimedOut, None)
        }
        QueryFailure::Bounded(other) => {
            McpInventoryUnavailable::new(McpInventoryFailure::Failed, Some(other.to_string()))
        }
    }
}

/// What says why a query failed (see [`crate::adapter::account_cli::failure_detail`]), else
/// `fallback`.
pub(crate) fn failure_detail(output: &std::process::Output, fallback: &str) -> String {
    let last = crate::adapter::account_cli::failure_detail(output, |line| line);
    if last.is_empty() {
        fallback.to_string()
    } else {
        last
    }
}

/// The serde default of an `enabled` flag a CLI's listing may leave out: on.
pub(crate) fn enabled_by_default() -> bool {
    true
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
            let started = Instant::now();
            let outcome = attempt(&held, agent, config_dir, refresh, shell, stopping).await;
            log_reading(agent, account.as_deref(), started.elapsed(), &outcome);
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

/// Whether a failure is a state the account or the machine is in, not a fault worth a log line.
fn is_expected(reason: McpInventoryFailure) -> bool {
    matches!(
        reason,
        McpInventoryFailure::NotSignedIn
            | McpInventoryFailure::CliMissing
            | McpInventoryFailure::UnsupportedAgent
    )
}

/// Writes one line for a reading that failed unexpectedly, timed out or was slow.
fn log_reading(
    agent: AgentId,
    account: Option<&str>,
    elapsed: Duration,
    outcome: &Result<Listing, McpInventoryUnavailable>,
) {
    let elapsed_ms = elapsed.as_millis() as u64;
    match outcome {
        Err(unavailable) if !is_expected(unavailable.reason) => {
            tracing::warn!(
                agent = agent.label(),
                account,
                elapsed_ms,
                outcome = ?unavailable.reason,
                detail = %unavailable
                    .detail
                    .as_deref()
                    .map(|detail| truncate(&redact(detail), LINE_LIMIT))
                    .unwrap_or_default(),
                "reading the account's MCP servers failed"
            );
        }
        Ok(_) if elapsed >= SLOW_READING => tracing::warn!(
            agent = agent.label(),
            account,
            elapsed_ms,
            "reading the account's MCP servers was slow"
        ),
        _ => {}
    }
}

async fn attempt(
    state: &Arc<AppState>,
    agent: AgentId,
    config_dir: Option<String>,
    refresh: bool,
    shell: ShellSource,
    stopping: Arc<AtomicBool>,
) -> Result<Listing, McpInventoryUnavailable> {
    let unavailable = |reason| McpInventoryUnavailable::new(reason, None);
    let Some(reader) = provider(agent)
        .accounts()
        .and_then(|ops| ops.mcp_inventory())
    else {
        return Err(unavailable(McpInventoryFailure::UnsupportedAgent));
    };
    if !agent_is_available(state, agent) {
        return Err(unavailable(McpInventoryFailure::CliMissing));
    }
    read_within(
        reader,
        config_dir,
        refresh,
        shell,
        READ_TIMEOUT,
        OVERALL_TIMEOUT,
        stopping,
    )
    .await
}

/// Sets the flag it holds when dropped, so that a reading whose future is dropped, by a runtime
/// shutting down or a caller that goes away, still stops its CLI.
struct CancelOnDrop(Arc<AtomicBool>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// Runs `reader` off the async runtime, after the shell snapshot, and answers within `overall`
/// whatever it is doing: past it the reading is cancelled, which stops its CLI, and the answer is
/// `timed_out` without waiting for that to finish. The daemon stopping cancels it too, as does
/// dropping the returned future.
async fn read_within(
    reader: &'static dyn McpInventoryReader,
    config_dir: Option<String>,
    refresh: bool,
    shell: ShellSource,
    read_timeout: Duration,
    overall: Duration,
    stopping: Arc<AtomicBool>,
) -> Result<Listing, McpInventoryUnavailable> {
    let cancel = Arc::new(AtomicBool::new(stopping.load(Ordering::Relaxed)));
    let _cancel_on_drop = CancelOnDrop(cancel.clone());
    let step = Arc::new(Mutex::new(STEP_SHELL));
    let mut work = tokio::task::spawn_blocking({
        let (cancel, step) = (cancel.clone(), step.clone());
        move || {
            let shell_env = shell().map_err(|err| {
                McpInventoryUnavailable::new(McpInventoryFailure::Failed, Some(format!("{err:#}")))
            })?;
            *step
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = STEP_CLI;
            reader.read(
                &shell_env,
                config_dir.as_deref().map(Path::new),
                refresh,
                read_timeout,
                &cancel,
            )
        }
    });
    let deadline = tokio::time::sleep(overall);
    tokio::pin!(deadline);
    let mut poll = tokio::time::interval(STOP_POLL);
    loop {
        tokio::select! {
            done = &mut work => {
                return done.unwrap_or_else(|_| {
                    Err(McpInventoryUnavailable::new(
                        McpInventoryFailure::Failed,
                        Some("the reading stopped unexpectedly".to_string()),
                    ))
                });
            }
            () = &mut deadline => {
                let step = *step.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                return Err(McpInventoryUnavailable::new(
                    McpInventoryFailure::TimedOut,
                    Some(format!("no answer after {}s, {step}", overall.as_secs())),
                ));
            }
            _ = poll.tick() => {
                if stopping.load(Ordering::Relaxed) {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
        }
    }
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

    /// What a failed query printed on either stream is its detail; with nothing printed it is the
    /// fallback.
    #[test]
    fn a_failure_detail_is_what_the_query_printed_else_the_fallback() {
        let printed = |stdout: &str, stderr: &str| std::process::Output {
            status: std::process::ExitStatus::default(),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        };
        assert_eq!(failure_detail(&printed("", ""), "no word"), "no word");
        assert_eq!(failure_detail(&printed("why\n", ""), "no word"), "why");
        assert_eq!(failure_detail(&printed("", "why\n"), "no word"), "why");
        assert_eq!(
            failure_detail(&printed("why\n", "warn\n"), "no word"),
            "why (stderr: warn)"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_second_request_for_the_same_account_joins_the_reading_under_way() {
        // How long a reading takes. Its length does not matter: on the paused clock a reading
        // that waits at all is still under way when the other request of the pair asks.
        const A_MOMENT: Duration = Duration::from_secs(1);
        let readings = McpInventoryReadings::default();
        let runs = AtomicUsize::new(0);
        let work = |read_at| {
            let runs = &runs;
            async move {
                runs.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(A_MOMENT).await;
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

    /// What a test sees of a reader on its blocking thread.
    #[derive(Default)]
    struct Probe {
        entered: AtomicBool,
        /// Raised the moment the reader first sees `cancel` raised, while it still works.
        cancelled: AtomicBool,
    }

    /// A reader that works until it is cancelled, recording that it started and that it was.
    struct UntilCancelled(Arc<Probe>);

    impl McpInventoryReader for UntilCancelled {
        fn read(
            &self,
            _shell_env: &HashMap<String, String>,
            _config_dir: Option<&Path>,
            _refresh: bool,
            _timeout: Duration,
            cancel: &AtomicBool,
        ) -> Result<Listing, McpInventoryUnavailable> {
            self.0.entered.store(true, Ordering::SeqCst);
            // Not a wait on time: it ends when the flag is raised, and gives up only past `PATIENCE`.
            let started = Instant::now();
            while started.elapsed() < crate::test_support::PATIENCE {
                if cancel.load(Ordering::Relaxed) {
                    self.0.cancelled.store(true, Ordering::SeqCst);
                    break;
                }
                std::thread::yield_now();
            }
            Err(McpInventoryUnavailable::new(
                McpInventoryFailure::Failed,
                None,
            ))
        }
    }

    /// A reader that answers at once with an empty listing.
    struct Immediate;

    impl McpInventoryReader for Immediate {
        fn read(
            &self,
            _shell_env: &HashMap<String, String>,
            _config_dir: Option<&Path>,
            _refresh: bool,
            _timeout: Duration,
            cancel: &AtomicBool,
        ) -> Result<Listing, McpInventoryUnavailable> {
            assert!(!cancel.load(Ordering::Relaxed));
            Ok((vec![], vec![]))
        }
    }

    /// The overall deadline the tests run `read_within` under; they move the paused clock by it.
    const DEADLINE: Duration = Duration::from_secs(5);

    /// Waits, off the clock, until `flag` is raised: a blocking thread's progress is not a timer,
    /// so the paused clock does not wait for it.
    async fn raised(flag: &AtomicBool) {
        let started = Instant::now();
        while !flag.load(Ordering::SeqCst) {
            assert!(
                started.elapsed() < crate::test_support::PATIENCE,
                "the flag was never raised"
            );
            tokio::task::yield_now().await;
        }
    }

    fn leaked(reader: impl McpInventoryReader + 'static) -> &'static dyn McpInventoryReader {
        Box::leak(Box::new(reader))
    }

    fn shell_ok() -> ShellSource {
        Arc::new(|| Ok(HashMap::new()))
    }

    /// A shell snapshot that is under way until the test lets it go.
    fn shell_held(entered: Arc<AtomicBool>, release: Arc<AtomicBool>) -> ShellSource {
        Arc::new(move || {
            entered.store(true, Ordering::SeqCst);
            let started = Instant::now();
            while !release.load(Ordering::SeqCst)
                && started.elapsed() < crate::test_support::PATIENCE
            {
                std::thread::yield_now();
            }
            Ok(HashMap::new())
        })
    }

    fn read(
        reader: &'static dyn McpInventoryReader,
        shell: ShellSource,
        stopping: Arc<AtomicBool>,
    ) -> tokio::task::JoinHandle<Result<Listing, McpInventoryUnavailable>> {
        tokio::spawn(read_within(
            reader,
            None,
            false,
            shell,
            READ_TIMEOUT,
            DEADLINE,
            stopping,
        ))
    }

    #[tokio::test(start_paused = true)]
    async fn a_reading_past_the_overall_deadline_is_answered_as_timed_out_and_cancelled() {
        let probe = Arc::new(Probe::default());
        let reading = read(
            leaked(UntilCancelled(probe.clone())),
            shell_ok(),
            Arc::new(AtomicBool::new(false)),
        );
        raised(&probe.entered).await;
        // Just short of the deadline the reading is still going, and not cancelled.
        tokio::time::advance(DEADLINE - Duration::from_millis(1)).await;
        // Give the reader's thread turns to observe a cancel raised too early.
        for _ in 0..1000 {
            tokio::task::yield_now().await;
            std::thread::yield_now();
        }
        assert!(!reading.is_finished());
        assert!(!probe.cancelled.load(Ordering::SeqCst));
        tokio::time::advance(Duration::from_millis(1)).await;
        let unavailable = reading.await.unwrap().unwrap_err();
        assert_eq!(unavailable.reason, McpInventoryFailure::TimedOut);
        let detail = unavailable.detail.unwrap();
        assert!(detail.contains(STEP_CLI), "{detail}");
        assert!(!detail.contains(STEP_SHELL), "{detail}");
        // The blocking work was told to stop, not left running.
        raised(&probe.cancelled).await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_shell_snapshot_that_hangs_counts_against_the_overall_deadline() {
        let probe = Arc::new(Probe::default());
        let (entered, release) = (
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        let reading = read(
            leaked(UntilCancelled(probe.clone())),
            shell_held(entered.clone(), release.clone()),
            Arc::new(AtomicBool::new(false)),
        );
        raised(&entered).await;
        tokio::time::advance(DEADLINE).await;
        let unavailable = reading.await.unwrap().unwrap_err();
        assert_eq!(unavailable.reason, McpInventoryFailure::TimedOut);
        let detail = unavailable.detail.unwrap();
        assert!(detail.contains(STEP_SHELL), "{detail}");
        assert!(!detail.contains(STEP_CLI), "{detail}");
        // Once the snapshot ends, the reader that follows it finds the reading already cancelled.
        release.store(true, Ordering::SeqCst);
        raised(&probe.cancelled).await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_reading_whose_future_is_dropped_cancels_its_reader() {
        let probe = Arc::new(Probe::default());
        let reading = read(
            leaked(UntilCancelled(probe.clone())),
            shell_ok(),
            Arc::new(AtomicBool::new(false)),
        );
        raised(&probe.entered).await;
        reading.abort();
        raised(&probe.cancelled).await;
    }

    #[tokio::test(start_paused = true)]
    async fn the_daemon_stopping_cancels_a_reading_under_way() {
        let probe = Arc::new(Probe::default());
        let stopping = Arc::new(AtomicBool::new(false));
        let reading = read(
            leaked(UntilCancelled(probe.clone())),
            shell_ok(),
            stopping.clone(),
        );
        raised(&probe.entered).await;
        assert!(!probe.cancelled.load(Ordering::SeqCst));
        stopping.store(true, Ordering::SeqCst);
        tokio::time::advance(STOP_POLL).await;
        // The reader's own answer comes back, well before the overall deadline.
        let unavailable = reading.await.unwrap().unwrap_err();
        assert_eq!(unavailable.reason, McpInventoryFailure::Failed);
        assert!(probe.cancelled.load(Ordering::SeqCst));
    }

    #[tokio::test(start_paused = true)]
    async fn a_reading_that_answers_in_time_returns_its_listing_uncancelled() {
        let reading = read(
            leaked(Immediate),
            shell_ok(),
            Arc::new(AtomicBool::new(false)),
        );
        let (connectors, servers) = reading.await.unwrap().unwrap();
        assert!(connectors.is_empty() && servers.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn a_shell_snapshot_that_fails_answers_failed_with_its_error() {
        let shell: ShellSource = Arc::new(|| Err(anyhow::anyhow!("no login shell")));
        let reading = read(leaked(Immediate), shell, Arc::new(AtomicBool::new(false)));
        let unavailable = reading.await.unwrap().unwrap_err();
        assert_eq!(unavailable.reason, McpInventoryFailure::Failed);
        assert!(unavailable.detail.unwrap().contains("no login shell"));
    }
}
