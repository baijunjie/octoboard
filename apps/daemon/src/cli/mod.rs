//! The command line: the daemon itself when no mode is named, and the modes other processes start
//! it in — an agent's hook, a session's git hooks, a provider's status line and the MCP server —
//! each of which runs in a process of its own and calls the running daemon.

pub(crate) mod hook;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::protocol::AgentId;
use crate::{adapter, mcp, protocol, revert_gate, session_git_hooks};

#[derive(Parser)]
#[command(name = "octoboardd")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<Command>,

    /// Exit once this process is gone. The application passes its own pid, so closing or crashing
    /// it never leaves the daemon and its agent processes behind.
    #[arg(long)]
    pub(crate) parent_pid: Option<u32>,
}

#[derive(Subcommand)]
pub(crate) enum Command {
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
