//! Bringing back, at launch, the remote windows that were open when the application quit.
//!
//! At every quit the entries whose windows are open (not those held for the browser) are written
//! to `remote-windows.json`, with whether the main window was hidden then. At the next launch they
//! are connected again, all at once, and each window opens where it was left (`window_state`);
//! nothing is asked: an entry that needs a password the application cannot supply, a host that
//! cannot be reached or a daemon of another generation opens no dialog and no window, and stays
//! unconnected (the rail lists the connected daemons alone) until the owner connects it. A launch
//! at login that keeps the application in the background brings none back.
//!
//! A remote-only user, who closed the main window and kept a remote one on screen, sees only their
//! remote windows: while such a launch connects them the main window is held hidden, the UI's
//! reveal and the reveal safety net waiting, and it stays hidden once one of them is back
//! (`background::keep_main_hidden`), until something shows it. When none has come back within
//! `HOLD_LIMIT` it is shown as usual; a remote window that connects later still opens.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;

use super::connect::restoring_request;
use super::ConnectOutcome;
use crate::config_dir::{config_path, write_json};
use crate::diagnostics::diagnostic;

/// The file the windows to bring back are kept in, under the application's config directory.
const FILE_NAME: &str = "remote-windows.json";

/// How long the main window is held hidden at most for a remote window to come back.
const HOLD_LIMIT: Duration = Duration::from_secs(15);

/// What a quit leaves for the next launch.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Remembered {
    /// The entries whose windows were open, in the order they were opened in.
    pub entries: Vec<String>,
    /// Whether the main window was hidden: closed while a remote window stayed on screen.
    pub main_hidden: bool,
}

/// What the quit leaves, from the entries with a window open (`open`) and whether the main window
/// was on screen.
pub fn remembered_at_quit(open: &[String], main_visible: bool) -> Remembered {
    let mut entries: Vec<String> = Vec::new();
    for id in open {
        if !entries.contains(id) {
            entries.push(id.clone());
        }
    }
    Remembered {
        main_hidden: !entries.is_empty() && !main_visible,
        entries,
    }
}

/// Whether the launch brings remote windows back: not a launch, at login or by the command line, that stays in the background.
pub fn restores(remembered: &Remembered, stays_in_background: bool) -> bool {
    !stays_in_background && !remembered.entries.is_empty()
}

/// Whether the main window is held hidden while the remote windows are connected.
pub fn holds_main(remembered: &Remembered, stays_in_background: bool) -> bool {
    restores(remembered, stays_in_background) && remembered.main_hidden
}

/// What becomes of a held main window once the remote windows have been connected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainAfterRestore {
    /// It was not held: the UI's own reveal decided.
    Untouched,
    /// One remote window or more is back: it stays hidden, as it was left.
    StaysHidden,
    /// None is back: it is shown as at any launch.
    Shown,
}

/// The decision once `opened` remote windows came back, for a main window `held` or not.
pub fn main_after_restore(held: bool, opened: usize) -> MainAfterRestore {
    match (held, opened) {
        (false, _) => MainAfterRestore::Untouched,
        (true, 0) => MainAfterRestore::Shown,
        (true, _) => MainAfterRestore::StaysHidden,
    }
}

/// Reads what the last quit left; nothing when the file is missing or unreadable.
pub fn load(path: &Path) -> Remembered {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Whether the main window is held hidden now, and whether this process has remembered its remote
/// windows.
#[derive(Default)]
pub struct RestoreState {
    holding_main: AtomicBool,
    remembered: AtomicBool,
}

impl RestoreState {
    /// Whether this is the first remembering of the process, which alone is written: every way out
    /// remembers before the windows end. The installer started from the exit would find none open
    /// by then, and `RunEvent::Exit` after an installer that could not start adds nothing.
    fn first_remembering(&self) -> bool {
        !self.remembered.swap(true, Ordering::SeqCst)
    }
}

/// How many of the remote windows being brought back came back before `limit` passed, counting
/// only up to the first: `came_back` hears whether each one did, and is over once all are. The main
/// window's hold needs no more.
async fn came_back_within(mut came_back: mpsc::UnboundedReceiver<bool>, limit: Duration) -> usize {
    let first = async {
        while let Some(opened) = came_back.recv().await {
            if opened {
                return 1;
            }
        }
        0
    };
    tokio::time::timeout(limit, first).await.unwrap_or(0)
}

/// Whether the main window is held hidden while remote windows are brought back: its page's reveal
/// and the reveal safety net leave it alone then (`background::main_reveal`), and the end of the
/// restore decides (`finish`).
pub fn holding_main(app: &AppHandle) -> bool {
    app.state::<RestoreState>()
        .holding_main
        .load(Ordering::SeqCst)
}

/// Writes what this quit leaves for the next launch, once per process.
pub fn remember(app: &AppHandle) {
    if !app.state::<RestoreState>().first_remembering() {
        return;
    }
    let main_visible = app
        .get_webview_window(crate::MAIN_WINDOW_LABEL)
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(true);
    let remembered = remembered_at_quit(&super::window_entries(app), main_visible);
    let written = config_path(app, FILE_NAME)
        .map_err(anyhow::Error::from)
        .and_then(|path| write_json(&path, &remembered));
    if let Err(err) = written {
        diagnostic!("cannot remember the remote windows: {err}");
    }
}

/// Starts bringing back the remote windows the last quit left, holding the main window hidden
/// first where that applies; `stays_in_background` is a launch, at login or by the command line, that stays in the background.
/// Before the main window is opened.
pub fn begin(app: &AppHandle, stays_in_background: bool) {
    let Ok(path) = config_path(app, FILE_NAME) else {
        return;
    };
    let remembered = load(&path);
    if !restores(&remembered, stays_in_background) {
        return;
    }
    let held = holds_main(&remembered, stays_in_background);
    app.state::<RestoreState>()
        .holding_main
        .store(held, Ordering::SeqCst);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // All at once, so hosts that cannot be reached cost the time of one; each takes the rank
        // its window had, so they are listed, and remembered at the next quit, in the order they
        // were opened in whichever connects first.
        let (told, came_back) = mpsc::unbounded_channel();
        for id in remembered.entries {
            let app = app.clone();
            let order = super::next_order();
            let told = told.clone();
            tauri::async_runtime::spawn(async move {
                let outcome =
                    super::connect::connect(&app, restoring_request(&id), Some(order)).await;
                let opened = matches!(outcome, Ok(ConnectOutcome::Opened));
                match outcome {
                    Ok(ConnectOutcome::Opened) => {}
                    Ok(other) => diagnostic!("not bringing back the window of {id}: {other:?}"),
                    Err(err) => diagnostic!("not bringing back the window of {id}: {err}"),
                }
                let _ = told.send(opened);
            });
        }
        drop(told);
        // The hold ends at the first window back, or once none has come back in time; the
        // connections still under way go on, and their windows open whenever they connect.
        let opened = came_back_within(came_back, HOLD_LIMIT).await;
        finish(&app, main_after_restore(held, opened));
    });
}

/// Lets go of the held main window: showing it where no remote window came back, and otherwise
/// leaving it hidden in the state its page's reveal and the reveal safety net respect.
fn finish(app: &AppHandle, decision: MainAfterRestore) {
    match decision {
        MainAfterRestore::Shown => {
            let showing = app.clone();
            let _ = app.run_on_main_thread(move || crate::background::show_main_window(&showing));
        }
        // Before the hold ends, so no reveal falls between the two.
        MainAfterRestore::StaysHidden => crate::background::keep_main_hidden(app),
        MainAfterRestore::Untouched => {}
    }
    app.state::<RestoreState>()
        .holding_main
        .store(false, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_dir::test_support::Scratch;

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|id| (*id).to_owned()).collect()
    }

    #[test]
    fn a_quit_remembers_each_open_window_once_and_whether_the_main_window_was_hidden() {
        assert_eq!(
            remembered_at_quit(&ids(&["a", "b", "a"]), false),
            Remembered {
                entries: ids(&["a", "b"]),
                main_hidden: true
            }
        );
        assert_eq!(
            remembered_at_quit(&ids(&["a"]), true),
            Remembered {
                entries: ids(&["a"]),
                main_hidden: false
            }
        );
        // A hidden main window with no remote window is the application in the background, which
        // is not what is remembered.
        assert_eq!(remembered_at_quit(&[], false), Remembered::default());
    }

    #[test]
    fn a_launch_at_login_into_the_background_brings_nothing_back() {
        let remembered = remembered_at_quit(&ids(&["a"]), false);
        assert!(restores(&remembered, false));
        assert!(holds_main(&remembered, false));
        assert!(!restores(&remembered, true));
        assert!(!holds_main(&remembered, true));
        assert!(!restores(&Remembered::default(), false));
    }

    #[test]
    fn the_main_window_is_held_only_when_it_was_hidden_at_the_quit() {
        assert!(!holds_main(&remembered_at_quit(&ids(&["a"]), true), false));
        assert!(holds_main(&remembered_at_quit(&ids(&["a"]), false), false));
    }

    #[test]
    fn a_held_main_window_stays_hidden_only_when_a_remote_window_came_back() {
        assert_eq!(main_after_restore(true, 1), MainAfterRestore::StaysHidden);
        assert_eq!(main_after_restore(true, 0), MainAfterRestore::Shown);
        assert_eq!(main_after_restore(false, 0), MainAfterRestore::Untouched);
        assert_eq!(main_after_restore(false, 2), MainAfterRestore::Untouched);
    }

    #[test]
    fn what_a_quit_leaves_survives_to_the_next_launch_and_a_broken_file_is_nothing() {
        let scratch = Scratch::new("remote", "restore");
        let path = scratch.0.join(FILE_NAME);
        assert_eq!(load(&path), Remembered::default());
        let remembered = remembered_at_quit(&ids(&["a", "b"]), false);
        write_json(&path, &remembered).unwrap();
        assert_eq!(load(&path), remembered);
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains("\"mainHidden\": true"), "{written}");
        std::fs::write(&path, b"[").unwrap();
        assert_eq!(load(&path), Remembered::default());
    }

    #[test]
    fn a_launch_brings_back_what_the_quit_left_and_decides_the_main_window_by_how_many_came() {
        // The main window was closed while a remote window stayed: it stays hidden once one is
        // back, and is shown when none is.
        let left = remembered_at_quit(&ids(&["a", "b"]), false);
        assert!(restores(&left, false));
        let held = holds_main(&left, false);
        assert_eq!(main_after_restore(held, 2), MainAfterRestore::StaysHidden);
        assert_eq!(main_after_restore(held, 1), MainAfterRestore::StaysHidden);
        assert_eq!(main_after_restore(held, 0), MainAfterRestore::Shown);
        // The main window was on screen: remote windows come back, the main window is not held.
        let visible = remembered_at_quit(&ids(&["a"]), true);
        assert!(restores(&visible, false));
        assert_eq!(
            main_after_restore(holds_main(&visible, false), 1),
            MainAfterRestore::Untouched
        );
        // Nothing was open: nothing is brought back and nothing is held.
        let none = remembered_at_quit(&[], true);
        assert!(!restores(&none, false) && !holds_main(&none, false));
    }

    #[test]
    fn only_the_first_remembering_of_a_process_is_written() {
        // The exit remembers the open windows, ends them, and may then start an installer that
        // remembers again: that second one would write an empty list over the first.
        let state = RestoreState::default();
        assert!(state.first_remembering());
        assert!(!state.first_remembering());
        assert!(!state.first_remembering());
    }

    #[tokio::test]
    async fn the_hold_ends_at_the_first_window_back() {
        let (told, came_back) = mpsc::unbounded_channel();
        told.send(false).unwrap();
        told.send(true).unwrap();
        // Another connection still under way does not keep the hold.
        let started = std::time::Instant::now();
        assert_eq!(came_back_within(came_back, Duration::from_secs(5)).await, 1);
        assert!(started.elapsed() < Duration::from_secs(5));
        drop(told);
    }

    #[tokio::test]
    async fn the_hold_ends_once_every_connection_failed() {
        let (told, came_back) = mpsc::unbounded_channel();
        told.send(false).unwrap();
        told.send(false).unwrap();
        drop(told);
        let started = std::time::Instant::now();
        assert_eq!(came_back_within(came_back, Duration::from_secs(5)).await, 0);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn a_hold_with_no_window_back_in_time_lets_the_main_window_show() {
        let (told, came_back) = mpsc::unbounded_channel();
        // A connection still under way when the limit passes: no window came back in time, so the
        // main window is shown...
        assert_eq!(
            came_back_within(came_back, Duration::from_millis(50)).await,
            0
        );
        // ...and the window that connects later is no concern of the hold's any more.
        assert!(told.send(true).is_err());
    }

    #[test]
    fn the_windows_come_back_in_the_order_they_were_opened_in() {
        let left = remembered_at_quit(&ids(&["c", "a", "b"]), true);
        assert_eq!(left.entries, ids(&["c", "a", "b"]));
    }

    #[test]
    fn a_file_left_by_something_else_is_nothing_to_bring_back() {
        let scratch = Scratch::new("remote", "restore-foreign");
        std::fs::create_dir_all(&scratch.0).unwrap();
        let path = scratch.0.join(FILE_NAME);
        for content in ["", "null", "{}", "{\"entries\": 3}", "{\"entries\": [1]}"] {
            std::fs::write(&path, content).unwrap();
            assert_eq!(load(&path), Remembered::default(), "{content:?}");
        }
    }
}
