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

use super::diagnostics::{redact, truncate, LINE_LIMIT};
use crate::adapter::account_cli::{self, NotRun};
use crate::adapter::provider;
use crate::processes::subprocess::BoundedFailure;
use crate::protocol::{AgentId, CodedError, ProviderOperation};

/// How long a kick may take: the CLI's start, which can be slow on a loaded machine, and a model
/// round trip.
pub(super) const KICK_TIMEOUT: Duration = Duration::from_secs(60);

/// What a kick says.
pub(crate) const KICK_PROMPT: &str = "hi";

/// What replaces the CLI's own system prompt, which with its tool descriptions is most of what a
/// default request carries.
pub(crate) const KICK_SYSTEM_PROMPT: &str = "Reply with one word.";

/// The most output tokens a reply may have, where the CLI lets that be set. A one-word answer
/// needs a handful; this leaves room for a model that thinks first.
pub(crate) const REPLY_TOKEN_CAP: u32 = 128;

/// The most a kick may print. Its reply is not read, so this only keeps a runaway CLI in check.
const OUTPUT_LIMIT: usize = 64 * 1024;

/// What a kick runs: the arguments after the binary and the environment set over the shell's.
pub(crate) struct KickCommand {
    pub args: Vec<String>,
    pub env: Vec<(&'static str, String)>,
}

pub(crate) fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| arg.to_string()).collect()
}

/// Runs `agent`'s kick, the way [`super::probe::probe`] runs its probe, and waits for it to end.
/// Blocking. The reply of a kick that succeeds is not read: the request having been served is all
/// a kick is for; a failed one is read only for the last lines that say why. Setting `cancel`
/// stops the CLI and fails the kick, which is how a shutdown reaches one.
pub(super) fn kick(
    agent: AgentId,
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
    timeout: Duration,
    cancel: &AtomicBool,
) -> anyhow::Result<()> {
    let KickCommand { args, env } = CodedError::require_offered(
        agent,
        ProviderOperation::Kick,
        provider(agent)
            .accounts()
            .and_then(|ops| ops.kick_command()),
    )?;
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut run =
        account_cli::admitted_command(agent, shell_env, config_dir, &args).map_err(|refused| {
            match refused {
                NotRun::BinaryRefused(refusal) => anyhow::Error::new(refusal),
                other => anyhow::anyhow!(other.detail()),
            }
        })?;
    run.command.envs(env);
    let output = run
        .run(timeout, OUTPUT_LIMIT, Some(cancel))
        .map_err(|failure| match failure {
            BoundedFailure::Failed(err) => err,
            other => anyhow::anyhow!("{} {other}", agent.label()),
        })?;
    if !output.status.success() {
        bail!(
            "{} exited with {}: {}",
            agent.label(),
            output.status,
            failure_reason(&output)
        );
    }
    Ok(())
}

/// What says why a kick failed: the last line on stdout and the last on stderr, whichever were
/// printed, each redacted and the whole cut as any agent text bound for the log is. Claude Code in
/// print mode writes its error (a model it does not know) to stdout, while its stderr can end in
/// unrelated warnings, such as the settings an untrusted directory ignores. Neither line is
/// dropped, since which one holds the reason is not known for every CLI.
fn failure_reason(output: &std::process::Output) -> String {
    let stdout = redact(&account_cli::last_line(&output.stdout));
    let stderr = redact(&account_cli::last_line(&output.stderr));
    let reason = match (stdout.is_empty(), stderr.is_empty()) {
        (false, false) => format!("{stdout} (stderr: {stderr})"),
        (false, true) => stdout,
        _ => stderr,
    };
    truncate(&reason, LINE_LIMIT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fake_agent, ScratchDir};

    /// Long enough for a fake agent to start on a loaded machine, short enough to wait out.
    const SHORT: Duration = Duration::from_millis(700);

    fn fake(dir: &Path, agent: AgentId, body: &str) -> HashMap<String, String> {
        fake_agent(dir, agent.descriptor().binary, body)
    }

    /// A kick admitted after the account's directory was moved (a removal held it back) runs
    /// nothing, which would create the directory again.
    #[test]
    fn a_kick_admitted_after_the_directory_was_moved_runs_nothing() {
        let dir = ScratchDir::new("kick-moved");
        let ran = dir.join("ran");
        let env = fake(&dir, AgentId::CODEX, &format!("touch '{}'", ran.display()));
        let account = dir.join("account");
        let err = kick(
            AgentId::CODEX,
            &env,
            Some(&account),
            SHORT,
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert!(err.to_string().contains("does not exist"), "{err}");
        assert!(!ran.exists(), "the CLI was not run");
        assert!(!account.exists());
    }

    /// A provider with no kick command is refused with `provider_unsupported` before its CLI runs.
    #[test]
    fn a_provider_with_no_kick_command_is_refused_before_anything_runs() {
        let dir = ScratchDir::new("kick-unsupported");
        let ran = dir.join("ran");
        let env = fake(&dir, AgentId::TEST, &format!("touch '{}'", ran.display()));
        let err = kick(AgentId::TEST, &env, None, SHORT, &AtomicBool::new(false)).unwrap_err();
        let coded = err.downcast_ref::<CodedError>().expect("coded");
        assert_eq!(
            coded.code,
            crate::protocol::error_code::PROVIDER_UNSUPPORTED
        );
        assert_eq!(coded.params["agent"], "test_agent");
        assert_eq!(coded.params["operation"], "kick");
        assert!(!ran.exists(), "the CLI was not run");
    }

    /// Each agent's own choice of model and effort, and what it leaves out, in the order the
    /// arguments are given, run in the account's directory like a probe.
    #[test]
    fn each_agent_is_kicked_with_its_cheapest_model_and_lowest_effort() {
        let expected = [
            (
                AgentId::CLAUDE,
                "-p hi --model haiku --effort low --no-session-persistence --system-prompt \
                 Reply with one word. --tools  --disable-slash-commands --strict-mcp-config \
                 --mcp-config {\"mcpServers\":{}} --settings \
                 {\"disableAllHooks\":true,\"outputStyle\":\"default\"}",
            ),
            (
                AgentId::CODEX,
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
                AgentId::GROK,
                "-p hi -m grok-4.7 --effort low --system-prompt-override Reply with one word. \
                 --no-subagents --disable-web-search --no-plan --max-turns 1 --disallowed-tools \
                 run_terminal_cmd,read_file,search_replace,list_dir,grep,kill_task,todo_write,\
                 get_task_output,wait_tasks,task,scheduler_create,scheduler_delete,\
                 scheduler_list,monitor,search_tool,use_tool,workflow,enter_plan_mode,\
                 exit_plan_mode,ask_user_question,send_feedback,image_gen,image_edit,\
                 image_to_video,reference_to_video,write,web_search,web_fetch,AgentId",
            ),
        ];
        for (agent, args) in expected {
            let dir = ScratchDir::new("quota-kick-args");
            let var = crate::providers::layout::layout(agent).config_dir_var;
            let env = fake(
                &dir,
                agent,
                &format!(
                    "echo \"$*\" > '{d}/args'; echo \"${var}\" > '{d}/dir'",
                    d = dir.display(),
                ),
            );
            let account = dir.join("account");
            std::fs::create_dir(&account).unwrap();
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
            (AgentId::CLAUDE, "128 "),
            (AgentId::CODEX, " "),
            (
                AgentId::GROK,
                r#" {"models":{"max_completion_tokens":128}}"#,
            ),
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
        let env = fake(&dir, AgentId::CODEX, "echo 'model not found' >&2; exit 3");
        let err = kick(
            AgentId::CODEX,
            &env,
            None,
            crate::test_support::PATIENCE,
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert!(err.to_string().contains("model not found"), "{err}");

        let env = fake(&dir, AgentId::CODEX, "sleep 30");
        assert!(kick(AgentId::CODEX, &env, None, SHORT, &AtomicBool::new(false)).is_err());

        let empty = ScratchDir::new("quota-kick-missing");
        let env = HashMap::from([("PATH".to_string(), empty.display().to_string())]);
        let err = kick(AgentId::CODEX, &env, None, SHORT, &AtomicBool::new(false)).unwrap_err();
        assert!(err.to_string().contains("was not found"), "{err}");
    }

    /// A failed kick's error carries the last line of each stream that printed one, redacted and
    /// cut for the log.
    #[test]
    fn a_failed_kick_says_why_from_either_stream() {
        let dir = ScratchDir::new("quota-kick-reason");
        let failure = |body: &str| {
            let env = fake(&dir, AgentId::CLAUDE, body);
            kick(
                AgentId::CLAUDE,
                &env,
                None,
                crate::test_support::PATIENCE,
                &AtomicBool::new(false),
            )
            .unwrap_err()
            .to_string()
        };

        // A reason on stdout is kept beside a warning that ends stderr, as Claude Code prints them.
        let err = failure(
            "echo 'usage limit reached'; echo 'Ignoring settings: not trusted' >&2; exit 1",
        );
        assert!(
            err.ends_with(": usage limit reached (stderr: Ignoring settings: not trusted)"),
            "{err}"
        );

        // A secret's name ending stdout does not mask the label of the stderr line.
        let err = failure("echo 'invalid token'; echo 'warn' >&2; exit 1");
        assert!(err.ends_with(": invalid token (stderr: warn)"), "{err}");

        let err = failure("echo 'model refused'; exit 1");
        assert!(err.ends_with(": model refused"), "{err}");

        let err = failure("echo 'not allowed for someone@example.com'; exit 1");
        assert!(!err.contains("someone@example.com"), "{err}");

        let err = failure("head -c 2000 /dev/zero | tr '\\0' x; exit 1");
        assert!(
            err.ends_with(&format!(": {}…", "x".repeat(LINE_LIMIT))),
            "{err}"
        );
    }

    /// A kick run with the cancel flag already set fails as cancelled, without waiting out its CLI.
    #[test]
    fn a_cancelled_kick_stops_its_cli() {
        let dir = ScratchDir::new("quota-kick-cancel");
        let env = fake(&dir, AgentId::CODEX, "sleep 30");
        let err = kick(
            AgentId::CODEX,
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
        let env = fake(&dir, AgentId::CODEX, "yes | head -c 200000");
        let err = kick(
            AgentId::CODEX,
            &env,
            None,
            crate::test_support::PATIENCE,
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert!(err.to_string().contains("more than"), "{err}");
    }
}
