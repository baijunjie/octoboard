//! The command line: the daemon itself when no mode is named; `start`, `stop`, `restart`, `status`
//! and `address`, which a user runs to keep the daemon in the background; `uninstall`, which
//! removes the server form's installation; the modes other processes
//! start it in — an agent's hook, a session's git hooks, a provider's status line and the MCP
//! server — each of which runs in a process of its own and calls the running daemon, and an
//! agent's cross-project guard, which decides from a file alone; and the holder, which the running
//! daemon calls instead.

mod address;
mod application;
mod background;
#[cfg(all(test, unix))]
mod background_tests;
mod browser;
pub(crate) mod channel;
mod cleanup;
mod connection;
mod holder;
pub(crate) mod hook;
mod installation;
#[cfg(test)]
mod key_source_tests;
mod owner_endpoint;
mod restart_check;
#[cfg(all(test, unix))]
mod restart_tests;
#[cfg(all(test, unix))]
mod stop_and_browser_tests;
mod stop_guard;
mod succession;
mod uninstall;
pub(crate) mod update;
mod verify_archive;

pub(crate) use background::start_successor;
pub(crate) use installation::launch_path;
pub(crate) use succession::handed_over_from;

use std::path::Path;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::protocol::AgentId;
use crate::{adapter, cross_project_guard, mcp, protocol, revert_gate, session_git_hooks};

#[derive(Parser)]
#[command(name = "octoboardd", version, args_conflicts_with_subcommands = true)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<Command>,

    /// Stop once this process is gone. The application passes its own pid, so closing or crashing
    /// it never leaves the daemon running with no application: a quit it wrote to the daemon's
    /// stdin first ends every session, anything else hands them over for the next launch.
    #[arg(long)]
    pub(crate) parent_pid: Option<u32>,

    /// Listen on this loopback port instead of one the system assigns, so that it stays the same
    /// across starts; the browser port (`octoboardd browser`) is another.
    #[arg(long, value_parser = clap::value_parser!(u16).range(1..))]
    pub(crate) port: Option<u16>,

    /// Read the key the daemon requires of every connection from the first line of stdin instead
    /// of generating one: what a daemon passes the successor it starts after a planned restart, so
    /// that the address and the key stay the same for its clients. The key is never in arguments
    /// or the environment, which other processes can read.
    #[arg(long, hide = true)]
    pub(crate) key_stdin: bool,

    /// Take over the listening socket inherited as this descriptor instead of binding the port:
    /// what a daemon passes the successor it starts after a planned restart, so that the port is
    /// never unbound between the two. A descriptor that is not a listener on `--port` is left as it
    /// is and the port bound as usual.
    #[arg(long, hide = true, requires = "port")]
    pub(crate) listen_fd: Option<i32>,

    /// Exit with status 75 after a planned restart, for the service manager to start the next
    /// daemon, instead of starting it itself. Pass it only from a service that restarts the daemon
    /// on status 75 (the maintained systemd unit does). Ignored on Windows.
    #[arg(long)]
    pub(crate) service_managed: bool,

    /// Run only to clear what the daemon's features have in effect outside Octoboard's own
    /// directories, for whoever is uninstalling Octoboard and stops the daemon again afterwards
    /// (`crate::state::StartWork` lists what such a daemon leaves out). What the desktop
    /// application's Windows uninstaller and `octoboardd uninstall` start it with.
    #[arg(long = CLEAN_UP_RUN_OPTION, hide = true)]
    pub(crate) clean_up_run: bool,

    /// Stop, handing the sessions back to their holders, once this process is gone: the command
    /// that started the daemon for a clean-up run (`octoboardd uninstall`), which the daemon
    /// never outlives, named by its pid and its start time (`<pid>:<start>`) so that a process
    /// given the pid later is not taken for it. Unlike `--parent-pid` it does not make the daemon
    /// an application's.
    #[arg(
        long,
        hide = true,
        requires = "clean_up_run",
        conflicts_with = "parent_pid",
        value_parser = parse_followed
    )]
    pub(crate) follow_process: Option<crate::platform::proc::ProcessIdentity>,
}

/// The option that names the process a clean-up run follows.
pub(crate) const FOLLOW_PROCESS_ARGUMENT: &str = "--follow-process";

fn parse_followed(text: &str) -> Result<crate::platform::proc::ProcessIdentity, String> {
    crate::platform::proc::ProcessIdentity::parse(text)
        .ok_or_else(|| "expected `<pid>:<start>`".to_owned())
}

/// The name of the option that starts the daemon for a clean-up run, without its dashes.
const CLEAN_UP_RUN_OPTION: &str = "clean-up-run";

/// The argument that starts the daemon for a clean-up run.
pub(crate) fn clean_up_run_argument() -> String {
    format!("--{CLEAN_UP_RUN_OPTION}")
}

/// The line the daemon prints once it listens, which the desktop application reads for the port and
/// key and `octoboardd start` shows.
pub(crate) fn listening_line(port: u16, key: &str) -> String {
    format!("octoboardd listening on 127.0.0.1:{port} with key {key}")
}

/// The port a line made by [`listening_line`] names.
pub(crate) fn listening_port(line: &str) -> Option<u16> {
    line.strip_prefix("octoboardd listening on 127.0.0.1:")?
        .split(' ')
        .next()?
        .parse()
        .ok()
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Start the daemon in the background and return once it is listening; it keeps running
    /// after the terminal closes, until `stop`.
    Start {
        /// The loopback port the daemon listens on, and keeps across restarts; one the system
        /// assigns when absent. The browser port (`octoboardd browser`) is another.
        #[arg(long, value_parser = clap::value_parser!(u16).range(1..))]
        port: Option<u16>,
    },
    /// Stop the daemon running against this data directory, which ends every session it runs
    /// as interrupted. Where that would interrupt work (a session the application's quit
    /// confirmation asks about, or a running shell terminal) it says what runs, stops nothing and
    /// exits with status 1. On macOS, the desktop application's own copy quits the application instead.
    Stop {
        /// Stop regardless of the work it would interrupt.
        #[arg(long)]
        force: bool,
    },
    /// Restart the daemon running against this data directory, keeping every session it runs
    /// running, with its screen, under the daemon that takes over; the browser address, which
    /// carries the owner key and the browser port, stays the same, and so does the daemon's own
    /// port unless a service manager restarts it without `--port`, which takes a new one. Where the
    /// daemon this executable would start cannot take some of them over (a process holder older
    /// than it adopts), it says which and asks first when run in a terminal; run otherwise, it
    /// restarts regardless. Refused for a daemon that follows the desktop application.
    Restart {
        /// Restart without asking, whatever the restart would interrupt.
        #[arg(long, conflicts_with = "if_safe")]
        yes: bool,
        /// Restart only when it would interrupt nothing; otherwise say what it would interrupt and
        /// exit with status 4 without restarting.
        #[arg(long)]
        if_safe: bool,
    },
    /// Say whether a daemon runs against this data directory, and where it listens; exits 3 when
    /// none does.
    Status,
    /// Download the latest daemon, check it against the release key and replace this executable
    /// with it, keeping the previous one as `octoboardd.previous`, then restart the running daemon
    /// into it as `octoboardd restart` does, keeping every session running. Where the restart
    /// would interrupt some (a process holder older than the new build adopts), it says which and
    /// asks first when run in a terminal, and run otherwise leaves the restart for later. Where
    /// the user may not write to the executable's directory, upgrade by hand with the install
    /// script instead. Refused for a daemon that follows the desktop application.
    Update {
        /// Only check: exits 0 when up to date, 10 when a newer daemon is published and 1 when the
        /// check failed.
        #[arg(long, conflicts_with = "rollback")]
        check: bool,
        /// Put the previous build back, by renaming `octoboardd.previous` over the executable
        /// (what was replaced becomes the previous one), instead of updating.
        #[arg(long, conflicts_with_all = ["check", "channel"])]
        rollback: bool,
        /// The release line to look in, instead of the one the daemon's settings name: `stable` or
        /// `beta`.
        #[arg(long, value_parser = parse_channel)]
        channel: Option<protocol::DaemonUpdateChannel>,
        /// Restart the running daemon without asking, whatever the restart would interrupt.
        #[arg(long, conflicts_with_all = ["check", "no_restart"])]
        yes: bool,
        /// Only replace the executable: leave the running daemon on the build it runs until
        /// `octoboardd restart`.
        #[arg(long, conflicts_with = "check")]
        no_restart: bool,
    },
    /// Check that a downloaded release archive is the one the release key signed and that the
    /// signature names the archive's file name; exits 0 when it is and 1 when it is not, changing
    /// nothing. This is what the install script runs, with the installed daemon, where `minisign`
    /// is not installed; it is not meant to be run by hand.
    #[command(hide = true)]
    VerifyArchive {
        /// The archive, saved under the name the release gave it.
        archive: std::path::PathBuf,
        /// A file holding the manifest's `signature` (or the minisign signature file).
        signature: std::path::PathBuf,
    },
    /// Print the address to open the running daemon at in a browser, its key included:
    /// `http://127.0.0.1:<port>/?key=<key>`, and on stderr the SSH forward that reaches it. Exits
    /// 3 when none runs. The key is the owner key, which stays the same across launches, when the
    /// daemon accepts one (a daemon that serves the browser on its way in), else the launch key.
    Address {
        /// Replace the owner key first, so the address printed is the new one's and an earlier
        /// address stops working. Where no daemon runs, or the running one takes no owner key,
        /// the file alone is replaced, for the next daemon that serves the browser.
        #[arg(long)]
        rotate: bool,
        /// Print instead, for a program to read, one line of JSON with the owner key
        /// (`owner_key`), the protocol generation (`generation`) and the release version
        /// (`daemon_version`) the running daemon states on its owner endpoint, once it has accepted
        /// the key there. Exits 3 when no daemon runs, 4 when it has no owner endpoint, 5 when the
        /// endpoint cannot be reached and 6 when this is the desktop application's own copy and the
        /// application is not running.
        #[arg(long, conflicts_with = "rotate")]
        json: bool,
    },
    /// Turn the running daemon's browser access on or off, or say where it stands: the setting
    /// Settings changes, so a change is made through the running daemon and applies at once. With
    /// no daemon running, it says so and changes nothing.
    Browser {
        #[command(subcommand)]
        action: browser::Action,
    },
    /// Uninstall this octoboardd: first clear what Octoboard's features have in effect on this
    /// machine outside its own directories (each account that shares its agent's default
    /// configuration stops sharing; the running daemon does it, and is started for it where none
    /// runs), then stop the daemon, then remove what the install set up: the systemd user unit,
    /// lingering where the install turned it on and no other service of yours needs it, this
    /// executable, `octoboardd.previous` and what an update left beside them. Where something
    /// stays in effect, or the stop would interrupt work, nothing is removed. ~/.octoboard, the
    /// daemon's data, is left, with the command that removes it. In a terminal it says what it
    /// will do and asks for a word to be typed first. Refused for the desktop application's own
    /// daemon, which is uninstalled with the application. Exits 0 once uninstalled, or with
    /// `--dry-run`, and 1 when nothing or not everything was removed.
    Uninstall {
        /// Uninstall without asking: needed where there is no terminal to ask in.
        #[arg(long)]
        yes: bool,
        /// Stop the daemon whatever work that interrupts, as `stop --force` does.
        #[arg(long)]
        force: bool,
        /// Do not clear first what Octoboard's features have in effect outside its own
        /// directories: it stays in place (the links in a sharing account's directory that lead
        /// into its agent's default account's directory, and the backups of its own configuration
        /// that sharing set aside).
        #[arg(long)]
        keep_changes: bool,
        /// Only say what would be cleared, stopped and removed, and change nothing.
        #[arg(long, conflicts_with = "yes")]
        dry_run: bool,
    },
    /// Print the connection details another device sets itself up from: the SSH host, port and
    /// user, the SSH server's host key fingerprint and this octoboardd's path, with no secret, as
    /// a QR code and as one line of text the application takes pasted. The host is the address
    /// this SSH session reached the machine at, else the machine's own name; on stderr, where it
    /// came from and how to correct it.
    Connection {
        /// The host other devices reach this machine at, instead of the one chosen.
        #[arg(long)]
        host: Option<String>,
        /// The port this machine's SSH server is reached at, instead of the SSH session's or 22.
        #[arg(long, value_parser = clap::value_parser!(u16).range(1..))]
        ssh_port: Option<u16>,
        /// Print the text alone, with no QR code.
        #[arg(long)]
        text: bool,
    },
    /// Connect to the running daemon's owner endpoint and carry bytes both ways between it and
    /// this command's standard input and output until either side closes: what a program on
    /// another machine runs over SSH to reach the daemon, sending the owner key in its own
    /// requests. Exits 3 when no daemon runs, 4 when it has no owner endpoint (it serves no
    /// browser, or remote access is off), 5 when the endpoint cannot be reached and 6 when this is
    /// the desktop application's own copy and the application is not running, with the reason on
    /// stderr.
    Proxy,
    /// Forward one agent hook payload to the daemon. This is what the per-session hook script
    /// execs; it is not meant to be run by hand.
    Hook {
        #[arg(long)]
        session: String,
        /// The launch the hook reports for, which a hook script of an earlier build does not name.
        #[arg(long)]
        launch: Option<String>,
        #[arg(long)]
        port: u16,
        /// The owner-only file holding the key the daemon requires of every connection.
        #[arg(long)]
        key_file: std::path::PathBuf,
        /// The name of the event, for an agent whose payload names none and whose hook command is
        /// registered once per event. A name in the payload wins.
        #[arg(long)]
        event: Option<String>,
        /// Text only the session's own agent process has on its command line: the payload is
        /// forwarded only when the process that fired this hook has it, and dropped when it is an
        /// agent started inside the session.
        #[arg(long)]
        fired_by: Option<String>,
    },
    /// Carry a session's messages between the daemon and an extension inside its agent's process:
    /// one JSON line per message to send on standard output, one JSON line per outcome on standard
    /// input. This is started by the session extension.
    Channel {
        #[arg(long)]
        session: String,
        #[arg(long)]
        port: u16,
        /// The owner-only file holding the key the daemon requires of every connection.
        #[arg(long)]
        key_file: std::path::PathBuf,
    },
    /// Decide whether a file-edit tool call may go ahead, from the list a project session's launch
    /// wrote, and print a `deny` decision when the call targets another registered project. This is
    /// the `PreToolUse` command an agent's adapter injects; it is not meant to be run by hand.
    GuardHook {
        /// The list in the session's scratch directory.
        #[arg(long)]
        list: std::path::PathBuf,
        /// Text only the session's own agent process has on its command line: the call is decided
        /// only when the process that fired this hook has it, and let through when it is an agent
        /// started inside the session, as `hook --fired-by` forwards.
        #[arg(long)]
        fired_by: Option<String>,
    },
    /// Run a helper of a provider's: a command the provider's launch names for its agent to run,
    /// reading what the agent writes on standard input and printing what it reads. This is what
    /// such a command execs; it is not meant to be run by hand.
    AgentHelper {
        /// The provider whose helper it is.
        #[arg(long)]
        provider: String,
        /// The helper's name.
        #[arg(long)]
        helper: String,
        /// The helper's own arguments, which come last.
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Report the branches a session's git has just created, and whether it moved the refs a
    /// project's branch header reads. This is what the session's `reference-transaction` hook
    /// dispatcher starts; it is not meant to be run by hand.
    GitRef {
        #[arg(long)]
        session: String,
        /// The launch's port; absent, the report goes to the daemon its owner record names.
        #[arg(long)]
        port: Option<u16>,
        /// The data directory the dispatcher lives in, where that daemon's records are; absent (a
        /// dispatcher of an earlier build), this process's own.
        #[arg(long)]
        data_dir: Option<std::path::PathBuf>,
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
        /// The launch's port; absent, the daemon its owner record names is asked.
        #[arg(long)]
        port: Option<u16>,
        /// The data directory the dispatcher lives in, where that daemon's records are; absent (a
        /// dispatcher of an earlier build), this process's own.
        #[arg(long)]
        data_dir: Option<std::path::PathBuf>,
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
        /// The owner-only file holding the key the daemon requires of every connection.
        #[arg(long)]
        key_file: std::path::PathBuf,
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
        /// Octoboard's memory was on at the session's launch, which offers the memory tools.
        #[arg(long)]
        memory: bool,
        #[arg(long)]
        port: u16,
        /// The owner-only file holding the key the daemon requires of every connection.
        #[arg(long)]
        key_file: std::path::PathBuf,
        #[arg(long)]
        token: String,
    },
}

/// The `--channel` values `update` accepts.
fn parse_channel(text: &str) -> Result<protocol::DaemonUpdateChannel, String> {
    protocol::DaemonUpdateChannel::from_wire_name(text)
        .ok_or_else(|| "expected `stable` or `beta`".to_owned())
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
        Command::Stop { force } => std::process::exit(background::stop(force)?),
        Command::Restart { yes, if_safe } => std::process::exit(restart_check::run_restart(
            restart_check::mode_of(yes, if_safe, restart_check::Mode::Regardless),
        )?),
        Command::Status => std::process::exit(background::status()?),
        Command::Update {
            check,
            rollback,
            channel,
            yes,
            no_restart,
        } => {
            let task = match (check, rollback) {
                (true, _) => update::Task::Check,
                (_, true) => update::Task::Rollback,
                _ => update::Task::Install,
            };
            let restart = (!no_restart)
                .then(|| restart_check::mode_of(yes, false, restart_check::Mode::OnlyIfSafe));
            std::process::exit(update::run(task, channel, restart)?)
        }
        Command::VerifyArchive { archive, signature } => {
            std::process::exit(verify_archive::run(&archive, &signature))
        }
        Command::Address { rotate, json } => std::process::exit(if json {
            owner_endpoint::print_answer()?
        } else {
            address::print_address(rotate)?
        }),
        Command::Browser { action } => std::process::exit(browser::run(&action)?),
        Command::Uninstall {
            yes,
            force,
            keep_changes,
            dry_run,
        } => std::process::exit(uninstall::run(uninstall::Options {
            yes,
            force,
            keep_changes,
            dry_run,
        })?),
        Command::Connection {
            host,
            ssh_port,
            text,
        } => std::process::exit(connection::print_details(host, ssh_port, text)?),
        // Its stdout carries the connection's bytes and nothing else.
        Command::Proxy => std::process::exit(owner_endpoint::proxy()?),
        // The hook path must stay silent and must not pay for a runtime: it is on the agent's
        // turn critical path, several times per turn.
        Command::Hook {
            session,
            launch,
            port,
            key_file,
            event,
            fired_by,
        } => {
            hook::run(
                &session,
                launch.as_deref(),
                port,
                &key_file,
                event.as_deref(),
                fired_by.as_deref(),
            );
            Ok(())
        }
        // Its stdout carries the messages and nothing else, and it needs no runtime.
        Command::Channel {
            session,
            port,
            key_file,
        } => {
            channel::run(&session, port, &key_file);
            Ok(())
        }
        // Synchronous, and silent unless it denies: the agent reads its stdout as the decision.
        Command::GuardHook { list, fired_by } => {
            cross_project_guard::run(&list, || {
                fired_by
                    .as_deref()
                    .is_none_or(|mark| hook::fired_by(&list.to_string_lossy(), mark))
            });
            Ok(())
        }
        // On the path of the agent's own command, which waits for it: no runtime.
        Command::AgentHelper {
            provider,
            helper,
            args,
        } => std::process::exit(adapter::helper::run(&provider, &helper, &args)),
        Command::GitRef {
            session,
            port,
            data_dir,
            head,
            moved,
            refs,
        } => {
            session_git_hooks::ref_mode::run(
                &session,
                port,
                &data_dir.unwrap_or_else(crate::paths::data_dir),
                head.as_deref(),
                moved,
                &refs,
            );
            Ok(())
        }
        // Both run synchronously inside the user's git, which waits for their exit status.
        Command::GateHook {
            session,
            port,
            data_dir,
            hook,
        } => std::process::exit(session_git_hooks::gate_mode::run(
            &session,
            port,
            &data_dir.unwrap_or_else(crate::paths::data_dir),
            &hook,
        )),
        // On the path of every status line update, and its stdout is the user's status line's.
        Command::StatusLine {
            provider,
            session,
            port,
            key_file,
            args,
        } => std::process::exit(run_status_line(
            provider.as_deref(),
            &session,
            port,
            &key_file,
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
            memory,
            port,
            key_file,
            token,
        } => mcp::stdio::run(session, kind.into(), worker, memory, port, &key_file, token),
    }
}

/// Runs `provider`'s status line, or the one provider's that has one when none is named. A provider
/// without one, or an unknown one, has its input drained and nothing shown.
fn run_status_line(
    provider: Option<&str>,
    session: &str,
    port: u16,
    key_file: &Path,
    args: &[String],
) -> i32 {
    let agent = match provider {
        Some(id) => AgentId::from_wire_name(id),
        None => AgentId::ALL
            .into_iter()
            .find(|agent| agent.descriptor().capabilities.status_line),
    };
    match agent.and_then(|agent| adapter::provider(agent).status_line()) {
        Some(status_line) => status_line.run(session, port, key_file, args),
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

    /// The documentation site quotes the command's name and its uninstall options as this fixture
    /// holds them, which its own test reads too.
    #[test]
    fn the_uninstall_options_are_the_ones_the_docs_quote() {
        let quoted: serde_json::Value =
            serde_json::from_str(include_str!("../../testdata/docs_uninstall_command.json"))
                .unwrap();
        assert_eq!(quoted["cli_name"].as_str(), Some(env!("CARGO_BIN_NAME")));
        let command = <Cli as clap::CommandFactory>::command();
        let uninstall = command
            .find_subcommand("uninstall")
            .expect("the uninstall command");
        let mut options: Vec<String> = uninstall
            .get_arguments()
            .filter_map(|arg| arg.get_long())
            .filter(|long| *long != "help")
            .map(|long| format!("--{long}"))
            .collect();
        options.sort();
        let mut quoted_options: Vec<&str> = quoted["uninstall_options"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|option| option.as_str())
            .collect();
        quoted_options.sort_unstable();
        assert_eq!(options, quoted_options);
    }

    /// The desktop application starts a clean-up run with the option this fixture names, which
    /// its own test holds its copy to.
    #[test]
    fn the_clean_up_run_option_is_the_one_the_shell_names() {
        let contract: serde_json::Value =
            serde_json::from_str(include_str!("../../testdata/shell_contract.json")).unwrap();
        assert_eq!(
            contract["clean_up_run_option"].as_str(),
            Some(CLEAN_UP_RUN_OPTION)
        );
    }

    #[test]
    fn the_listening_line_names_its_port() {
        let line = listening_line(40123, &"a".repeat(64));
        assert_eq!(listening_port(&line), Some(40123));
        assert_eq!(listening_port("something else"), None);
    }

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

    /// The listener a predecessor hands over is named by its descriptor, only beside the port it is
    /// a listener on, and with no mode.
    #[test]
    fn the_listener_handed_over_is_named_by_descriptor_and_needs_a_port() {
        assert_eq!(parse(&[]).unwrap().listen_fd, None);
        let cli = parse(&["--port", "7531", "--key-stdin", "--listen-fd", "3"]).unwrap();
        assert_eq!(cli.listen_fd, Some(3));
        assert_eq!(cli.port, Some(7531));
        assert!(
            parse(&["--listen-fd", "3"]).is_err(),
            "no port to check it against"
        );
        assert!(parse(&["--port", "7531", "--listen-fd", "not-a-number"]).is_err());
        assert!(parse(&["--port", "7531", "--listen-fd"]).is_err());
        assert!(parse(&["--port", "7531", "--listen-fd", "3", "status"]).is_err());
    }

    /// The option is for the daemon's own successor start: it is not listed in the help.
    #[test]
    fn the_listener_option_is_hidden_from_the_help() {
        use clap::CommandFactory;

        let command = Cli::command();
        let option = command
            .get_arguments()
            .find(|argument| argument.get_id() == "listen_fd")
            .expect("the option exists");
        assert!(option.is_hide_set());
        assert!(!command
            .clone()
            .render_help()
            .to_string()
            .contains("--listen-fd"));
    }

    #[test]
    fn the_clean_up_run_is_asked_for_by_its_argument_and_hidden_from_the_help() {
        use clap::CommandFactory;

        assert!(!parse(&[]).unwrap().clean_up_run);
        let argument = clean_up_run_argument();
        assert_eq!(argument, "--clean-up-run");
        assert!(parse(&[&argument]).unwrap().clean_up_run);
        let cli = parse(&["--parent-pid", "1", &argument]).unwrap();
        assert!(cli.clean_up_run && cli.parent_pid == Some(1));
        assert!(parse(&[&argument, "status"]).is_err());
        assert!(!Cli::command().render_help().to_string().contains(&argument));
    }

    #[test]
    fn a_followed_process_is_named_only_for_a_clean_up_run_and_not_beside_an_application() {
        let argument = clean_up_run_argument();
        let followed = crate::platform::proc::ProcessIdentity::parse("7:123456").unwrap();
        let cli = parse(&[&argument, FOLLOW_PROCESS_ARGUMENT, &followed.key()]).unwrap();
        assert_eq!((cli.follow_process, cli.parent_pid), (Some(followed), None));
        // A bare pid is not a process: it may be another's by the time it is looked for.
        assert!(parse(&[&argument, FOLLOW_PROCESS_ARGUMENT, "7"]).is_err());
        assert!(parse(&[FOLLOW_PROCESS_ARGUMENT, "7:123456"]).is_err());
        let both = [
            &argument,
            FOLLOW_PROCESS_ARGUMENT,
            "7:123456",
            "--parent-pid",
            "1",
        ];
        assert!(parse(&both).is_err());
    }

    #[test]
    fn the_service_managed_mode_is_asked_for_and_does_not_combine_with_a_mode() {
        assert!(!parse(&[]).unwrap().service_managed);
        let cli = parse(&["--port", "7531", "--service-managed"]).unwrap();
        assert!(cli.service_managed);
        assert!(parse(&["--service-managed", "status"]).is_err());
    }

    #[test]
    fn restart_takes_yes_or_if_safe_and_no_daemon_option() {
        assert!(matches!(
            parse(&["restart"]).unwrap().command,
            Some(Command::Restart {
                yes: false,
                if_safe: false
            })
        ));
        assert!(matches!(
            parse(&["restart", "--yes"]).unwrap().command,
            Some(Command::Restart {
                yes: true,
                if_safe: false
            })
        ));
        assert!(matches!(
            parse(&["restart", "--if-safe"]).unwrap().command,
            Some(Command::Restart {
                yes: false,
                if_safe: true
            })
        ));
        assert!(parse(&["restart", "--yes", "--if-safe"]).is_err());
        assert!(parse(&["restart", "--port", "7531"]).is_err());
    }

    #[test]
    fn start_takes_a_port_only_when_given_one_and_stop_and_status_take_none() {
        assert!(matches!(
            parse(&["start"]).unwrap().command,
            Some(Command::Start { port: None })
        ));
        assert!(parse(&["start", "--port", "0"]).is_err());
        assert!(matches!(
            parse(&["start", "--port", "7531"]).unwrap().command,
            Some(Command::Start { port: Some(7531) })
        ));
        assert!(matches!(
            parse(&["stop"]).unwrap().command,
            Some(Command::Stop { force: false })
        ));
        assert!(matches!(
            parse(&["stop", "--force"]).unwrap().command,
            Some(Command::Stop { force: true })
        ));
        assert!(matches!(
            parse(&["status"]).unwrap().command,
            Some(Command::Status)
        ));
    }

    #[test]
    fn browser_takes_on_with_an_optional_port_off_and_status() {
        use browser::Action;
        let action = |args: &[&str]| match parse(args).unwrap().command {
            Some(Command::Browser { action }) => action,
            _ => panic!("not the browser command: {args:?}"),
        };
        assert!(matches!(
            action(&["browser", "on"]),
            Action::On { port: None }
        ));
        assert!(matches!(
            action(&["browser", "on", "--port", "4000"]),
            Action::On { port: Some(4000) }
        ));
        assert!(matches!(action(&["browser", "off"]), Action::Off));
        assert!(matches!(action(&["browser", "status"]), Action::Status));
        // A port is a number from 1 to 65535, and only `on` takes one; the action is not optional.
        for args in [
            &["browser", "on", "--port", "0"][..],
            &["browser", "on", "--port", "65536"],
            &["browser", "off", "--port", "4000"],
            &["browser", "status", "--port", "4000"],
            &["browser"],
        ] {
            assert!(parse(args).is_err(), "{args:?}");
        }
    }

    #[test]
    fn uninstall_takes_yes_force_keep_changes_and_dry_run_and_no_daemon_option() {
        let uninstall = |args: &[&str]| -> Result<(bool, bool, bool, bool), clap::Error> {
            match parse(&[&["uninstall"], args].concat())?.command {
                Some(Command::Uninstall {
                    yes,
                    force,
                    keep_changes,
                    dry_run,
                }) => Ok((yes, force, keep_changes, dry_run)),
                _ => panic!("not the uninstall command: {args:?}"),
            }
        };
        assert_eq!(uninstall(&[]).unwrap(), (false, false, false, false));
        assert_eq!(
            uninstall(&["--yes", "--force", "--keep-changes"]).unwrap(),
            (true, true, true, false)
        );
        assert_eq!(
            uninstall(&["--dry-run", "--keep-changes"]).unwrap(),
            (false, false, true, true)
        );
        assert!(uninstall(&["--dry-run", "--yes"]).is_err());
        assert!(uninstall(&["--bin-dir", "/x"]).is_err());
        assert!(uninstall(&["--port", "7531"]).is_err());
        assert!(parse(&["--port", "7531", "uninstall"]).is_err());
    }

    /// The clean-up is no command of its own: it is the first step of `uninstall`.
    #[test]
    fn there_is_no_cleanup_command_and_the_help_names_none() {
        use clap::CommandFactory;

        assert!(parse(&["cleanup"]).is_err());
        assert!(parse(&["cleanup", "--list"]).is_err());
        let mut command = Cli::command();
        assert!(command.find_subcommand("cleanup").is_none());
        let help = command.render_long_help().to_string();
        assert!(!help.contains("cleanup"), "{help}");
        assert!(help.contains("uninstall"), "{help}");
    }

    /// The help says what the command does first, what it leaves and whose daemon it refuses.
    #[test]
    fn the_help_of_uninstall_says_what_it_clears_removes_and_leaves() {
        use clap::CommandFactory;

        let mut command = Cli::command();
        let help = command
            .find_subcommand_mut("uninstall")
            .expect("the command exists")
            .render_long_help()
            .to_string();
        // The help is wrapped to the width of a terminal: read it as one line.
        let help = help.split_whitespace().collect::<Vec<_>>().join(" ");
        for said in [
            "first clear",
            "~/.octoboard",
            "desktop application",
            "--yes",
            "--force",
            "--keep-changes",
            "--dry-run",
        ] {
            assert!(help.contains(said), "{said}: {help}");
        }
    }

    #[test]
    fn connection_takes_a_host_an_ssh_port_and_the_text_option() {
        assert!(matches!(
            parse(&["connection"]).unwrap().command,
            Some(Command::Connection {
                host: None,
                ssh_port: None,
                text: false
            })
        ));
        assert!(matches!(
            parse(&["connection", "--host", "box.example", "--ssh-port", "2222", "--text"])
                .unwrap()
                .command,
            Some(Command::Connection {
                host: Some(ref host),
                ssh_port: Some(2222),
                text: true
            }) if host == "box.example"
        ));
        assert!(parse(&["connection", "--ssh-port", "0"]).is_err());
    }

    #[test]
    fn address_takes_the_rotate_or_the_json_option_and_no_daemon_option() {
        assert!(matches!(
            parse(&["address"]).unwrap().command,
            Some(Command::Address {
                rotate: false,
                json: false
            })
        ));
        assert!(matches!(
            parse(&["address", "--rotate"]).unwrap().command,
            Some(Command::Address {
                rotate: true,
                json: false
            })
        ));
        assert!(matches!(
            parse(&["address", "--json"]).unwrap().command,
            Some(Command::Address {
                rotate: false,
                json: true
            })
        ));
        assert!(parse(&["address", "--json", "--rotate"]).is_err());
        assert!(parse(&["status", "--rotate"]).is_err());
        assert!(parse(&["--rotate"]).is_err());
        assert!(parse(&["address", "--port", "7531"]).is_err());
        assert!(parse(&["--port", "7531", "address"]).is_err());
    }

    #[test]
    fn proxy_takes_no_option() {
        assert!(matches!(
            parse(&["proxy"]).unwrap().command,
            Some(Command::Proxy)
        ));
        assert!(parse(&["proxy", "--port", "7531"]).is_err());
        assert!(parse(&["--port", "7531", "proxy"]).is_err());
    }

    #[test]
    fn update_takes_a_check_a_rollback_a_channel_or_how_to_restart_and_no_daemon_option() {
        let update = |args: &[&str]| parse(&[&["update"], args].concat());
        assert!(matches!(
            update(&[]).unwrap().command,
            Some(Command::Update {
                check: false,
                rollback: false,
                channel: None,
                yes: false,
                no_restart: false
            })
        ));
        assert!(matches!(
            update(&["--check"]).unwrap().command,
            Some(Command::Update {
                check: true,
                rollback: false,
                channel: None,
                yes: false,
                no_restart: false
            })
        ));
        assert!(matches!(
            update(&["--rollback"]).unwrap().command,
            Some(Command::Update {
                check: false,
                rollback: true,
                channel: None,
                ..
            })
        ));
        assert!(matches!(
            update(&["--channel", "beta"]).unwrap().command,
            Some(Command::Update {
                channel: Some(protocol::DaemonUpdateChannel::Beta),
                ..
            })
        ));
        assert!(matches!(
            update(&["--check", "--channel", "stable"]).unwrap().command,
            Some(Command::Update {
                check: true,
                channel: Some(protocol::DaemonUpdateChannel::Stable),
                ..
            })
        ));
        assert!(matches!(
            update(&["--yes"]).unwrap().command,
            Some(Command::Update {
                yes: true,
                no_restart: false,
                ..
            })
        ));
        assert!(matches!(
            update(&["--rollback", "--no-restart"]).unwrap().command,
            Some(Command::Update {
                rollback: true,
                no_restart: true,
                ..
            })
        ));
        assert!(update(&["--yes", "--no-restart"]).is_err());
        assert!(update(&["--check", "--yes"]).is_err());
        assert!(update(&["--check", "--no-restart"]).is_err());
        assert!(update(&["--check", "--rollback"]).is_err());
        assert!(update(&["--rollback", "--channel", "beta"]).is_err());
        assert!(update(&["--channel", "nightly"]).is_err());
        assert!(update(&["--channel"]).is_err());
        assert!(parse(&["--port", "1", "update"]).is_err());
    }

    #[test]
    fn verify_archive_takes_an_archive_and_a_signature() {
        assert!(matches!(
            parse(&["verify-archive", "a.tar.gz", "a.sig"])
                .unwrap()
                .command,
            Some(Command::VerifyArchive { .. })
        ));
        assert!(parse(&["verify-archive", "a.tar.gz"]).is_err());
        assert!(parse(&["--port", "1", "verify-archive", "a.tar.gz", "a.sig"]).is_err());
    }

    /// The hook mode names the event only when asked to; a name is a value of `--event`, spaces
    /// and all, and the option takes no other form.
    #[test]
    fn the_hook_mode_takes_an_event_name_only_when_given_one() {
        let event_of = |extra: &[&str]| {
            let mut args = vec!["hook", "--session", "s", "--port", "1", "--key-file", "/k"];
            args.extend(extra);
            match parse(&args).unwrap().command {
                Some(Command::Hook { event, .. }) => event,
                _ => panic!("not the hook mode"),
            }
        };
        assert_eq!(event_of(&[]), None);
        assert_eq!(
            event_of(&["--event", "PreToolUse"]).as_deref(),
            Some("PreToolUse")
        );
        assert_eq!(
            event_of(&["--event", "tool use"]).as_deref(),
            Some("tool use")
        );
        assert!(parse(&[
            "hook",
            "--session",
            "s",
            "--port",
            "1",
            "--key-file",
            "/k",
            "--event"
        ])
        .is_err());
        let fired_by = |extra: &[&str]| {
            let mut args = vec!["hook", "--session", "s", "--port", "1", "--key-file", "/k"];
            args.extend(extra);
            match parse(&args).unwrap().command {
                Some(Command::Hook { fired_by, .. }) => fired_by,
                _ => panic!("not the hook mode"),
            }
        };
        assert_eq!(fired_by(&[]), None);
        assert_eq!(
            fired_by(&["--fired-by", "/run/s/mcp config.json"]).as_deref(),
            Some("/run/s/mcp config.json")
        );
        // The other helpers have no event of their own to name.
        assert!(parse(&["git-ref", "--session", "s", "--event", "x"]).is_err());
    }

    /// The helpers take the key's file, never the key: a key in arguments is listed by `ps` for
    /// every user of the machine.
    #[test]
    fn the_helpers_take_a_key_file_and_refuse_a_key() {
        let channel = parse(&[
            "channel",
            "--session",
            "s",
            "--port",
            "1",
            "--key-file",
            "/k",
        ])
        .unwrap();
        assert!(matches!(
            channel.command,
            Some(Command::Channel { key_file, port: 1, .. }) if key_file == Path::new("/k")
        ));
        assert!(parse(&["channel", "--session", "s", "--port", "1", "--key", "k"]).is_err());
        let hook = parse(&["hook", "--session", "s", "--port", "1", "--key-file", "/k"]).unwrap();
        assert!(matches!(
            hook.command,
            Some(Command::Hook { key_file, launch: None, .. }) if key_file == Path::new("/k")
        ));
        // The launch, which a hook script of an earlier build leaves out.
        let launched = parse(&[
            "hook",
            "--session",
            "s",
            "--launch",
            "l",
            "--port",
            "1",
            "--key-file",
            "/k",
        ])
        .unwrap();
        assert!(matches!(
            launched.command,
            Some(Command::Hook { launch: Some(launch), .. }) if launch == "l"
        ));
        let mcp = parse(&[
            "mcp",
            "--session",
            "s",
            "--kind",
            "project",
            "--port",
            "1",
            "--key-file",
            "/k",
            "--token",
            "t",
        ])
        .unwrap();
        assert!(matches!(
            mcp.command,
            Some(Command::Mcp { key_file, .. }) if key_file == Path::new("/k")
        ));
        assert!(parse(&["hook", "--session", "s", "--port", "1"]).is_err());
        assert!(parse(&["hook", "--session", "s", "--port", "1", "--key", "k"]).is_err());
        assert!(parse(&["status-line", "--session", "s", "--port", "1", "--key", "k"]).is_err());
        // The git hook helpers take the key from a file the environment names, and no argument.
        assert!(parse(&["git-ref", "--session", "s", "--port", "1", "--key", "k"]).is_err());
        assert!(parse(&[
            "gate-hook",
            "--session",
            "s",
            "--port",
            "1",
            "--key",
            "k",
            "pre-push"
        ])
        .is_err());
        assert!(parse(&["git-ref", "--session", "s", "--port", "1"]).is_ok());
        // With no port, the git hook helpers go to the daemon's own route.
        assert!(matches!(
            parse(&["git-ref", "--session", "s"]).unwrap().command,
            Some(Command::GitRef { port: None, .. })
        ));
        assert!(matches!(
            parse(&["gate-hook", "--session", "s", "pre-push"])
                .unwrap()
                .command,
            Some(Command::GateHook { port: None, .. })
        ));
    }

    /// The command lines the git hook dispatchers of the earlier builds start, which always carry a
    /// port and no data directory, still parse: a session launched by one keeps its dispatchers.
    #[test]
    fn the_git_hook_helpers_take_the_command_lines_earlier_dispatchers_start() {
        let Some(Command::GitRef {
            session,
            port,
            data_dir,
            head,
            moved,
            refs,
        }) = parse(&[
            "git-ref",
            "--session",
            "s",
            "--port",
            "7531",
            "--head",
            "refs/heads/main",
            "--moved",
            "--",
            "refs/heads/a",
            "abc",
        ])
        .unwrap()
        .command
        else {
            panic!("not git-ref");
        };
        assert_eq!((session.as_str(), port, moved), ("s", Some(7531), true));
        assert_eq!(data_dir, None);
        assert_eq!(
            head.as_deref(),
            Some(std::ffi::OsStr::new("refs/heads/main"))
        );
        assert_eq!(refs, ["refs/heads/a", "abc"]);
        for hook in [&["reference-transaction", "prepared"][..], &["pre-push"]] {
            let mut args = vec!["gate-hook", "--session", "s", "--port", "7531"];
            args.extend(hook);
            assert!(
                matches!(
                    parse(&args).unwrap().command,
                    Some(Command::GateHook {
                        port: Some(7531),
                        data_dir: None,
                        ..
                    })
                ),
                "{hook:?}"
            );
        }
    }

    /// The data directory the dispatcher is under is an option of both git hook helpers, and goes
    /// before the gate's hook name and the references the report lists.
    #[test]
    fn the_git_hook_helpers_take_the_data_directory_of_their_dispatcher() {
        assert!(matches!(
            parse(&["git-ref", "--session", "s", "--data-dir", "/d", "--moved", "--", "r", "o"])
                .unwrap()
                .command,
            Some(Command::GitRef { port: None, data_dir: Some(dir), moved: true, .. })
                if dir == Path::new("/d")
        ));
        assert!(matches!(
            parse(&["gate-hook", "--session", "s", "--port", "1", "--data-dir", "/d", "pre-push"])
                .unwrap()
                .command,
            Some(Command::GateHook { port: Some(1), data_dir: Some(dir), .. })
                if dir == Path::new("/d")
        ));
    }

    fn status_line(args: &str) -> (Option<String>, Vec<String>) {
        let cli = Cli::try_parse_from(["octoboardd"].into_iter().chain(args.split(' '))).unwrap();
        let Some(Command::StatusLine {
            provider,
            session,
            port,
            key_file,
            args,
        }) = cli.command
        else {
            panic!("not the status-line command");
        };
        assert_eq!(
            (session.as_str(), port, key_file),
            ("s", 1, Path::new("/k").to_path_buf())
        );
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
            status_line("status-line --session s --port 1 --key-file /k --cwd /p --config-dir /c"),
            (None, own.clone())
        );
        assert_eq!(
            status_line(
                "status-line --provider claude --session s --port 1 --key-file /k --cwd /p --config-dir /c"
            ),
            (Some("claude".to_owned()), own)
        );
    }

    /// The guard mode takes a mark only when given one, as the hook mode does; one guard command
    /// of a session launched before the mark existed still parses.
    #[test]
    fn the_guard_mode_takes_a_mark_only_when_given_one() {
        let fired_by = |extra: &[&str]| {
            let mut args = vec!["guard-hook", "--list", "/l"];
            args.extend(extra);
            match parse(&args).unwrap().command {
                Some(Command::GuardHook { list, fired_by }) => {
                    assert_eq!(list, Path::new("/l"));
                    fired_by
                }
                _ => panic!("not the guard mode"),
            }
        };
        assert_eq!(fired_by(&[]), None);
        assert_eq!(
            fired_by(&["--fired-by", "/run/s/mcp.json"]).as_deref(),
            Some("/run/s/mcp.json")
        );
    }

    /// A helper's command parses into its provider, its name and its own arguments after `--`,
    /// which may look like options.
    #[test]
    fn an_agent_helper_command_parses_its_provider_name_and_arguments() {
        match parse(&[
            "agent-helper",
            "--provider",
            "p",
            "--helper",
            "title",
            "--",
            "/a dir",
            "--flag",
        ])
        .unwrap()
        .command
        {
            Some(Command::AgentHelper {
                provider,
                helper,
                args,
            }) => {
                assert_eq!(provider, "p");
                assert_eq!(helper, "title");
                assert_eq!(args, ["/a dir", "--flag"]);
            }
            _ => panic!("not the agent-helper command"),
        }
    }
}
