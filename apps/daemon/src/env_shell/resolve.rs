//! Resolving a binary name on a snapshot's `PATH`.

use std::collections::HashMap;

use anyhow::Result;

use crate::protocol::{error_code, CodedError};

/// Resolves a bare binary name to an absolute path by searching the snapshot's `PATH`, mirroring
/// what the login shell would have found. Deliberately does not leave resolution to
/// `portable_pty`, whose own search tries `cwd.join(exe)` first and accepts it on `.exists()`
/// alone — so a project file named `claude` would silently shadow the real CLI.
pub fn resolve_binary(name: &str, env: &HashMap<String, String>) -> Result<String> {
    use std::os::unix::fs::PermissionsExt;

    let path_var = env.get("PATH").map(String::as_str).unwrap_or_default();
    for dir in path_var.split(':') {
        if dir.is_empty() {
            continue;
        }
        let candidate = std::path::Path::new(dir).join(name);
        let executable = candidate
            .metadata()
            .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false);
        if executable {
            return Ok(candidate.to_string_lossy().into_owned());
        }
    }
    Err(CodedError::raised(
        error_code::BINARY_NOT_FOUND,
        format!("`{name}` was not found on PATH in the snapshotted shell environment"),
        &[("binary", name)],
    ))
}
