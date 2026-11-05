//! The macOS half of the updater: the archive is installed in place as soon as it is downloaded, and
//! the restart only relaunches.

use tauri::AppHandle;
use tauri_plugin_updater::Update;

/// What goes between a channel's name and `.json` in the macOS manifests: nothing, so they are
/// `stable.json` and `beta.json`, the addresses every released macOS build reads and must go on
/// reading.
pub const MANIFEST_SUFFIX: &str = "";

/// Downloads `update` and replaces the `.app` with it; nothing is held back for the restart.
pub async fn install(
    update: &Update,
    on_chunk: impl FnMut(usize, Option<u64>),
) -> tauri_plugin_updater::Result<Option<Vec<u8>>> {
    update.download_and_install(on_chunk, || {}).await?;
    Ok(None)
}

/// Nothing to start: what was installed runs from the instance the restart launches, and nothing
/// is ever held.
pub fn install_for_restart(_app: &AppHandle, _held: Option<(Update, Vec<u8>)>) {}

/// Nothing to start: what was installed runs from the next launch, and nothing is ever held.
pub fn install_on_quit(_app: &AppHandle, _held: Option<(Update, Vec<u8>)>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_macos_manifests_keep_the_names_released_builds_read() {
        assert_eq!(MANIFEST_SUFFIX, "");
    }
}
