//! `octoboardd` — the Octoboard daemon. It plays two roles in one process: the host role (PTYs,
//! agent processes, directories and repositories) and the coordinator role (the stored consoles,
//! projects and sessions). The desktop application is purely a client of it, over the protocol in
//! `apps/daemon/PROTOCOL.md`.

mod accounts;
mod adapter;
mod agents;
mod blocking_pass;
mod branch_cleanup;
mod branch_origin;
mod browse;
mod cli;
mod config_document;
mod coordinator;
mod env_shell;
mod git;
mod git_workflow;
mod handoff;
mod hooks;
mod host_open;
mod host_tools;
mod hostfs;
mod instance_lock;
mod interface_language;
#[cfg(test)]
mod layering_tests;
mod log_file;
mod loopback;
mod mcp;
mod messaging;
mod path_links;
mod paths;
mod processes;
mod project_git;
mod project_trash;
mod protocol;
mod records;
mod revert_gate;
mod rfc3339;
mod server;
mod session;
mod session_git_hooks;
mod session_request;
mod state;
mod store;
mod terminal;
#[cfg(test)]
mod test_support;
mod titles;
mod trash_safety;
mod wire_path;

use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;

use state::AppState;
use store::Store;

/// The product name, from `config/app.json` (see `build.rs`), for the wording the daemon itself
/// puts in front of the user.
pub(crate) const APP_NAME: &str = env!("OCTOBOARD_APP_NAME");

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    if let Some(command) = cli.command {
        return cli::run(command);
    }

    // While this is still the only thread.
    processes::ledger::forget_inherited_tag();
    let log_file = init_logging();
    processes::crash_cleanup::install();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run_daemon(cli.parent_pid, log_file))
}

/// Sends the daemon's log, at `INFO` and above, to stderr and to `daemon.log` (`log_file`). Never
/// to stdout: that carries the handshake line (port and key) the application reads. The file is
/// written only once the returned handle is enabled, which the daemon does when it holds the
/// instance lock.
///
/// The application owns the read end of that stderr, so from the moment it is gone every write to
/// the log fails. Reporting those failures is therefore turned off: the report is an `eprintln!`
/// of its own, which panics on the same dead stderr, and that panic would take out whichever task
/// logged — the parent watch among them, leaving the daemon running with no application (see
/// `spawn_parent_watch`). A log that cannot be written is dropped instead. The file is what
/// remains of the log then, and the only one a packaged application ever has: launched from the
/// Finder, it has no stderr to read at all.
fn init_logging() -> log_file::LogFile {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::{fmt, EnvFilter};

    let log_file = log_file::LogFile::open();
    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_writer(std::io::stderr)
                .log_internal_errors(false),
        )
        .with(
            fmt::layer()
                .with_writer(log_file.clone())
                .with_ansi(false)
                .log_internal_errors(false),
        )
        .with(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .init();
    log_file
}

async fn run_daemon(parent_pid: Option<u32>, log_file: log_file::LogFile) -> Result<()> {
    // Held for the whole run: see `instance_lock` for what a second daemon would do to the first
    // one's sessions.
    let _lock = instance_lock::acquire()?;
    // Only the daemon that holds the lock writes `daemon.log`; a second one has only stderr.
    log_file.enable();
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        pid = std::process::id(),
        "octoboardd starting"
    );

    let store = Store::open(&paths::db_path())?;
    // Nothing the previous daemon ran survived it, whatever the stored status said.
    let interrupted = store.mark_live_sessions_interrupted()?;
    if interrupted > 0 {
        tracing::info!(
            interrupted,
            "sessions left by a previous daemon were marked interrupted"
        );
    }
    clear_run_dir();
    // A switch that died with the previous daemon may have left a staging directory in an account's
    // config directory. The default accounts' directories are not known until the shell has been
    // read, so only the ones Octoboard has been told of are swept.
    let known_dirs: Vec<String> = store
        .list_accounts()?
        .into_iter()
        .map(|account| account.config_dir)
        .chain(
            store
                .list_sessions()?
                .into_iter()
                .filter_map(|session| session.config_dir),
        )
        .collect();
    accounts::relocate::sweep_staging(known_dirs.iter().map(std::path::Path::new));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let key = server::access::generate_key().context("generating the daemon's key")?;
    let self_exe = std::env::current_exe()?.to_string_lossy().into_owned();
    let state = Arc::new(AppState::new(
        store,
        port,
        key.clone(),
        self_exe,
        paths::saved_output_dir(),
        paths::consoles_dir(),
        paths::handoff_dir(),
        processes::ledger::ProcessLedger::new(&paths::data_dir()),
    ));
    // Saved output whose session is gone (a daemon that died between deleting the record and the
    // file) or whose write was cut short has nothing else to remove it.
    state.sweep_saved_output()?;
    // Likewise for a handoff package whose session is gone.
    handoff::sweep(&state)?;
    // Pages a session pushed before it took the bound role, which no tool of its can list or
    // delete, and which a failed withdrawal or a push racing a binding may have left.
    if let Err(err) = coordinator::pages::withdraw_stray_pages(&state) {
        tracing::warn!(%err, "withdrawing the pages of bound sessions failed");
    }

    // Printed before anything else can block: the application waits for this line to know where to
    // connect and what to present. Besides this line the key is in memory and in what the daemon
    // starts (their arguments and environment, and the owner-only files of a session's scratch
    // directory): the port file below stays port-only.
    println!("octoboardd listening on 127.0.0.1:{port} with key {key}");
    use std::io::Write;
    std::io::stdout().flush().ok();
    write_port_file(port)?;

    if let Some(parent_pid) = parent_pid {
        spawn_parent_watch(state.clone(), parent_pid);
    }

    // Off the runtime rather than ahead of the line above: a ten-second shell snapshot must not
    // put that cost on every application launch, and clients are served before it lands either way
    // (see `apps/daemon/src/agents/availability.rs`).
    agents::availability::spawn_determine(state.clone());
    records::collector::spawn(&state);
    accounts::quotas::spawn(&state);
    coordinator::archive_cleanup::spawn(&state);
    // Likewise off the runtime: it runs `git` in every project's directory, a few at a time.
    project_git::remote::reconcile_remote_urls(&state);
    project_git::watch::start(&state);
    // On a thread of its own: what the previous run's agents left is stopped without holding up
    // anything else. A launch meanwhile is safe from it, since its launch id is reserved before its
    // agent starts.
    state.process_ledger.start(true);

    let serve = axum::serve(listener, server::router(state.clone()));
    tokio::select! {
        result = serve => {
            if let Err(err) = result {
                tracing::error!(%err, "the server stopped with an error");
                stop_all_processes(&state).await;
                return Err(err.into());
            }
        }
        _ = state.await_shutdown() => {
            tracing::info!("shutdown requested");
        }
        _ = shutdown_signal() => {
            tracing::info!("a termination signal arrived");
        }
    }

    stop_all_processes(&state).await;
    remove_port_file();
    Ok(())
}

/// Every login, agent process, shell, quota probe and quota kick is the daemon's child, so none of
/// them may outlive it. Each session is left interrupted, which is the status a click resumes from;
/// each shell terminal ends with its record.
async fn stop_all_processes(state: &Arc<AppState>) {
    state.account_quotas.stop_cli_runs();
    state.account_logins.stop_all().await;
    tokio::join!(state.stop_all_sessions(), terminal::shell::close_all(state));
}

/// Polls the application's pid and ends the daemon once it is gone. The daemon lives and dies with
/// the application; only this watch stands in the way of background operation.
fn spawn_parent_watch(state: Arc<AppState>, parent_pid: u32) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            // SAFETY: signal 0 only probes whether the pid exists.
            if unsafe { libc::kill(parent_pid as i32, 0) } != 0
                && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
            {
                tracing::info!(parent_pid, "the application is gone; shutting down");
                state.request_shutdown();
                break;
            }
        }
    });
}

async fn shutdown_signal() {
    let mut sigterm = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
    {
        Ok(signal) => signal,
        Err(err) => {
            tracing::warn!(%err, "installing the SIGTERM handler failed");
            return std::future::pending().await;
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = sigterm.recv() => {}
    }
}

/// Removes the per-session scratch directories of sessions that no longer exist. Every directory
/// under it belonged to a session of a daemon that has already exited.
fn clear_run_dir() {
    let run_dir = paths::run_dir();
    if let Err(err) = std::fs::remove_dir_all(&run_dir) {
        if err.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(path = %run_dir.display(), %err, "clearing the run directory failed");
        }
    }
}

/// The port and key are on stdout for the application. The port alone is in this file, for a
/// developer attaching to a daemon started by hand, who reads the key from the startup line: the
/// key is not in this file.
fn write_port_file(port: u16) -> Result<()> {
    std::fs::write(port_file_path(), port.to_string())
        .with_context(|| format!("writing {}", port_file_path().display()))?;
    Ok(())
}

/// Removed on the way out, so nothing points at a port this daemon no longer holds.
fn remove_port_file() {
    if let Err(err) = std::fs::remove_file(port_file_path()) {
        if err.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(%err, "removing the port file failed");
        }
    }
}

fn port_file_path() -> std::path::PathBuf {
    std::env::temp_dir().join("octoboardd.port")
}

#[cfg(test)]
mod tests {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    use crate::test_support::{app_state, ScratchDir, PATIENCE};

    /// Where [`parent_watch_child`] records that the watch got as far as the shutdown. Its presence
    /// in the environment is also what tells that case it is being run as one.
    const DONE_FILE_VAR: &str = "OCTOBOARDD_TEST_PARENT_WATCH_DONE";

    /// The parent watch ends the daemon even when the log line it writes on the way cannot be
    /// written. The application owns the read end of the daemon's stderr, so from its death every
    /// such write fails, which is the state this test puts the child in by closing the read end of
    /// its stderr.
    #[test]
    fn the_parent_watch_asks_for_the_shutdown_with_a_dead_log() {
        let dir = ScratchDir::new("parent-watch");
        let done = dir.join("done");
        let log = dir.join("child.log");
        let mut child = Command::new(std::env::current_exe().expect("the binary"))
            .args([
                "--exact",
                "tests::parent_watch_child",
                "--ignored",
                // The case has to run uncaptured: libtest's capture would take the subscriber's own
                // `eprintln!` about the failed write, which is the very thing that must land on the
                // dead stderr, and the test would pass whether that report is turned off or not.
                "--nocapture",
                "--test-threads=1",
            ])
            .env(DONE_FILE_VAR, &done)
            .stdout(std::fs::File::create(&log).expect("the child's log"))
            .stderr(Stdio::piped())
            .spawn()
            .expect("the child test started");
        drop(child.stderr.take());

        let deadline = Instant::now() + PATIENCE;
        let status = loop {
            match child.try_wait().expect("the child was waited on") {
                Some(status) => break status,
                None if Instant::now() >= deadline => {
                    child.kill().ok();
                    panic!("the child never ended; it said {}", said(&log));
                }
                None => std::thread::sleep(Duration::from_millis(50)),
            }
        };
        assert!(
            status.success(),
            "the child ended with {status}; it said {}",
            said(&log)
        );
        // A child that exits cleanly without having run the case at all — a filter that matched
        // nothing, because the case was renamed or moved — would otherwise read as a pass.
        assert!(
            done.exists(),
            "the child never ran the case; it said {}",
            said(&log)
        );
    }

    /// What the child wrote where libtest reports its result, for a failure to be read by.
    fn said(log: &std::path::Path) -> String {
        std::fs::read_to_string(log).unwrap_or_else(|err| format!("<unreadable: {err}>"))
    }

    /// The case [`the_parent_watch_asks_for_the_shutdown_with_a_dead_log`] runs in a child process;
    /// does nothing when run any other way. It has to be a process of its own in any case: it
    /// installs the daemon's own log subscriber, which is installed once per process.
    #[test]
    #[ignore = "run in a child process by the_parent_watch_asks_for_the_shutdown_with_a_dead_log"]
    fn parent_watch_child() {
        let Ok(done) = std::env::var(DONE_FILE_VAR) else {
            return;
        };
        super::init_logging();
        let (state, _dir) = app_state("parent-watch-child");
        // Stands in for the application: spawned and reaped here, so the watch finds its pid gone
        // the first time it looks.
        let mut gone = Command::new("/usr/bin/true")
            .spawn()
            .expect("the stand-in application ran");
        let parent_pid = gone.id();
        gone.wait().expect("the stand-in application was reaped");

        // Unbounded on purpose: the parent bounds the whole case by `PATIENCE`, and a second
        // deadline inside it would only ever expire after that one.
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("a runtime")
            .block_on(async {
                super::spawn_parent_watch(state.clone(), parent_pid);
                state.await_shutdown().await;
            });
        std::fs::write(&done, "reached").expect("the marker was written");
    }
}
