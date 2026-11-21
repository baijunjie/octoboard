//! The command-line tool of the macOS application: a symbolic link named `octoboardd` to the
//! `octoboardd` inside the application bundle, so that a terminal finds it by name. The user makes
//! it from Settings and nothing makes it at launch. The link points into the bundle and not at a
//! copy, so it runs the build an update puts there.
//!
//! Settings reads the state each time it opens (`command_line_status`) and offers install,
//! remove and reinstall to match (`command_line_install` does both of the last, replacing the
//! link). What the link is, where it goes and how it is made is `macos`; the other targets have
//! no such tool (`host_facts` says so), and their half answers that.

use serde::Serialize;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(not(target_os = "macos"))]
mod windows;
#[cfg(not(target_os = "macos"))]
use windows as platform;

/// Where the tool stands, as the UI words it. `path` is the link's own path.
#[derive(Serialize, Debug, PartialEq, Eq, Clone)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum State {
    NotInstalled,
    /// The link runs this application's `octoboardd`. `path_note` says the link is in a directory
    /// the user may still have to add to the PATH.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Installed {
        path: String,
        path_note: bool,
    },
    /// The link runs another copy of the application's `octoboardd`.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    OtherCopy {
        path: String,
    },
    /// The link leads nowhere: the copy it was made for is gone.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Unresolved {
        path: String,
    },
}

/// Why a command failed, for the UI to word: `code` says which case it is, `path` the link it is
/// about where there is one, and `detail` is the system's or the shell's own text, for a diagnostic
/// and never for the user.
#[derive(Serialize, Debug, PartialEq, Eq, Clone)]
pub struct Error {
    pub code: ErrorCode,
    pub path: Option<String>,
    pub detail: String,
}

#[derive(Serialize, Debug, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The administrator prompt was refused or cancelled.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Refused,
    /// A file that is not a link is where the link goes.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    NotALink,
    /// A link there leads to something that is not an application's `octoboardd`.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    ForeignLink,
    /// Anything else.
    Failed,
}

impl Error {
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn new(code: ErrorCode, path: Option<String>, detail: impl Into<String>) -> Self {
        Self {
            code,
            path,
            detail: detail.into(),
        }
    }

    pub fn failed(detail: impl Into<String>) -> Self {
        Self::new(ErrorCode::Failed, None, detail)
    }
}

/// Where the tool stands now.
#[tauri::command(async)]
pub fn command_line_status() -> Result<State, Error> {
    platform::status()
}

/// Makes the link to this application's `octoboardd`, replacing one made earlier, and answers where
/// the tool stands. Waits for the system's administrator prompt where it is raised.
#[tauri::command(async)]
pub fn command_line_install() -> Result<State, Error> {
    platform::install()
}

/// Removes the link, and answers where the tool stands.
#[tauri::command(async)]
pub fn command_line_remove() -> Result<State, Error> {
    platform::remove()
}
