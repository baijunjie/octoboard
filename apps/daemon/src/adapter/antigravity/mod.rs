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
//!
//! What a launch of Octoboard's cannot prepare (the directory flag not taken, a file of the user's
//! that cannot be used, a configuration still in an earlier layout) fails the plan, and the
//! session starts bare against the user's own home. A launch that exits as it starts, refusing one
//! of the settings it was given, is started again without that setting
//! ([`AntigravityProvider::reduced_plan`]).

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

use super::{AgentProvider, CliPackaging, EarlyExit, FirstHook, LaunchPlan, LaunchSpec};
use crate::hooks::HookInterpreter;
use crate::protocol::{Code, Notice};
use crate::providers::antigravity::LAUNCH_SETTING_NOT_TAKEN;
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

/// The option that names the file of a launch's settings.
const SETTINGS_OPTION: &str = "--settings";

/// The settings every launch gives the CLI ([`AntigravityProvider::launch_settings`]); a console
/// session's launch adds [`TRUSTED_WORKSPACES`].
const PERMISSIONS: &str = "permissions";
const FEEDBACK_SURVEY: &str = "showFeedbackSurvey";
const TRUSTED_WORKSPACES: &str = "trustedWorkspaces";

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
    ///
    /// `left_out` is a key of these that the CLI refused at an earlier launch of the session
    /// ([`Self::reduced_plan`]): it is not given, and the user's own setting of that name, if any,
    /// stands. `None` where it is not one of these keys.
    fn launch_settings(
        user_allow: &[Value],
        title: Value,
        trusted_console: Option<&std::path::Path>,
        left_out: Option<&str>,
    ) -> Option<String> {
        let ours = Value::String(format!("mcp({}/*)", session_tools::catalogue::SERVER_KEY));
        let mut allow: Vec<Value> = user_allow
            .iter()
            .filter(|rule| **rule != ours)
            .cloned()
            .collect();
        allow.push(ours);
        let mut settings = serde_json::Map::new();
        settings.insert(PERMISSIONS.to_string(), json!({ "allow": allow }));
        settings.insert(FEEDBACK_SURVEY.to_string(), Value::Bool(false));
        settings.insert(title::SETTING.to_string(), title);
        if let Some(folder) = trusted_console {
            settings.insert(TRUSTED_WORKSPACES.to_string(), json!([folder]));
        }
        if let Some(key) = left_out {
            settings.remove(key)?;
        }
        Some(Value::Object(settings).to_string())
    }

    /// The plan of a launch, leaving out the setting `left_out` names ([`Self::launch_settings`]);
    /// an error where that is not a setting the launch gives.
    fn plan_leaving_out(spec: &LaunchSpec<'_>, left_out: Option<&str>) -> Result<LaunchPlan> {
        // Fails for a CLI that does not take the directory flag before asking it of the settings.
        let launch = home::build(spec)?;

        // The settings go in a file, not in the argument itself: they repeat the user's own
        // allow rules, which may hold command lines.
        let title = title::command_setting(spec.self_exe, &launch.dir);
        let settings = Self::launch_settings(
            &launch.user_allow,
            title,
            (spec.kind == crate::protocol::SessionKind::Console).then_some(spec.cwd),
            left_out,
        )
        .ok_or_else(|| anyhow::anyhow!("no launch setting {left_out:?} to leave out"))?;
        let settings = home::write_settings(&launch.dir, &settings)?;
        let mut args = vec![
            home::directory_argument(&launch.dir),
            SETTINGS_OPTION.to_string(),
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

        let first_message = first_message(spec);

        // The notice of a settings file of the user's that the CLI does not read is one of the
        // further notices (`home::settings_notice`), so that the plan's own can say what a reduced
        // plan left out.
        Ok(LaunchPlan {
            args,
            env: vec![(STATUS_REPORTS_VAR.to_string(), "on".to_string())],
            first_message,
            notice: left_out.map(Self::left_out_notice),
            ..LaunchPlan::default()
        })
    }

    /// The notice of the setting `key` of Octoboard's that a reduced plan leaves out.
    fn left_out_notice(key: &str) -> Notice {
        Notice::new(
            Code::of_provider(LAUNCH_SETTING_NOT_TAKEN),
            format!(
                "Antigravity CLI exited as it started, refusing the setting `{key}` that \
                     Octoboard gives a session, so it was started again without that setting \
                     and runs without what it gives"
            ),
            &[("setting", key)],
        )
        // It holds for the whole launch, which was given the settings once.
        .standing()
    }
}

/// A new conversation's task, the plan and the bare plan alike: never the CLI's own first prompt
/// (`--prompt-interactive`), since a prompt given at launch runs before the CLI has started its MCP
/// servers, so its first model request offers no `call_mcp_tool` and the session cannot report to
/// its owner in that turn, and in a folder the CLI does not trust it runs behind the trust screen,
/// before anyone answered it (1.5.14). It is the session's first message instead. A resume is
/// given no task.
fn first_message(spec: &LaunchSpec<'_>) -> Option<String> {
    spec.task
        .filter(|_| spec.resume_agent_session_id.is_none())
        .map(str::to_string)
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

    /// The login is one item of the system keychain, whatever directory the CLI is given: an
    /// account with a directory of its own would run as the default account under another name,
    /// in a plain terminal as much as through Octoboard.
    fn launch_refusal(&self, spec: &LaunchSpec<'_>) -> Result<()> {
        if spec.config_dir.is_some() {
            return Err(crate::protocol::CodedError::provider_unsupported(
                AgentId::ANTIGRAVITY,
                crate::protocol::ProviderOperation::OwnedAccounts,
            ));
        }
        Ok(())
    }

    /// `--conversation <id>` for a resume and `--model`; a new conversation's task is the
    /// session's first message, as in the plan, since a prompt given at launch runs behind the
    /// trust screen.
    fn bare_plan(&self, spec: &LaunchSpec<'_>) -> super::BarePlan {
        let mut args = Vec::new();
        if let Some(id) = spec.resume_agent_session_id {
            args.push("--conversation".to_string());
            args.push(id.to_string());
        }
        if let Some(model) = spec.model {
            args.push("--model".to_string());
            args.push(model.to_string());
        }
        super::BarePlan {
            args,
            shaped_prompt: None,
            first_message: first_message(spec),
        }
    }

    /// A new conversation gets its id from the CLI, which its first turn's hook reports; a resume
    /// names it.
    fn plan(&self, spec: &LaunchSpec<'_>) -> Result<LaunchPlan> {
        Self::plan_leaving_out(spec, None)
    }

    /// The plan again without the one setting the early exit names, where it holds the CLI's
    /// refusal of `--settings` (an unknown key, or a value it does not take: the line a release
    /// that drops or retypes one of Octoboard's settings would end the launch with, measured on
    /// 1.5.14 to end an interactive launch at once, before it draws anything, with status 1) and
    /// the key is one the launch gave. Anything else gives nothing and the launch goes bare: an
    /// option the CLI does not take ends it another way (`flags provided but not defined`, status
    /// 2), and the reduced plan has no other option to leave out, the directory flag being probed
    /// before the plan. The notice of the setting left out is the plan's own
    /// ([`Self::left_out_notice`]).
    fn reduced_plan(&self, spec: &LaunchSpec<'_>, exit: &EarlyExit) -> Option<LaunchPlan> {
        let refused = home::settings_refused(&exit.output)?;
        Self::plan_leaving_out(spec, Some(refused.key())).ok()
    }

    /// The first hook comes with the first turn: nothing fires at launch, a new conversation's
    /// first turn fires `SessionStart` as it starts, every turn of a new or a resumed
    /// conversation `PreInvocation` at its first model request, and every turn that ends by
    /// itself `Stop` (1.5.14). Both the enhanced and the reduced plan register Octoboard's hooks.
    fn first_hook_due(&self, _spec: &LaunchSpec<'_>, _plan: &LaunchPlan) -> FirstHook {
        FirstHook::WithFirstTurn
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

    /// The settings file of the user's that the CLI does not read, then the leftovers of an
    /// earlier layout, beside the plan's notice of a setting a reduced plan left out.
    fn further_launch_notices(&self, spec: &LaunchSpec<'_>, _plan: &LaunchPlan) -> Vec<Notice> {
        home::settings_notice(spec)
            .into_iter()
            .chain(home::leftovers_notice(spec))
            .collect()
    }
}
