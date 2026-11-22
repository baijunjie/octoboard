//! Opening the application when the user logs in, which the UI's Settings switch (Launch at login)
//! offers. The switch shows the system's own state, read when the section opens, and changes it
//! through `set_launch_at_login`: the user can also turn the entry off in the system's own list
//! (System Settings' Login Items on macOS, Task Manager's Startup apps on Windows), so nothing here
//! is remembered.
//!
//! A launch the system made for the entry is told apart from any other (`capture_launch`), which is
//! what lets the window stay hidden after it when the user keeps the app running in the background
//! (`background::reveal_main_window`): the choice to run in the background is the UI's, handed over
//! when it reveals the window.
//!
//! What the entry is on each platform is in `macos` and `windows`; the latter serves every target
//! but macOS, Linux included, where there is no entry to make.

use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::AppHandle;

use crate::system_changes::{on_main_thread, Answer, Change, Cleared, Exception, Outcome};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(not(target_os = "macos"))]
mod windows;
#[cfg(not(target_os = "macos"))]
use windows as platform;

/// The command-line argument of the Windows entry, which is how a launch by it is recognised.
#[cfg_attr(target_os = "macos", allow(dead_code))]
const LOGIN_ARGUMENT: &str = "--launched-at-login";

/// Where the entry stands, as the UI words it.
#[derive(Serialize, Debug, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Enabled,
    Disabled,
    /// Registered, but the system waits for the user to allow it (macOS, System Settings' Login
    /// Items).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    RequiresApproval,
}

/// Whether this process was started by the system for the login entry, until the window is first
/// revealed (`take_login_launch`).
static LOGIN_LAUNCH: AtomicBool = AtomicBool::new(false);

/// Whether this process was started by the system for the login entry, once: answering clears it,
/// as only the first reveal of a process can be that launch (`background::reveal_main_window`).
pub fn take_login_launch() -> bool {
    LOGIN_LAUNCH.swap(false, Ordering::SeqCst)
}

/// Whether the system started this process for the login entry and the window has not been revealed
/// yet, which leaves the reveal to decide whether the window shows at all.
pub fn login_launch_pending() -> bool {
    LOGIN_LAUNCH.load(Ordering::SeqCst)
}

/// Whether this machine can open the app at login; what the host facts announce.
pub fn supported() -> bool {
    platform::supported()
}

/// Records whether the system started this process for the login entry. Called once, from `setup`
/// on the main thread, where the macOS launch event is still the current one.
pub fn capture_launch() {
    LOGIN_LAUNCH.store(platform::launched_at_login(), Ordering::SeqCst);
}

/// Whether `args` carry the argument the Windows entry starts the app with.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub(crate) fn has_login_argument<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    args.into_iter()
        .any(|arg| arg.as_ref() == std::ffi::OsStr::new(LOGIN_ARGUMENT))
}

/// Whether the system opens the app at login now.
#[tauri::command]
pub fn launch_at_login_status() -> Result<Status, String> {
    platform::status()
}

/// Registers or removes the login entry, and answers where it stands afterwards.
#[tauri::command]
pub fn set_launch_at_login(enabled: bool) -> Result<Status, String> {
    if enabled {
        platform::set(true)
    } else {
        turn_off()
    }
}

/// Removes everything the entry is on this system, whether or not the switch shows it on (an entry
/// the user turned off in the system's own list is still there), and answers where it stands
/// afterwards. The one way the entry is removed: the switch turned off calls it, and so does the
/// clean-up before the application is uninstalled. Where there is no entry to make, there is none
/// to remove.
pub fn turn_off() -> Result<Status, String> {
    if !supported() {
        return Ok(Status::Disabled);
    }
    platform::set(false)
}

/// The login entry as the shell registers it for the clean-up before the application is
/// uninstalled (`crate::system_changes`): in effect while the system holds it, which includes an
/// entry the user turned off in the system's own list (Task Manager's Startup apps leaves it in the
/// registry), and cleared by [`turn_off`], which runs whatever the status says. The system is asked
/// on the main thread, as the switch's own commands ask it.
pub struct LoginEntry;

impl Exception for LoginEntry {
    fn kind(&self) -> &'static str {
        "launch_at_login"
    }

    fn in_effect<'a>(&'a self, app: &'a AppHandle) -> Answer<'a, Change> {
        Box::pin(async move { Ok(listed(on_main_thread(app, standing).await??)) })
    }

    fn clear_all<'a>(&'a self, app: &'a AppHandle) -> Answer<'a, Cleared> {
        Box::pin(async move {
            let clear = || {
                let before = standing();
                let turned_off = turn_off();
                cleared(before, turned_off, standing())
            };
            on_main_thread(app, clear).await
        })
    }
}

/// Where the entry stands for the clean-up: whether the system holds anything of it, and whether
/// the switch shows it off all the same.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct Standing {
    present: bool,
    turned_off: bool,
}

/// Where the entry stands now; absent where there is none to make.
fn standing() -> Result<Standing, String> {
    if !supported() {
        return Ok(Standing {
            present: false,
            turned_off: true,
        });
    }
    Ok(Standing {
        present: platform::present()?,
        turned_off: platform::status()? == Status::Disabled,
    })
}

/// The entry as the clean-up lists it: an item while the system holds anything of it.
fn listed(standing: Standing) -> Vec<Change> {
    if !standing.present {
        return Vec::new();
    }
    vec![Change::LaunchAtLogin {
        turned_off: standing.turned_off,
    }]
}

/// What turning the entry off came to, from where it stood `before`, what [`turn_off`] answered
/// and where it stands `after`: nothing to tell where nothing was there and nothing is, cleared
/// where it went, and left with the system's text where it could not be removed or is still there.
fn cleared(
    before: Result<Standing, String>,
    turned_off: Result<Status, String>,
    after: Result<Standing, String>,
) -> Vec<Cleared> {
    let was_there = !matches!(before, Ok(Standing { present: false, .. }));
    let outcome = match (turned_off, after) {
        (Err(detail), _) | (Ok(_), Err(detail)) => Outcome::LaunchAtLoginLeft { detail },
        (Ok(_), Ok(Standing { present: true, .. })) => Outcome::LaunchAtLoginLeft {
            detail: "the system still holds the login entry".into(),
        },
        (Ok(_), Ok(Standing { present: false, .. })) if was_there => Outcome::Cleared,
        (Ok(_), Ok(Standing { present: false, .. })) => return Vec::new(),
    };
    vec![Cleared {
        change: Change::LaunchAtLogin {
            turned_off: before.is_ok_and(|standing| standing.turned_off),
        },
        outcome,
    }]
}

/// Opens System Settings at its list of login items, where an entry waiting for approval is
/// allowed.
#[tauri::command]
pub fn open_login_items_settings() -> Result<(), String> {
    platform::open_settings()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_login_argument_is_recognised_among_others() {
        assert!(has_login_argument(["octoboard.exe", LOGIN_ARGUMENT]));
        assert!(!has_login_argument(["octoboard.exe", "--other"]));
        assert!(!has_login_argument(Vec::<String>::new()));
    }

    const ON: Standing = Standing {
        present: true,
        turned_off: false,
    };
    /// Turned off in the system's own list, where the entry stays.
    const OFF_BUT_THERE: Standing = Standing {
        present: true,
        turned_off: true,
    };
    const GONE: Standing = Standing {
        present: false,
        turned_off: true,
    };

    #[test]
    fn the_clean_up_lists_the_entry_while_the_system_holds_it_even_turned_off() {
        assert!(listed(GONE).is_empty());
        assert_eq!(listed(ON), [Change::LaunchAtLogin { turned_off: false }]);
        assert_eq!(
            listed(OFF_BUT_THERE),
            [Change::LaunchAtLogin { turned_off: true }]
        );
    }

    #[test]
    fn the_clean_up_tells_what_turning_the_entry_off_came_to() {
        let off = || Ok(Status::Disabled);
        // Nothing before and nothing after: nothing was in effect, so there is nothing to tell.
        assert!(cleared(Ok(GONE), off(), Ok(GONE)).is_empty());
        assert_eq!(
            cleared(Ok(ON), off(), Ok(GONE)),
            [Cleared {
                change: Change::LaunchAtLogin { turned_off: false },
                outcome: Outcome::Cleared,
            }]
        );
        // An entry the switch showed off is removed and told as such.
        assert_eq!(
            cleared(Ok(OFF_BUT_THERE), off(), Ok(GONE)),
            [Cleared {
                change: Change::LaunchAtLogin { turned_off: true },
                outcome: Outcome::Cleared,
            }]
        );
        let outcome = |before, turned_off, after| {
            let [told]: [Cleared; 1] = cleared(before, turned_off, after).try_into().unwrap();
            told.outcome
        };
        // Unreadable before: whatever it was, it is gone now.
        assert_eq!(
            outcome(Err("unreadable".into()), off(), Ok(GONE)),
            Outcome::Cleared
        );
        let refused = || Outcome::LaunchAtLoginLeft {
            detail: "refused".into(),
        };
        assert_eq!(outcome(Ok(ON), Err("refused".into()), Ok(ON)), refused());
        // Refused where nothing was seen before: told all the same.
        assert_eq!(
            outcome(Ok(GONE), Err("refused".into()), Ok(GONE)),
            refused()
        );
        // Turned off as far as the system says, and yet the entry is there, or cannot be read.
        assert!(matches!(
            outcome(Ok(ON), off(), Ok(OFF_BUT_THERE)),
            Outcome::LaunchAtLoginLeft { .. }
        ));
        assert_eq!(outcome(Ok(ON), off(), Err("refused".into())), refused());
    }

    #[test]
    fn a_status_is_worded_for_the_ui_in_camel_case() {
        assert_eq!(
            serde_json::to_string(&Status::RequiresApproval).unwrap(),
            "\"requiresApproval\""
        );
    }
}
