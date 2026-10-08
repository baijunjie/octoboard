//! The exit-confirmation flow.
//!
//! Three gestures can end the process, and all three need the same answer to the same question —
//! is there a confirmation flow to defer to, and if so, has this attempt already cleared it or
//! does it need to wait: Cmd+Q and the app menu's Quit item (`on_menu_event` in `lib.rs`), the Dock
//! icon's own Quit plus a system-initiated logout/restart/shutdown (`application_should_terminate`
//! below, installed onto AppKit's own delegate), and `RunEvent::ExitRequested` (also handled in
//! `lib.rs`) — which in practice only ever serves `confirm_quit`'s own self-raised `app.exit(0)`, or
//! a window torn down before the frontend got as far as registering with `frontend_exit_heartbeat`.
//! The menu bar icon's Quit (`tray.rs`) takes the same path as the app menu's. The window's close
//! button is not a quit at all: it hides the window (`background.rs`), unless the daemon is not
//! running, when it quits without asking. This module owns the one decision the quit gestures share
//! (`should_let_quit_through`), so those call sites share it instead of each carrying their own
//! slowly diverging copy.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};

use crate::sidecar::daemon_stopped;

/// How soon a second prevented exit must follow the first for it to be let through unconditionally
/// — the escape hatch in `should_let_quit_through` below for a frontend that registered for the
/// exit flow and then wedged (the webview process died, or the page hung) before it could ever
/// call `confirm_quit`: without this, every quit gesture would keep re-emitting `exit-requested` to
/// a page that can no longer answer, forever. A short, no-timer "does a second attempt land right
/// behind the first" check is simpler and more predictable here than arming a deadline timer would
/// be — a deadline has to be captured and then either fire correctly or get cancelled if the user
/// cancels the dialog instead, and there is no signal back from the frontend for that cancellation
/// to clear it; this reuses only an `Instant` compared on the same thread that already handles the
/// event. Two seconds is long enough that reading and answering the confirmation dialog never comes
/// close to it, and short enough that it means "the same quit gesture, repeated" rather than a
/// second, unrelated quit attempt minutes later.
const FORCE_QUIT_WINDOW: Duration = Duration::from_secs(2);

/// Tracks the facts the exit flow needs across all three gestures described in the module comment
/// above:
///
/// - `confirmed`: set by `confirm_quit` right before it calls `app.exit(0)`, which raises its own
///   `ExitRequested`. Without this flag that self-raised event would be prevented again and the
///   application would never actually terminate — just end up running headlessly with no window.
/// - `frontend_registered`: set once the webview has called `frontend_exit_heartbeat`, meaning a
///   confirmation dialog actually exists to show. Before that point (or if the page never finishes
///   loading at all), deferring to it would trap the user in a window nothing can close — the
///   daemon-failed-to-start screen hits this if it does not register, so the default is to let the
///   exit through rather than trap first and hope the frontend shows up.
/// - `last_prevented_at`: when the previous attempt was deferred, for the `FORCE_QUIT_WINDOW` check
///   above. Cleared by `frontend_exit_heartbeat` too: `useAppExit.ts`'s `requestQuit` re-invokes it
///   on every quit gesture the frontend actually receives — not only when the user cancels the
///   dialog — which is itself proof the frontend is alive and answering, so the next gesture should
///   wait for it rather than treat a prompt the frontend is still showing as one that wedged. A
///   wedged webview never reaches that call at all, so the debounce stays armed and the escape
///   hatch below still fires for it.
#[derive(Default)]
pub struct ExitState {
    confirmed: AtomicBool,
    frontend_registered: AtomicBool,
    last_prevented_at: Mutex<Option<Instant>>,
}

/// Marks this webview as the one handling the exit flow, and doubles as the liveness ping that
/// clears the `FORCE_QUIT_WINDOW` debounce (see `last_prevented_at` above) — `useAppExit.ts` calls
/// this once at mount to register, and again on every quit gesture it handles thereafter. Until a
/// window registers at all, a quit is let through unchanged: a screen that cannot ask the user
/// about it must not be able to block it.
#[tauri::command]
pub fn frontend_exit_heartbeat(state: tauri::State<ExitState>) {
    state.frontend_registered.store(true, Ordering::SeqCst);
    *state
        .last_prevented_at
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = None;
}

/// Marks the pending quit as confirmed and asks Tauri to exit. The `RunEvent::ExitRequested`
/// handler in `lib.rs` sees `should_let_quit_through` consume the flag below and lets this one
/// through.
#[tauri::command]
pub fn confirm_quit(app: AppHandle, state: tauri::State<ExitState>) {
    state.confirmed.store(true, Ordering::SeqCst);
    app.exit(0);
}

/// What a quit gesture that reaches `on_menu_event` in `lib.rs` does: exits unless a confirmation
/// flow has taken the attempt over. `should_let_quit_through` itself emits `exit-requested` on the
/// `false` path, so there is nothing left to do here but act on its answer.
pub fn request_quit(app: &AppHandle) {
    if should_let_quit_through(app) {
        app.exit(0);
    }
}

/// The one decision all three quit gestures share. Returns `true` when the caller should let the
/// quit proceed unconditionally; `false` means a confirmation flow exists and has not yet cleared
/// this attempt, so the caller should defer to it instead of exiting — which this function has
/// already done its own share of: it has emitted `exit-requested` itself, so the caller only has to
/// add whatever is specific to its own mechanism (`prevent_exit`, `TerminateCancel`, or nothing).
///
/// Also records the deferral: the next call's `FORCE_QUIT_WINDOW` check depends on
/// `last_prevented_at` having been set here.
pub fn should_let_quit_through(app: &AppHandle) -> bool {
    if daemon_stopped(app) {
        // No session can be live, so there is nothing to ask about.
        return true;
    }
    let state = app.state::<ExitState>();
    if state.confirmed.swap(false, Ordering::SeqCst) {
        // Already confirmed by `confirm_quit` above — this is that call's own self-raised attempt,
        // not a fresh gesture.
        return true;
    }
    if !state.frontend_registered.load(Ordering::SeqCst) {
        // No webview has registered as handling the exit flow (it has not finished loading yet, or
        // never will) — nothing could ever confirm this quit, so let it proceed rather than trap
        // the user in a window they cannot close.
        return true;
    }

    let now = Instant::now();
    let mut last_prevented_at = state
        .last_prevented_at
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if last_prevented_at.is_some_and(|previous| now.duration_since(previous) < FORCE_QUIT_WINDOW) {
        // See `FORCE_QUIT_WINDOW`: a second attempt landed right behind the one already deferred,
        // so whatever should have answered it — the confirmation dialog — either never showed or
        // wedged. Let this one proceed as a normal, unconfirmed exit.
        return true;
    }
    *last_prevented_at = Some(now);
    let _ = app.emit("exit-requested", ());
    false
}

/// Holds the `AppHandle` for `application_should_terminate` below to reach — an ObjC method
/// carries no user data of its own, so this `OnceLock`, set once from `install_..._override`
/// during `setup`, is the only way that callback can get at `ExitState` and emit into the window.
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// Installs an `applicationShouldTerminate:` override on the `NSApplication` delegate tao already
/// created by the time `setup` runs (see the call site in `lib.rs`). The Dock icon's own Quit sends
/// `terminate:` straight to the process; this selector is AppKit's one hook for answering it, where
/// Cmd+Q and the app menu's `MenuItem` both reach `on_menu_event` instead.
///
/// One override, one behaviour, for every `terminate:` this selector is asked about — a Dock Quit,
/// but equally a system-initiated one from a logout, restart, or shutdown, which macOS answers with
/// this exact same selector and gives this process no way to tell apart from a Dock Quit.
/// Deliberately not special-cased back out: the accepted cost is that Octoboard can hold up a
/// logout or restart until the user answers the confirmation dialog, same as it already holds up a
/// Dock Quit.
///
/// tao 0.35.3's `TaoAppDelegateParent` does not implement this selector (verified against its
/// source: it registers `applicationDidFinishLaunching:`, `applicationWillTerminate:`, and a
/// handful of others, not this one), so the `class_addMethod` call below adds a method rather than
/// overriding one — there is no existing implementation for it to disagree with.
pub fn install_application_should_terminate_override(app: &AppHandle) {
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
            // A future tao that implements this selector itself would land here instead of
            // panicking: an app that fails to launch is worse for the end user than one where Dock
            // Quit and a system-initiated logout/restart/shutdown simply bypass the confirmation
            // and behave as they did before this override existed (Cmd+Q and the app menu are
            // unaffected either way, since they go through `on_menu_event` instead).
            eprintln!(
                "octoboard: applicationShouldTerminate: is already implemented on tao's delegate \
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
/// panicking (see its `unwrap_or_else` there), so there is no longer a reachable panic on this
/// path. But letting some other, truly unrelated panic unwind out of this function would unwind
/// across the Objective-C message-send that invoked it as this selector's `IMP` — undefined
/// behaviour beyond this function's own `extern "C-unwind"` boundary, which only guarantees a
/// well-defined unwind up to here. `TerminateNow` is the safe fallback if that ever happens: the
/// one answer that cannot trap the user.
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
        if should_let_quit_through(app) {
            NSApplicationTerminateReply::TerminateNow
        } else {
            // `should_let_quit_through` has already emitted `exit-requested`; nothing left to do
            // here but answer AppKit's question.
            NSApplicationTerminateReply::TerminateCancel
        }
    })
    .unwrap_or(NSApplicationTerminateReply::TerminateNow)
}
