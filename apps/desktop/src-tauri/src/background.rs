//! Running in the background: the window's close button hides the window and takes the app out of
//! the Dock instead of quitting, leaving it reachable through the menu bar icon (`tray.rs`), the
//! Dock icon or a reopen from the Finder or Spotlight, all of which bring the window back through
//! `show_main_window`. The UI's `bringToFront` does too, through `bring_to_front`, for the exit
//! confirmation of a quit that came while the window was hidden and for the notice that the daemon
//! exited. Activating the app while the window is hidden by a close, such as by clicking a system
//! notification, brings it back too. With no daemon left running there is nothing to keep in the
//! background, so the close button quits instead.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
#[cfg(target_os = "macos")]
use std::sync::Mutex;
#[cfg(target_os = "macos")]
use std::time::{Duration, Instant};

use tauri::{AppHandle, CloseRequestApi, Manager, Window};

use crate::sidecar::daemon_stopped;
use crate::MAIN_WINDOW_LABEL;

/// How long after a Dock show a Dock hide has to wait. tao drops a Dock hide that comes less than
/// a second after a Dock show, because macOS gives no signal for the change having finished and
/// leaves duplicate Dock icons behind when the two follow each other too closely; the margin
/// covers the gap between our clock and tao's.
#[cfg(target_os = "macos")]
const DOCK_HIDE_HOLDOFF: Duration = Duration::from_millis(1100);

/// How long the window is given to leave native fullscreen before it is hidden. Hiding it while
/// still in fullscreen would leave its Space behind as an empty black screen; macOS's own
/// transition takes about half a second.
#[cfg(target_os = "macos")]
const LEAVE_FULLSCREEN_DELAY: Duration = Duration::from_millis(800);

#[derive(Default)]
pub struct BackgroundState {
    /// Bumped by every show of the window, so a hide that waited for something (a fullscreen
    /// transition, the Dock's hold-off) can tell the window was shown again in the meantime and
    /// leave it alone.
    shown: AtomicU64,
    /// Whether the window is hidden by the close button, as opposed to not shown yet or hidden by
    /// the system (Cmd+H): the state in which the app is out of the Dock and activating it brings
    /// the window back.
    hidden_by_close: AtomicBool,
    /// When the Dock was last asked to show the app, for `DOCK_HIDE_HOLDOFF`.
    #[cfg(target_os = "macos")]
    last_dock_show: Mutex<Option<Instant>>,
}

/// Whether the window is hidden by the close button; false until the first close.
pub fn hidden_by_close(app: &AppHandle) -> bool {
    app.state::<BackgroundState>()
        .hidden_by_close
        .load(Ordering::SeqCst)
}

/// Answers the main window's close button: hides the window and takes the app out of the Dock,
/// or quits when the daemon is not running.
pub fn handle_close_request(window: &Window, api: &CloseRequestApi) {
    if window.label() != MAIN_WINDOW_LABEL {
        return;
    }
    api.prevent_close();
    if daemon_stopped(window.app_handle()) {
        window.app_handle().exit(0);
    } else {
        hide_main_window(window);
    }
}

fn hide_main_window(window: &Window) {
    let shown = window
        .app_handle()
        .state::<BackgroundState>()
        .shown
        .load(Ordering::SeqCst);
    #[cfg(target_os = "macos")]
    if window.is_fullscreen().unwrap_or(false) {
        let _ = window.set_fullscreen(false);
        let window = window.clone();
        std::thread::spawn(move || {
            std::thread::sleep(LEAVE_FULLSCREEN_DELAY);
            // On the main thread, where every show runs too, so a show cannot slip in between the
            // check in `hide` and the hide itself while the main thread is busy animating.
            let app = window.app_handle().clone();
            let _ = app.run_on_main_thread(move || hide(&window, shown));
        });
        return;
    }
    hide(window, shown);
}

/// Hides the window unless it has been shown again since `shown` was read.
fn hide(window: &Window, shown: u64) {
    let app = window.app_handle();
    if app.state::<BackgroundState>().shown.load(Ordering::SeqCst) != shown {
        return;
    }
    let _ = window.hide();
    app.state::<BackgroundState>()
        .hidden_by_close
        .store(true, Ordering::SeqCst);
    #[cfg(target_os = "macos")]
    {
        deactivate(app);
        hide_dock(app, shown);
    }
}

/// Hiding the window leaves the app the active one, and an app that is already active gets no
/// activation when a notification is clicked, so there would be nothing to bring the window back
/// with. Hiding the app (what Cmd+H does) passes the focus on to another app; `restore_dock`
/// undoes it.
#[cfg(target_os = "macos")]
fn deactivate(app: &AppHandle) {
    let _ = app.run_on_main_thread(|| {
        if let Some(mtm) = objc2::MainThreadMarker::new() {
            objc2_app_kit::NSApplication::sharedApplication(mtm).hide(None);
        }
    });
}

/// Brings the window back when the app is activated while the close button has it hidden: by a
/// click on one of its notifications, which no other event reports. Every path through
/// `show_main_window` (the tray click and menu, a reopen, `bring_to_front`) clears
/// `hidden_by_close` before it activates the app, so none is answered twice, and Cmd+H never sets
/// it, so unhiding the app that way is left to AppKit. Called on the main thread.
#[cfg(target_os = "macos")]
pub fn show_on_activation(app: &AppHandle) {
    use objc2_app_kit::NSApplicationDidBecomeActiveNotification;
    use objc2_foundation::{NSNotification, NSNotificationCenter};

    let app = app.clone();
    let block = block2::RcBlock::new(move |_: std::ptr::NonNull<NSNotification>| {
        if hidden_by_close(&app) {
            show_main_window(&app);
        }
    });
    // SAFETY: the block is `'static` and sendable, and no queue is given, so it runs on the thread
    // that posts the notification, which for activation is the main one. The center keeps the
    // observer and the block alive for the life of the process.
    unsafe {
        NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(NSApplicationDidBecomeActiveNotification),
            None,
            None,
            &block,
        );
    }
}

#[cfg(target_os = "macos")]
fn hide_dock(app: &AppHandle, shown: u64) {
    let state = app.state::<BackgroundState>();
    let since_show = state
        .last_dock_show
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .map(|at| at.elapsed());
    let Some(wait) = since_show.and_then(|elapsed| DOCK_HIDE_HOLDOFF.checked_sub(elapsed)) else {
        let _ = app.set_dock_visibility(false);
        return;
    };
    // The Dock was shown a moment ago, so tao would drop the hide now; ask again once it would
    // not, provided the window is still hidden by then.
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(wait);
        if app.state::<BackgroundState>().shown.load(Ordering::SeqCst) == shown {
            let _ = app.set_dock_visibility(false);
        }
    });
}

/// `show_main_window` for the UI, which the exit confirmation uses to raise a window that may be
/// hidden. Carries nothing.
#[tauri::command]
pub fn bring_to_front(app: AppHandle) {
    show_main_window(&app);
}

/// Brings the window to the front, restoring it first if it was hidden or minimized, and puts the
/// app back in the Dock if the window had taken it out.
pub fn show_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return;
    };
    let state = app.state::<BackgroundState>();
    state.shown.fetch_add(1, Ordering::SeqCst);
    state.hidden_by_close.store(false, Ordering::SeqCst);
    // A window that is on screen, or minimized into the Dock, has the app in the Dock already,
    // and showing it again would only keep a later hide waiting.
    #[cfg(target_os = "macos")]
    if !window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(false) {
        restore_dock(app, &window);
    }
    let _ = window.unminimize();
    let _ = window.show();
    // Only activates the app for a window that is visible and not minimized, so it comes last.
    let _ = window.set_focus();
}

/// Unhides the app, puts it back in the Dock and undoes what tao's Dock hide did to the window. All
/// of it runs as one task on the main thread: tao applies the Dock change right away there, so the
/// steps after it see its result, and a caller on another thread gets the same order.
#[cfg(target_os = "macos")]
fn restore_dock(app: &AppHandle, window: &tauri::WebviewWindow) {
    *app.state::<BackgroundState>()
        .last_dock_show
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
    let app_handle = app.clone();
    let window = window.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(mtm) = objc2::MainThreadMarker::new() {
            objc2_app_kit::NSApplication::sharedApplication(mtm).unhide(None);
        }
        let _ = app_handle.set_dock_visibility(true);
        allow_hiding_with_cmd_h(&window);
        reapply_dock_badge();
    });
}

/// tao's Dock hide stops every window from being hidden by Cmd+H and never lifts that when the
/// app returns to the Dock.
#[cfg(target_os = "macos")]
fn allow_hiding_with_cmd_h(window: &tauri::WebviewWindow) {
    let Ok(ns_window) = window.ns_window() else {
        return;
    };
    // SAFETY: the pointer is the window's own `NSWindow`, which outlives this call, and the task
    // calling this runs on the main thread.
    unsafe { &*ns_window.cast::<objc2_app_kit::NSWindow>() }.setCanHide(true);
}

/// Coming back into the Dock gives the app a fresh tile that shows no badge, although the dock
/// tile still holds the label the UI last set (`setBadgeCount`, kept up while hidden too), and the
/// UI sets it again only when the count changes. Setting the label anew puts it on the new tile.
/// macOS reports no completion for the change into a Dock app (see `DOCK_HIDE_HOLDOFF`), so this
/// rests on the new tile existing by the time it runs, which it has done in practice.
#[cfg(target_os = "macos")]
fn reapply_dock_badge() {
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return;
    };
    let tile = objc2_app_kit::NSApplication::sharedApplication(mtm).dockTile();
    let label = tile.badgeLabel();
    tile.setBadgeLabel(None);
    tile.setBadgeLabel(label.as_deref());
}
