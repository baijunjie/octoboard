//! The private Codex app server's launch plan. Every launch with Octoboard's additions runs one,
//! whatever the version: no release is known to have introduced the server or the remote terminal
//! (0.166.0 is only the first one measured), and a server that does not come up falls back to the
//! ordinary launch (`crate::session::term::launch`), as a terminal that refuses `--remote` falls
//! to a reduced launch without it ([`super::reduced`]).
//!
//! The server takes every `-c` override of the launch. The terminal attached to it with `--remote`
//! takes them too, since rendering and startup notices are the client's, except on a resume: a
//! remote resume refuses any permission override on its command line, so those are left off it and
//! applied to the thread through the server before the terminal connects.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;

use super::transport::CodexTransport;
use crate::adapter::AdoptedLaunch;
use crate::platform::shell::ResolvedBinary;
use crate::session::term::{MessageTransport, PrivateServer, StartedServer};
use crate::terminal::server_host::{FoundServer, ServerHost};

/// The configuration keys a remote resume refuses on the terminal's command line ("Permission
/// overrides are not supported when resuming a remote task").
pub(super) const PERMISSION_KEYS: [&str; 4] = [
    "default_permissions",
    "approval_policy",
    "approvals_reviewer",
    "sandbox_mode",
];

/// The permission preset a resume applies to its thread through the server, as `thread/resume`
/// names its fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResumePermissions {
    /// The built-in permission profile (`permissions`).
    pub profile: &'static str,
    /// `approvalPolicy`.
    pub approval_policy: &'static str,
    /// `approvalsReviewer`.
    pub reviewer: &'static str,
}

pub struct PrivateServerPlan {
    /// The overrides the server is started with.
    pub args: Vec<String>,
    /// The terminal's arguments before the endpoint is added, without the task.
    pub tui_args: Vec<String>,
    /// The task the terminal is started with, passed after every option and a bare `--`.
    pub task: Option<String>,
    /// The hook command the launch generated, whose trust a resume grants by its fingerprint.
    pub hook_command: String,
    /// The cross-project guard's hook command, trusted on a resume the same way.
    pub guard_command: Option<String>,
    pub resume_thread: Option<String>,
    /// Applied to the resumed thread before the terminal connects; `None` leaves its saved choice.
    pub resume_permissions: Option<ResumePermissions>,
}

impl PrivateServerPlan {
    /// The plan for a launch whose ordinary terminal arguments, without the task, are `args`: the
    /// resume subcommand first when `resume_thread` is set, then its options.
    pub fn new(
        args: &[String],
        cwd: &Path,
        task: Option<&str>,
        hook_command: String,
        guard_command: Option<String>,
        resume_thread: Option<&str>,
        resume_permissions: Option<ResumePermissions>,
    ) -> Self {
        let subcommand = if resume_thread.is_some() { 2 } else { 0 };
        let (prefix, options) = args.split_at(subcommand.min(args.len()));
        let mut tui_args = prefix.to_vec();
        // A remote terminal takes its workspace from the server unless told, which would skip the
        // folder-trust confirmation a local launch shows.
        tui_args.extend(["--cd".to_string(), cwd.to_string_lossy().into_owned()]);
        if resume_thread.is_some() {
            tui_args.extend(without_permission_overrides(options));
        } else {
            tui_args.extend(options.iter().cloned());
        }
        Self {
            args: overrides(args),
            tui_args,
            task: task.map(str::to_string),
            hook_command,
            guard_command,
            resume_thread: resume_thread.map(str::to_string),
            resume_permissions: resume_thread.and(resume_permissions),
        }
    }

    /// The terminal's arguments as a client of the server `client_args` name: its options, the
    /// endpoint, then the task after a bare `--`, so that a task which starts with `-` is the
    /// prompt and not an option (`codex --remote <endpoint> -- <task>`, measured).
    fn terminal_args(&self, client_args: Vec<String>) -> Vec<String> {
        let mut args = self.tui_args.clone();
        args.extend(client_args);
        if let Some(task) = &self.task {
            args.extend(["--".to_string(), task.clone()]);
        }
        args
    }
}

impl PrivateServer for PrivateServerPlan {
    /// Starts the server, and the terminal is then a client of it.
    fn start(
        &self,
        binary: &ResolvedBinary,
        environment: &HashMap<String, String>,
        cwd: &Path,
        host: &dyn ServerHost,
        launch_id: &str,
    ) -> Result<StartedServer> {
        let transport = CodexTransport::start(binary, self, environment, cwd, host, launch_id)?;
        Ok(StartedServer {
            args: self.terminal_args(transport.client_args()),
            env: transport.client_env(),
            transport: Arc::new(transport),
        })
    }

    #[cfg(test)]
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// The transport to the private server found with the terminal of the adopted `launch`, linked
/// to again; `None` where none was found (an ordinary launch, or one in a holder that runs no
/// servers) or the one found names no port to reach, the session's messages then going to its
/// terminal.
pub(super) fn adopt(launch: AdoptedLaunch<'_>) -> Option<Arc<dyn MessageTransport>> {
    let FoundServer { endpoint, server } = launch.server?;
    match CodexTransport::adopt(endpoint, server, launch.launch_id, launch.agent_session_id) {
        Ok(transport) => Some(Arc::new(transport)),
        Err(err) => {
            tracing::warn!(
                launch = %launch.launch_id,
                error = %format!("{err:#}"),
                "the private Codex app server of an adopted terminal cannot be reached; its \
                 messages go to the terminal"
            );
            None
        }
    }
}

/// The `-c` overrides among `args`, each with its `-c`, read in pairs as Codex reads them.
fn overrides(args: &[String]) -> Vec<String> {
    let mut kept = Vec::new();
    let mut items = args.iter();
    while let Some(arg) = items.next() {
        if arg == "-c" {
            if let Some(value) = items.next() {
                kept.extend([arg.clone(), value.clone()]);
            }
        }
    }
    kept
}

/// `args` without the `-c` overrides of a key in [`PERMISSION_KEYS`].
fn without_permission_overrides(args: &[String]) -> Vec<String> {
    without_overrides_of(args, &PERMISSION_KEYS)
}

/// `args` without the `-c` overrides of a key in `keys`, read in pairs as Codex reads them; every
/// other argument kept in its place.
pub(super) fn without_overrides_of(args: &[String], keys: &[&str]) -> Vec<String> {
    let mut kept = Vec::with_capacity(args.len());
    let mut items = args.iter();
    while let Some(arg) = items.next() {
        if arg == "-c" {
            let Some(value) = items.next() else {
                kept.push(arg.clone());
                break;
            };
            let key = value.split('=').next().unwrap_or_default().trim();
            if !keys.contains(&key) {
                kept.extend([arg.clone(), value.clone()]);
            }
        } else {
            kept.push(arg.clone());
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn a_remote_resume_carries_no_permission_override() {
        let args = strings(&[
            "-c",
            "default_permissions=\":workspace\"",
            "--dangerously-bypass-hook-trust",
            "-c",
            "approval_policy=\"on-request\"",
            "-c",
            "model=\"m\"",
            "-c",
            "approvals_reviewer=\"user\"",
            "-c",
            "sandbox_mode = \"read-only\"",
            "-c",
            "tui.resume_cwd=\"current\"",
        ]);
        assert_eq!(
            without_permission_overrides(&args),
            strings(&[
                "--dangerously-bypass-hook-trust",
                "-c",
                "model=\"m\"",
                "-c",
                "tui.resume_cwd=\"current\"",
            ])
        );
    }

    /// An override whose value is itself `-c` is one override, not the start of another.
    #[test]
    fn overrides_are_read_in_pairs() {
        let args = strings(&["--flag", "-c", "-c", "-c", "model=\"m\"", "task"]);
        assert_eq!(
            overrides(&args),
            strings(&["-c", "-c", "-c", "model=\"m\""])
        );
    }

    #[test]
    fn the_terminal_is_told_its_workspace_and_its_task_is_kept_apart_from_its_options() {
        let args = strings(&["--dangerously-bypass-hook-trust", "-c", "model=\"m\""]);
        let plan = PrivateServerPlan::new(
            &args,
            Path::new("/work"),
            Some("first task"),
            "hook".into(),
            None,
            None,
            None,
        );
        assert_eq!(
            plan.tui_args,
            strings(&[
                "--cd",
                "/work",
                "--dangerously-bypass-hook-trust",
                "-c",
                "model=\"m\""
            ])
        );
        assert_eq!(plan.args, strings(&["-c", "model=\"m\""]));
        assert_eq!(plan.task.as_deref(), Some("first task"));
        assert_eq!(plan.resume_thread, None);
    }

    /// The terminal's task comes after the endpoint and a bare `--`, whatever it starts with; a
    /// resume, which has no task, gets no `--`.
    #[test]
    fn the_terminals_task_is_the_one_argument_after_a_bare_double_dash() {
        let options = strings(&["-c", "model=\"m\""]);
        let endpoint = || strings(&["--remote", "unix:///run/codex.sock"]);
        for task in ["first task", "- a list", "--help me with this", "-p x"] {
            let plan = PrivateServerPlan::new(
                &options,
                Path::new("/work"),
                Some(task),
                "hook".into(),
                None,
                None,
                None,
            );
            assert_eq!(
                plan.terminal_args(endpoint()),
                strings(&[
                    "--cd",
                    "/work",
                    "-c",
                    "model=\"m\"",
                    "--remote",
                    "unix:///run/codex.sock",
                    "--",
                    task
                ]),
                "{task:?}"
            );
        }
        let resume = strings(&["resume", "thread-1", "-c", "model=\"m\""]);
        let plan = PrivateServerPlan::new(
            &resume,
            Path::new("/work"),
            None,
            "hook".into(),
            None,
            Some("thread-1"),
            None,
        );
        assert_eq!(
            plan.terminal_args(endpoint()),
            strings(&[
                "resume",
                "thread-1",
                "--cd",
                "/work",
                "-c",
                "model=\"m\"",
                "--remote",
                "unix:///run/codex.sock"
            ])
        );
    }

    #[test]
    fn permissions_are_applied_through_the_server_only_on_a_resume() {
        let preset = ResumePermissions {
            profile: ":workspace",
            approval_policy: "on-request",
            reviewer: "user",
        };
        let args = strings(&[
            "resume",
            "thread-1",
            "--dangerously-bypass-hook-trust",
            "-c",
            "approvals_reviewer=\"user\"",
        ]);
        let resumed = PrivateServerPlan::new(
            &args,
            Path::new("/work"),
            None,
            "hook".into(),
            None,
            Some("thread-1"),
            Some(preset),
        );
        assert_eq!(
            resumed.tui_args,
            strings(&[
                "resume",
                "thread-1",
                "--cd",
                "/work",
                "--dangerously-bypass-hook-trust"
            ])
        );
        assert_eq!(
            resumed.args,
            strings(&["-c", "approvals_reviewer=\"user\""])
        );
        assert_eq!(resumed.resume_permissions, Some(preset));
        let fresh = PrivateServerPlan::new(
            &args[2..],
            Path::new("/work"),
            None,
            "hook".into(),
            None,
            None,
            Some(preset),
        );
        assert_eq!(fresh.resume_permissions, None);
    }
}
