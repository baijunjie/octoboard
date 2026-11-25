//! Antigravity CLI's behavior (`crate::providers::antigravity`): its launch, with Octoboard's
//! hooks, MCP server and role text injected, and the sub-interfaces its measurements give an
//! answer for. The measurements are in `docs/agent-cli-reference.md`, section "Antigravity CLI".
//!
//! Everything is injected per launch and nothing of the user's is edited: the launch gets a
//! directory of its own that stands for `~/.gemini` ([`home`]), and its settings for the launch
//! alone go in a file of that directory named with `--settings`, which the CLI never saves.
//!
//! The session's status is the CLI's own: it reports it over OSC 7501
//! (`crate::session::program_status`), a failed turn and each kind of wait on the user among
//! them. Its hooks say the rest ([`hooks`]).

mod accounts;
mod background;
mod home;
mod hooks;
mod interaction;
mod login;
mod mcp_inventory;
mod models;
mod native_dirs;
mod quota;
mod records;
mod screen;
#[cfg(test)]
mod tests;
mod title;

use std::time::Duration;

use anyhow::Result;
use serde_json::{json, Value};

use super::{AgentProvider, CliPackaging, LaunchPlan, LaunchSpec};
use crate::hooks::HookInterpreter;
use crate::protocol::Notice;
use crate::providers::{AgentId, ProviderDescriptor};
use crate::session::screen::ScreenReader;
use crate::session_tools;

pub struct AntigravityProvider;

/// The variable that makes the CLI report its status over OSC 7501 without first asking the
/// terminal whether it reads the reports.
const STATUS_REPORTS_VAR: &str = "AGY_CLI_OSC7501";

/// The most a launch's first message waits for the CLI's listing of Octoboard's tools. The CLI
/// starts its MCP servers as its input box comes up (1.5.14: the listing was asked 0.2 to 0.5 s
/// after the launch, from 0.02 s before the box was drawn to 0.07 s after it, with the machine
/// idle and with every core busy; 1.0 s and 3.1 s on a first launch after a pause), so the bound
/// is several times the slowest seen, and a server the CLI never asks holds a message no longer.
const TOOL_LISTING_WAIT: Duration = Duration::from_secs(15);

impl AntigravityProvider {
    /// The settings of one launch, which the CLI takes from `--settings` and never saves. A key
    /// given there stands in place of the user's own of that name, and inside `permissions` each
    /// list does (1.5.14): so `allow` is the user's own rules, `user_allow`, with the one that
    /// lets Octoboard's own tools run with no permission prompt after them, and their `deny` and
    /// `ask` lists, which are not given, stand. The survey the CLI puts in place of its input box
    /// after some turns is turned off, since a digit typed next would be its answer and the box
    /// could not be read meanwhile. A rule the user has the CLI keep from a permission prompt
    /// ("Persist to settings.json") is added to their own list on disk, not to this one: Octoboard's
    /// rule was measured never to reach their settings.
    ///
    /// The title command ([`title`]) is given here as well; it stands in place of a `title`
    /// command of the user's own for the session, whose terminal title is Octoboard's to read.
    fn launch_settings(
        user_allow: &[Value],
        title: Value,
        trusted_console: Option<&std::path::Path>,
    ) -> String {
        let ours = Value::String(format!("mcp({}/*)", session_tools::catalogue::SERVER_KEY));
        let mut allow: Vec<Value> = user_allow
            .iter()
            .filter(|rule| **rule != ours)
            .cloned()
            .collect();
        allow.push(ours);
        let mut settings = json!({
            "permissions": { "allow": allow },
            "showFeedbackSurvey": false,
        });
        settings[title::SETTING] = title;
        if let Some(folder) = trusted_console {
            settings["trustedWorkspaces"] = json!([folder]);
        }
        settings.to_string()
    }
}

impl AgentProvider for AntigravityProvider {
    fn descriptor(&self) -> &'static ProviderDescriptor {
        AgentId::ANTIGRAVITY.descriptor()
    }

    fn hooks(&self) -> &'static dyn HookInterpreter {
        &hooks::AntigravityHooks
    }

    fn screen(&self) -> &'static dyn ScreenReader {
        &screen::AntigravityScreen
    }

    /// Its numbered permission prompts and its `ask_question` form ([`interaction`]).
    fn interactions(&self) -> Option<&'static dyn crate::session::interaction::InteractionReader> {
        Some(&interaction::AntigravityInteractions)
    }

    fn accounts(&self) -> Option<&'static dyn crate::accounts::AccountOps> {
        Some(&accounts::AntigravityAccounts)
    }

    fn records(&self) -> &'static dyn crate::records::RecordStore {
        &records::AntigravityRecords
    }

    fn catalogue(&self) -> &'static dyn crate::agents::models::ModelCatalogue {
        &models::AntigravityCatalogue
    }

    /// Its installer places one executable in `~/.local/bin` or `%LOCALAPPDATA%\agy\bin`, which updates
    /// itself in the background and with `agy update` (1.5.14). It is on no package registry
    /// Octoboard knows of, and no command of its own prints the latest version without updating
    /// (`agy update` takes no flag); the latest version is read from the manifest of Google's
    /// updater that the installer reads, one per platform (`{"version", "url", "sha512"}`,
    /// 1.5.15 on 2026-11-23). Linux has a musl build with manifests of its own
    /// (`linux_amd64_musl`), which named the same version; the glibc one is read.
    fn cli_packaging(&self) -> &'static CliPackaging {
        // The manifest of the platform Rust names `$os` and `$arch`, which the updater names
        // `$platform`.
        macro_rules! manifest {
            ($os:literal, $arch:literal, $platform:literal) => {
                super::ManifestUrl {
                    os: $os,
                    arch: $arch,
                    url: concat!(
                        "https://antigravity-cli-auto-updater-974169037036.us-central1.run.app",
                        "/manifests/",
                        $platform,
                        ".json"
                    ),
                }
            };
        }
        &CliPackaging {
            native_layouts: &[&[".local", "bin", "agy"], &["agy", "bin", "agy.exe"]],
            native_markers: &[],
            native_update: &["update"],
            native_latest: None,
            native_manifest: Some(super::ManifestLatest {
                urls: &[
                    manifest!("macos", "aarch64", "darwin_arm64"),
                    manifest!("macos", "x86_64", "darwin_amd64"),
                    manifest!("linux", "aarch64", "linux_arm64"),
                    manifest!("linux", "x86_64", "linux_amd64"),
                    manifest!("windows", "aarch64", "windows_arm64"),
                    manifest!("windows", "x86_64", "windows_amd64"),
                ],
                version_field: "version",
            }),
            npm_packages: &[],
            homebrew_formula: None,
            homebrew_cask: None,
        }
    }

    /// A new conversation gets its id from the CLI, which its first turn's hook reports; a resume
    /// names it.
    fn plan(&self, spec: &LaunchSpec<'_>) -> Result<LaunchPlan> {
        // Refuses a CLI that does not take the directory flag before asking it of the settings.
        let launch = home::build(spec)?;

        // The settings go in a file, not in the argument itself: they repeat the user's own
        // allow rules, which may hold command lines.
        let title = title::command_setting(spec.self_exe, &launch.dir);
        let settings = home::write_settings(
            &launch.dir,
            &Self::launch_settings(
                &launch.user_allow,
                title,
                (spec.kind == crate::protocol::SessionKind::Console).then_some(spec.cwd),
            ),
        )?;
        let mut args = vec![
            home::directory_argument(&launch.dir),
            "--settings".to_string(),
            settings.to_string_lossy().into_owned(),
        ];
        if let Some(id) = spec.resume_agent_session_id {
            args.push("--conversation".to_string());
            args.push(id.to_string());
        }
        if let Some(model) = spec.model {
            args.push("--model".to_string());
            args.push(model.to_string());
        }
        // The effort is the level of a model the catalogue lists by its base id
        // (`gemini-3.8-flash` with `high`). An id that carries a level of its own (one a session
        // kept from before, or one listed alone) takes no `--effort`: the CLI refuses a level
        // beside it that is not the id's own, and refuses `--effort` for a model with no levels.
        // With no model chosen the CLI applies `--effort` to its default model (1.5.14: `--effort
        // high` alone sent the default `gemini-3.1-pro` a larger thinking budget than `low` or none
        // did). A level that model does not have is no failure in the session's terminal: the
        // interactive CLI draws a warning naming the levels it has and the one it uses instead
        // (`gemini-3.1-pro has no "medium" effort (available: low, high). Using "Gemini 3.1 Pro
        // (Low)" instead.`) and goes on; only `-p` refuses it. Which model is the default is the
        // CLI's own choice, so it is not refused here.
        if let Some(effort) = spec.effort.filter(|_| {
            spec.model
                .is_none_or(|model| models::split_level(model).is_none())
        }) {
            args.push("--effort".to_string());
            args.push(effort.to_string());
        }
        // With none chosen nothing is passed, and the CLI's own settings decide: auto for a
        // signed-in account, whose default mode it is (1.5.14).
        match spec.settings.choice("approval_mode") {
            Some("accept_edits") => {
                args.push("--mode".to_string());
                args.push("accept-edits".to_string());
            }
            Some("auto") => {
                args.push("--mode".to_string());
                args.push("auto".to_string());
            }
            Some("skip_permissions") => {
                args.push("--dangerously-skip-permissions".to_string());
            }
            _ => {}
        }

        // The task is never the CLI's own first prompt (`--prompt-interactive`): a prompt given at
        // launch runs before the CLI has started its MCP servers, so its first model request
        // offers no `call_mcp_tool` and the session cannot report to its owner in that turn, and
        // in a folder the CLI does not trust it runs behind the trust screen, before anyone
        // answered it (1.5.14). It is the session's first message instead.
        let first_message = spec
            .task
            .filter(|_| spec.resume_agent_session_id.is_none())
            .map(str::to_string);

        Ok(LaunchPlan {
            args,
            env: vec![(STATUS_REPORTS_VAR.to_string(), "on".to_string())],
            first_message,
            notice: launch.notice,
            ..LaunchPlan::default()
        })
    }

    /// The CLI offers its model `call_mcp_tool` only once a server has answered its listing, and
    /// does not wait for that itself: a message written before then runs a turn that cannot call
    /// Octoboard's tools.
    fn awaits_tool_listing(&self, _spec: &LaunchSpec<'_>, _plan: &LaunchPlan) -> Option<Duration> {
        Some(TOOL_LISTING_WAIT)
    }

    /// The title the launch's own title command printed ([`title`]).
    fn task_title(&self, raw: &str, _project_names: &[String]) -> Option<String> {
        title::task_title(raw)
    }

    /// The title command's helper ([`title`]).
    fn run_helper(&self, name: &str, args: &[String]) -> Option<i32> {
        (name == title::HELPER).then(|| title::run_helper(args))
    }

    /// The leftovers of an earlier layout, beside the plan's notice of a settings file not read.
    fn further_launch_notices(&self, spec: &LaunchSpec<'_>, _plan: &LaunchPlan) -> Vec<Notice> {
        home::leftovers_notice(spec).into_iter().collect()
    }
}
