//! The plugin registries that record the path a plugin was installed under. An account that
//! shares its plugin directory installs into the default directory through its own link, so the
//! registry — itself in the default directory — names the plugin by the account's spelling
//! (`<account>/plugins/…`). Once the account stops sharing, that spelling leads into the account's
//! own directory, where the plugin is not, for every account that still shares; rewriting it to
//! the default directory's spelling keeps the plugin usable there.

use std::io;
use std::path::{Path, PathBuf};

use super::items::catalogued_items;
use super::link::Dirs;
use super::moves::resolve;
use crate::adapter::files::LARGE_FILE_LIMIT;
use crate::adapter::provider;
use crate::hostfs::json_paths::{below, rewrite_paths, Select};
use crate::protocol::{AgentId, SharingCause, SharingItemFailure, SharingLeft};

/// The registries `agent` keeps, by the shared directory item they sit in and their path inside
/// it.
pub(super) fn registries(agent: AgentId) -> &'static [(&'static str, &'static str)] {
    provider(agent)
        .accounts()
        .and_then(|ops| ops.plugin_registries())
        .unwrap_or(&[])
}

/// Points what `agent`'s plugin registries in the default directory record by the account's
/// spelling at the default directory's own, so nothing there leads through the account's
/// directory: an account that shares installs through its own link, and the registry records the
/// plugin under the spelling it was installed by. `each` is told the item of every registry as it
/// is reached.
///
/// A registry is rewritten only where the two spellings name one place, its item in the
/// account's directory leading to the default directory's, or where the account's directory is
/// gone, so that its spelling leads nowhere. An item of the account's own, or one that is missing
/// from a directory still there, is left alone. An item that cannot be looked up is answered as
/// a failure, like a registry that could not be rewritten, the item left linked: the caller must
/// not unlink it while the registry may still name the account's path.
///
/// A registry that names nothing of the account's is not written. One that does is replaced only
/// if it still holds the bytes that were read, which narrows, without closing, the window in
/// which an install an agent saves at that moment is lost: an agent's own process takes part in
/// no lock of this daemon's.
pub(super) fn keep(
    agent: AgentId,
    dirs: &Dirs,
    each: &mut dyn FnMut(&str),
) -> Vec<SharingItemFailure> {
    let mut failures: Vec<SharingItemFailure> = Vec::new();
    let account_gone = matches!(resolve(&dirs.account), Ok(None));
    for (name, file) in registries(agent) {
        each(name);
        let Some(item) = catalogued_items(agent)
            .iter()
            .find(|item| item.name == *name)
        else {
            continue;
        };
        // Told once per item, however many registries it holds.
        let mut not_looked_up = |err: io::Error| {
            if failures.iter().all(|told| told.item != *name) {
                failures.push(SharingItemFailure::io(item, SharingLeft::Linked, err));
            }
        };
        let shared = match resolve(&dirs.default.join(name)) {
            Ok(Some(shared)) => shared,
            // No such item in the default directory, so no registry in it.
            Ok(None) => continue,
            Err(err) => {
                not_looked_up(err);
                continue;
            }
        };
        // Whatever becomes of this registry now: what a rewrite of it cut short by the daemon's
        // end left staged beside it would otherwise stay in the default directory.
        crate::hostfs::replace::remove_stale_staged(&shared.join(file));
        match resolve(&dirs.account.join(name)) {
            Ok(Some(leads)) if leads == shared => {}
            Ok(None) if account_gone => {}
            Ok(_) => continue,
            Err(err) => {
                not_looked_up(err);
                continue;
            }
        }
        let from = [dirs.account.join(name), dirs.account_real.join(name)];
        if let Err(err) = repoint(&shared.join(file), &from, &shared) {
            failures.push(SharingItemFailure::new(
                item,
                SharingLeft::Linked,
                SharingCause::of(&err),
                format!("`{file}` not repointed: {err}"),
            ));
        }
    }
    failures
}

/// Rewrites, in `registry`, every string that is one of `from` or lies below it to the same path
/// under `to`, and nothing else of the file (`crate::hostfs::json_paths`): a registry that is not
/// there, or names nothing below `from`, is left as it is, byte for byte. One that is no regular
/// file, or too large to read whole, is an error, and so is one that is not JSON while its text
/// holds one of `from`: it may name the account's directory in a way that cannot be rewritten.
pub(super) fn repoint(registry: &Path, from: &[PathBuf], to: &Path) -> io::Result<()> {
    let moved = |path: &Path| {
        from.iter()
            .find_map(|prefix| path.strip_prefix(prefix).ok())
            .map(|rest| below(to, rest))
    };
    let names_from = |text: &str| from.iter().any(|path| holds(text, path));
    rewrite_paths(registry, LARGE_FILE_LIMIT, Select::Any, &moved, &names_from).map(|_| ())
}

/// Whether `text`, which does not read as JSON, holds `path`: as it is spelled, or as a JSON
/// string spells it (a Windows path's separators doubled).
fn holds(text: &str, path: &Path) -> bool {
    let spelled = path.to_string_lossy();
    if text.contains(spelled.as_ref()) {
        return true;
    }
    serde_json::to_string(spelled.as_ref()).is_ok_and(|quoted| {
        let escaped = &quoted[1..quoted.len() - 1];
        escaped != spelled && text.contains(escaped)
    })
}
