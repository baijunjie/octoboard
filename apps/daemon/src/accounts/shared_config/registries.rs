//! The plugin registries that record the path a plugin was installed under. An account that
//! shares its plugin directory installs into the default directory through its own link, so the
//! registry — itself in the default directory — names the plugin by the account's spelling
//! (`<account>/plugins/…`). Once the account stops sharing, that spelling leads into the account's
//! own directory, where the plugin is not, for every account that still shares; rewriting it to
//! the default directory's spelling keeps the plugin usable there.

use std::io;
use std::path::{Path, PathBuf};

use crate::protocol::Agent;

/// The registries `agent` keeps, by the shared directory item they sit in and their path inside
/// it.
pub(super) fn registries(agent: Agent) -> &'static [(&'static str, &'static str)] {
    match agent {
        Agent::Claude => &[
            ("plugins", "installed_plugins.json"),
            ("plugins", "known_marketplaces.json"),
        ],
        Agent::Grok => &[("installed-plugins", "registry.json")],
        Agent::Codex => &[],
    }
}

/// Rewrites, in `registry`, every string that is one of `from` or lies below it to the same path
/// under `to`. Written through a file of its own and renamed over the registry, so a reader never
/// sees half of it; a registry that is not there, or names nothing below `from`, is left as it
/// is.
pub(super) fn repoint(registry: &Path, from: &[PathBuf], to: &Path) -> io::Result<()> {
    let text = match std::fs::read_to_string(registry) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    };
    let mut value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|err| io::Error::other(format!("`{}` is not JSON: {err}", registry.display())))?;
    if !rewrite(&mut value, from, to) {
        return Ok(());
    }
    let mut written = serde_json::to_string_pretty(&value).map_err(io::Error::other)?;
    written.push('\n');
    // A registry that is itself a link is written where it leads, keeping the link.
    let target = std::fs::canonicalize(registry)?;
    let temporary = target.with_file_name(format!(
        ".{}.octoboard-{}",
        target
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        std::process::id()
    ));
    std::fs::write(&temporary, written)?;
    if let Ok(metadata) = std::fs::metadata(&target) {
        std::fs::set_permissions(&temporary, metadata.permissions()).ok();
    }
    std::fs::rename(&temporary, &target).inspect_err(|_| {
        std::fs::remove_file(&temporary).ok();
    })
}

fn rewrite(value: &mut serde_json::Value, from: &[PathBuf], to: &Path) -> bool {
    match value {
        serde_json::Value::String(text) => {
            for prefix in from {
                if let Ok(rest) = Path::new(text.as_str()).strip_prefix(prefix) {
                    let moved = if rest.as_os_str().is_empty() {
                        to.to_path_buf()
                    } else {
                        to.join(rest)
                    };
                    *text = moved.to_string_lossy().into_owned();
                    return true;
                }
            }
            false
        }
        // Every element is visited, not only up to the first that changed.
        serde_json::Value::Array(items) => {
            let mut changed = false;
            for item in items {
                changed |= rewrite(item, from, to);
            }
            changed
        }
        serde_json::Value::Object(map) => {
            let mut changed = false;
            for item in map.values_mut() {
                changed |= rewrite(item, from, to);
            }
            changed
        }
        _ => false,
    }
}
