//! The only place OS-specific code lives: everything the daemon asks of the host that differs by
//! operating system goes through a neutral signature here, and the rest of the crate neither names
//! `libc` or `std::os` nor asks which OS it runs on. A capability module is selected by `cfg` at
//! module level, as `proc_table` selects its reader; a fact business code must branch on is
//! exposed as a value ([`HostFacts`]), not as an OS test.
//!
//! This module uses nothing else in the crate: it sits in the foundation layer, so any module may
//! use it.

pub(crate) mod fs;
pub(crate) mod names;
pub(crate) mod open;
pub(crate) mod policy;
pub(crate) mod proc;
pub(crate) mod proc_table;
pub(crate) mod random;
#[cfg(windows)]
mod registry;
pub(crate) mod shell;
pub(crate) mod term;
#[cfg(test)]
mod test_helper;

/// The operating system the daemon runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostOs {
    Macos,
    Linux,
    Windows,
}

impl HostOs {
    /// The operating system this daemon runs on.
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

/// How the host's own interface names the place a move to the Trash ([`fs::trash_context`])
/// puts things.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrashName {
    Trash,
    RecycleBin,
}

impl TrashName {
    /// The name on the host this daemon runs on.
    pub fn current() -> Self {
        if cfg!(windows) {
            Self::RecycleBin
        } else {
            Self::Trash
        }
    }
}

/// What the host's own interface calls the app its system settings are changed in: System
/// Settings on macOS, Settings on Windows and, by no name of its own, on Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsName {
    SystemSettings,
    Settings,
}

impl SettingsName {
    /// The name on the host this daemon runs on.
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::SystemSettings
        } else {
            Self::Settings
        }
    }
}

/// What the host is, as the protocol announces it (`snapshot.host`). The home directory is not
/// among them: `crate::paths::known_home_dir` is what the snapshot's `home_dir` reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostFacts {
    pub os: HostOs,
    pub path_style: names::PathStyle,
    /// Whether the host can make the file links configuration sharing needs.
    pub file_links: bool,
    pub trash: TrashName,
    pub settings: SettingsName,
}

impl HostFacts {
    /// The facts of the host this daemon runs on.
    pub fn current() -> Self {
        Self {
            os: HostOs::current(),
            path_style: names::PATH_STYLE,
            file_links: fs::file_links_available(),
            trash: TrashName::current(),
            settings: SettingsName::current(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_current_host_facts_name_this_machine() {
        let facts = HostFacts::current();
        assert_eq!(facts.os == HostOs::Macos, cfg!(target_os = "macos"));
        assert_eq!(facts.path_style, names::PATH_STYLE);
        assert_eq!(facts.file_links, fs::file_links_available());
        assert_eq!(facts.trash == TrashName::RecycleBin, cfg!(windows));
        assert_eq!(
            facts.settings == SettingsName::SystemSettings,
            cfg!(target_os = "macos")
        );
    }
}
