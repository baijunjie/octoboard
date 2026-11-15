//! The Dock badge, which is the application's: one number on its Dock tile, which every daemon's
//! window has a share of. Each window reports the number of its daemon's sessions waiting for the
//! user (`set_badge_count`), and the badge shows their sum; a window's share goes with it when it
//! closes (`forget`).

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::{AppHandle, Manager, WebviewWindow};

use crate::MAIN_WINDOW_LABEL;

/// Each window's count, by its label.
#[derive(Default)]
pub struct BadgeState(Mutex<Counts>);

#[derive(Default)]
struct Counts(HashMap<String, u32>);

impl Counts {
    /// Records `count` as the window `label`'s share and answers the sum; a window at zero holds
    /// no share.
    fn set(&mut self, label: &str, count: u32) -> u32 {
        if count == 0 {
            self.0.remove(label);
        } else {
            self.0.insert(label.to_owned(), count);
        }
        self.sum()
    }

    /// Drops the share of the window `label`, which has closed, and answers the sum, or `None` when
    /// it held none and the badge stays as it is.
    fn forget(&mut self, label: &str) -> Option<u32> {
        self.0.remove(label).map(|_| self.sum())
    }

    fn sum(&self) -> u32 {
        self.0
            .values()
            .fold(0, |sum, count| sum.saturating_add(*count))
    }
}

/// Sets the asking window's share of the Dock badge to `count`, the sessions of its daemon waiting
/// for the user, and shows the sum of every window's share.
#[tauri::command]
pub fn set_badge_count(
    app: AppHandle,
    webview_window: WebviewWindow,
    count: u32,
) -> Result<(), String> {
    let sum = lock(&app).set(webview_window.label(), count);
    show(&app, sum)
}

/// Drops the share of the window labelled `label`, which has closed.
pub fn forget(app: &AppHandle, label: &str) {
    let sum = lock(app).forget(label);
    if let Some(sum) = sum {
        let _ = show(app, sum);
    }
}

fn lock(app: &AppHandle) -> std::sync::MutexGuard<'_, Counts> {
    app.state::<BadgeState>()
        .inner()
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Puts `sum` on the Dock tile, or no badge at zero. The badge is the application's whichever
/// window sets it; the main window, which lives as long as the application, is the one asked.
fn show(app: &AppHandle, sum: u32) -> Result<(), String> {
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return Ok(());
    };
    window
        .set_badge_count((sum > 0).then_some(i64::from(sum)))
        .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_badge_is_the_sum_of_every_window_s_count() {
        let mut counts = Counts::default();
        assert_eq!(counts.set("main", 2), 2);
        assert_eq!(counts.set("daemon-1", 3), 5);
        assert_eq!(counts.set("main", 1), 4);
        assert_eq!(counts.set("daemon-1", 0), 1);
        assert_eq!(counts.set("main", 0), 0);
    }

    #[test]
    fn a_closed_window_takes_its_share_with_it() {
        let mut counts = Counts::default();
        counts.set("main", 1);
        counts.set("daemon-1", 4);
        assert_eq!(counts.forget("daemon-1"), Some(1));
        // A window that held no share leaves the badge as it is.
        assert_eq!(counts.forget("daemon-2"), None);
        assert_eq!(counts.forget("daemon-1"), None);
    }

    #[test]
    fn the_last_share_leaving_takes_the_badge_off() {
        let mut counts = Counts::default();
        counts.set("main", 2);
        counts.set("daemon-1", 3);
        assert_eq!(counts.forget("main"), Some(3));
        assert_eq!(counts.forget("daemon-1"), Some(0));
    }

    #[test]
    fn a_window_reporting_again_replaces_its_share_and_does_not_add_to_it() {
        let mut counts = Counts::default();
        assert_eq!(counts.set("daemon-1", 3), 3);
        assert_eq!(counts.set("daemon-1", 3), 3);
        assert_eq!(counts.set("daemon-1", 1), 1);
    }

    #[test]
    fn a_window_that_never_reported_has_no_share_to_lose_and_zero_reports_nothing_held() {
        let mut counts = Counts::default();
        assert_eq!(counts.set("daemon-1", 0), 0);
        assert_eq!(counts.forget("daemon-1"), None);
    }

    #[test]
    fn the_sum_does_not_wrap_around() {
        let mut counts = Counts::default();
        counts.set("main", u32::MAX);
        assert_eq!(counts.set("daemon-1", 5), u32::MAX);
    }
}
