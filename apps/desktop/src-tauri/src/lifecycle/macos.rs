//! The macOS side of the application's life: AppKit's own ways in and out of it (the Dock icon's
//! Quit and a logout, a reopen, an activation), and the Dock the app leaves while the close button
//! has the window hidden and comes back into when it is shown.

use std::sync::atomic::Ordering;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, RunEvent, WebviewWindow, Window};

use crate::apple_event::{current_enum_code, Slot};
use crate::background::{hidden_by_close, hide, show_main_window, BackgroundState};
use crate::diagnostics::diagnostic;
use crate::exit::{end_session, quit_let_through, should_let_quit_through, way_out_under_way};

/// How long after a Dock show a Dock hide has to wait. tao drops a Dock hide that comes less than
/// a second after a Dock show, because macOS gives no signal for the change having finished and
/// leaves duplicate Dock icons behind when the two follow each other too closely; the margin
/// covers the gap between our clock and tao's.
const DOCK_HIDE_HOLDOFF: Duration = Duration::from_millis(1100);

/// How long the window is given to leave native fullscreen before it is hidden. Hiding it while
/// still in fullscreen would leave its Space behind as an empty black screen; macOS's own
/// transition takes about half a second.
const LEAVE_FULLSCREEN_DELAY: Duration = Duration::from_millis(800);

#[derive(Default)]
pub struct LifecycleState {
    /// When the Dock was last asked to show the app, for `DOCK_HIDE_HOLDOFF`.
    last_dock_show: Mutex<Option<Instant>>,
}

/// Nothing to register first: LaunchServices keeps the app to one instance.
pub fn register_first(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder
}

/// Installs the app's answers to AppKit once `setup` runs, on the main thread.
pub fn install(app: &AppHandle) {
    show_on_activation(app);
    // `setup` runs once the event loop (and with it, tao's `NSApplicationDelegate`)
    // is up and running — `App::run` calls it on the runtime's `Ready` event — so the
    // delegate this installs onto already exists by now. See the function's own doc
    // comment for why the Dock icon's own Quit needs this at all.
    install_application_should_terminate_override(app);
}

pub fn on_run_event(app: &AppHandle, event: &RunEvent) {
    // Opening the app again while it runs in the background, from the Finder, Spotlight or the
    // Dock, brings the window back. Answered with a window still on screen too: one closed in
    // fullscreen stays visible while it leaves fullscreen, and this cancels its pending hide.
    if let RunEvent::Reopen { .. } = event {
        show_main_window(app);
    }
}

/// Takes a window in native fullscreen out of it and hides it once it has left (`hide`), and
/// answers whether it did; a window that is not in fullscreen is left to the caller to hide.
pub fn hide_after_leaving_fullscreen(window: &Window, shown: u64) -> bool {
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
        return true;
    }
    false
}

/// What follows the window's hide by the close button: the app gives up the focus and leaves the
/// Dock.
pub fn window_hidden(app: &AppHandle, shown: u64) {
    deactivate(app);
    hide_dock(app, shown);
}

/// What precedes showing the window: the app comes back into the Dock if the window had taken it
/// out.
pub fn before_show(app: &AppHandle, window: &WebviewWindow) {
    // A window that is on screen, or minimized into the Dock, has the app in the Dock already,
    // and showing it again would only keep a later hide waiting.
    if !window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(false) {
        restore_dock(app, window);
    }
}

/// Hiding the window leaves the app the active one, and an app that is already active gets no
/// activation when a notification is clicked, so there would be nothing to bring the window back
/// with. Hiding the app (what Cmd+H does) passes the focus on to another app; `restore_dock`
/// undoes it.
fn deactivate(app: &AppHandle) {
    let _ = app.run_on_main_thread(|| {
        if let Some(mtm) = objc2::MainThreadMarker::new() {
            objc2_app_kit::NSApplication::sharedApplication(mtm).hide(None);
        }
    });
}

/// Brings the window back when the app is activated while the close button has it hidden: by a
/// click on one of its notifications, which no other event reports. Every path through
/// `show_main_window` (the tray menu's Open and session lines, a reopen, `bring_to_front`) clears
/// `hidden_by_close` before it activates the app, so none is answered twice, and Cmd+H never sets
/// it, so unhiding the app that way is left to AppKit. Called on the main thread.
fn show_on_activation(app: &AppHandle) {
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

fn hide_dock(app: &AppHandle, shown: u64) {
    let since_show = app
        .state::<LifecycleState>()
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

/// Unhides the app, puts it back in the Dock and undoes what tao's Dock hide did to the window. All
/// of it runs as one task on the main thread: tao applies the Dock change right away there, so the
/// steps after it see its result, and a caller on another thread gets the same order.
fn restore_dock(app: &AppHandle, window: &tauri::WebviewWindow) {
    *app.state::<LifecycleState>()
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
fn reapply_dock_badge() {
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return;
    };
    let tile = objc2_app_kit::NSApplication::sharedApplication(mtm).dockTile();
    let label = tile.badgeLabel();
    tile.setBadgeLabel(None);
    tile.setBadgeLabel(label.as_deref());
}

/// `keyAEQuitReason` ('why?'): the attribute of the quit Apple event that says why the application
/// is asked to quit, when the system asks.
const KEY_QUIT_REASON: u32 = u32::from_be_bytes(*b"why?");

/// Whether `code`, the enum value of a quit event's reason, is one the system ends a session with:
/// `kAELogOut`, `kAEReallyLogOut`, `kAEShowRestartDialog`, `kAEShowShutdownDialog`, `kAERestart`
/// and `kAEShutDown`. A quit of the user's (Cmd+Q, the Dock's Quit) carries none of them.
fn is_system_end_reason(code: u32) -> bool {
    [b"logo", b"rlgo", b"rrst", b"rsdn", b"rest", b"shut"]
        .into_iter()
        .any(|reason| code == u32::from_be_bytes(*reason))
}

/// The reason the quit event being handled now carries, `None` for one that carries none (a plain
/// Dock Quit or an AppleScript quit) or when no event is current. The reason is an attribute of the
/// event, with the parameter as a fallback for a sender that puts it there.
fn current_quit_reason() -> Option<u32> {
    current_enum_code(KEY_QUIT_REASON, Slot::Attribute)
        .or_else(|| current_enum_code(KEY_QUIT_REASON, Slot::Param))
}

/// Whether the termination being asked about is the system ending the session: its quit event
/// names such a reason. One that names none is taken for the user's, so it asks.
fn system_is_ending() -> bool {
    current_quit_reason().is_some_and(is_system_end_reason)
}

/// Holds the `AppHandle` for `application_should_terminate` below to reach — an ObjC method
/// carries no user data of its own, so this `OnceLock`, set once from `install_..._override`
/// during `setup`, is the only way that callback can get at `ExitState` and emit into the window.
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// Installs an `applicationShouldTerminate:` override on the `NSApplication` delegate tao already
/// created by the time `setup` runs (see `install`). The Dock icon's own Quit sends
/// `terminate:` straight to the process; this selector is AppKit's one hook for answering it, where
/// Cmd+Q and the app menu's `MenuItem` both reach `on_menu_event` instead.
///
/// One override for every `terminate:` this selector is asked about — a Dock Quit, but equally a
/// system-initiated one from a logout, restart, or shutdown. A Dock Quit asks for the confirmation
/// like any quit. A system-initiated one (`system_is_ending`: the quit event's reason) never asks,
/// whatever the settings: the daemon makes its ordinary stop first (`end_session`), also over an
/// update restart's planned stop under way, which it ends, freezing each session's preview, and the
/// sessions it leaves interrupted keep their last screen as the preview. A quit
/// the user starts is told apart by carrying none of the system's reasons, and one that carries no
/// reason at all asks. No announcement of the power-off is used instead, as a logout can be
/// cancelled after it.
///
/// A Dock Quit while an update restart or an installing quit is under way
/// (`way_out_under_way`) is cancelled: that way out ends the process itself, and letting this one
/// through would end it mid-restart.
///
/// tao 0.35.3's `TaoAppDelegateParent` does not implement this selector (verified against its
/// source: it registers `applicationDidFinishLaunching:`, `applicationWillTerminate:`, and a
/// handful of others, not this one), so the `class_addMethod` call below adds a method rather than
/// overriding one — there is no existing implementation for it to disagree with.
fn install_application_should_terminate_override(app: &AppHandle) {
    APP_HANDLE
        .set(app.clone())
        .expect("installed exactly once, from `setup`");

    use objc2::runtime::AnyObject;
    use objc2::{Encode, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSApplicationTerminateReply};

    // `setup` runs on the main thread (it is where tao's event loop gets built), so this is
    // always `Some`.
    let mtm = MainThreadMarker::new().expect("`setup` runs on the main thread");
    let delegate = NSApplication::sharedApplication(mtm)
        .delegate()
        .expect("tao installs its own delegate before `setup` runs");
    let delegate: &AnyObject = delegate.as_ref();
    let class = delegate.class() as *const objc2::runtime::AnyClass as *mut _;

    // SAFETY: `class` is tao's own, already-registered delegate class (see the doc comment above
    // for why adding a method to it is safe), and the hand-built type-encoding string below is
    // assembled from the very `Encode` impls that `objc2-app-kit`'s generated bindings derived
    // from the real Objective-C declaration, so it matches what AppKit expects to find.
    unsafe {
        let types = std::ffi::CString::new(format!(
            "{}{}{}{}",
            NSApplicationTerminateReply::ENCODING,
            <*mut AnyObject>::ENCODING,
            objc2::runtime::Sel::ENCODING,
            <&NSApplication>::ENCODING,
        ))
        .expect("encoding string has no interior NUL");
        let imp: objc2::runtime::Imp = std::mem::transmute::<
            unsafe extern "C-unwind" fn(
                &AnyObject,
                objc2::runtime::Sel,
                &NSApplication,
            ) -> NSApplicationTerminateReply,
            objc2::runtime::Imp,
        >(application_should_terminate);
        let added = objc2::ffi::class_addMethod(
            class,
            objc2::sel!(applicationShouldTerminate:),
            imp,
            types.as_ptr(),
        );
        if !added.as_bool() {
            // A tao that implements this selector itself lands here instead of panicking: an app
            // that fails to launch is worse for the end user than one where Dock Quit and a
            // system-initiated logout/restart/shutdown bypass the confirmation (Cmd+Q and the app
            // menu are unaffected either way, since they go through `on_menu_event` instead).
            diagnostic!(
                "applicationShouldTerminate: is already implemented on tao's delegate \
                 — Dock Quit and a system-initiated logout/restart/shutdown will bypass the exit \
                 confirmation."
            );
        }
    }
}

/// The override installed above.
///
/// Wrapped in `catch_unwind` purely as a backstop, not because any panic is expected to reach it:
/// `should_let_quit_through`'s only lock site survives a poisoned `ExitState` mutex rather than
/// panicking (see its `unwrap_or_else` there). But letting some other, truly unrelated panic unwind
/// out of this function would unwind across the Objective-C message-send that invoked it as this
/// selector's `IMP` — undefined behaviour beyond this function's own `extern "C-unwind"` boundary,
/// which only guarantees a well-defined unwind up to here. `TerminateNow` is the safe fallback if
/// that ever happens: the one answer that cannot trap the user.
unsafe extern "C-unwind" fn application_should_terminate(
    _this: &objc2::runtime::AnyObject,
    _sel: objc2::runtime::Sel,
    _sender: &objc2_app_kit::NSApplication,
) -> objc2_app_kit::NSApplicationTerminateReply {
    use objc2_app_kit::NSApplicationTerminateReply;

    std::panic::catch_unwind(|| {
        let Some(app) = APP_HANDLE.get() else {
            // Can only happen before `setup` installs this override, and nothing sends
            // `terminate:` that early.
            return NSApplicationTerminateReply::TerminateNow;
        };
        // The system's own termination is not put through the exit flow at all: nothing is deferred
        // or recorded for it. The daemon is stopped before the system goes on, on this (the main)
        // thread, which blocks the logout for as long as the stop takes, bounded.
        if system_is_ending() {
            end_session(app);
            NSApplicationTerminateReply::TerminateNow
        } else if way_out_under_way(app) {
            // The restart or install under way ends the process by itself when it is done.
            NSApplicationTerminateReply::TerminateCancel
        } else if should_let_quit_through(app) {
            quit_let_through(app);
            NSApplicationTerminateReply::TerminateNow
        } else {
            // `should_let_quit_through` has already emitted `exit-requested`; nothing left to do
            // here but answer AppKit's question.
            NSApplicationTerminateReply::TerminateCancel
        }
    })
    .unwrap_or(NSApplicationTerminateReply::TerminateNow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reasons_the_system_ends_a_session_with_are_told_from_the_rest() {
        for reason in [b"logo", b"rlgo", b"rrst", b"rsdn", b"rest", b"shut"] {
            assert!(is_system_end_reason(u32::from_be_bytes(*reason)));
        }
        // `kAEQuitAll`, `kAEQuitApplication`, and no reason.
        assert!(!is_system_end_reason(u32::from_be_bytes(*b"quit")));
        assert!(!is_system_end_reason(u32::from_be_bytes(*b"quia")));
        assert!(!is_system_end_reason(0));
    }

    #[test]
    fn no_current_event_is_no_reason() {
        assert_eq!(current_quit_reason(), None);
    }
}
