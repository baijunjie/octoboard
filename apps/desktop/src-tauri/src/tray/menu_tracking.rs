//! Holding back a menu the UI sends while a menu is open. What tells it a menu began and ended
//! tracking is native (`macos/menu_tracking.rs`).

use tauri::{AppHandle, Manager};

use super::{model::TrayMenu, TrayState};

/// Whether a menu is open, and the latest menu the UI sent while one was. Any menu of the app counts
/// as open, not only this one: telling them apart would take the tray's own `NSMenu` out of the
/// notifications, and holding the tray's menu back while the app menu is open costs nothing anyone
/// can see.
#[derive(Default)]
pub(super) struct MenuTracking {
    open: bool,
    held: Option<TrayMenu>,
}

impl MenuTracking {
    /// The menu to apply now, or `None` when it is held until the open menu closes. Only the
    /// latest is worth showing, so a menu replaces one already held, and one applied now drops it.
    pub(super) fn offer(&mut self, menu: TrayMenu) -> Option<TrayMenu> {
        if self.open {
            self.held = Some(menu);
            None
        } else {
            self.held = None;
            Some(menu)
        }
    }

    pub(super) fn opened(&mut self) {
        self.open = true;
    }

    /// Whether a menu was held back while the menu was open, and is now waiting for `due`.
    pub(super) fn closed(&mut self) -> bool {
        self.open = false;
        self.held.is_some()
    }

    /// The menu last set, `menu`, to build again now in a theme the system changed to, or `None`
    /// while a menu is open: a menu held then is newer and is built when the menu closes, and
    /// otherwise `menu` waits there in its place.
    #[cfg_attr(
        target_os = "macos",
        allow(dead_code, reason = "only Windows builds the menu again for a theme")
    )]
    pub(super) fn rebuild(&mut self, menu: TrayMenu) -> Option<TrayMenu> {
        if self.open {
            self.held.get_or_insert(menu);
            None
        } else {
            Some(menu)
        }
    }

    /// The held menu, unless a menu has opened again since, in which case it waits for that one
    /// to close.
    pub(super) fn due(&mut self) -> Option<TrayMenu> {
        if self.open {
            None
        } else {
            self.held.take()
        }
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

    fn menu(open: &str) -> TrayMenu {
        TrayMenu {
            open: open.to_string(),
            ..TrayMenu::default()
        }
    }

    #[test]
    fn a_menu_sent_while_none_is_open_applies_at_once() {
        let mut tracking = MenuTracking::default();
        assert_eq!(tracking.offer(menu("a")).map(|m| m.open), Some("a".into()));
        assert!(!tracking.closed());
        assert!(tracking.due().is_none());
    }

    #[test]
    fn a_menu_built_again_while_one_is_open_never_replaces_a_newer_one() {
        let mut tracking = MenuTracking::default();
        assert_eq!(
            tracking.rebuild(menu("a")).map(|m| m.open),
            Some("a".into())
        );
        tracking.opened();
        assert!(tracking.offer(menu("newer")).is_none());
        assert!(tracking.rebuild(menu("a")).is_none());
        assert!(tracking.closed());
        assert_eq!(tracking.due().map(|m| m.open), Some("newer".into()));
        tracking.opened();
        assert!(tracking.rebuild(menu("a")).is_none());
        assert!(tracking.closed());
        assert_eq!(tracking.due().map(|m| m.open), Some("a".into()));
    }

    #[test]
    fn menus_sent_while_one_is_open_wait_and_only_the_latest_applies() {
        let mut tracking = MenuTracking::default();
        tracking.opened();
        assert!(tracking.offer(menu("a")).is_none());
        assert!(tracking.offer(menu("b")).is_none());
        assert!(tracking.closed());
        assert_eq!(tracking.due().map(|m| m.open), Some("b".into()));
        assert!(tracking.due().is_none());
    }

    #[test]
    fn a_held_menu_yields_to_a_newer_one_and_to_a_menu_opened_again() {
        let mut tracking = MenuTracking::default();
        tracking.opened();
        tracking.offer(menu("a"));
        assert!(tracking.closed());
        assert!(tracking.offer(menu("b")).is_some());
        assert!(tracking.due().is_none());

        tracking.opened();
        tracking.offer(menu("c"));
        tracking.closed();
        tracking.opened();
        assert!(tracking.due().is_none());
        tracking.closed();
        assert_eq!(tracking.due().map(|m| m.open), Some("c".into()));
    }
}
