//! The exit-confirmation flow.
//!
//! Three gestures can end the process, and all three need the same answer to the same question —
//! is there a confirmation flow to defer to, and if so, has this attempt already cleared it or
//! does it need to wait:
//!
//! - Cmd+Q and the app menu's Quit item (`on_menu_event` in `lib.rs`), along with the menu bar
//!   icon's Quit (`tray/`), which takes the same path.
//! - The Dock icon's own Quit plus a system-initiated logout/restart/shutdown
//!   (`application_should_terminate` in `lifecycle/macos.rs`, installed onto AppKit's own delegate).
//!   The system's own termination is told apart there and, while the UI has said it needs no
//!   confirmation (`set_system_quit_without_asking`), is let through without one.
//! - `RunEvent::ExitRequested` (also handled in `lib.rs`), which in practice only ever serves
//!   `confirm_quit`'s own self-raised `app.exit(0)` or restart, or a window torn down before the
//!   frontend got as far as registering with `frontend_exit_heartbeat`.
//!
//! The window's close button is a fourth way out, except that it hides the window instead when the
//! user chose to run in the background (`background.rs`); with no daemon left to keep running it
//! quits whatever the choice. Cmd+W and the Window menu's Close Window item are the same gesture to
//! the shell. When the close route quits, it reaches the same confirmation flow through
//! `request_quit_from_window`, with one difference: it never arms the `FORCE_QUIT_WINDOW`
//! debounce, because a double press there is ordinary and must not force a quit through. This
//! module owns the one decision the three quit gestures above share (`should_let_quit_through`), so
//! those call sites share it instead of each carrying their own slowly diverging copy.
//!
//! Whichever way a quit gets through, a downloaded update the application still holds (Windows,
//! `updater::holds_download`) is installed on the way out, as a confirmed quit installs it: the exit
//! is held back while the daemon stops and the installer starts (`on_exit_requested`). The system
//! ending the user session (sign-out, shutdown) is not a quit: Windows ends the process with
//! `RunEvent::Exit` and no `ExitRequested` before it, and `install_on_exit` leaves the update alone
//! then, to be offered again at the next start.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};

use crate::diagnostics::diagnostic;
use crate::sidecar::{daemon_stopped, mark_daemon_stopping, stop_daemon};
use crate::updater;

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
/// second, unrelated quit attempt minutes later. It is only ever armed by the three quit gestures
/// the module comment lists, where a repeat is deliberate (a double Cmd+Q): the window's close
/// route leaves it alone (`request_quit_from_window`), since a double click there is ordinary and
/// must not quit a live session unasked.
const FORCE_QUIT_WINDOW: Duration = Duration::from_secs(2);

/// Tracks the facts the exit flow needs across the gestures described in the module comment
/// above:
///
/// - `confirmed`: set by `confirm_quit` right before it calls `app.exit(0)`, and by
///   `quit_installing` right before its own `app.exit(0)` and the restart of `confirm_quit` right
///   before its relaunch, when no installer ended the process, each of which raises its own
///   `ExitRequested`. Without this flag that self-raised event would be
///   prevented again and the application would never actually terminate — just end up running
///   headlessly with no window. They set it again there because a quit made while they waited
///   (through the menu, tray or window) may have consumed the flag in `should_let_quit_through`, so
///   their exit would otherwise be asked about once more.
/// - `frontend_registered`: set once the webview has called `frontend_exit_heartbeat`, meaning a
///   confirmation dialog actually exists to show. Before that point (or if the page never finishes
///   loading at all), deferring to it would trap the user in a window nothing can close — the
///   daemon-failed-to-start screen, which never registers, relies on this, so the default is to let
///   the exit through rather than trap first and hope the frontend shows up.
/// - `last_prevented_at`: when the previous attempt was deferred, for the `FORCE_QUIT_WINDOW` check
///   above. Cleared by `frontend_exit_heartbeat` too: `useAppExit.ts`'s `request` re-invokes it
///   on every quit gesture the frontend actually receives — not only when the user cancels the
///   dialog — which is itself proof the frontend is alive and answering, so the next gesture should
///   wait for it rather than treat a prompt the frontend is still showing as one that wedged. A
///   wedged webview never reaches that call at all, so the debounce stays armed and the escape
///   hatch below still fires for it.
/// - `installing`: set while the way out stops the daemon and starts the installer of a held update
///   (`quit_installing`, and the restart of `confirm_quit`, which installs one where the platform
///   holds it). Every exit requested meanwhile through `RunEvent::ExitRequested` waits for it (a quit
///   through the menu, tray or window; the Dock's Quit and a logout, which `lifecycle/macos.rs`
///   answers on its own, are not held), so none can end the process before the
///   installer has started, and a second restart or quit does not start another; it is cleared
///   before the exit or relaunch that follows when no installer ended the process.
/// - `exit_let_through`: set when an `ExitRequested` was let through (`ExitRoute::Proceed`). Every
///   exit a user or the application starts passes there before `RunEvent::Exit`; one that reaches
///   `RunEvent::Exit` without it is the system ending the session (`install_on_exit`).
/// - `system_quit_without_asking`: the UI's word that a termination the system starts needs no
///   confirmation (`set_system_quit_without_asking`).
#[derive(Default)]
pub struct ExitState {
    confirmed: AtomicBool,
    frontend_registered: AtomicBool,
    last_prevented_at: Mutex<Option<Instant>>,
    installing: AtomicBool,
    exit_let_through: AtomicBool,
    system_quit_without_asking: AtomicBool,
}

impl ExitState {
    /// Takes the `installing` guard: `false` when a way out already holds it, which this one then
    /// leaves alone.
    fn begin_installing(&self) -> bool {
        !self.installing.swap(true, Ordering::SeqCst)
    }

    /// Gives the `installing` guard back, before an exit or relaunch that no installer made.
    fn end_installing(&self) {
        self.installing.store(false, Ordering::SeqCst);
    }
}

/// Holds the `installing` guard of a way out that runs on a blocking task, and gives it back when
/// dropped: before the exit or relaunch that follows, or when the task unwinds, which would
/// otherwise leave every later exit waiting for an installer that is never started.
struct InstallingGuard<'a>(&'a ExitState);

impl Drop for InstallingGuard<'_> {
    fn drop(&mut self) {
        self.0.end_installing();
    }
}

/// Tells the shell whether a termination the system starts (macOS: logging out, restarting or
/// shutting down) is let through without the confirmation, which is the case while the daemon
/// resumes the sessions the quit leaves interrupted at its next start (the daemon's Resume sessions
/// on start). The UI sends it once it has the daemon's settings and on every change; until it has,
/// the shell asks as for any quit. A quit the user starts is never affected.
#[tauri::command]
pub fn set_system_quit_without_asking(state: tauri::State<ExitState>, enabled: bool) {
    state
        .system_quit_without_asking
        .store(enabled, Ordering::SeqCst);
}

/// Whether the system ending the session may quit the application without the confirmation (see
/// `set_system_quit_without_asking`).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn system_quit_without_asking(app: &AppHandle) -> bool {
    app.state::<ExitState>()
        .system_quit_without_asking
        .load(Ordering::SeqCst)
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
/// handler in `lib.rs` (`on_exit_requested`) sees `should_let_quit_through` consume the flag below
/// and lets this one through.
///
/// With `restart` the process is relaunched instead of just ending, which is how an installed
/// update takes effect (`updater/`): the UI asks for it through the same confirmation as any
/// quit, since a restart ends the sessions as a quit does. The new instance's daemon would find this
/// one's instance lock held and fail to start, so the daemon is stopped first, off the main thread
/// because that waits, under the `installing` guard every exit shares; where the platform holds a
/// downloaded update its installer then ends the process (`updater::install_for_restart`), saving
/// the window state first, and otherwise the relaunch goes through the event loop like any exit, so
/// `RunEvent::Exit` saves it on the way out. A plain quit while a downloaded update waits (Windows)
/// stops the daemon the same way and installs it without the relaunch (`quit_installing`).
#[tauri::command]
pub fn confirm_quit(app: AppHandle, state: tauri::State<ExitState>, restart: Option<bool>) {
    state.confirmed.store(true, Ordering::SeqCst);
    // Whatever the daemon's own end looks like from here, it is not to be started again.
    mark_daemon_stopping(&app);
    if restart == Some(true) {
        if !state.begin_installing() {
            return;
        }
        tauri::async_runtime::spawn_blocking(move || {
            let state = app.state::<ExitState>();
            let installing = InstallingGuard(&state);
            stop_daemon(&app);
            updater::install_for_restart(&app);
            // No installer ended the process. The relaunch is an exit of its own, which the guard
            // would hold back, and which a quit through the menu, tray or window may have taken
            // the confirmation of meanwhile.
            state.confirmed.store(true, Ordering::SeqCst);
            drop(installing);
            app.request_restart();
        });
        return;
    }
    if updater::holds_download(&app) {
        quit_installing(&app);
        return;
    }
    app.exit(0);
}

/// What becomes of an exit Tauri is about to make (`RunEvent::ExitRequested`).
#[derive(Debug, PartialEq)]
enum ExitRoute {
    /// The installer of a held update is being started: hold the exit back, as that ends the process.
    Wait,
    /// A confirmation flow has taken the attempt over (`should_let_quit_through` said so and has
    /// emitted `exit-requested`): hold the exit back.
    Defer,
    /// Hold the exit back and end through `quit_installing`, which installs the held update.
    Install,
    /// Let the process end.
    Proceed,
}

/// The route of an exit: `installing` says whether `quit_installing` is under way, `let_through`
/// is the quit decision (`should_let_quit_through`) and `holds_download` whether an update waits to
/// be installed. The two are asked only when the answer depends on them, as the decision records a
/// deferral and emits `exit-requested`.
fn exit_route(
    installing: bool,
    let_through: impl FnOnce() -> bool,
    holds_download: impl FnOnce() -> bool,
) -> ExitRoute {
    if installing {
        ExitRoute::Wait
    } else if !let_through() {
        ExitRoute::Defer
    } else if holds_download() {
        ExitRoute::Install
    } else {
        ExitRoute::Proceed
    }
}

/// Answers `RunEvent::ExitRequested`: whether to prevent the exit. Every quit that the flow lets
/// through without `confirm_quit` reaches the process's end here (the daemon already stopped, the
/// repeated gesture of `FORCE_QUIT_WINDOW`, no registered frontend, a start that failed), so this is
/// where a held update is installed for them.
pub fn on_exit_requested(app: &AppHandle) -> bool {
    let installing = app.state::<ExitState>().installing.load(Ordering::SeqCst);
    match exit_route(
        installing,
        || should_let_quit_through(app),
        || updater::holds_download(app),
    ) {
        ExitRoute::Proceed => {
            app.state::<ExitState>()
                .exit_let_through
                .store(true, Ordering::SeqCst);
            false
        }
        ExitRoute::Wait | ExitRoute::Defer => true,
        ExitRoute::Install => {
            quit_installing(app);
            true
        }
    }
}

/// Ends the application with the held update installed on the way, without relaunching it: stops
/// the daemon, whose executable the installer replaces, off the main thread because that waits, then
/// starts the installer, which ends the process. One that could not be started leaves an ordinary
/// exit, already confirmed. Only the first call does anything; a quit while it runs waits for it.
fn quit_installing(app: &AppHandle) {
    if !app.state::<ExitState>().begin_installing() {
        return;
    }
    mark_daemon_stopping(app);
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<ExitState>();
        let installing = InstallingGuard(&state);
        stop_daemon(&app);
        updater::install_on_quit(&app);
        state.confirmed.store(true, Ordering::SeqCst);
        drop(installing);
        app.exit(0);
    });
}

/// What `RunEvent::Exit` does about a held update.
#[derive(Debug, PartialEq)]
enum ExitEnd {
    /// Install it: a quit of the user's was let through, and a download finished since.
    Install,
    /// Leave it for the next start: the system ended the session, which is no quit.
    Leave,
    /// No update is held.
    Nothing,
}

/// What a held update comes to at `RunEvent::Exit`: `exit_let_through` says whether an
/// `ExitRequested` was let through before it, which only an exit of the user's or the application's
/// own has, and `holds_download` whether an update waits to be installed.
fn exit_end(exit_let_through: bool, holds_download: bool) -> ExitEnd {
    match (holds_download, exit_let_through) {
        (false, _) => ExitEnd::Nothing,
        (true, true) => ExitEnd::Install,
        (true, false) => ExitEnd::Leave,
    }
}

/// Settles a held update at `RunEvent::Exit`. It is installed only for the narrow case of a download
/// finishing between a quit being let through and the exit; the system ending the session (Windows
/// sign-out or shutdown) installs nothing, and the next start finds the update again. The process
/// ends once the event returns, so the daemon is stopped and the installer started here, on the main
/// thread, which has nothing left to serve.
pub fn install_on_exit(app: &AppHandle) {
    let let_through = app
        .state::<ExitState>()
        .exit_let_through
        .load(Ordering::SeqCst);
    match exit_end(let_through, updater::holds_download(app)) {
        ExitEnd::Nothing => {}
        ExitEnd::Leave => {
            diagnostic!("the session ended: leaving the downloaded update for the next start");
        }
        ExitEnd::Install => {
            if !app.state::<ExitState>().begin_installing() {
                return;
            }
            diagnostic!("installing the downloaded update as the application exits");
            stop_daemon(app);
            updater::install_on_quit(app);
        }
    }
}

/// What a quit gesture that reaches `on_menu_event` in `lib.rs` does: exits unless a confirmation
/// flow has taken the attempt over. `should_let_quit_through` itself emits `exit-requested` on the
/// `false` path, so there is nothing left to do here but act on its answer.
pub fn request_quit(app: &AppHandle) {
    if should_let_quit_through(app) {
        app.exit(0);
    }
}

/// What the window's close route does when it quits (run-in-background off, or no daemon left to
/// keep running): the same as `request_quit`, except that it neither arms nor consumes the
/// `FORCE_QUIT_WINDOW` debounce. A second press of the close button right behind the first, a
/// double click, is an ordinary gesture, and must not take the escape hatch and quit a live
/// session unasked. The cost is that a wedged webview cannot be quit through the close button
/// alone; two Cmd+Q still do it.
pub fn request_quit_from_window(app: &AppHandle) {
    if decide(app, false) {
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
    decide(app, true)
}

/// `should_let_quit_through`, with `debounce` saying whether `FORCE_QUIT_WINDOW` applies: when it
/// does not, the check is skipped and the deferral is not recorded.
fn decide(app: &AppHandle, debounce: bool) -> bool {
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

    if !debounce {
        let _ = app.emit("exit-requested", ());
        return false;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn never() -> bool {
        panic!("not to be asked")
    }

    #[test]
    fn one_way_out_at_a_time_holds_the_installing_guard() {
        let state = ExitState::default();
        assert!(state.begin_installing());
        assert!(
            !state.begin_installing(),
            "a second restart or quit is turned back"
        );
        state.end_installing();
        assert!(state.begin_installing());
    }

    #[test]
    fn a_way_out_that_unwinds_gives_the_installing_guard_back() {
        let state = ExitState::default();
        assert!(state.begin_installing());
        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _installing = InstallingGuard(&state);
            panic!("the daemon's stop failed");
        }));
        assert!(unwound.is_err());
        assert!(state.begin_installing());
    }

    #[test]
    fn an_exit_while_the_installer_starts_waits_without_asking_anything() {
        assert_eq!(exit_route(true, never, never), ExitRoute::Wait);
    }

    #[test]
    fn an_exit_the_confirmation_holds_is_deferred_whatever_is_downloaded() {
        assert_eq!(exit_route(false, || false, never), ExitRoute::Defer);
    }

    #[test]
    fn a_quit_let_through_installs_a_held_update() {
        assert_eq!(exit_route(false, || true, || true), ExitRoute::Install);
    }

    #[test]
    fn a_quit_let_through_with_nothing_held_ends_the_process() {
        assert_eq!(exit_route(false, || true, || false), ExitRoute::Proceed);
    }

    #[test]
    fn a_session_end_leaves_a_held_update_uninstalled() {
        assert_eq!(exit_end(false, true), ExitEnd::Leave);
    }

    #[test]
    fn an_exit_let_through_installs_a_held_update() {
        assert_eq!(exit_end(true, true), ExitEnd::Install);
    }

    #[test]
    fn an_exit_with_nothing_held_does_nothing() {
        assert_eq!(exit_end(true, false), ExitEnd::Nothing);
        assert_eq!(exit_end(false, false), ExitEnd::Nothing);
    }
}
