//! Running in the background: with the user's Settings switch on (kept in `settings.json`, see
//! `app_settings`, which the shell reads at startup, so it holds from the first moment of a launch
//! and not only once the page has loaded), the window's close button hides the window and takes the
//! app out of the Dock instead of quitting. Cmd+W and the Window menu's Close Window item are the
//! same gesture: `menu/macos.rs` builds that item with Tauri's predefined `close_window_with_text`,
//! which carries ⌘W and arrives as the same `CloseRequested` as the button. The app stays reachable
//! through the menu bar icon (`tray/`), the Dock icon or a reopen from the Finder or Spotlight, all
//! of which bring the window back through `show_main_window`. The UI's `bringToFront` does too,
//! through `bring_to_front`, for the exit confirmation of a quit that came while the window was
//! hidden and for the notice that the daemon exited. Activating the app while the window is hidden
//! by a close, such as by clicking a system notification, brings it back too. With the switch off,
//! the default, all three are an ordinary quit (`exit::request_quit_from_window`), confirmation
//! included; and with no daemon left running there is nothing to keep in the background, so they
//! quit at once whatever the switch says. A launch at login with the switch on starts in this
//! state, the window never shown (`start_in_background`). What the platform adds to hiding and
//! showing the window (the Dock, fullscreen, activation on macOS) is in `lifecycle`.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use tauri::{AppHandle, CloseRequestApi, Manager, WebviewWindow, Window};

use crate::daemon_window::{another_visible, is_main, last_focused_or_main};
use crate::window_state;

use crate::exit::request_quit_from_window;
use crate::launch_at_login::take_login_launch;
use crate::lifecycle::{before_show, hide_after_leaving_fullscreen, window_hidden};
use crate::sidecar::daemon_stopped;
use crate::MAIN_WINDOW_LABEL;

#[derive(Default)]
pub struct BackgroundState {
    /// Bumped by every show of the window, so a hide that waited for something (a fullscreen
    /// transition, the Dock's hold-off) can tell the window was shown again in the meantime and
    /// leave it alone.
    pub(crate) shown: AtomicU64,
    /// Whether the window is hidden by the close button, as opposed to not shown yet or hidden by
    /// the system (Cmd+H): the state in which the app is out of the Dock and activating it brings
    /// the window back.
    hidden_by_close: AtomicBool,
    /// Whether the close button hides the window rather than quitting: the user's Settings choice,
    /// set from `app_settings` when the shell starts and on every change the UI makes.
    run_in_background: AtomicBool,
    /// Whether the main window is meant to stay hidden until something shows it (`show_window`):
    /// hidden by a close, or left hidden once the remote windows the last quit left came back. A
    /// reveal from its page, a page that loads again included, and the reveal safety net leave it
    /// hidden then.
    main_kept_hidden: AtomicBool,
}

/// Whether the window is hidden by the close button; false until the first close.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn hidden_by_close(app: &AppHandle) -> bool {
    app.state::<BackgroundState>()
        .hidden_by_close
        .load(Ordering::SeqCst)
}

/// Sets whether the main window's close button keeps the app running in the background, which is
/// the Settings switch; see `handle_close_request`.
pub fn set_run_in_background(app: &AppHandle, enabled: bool) {
    app.state::<BackgroundState>()
        .run_in_background
        .store(enabled, Ordering::SeqCst);
}

/// Answers a window's close request, whether it came from the close button, Cmd+W or the Window
/// menu's Close Window item.
///
/// A remote window (`daemon_window`) closes for good, which disconnects it and ends nothing on its
/// daemon. The main window is only ever hidden: while another window is on screen it is simply
/// hidden. Once the last window on screen is closed, the application's closing rule applies: it
/// goes into the background (the main window hidden and the app out of the Dock) if the user chose
/// to run in the background and there is a daemon to keep running, and quits otherwise, through the
/// same confirmation as any quit while a session is live.
///
/// A remote window whose close quits the application stays as it is while the quit is confirmed: it
/// is left on screen when the quit is cancelled, and counted as open, to come back at the next
/// launch, when it goes ahead.
pub fn handle_close_request(window: &Window, api: &CloseRequestApi) {
    let app = window.app_handle();
    let main = is_main(window.label());
    let state = app.state::<BackgroundState>();
    let decision = closing_decision(
        main,
        another_visible(app, window.label()),
        state.run_in_background.load(Ordering::SeqCst),
        daemon_stopped(app),
    );
    if keeps_window(main, decision) {
        api.prevent_close();
    } else if let Some(webview) = app.get_webview_window(window.label()) {
        window_state::forget(&webview);
    }
    match decision {
        Closing::HideMain => hide_main_window(window),
        Closing::EnterBackground => enter_background(app, state.shown.load(Ordering::SeqCst)),
        // Quits at once when the daemon is stopped; `exit.rs` owns that rule.
        Closing::Quit => request_quit_from_window(app),
        Closing::ShowMain => show_main_window(app),
        Closing::Nothing => {}
    }
}

/// What closing a window, or disconnecting one, leads to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Closing {
    /// The main window is hidden; with no other window on screen, the application goes into the
    /// background (`hide`).
    HideMain,
    /// No window is left on screen, and the application waits in the background.
    EnterBackground,
    Quit,
    /// The main window is brought back.
    ShowMain,
    /// The window goes, and nothing else changes: another is on screen.
    Nothing,
}

/// The closing rule: closing the main window while another window is on screen only hides it; the
/// last window on screen quits the application, unless the user keeps it running in the background
/// (and there is a daemon to keep running), when the main window is hidden or, for a remote window,
/// the application goes into the background. Either closed while another window is on screen just
/// goes.
pub fn closing_decision(
    is_main: bool,
    others_on_screen: bool,
    run_in_background: bool,
    daemon_stopped: bool,
) -> Closing {
    if others_on_screen {
        return if is_main {
            Closing::HideMain
        } else {
            Closing::Nothing
        };
    }
    if !run_in_background || daemon_stopped {
        return Closing::Quit;
    }
    if is_main {
        Closing::HideMain
    } else {
        Closing::EnterBackground
    }
}

/// Whether a window whose close leads to `decision` stays (its close prevented): the main window
/// always, as it is only ever closed by the application exiting and a quit the confirmation defers
/// must leave it on screen, and a remote window whose close quits, for the same reason and so that
/// a quit that goes ahead remembers it as open.
pub fn keeps_window(is_main: bool, decision: Closing) -> bool {
    is_main || decision == Closing::Quit
}

/// What disconnecting a remote window leads to: never a quit. With no other window on screen, the
/// application goes into the background when the user keeps it running there (and there is a daemon
/// to keep running), and the main window comes back otherwise.
pub fn disconnect_decision(
    others_on_screen: bool,
    run_in_background: bool,
    daemon_stopped: bool,
) -> Closing {
    if others_on_screen {
        Closing::Nothing
    } else if run_in_background && !daemon_stopped {
        Closing::EnterBackground
    } else {
        Closing::ShowMain
    }
}

/// Closes the remote window labelled `label` for good, which disconnects it, as its tab's
/// Disconnect in the menu bar icon's menu asks: the application runs on, so the closing rule that
/// applies to the last window on screen does not. With no other window left on screen, the
/// application goes into the background when the user chose to run in the background (and there is
/// a daemon to keep running), and otherwise the main window is brought back.
pub fn disconnect_window(app: &AppHandle, label: &str) {
    if is_main(label) {
        return;
    }
    let Some(window) = app.get_webview_window(label) else {
        return;
    };
    window_state::forget(&window);
    let others_on_screen = another_visible(app, label);
    // Destroyed rather than closed, which would ask the closing rule.
    let _ = window.destroy();
    let state = app.state::<BackgroundState>();
    match disconnect_decision(
        others_on_screen,
        state.run_in_background.load(Ordering::SeqCst),
        daemon_stopped(app),
    ) {
        Closing::EnterBackground => enter_background(app, state.shown.load(Ordering::SeqCst)),
        Closing::ShowMain => show_main_window(app),
        Closing::HideMain | Closing::Quit | Closing::Nothing => {}
    }
}

fn hide_main_window(window: &Window) {
    let shown = window
        .app_handle()
        .state::<BackgroundState>()
        .shown
        .load(Ordering::SeqCst);
    if hide_after_leaving_fullscreen(window, shown) {
        return;
    }
    hide(window, shown);
}

/// Hides the window unless it has been shown again since `shown` was read, and puts the application
/// in the background when no other window is left on screen.
pub(crate) fn hide(window: &Window, shown: u64) {
    let app = window.app_handle();
    let state = app.state::<BackgroundState>();
    if state.shown.load(Ordering::SeqCst) != shown {
        return;
    }
    let _ = window.hide();
    if is_main(window.label()) {
        state.main_kept_hidden.store(true, Ordering::SeqCst);
    }
    if another_visible(app, window.label()) {
        return;
    }
    enter_background(app, shown);
}

/// The application is in the background: no window is on screen, and activating it brings one back.
fn enter_background(app: &AppHandle, shown: u64) {
    app.state::<BackgroundState>()
        .hidden_by_close
        .store(true, Ordering::SeqCst);
    window_hidden(app, shown);
}

/// Leaves the window, which the shell created hidden and has not shown, hidden for good, in the
/// state a window closed into the background is in: the app is out of the Dock and waits behind its
/// menu bar icon, and a reopen or the menu bar icon brings the window back. Where the app was
/// launched at login with the user's choice to keep running in the background
/// (`reveal_main_window`).
pub fn start_in_background(app: &AppHandle) {
    let state = app.state::<BackgroundState>();
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return;
    };
    let shown = state.shown.load(Ordering::SeqCst);
    hide(&window.as_ref().window(), shown);
}

/// Leaves the main window, which the shell created hidden and has not shown, hidden until something
/// shows it (`show_window`): its page's reveal and the reveal safety net leave it alone. Where the
/// remote windows the last quit left came back for a remote-only user (`remote::begin_restore`);
/// unlike `start_in_background`, the application stays in the Dock, its remote windows on screen.
pub fn keep_main_hidden(app: &AppHandle) {
    app.state::<BackgroundState>()
        .main_kept_hidden
        .store(true, Ordering::SeqCst);
}

/// Whether the reveal safety net shows the window labelled `label` (see `safety_net_decision`).
pub fn safety_net_shows(app: &AppHandle, label: &str) -> bool {
    let state = app.state::<BackgroundState>();
    safety_net_decision(
        is_main(label),
        state.hidden_by_close.load(Ordering::SeqCst),
        crate::remote::holding_main(app),
        state.main_kept_hidden.load(Ordering::SeqCst),
    )
}

/// Whether the reveal safety net shows a window: not while the application is closed into the
/// background (a window shown then would come back without the Dock icon), and for the main window
/// not while it is held hidden for the remote windows to come back (`main_held`), nor once it is
/// meant to stay hidden (`main_kept_hidden`).
pub fn safety_net_decision(
    is_main: bool,
    hidden_by_close: bool,
    main_held: bool,
    main_kept_hidden: bool,
) -> bool {
    !hidden_by_close && !(is_main && (main_held || main_kept_hidden))
}

/// What the main window's page asking to reveal it leads to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainReveal {
    /// The launch at login keeps the application in the background: the window stays hidden for
    /// good (`start_in_background`).
    StartInBackground,
    /// The window is held while the remote windows come back, which decide
    /// (`remote::begin_restore`).
    Defer,
    /// It is meant to stay hidden (`keep_main_hidden`, or a close).
    StayHidden,
    Show,
}

/// The decision for a reveal of the main window, from whether this is the launch at login that
/// keeps the application in the background, whether the window is held for the remote windows, and
/// whether it is meant to stay hidden.
pub fn main_reveal(background_login: bool, held: bool, kept_hidden: bool) -> MainReveal {
    if background_login {
        MainReveal::StartInBackground
    } else if held {
        MainReveal::Defer
    } else if kept_hidden {
        MainReveal::StayHidden
    } else {
        MainReveal::Show
    }
}

/// Whether this launch is one at login that keeps the application in the background, as
/// `reveal_main_window` will find it; read before the window reveals.
pub fn starts_in_background_at_login(app: &AppHandle) -> bool {
    crate::launch_at_login::login_launch_pending()
        && app
            .state::<BackgroundState>()
            .run_in_background
            .load(Ordering::SeqCst)
}

/// Shows the window the shell created hidden, once the UI has painted it, except after a launch at
/// login while the user keeps the app running in the background (the shell's own
/// `run_in_background`): there the window stays hidden and the app waits behind its menu bar icon,
/// as after the window is closed into the background. Only the first reveal of a process can be
/// that launch.
///
/// A remote window, which has no launch at login of its own, is simply shown.
#[tauri::command]
pub fn reveal_main_window(app: AppHandle, webview_window: WebviewWindow) -> Result<(), String> {
    if !is_main(webview_window.label()) {
        show_window(&app, &webview_window);
        return Ok(());
    }
    let state = app.state::<BackgroundState>();
    let run_in_background = state.run_in_background.load(Ordering::SeqCst);
    // Taken whatever follows: only the first reveal of a process can be the launch at login's.
    let background_login = take_login_launch() && run_in_background;
    match main_reveal(
        background_login,
        crate::remote::holding_main(&app),
        state.main_kept_hidden.load(Ordering::SeqCst),
    ) {
        MainReveal::StartInBackground => start_in_background(&app),
        // Held hidden while the remote windows the last quit left come back
        // (`remote::begin_restore`), which shows it, or leaves it hidden, once they have.
        MainReveal::Defer | MainReveal::StayHidden => {}
        MainReveal::Show => {
            let window = app
                .get_webview_window(MAIN_WINDOW_LABEL)
                .ok_or_else(|| "there is no main window".to_string())?;
            window.show().map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

/// `show_window` for the UI, on the window that asks: the exit confirmation uses it to raise a
/// window that may be hidden. Carries nothing.
#[tauri::command]
pub fn bring_to_front(app: AppHandle, webview_window: WebviewWindow) {
    show_window(&app, &webview_window);
}

/// Brings the main window to the front; see `show_window`.
pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        show_window(app, &window);
    }
}

/// Brings the window the user focused last to the front, else the main one: what reopening or
/// activating the application shows.
pub fn show_last_focused_window(app: &AppHandle) {
    if let Some(window) = last_focused_or_main(app) {
        show_window(app, &window);
    }
}

/// Brings `window` to the front, restoring it first if it was hidden or minimized, and puts the
/// app back in the Dock if it was in the background.
pub fn show_window(app: &AppHandle, window: &WebviewWindow) {
    let state = app.state::<BackgroundState>();
    state.shown.fetch_add(1, Ordering::SeqCst);
    state.hidden_by_close.store(false, Ordering::SeqCst);
    if is_main(window.label()) {
        state.main_kept_hidden.store(false, Ordering::SeqCst);
    }
    before_show(app, window);
    let _ = window.unminimize();
    let _ = window.show();
    // Only activates the app for a window that is visible and not minimized, so it comes last.
    let _ = window.set_focus();
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: bool = true;
    const REMOTE: bool = false;

    #[test]
    fn closing_the_main_window_while_a_remote_one_is_on_screen_only_hides_it() {
        for run_in_background in [false, true] {
            for daemon_stopped in [false, true] {
                assert_eq!(
                    closing_decision(MAIN, true, run_in_background, daemon_stopped),
                    Closing::HideMain
                );
            }
        }
    }

    #[test]
    fn a_remote_window_closed_while_another_is_on_screen_just_goes() {
        for run_in_background in [false, true] {
            assert_eq!(
                closing_decision(REMOTE, true, run_in_background, false),
                Closing::Nothing
            );
        }
    }

    #[test]
    fn the_last_window_on_screen_applies_the_closing_rule() {
        // Kept running in the background, with a daemon to keep running.
        assert_eq!(
            closing_decision(MAIN, false, true, false),
            Closing::HideMain
        );
        assert_eq!(
            closing_decision(REMOTE, false, true, false),
            Closing::EnterBackground
        );
        // Otherwise, whichever window it is, the application quits.
        for window in [MAIN, REMOTE] {
            assert_eq!(closing_decision(window, false, false, false), Closing::Quit);
            assert_eq!(closing_decision(window, false, true, true), Closing::Quit);
            assert_eq!(closing_decision(window, false, false, true), Closing::Quit);
        }
    }

    #[test]
    fn disconnect_never_quits() {
        for others_on_screen in [false, true] {
            for run_in_background in [false, true] {
                for daemon_stopped in [false, true] {
                    assert_ne!(
                        disconnect_decision(others_on_screen, run_in_background, daemon_stopped),
                        Closing::Quit
                    );
                }
            }
        }
        assert_eq!(disconnect_decision(true, false, false), Closing::Nothing);
        assert_eq!(
            disconnect_decision(false, true, false),
            Closing::EnterBackground
        );
        // With nothing left on screen and nothing to run in the background, the main window comes
        // back rather than the application quitting.
        assert_eq!(disconnect_decision(false, false, false), Closing::ShowMain);
        assert_eq!(disconnect_decision(false, true, true), Closing::ShowMain);
    }

    #[test]
    fn a_remote_window_whose_close_quits_stays_until_the_quit_goes_ahead() {
        // Its close is prevented, so a cancelled quit leaves it as it was and one that goes ahead
        // finds it open.
        let quitting = closing_decision(REMOTE, false, false, false);
        assert_eq!(quitting, Closing::Quit);
        assert!(keeps_window(REMOTE, quitting));
        // Any other close lets it go.
        for decision in [Closing::Nothing, Closing::EnterBackground] {
            assert!(!keeps_window(REMOTE, decision));
        }
        // The main window is only ever hidden.
        for decision in [Closing::HideMain, Closing::Quit] {
            assert!(keeps_window(MAIN, decision));
        }
    }

    #[test]
    fn a_main_window_left_hidden_after_the_restore_stays_hidden_through_its_reveal() {
        assert_eq!(main_reveal(false, false, true), MainReveal::StayHidden);
        assert_eq!(main_reveal(false, true, false), MainReveal::Defer);
        assert_eq!(main_reveal(false, false, false), MainReveal::Show);
        // The launch at login into the background decides first.
        assert_eq!(main_reveal(true, true, true), MainReveal::StartInBackground);
    }

    #[test]
    fn the_safety_net_leaves_a_main_window_meant_to_stay_hidden_alone() {
        // The main window: held, or kept hidden, it is not shown.
        assert!(!safety_net_decision(MAIN, false, true, false));
        assert!(!safety_net_decision(MAIN, false, false, true));
        assert!(safety_net_decision(MAIN, false, false, false));
        // A remote window is shown while the main one is held or kept hidden.
        assert!(safety_net_decision(REMOTE, false, true, true));
        // Nothing is, while the application is closed into the background.
        assert!(!safety_net_decision(REMOTE, true, false, false));
        assert!(!safety_net_decision(MAIN, true, false, false));
    }
}
