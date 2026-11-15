//! The Remote items of every window's system menu on Windows, the menu the title bar's icon and
//! Alt+Space open: after a separator, New remote window, the saved remote daemons (or a disabled
//! line saying there is none) and Open this PC's window, as the macOS Remote menu lists them. Their
//! English defaults are in sentence case, as Windows' own menus are; the UI sends them so in its
//! catalogs' languages (`useNativeMenuLabels`). Each window is subclassed (`install`) to hear the
//! items' `WM_SYSCOMMAND`; `rebuild` puts the current items on every such window's menu, in the
//! language the UI last sent, whenever the saved daemons or the labels change. The items are built
//! on every platform, for their tests; only Windows shows them.

use std::collections::HashMap;

use crate::remote::MenuAction;

/// The command of the first item. The system keeps commands from `0xF000` up for its own, and uses
/// a command's four low bits itself, so every item's command is a multiple of 16 below that.
const FIRST_COMMAND: u32 = 0x1000;
/// The step between two items' commands.
const COMMAND_STEP: u32 = 0x10;
/// Where the commands must stay below: the first of the system's own.
const SYSTEM_COMMANDS: u32 = 0xF000;

/// One line the system menu gets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    Command {
        command: u32,
        text: String,
        /// What choosing it asks for; `None` for the disabled line that says there is no daemon.
        action: Option<MenuAction>,
    },
    Separator,
}

/// The label under `key`, or its English default.
fn label<'a>(labels: &'a HashMap<String, String>, key: &str, default: &'a str) -> &'a str {
    labels.get(key).map_or(default, String::as_str)
}

/// The lines appended to the system menu, in order, from the UI's labels and the saved entries
/// (`remote::menu_entries`): a separator, New remote window, a separator, the saved daemons, a
/// separator and Open this PC's window. Daemons past what the commands below the system's own can
/// number are left out.
pub fn items(labels: &HashMap<String, String>, entries: &[(String, String)]) -> Vec<Item> {
    // The two fixed lines take the first two commands, so the daemons' commands follow on from
    // them whatever their number.
    let fixed = |index: u32, text: &str, action| Item::Command {
        command: FIRST_COMMAND + index * COMMAND_STEP,
        text: text.to_owned(),
        action: Some(action),
    };
    let new_window = fixed(
        0,
        label(labels, "menu.newRemoteWindow", "New remote window"),
        MenuAction::NewWindow,
    );
    let main_window = fixed(
        1,
        label(labels, "menu.openMainWindow", "Open this PC’s window"),
        MenuAction::MainWindow,
    );
    let mut next = FIRST_COMMAND + 2 * COMMAND_STEP;
    let mut command = |text: &str, action: Option<MenuAction>| {
        let item = Item::Command {
            command: next,
            text: text.to_owned(),
            action,
        };
        next += COMMAND_STEP;
        item
    };
    let mut items = vec![Item::Separator, new_window, Item::Separator];
    if entries.is_empty() {
        items.push(command(
            label(labels, "menu.noRemoteDaemons", "No remote daemons"),
            None,
        ));
    }
    let room = ((SYSTEM_COMMANDS - FIRST_COMMAND) / COMMAND_STEP) as usize - 3;
    for (id, name) in entries.iter().take(room) {
        items.push(command(name, Some(MenuAction::Open(id.clone()))));
    }
    items.push(Item::Separator);
    items.push(main_window);
    items
}

/// What the system command `wparam` of a `WM_SYSCOMMAND` asks for, when it is one of `items`.
pub fn action_for(items: &[Item], wparam: usize) -> Option<MenuAction> {
    // The system's own use of the four low bits is masked off, as `WM_SYSCOMMAND` asks.
    let command = (wparam & 0xFFF0) as u32;
    items.iter().find_map(|item| match item {
        Item::Command {
            command: own,
            action,
            ..
        } if *own == command => action.clone(),
        _ => None,
    })
}

#[cfg(windows)]
pub use platform::{install, rebuild};

#[cfg(windows)]
mod platform {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use tauri::{AppHandle, Manager};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, GetSystemMenu, MF_GRAYED, MF_SEPARATOR, MF_STRING, WM_NCDESTROY, WM_SYSCOMMAND,
    };

    use super::Item;
    use crate::diagnostics::diagnostic;

    /// Names this module's subclass of a window.
    const SUBCLASS_ID: usize = 0x4f42_534d;

    /// The items every window's system menu has now.
    static ITEMS: Mutex<Vec<Item>> = Mutex::new(Vec::new());
    /// The windows subclassed, as numbers, which `rebuild` puts the items on.
    static WINDOWS: Mutex<Vec<usize>> = Mutex::new(Vec::new());

    /// Subclasses `window` to hear its system menu's Remote items, and gives its menu the current
    /// ones. On the main thread, where the window was built.
    pub fn install(window: &tauri::WebviewWindow) {
        let hwnd = match window.hwnd() {
            Ok(hwnd) => hwnd.0 as usize,
            Err(err) => {
                diagnostic!("cannot reach the window to add its system menu's items: {err}");
                return;
            }
        };
        // Lives until the window's `WM_NCDESTROY`, which frees it.
        let app = Box::into_raw(Box::new(window.app_handle().clone())) as usize;
        // SAFETY: on the main thread, which created the window; the procedure stays valid for the
        // process's life and its data until `WM_NCDESTROY` removes the subclass.
        let installed =
            unsafe { SetWindowSubclass(hwnd as HWND, Some(subclass), SUBCLASS_ID, app) };
        if installed == 0 {
            // SAFETY: the box was leaked just above and handed to no one.
            drop(unsafe { Box::from_raw(app as *mut AppHandle) });
            diagnostic!("cannot subclass the window to add its system menu's items");
            return;
        }
        WINDOWS.lock().unwrap_or_else(|e| e.into_inner()).push(hwnd);
        let items = ITEMS.lock().unwrap_or_else(|e| e.into_inner()).clone();
        apply(hwnd, &items);
    }

    /// Makes the items those of `labels` and `entries`, and puts them on every window's system
    /// menu. On any thread.
    pub fn rebuild(
        app: &AppHandle,
        labels: &HashMap<String, String>,
        entries: &[(String, String)],
    ) {
        let items = super::items(labels, entries);
        let _ = app.run_on_main_thread(move || {
            *ITEMS.lock().unwrap_or_else(|e| e.into_inner()) = items.clone();
            let windows = WINDOWS.lock().unwrap_or_else(|e| e.into_inner()).clone();
            for hwnd in windows {
                apply(hwnd, &items);
            }
        });
    }

    /// Gives the window `hwnd` the system's own system menu again, with `items` appended. On the
    /// main thread.
    fn apply(hwnd: usize, items: &[Item]) {
        // SAFETY: plain menu calls on a window this module subclassed and has not seen destroyed;
        // every string passed is NUL-terminated and outlives its call.
        unsafe {
            // Reverting destroys the window's own copy, so the next call makes a fresh one.
            GetSystemMenu(hwnd as HWND, 1);
            let menu = GetSystemMenu(hwnd as HWND, 0);
            if menu.is_null() {
                return;
            }
            for item in items {
                match item {
                    Item::Separator => {
                        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
                    }
                    Item::Command {
                        command,
                        text,
                        action,
                    } => {
                        let wide: Vec<u16> = text.encode_utf16().chain([0]).collect();
                        let flags = if action.is_some() {
                            MF_STRING
                        } else {
                            MF_STRING | MF_GRAYED
                        };
                        AppendMenuW(menu, flags, *command as usize, wide.as_ptr());
                    }
                }
            }
        }
    }

    unsafe extern "system" fn subclass(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        data: usize,
    ) -> LRESULT {
        match message {
            WM_SYSCOMMAND => {
                let items = ITEMS.lock().unwrap_or_else(|e| e.into_inner()).clone();
                if let Some(action) = super::action_for(&items, wparam) {
                    // SAFETY: `data` is the handle `install` leaked for this subclass, freed only
                    // at `WM_NCDESTROY`.
                    let app = unsafe { &*(data as *const AppHandle) }.clone();
                    // From the event loop rather than inside this window's procedure, as opening
                    // or showing a window from here would re-enter it. `run_on_main_thread` runs a
                    // task at once when called on the main thread, as this procedure is, so it is
                    // called from the async runtime.
                    tauri::async_runtime::spawn(async move {
                        let target = app.clone();
                        let _ = app.run_on_main_thread(move || {
                            crate::remote::run_menu_action(&target, action)
                        });
                    });
                    return 0;
                }
            }
            WM_NCDESTROY => {
                WINDOWS
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .retain(|window| *window != hwnd as usize);
                // SAFETY: the window is going; the subclass is removed before its data is freed,
                // and `data` is the box `install` leaked for it.
                unsafe {
                    RemoveWindowSubclass(hwnd, Some(subclass), SUBCLASS_ID);
                    drop(Box::from_raw(data as *mut AppHandle));
                }
            }
            _ => {}
        }
        // SAFETY: the arguments are the ones this procedure was called with.
        unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commands(items: &[Item]) -> Vec<(u32, &str, Option<MenuAction>)> {
        items
            .iter()
            .filter_map(|item| match item {
                Item::Command {
                    command,
                    text,
                    action,
                } => Some((*command, text.as_str(), action.clone())),
                Item::Separator => None,
            })
            .collect()
    }

    #[test]
    fn the_system_menu_gets_new_window_the_saved_daemons_and_the_main_window_after_a_separator() {
        let entries = vec![
            ("a".to_owned(), "Staging".to_owned()),
            ("b".to_owned(), "Lab".to_owned()),
        ];
        let items = items(&HashMap::new(), &entries);
        assert_eq!(items[0], Item::Separator);
        let lines: Vec<(&str, Option<MenuAction>)> = commands(&items)
            .into_iter()
            .map(|(_, text, action)| (text, action))
            .collect();
        assert_eq!(
            lines,
            [
                ("New remote window", Some(MenuAction::NewWindow)),
                ("Staging", Some(MenuAction::Open("a".into()))),
                ("Lab", Some(MenuAction::Open("b".into()))),
                ("Open this PC’s window", Some(MenuAction::MainWindow)),
            ]
        );
        assert_eq!(items[2], Item::Separator);
        assert_eq!(items[items.len() - 2], Item::Separator);
    }

    #[test]
    fn with_no_saved_daemon_a_disabled_line_says_so() {
        let items = items(&HashMap::new(), &[]);
        let lines = commands(&items);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[1].1, "No remote daemons");
        assert_eq!(lines[1].2, None);
        assert_eq!(action_for(&items, lines[1].0 as usize), None);
    }

    #[test]
    fn the_items_read_the_ui_s_labels() {
        let labels = HashMap::from([
            ("menu.newRemoteWindow".to_owned(), "新建远程窗口".to_owned()),
            ("menu.openMainWindow".to_owned(), "打开本机窗口".to_owned()),
            (
                "menu.noRemoteDaemons".to_owned(),
                "没有远程守护进程".to_owned(),
            ),
        ]);
        let items = items(&labels, &[]);
        let texts: Vec<&str> = commands(&items)
            .into_iter()
            .map(|(_, text, _)| text)
            .collect();
        assert_eq!(texts, ["新建远程窗口", "没有远程守护进程", "打开本机窗口"]);
    }

    #[test]
    fn every_command_is_its_own_multiple_of_16_below_the_system_s() {
        let entries: Vec<(String, String)> = (0..5000)
            .map(|i| (format!("e{i}"), format!("D{i}")))
            .collect();
        let items = items(&HashMap::new(), &entries);
        let lines = commands(&items);
        let mut seen = std::collections::HashSet::new();
        for (command, _, _) in &lines {
            assert_eq!(command % 16, 0);
            assert!(*command < SYSTEM_COMMANDS && *command >= FIRST_COMMAND);
            assert!(seen.insert(*command));
        }
        // The main window's line stays, however many daemons there are.
        assert_eq!(lines.last().unwrap().2, Some(MenuAction::MainWindow));
    }

    #[test]
    fn a_system_command_routes_to_its_item_s_action_whatever_its_low_bits() {
        let entries = vec![("abc".to_owned(), "Lab".to_owned())];
        let items = items(&HashMap::new(), &entries);
        for (command, _, action) in commands(&items) {
            assert_eq!(action_for(&items, command as usize), action);
            assert_eq!(action_for(&items, command as usize | 0x2), action);
        }
        // The system's own commands are not the items'.
        const SC_CLOSE: usize = 0xF060;
        const SC_KEYMENU: usize = 0xF100;
        assert_eq!(action_for(&items, SC_CLOSE), None);
        assert_eq!(action_for(&items, SC_KEYMENU), None);
    }
}
