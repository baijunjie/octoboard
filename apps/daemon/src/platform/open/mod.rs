//! The host's launchers: revealing a path in the file manager, opening it with its default
//! application, opening a terminal at a directory and opening an agent's desktop application by
//! its URL scheme. What each starts is chosen here; whether a path may be opened and the running
//! of the command are `host_open`'s.

pub mod native_app;
pub mod paths;
pub mod terminal;

use std::sync::LazyLock;

use serde::Deserialize;

/// The extensions of what the system launches or acts on rather than shows, shared with the
/// desktop shell and the UI through `config/launchable-extensions.json`, so that a security list is
/// kept in one place. All are matched without case.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchableExtensions {
    /// Matched on every component of a path below its root (a bundle is a directory carrying one):
    /// besides bundles, file types macOS acts on when opened, shell scripts and desktop entries. A
    /// folder with such a name is refused as well.
    pub any_component: Vec<String>,
    /// Matched on the final component only and only when it is a regular file, as a folder name
    /// (`vendor/github.com/`) the same text launches nothing: file types an interpreter or
    /// installer handles by default on macOS or Linux (Python launcher, Java Web Start, Wine for
    /// Windows programs, package and AppImage installers).
    pub final_file: Vec<String>,
    /// Matched like `final_file`, on Windows only, for what its shell launches or acts on when
    /// opened: a shortcut, a ClickOnce or Store package or installer, a saved search or library, a
    /// settings or Explorer command, a compiled help file, a script component, a setup information
    /// file, a troubleshooting pack, and a disk image Explorer mounts.
    pub windows_final_file: Vec<String>,
}

/// The shared lists, parsed once.
pub fn launchable_extensions() -> &'static LaunchableExtensions {
    static LISTS: LazyLock<LaunchableExtensions> = LazyLock::new(|| {
        serde_json::from_str(include_str!(
            "../../../../../config/launchable-extensions.json"
        ))
        .expect("config/launchable-extensions.json is the lists of launchable extensions")
    });
    &LISTS
}

/// The extensions of files this host's shell launches or acts on when opened, beyond those refused
/// on every host: the Windows list on Windows, none elsewhere. Matched like `final_file`.
pub fn launched_file_extensions() -> &'static [String] {
    if cfg!(windows) {
        &launchable_extensions().windows_final_file
    } else {
        &[]
    }
}
