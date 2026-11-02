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
        if !std::path::Path::new(dir).is_absolute() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn a_relative_path_entry_is_skipped() {
        let dir = ScratchDir::new("resolve-binary");
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let tool = bin.join("tool");
        std::fs::write(&tool, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();

        // A relative entry that does reach the fixture from the daemon's cwd: enough `..` to get to
        // `/`, then the bin directory's path without its leading `/`.
        let cwd = std::env::current_dir().unwrap();
        let mut relative_bin = std::path::PathBuf::new();
        for _ in cwd.components().skip(1) {
            relative_bin.push("..");
        }
        relative_bin.push(bin.strip_prefix("/").unwrap());
        assert!(relative_bin.is_relative() && relative_bin.join("tool").is_file());

        let relative = HashMap::from([("PATH".to_string(), relative_bin.display().to_string())]);
        let err = resolve_binary("tool", &relative).expect_err("only absolute entries count");
        assert!(format!("{err:#}").contains("tool"));

        let absolute = HashMap::from([("PATH".to_string(), format!("bin:{}", bin.display()))]);
        assert_eq!(
            resolve_binary("tool", &absolute).unwrap(),
            tool.to_string_lossy()
        );
    }
}
