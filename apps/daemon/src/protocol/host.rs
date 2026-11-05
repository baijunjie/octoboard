//! What the snapshot tells of the machine the daemon runs on.

use serde::Serialize;

use crate::platform::names::PathStyle;
use crate::platform::{HostFacts, HostOs, SettingsName, TrashName};

/// The operating system of the daemon's host.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HostOsName {
    Macos,
    Linux,
    Windows,
}

/// How the daemon's host writes an absolute path.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HostPathStyle {
    Unix,
    Windows,
}

/// How the daemon's host names the place a move to the Trash puts things.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HostTrashName {
    Trash,
    RecycleBin,
}

/// What the daemon's host calls the app its system settings are changed in, for wording a remedy
/// that has the user change one there.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HostSettingsName {
    SystemSettings,
    Settings,
}

/// The daemon host's own facts, which a client takes from the host and never from its own
/// platform, since the two may be different machines.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct HostInfo {
    pub os: HostOsName,
    /// How the host writes an absolute path. Paths travel in the host's own spelling; a client
    /// uses this only to split and shorten them for display.
    pub path_style: HostPathStyle,
    /// Whether the host can make the file links configuration sharing needs. When false,
    /// `set_account_sharing` is refused with `sharing_unavailable_on_host`.
    pub file_links: bool,
    /// How the host names its Trash, for wording what a removal does there.
    pub trash: HostTrashName,
    /// The host's system settings app, for wording a setting to change there.
    pub settings: HostSettingsName,
}

impl From<HostFacts> for HostInfo {
    fn from(facts: HostFacts) -> Self {
        Self {
            os: match facts.os {
                HostOs::Macos => HostOsName::Macos,
                HostOs::Linux => HostOsName::Linux,
                HostOs::Windows => HostOsName::Windows,
            },
            path_style: match facts.path_style {
                PathStyle::Unix => HostPathStyle::Unix,
                PathStyle::Windows => HostPathStyle::Windows,
            },
            file_links: facts.file_links,
            trash: match facts.trash {
                TrashName::Trash => HostTrashName::Trash,
                TrashName::RecycleBin => HostTrashName::RecycleBin,
            },
            settings: match facts.settings {
                SettingsName::SystemSettings => HostSettingsName::SystemSettings,
                SettingsName::Settings => HostSettingsName::Settings,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_host_is_announced_in_the_wire_spelling() {
        let host = serde_json::to_value(HostInfo::from(HostFacts {
            os: HostOs::Windows,
            path_style: PathStyle::Windows,
            file_links: false,
            trash: TrashName::RecycleBin,
            settings: SettingsName::Settings,
        }))
        .unwrap();
        assert_eq!(
            host,
            serde_json::json!({
                "os": "windows",
                "path_style": "windows",
                "file_links": false,
                "trash": "recycle_bin",
                "settings": "settings",
            })
        );
        let linux = serde_json::to_value(HostInfo::from(HostFacts {
            os: HostOs::Linux,
            path_style: PathStyle::Unix,
            file_links: true,
            trash: TrashName::Trash,
            settings: SettingsName::Settings,
        }))
        .unwrap();
        assert_eq!(linux["trash"], "trash");
        assert_eq!(linux["settings"], "settings");
        let here = serde_json::to_value(HostInfo::from(HostFacts::current())).unwrap();
        assert_eq!(here["os"] == "macos", cfg!(target_os = "macos"));
        assert_eq!(
            here["path_style"],
            if cfg!(windows) { "windows" } else { "unix" }
        );
        assert_eq!(
            here["trash"],
            if cfg!(windows) {
                "recycle_bin"
            } else {
                "trash"
            }
        );
        assert_eq!(
            here["settings"] == "system_settings",
            cfg!(target_os = "macos")
        );
    }
}
