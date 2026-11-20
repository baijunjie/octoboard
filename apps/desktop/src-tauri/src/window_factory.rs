//! Building the application's windows: the main window and a remote daemon's window, each on
//! `index.html` with the query that tells its page which daemon it shows (the main one's none) and
//! what it needs of the host (`window_query`), created hidden where its frame was last left
//! (`window_state`) and shown by its page once painted, or by the reveal safety net. Which window
//! shows which daemon is `daemon_window`'s.

use std::thread;
use std::time::Duration;

use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::daemon_window::{self, DaemonWindows, RemoteDaemon};
use crate::diagnostics::diagnostic;
use crate::remote::RemoteState;
use crate::sidecar::window_params;
use crate::{
    app_settings, background, chrome, host_facts, languages, launch_at_login, menu, page_isolation,
    percent_encode_query_value, recovery, remote, sidecar, window_state, MAIN_WINDOW_LABEL,
};

/// Upper bound on how long a window stays hidden (see `.visible(false)` in `build_window`) before
/// this safety net shows it regardless of what the frontend is doing. The UI reveals the window
/// itself (`background::reveal_main_window`, which keeps the main window hidden after a launch at
/// login that keeps the app in the background) well inside this, once it has pushed the native
/// theme (`packages/ui/src/main.tsx`, measured at 140-220 ms after the window is created); this
/// exists only for the paths that never reach that point at all — the webview failing to load the
/// bundle at all, say — where a window left hidden forever is strictly worse than one that briefly
/// shows through in the OS's own appearance. Four seconds is generous next to that measured reveal
/// without being so long that a genuinely stuck launch reads as a hung application rather than a
/// slow one. Not armed for the main window after a launch at login.
const REVEAL_SAFETY_NET: Duration = Duration::from_secs(4);

/// Opens the main window for the daemon as it is when the window is built. On the main thread.
pub fn open_main_window(
    app: &AppHandle,
    configuration: page_isolation::Configuration,
) -> tauri::Result<()> {
    let daemon_query = window_params(app);
    let query = window_query(app, &daemon_query, &host_facts::current_json());
    let window = build_window(
        app,
        MAIN_WINDOW_LABEL,
        &query,
        menu::APP_NAME,
        None,
        configuration,
    )?;
    // A recovery that finished while the window was being built found no window to point at the
    // restarted daemon, so the window is pointed at it here instead.
    if window_params(app) != daemon_query {
        if let Some(address) = sidecar::current_address(app) {
            recovery::point_window_at(app, &address);
        }
    }
    // Not after a launch at login: the UI's reveal can arrive late there (the system is busy
    // starting), and the net would show the whole window at a launch that may be meant to stay
    // hidden. Should the UI never load, the menu bar icon, a reopen of the app or a notification
    // still bring the window back. Peeked at, not taken: the reveal still needs to find it.
    if !launch_at_login::login_launch_pending() {
        arm_reveal_safety_net(app, window);
    }
    Ok(())
}

/// Opens a window of its own, labelled `label` (`daemon_window::register` took it), on `remote`, a
/// daemon other than the local one. On the main thread.
pub fn open_remote_window(app: &AppHandle, label: String, remote: RemoteDaemon) {
    let handle = app.clone();
    page_isolation::prepare(move |configuration| {
        let query = window_query(
            &handle,
            &remote_daemon_query(&remote),
            &host_facts::remote_window_json(),
        );
        let title = daemon_window::remote_title(menu::APP_NAME, &remote);
        match build_window(
            &handle,
            &label,
            &query,
            &title,
            Some(remote.identity.clone()),
            configuration,
        ) {
            Ok(window) => arm_reveal_safety_net(&handle, window),
            Err(err) => {
                diagnostic!("cannot open the window of {}: {err}", remote.name);
                window_lost(&handle, &label);
            }
        }
    });
}

/// Lets go of what a remote window labelled `label` that is gone, or never came to be, leaves
/// behind: its label, and the connection made for it.
pub fn window_lost(app: &AppHandle, label: &str) {
    if let_go(
        &app.state::<DaemonWindows>(),
        &app.state::<RemoteState>(),
        label,
    ) {
        remote::connections_changed(app);
    }
}

/// `window_lost` on the states themselves; whether a connection was held for the window.
fn let_go(windows: &DaemonWindows, remote: &RemoteState, label: &str) -> bool {
    windows.forget(label);
    remote.end(label)
}

/// What a remote window's query says of its daemon: where it is and the key it takes, and the
/// identity and name it is shown under.
fn remote_daemon_query(remote: &RemoteDaemon) -> String {
    format!(
        "port={}&key={}&{}",
        remote.port,
        remote.key,
        daemon_window::remote_params(remote)
    )
}

/// What every window's query carries beside `daemon_query`, which says where its daemon is (or,
/// for the flyout, what page it is): the host's facts as the window is given them (`host`), the
/// shell's settings and the system's preferred languages.
pub fn window_query(app: &AppHandle, daemon_query: &str, host_json: &str) -> String {
    let host = percent_encode_query_value(host_json);
    let settings = percent_encode_query_value(&app_settings::current_json(app));
    match languages::preferred_languages() {
        Some(languages) => format!(
            "{daemon_query}&host={host}&settings={settings}&languages={}",
            percent_encode_query_value(&languages)
        ),
        None => format!("{daemon_query}&host={host}&settings={settings}"),
    }
}

/// Builds a window labelled `label` on `index.html?{query}`, hidden, placed where the window of the
/// daemon `identity` names (`None` for the main window) was last left, and follows its frame from
/// then on.
fn build_window(
    app: &AppHandle,
    label: &str,
    query: &str,
    title: &str,
    identity: Option<String>,
    configuration: page_isolation::Configuration,
) -> tauri::Result<tauri::WebviewWindow> {
    let url = WebviewUrl::App(format!("index.html?{query}").into());
    // Decided before the window exists, from the saved frame and the displays connected now, so the
    // window is placed while it is still hidden and is never seen moving into place.
    let (displays, main_display) = window_state::displays(app);
    let initial = window_state::initial_window(
        window_state::load(app, identity.as_deref()).as_ref(),
        &displays,
        main_display,
        chrome::system_frame(),
    );

    let builder = WebviewWindowBuilder::new(app, label, url)
        .title(title)
        .inner_size(initial.size.0, initial.size.1)
        // 48 (the left rail) + 280 (sidebar) + 520 (the terminal pane's own floor) + 320 (the
        // report panel's own floor) = 1168: at this minimum, both panes already sit on their
        // floors with nothing left to give up, so neither can be squeezed past usability by a
        // narrower window. Height 600 gives the terminal about 30 rows, which is comfortably
        // usable.
        .min_inner_size(window_state::MIN_SIZE.0, window_state::MIN_SIZE.1)
        // Created hidden so AppKit never paints the window in the OS's own appearance before the
        // webview has painted anything themed — with no window on screen yet, there is nothing
        // for it to paint prematurely. `packages/ui/src/main.tsx` shows it once that has
        // happened, through `background::reveal_main_window`; `REVEAL_SAFETY_NET` is the backstop
        // for every path that does not reach that call (not armed for the main window after a
        // launch at login).
        .visible(false);
    let builder = match initial.position {
        Some((x, y)) => builder.position(x, y),
        None => builder,
    };
    let builder = page_isolation::apply(builder, configuration);
    // A remote daemon's window must see a dropped file as the page's own `drop` event with `File`
    // objects, since the page reads the bytes itself to upload them to the daemon. The native
    // handler would swallow that event and give only local paths, which mean nothing on the remote
    // host. The main window keeps it: its daemon runs here, so the paths it reports are the ones
    // to paste.
    let builder = if identity.is_some() {
        builder.disable_drag_drop_handler()
    } else {
        builder
    };
    let builder = chrome::decorate(builder);
    // Every load of the main window's page is counted, whoever asks for it (the shell pointing it
    // at a restarted daemon, the page reloading itself): what a connection had under way from the
    // page is gone with the page.
    let builder = if label == MAIN_WINDOW_LABEL {
        builder.on_page_load(|webview, payload| {
            if payload.event() == PageLoadEvent::Started {
                remote::page::page_load_started(webview.app_handle());
            }
        })
    } else {
        builder
    };

    let window = builder.build()?;
    page_isolation::install(&window);
    chrome::install(&window);
    menu::install_window(&window);
    // Not the builder's `.maximized`. AppKit first puts a new window's frame on the main display,
    // and the builder's position is applied afterwards, queued on the main thread; tao queues a
    // builder-requested zoom while creating the window, ahead of that move, so the zoom would
    // happen on the main display and record that frame as the one to un-zoom to. Asked for here,
    // the zoom is queued behind the move and happens on the display the window belongs on.
    if initial.maximized {
        if let Err(err) = window.maximize() {
            diagnostic!("cannot maximize the window: {err}");
        }
    }
    window_state::track(&window, &initial, identity);
    Ok(window)
}

/// Shows `window` after `REVEAL_SAFETY_NET` whatever its page is doing, unless it is meant to stay
/// hidden by then (`background::safety_net_shows`).
fn arm_reveal_safety_net(app: &AppHandle, window: tauri::WebviewWindow) {
    // `WebviewWindow::show` dispatches onto the window's own event loop internally, so calling it
    // from this background thread rather than the main one is safe, so a plain `thread::sleep`
    // here needs neither `tauri::async_runtime` nor a direct `tokio` dependency.
    let app = app.clone();
    thread::spawn(move || {
        thread::sleep(REVEAL_SAFETY_NET);
        if background::safety_net_shows(&app, window.label()) {
            let _ = window.show();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_remote_window_s_query_says_where_its_daemon_is_and_what_to_show_it_as() {
        let remote = RemoteDaemon {
            identity: "ssh:me@host:22".into(),
            name: "Build & Test".into(),
            port: 4100,
            key: "0a1b".into(),
        };
        assert_eq!(
            remote_daemon_query(&remote),
            "port=4100&key=0a1b&daemon=ssh%3Ame%40host%3A22&remote=Build%20%26%20Test"
        );
    }

    #[test]
    fn a_remote_window_lost_before_it_was_built_leaves_nothing_behind() {
        let windows = DaemonWindows::default();
        let remote = RemoteState::default();
        let label = windows.register();
        let stopped = remote.hold_for_test(&label);
        assert!(let_go(&windows, &remote, &label));
        // The connection is ended and its port let go.
        assert!(*stopped.borrow());
        assert!(!remote.end(&label));
        // A window lost before any connection was made for it leaves nothing either.
        let other = windows.register();
        assert!(!let_go(&windows, &remote, &other));
    }
}
