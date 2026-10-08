//! `octoboardd` — the Octoboard daemon. It plays two roles in one process: the host role (PTYs,
//! agent processes, directories and repositories) and the coordinator role (the stored consoles,
//! projects and sessions). The desktop application is purely a client of it, over the protocol in
//! `apps/daemon/PROTOCOL.md`.

mod access;
mod adapter;
mod availability;
mod coordinator;
mod env_shell;
mod git_status;
mod hook_mode;
mod hooks;
mod hostfs;
mod instance_lock;
mod loopback;
mod mcp;
mod outbox;
mod paths;
mod protocol;
mod ptyio;
mod reporting;
mod ringbuf;
mod server;
mod session;
mod state;
mod store;
mod term;
mod transcript;
mod trust;

use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use state::AppState;
use store::Store;

/// The product name, from `config/app.json` (see `build.rs`), for the wording the daemon itself
/// puts in front of the user.
pub(crate) const APP_NAME: &str = env!("OCTOBOARD_APP_NAME");

#[derive(Parser)]
#[command(name = "octoboardd")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Exit once this process is gone. The application passes its own pid, so closing or crashing
    /// it never leaves the daemon and its agent processes behind.
    #[arg(long)]
    parent_pid: Option<u32>,
}

#[derive(Subcommand)]
enum Command {
    /// Forward one agent hook payload to the daemon. This is what the per-session hook script
    /// execs; it is not meant to be run by hand.
    Hook {
        #[arg(long)]
        session: String,
        #[arg(long)]
        port: u16,
    },
    /// Serve the Octoboard MCP tools for one session over stdio. This is what the adapters
    /// register as the session's MCP server; it is not meant to be run by hand.
    Mcp {
        #[arg(long)]
        session: String,
        #[arg(long)]
        role: McpRole,
        #[arg(long)]
        port: u16,
        #[arg(long)]
        token: String,
    },
}

/// The `--role` values the MCP mode accepts. A separate type from `protocol::Role` because clap
/// has to derive a value parser for it, and the protocol types are the wire's, not the CLI's.
#[derive(Clone, Copy, clap::ValueEnum)]
enum McpRole {
    Console,
    Project,
}

impl From<McpRole> for protocol::Role {
    fn from(role: McpRole) -> Self {
        match role {
            McpRole::Console => protocol::Role::Console,
            McpRole::Project => protocol::Role::Project,
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // The hook path must stay silent and must not pay for a runtime: it is on the agent's turn
    // critical path, several times per turn.
    if let Some(Command::Hook { session, port }) = &cli.command {
        hook_mode::run(session, *port);
        return Ok(());
    }

    // The MCP child owns its stdout for the protocol, so it installs no log subscriber either.
    if let Some(Command::Mcp {
        session,
        role,
        port,
        token,
    }) = cli.command
    {
        return mcp::stdio::run(session, role.into(), port, token);
    }

    // stdout carries the port handshake line the application reads, so logs always go to stderr.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();
    state::install_panic_hook();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run_daemon(cli.parent_pid))
}

async fn run_daemon(parent_pid: Option<u32>) -> Result<()> {
    // Held for the whole run: see `instance_lock` for what a second daemon would do to the first
    // one's sessions.
    let _lock = instance_lock::acquire()?;

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

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let self_exe = std::env::current_exe()?.to_string_lossy().into_owned();
    let state = Arc::new(AppState::new(store, port, self_exe));

    // Printed before anything else can block: the application waits for this line to know where to
    // connect.
    println!("octoboardd listening on 127.0.0.1:{port}");
    use std::io::Write;
    std::io::stdout().flush().ok();
    write_port_file(port)?;

    if let Some(parent_pid) = parent_pid {
        spawn_parent_watch(state.clone(), parent_pid);
    }

    // Off the runtime rather than ahead of the line above: a ten-second shell snapshot must not
    // put that cost on every application launch, and clients are served before it lands either way
    // (see `apps/daemon/src/availability.rs`).
    availability::spawn_determine(state.clone());

    let serve = axum::serve(listener, server::router(state.clone()));
    tokio::select! {
        result = serve => {
            if let Err(err) = result {
                tracing::error!(%err, "the server stopped with an error");
                state.stop_all_sessions().await;
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

    // Every agent process is the daemon's child, so none of them may outlive it. Each is left
    // interrupted, which is the status a click resumes from.
    state.stop_all_sessions().await;
    remove_port_file();
    Ok(())
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

/// The port is on stdout for the application, and in this file for a developer attaching to a
/// daemon started by hand.
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
