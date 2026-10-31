//! `codex unarchive`, which brings an archived rollout back before a resume.

use std::path::Path;

use crate::protocol::{error_code, Agent, CodedError};

/// How long `codex unarchive` may take; it only moves one file.
const UNARCHIVE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

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
                    format!("`{binary}` was not found on PATH in the shell environment"),
                    &[("binary", binary)],
                )
            })?;
    let output =
        crate::processes::subprocess::run_agent_with_timeout(&mut command, UNARCHIVE_TIMEOUT)
            .map_err(|err| failed(err.to_string()))?;
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
