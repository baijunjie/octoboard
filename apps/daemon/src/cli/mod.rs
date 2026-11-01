//! The command line: the daemon itself when no mode is named, and the modes other processes start
//! it in — an agent's hook, a session's git hooks, the revert gate, the Claude Code status line and
//! the MCP server — each of which runs in a process of its own. All but the revert gate call the
//! running daemon; the revert gate runs the gate directly from its arguments.

pub(crate) mod hook;
pub(crate) mod loopback;
pub(crate) mod status_line;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::{mcp, protocol, revert_gate, session_git_hooks};

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
    /// Decide whether a `reference-transaction` hook's ref updates, or with `pre-push` the refs
    /// of a push, undo changes already merged into a guarded branch; exits 3 to reject. Runs the
    /// gate directly with the guarded branches, target branches and fork points given as
    /// arguments; the session's hook dispatchers go through `gate-hook` instead, which asks the
    /// daemon for them.
    RevertGate(revert_gate::Args),
    /// Report a Claude Code session's context window and run the user's own status line. This is
    /// the status line the Claude Code adapter injects; it is not meant to be run by hand.
    StatusLine {
        #[arg(long)]
        session: String,
        #[arg(long)]
        port: u16,
        /// The key the daemon requires of every connection.
        #[arg(long)]
        key: String,
        /// The session's working directory, whose project settings name the user's status line.
        #[arg(long)]
        cwd: std::path::PathBuf,
        /// The Claude Code configuration directory whose user settings name it.
        #[arg(long)]
        config_dir: std::path::PathBuf,
    },
    /// Serve the Octoboard MCP tools for one session over stdio. This is what the adapters
    /// register as the session's MCP server; it is not meant to be run by hand.
    Mcp {
        #[arg(long)]
        session: String,
        #[arg(long)]
        role: McpRole,
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

/// The `--role` values the MCP mode accepts. A separate type from `protocol::Role` because clap
/// has to derive a value parser for it, and the protocol types are the wire's, not the CLI's.
#[derive(Clone, Copy, clap::ValueEnum)]
pub(crate) enum McpRole {
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
        Command::RevertGate(args) => std::process::exit(revert_gate::run(&args)),
        // On the path of every status line update, and its stdout is the user's status line's.
        Command::StatusLine {
            session,
            port,
            key,
            cwd,
            config_dir,
        } => std::process::exit(status_line::run(&session, port, &key, &cwd, &config_dir)),
        // The MCP child owns its stdout for the protocol, so it installs no log subscriber either.
        Command::Mcp {
            session,
            role,
            worker,
            port,
            key,
            token,
        } => mcp::stdio::run(session, role.into(), worker, port, key, token),
    }
}
