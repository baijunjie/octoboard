//! Creates the two empty native directories Antigravity CLI would create on launch. The
//! per-launch configuration links to them so the CLI keeps its own conversation and project data.

use std::path::Path;

use anyhow::{Context, Result};

pub(super) fn ensure_account_dirs(app_data: &Path, projects: &Path) -> Result<()> {
    for path in [app_data, projects] {
        std::fs::create_dir_all(path).with_context(|| format!("creating {}", path.display()))?;
    }
    Ok(())
}
