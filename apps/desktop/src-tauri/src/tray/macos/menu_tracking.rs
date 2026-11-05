//! Following AppKit's notifications that a menu began and ended tracking.

use tauri::{AppHandle, Emitter};

use super::finish::finish_opening_menu;
use crate::tray::menu_tracking::tracking;
use crate::tray::{apply, MENU_OPENED_EVENT, TRAY_ID};

/// Keeps `MenuTracking` up to date from AppKit's notifications that a menu began and ended
/// tracking, both posted on the main thread.
pub(in crate::tray) fn follow_menu_tracking(app: &AppHandle) {
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
