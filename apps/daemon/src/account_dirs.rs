//! An account's configuration directory as text: how what the user typed is stored, and how two
//! stored directories are compared. Nothing here touches the file system.

use anyhow::Result;

use crate::hostfs;
use crate::protocol::{error_code, Agent, CodedError};

/// What the user typed for one agent's config directory, as the absolute, lexically normalised
/// path an account's own directory is stored as. Required and non-blank: an account always has a
/// directory. Existence is not checked: the directory is created on first launch (by Claude Code
/// itself, by the Codex adapter for Codex), so a user pointing an account at a directory they are
/// about to create should not be stopped; the agent that needs one to already exist (Grok Build,
/// against a *pinned* source home) checks that itself, at launch.
pub(crate) fn normalize_account_dir(agent: Agent, text: &str) -> Result<String> {
    let text = text.trim();
    if text.is_empty() {
        return Err(CodedError::raised(
            error_code::FIELD_REQUIRED,
            "`config_dir` is required",
            &[("field", "config_dir")],
        ));
    }
    let dir = hostfs::expand(text);
    if !dir.is_absolute() {
        return Err(CodedError::raised(
            error_code::CONFIG_DIR_NOT_ABSOLUTE,
            format!(
                "the {} config directory must be an absolute path or start with `~/`",
                agent.label()
            ),
            &[("agent", agent.label())],
        ));
    }
    // Lexical only: a symlink stays as typed, so the stored path is what the user chose.
    let dir = hostfs::lexically_normalise(&dir);
    Ok(dir.to_string_lossy().into_owned())
}

/// The comparison key for an account directory collision. The stored text stays as typed; only
/// the key folds case on macOS, whose default file system is case-insensitive, so `~/.Claude` and
/// `~/.claude` read as one directory there. Other platforms compare byte for byte. Unicode
/// normalisation is not folded: no crate for it is in the dependency tree.
pub(crate) fn account_dir_key(dir: &str) -> String {
    if cfg!(target_os = "macos") {
        dir.to_lowercase()
    } else {
        dir.to_string()
    }
}
