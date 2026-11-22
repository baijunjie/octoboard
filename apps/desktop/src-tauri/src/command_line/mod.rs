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

use crate::system_changes::{Answer, Change, Cleared, Exception, Outcome};

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
    remove()
}

/// Removes every link of the application's, and answers where the tool stands. The one way a link is
/// removed: Settings' Remove calls it, and so does the clean-up before the application is
/// uninstalled. Blocking: it may wait for the system's administrator prompt.
pub fn remove() -> Result<State, Error> {
    platform::remove()
}

/// The command-line tool's link as the shell registers it for the clean-up before the application
/// is uninstalled (`crate::system_changes`): in effect while a link of the application's is
/// there, whatever it runs, and cleared by [`remove`], off the main thread since it may wait for
/// the administrator prompt.
pub struct Link;

impl Exception for Link {
    fn kind(&self) -> &'static str {
        "command_line"
    }

    fn in_effect<'a>(&'a self, _app: &'a tauri::AppHandle) -> Answer<'a, Change> {
        Box::pin(async {
            let state = blocking(platform::status).await?;
            Ok(listed(&state.map_err(|err| err.detail)?))
        })
    }

    fn clear_all<'a>(&'a self, _app: &'a tauri::AppHandle) -> Answer<'a, Cleared> {
        Box::pin(async { blocking(|| cleared(platform::status(), remove)).await? })
    }
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|err| err.to_string())
}

/// The link's path, where there is one.
fn link_path(state: &State) -> Option<&str> {
    match state {
        State::NotInstalled => None,
        State::Installed { path, .. } | State::OtherCopy { path } | State::Unresolved { path } => {
            Some(path)
        }
    }
}

/// The link as the clean-up lists it: an item wherever there is one.
fn listed(state: &State) -> Vec<Change> {
    link_path(state)
        .map(|path| Change::CommandLine { path: path.into() })
        .into_iter()
        .collect()
}

/// What removing the link came to, from where it stood `before`: nothing to tell, and nothing
/// removed (so no prompt raised), where there was none; cleared where `remove` left none; and left
/// with the failure where it could not be removed or is still there. A state that could not be read
/// is an error, since there is no link to name.
fn cleared(
    before: Result<State, Error>,
    remove: impl FnOnce() -> Result<State, Error>,
) -> Result<Vec<Cleared>, String> {
    let before = before.map_err(|err| err.detail)?;
    let Some(path) = link_path(&before) else {
        return Ok(Vec::new());
    };
    let outcome = match remove() {
        Ok(State::NotInstalled) => Outcome::Cleared,
        Ok(after) => Outcome::CommandLineLeft {
            error: Error::new(
                ErrorCode::Failed,
                link_path(&after).map(str::to_owned),
                "a link is still there after its removal",
            ),
        },
        Err(error) => Outcome::CommandLineLeft { error },
    };
    Ok(vec![Cleared {
        change: Change::CommandLine { path: path.into() },
        outcome,
    }])
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATH: &str = "/usr/local/bin/octoboardd";

    fn link() -> Change {
        Change::CommandLine { path: PATH.into() }
    }

    #[test]
    fn the_clean_up_lists_a_link_wherever_there_is_one() {
        assert!(listed(&State::NotInstalled).is_empty());
        for state in [
            State::Installed {
                path: PATH.into(),
                path_note: false,
            },
            State::OtherCopy { path: PATH.into() },
            State::Unresolved { path: PATH.into() },
        ] {
            assert_eq!(listed(&state), [link()], "{state:?}");
        }
    }

    #[test]
    fn the_clean_up_removes_nothing_where_there_is_no_link() {
        let removed = cleared(Ok(State::NotInstalled), || {
            panic!("no link, so nothing to remove and no prompt to raise")
        });
        assert_eq!(removed, Ok(Vec::new()));
    }

    #[test]
    fn the_clean_up_tells_what_removing_the_link_came_to() {
        let installed = || {
            Ok(State::Installed {
                path: PATH.into(),
                path_note: false,
            })
        };
        assert_eq!(
            cleared(installed(), || Ok(State::NotInstalled)),
            Ok(vec![Cleared {
                change: link(),
                outcome: Outcome::Cleared,
            }])
        );
        let refused = Error::new(ErrorCode::Refused, Some(PATH.into()), "cancelled");
        assert_eq!(
            cleared(installed(), || Err(refused.clone())),
            Ok(vec![Cleared {
                change: link(),
                outcome: Outcome::CommandLineLeft { error: refused },
            }])
        );
        // Removed, as far as the removal says, and yet a link is there.
        let still = cleared(installed(), installed).unwrap();
        assert!(matches!(
            still.as_slice(),
            [Cleared { outcome: Outcome::CommandLineLeft { error }, .. }]
                if error.code == ErrorCode::Failed && error.path.as_deref() == Some(PATH)
        ));
    }

    #[test]
    fn a_link_whose_state_cannot_be_read_fails_the_clearing() {
        let removed = cleared(Err(Error::failed("unreadable")), || {
            panic!("nothing is removed where the state is unknown")
        });
        assert_eq!(removed, Err("unreadable".to_string()));
    }
}
