//! Starting a usage window that has reset but not begun counting: a kick, one one-word request to
//! the agent's cheapest model, measured in `docs/agent-cli-reference.md` ("Starting a quota
//! window"). It is run the way a probe is (login-shell snapshot, the agent's configuration
//! directory variable pointed at the account's directory) and costs as little as the CLI lets it:
//! a one-line system prompt in place of its own, no tools, one turn, a capped reply, and as little
//! of the user's setup loaded as the CLI can leave out without losing its sign-in.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use anyhow::bail;

use super::probe::{CODEX_QUIET_FEATURES, STDERR_LIMIT};
use crate::adapter;
use crate::protocol::Agent;
use crate::subprocess::{run_bounded, BoundedFailure, Bounds, Reach};

/// How long a kick may take: the CLI's start, which can be slow on a loaded machine, and a model
/// round trip.
pub(super) const KICK_TIMEOUT: Duration = Duration::from_secs(60);

/// What a kick says.
const KICK_PROMPT: &str = "hi";

/// What replaces the CLI's own system prompt, which with its tool descriptions is most of what a
/// default request carries.
const KICK_SYSTEM_PROMPT: &str = "Reply with one word.";

/// The most output tokens a reply may have, where the CLI lets that be set. A one-word answer
/// needs a handful; this leaves room for a model that thinks first.
const REPLY_TOKEN_CAP: u32 = 128;

/// The most a kick may print. Its reply is not read, so this only keeps a runaway CLI in check.
const OUTPUT_LIMIT: usize = 64 * 1024;

/// What Codex is told on top of [`CODEX_QUIET_FEATURES`] to leave its tools out of the request.
/// Measured against an API key, only `shell_tool`, `goals` and `view_image` shrink it. The rest
/// (`unified_exec`, `multi_agent`, `sleep_tool` and `image_generation`) are kept off as a
/// precaution, as are the browser, computer, tool-suggestion and skill-search ones, for the tools
/// a ChatGPT sign-in may add.
const CODEX_TOOL_FEATURES: &[&str] = &[
    "-c",
    "features.shell_tool=false",
    "-c",
    "features.unified_exec=false",
    "-c",
    "features.sleep_tool=false",
    "-c",
    "features.view_image=false",
    "-c",
    "features.image_generation=false",
    "-c",
    "features.goals=false",
    "-c",
    "features.multi_agent=false",
    "-c",
    "features.browser_use=false",
    "-c",
    "features.browser_use_external=false",
    "-c",
    "features.in_app_browser=false",
    "-c",
    "features.computer_use=false",
    "-c",
    "features.tool_suggest=false",
    "-c",
    "features.skill_search=false",
];

/// Grok Build's built-in tools by id, all of which a kick takes out. `--tools ""` takes none out.
const GROK_TOOLS: &str = "run_terminal_cmd,read_file,search_replace,list_dir,grep,kill_task,\
    todo_write,get_task_output,wait_tasks,task,scheduler_create,scheduler_delete,scheduler_list,\
    monitor,search_tool,use_tool,workflow,enter_plan_mode,exit_plan_mode,ask_user_question,\
    send_feedback,image_gen,image_edit,image_to_video,reference_to_video,write,web_search,\
    web_fetch,Agent";

/// What a kick runs: the arguments after the binary and the environment set over the shell's.
struct KickCommand {
    args: Vec<String>,
    env: Vec<(&'static str, String)>,
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| arg.to_string()).collect()
}

/// The request that starts an agent's usage windows: its cheapest model at its lowest reasoning
/// effort, run without a terminal, with no session kept where the CLI can leave it out.
fn kick_command(agent: Agent) -> KickCommand {
    match agent {
        Agent::Claude => KickCommand {
            args: strings(&[
                "-p",
                KICK_PROMPT,
                "--model",
                "haiku",
                "--effort",
                "low",
                "--no-session-persistence",
                "--system-prompt",
                KICK_SYSTEM_PROMPT,
                "--tools",
                "",
                "--disable-slash-commands",
                "--strict-mcp-config",
                "--mcp-config",
                r#"{"mcpServers":{}}"#,
                // Settings merge over the user's. No hooks (the user's and the plugins') and the
                // default output style, whose text would otherwise ride along in the request.
                "--settings",
                r#"{"disableAllHooks":true,"outputStyle":"default"}"#,
            ]),
            env: vec![
                ("CLAUDE_CODE_MAX_OUTPUT_TOKENS", REPLY_TOKEN_CAP.to_string()),
                // No CLAUDE.md of the user's, which the request would carry as context.
                ("CLAUDE_CODE_DISABLE_CLAUDE_MDS", "1".to_string()),
            ],
        },
        Agent::Codex => {
            let mut args = strings(&[
                "exec",
                "--ephemeral",
                "--skip-git-repo-check",
                // Not the user's `config.toml`, whose MCP servers, `notify` program and plugins
                // would all start: a `-c` override merges into those tables rather than emptying
                // them. The sign-in is read from `CODEX_HOME` all the same.
                "--ignore-user-config",
                "--ignore-rules",
                "-m",
                "gpt-6-luna",
                "-c",
                r#"model_reasoning_effort="low""#,
                "-c",
                &format!(r#"instructions="{KICK_SYSTEM_PROMPT}""#),
                "-c",
                "include_permissions_instructions=false",
                "-c",
                "include_apps_instructions=false",
                "-c",
                "include_environment_context=false",
                "-c",
                "include_collaboration_mode_instructions=false",
                "-c",
                r#"web_search="disabled""#,
            ]);
            args.extend(strings(CODEX_QUIET_FEATURES));
            args.extend(strings(CODEX_TOOL_FEATURES));
            args.push(KICK_PROMPT.to_string());
            KickCommand {
                args,
                env: Vec::new(),
            }
        }
        // Grok Build can neither leave its session out nor the MCP servers and hooks of its own
        // configuration, but it can leave out what it scans of Claude Code's.
        Agent::Grok => KickCommand {
            args: strings(&[
                "-p",
                KICK_PROMPT,
                "-m",
                "grok-4.7",
                "--effort",
                "low",
                "--system-prompt-override",
                KICK_SYSTEM_PROMPT,
                "--no-subagents",
                "--disable-web-search",
                "--no-plan",
                "--max-turns",
                "1",
                "--disallowed-tools",
                GROK_TOOLS,
            ]),
            env: vec![
                (
                    "GROK_CONFIG",
                    format!(r#"{{"models":{{"max_completion_tokens":{REPLY_TOKEN_CAP}}}}}"#),
                ),
                ("GROK_CLAUDE_SKILLS_ENABLED", "0".to_string()),
                ("GROK_CLAUDE_MCPS_ENABLED", "0".to_string()),
                ("GROK_CLAUDE_AGENTS_ENABLED", "0".to_string()),
                ("GROK_CLAUDE_RULES_ENABLED", "0".to_string()),
                ("GROK_CLAUDE_HOOKS_ENABLED", "0".to_string()),
                ("GROK_TURN_SUMMARY", "0".to_string()),
                ("GROK_SESSION_RECAP", "0".to_string()),
            ],
        },
    }
}

/// Runs `agent`'s kick, the way [`super::probe::probe`] runs its probe, and waits for it to end.
/// Blocking. The reply is not read: the request having been served is all a kick is for. Setting
/// `cancel` stops the CLI and fails the kick, which is how a shutdown reaches one.
pub(super) fn kick(
    agent: Agent,
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
    timeout: Duration,
    cancel: &AtomicBool,
) -> anyhow::Result<()> {
    let KickCommand { args, env } = kick_command(agent);
    // The account's directory must not change sharing under the CLI while it runs.
    let _reading = crate::shared_config::reading_account(config_dir);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut command =
        adapter::account_command(agent, shell_env, config_dir, &args).map_err(|binary| {
            anyhow::anyhow!("`{binary}` was not found on PATH in the shell environment")
        })?;
    command.envs(env);
    let bounds = Bounds {
        timeout,
        stdout_limit: OUTPUT_LIMIT,
        stderr_limit: STDERR_LIMIT,
        reach: Reach::Session,
    };
    let output = run_bounded(&mut command, &bounds, cancel).map_err(|failure| match failure {
        BoundedFailure::Failed(err) => err,
        other => anyhow::anyhow!("{} {other}", agent.label()),
    })?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        bail!(
            "{} exited with {}: {}",
            agent.label(),
            output.status,
            detail.trim()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fake_agent, ScratchDir};

    /// Long enough for a fake agent to start on a loaded machine, short enough to wait out.
    const SHORT: Duration = Duration::from_millis(700);

    fn fake(dir: &Path, agent: Agent, body: &str) -> HashMap<String, String> {
        fake_agent(dir, adapter::adapter_for(agent).binary(), body)
    }

    /// Each agent's own choice of model and effort, and what it leaves out, in the order the
    /// arguments are given, run in the account's directory like a probe.
    #[test]
    fn each_agent_is_kicked_with_its_cheapest_model_and_lowest_effort() {
        let expected = [
            (
                Agent::Claude,
                "-p hi --model haiku --effort low --no-session-persistence --system-prompt \
                 Reply with one word. --tools  --disable-slash-commands --strict-mcp-config \
                 --mcp-config {\"mcpServers\":{}} --settings \
                 {\"disableAllHooks\":true,\"outputStyle\":\"default\"}",
            ),
            (
                Agent::Codex,
                "exec --ephemeral --skip-git-repo-check --ignore-user-config --ignore-rules -m \
                 gpt-6-luna -c model_reasoning_effort=\"low\" -c instructions=\"Reply with one \
                 word.\" -c include_permissions_instructions=false -c \
                 include_apps_instructions=false -c include_environment_context=false -c \
                 include_collaboration_mode_instructions=false -c web_search=\"disabled\" -c \
                 features.plugins=false -c features.remote_plugin=false -c features.apps=false \
                 -c features.hooks=false -c features.shell_tool=false -c \
                 features.unified_exec=false -c features.sleep_tool=false -c \
                 features.view_image=false -c features.image_generation=false -c \
                 features.goals=false -c features.multi_agent=false -c features.browser_use=false \
                 -c features.browser_use_external=false -c features.in_app_browser=false \
                 -c features.computer_use=false -c features.tool_suggest=false \
                 -c features.skill_search=false hi",
            ),
            (
                Agent::Grok,
                "-p hi -m grok-4.7 --effort low --system-prompt-override Reply with one word. \
                 --no-subagents --disable-web-search --no-plan --max-turns 1 --disallowed-tools \
                 run_terminal_cmd,read_file,search_replace,list_dir,grep,kill_task,todo_write,\
                 get_task_output,wait_tasks,task,scheduler_create,scheduler_delete,\
                 scheduler_list,monitor,search_tool,use_tool,workflow,enter_plan_mode,\
                 exit_plan_mode,ask_user_question,send_feedback,image_gen,image_edit,\
                 image_to_video,reference_to_video,write,web_search,web_fetch,Agent",
            ),
        ];
        for (agent, args) in expected {
            let dir = ScratchDir::new("quota-kick-args");
            let var = adapter::config_dir_var(agent);
            let env = fake(
                &dir,
                agent,
                &format!(
                    "echo \"$*\" > '{d}/args'; echo \"${var}\" > '{d}/dir'",
                    d = dir.display(),
                ),
            );
            let account = dir.join("account");
            kick(
                agent,
                &env,
                Some(&account),
                crate::test_support::PATIENCE,
                &AtomicBool::new(false),
            )
            .unwrap_or_else(|err| panic!("{agent:?}: {err}"));
            assert_eq!(
                std::fs::read_to_string(dir.join("args")).unwrap().trim(),
                args,
                "{agent:?}"
            );
            assert_eq!(
                std::fs::read_to_string(dir.join("dir")).unwrap().trim(),
                account.to_str().unwrap(),
                "{agent:?}"
            );
        }
    }

    /// The reply of a kick is capped through the environment where the CLI has no flag for it.
    #[test]
    fn the_reply_cap_reaches_the_cli_through_its_environment() {
        for (agent, expected) in [
            (Agent::Claude, "128 "),
            (Agent::Codex, " "),
            (Agent::Grok, r#" {"models":{"max_completion_tokens":128}}"#),
        ] {
            let dir = ScratchDir::new("quota-kick-env");
            let env = fake(
                &dir,
                agent,
                &format!(
                    "echo \"$CLAUDE_CODE_MAX_OUTPUT_TOKENS $GROK_CONFIG\" > '{}/env'",
                    dir.display()
                ),
            );
            kick(
                agent,
                &env,
                None,
                crate::test_support::PATIENCE,
                &AtomicBool::new(false),
            )
            .unwrap();
            assert_eq!(
                std::fs::read_to_string(dir.join("env")).unwrap().trim_end(),
                expected.trim_end(),
                "{agent:?}"
            );
        }
    }

    #[test]
    fn a_kick_that_fails_or_hangs_is_an_error() {
        let dir = ScratchDir::new("quota-kick-fails");
        let env = fake(&dir, Agent::Codex, "echo 'model not found' >&2; exit 3");
        let err = kick(
            Agent::Codex,
            &env,
            None,
            crate::test_support::PATIENCE,
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert!(err.to_string().contains("model not found"), "{err}");

        let env = fake(&dir, Agent::Codex, "sleep 30");
        assert!(kick(Agent::Codex, &env, None, SHORT, &AtomicBool::new(false)).is_err());

        let empty = ScratchDir::new("quota-kick-missing");
        let env = HashMap::from([("PATH".to_string(), empty.display().to_string())]);
        let err = kick(Agent::Codex, &env, None, SHORT, &AtomicBool::new(false)).unwrap_err();
        assert!(err.to_string().contains("was not found"), "{err}");
    }

    /// A kick run with the cancel flag already set fails as cancelled, without waiting out its CLI.
    #[test]
    fn a_cancelled_kick_stops_its_cli() {
        let dir = ScratchDir::new("quota-kick-cancel");
        let env = fake(&dir, Agent::Codex, "sleep 30");
        let err = kick(
            Agent::Codex,
            &env,
            None,
            crate::test_support::PATIENCE,
            &AtomicBool::new(true),
        )
        .unwrap_err();
        assert!(err.to_string().contains("was cancelled"), "{err}");
    }

    #[test]
    fn a_kick_that_prints_too_much_is_an_error() {
        let dir = ScratchDir::new("quota-kick-output");
        let env = fake(&dir, Agent::Codex, "yes | head -c 200000");
        let err = kick(
            Agent::Codex,
            &env,
            None,
            crate::test_support::PATIENCE,
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert!(err.to_string().contains("more than"), "{err}");
    }
}
