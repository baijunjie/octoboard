//! Holding back a rebuild of the menu while a menu is open. What tells it a menu began and ended
//! tracking is native (`macos/menu_tracking.rs`), or the flyout's own (`flyout.rs`).

use tauri::{AppHandle, Manager};

use super::TrayState;

/// Whether a menu is open, and whether the menu is to be built again once it closes. Any menu of
/// the app counts as open, not only the icon's: holding the icon's menu back while another menu is
/// open costs nothing anyone can see. What the menu is built from is kept up to date meanwhile, so
/// the rebuild takes the latest of everything.
#[derive(Default)]
pub(super) struct MenuTracking {
    open: bool,
    due: bool,
}

impl MenuTracking {
    /// Whether the menu may be built again now; otherwise the rebuild waits for the open menu to
    /// close. Replacing the menu closes it if it is open.
    pub(super) fn rebuild_now(&mut self) -> bool {
        if self.open {
            self.due = true;
            false
        } else {
            self.due = false;
            true
        }
    }

    pub(super) fn opened(&mut self) {
        self.open = true;
    }

    /// Whether a rebuild was held back while the menu was open, and is now waiting for `due`.
    pub(super) fn closed(&mut self) -> bool {
        self.open = false;
        self.due
    }

    /// Whether the held rebuild is to happen now: not when a menu has opened again since, which it
    /// then waits for to close.
    pub(super) fn due(&mut self) -> bool {
        if self.open {
            return false;
        }
        std::mem::take(&mut self.due)
    }
}

pub(super) fn tracking(app: &AppHandle) -> std::sync::MutexGuard<'_, MenuTracking> {
    app.state::<TrayState>()
        .inner()
        .tracking
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rebuild_asked_for_while_no_menu_is_open_happens_at_once() {
        let mut tracking = MenuTracking::default();
        assert!(tracking.rebuild_now());
        assert!(!tracking.closed());
        assert!(!tracking.due());
    }

    #[test]
    fn rebuilds_asked_for_while_a_menu_is_open_happen_once_when_it_closes() {
        let mut tracking = MenuTracking::default();
        tracking.opened();
        assert!(!tracking.rebuild_now());
        assert!(!tracking.rebuild_now());
        assert!(tracking.closed());
        assert!(tracking.due());
        assert!(!tracking.due());
    }

    #[test]
    fn a_held_rebuild_yields_to_one_made_meanwhile_and_to_a_menu_opened_again() {
        let mut tracking = MenuTracking::default();
        tracking.opened();
        tracking.rebuild_now();
        assert!(tracking.closed());
        // A rebuild made before the held one ran takes its place.
        assert!(tracking.rebuild_now());
        assert!(!tracking.due());

        tracking.opened();
        tracking.rebuild_now();
        tracking.closed();
        tracking.opened();
        assert!(!tracking.due());
        assert!(tracking.closed());
        assert!(tracking.due());
    }
}
