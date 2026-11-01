//! Holding back a menu the UI sends while a menu is open, and following AppKit's notifications that
//! a menu began and ended tracking.

use tauri::{AppHandle, Manager};

#[cfg(target_os = "macos")]
use super::{apply, finish::finish_opening_menu, MENU_OPENED_EVENT, TRAY_ID};
use super::{model::TrayMenu, TrayState};
#[cfg(target_os = "macos")]
use tauri::Emitter;

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

/// Keeps `MenuTracking` up to date from AppKit's notifications that a menu began and ended
/// tracking, both posted on the main thread.
#[cfg(target_os = "macos")]
pub(super) fn follow_menu_tracking(app: &AppHandle) {
    use objc2_app_kit::{NSMenuDidBeginTrackingNotification, NSMenuDidEndTrackingNotification};
    use objc2_foundation::{NSNotification, NSNotificationCenter, NSOperationQueue};

    let began = {
        let app = app.clone();
        block2::RcBlock::new(move |notification: std::ptr::NonNull<NSNotification>| {
            tracking(&app).opened();
            // SAFETY: the notification is alive for the duration of its delivery.
            let notification = unsafe { notification.as_ref() };
            if is_tray_menu(&app, notification) {
                finish_opening_menu(&app, notification);
                let _ = app.emit(MENU_OPENED_EVENT, ());
            }
        })
    };
    let ended = {
        let app = app.clone();
        block2::RcBlock::new(move |_: std::ptr::NonNull<NSNotification>| {
            if !tracking(&app).closed() {
                return;
            }
            // Queued on the main queue rather than applied here: the item just chosen, if any, may
            // have its action sent after the menu stops tracking, and replacing the menu drops the
            // items that action belongs to. `run_on_main_thread` would not do, since on the main
            // thread it runs the task at once. The held menu is read when the block runs, so a
            // newer one sent meanwhile, or a menu opened again, is respected.
            let app = app.clone();
            let apply_held = block2::RcBlock::new(move || {
                let due = tracking(&app).due();
                if let Some(menu) = due {
                    let _ = apply(&app, &menu);
                }
            });
            // SAFETY: the main queue runs the block on the main thread, where the menu is built.
            unsafe { NSOperationQueue::mainQueue().addOperationWithBlock(&apply_held) };
        })
    };
    // SAFETY: the blocks are `'static`, no queue is given, so they run on the main thread that
    // posts these notifications, and the center keeps the observers and the blocks alive for the
    // life of the process.
    unsafe {
        let center = NSNotificationCenter::defaultCenter();
        center.addObserverForName_object_queue_usingBlock(
            Some(NSMenuDidBeginTrackingNotification),
            None,
            None,
            &began,
        );
        center.addObserverForName_object_queue_usingBlock(
            Some(NSMenuDidEndTrackingNotification),
            None,
            None,
            &ended,
        );
    }
}

/// Whether `notification` is about the menu on the tray's status item rather than another menu of
/// the app.
#[cfg(target_os = "macos")]
fn is_tray_menu(app: &AppHandle, notification: &objc2_foundation::NSNotification) -> bool {
    use objc2::{rc::Retained, MainThreadMarker};

    // The address stands for the object, as the closure has to be `Send`.
    let Some(notified) = notification
        .object()
        .map(|object| Retained::as_ptr(&object) as usize)
    else {
        return false;
    };
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return false;
    };
    tray.with_inner_tray_icon(move |inner| {
        let menu = MainThreadMarker::new()
            .and_then(|mtm| inner.ns_status_item().and_then(|item| item.menu(mtm)));
        menu.is_some_and(|menu| Retained::as_ptr(&menu) as usize == notified)
    })
    .unwrap_or(false)
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
