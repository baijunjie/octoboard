//! The command line: the daemon itself when no mode is named; `start`, `stop`, `restart`, `status`
//! and `address`, which a user runs to keep the daemon in the background; the modes other processes
//! start it in — an agent's hook, a session's git hooks, a provider's status line and the MCP
//! server — each of which runs in a process of its own and calls the running daemon, and an
//! agent's cross-project guard, which decides from a file alone; and the holder, which the running
//! daemon calls instead.

mod address;
mod background;
#[cfg(all(test, unix))]
mod background_tests;
mod holder;
pub(crate) mod hook;
mod installation;
#[cfg(test)]
mod key_source_tests;
mod succession;
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
}

/// The port `start` listens on when none is given: `defaultServerPort` in `config/app.json`, which
/// the install script's service uses too, so that the documented SSH forward works unchanged. The
/// foreground daemon keeps a port the system assigns: the desktop application and development runs
/// start it without one.
pub(crate) const DEFAULT_START_PORT: u16 =
    match u16::from_str_radix(env!("OCTOBOARD_DEFAULT_SERVER_PORT"), 10) {
        Ok(port) => port,
        Err(_) => panic!("OCTOBOARD_DEFAULT_SERVER_PORT is not a port number"),
    };

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Start the daemon in the background on a fixed loopback port and return once it is
    /// listening; it keeps running after the terminal closes, until `stop`.
    Start {
        /// The loopback port the daemon listens on, and keeps across restarts.
        #[arg(long, default_value_t = DEFAULT_START_PORT, value_parser = clap::value_parser!(u16).range(1..))]
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
    /// Download the latest daemon, check it against the release key and replace this executable
    /// with it, keeping the previous one as `octoboardd.previous`. The running daemon keeps
    /// running the build it started as until `octoboardd restart`, which keeps every session
    /// running. Where the user may not write to the executable's directory, upgrade by hand with
    /// the install script instead. Refused for a daemon that follows the desktop application.
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
    /// daemon accepts one (a daemon that serves the browser on its socket), else the launch key.
    Address {
        /// Replace the owner key first, so the address printed is the new one's and an earlier
        /// address stops working. Where no daemon runs, or the running one takes no owner key,
        /// the file alone is replaced, for the next daemon that serves the browser.
        #[arg(long)]
        rotate: bool,
    },
    /// Forward one agent hook payload to the daemon. This is what the per-session hook script
    /// execs; it is not meant to be run by hand.
    Hook {
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
        Command::Stop => std::process::exit(background::stop()?),
        Command::Restart => std::process::exit(background::restart()?),
        Command::Status => std::process::exit(background::status()?),
        Command::Update {
            check,
            rollback,
            channel,
        } => {
            let task = match (check, rollback) {
                (true, _) => update::Task::Check,
                (_, true) => update::Task::Rollback,
                _ => update::Task::Install,
            };
            std::process::exit(update::run(task, channel)?)
        }
        Command::VerifyArchive { archive, signature } => {
            std::process::exit(verify_archive::run(&archive, &signature))
        }
        Command::Address { rotate } => std::process::exit(address::print_address(rotate)?),
        // The hook path must stay silent and must not pay for a runtime: it is on the agent's
        // turn critical path, several times per turn.
        Command::Hook {
            session,
            port,
            key_file,
        } => {
            hook::run(&session, port, &key_file);
            Ok(())
        }
        // Synchronous, and silent unless it denies: the agent reads its stdout as the decision.
        Command::GuardHook { list } => {
            cross_project_guard::run(&list);
            Ok(())
        }
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
            port,
            key_file,
            token,
        } => mcp::stdio::run(session, kind.into(), worker, port, &key_file, token),
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
    fn start_takes_the_default_port_unless_given_one_and_stop_and_status_take_none() {
        assert!(matches!(
            parse(&["start"]).unwrap().command,
            Some(Command::Start {
                port: DEFAULT_START_PORT
            })
        ));
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

    #[test]
    fn address_takes_only_the_rotate_option_and_no_daemon_option() {
        assert!(matches!(
            parse(&["address"]).unwrap().command,
            Some(Command::Address { rotate: false })
        ));
        assert!(matches!(
            parse(&["address", "--rotate"]).unwrap().command,
            Some(Command::Address { rotate: true })
        ));
        assert!(parse(&["status", "--rotate"]).is_err());
        assert!(parse(&["--rotate"]).is_err());
        assert!(parse(&["address", "--port", "7531"]).is_err());
        assert!(parse(&["--port", "7531", "address"]).is_err());
    }

    #[test]
    fn update_takes_a_check_a_rollback_or_a_channel_and_no_daemon_option() {
        let update = |args: &[&str]| parse(&[&["update"], args].concat());
        assert!(matches!(
            update(&[]).unwrap().command,
            Some(Command::Update {
                check: false,
                rollback: false,
                channel: None
            })
        ));
        assert!(matches!(
            update(&["--check"]).unwrap().command,
            Some(Command::Update {
                check: true,
                rollback: false,
                channel: None
            })
        ));
        assert!(matches!(
            update(&["--rollback"]).unwrap().command,
            Some(Command::Update {
                check: false,
                rollback: true,
                channel: None
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

    /// The helpers take the key's file, never the key: a key in arguments is listed by `ps` for
    /// every user of the machine.
    #[test]
    fn the_helpers_take_a_key_file_and_refuse_a_key() {
        let hook = parse(&["hook", "--session", "s", "--port", "1", "--key-file", "/k"]).unwrap();
        assert!(matches!(
            hook.command,
            Some(Command::Hook { key_file, .. }) if key_file == Path::new("/k")
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
}
