//! The exit-confirmation flow.
//!
//! Three gestures can end the process, and all three need the same answer to the same question —
//! is there a confirmation flow to defer to, and if so, has this attempt already cleared it or
//! does it need to wait:
//!
//! - Cmd+Q and the app menu's Quit item (`on_menu_event` in `lib.rs`), along with the menu bar
//!   icon's Quit (`tray/`), which takes the same path.
//! - The Dock icon's own Quit plus a system-initiated logout/restart/shutdown
//!   (`application_should_terminate` in `lifecycle/macos.rs`, installed onto AppKit's own
//!   delegate). The system's own termination is told apart there and is never asked about: it ends
//!   the daemon at once with its ordinary stop, the quit record and note included (`end_session`),
//!   an update restart's planned stop under way too, and lets the process go.
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
//! Every way out that is a quit tells the daemon so before the process ends (`note_quit` in
//! `sidecar/quit_note.rs`): it leaves a durable quit record for the daemon's next start and writes
//! the note to the running one, and a daemon that finds the application gone after that makes an
//! ordinary stop, which interrupts the sessions, while one that was told nothing, because the
//! application crashed or was killed, makes a planned stop that leaves them running for the next
//! launch. The UI's plain quit has `begin_quit` do it before its own shutdown request, so a daemon
//! that ends in between leaves the intent behind; a confirmed quit, an exit let through
//! (`quit_let_through`) and `RunEvent::Exit` do it again, which is written at most once. The update
//! restart is no quit: its daemon is stopped with the planned stop (`StopKind::Planned`), after
//! which the next launch adopts the sessions, and it never writes the record or the note. The
//! system ending the user's session is not asked about and is an ordinary stop, the record and
//! note included, even where an update restart's planned stop is under way, which it ends:
//! `end_session` passes it on to the daemon as far as the time the system allows, both where AppKit
//! asks (macOS) and where `RunEvent::Exit` is reached with no exit let through before it (Windows
//! sign-out and shutdown). It takes priority over the installer and the relaunch: once it has
//! begun, no way out still to commit its last step starts one (`ExitState::commit_final_action`).
//!
//! Whichever way a quit gets through, a downloaded update the application still holds (Windows,
//! `updater::holds_download`) is installed on the way out, as a confirmed quit installs it: the
//! exit is held back while the daemon stops and the installer starts (`on_exit_requested`). The
//! system ending the user session (sign-out, shutdown) is not a quit of the user's: Windows ends
//! the process with `RunEvent::Exit` and no `ExitRequested` before it, and `install_on_exit` leaves
//! the update alone then, to be offered again at the next start.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};

use crate::diagnostics::diagnostic;
use crate::sidecar::{
    daemon_stopped, mark_daemon_stopping, note_quit, skip_quit_note, stop_daemon,
    stop_daemon_at_session_end, wait_for_recoveries, StopKind,
};
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

/// How long the daemon is given to stop when the system ends the user's session before it is
/// killed. macOS waits for the application's answer to the logout, so a few seconds of it are
/// acceptable; on Windows the system gives the process about 5 seconds after the end-session notice
/// and ends it then, so this stays inside that.
#[cfg(target_os = "macos")]
const SESSION_END_WAIT: Duration = Duration::from_secs(5);
#[cfg(not(target_os = "macos"))]
const SESSION_END_WAIT: Duration = Duration::from_secs(4);
const _: () = assert!(SESSION_END_WAIT.as_secs() <= 5);

/// How long `RunEvent::Exit` waits for the recoveries under way to notice the quit and end the
/// daemon they were starting, so that the process does not end under a daemon half started.
const RECOVERY_EXIT_WAIT: Duration = Duration::from_secs(2);

/// Tracks the facts the exit flow needs across the gestures described in the module comment
/// above:
///
/// - `confirmed`: set by `confirm_quit` right before it calls `app.exit(0)`, and by
///   `quit_installing` right before its own `app.exit(0)` and the restart of `confirm_quit` right
///   before its relaunch, when no installer ended the process, each of which raises its own
///   `ExitRequested`. Without this flag that self-raised event would be prevented again and the
///   application would never actually terminate — just end up running headlessly with no window.
///   They set it again there because a quit made while they waited (through the menu, tray or
///   window) may have consumed the flag in `should_let_quit_through`, so their exit would otherwise
///   be asked about once more.
/// - `frontend_registered`: set once the webview has called `frontend_exit_heartbeat`, meaning a
///   confirmation dialog actually exists to show. Before that point (or if the page never finishes
///   loading at all), deferring to it would trap the user in a window nothing can close — the
///   daemon-failed-to-start screen, which never registers, relies on this, so the default is to let
///   the exit through rather than trap first and hope the frontend shows up.
/// - `last_prevented_at`: when the previous attempt was deferred, for the `FORCE_QUIT_WINDOW` check
///   above. Cleared by `frontend_exit_heartbeat` too: `useAppExit.ts`'s `request` re-invokes it on
///   every quit gesture the frontend actually receives — not only when the user cancels the dialog
///   — which is itself proof the frontend is alive and answering, so the next gesture should wait
///   for it rather than treat a prompt the frontend is still showing as one that wedged. A wedged
///   webview never reaches that call at all, so the debounce stays armed and the escape hatch below
///   still fires for it.
/// - `installing`: set while the way out stops the daemon and starts the installer of a held update
///   (`quit_installing`, and the restart of `confirm_quit`, which installs one where the platform
///   holds it). Every exit requested meanwhile through `RunEvent::ExitRequested` waits for it (a
///   quit through the menu, tray or window; the Dock's Quit, which `lifecycle/macos.rs` answers on
///   its own, is cancelled meanwhile (`way_out_under_way`), and a logout is not held), so none can
///   end the process before the installer has started, and a second restart or quit does not start
///   another; it is cleared before the exit or relaunch that follows when no installer ended the
///   process.
/// - `exit_let_through`: set when an exit was let through (`quit_let_through`:
///   `ExitRoute::Proceed`, or the Dock's Quit that `lifecycle/macos.rs` lets through). Every exit a
///   user or the application starts passes there before `RunEvent::Exit`; one that reaches
///   `RunEvent::Exit` without it is the system ending the session (`exit_kind`).
/// - `session_ended`: set once `end_session` has begun, which macOS reaches twice for one session
///   end (the delegate's answer, then `RunEvent::Exit`), so that the daemon is stopped only once.
///   It is also the priority of the session end over the installer and the relaunch: the ways out
///   that run on a blocking task (`confirm_quit`'s restart, `quit_installing`) check it
///   (`commit_final_action`) after the daemon is stopped and before they start either, and give up
///   once it is set. A way out that checks first commits and its installer or relaunch goes ahead,
///   which the session end, still stopping the daemon, cannot undo; a session end that begins
///   first stops it. What remains is the window between the check and the action.
///   `install_on_exit` needs no check: it runs on the main thread, where the session end is, so
///   the two never overlap.
#[derive(Default)]
pub struct ExitState {
    confirmed: AtomicBool,
    frontend_registered: AtomicBool,
    last_prevented_at: Mutex<Option<Instant>>,
    installing: AtomicBool,
    exit_let_through: AtomicBool,
    session_ended: AtomicBool,
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

    /// Marks the session as ended; true for the first call, which is to stop the daemon.
    fn begin_session_end(&self) -> bool {
        !self.session_ended.swap(true, Ordering::SeqCst)
    }

    /// Whether a way out may start its last step, the installer or the relaunch: false once
    /// `begin_session_end` has run, which takes priority, and the way out is then to give up.
    fn commit_final_action(&self) -> bool {
        !self.session_ended.load(Ordering::SeqCst)
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

/// Notes a plain quit natively, before the UI asks the daemon to shut down (`note_quit`): leaves
/// the quit record, writes the quit note and marks the daemon as stopping. The UI's shutdown
/// request alone would leave no intent behind if the daemon ended before `confirm_quit` was
/// reached; with this first, the daemon's next start still finds the quit. Idempotent, and no quit
/// during an update restart's planned stop.
#[tauri::command]
pub fn begin_quit(app: AppHandle) {
    note_quit(&app);
}

/// Marks the pending quit as confirmed and asks Tauri to exit. The `RunEvent::ExitRequested`
/// handler in `lib.rs` (`on_exit_requested`) sees `should_let_quit_through` consume the flag below
/// and lets this one through.
///
/// With `restart` the process is relaunched instead of just ending, which is how an installed
/// update takes effect (`updater/`). The UI asks for confirmation when the restart would interrupt
/// sessions: those the daemon's `handover_check` lists (whose holders the next daemon cannot
/// adopt), every live session and busy shell when compatibility could not be checked, and the
/// terminals of holders it cannot reach; it restarts without asking otherwise. The new instance's
/// daemon would find this one's instance lock held and fail to start, so the daemon is stopped
/// first, off the main thread because that waits, under the `installing` guard every exit shares,
/// with the planned stop (`StopKind::Planned`): the UI has asked for it already (its `planned_stop`
/// request), and the sessions are handed over to their process holders for the new instance's
/// daemon to adopt rather than interrupted. No quit note is written then (`skip_quit_note`). Where
/// the platform holds a downloaded update its installer then ends the process
/// (`updater::install_for_restart`), saving the window state first, and otherwise the relaunch goes
/// through the event loop like any exit, so `RunEvent::Exit` saves it on the way out. A plain quit
/// writes the quit note, and while a downloaded update waits (Windows) it stops the daemon with the
/// ordinary stop and installs the update without the relaunch (`quit_installing`).
#[tauri::command]
pub fn confirm_quit(app: AppHandle, state: tauri::State<ExitState>, restart: Option<bool>) {
    state.confirmed.store(true, Ordering::SeqCst);
    // Whatever the daemon's own end looks like from here, it is not to be started again.
    mark_daemon_stopping(&app);
    if restart == Some(true) {
        if !state.begin_installing() {
            return;
        }
        skip_quit_note(&app);
        tauri::async_runtime::spawn_blocking(move || {
            let state = app.state::<ExitState>();
            let installing = InstallingGuard(&state);
            stop_daemon(&app, StopKind::Planned);
            if !state.commit_final_action() {
                diagnostic!("the session is ending: the update restart gives up its relaunch");
                return;
            }
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
    note_quit(&app);
    if updater::holds_download(&app) {
        quit_installing(&app);
        return;
    }
    app.exit(0);
}

/// What becomes of an exit Tauri is about to make (`RunEvent::ExitRequested`).
#[derive(Debug, PartialEq)]
enum ExitRoute {
    /// The installer of a held update is being started: hold the exit back, as that ends the
    /// process.
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
/// repeated gesture of `FORCE_QUIT_WINDOW`, no registered frontend, a start that failed), so this
/// is where a held update is installed for them.
pub fn on_exit_requested(app: &AppHandle) -> bool {
    let installing = app.state::<ExitState>().installing.load(Ordering::SeqCst);
    match exit_route(
        installing,
        || should_let_quit_through(app),
        || updater::holds_download(app),
    ) {
        ExitRoute::Proceed => {
            quit_let_through(app);
            false
        }
        ExitRoute::Wait | ExitRoute::Defer => true,
        ExitRoute::Install => {
            note_quit(app);
            quit_installing(app);
            true
        }
    }
}

/// Records that an exit was let through and tells the daemon the application is quitting: the one
/// place a quit that no `confirm_quit` made is let through, for `on_exit_requested` and the Dock's
/// Quit that `application_should_terminate` (`lifecycle/macos.rs`) lets through. The note is not
/// written when the update restart is under way (`skip_quit_note`), whose relaunch passes here too.
pub fn quit_let_through(app: &AppHandle) {
    app.state::<ExitState>()
        .exit_let_through
        .store(true, Ordering::SeqCst);
    note_quit(app);
}

/// Ends the application with the held update installed on the way, without relaunching it: stops
/// the daemon (the ordinary stop), whose executable the installer replaces, off the main thread
/// because that waits, then starts the installer, which ends the process. One that could not be
/// started leaves an ordinary exit, already confirmed. Only the first call does anything; a quit
/// while it runs waits for it.
fn quit_installing(app: &AppHandle) {
    if !app.state::<ExitState>().begin_installing() {
        return;
    }
    mark_daemon_stopping(app);
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<ExitState>();
        let installing = InstallingGuard(&state);
        stop_daemon(&app, StopKind::Ordinary);
        if !state.commit_final_action() {
            diagnostic!("the session is ending: the quit gives up installing the update");
            return;
        }
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
    /// Leave it for the next start: the system ended the session, which is no quit of the user's.
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

/// What `RunEvent::Exit` is the end of.
#[derive(Debug, PartialEq)]
enum ExitKind {
    /// A quit of the user's or the application's own: the daemon is told so.
    Quit,
    /// The system ending the session, which reached `RunEvent::Exit` with no exit let through
    /// before it: the end is passed on to the daemon.
    SessionEnd,
}

fn exit_kind(exit_let_through: bool) -> ExitKind {
    if exit_let_through {
        ExitKind::Quit
    } else {
        ExitKind::SessionEnd
    }
}

/// The daemon's part in the system ending the user's session: its ordinary stop, quit record and
/// note included, with no wait for the UI and bounded by `SESSION_END_WAIT` in all, so the
/// previews are frozen before the system goes on; an update restart's planned stop under way is
/// ended with it, never a continuation of the restart. Marks the session ended first, which the
/// installer and relaunch of a way out still to start give way to (`ExitState::session_ended`); one
/// that has started them already is not stopped, only its daemon. Done once: macOS reaches it from
/// the delegate's answer and again from `RunEvent::Exit`, and the second call returns at once.
/// Blocks the calling thread, which is the main one on both platforms: the system waits for it
/// anyway.
pub fn end_session(app: &AppHandle) {
    if !app.state::<ExitState>().begin_session_end() {
        return;
    }
    diagnostic!("the session is ending: stopping the daemon");
    stop_daemon_at_session_end(app, SESSION_END_WAIT);
}

/// Whether a way out that has taken the `installing` guard is under way: an update restart, or a
/// quit that installs a held update. It ends the process itself, so an exit asked for meanwhile
/// (the macOS Dock's Quit, which `lifecycle/macos.rs` answers on its own) must not end it first.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn way_out_under_way(app: &AppHandle) -> bool {
    app.state::<ExitState>().installing.load(Ordering::SeqCst)
}

/// What `RunEvent::Exit` does: tells the daemon about a quit, passes a session end on to it, then
/// settles a held update (`install_on_exit`). This is where Windows sign-out and shutdown are met,
/// as the system ends the process with `RunEvent::Exit` and no `ExitRequested`.
pub fn on_exit(app: &AppHandle) {
    let let_through = app
        .state::<ExitState>()
        .exit_let_through
        .load(Ordering::SeqCst);
    match exit_kind(let_through) {
        ExitKind::Quit => {
            note_quit(app);
            // A recovery that was starting a daemon meanwhile sees the quit and ends it.
            wait_for_recoveries(app, RECOVERY_EXIT_WAIT);
        }
        ExitKind::SessionEnd => end_session(app),
    }
    install_on_exit(app, let_through);
}

/// Settles a held update at `RunEvent::Exit`. It is installed only for the narrow case of a
/// download finishing between a quit being let through and the exit; the system ending the session
/// (Windows sign-out or shutdown) installs nothing, and the next start finds the update again. The
/// process ends once the event returns, so the daemon is stopped and the installer started here, on
/// the main thread, which has nothing left to serve.
fn install_on_exit(app: &AppHandle, let_through: bool) {
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
            stop_daemon(app, StopKind::Ordinary);
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
    fn a_way_out_that_commits_before_the_session_ends_keeps_its_last_step() {
        let state = ExitState::default();
        assert!(state.commit_final_action());
        // The session end still stops the daemon, once.
        assert!(state.begin_session_end());
        assert!(!state.begin_session_end());
    }

    #[test]
    fn a_session_end_takes_priority_over_a_way_out_that_has_not_committed() {
        let state = ExitState::default();
        assert!(state.begin_session_end());
        assert!(!state.commit_final_action());
        assert!(
            !state.commit_final_action(),
            "every later way out gives up too"
        );
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
    fn an_exit_reaching_the_end_without_being_let_through_is_the_system_ending_the_session() {
        assert_eq!(exit_kind(false), ExitKind::SessionEnd);
        assert_eq!(exit_kind(true), ExitKind::Quit);
    }

    #[test]
    fn an_exit_with_nothing_held_does_nothing() {
        assert_eq!(exit_end(true, false), ExitEnd::Nothing);
        assert_eq!(exit_end(false, false), ExitEnd::Nothing);
    }
}
