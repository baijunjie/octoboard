//! Creating a Codex home that does not exist yet, the one write Octoboard makes for Codex before the
//! CLI runs: Codex refuses a `CODEX_HOME` that is not there rather than creating it as Claude Code
//! does. The directory is made empty, so nothing of the user's is in it
//! (`docs/product/launching-agents.md`, "What Octoboard never modifies").

use std::path::Path;

/// Creates `dir`, the home Codex is about to be run in, with what leads to it, where it is missing.
pub(super) fn create_missing(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)
}
