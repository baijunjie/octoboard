//! `codex unarchive`, which brings an archived rollout back before a resume.

use std::path::Path;

use crate::accounts::cli_run::NotRun;
use crate::processes::subprocess::{run_bounded, Bounds};
use crate::protocol::{error_code, Agent, CodedError};

/// How long `codex unarchive` may take; it only moves one file.
const UNARCHIVE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// The most `codex unarchive` may print. Its output is not read, so this only keeps a runaway CLI
/// in check.
const OUTPUT_LIMIT: usize = 64 * 1024;

/// Brings the archived rollout of thread `id` back under `sessions/` by running `codex unarchive`
/// the way a session of the account would run Codex (`config_dir` exported as `CODEX_HOME`, the
/// default account left as the shell has it). A resume refuses an archived thread, so this has to
/// come first.
pub fn unarchive(
    shell_env: &std::collections::HashMap<String, String>,
    config_dir: Option<&Path>,
    id: &str,
) -> anyhow::Result<()> {
    let failed = |detail: String| {
        CodedError::raised(
            error_code::CODEX_UNARCHIVE_FAILED,
            format!("`codex unarchive` failed: {detail}"),
            &[("detail", &detail)],
        )
    };
    let mut command =
        crate::adapter::account_command(Agent::Codex, shell_env, config_dir, &["unarchive", id])
            .map_err(|binary| {
                CodedError::raised(
                    error_code::BINARY_NOT_FOUND,
                    NotRun::BinaryNotFound(binary).detail(),
                    &[("binary", binary)],
                )
            })?;
    let never = std::sync::atomic::AtomicBool::new(false);
    let output = run_bounded(
        &mut command,
        &Bounds::agent_query(UNARCHIVE_TIMEOUT, OUTPUT_LIMIT),
        &never,
    )
    .map_err(|failure| failed(failure.into_error().to_string()))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let message = stderr.trim().lines().next_back().unwrap_or_default();
    Err(failed(if message.is_empty() {
        output.status.to_string()
    } else {
        message.to_owned()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fake_agent, ScratchDir};

    fn code_of(err: &anyhow::Error) -> Option<crate::protocol::Code> {
        err.downcast_ref::<CodedError>().map(|coded| coded.code)
    }

    #[test]
    fn an_unarchive_that_exits_zero_succeeds_and_one_that_fails_reports_its_last_line() {
        let dir = ScratchDir::new("unarchive-ok");
        let env = fake_agent(&dir, "codex", r#"[ "$1 $2" = "unarchive t1" ] || exit 2"#);
        unarchive(&env, None, "t1").expect("unarchived");

        let env = fake_agent(
            &dir,
            "codex",
            "echo first >&2; echo 'no such thread' >&2; exit 1",
        );
        let err = unarchive(&env, None, "t1").unwrap_err();
        assert_eq!(code_of(&err), Some(error_code::CODEX_UNARCHIVE_FAILED));
        assert!(err.to_string().ends_with("no such thread"), "{err}");
    }

    /// Only the first 4 KiB of its stderr is kept, as for every other short query of an agent CLI,
    /// so a failure reports the last line within those.
    #[test]
    fn a_failure_reports_the_last_line_within_the_stderr_that_is_kept() {
        let dir = ScratchDir::new("unarchive-stderr");
        let env = fake_agent(
            &dir,
            "codex",
            "head -c 5000 /dev/zero | tr '\\0' x >&2; echo last >&2; exit 1",
        );
        let err = unarchive(&env, None, "t1").unwrap_err();
        let coded = err.downcast_ref::<CodedError>().unwrap();
        assert_eq!(coded.code, error_code::CODEX_UNARCHIVE_FAILED);
        assert_eq!(coded.params["detail"], "x".repeat(4096));
    }

    /// Its output is bounded like every other short query of an agent CLI: a CLI that floods it
    /// fails the unarchive rather than growing the daemon's memory.
    #[test]
    fn an_unarchive_that_floods_its_output_fails() {
        let dir = ScratchDir::new("unarchive-flood");
        let env = fake_agent(
            &dir,
            "codex",
            &format!("head -c {} /dev/zero; exit 0", OUTPUT_LIMIT + 1),
        );
        let err = unarchive(&env, None, "t1").unwrap_err();
        assert_eq!(code_of(&err), Some(error_code::CODEX_UNARCHIVE_FAILED));
    }
}
