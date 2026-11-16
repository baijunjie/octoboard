//! What the remote daemons' menu items and a launch's arguments ask for, and doing it: the ids of
//! the items in the macOS Remote menu and Dock menu and of the menu bar icon's Open in browser line
//! (`OPEN_ITEM_PREFIX`), and the arguments of the jump list's tasks. The windows' system menus on
//! Windows number their items as `WM_SYSCOMMAND` commands instead (`menu/system_menu.rs`).

use tauri::AppHandle;

use super::{connect, page};
use crate::diagnostics::diagnostic;

/// The id of the menu item that shows the Remote devices page, in the macOS Remote menu and Dock
/// menu; the jump list's task for it is `DEVICES_ARGUMENT`.
pub const DEVICES_ITEM_ID: &str = "remote-devices-show";

/// What the id of a menu item that opens a saved remote daemon's window starts with, its entry's id
/// following.
pub const OPEN_ITEM_PREFIX: &str = "remote-window-open:";

/// The id of the menu item that shows the local daemon's window.
pub const MAIN_WINDOW_ITEM_ID: &str = "main-window-show";

/// What one of the remote daemons' menu items asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuAction {
    Devices,
    Open(String),
    MainWindow,
}

/// What the menu item `id` asks for, when it is one of the remote daemons' items.
pub fn menu_action(id: &str) -> Option<MenuAction> {
    match id {
        DEVICES_ITEM_ID => Some(MenuAction::Devices),
        MAIN_WINDOW_ITEM_ID => Some(MenuAction::MainWindow),
        _ => id
            .strip_prefix(OPEN_ITEM_PREFIX)
            .filter(|entry| !entry.is_empty())
            .map(|entry| MenuAction::Open(entry.to_owned())),
    }
}

/// The command-line argument a launch asks for the Remote devices page with (a taskbar jump list's
/// task); `OPEN_ARGUMENT` followed by an entry's id asks for that remote daemon's window,
/// and `MAIN_WINDOW_ARGUMENT` for the local daemon's.
pub const DEVICES_ARGUMENT: &str = "--remote-devices";
pub const OPEN_ARGUMENT: &str = "--remote-window-open";
pub const MAIN_WINDOW_ARGUMENT: &str = "--main-window-show";

/// What a launch's arguments ask of the remote daemons, the first such argument deciding: what a
/// jump list task starts the application with, passed on to the running one by the single-instance
/// plugin, or read by the first launch itself.
pub fn launch_action<I, S>(args: I) -> Option<MenuAction>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let arg = arg.as_ref();
        match arg {
            DEVICES_ARGUMENT => return Some(MenuAction::Devices),
            MAIN_WINDOW_ARGUMENT => return Some(MenuAction::MainWindow),
            OPEN_ARGUMENT => {
                return args
                    .next()
                    .map(|id| id.as_ref().to_owned())
                    .filter(|id| !id.is_empty())
                    .map(MenuAction::Open)
            }
            _ => {
                if let Some(id) = arg
                    .strip_prefix(OPEN_ARGUMENT)
                    .and_then(|rest| rest.strip_prefix('='))
                {
                    if !id.is_empty() {
                        return Some(MenuAction::Open(id.to_owned()));
                    }
                }
            }
        }
    }
    None
}

/// Does what `action` asks.
pub fn run_menu_action(app: &AppHandle, action: MenuAction) {
    match action {
        MenuAction::Devices => page::request_page(app),
        MenuAction::Open(entry) => open_entry(app, entry),
        MenuAction::MainWindow => show_main_window(app),
    }
}

/// Does what the menu item `id` asks when it is one of the remote daemons' items; whether it was.
pub fn handle_menu_item(app: &AppHandle, id: &str) -> bool {
    match menu_action(id) {
        Some(action) => {
            run_menu_action(app, action);
            true
        }
        None => false,
    }
}

/// The saved remote daemons as the menus list them: each entry's id and name, in the order saved.
pub fn menu_entries(app: &AppHandle) -> Vec<(String, String)> {
    super::remote_daemons_list(app.clone())
        .unwrap_or_default()
        .into_iter()
        .map(|entry| (entry.id, entry.name))
        .collect()
}

/// Shows and focuses the local daemon's window (the main one), which also holds the Remote devices
/// page.
pub(super) fn show_main_window(app: &AppHandle) {
    let showing = app.clone();
    let _ = app.run_on_main_thread(move || crate::background::show_main_window(&showing));
}

/// Opens, or brings forward, the window of the saved remote daemon `id`, as a menu's item for it
/// asks: what already shows it is brought forward (or the browser opened at it again), and
/// otherwise the shell connects to it quietly, which opens its window once connected. Nothing is
/// shown while it connects. An outcome that has to be shown, a question or a failure, is handed to
/// the Remote devices page, in the main window, which comes forward for it.
pub(super) fn open_entry(app: &AppHandle, id: String) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match connect::connect(&app, connect::quiet_request(&id), None).await {
            Ok(outcome) if page::needs_showing(&outcome) => page::hand_over(&app, id, outcome),
            Ok(_) => {}
            // The entry is no longer saved, or the list cannot be read.
            Err(err) => {
                diagnostic!("cannot open the remote daemon {id}: {err}");
                page::request_page(&app);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_remote_daemons_menu_items_are_told_apart_by_their_ids() {
        assert_eq!(menu_action(DEVICES_ITEM_ID), Some(MenuAction::Devices));
        assert_eq!(
            menu_action(MAIN_WINDOW_ITEM_ID),
            Some(MenuAction::MainWindow)
        );
        assert_eq!(
            menu_action(&format!("{OPEN_ITEM_PREFIX}abc")),
            Some(MenuAction::Open("abc".into()))
        );
        assert_eq!(menu_action(OPEN_ITEM_PREFIX), None);
        assert_eq!(menu_action("quit"), None);
        assert_eq!(menu_action("settings"), None);
    }

    #[test]
    fn a_launch_s_arguments_ask_for_the_remote_devices_page_or_the_main_window() {
        assert_eq!(
            launch_action(["octoboard.exe", DEVICES_ARGUMENT]),
            Some(MenuAction::Devices)
        );
        assert_eq!(
            launch_action(["octoboard.exe", MAIN_WINDOW_ARGUMENT]),
            Some(MenuAction::MainWindow)
        );
        assert_eq!(
            launch_action(["octoboard.exe", OPEN_ARGUMENT, "abc"]),
            Some(MenuAction::Open("abc".into()))
        );
        assert_eq!(
            launch_action(["octoboard.exe", "--remote-window-open=abc"]),
            Some(MenuAction::Open("abc".into()))
        );
        assert_eq!(launch_action(["octoboard.exe", OPEN_ARGUMENT]), None);
        assert_eq!(
            launch_action(["octoboard.exe", "--remote-window-open="]),
            None
        );
        assert_eq!(
            launch_action(["octoboard.exe", "--launched-at-login"]),
            None
        );
        assert_eq!(launch_action(Vec::<String>::new()), None);
    }

    #[test]
    fn a_menu_action_names_an_entry_whole_whatever_it_holds() {
        assert_eq!(
            menu_action("remote-window-open:a:b c"),
            Some(MenuAction::Open("a:b c".into()))
        );
        // What the first such argument of a launch asks for decides, whatever follows.
        assert_eq!(
            launch_action(["x", "--other", DEVICES_ARGUMENT, MAIN_WINDOW_ARGUMENT]),
            Some(MenuAction::Devices)
        );
        assert_eq!(
            launch_action(["x", MAIN_WINDOW_ARGUMENT, DEVICES_ARGUMENT]),
            Some(MenuAction::MainWindow)
        );
    }
}
