//! `obd-proto` — throwaway validation prototype daemon for Octoboard milestone 00. See
//! `prototype/README.md` and `prototype/PROTOCOL.md`.

mod adapter;
mod env_shell;
mod http_client;
mod mcp_stdio;
mod probe;
mod protocol;
mod ringbuf;
mod server;
mod state;
mod term;
mod watchdog;

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

use protocol::Role;
use state::AppState;

#[derive(Parser)]
#[command(name = "obd-proto")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Exit once this process is gone (T4). Polled roughly once a second.
    #[arg(long)]
    parent_pid: Option<u32>,

    /// Launch agents via `$SHELL -l -i -c '<command>'` instead of exec'ing the resolved binary
    /// directly with the snapshotted environment. Kept as a fallback for comparing the two
    /// launch strategies; the environment filtering is kept equivalent to the default path.
    #[arg(long)]
    login_shell_launch: bool,

    /// Run the self-test (see `probe.rs`) instead of serving the daemon, and exit with its
    /// pass/fail status.
    #[arg(long)]
    probe: bool,

    /// Size, in KiB, of the buffer the PTY reader thread reads into (term.rs's `spawn_session`).
    /// Defaults to today's fixed 8 KiB so nothing changes unless this is passed explicitly. Exists
    /// only so T2's bench (`prototype/bench/output_throughput.mjs`) can sweep this the same way
    /// the real product's implementer eventually would: this buffer size is what bounds the size
    /// of every daemon-to-client terminal frame, which T2 originally left unmeasured.
    #[arg(long, default_value_t = 8)]
    pty_read_buf_kib: u32,
}

#[derive(Subcommand)]
enum Command {
    /// Stdio MCP server mode, launched by the agent CLI itself via `--mcp-config`.
    Mcp {
        #[arg(long)]
        session: String,
        #[arg(long)]
        role: RoleArg,
        #[arg(long)]
        daemon: String,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum RoleArg {
    Hub,
    Worker,
}

impl From<RoleArg> for Role {
    fn from(value: RoleArg) -> Self {
        match value {
            RoleArg::Hub => Role::Hub,
            RoleArg::Worker => Role::Worker,
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // stdout carries the MCP JSON-RPC stream in `mcp` mode and the port handshake line in
    // daemon mode; tracing must never share either channel, so logs always go to stderr.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();
    watchdog::install_panic_hook();

    if let Some(Command::Mcp {
        session,
        role,
        daemon,
    }) = cli.command
    {
        return mcp_stdio::run(mcp_stdio::McpArgs {
            session,
            role: role.into(),
            daemon,
        });
    }

    let pty_read_buf_bytes = cli.pty_read_buf_kib as usize * 1024;

    if cli.probe {
        let result = probe::run(cli.login_shell_launch, pty_read_buf_bytes).await;
        state::kill_all_children();
        return result;
    }

    run_daemon(cli.parent_pid, cli.login_shell_launch, pty_read_buf_bytes).await
}

async fn run_daemon(
    parent_pid: Option<u32>,
    login_shell_launch: bool,
    pty_read_buf_bytes: usize,
) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    println!("obd-proto listening on 127.0.0.1:{port}");
    write_port_file(port)?;

    let self_exe = std::env::current_exe()?.to_string_lossy().into_owned();
    let state = std::sync::Arc::new(AppState::new(
        port,
        self_exe,
        login_shell_launch,
        pty_read_buf_bytes,
    ));

    if let Some(parent_pid) = parent_pid {
        watchdog::spawn(parent_pid);
    }
    watchdog::spawn_signal_handler();

    let router = server::router(state);
    if let Err(err) = axum::serve(listener, router).await {
        tracing::error!(%err, "axum::serve exited with an error, killing all agent processes");
        state::kill_all_children();
        return Err(err.into());
    }
    Ok(())
}

fn write_port_file(port: u16) -> anyhow::Result<()> {
    let path: PathBuf = std::env::temp_dir().join("obd-proto.port");
    std::fs::write(path, port.to_string())?;
    Ok(())
}
