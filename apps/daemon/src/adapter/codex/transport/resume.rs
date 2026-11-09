//! What a remote resume needs of the server before the terminal attaches: trust for the hooks the
//! launch generated, which a remote resume does not take on the terminal's bypass, and the
//! permission preset, which the terminal may not carry.

use std::path::Path;

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use super::super::private_server::ResumePermissions;

/// The parameters of the `thread/resume` that applies `permissions` to `thread` before the
/// terminal attaches. Codex restores a thread's saved permissions ahead of the server's defaults;
/// an explicit resume checkpoints the current choice.
pub(super) fn resume_params(thread: &str, cwd: &Path, permissions: ResumePermissions) -> Value {
    json!({
        "threadId": thread,
        "cwd": cwd,
        "permissions": permissions.profile,
        "approvalPolicy": permissions.approval_policy,
        "approvalsReviewer": permissions.reviewer,
    })
}

/// The override that trusts the hooks `command` was generated as, by Codex's own fingerprint of
/// each in `metadata` (a `hooks/list` reply), and nothing else. The state table goes as one
/// assignment, since a quoted dotted key of it is split wrongly at the `.toml` in Codex's keys.
/// Codex 0.166.0 merges it with the user's own `hooks.state` rather than replacing it: a hook the
/// user had trusted in `config.toml` stayed trusted alongside it.
pub(super) fn own_hook_trust(metadata: &Value, command: &str) -> Result<Vec<String>> {
    let mut entries_to_trust = Vec::new();
    let entries = metadata
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("Codex omitted hook metadata"))?;
    for hook in entries
        .iter()
        .filter_map(|entry| entry.get("hooks").and_then(Value::as_array))
        .flatten()
    {
        if hook.get("handlerType").and_then(Value::as_str) != Some("command")
            || hook.get("command").and_then(Value::as_str) != Some(command)
            || hook.get("source").and_then(Value::as_str) != Some("sessionFlags")
        {
            continue;
        }
        let key = hook
            .get("key")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("Codex omitted the generated hook key"))?;
        let hash = hook
            .get("currentHash")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("Codex omitted the generated hook fingerprint"))?;
        entries_to_trust.push(format!("{}={{trusted_hash={}}}", json!(key), json!(hash)));
    }
    if entries_to_trust.is_empty() {
        return Err(anyhow!(
            "Codex did not identify the generated session hooks"
        ));
    }
    Ok(vec![
        "-c".to_string(),
        format!("hooks.state={{{}}}", entries_to_trust.join(",")),
    ])
}
