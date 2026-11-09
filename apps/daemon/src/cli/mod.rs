//! The command line: the daemon itself when no mode is named; `start`, `stop`, `restart` and
//! `status`, which a user runs to keep the daemon in the background; the modes other processes
//! start it in — an agent's hook, a session's git hooks, a provider's status line and the MCP
//! server — each of which runs in a process of its own and calls the running daemon; and the
//! holder, which the running daemon calls instead.

mod background;
mod holder;
pub(crate) mod hook;
mod installation;

pub(crate) use background::start_successor;
pub(crate) use installation::launch_path;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::protocol::AgentId;
use crate::{adapter, mcp, protocol, revert_gate, session_git_hooks};

#[derive(Parser)]
#[command(name = "octoboardd", args_conflicts_with_subcommands = true)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<Command>,

    /// Stop once this process is gone. The application passes its own pid, so closing or crashing
    /// it never leaves the daemon running with no application: a quit it wrote to the daemon's
    /// stdin first ends every session, anything else hands them over for the next launch.
    #[arg(long)]
    pub(crate) parent_pid: Option<u32>,

    /// Listen on this loopback port instead of one the system assigns, so the address stays the
    /// same across restarts: what a service definition runs, and what `start` starts.
    #[arg(long, value_parser = clap::value_parser!(u16).range(1..))]
    pub(crate) port: Option<u16>,

    /// Read the key the daemon requires of every connection from the first line of stdin instead
    /// of generating one: what a daemon passes the successor it starts after a planned restart, so
    /// that the address and the key stay the same for its clients. The key is never in arguments
    /// or the environment, which other processes can read.
    #[arg(long, hide = true)]
    pub(crate) key_stdin: bool,

    /// Exit with status 75 after a planned restart, for the service manager to start the next
    /// daemon, instead of starting it itself. Pass it only from a service that restarts the daemon
    /// on status 75 (the maintained systemd unit does). Ignored on Windows.
    #[arg(long)]
    pub(crate) service_managed: bool,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Start the daemon in the background on a fixed loopback port and return once it is
    /// listening; it keeps running after the terminal closes, until `stop`.
    Start {
        /// The loopback port the daemon listens on, and keeps across restarts.
        #[arg(long, value_parser = clap::value_parser!(u16).range(1..))]
        port: u16,
    },
    /// Stop the daemon running against this data directory, which ends every session it runs
    /// as interrupted.
    Stop,
    /// Restart the daemon running against this data directory on the same address, keeping every
    /// session it runs running, with its screen, under the daemon that takes over. Refused for a
    /// daemon that follows the desktop application.
    Restart,
    /// Say whether a daemon runs against this data directory, and where it listens; exits 3 when
    /// none does.
    Status,
    /// Forward one agent hook payload to the daemon. This is what the per-session hook script
    /// execs; it is not meant to be run by hand.
    Hook {
        #[arg(long)]
        session: String,
        #[arg(long)]
        port: u16,
        /// The key the daemon requires of every connection.
        #[arg(long)]
        key: String,
    },
    /// Report the branches a session's git has just created, and whether it moved the refs a
    /// project's branch header reads. This is what the session's `reference-transaction` hook
    /// dispatcher starts; it is not meant to be run by hand.
    GitRef {
        #[arg(long)]
        session: String,
        #[arg(long)]
        port: u16,
        /// The key the daemon requires of every connection.
        #[arg(long)]
        key: String,
        /// The branch (`refs/heads/...`) the git that made them had checked out.
        #[arg(long)]
        head: Option<std::ffi::OsString>,
        /// The transaction touched `HEAD`, a local branch or a remote-tracking branch.
        #[arg(long)]
        moved: bool,
        /// Each created reference's name followed by the object id it was created at.
        #[arg(last = true)]
        refs: Vec<std::ffi::OsString>,
    },
    /// Ask the daemon whether the revert gate applies to the hook git is running and, when it
    /// does, run the revert gate in this process with what the daemon gives it; exits 3 to
    /// reject. This is what the session's `reference-transaction` and `pre-push` hook dispatchers
    /// start in the foreground; it is not meant to be run by hand.
    GateHook {
        #[arg(long)]
        session: String,
        #[arg(long)]
        port: u16,
        /// The key the daemon requires of every connection.
        #[arg(long)]
        key: String,
        #[command(subcommand)]
        hook: revert_gate::Hook,
    },
    /// Report a session's context window and run the user's own status line. This is the status
    /// line a provider's adapter injects; it is not meant to be run by hand.
    StatusLine {
        /// The provider whose status line it is. Absent from the command a session launched by an
        /// earlier build is still running, which is the one provider with a status line.
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        session: String,
        #[arg(long)]
        port: u16,
        /// The key the daemon requires of every connection.
        #[arg(long)]
        key: String,
        /// The provider's own arguments, which come last.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Own the terminals the daemon starts programs on, and serve them to the daemon over a local
    /// link. This is what the daemon starts; it is not meant to be run by hand.
    Holder {
        /// The id the holder leaves its registry file under, for the daemon that started it to
        /// find it by.
        #[arg(long)]
        holder_id: String,
        /// The directory of the registry file; the data directory's own when absent.
        #[arg(long)]
        registry_dir: Option<std::path::PathBuf>,
    },
    /// Serve the Octoboard MCP tools for one session over stdio. This is what the adapters
    /// register as the session's MCP server; it is not meant to be run by hand.
    Mcp {
        #[arg(long)]
        session: String,
        #[arg(long)]
        kind: McpKind,
        /// The session is a worker, which narrows what it is offered.
        #[arg(long)]
        worker: bool,
        #[arg(long)]
        port: u16,
        /// The key the daemon requires of every connection.
        #[arg(long)]
        key: String,
        #[arg(long)]
        token: String,
    },
}

/// The `--kind` values the MCP mode accepts. A separate type from `protocol::SessionKind` because
/// clap has to derive a value parser for it, and the protocol types are the wire's, not the CLI's.
#[derive(Clone, Copy, clap::ValueEnum)]
pub(crate) enum McpKind {
    Console,
    Project,
}

impl From<McpKind> for protocol::SessionKind {
    fn from(kind: McpKind) -> Self {
        match kind {
            McpKind::Console => protocol::SessionKind::Console,
            McpKind::Project => protocol::SessionKind::Project,
        }
    }
}

/// Runs the mode `command` names, in place of the daemon.
pub(crate) fn run(command: Command) -> Result<()> {
    match command {
        Command::Start { port } => std::process::exit(background::start(port)?),
        Command::Stop => std::process::exit(background::stop()?),
        Command::Restart => std::process::exit(background::restart()?),
        Command::Status => std::process::exit(background::status()?),
        // The hook path must stay silent and must not pay for a runtime: it is on the agent's
        // turn critical path, several times per turn.
        Command::Hook { session, port, key } => {
            hook::run(&session, port, &key);
            Ok(())
        }
        Command::GitRef {
            session,
            port,
            key,
            head,
            moved,
            refs,
        } => {
            session_git_hooks::ref_mode::run(&session, port, &key, head.as_deref(), moved, &refs);
            Ok(())
        }
        // Both run synchronously inside the user's git, which waits for their exit status.
        Command::GateHook {
            session,
            port,
            key,
            hook,
        } => std::process::exit(session_git_hooks::gate_mode::run(
            &session, port, &key, &hook,
        )),
        // On the path of every status line update, and its stdout is the user's status line's.
        Command::StatusLine {
            provider,
            session,
            port,
            key,
            args,
        } => std::process::exit(run_status_line(
            provider.as_deref(),
            &session,
            port,
            &key,
            &args,
        )),
        // Logs to a file of its own: it has no one to read its stdout and stderr.
        Command::Holder {
            holder_id,
            registry_dir,
        } => holder::run(holder_id, registry_dir),
        // The MCP child owns its stdout for the protocol, so it installs no log subscriber either.
        Command::Mcp {
            session,
            kind,
            worker,
            port,
            key,
            token,
        } => mcp::stdio::run(session, kind.into(), worker, port, key, token),
    }
}

/// Runs `provider`'s status line, or the one provider's that has one when none is named. A provider
/// without one, or an unknown one, has its input drained and nothing shown.
fn run_status_line(
    provider: Option<&str>,
    session: &str,
    port: u16,
    key: &str,
    args: &[String],
) -> i32 {
    let agent = match provider {
        Some(id) => AgentId::from_wire_name(id),
        None => AgentId::ALL
            .into_iter()
            .find(|agent| agent.descriptor().capabilities.status_line),
    };
    match agent.and_then(|agent| adapter::provider(agent).status_line()) {
        Some(status_line) => status_line.run(session, port, key, args),
        None => {
            let _ = std::io::copy(&mut std::io::stdin(), &mut std::io::sink());
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(["octoboardd"].into_iter().chain(args.iter().copied()))
    }

    #[test]
    fn the_daemon_takes_a_port_only_when_one_is_given() {
        assert_eq!(parse(&[]).unwrap().port, None);
        assert_eq!(parse(&["--port", "7531"]).unwrap().port, Some(7531));
        assert!(parse(&["--port", "70000"]).is_err());
        assert!(parse(&["--port", "0"]).is_err());
    }

    #[test]
    fn the_daemon_options_do_not_combine_with_a_mode() {
        assert!(parse(&["--port", "1", "status"]).is_err());
        assert!(parse(&["--parent-pid", "1", "stop"]).is_err());
        assert_eq!(parse(&["--parent-pid", "1"]).unwrap().parent_pid, Some(1));
    }

    #[test]
    fn the_key_is_read_from_stdin_only_when_asked_and_with_no_mode() {
        assert!(!parse(&[]).unwrap().key_stdin);
        let cli = parse(&["--port", "7531", "--key-stdin"]).unwrap();
        assert!(cli.key_stdin);
        assert_eq!(cli.port, Some(7531));
        assert!(parse(&["--key-stdin", "status"]).is_err());
    }

    #[test]
    fn the_service_managed_mode_is_asked_for_and_does_not_combine_with_a_mode() {
        assert!(!parse(&[]).unwrap().service_managed);
        let cli = parse(&["--port", "7531", "--service-managed"]).unwrap();
        assert!(cli.service_managed);
        assert!(parse(&["--service-managed", "status"]).is_err());
    }

    #[test]
    fn restart_takes_no_option() {
        assert!(matches!(
            parse(&["restart"]).unwrap().command,
            Some(Command::Restart)
        ));
        assert!(parse(&["restart", "--port", "7531"]).is_err());
    }

    #[test]
    fn start_needs_a_port_and_stop_and_status_take_none() {
        assert!(parse(&["start"]).is_err());
        assert!(parse(&["start", "--port", "0"]).is_err());
        assert!(matches!(
            parse(&["start", "--port", "7531"]).unwrap().command,
            Some(Command::Start { port: 7531 })
        ));
        assert!(matches!(
            parse(&["stop"]).unwrap().command,
            Some(Command::Stop)
        ));
        assert!(matches!(
            parse(&["status"]).unwrap().command,
            Some(Command::Status)
        ));
    }

    fn status_line(args: &str) -> (Option<String>, Vec<String>) {
        let cli = Cli::try_parse_from(["octoboardd"].into_iter().chain(args.split(' '))).unwrap();
        let Some(Command::StatusLine {
            provider,
            session,
            port,
            key,
            args,
        }) = cli.command
        else {
            panic!("not the status-line command");
        };
        assert_eq!((session.as_str(), port, key.as_str()), ("s", 1, "k"));
        (provider, args)
    }

    /// The command without `--provider` is an earlier build's, which only Claude Code's launches
    /// injected: it resolves to the one provider declaring a status line, which has to stay the
    /// only one and be Claude Code, or such a session's status line would go to another provider.
    #[test]
    fn exactly_one_provider_declares_a_status_line_and_it_is_claude_code() {
        let declaring: Vec<_> = AgentId::ALL
            .into_iter()
            .filter(|agent| agent.descriptor().capabilities.status_line)
            .collect();
        assert_eq!(
            declaring,
            [AgentId::CLAUDE],
            "a second provider with a status line needs the command without `--provider` resolved \
             to Claude Code explicitly"
        );
        assert!(adapter::provider(AgentId::CLAUDE).status_line().is_some());
    }

    /// The command of a session launched before `--provider` existed, and still running, parses,
    /// its provider's own arguments passed on as they stand.
    #[test]
    fn the_status_line_command_parses_with_and_without_a_provider() {
        let own = ["--cwd", "/p", "--config-dir", "/c"]
            .map(str::to_owned)
            .to_vec();
        assert_eq!(
            status_line("status-line --session s --port 1 --key k --cwd /p --config-dir /c"),
            (None, own.clone())
        );
        assert_eq!(
            status_line(
                "status-line --provider claude --session s --port 1 --key k --cwd /p --config-dir /c"
            ),
            (Some("claude".to_owned()), own)
        );
    }
}
