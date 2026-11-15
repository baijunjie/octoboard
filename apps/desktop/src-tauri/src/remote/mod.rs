//! Connecting to a daemon on another machine over SSH and showing it in a window of its own.
//!
//! The application makes the SSH connection itself (`ssh`), finds `octoboardd` on the remote
//! machine and reads the daemon's owner key, protocol generation and version through
//! `octoboardd address --json` (`octoboardd`), refuses a daemon of another generation before any
//! window opens, and then opens the window (`daemon_window`) on a loopback port of its own whose
//! connections are each carried by a run of `octoboardd proxy` over that connection (`tunnel`).
//! The pipeline is `connect`'s. The remote daemon never listens beyond its own loopback and owner
//! endpoint; nothing here asks anything of it beyond carrying connections, so closing the window or
//! quitting the application ends the SSH connection and nothing on the remote daemon.
//!
//! After a first connection authenticated with the remote user's own password or key, the
//! application installs a key of this device's own for the remote user and connects with it from
//! then on (`device_key`), its private key kept in the system's credential store (`credentials`)
//! for as long as the entry names the same destination.
//!
//! The owner reaches a remote daemon from a remote window on the connect page the application
//! provides (`open_connect_page`, from New Remote Window), which lists, adds and connects to the
//! saved remote daemons and becomes the window of the one it connects to; or directly from the
//! Remote menu, the Dock menu, the windows' system menus on Windows (`menu::system_menu`) and the
//! jump list (`actions`). The menu bar icon's menu lists the connected ones, and offers no way to
//! connect. Every window may use the commands (`permissions/remote-daemons.toml`), and none of them
//! returns a secret: a password or passphrase goes in, the browser's address with the owner key is
//! opened by the shell itself (`browser`), and device keys stay in the credential store. The local
//! window's rail shows the connected remote daemons, kept up to date through `CHANGED_EVENT` and
//! `remote_daemons_status`. The types the UI exchanges with these commands are `outcome`'s.

mod actions;
mod browser;
mod connect;
mod credentials;
mod device_key;
mod entries;
mod octoboardd;
mod outcome;
mod restore;
mod ssh;
mod tunnel;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::watch;

use crate::config_dir::config_path;
use crate::diagnostics::diagnostic;
pub use actions::{
    handle_menu_item, launch_action, menu_entries, open_connect_page, run_menu_action, MenuAction,
    MAIN_WINDOW_ARGUMENT, NEW_WINDOW_ARGUMENT, OPEN_ARGUMENT, OPEN_ITEM_PREFIX,
};
// The macOS Remote menu's and the Dock menu's item ids.
#[cfg(all(test, target_os = "macos"))]
pub use actions::menu_action;
#[cfg(target_os = "macos")]
pub use actions::{MAIN_WINDOW_ITEM_ID, NEW_WINDOW_ITEM_ID};
pub use entries::{Entry, EntryInput};
pub use outcome::{ConnectOutcome, ConnectRequest, ConnectionState, Status, ViaName};
pub use restore::{
    begin as begin_restore, holding_main, remember as remember_windows, RestoreState,
};

/// What the application keeps of the remote windows it has open.
#[derive(Default)]
pub struct RemoteState {
    /// Serializes the reads and writes of the entries' file.
    entries: Mutex<()>,
    /// The connection of each remote window, by the window's label.
    windows: Mutex<HashMap<String, Live>>,
    /// The daemons a connection is being made to, by identity, for a second request to wait on.
    connecting: connect::Connecting,
}

struct Live {
    /// The identity of the daemon (`Entry::identity`), which one window at most shows.
    identity: String,
    entry_id: String,
    /// The entry's name, which the menu bar icon's menu shows the daemon by.
    name: String,
    /// When the connection was made, as a rank among the others (`NEXT_ORDER`): the menu bar icon's
    /// menu lists the connected daemons in the order they were connected in, and a quit remembers
    /// the windows in that order.
    order: u64,
    stop: watch::Sender<bool>,
    /// Whether its SSH connection is up.
    connected: watch::Receiver<bool>,
    via: Via,
}

/// The rank the next connection is given (`Live::order`).
static NEXT_ORDER: AtomicU64 = AtomicU64::new(0);

/// The rank of a connection made now.
fn next_order() -> u64 {
    NEXT_ORDER.fetch_add(1, Ordering::SeqCst)
}

impl RemoteState {
    fn windows(&self) -> std::sync::MutexGuard<'_, HashMap<String, Live>> {
        self.windows.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Ends the connection kept under `label`, which is let go: its port and its SSH connection
    /// end. Whether there was one.
    pub(crate) fn end(&self, label: &str) -> bool {
        match self.windows().remove(label) {
            Some(live) => {
                let _ = live.stop.send(true);
                true
            }
            None => false,
        }
    }

    /// Holds a connection under `label` for a test, and gives what says it was stopped.
    #[cfg(test)]
    pub(crate) fn hold_for_test(&self, label: &str) -> watch::Receiver<bool> {
        let (held, stopped) = live(label, label, next_order(), Via::Window);
        self.windows().insert(label.to_owned(), held);
        stopped
    }

    /// The entries with a window of the application open, in the order their connections were made.
    fn window_entries(&self) -> Vec<String> {
        let mut open: Vec<(u64, String)> = self
            .windows()
            .values()
            .filter(|live| matches!(live.via, Via::Window))
            .map(|live| (live.order, live.entry_id.clone()))
            .collect();
        open.sort();
        open.into_iter().map(|(_, id)| id).collect()
    }
}

/// A remote daemon the application holds a connection to, as the menu bar icon's menu lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectedDaemon {
    /// The label of its window, or for one held for the browser the key its connection is kept
    /// under (no window has it).
    pub key: String,
    pub entry_id: String,
    pub name: String,
    /// Whether it is shown in the system's browser rather than in a window of the application.
    pub in_browser: bool,
}

/// The remote daemons the application holds a connection to, in the order they were connected in.
pub fn connected_daemons(app: &AppHandle) -> Vec<ConnectedDaemon> {
    let mut held: Vec<(u64, ConnectedDaemon)> = app
        .state::<RemoteState>()
        .windows()
        .iter()
        .map(|(key, live)| {
            (
                live.order,
                ConnectedDaemon {
                    key: key.clone(),
                    entry_id: live.entry_id.clone(),
                    name: live.name.clone(),
                    in_browser: matches!(live.via, Via::Browser { .. }),
                },
            )
        })
        .collect();
    held.sort_by_key(|(order, _)| *order);
    held.into_iter().map(|(_, daemon)| daemon).collect()
}

/// What a held connection is shown in.
#[derive(Clone, Debug)]
enum Via {
    /// A window of the application, the connection kept under the window's label.
    Window,
    /// The system's browser, at this address on the connection's port with the owner key: the
    /// remote daemon's own UI, for a daemon of another protocol generation, which no window of the
    /// application can show. Kept under `BROWSER_LABEL_PREFIX` and the entry's id.
    Browser { url: String },
}

/// What the key of a connection held for the browser starts with; no window has such a label.
const BROWSER_LABEL_PREFIX: &str = "browser-";

/// The event every window is sent when the remote daemons, or the state of their connections, may
/// have changed: the local window's rail and any connect page read them again.
pub const CHANGED_EVENT: &str = "remote-daemons-changed";

/// The saved remote daemons changed: every window is told, and the native menus that list them
/// (the menu bar and the Dock menu, the jump list and the system menus) are rebuilt.
fn entries_changed(app: &AppHandle) {
    let _ = app.emit(CHANGED_EVENT, ());
    crate::menu::remote_daemons_changed(app);
}

/// A connection was made, ended, dropped or made again: every window is told, and the menu bar
/// icon's menu, which lists the connected daemons, is rebuilt. The saved daemons are not read.
pub(crate) fn connections_changed(app: &AppHandle) {
    let _ = app.emit(CHANGED_EVENT, ());
    crate::tray::daemons_changed(app);
}

/// Opens a new remote window on the connect page.
#[tauri::command]
pub fn remote_window_new(app: AppHandle) {
    open_connect_page(&app, None);
}

/// Opens, or brings forward, the window of the saved remote daemon `id`; see `actions::open_entry`.
#[tauri::command]
pub fn remote_window_open(app: AppHandle, id: String) {
    actions::open_entry(&app, id);
}

/// Shows and focuses the local daemon's window.
#[tauri::command]
pub fn main_window_show(app: AppHandle) {
    actions::show_main_window(&app);
}

fn entries_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    config_path(app, entries::FILE_NAME).map_err(|err| err.to_string())
}

/// The remote daemons the owner has added.
#[tauri::command]
pub fn remote_daemons_list(app: AppHandle) -> Result<Vec<Entry>, String> {
    let state = app.state::<RemoteState>();
    let _lock = state.entries.lock().unwrap_or_else(|e| e.into_inner());
    Ok(entries::load(&entries_path(&app)?))
}

/// Saves a remote daemon, new or changed, as the dialog shows it: every field is taken as given,
/// the pinned host key fingerprint included, so a dialog that shows the pin sends it back and one
/// that sends `null` removes it. The UI keeps the pin when the owner pastes nothing over it.
#[tauri::command]
pub fn remote_daemon_save(app: AppHandle, entry: EntryInput) -> Result<Entry, String> {
    let saved = save_entry(&app, entry)?;
    // Outside the entries' lock: rebuilding the menus reads them again, on the main thread, which
    // may be this one.
    entries_changed(&app);
    Ok(saved)
}

fn save_entry(app: &AppHandle, entry: EntryInput) -> Result<Entry, String> {
    let state = app.state::<RemoteState>();
    let _lock = state.entries.lock().unwrap_or_else(|e| e.into_inner());
    let app = app.clone();
    let path = entries_path(&app)?;
    let mut all = entries::load(&path);
    let previous = entry
        .id
        .as_ref()
        .and_then(|id| all.iter().find(|existing| &existing.id == id).cloned());
    let saved = entries::upsert(&mut all, entry, entries::new_id)?;
    entries::save(&path, &all).map_err(|err| err.to_string())?;
    if previous.is_some_and(|previous| saved.outlives_device_key_of(&previous)) {
        device_key::forget(credentials::system(), &key_service(&app), &saved.id);
    }
    Ok(saved)
}

/// Removes a remote daemon and its device key; a window open on it stays until it is closed.
#[tauri::command]
pub fn remote_daemon_remove(app: AppHandle, id: String) -> Result<(), String> {
    {
        let state = app.state::<RemoteState>();
        let _lock = state.entries.lock().unwrap_or_else(|e| e.into_inner());
        let path = entries_path(&app)?;
        let mut all = entries::load(&path);
        all.retain(|entry| entry.id != id);
        entries::save(&path, &all).map_err(|err| err.to_string())?;
        device_key::forget(credentials::system(), &key_service(&app), &id);
    }
    // Outside the entries' lock, as `remote_daemon_save` says.
    entries_changed(&app);
    Ok(())
}

/// The credential store service the device keys of this application are kept under.
fn key_service(app: &AppHandle) -> String {
    credentials::service(&app.config().identifier)
}

/// Switches the entry `id` to the device key installed for `identity`.
fn use_device_key(app: &AppHandle, id: &str, identity: &str) {
    let state = app.state::<RemoteState>();
    let _lock = state.entries.lock().unwrap_or_else(|e| e.into_inner());
    let Ok(path) = entries_path(app) else {
        return;
    };
    let mut all = entries::load(&path);
    if entries::use_device_key(&mut all, id, identity) {
        if let Err(err) = entries::save(&path, &all) {
            diagnostic!("cannot switch {id} to its device key: {err}");
        }
    }
}

/// Pins `fingerprint` for the entry `id` once a first connection confirmed by the owner has
/// authenticated.
fn pin(app: &AppHandle, id: &str, fingerprint: &str) {
    let state = app.state::<RemoteState>();
    let _lock = state.entries.lock().unwrap_or_else(|e| e.into_inner());
    let Ok(path) = entries_path(app) else {
        return;
    };
    let mut all = entries::load(&path);
    if entries::pin(&mut all, id, fingerprint) {
        if let Err(err) = entries::save(&path, &all) {
            diagnostic!("cannot pin the host key of {id}: {err}");
        }
    }
}

/// Connects to a remote daemon and opens its window, or says why it did not.
///
/// From a window on the connect page, the window itself becomes the daemon's once it is connected;
/// where the daemon has a window already, that one is brought forward and the connect page closed,
/// as it is once the daemon is opened in the browser.
#[tauri::command]
pub async fn remote_daemon_connect(
    app: AppHandle,
    webview_window: tauri::WebviewWindow,
    request: ConnectRequest,
) -> Result<ConnectOutcome, String> {
    let caller = webview_window.label().to_owned();
    let page = crate::daemon_window::on_connect_page(&app, &caller).then_some(caller);
    connect::connect(&app, request, page, None).await
}

/// Ends the connection to the remote daemon `id`: the one held for the browser, or its window's,
/// closing the window. Its port is let go and the SSH connection ended, nothing asked of the remote
/// daemon.
#[tauri::command]
pub fn remote_daemon_disconnect(app: AppHandle, id: String) {
    window_closed(&app, &format!("{BROWSER_LABEL_PREFIX}{id}"));
    let windows: Vec<String> = app
        .state::<RemoteState>()
        .windows()
        .iter()
        .filter(|(_, live)| live.entry_id == id && matches!(live.via, Via::Window))
        .map(|(label, _)| label.clone())
        .collect();
    for label in windows {
        let closing = app.clone();
        // Destroyed rather than closed, as a menu bar icon's Disconnect does; its connection ends
        // with it (`window_closed`).
        let _ = app.run_on_main_thread(move || {
            crate::background::disconnect_window(&closing, &label);
        });
    }
}

/// Ends the connection of the remote window labelled `label`, which has closed (or never opened):
/// the port is let go and the SSH connection ended, nothing asked of the remote daemon.
pub fn window_closed(app: &AppHandle, label: &str) {
    if app.state::<RemoteState>().end(label) {
        connections_changed(app);
    }
}

/// How each remote daemon with a window open is connected; the others have none.
#[tauri::command]
pub fn remote_daemons_status(app: AppHandle) -> Vec<Status> {
    app.state::<RemoteState>()
        .windows()
        .values()
        .map(|live| Status {
            id: live.entry_id.clone(),
            via: match live.via {
                Via::Window => ViaName::Window,
                Via::Browser { .. } => ViaName::Browser,
            },
            state: if *live.connected.borrow() {
                ConnectionState::Connected
            } else {
                ConnectionState::Reconnecting
            },
        })
        .collect()
}

/// The entries with a window of the application open, in the order their connections were made.
fn window_entries(app: &AppHandle) -> Vec<String> {
    app.state::<RemoteState>().window_entries()
}

/// Ends every remote window's connection, as the application quits.
pub fn end_all(app: &AppHandle) {
    let all: Vec<Live> = app
        .state::<RemoteState>()
        .windows()
        .drain()
        .map(|(_, live)| live)
        .collect();
    for live in all {
        let _ = live.stop.send(true);
    }
}

/// A connection kept for a test, with the receiving end of its stop.
#[cfg(test)]
fn live(identity: &str, entry_id: &str, order: u64, via: Via) -> (Live, watch::Receiver<bool>) {
    let (stop, stopped) = watch::channel(false);
    let (_, connected) = watch::channel(true);
    (
        Live {
            identity: identity.into(),
            entry_id: entry_id.into(),
            name: entry_id.into(),
            order,
            stop,
            connected,
            via,
        },
        stopped,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ending_a_window_s_connection_stops_it_and_lets_it_go_once() {
        let state = RemoteState::default();
        let (kept, stopped) = live("ssh:u@h:22", "a", 0, Via::Window);
        state.windows().insert("daemon-1".into(), kept);
        assert!(state.end("daemon-1"));
        assert!(*stopped.borrow());
        assert!(state.windows().is_empty());
        assert!(!state.end("daemon-1"));
    }

    #[test]
    fn the_windows_a_quit_remembers_follow_the_order_their_connections_were_made_in() {
        let state = RemoteState::default();
        // Labels say the order the windows were registered in, not the one the connections came in.
        for (label, entry, order) in [("daemon-1", "late", 7), ("daemon-9", "early", 2)] {
            state
                .windows()
                .insert(label.into(), live("i", entry, order, Via::Window).0);
        }
        let browser = Via::Browser { url: "u".into() };
        state
            .windows()
            .insert("browser-b".into(), live("j", "b", 1, browser).0);
        assert_eq!(state.window_entries(), ["early", "late"]);
    }

    #[test]
    fn every_window_may_use_the_remote_daemons_and_no_answer_carries_a_secret() {
        // The local window, the remote windows and the connect pages all have the commands.
        for capability in [
            include_str!("../../capabilities/default.json"),
            include_str!("../../capabilities/remote.json"),
        ] {
            assert!(
                capability.contains("\"allow-remote-daemons\""),
                "{capability}"
            );
        }
        let permission = include_str!("../../permissions/remote-daemons.toml");
        for command in [
            "remote_daemons_list",
            "remote_daemon_save",
            "remote_daemon_remove",
            "remote_daemon_connect",
            "remote_daemons_status",
            "remote_daemon_disconnect",
            "remote_window_new",
            "remote_window_open",
            "main_window_show",
        ] {
            assert!(permission.contains(&format!("\"{command}\"")), "{command}");
        }
        // What the list hands a page is where and how to connect, never a password, a passphrase,
        // a device key or an owner key.
        let entry = Entry {
            id: "e".into(),
            name: "n".into(),
            host: "h".into(),
            port: 22,
            user: "u".into(),
            auth: entries::Auth::Device,
            key_path: None,
            fingerprint: Some("SHA256:x".into()),
            octoboardd_path: None,
        };
        let serde_json::Value::Object(fields) = serde_json::to_value(entry).unwrap() else {
            panic!("an entry is an object");
        };
        let mut names: Vec<&str> = fields.keys().map(String::as_str).collect();
        names.sort_unstable();
        assert_eq!(
            names,
            [
                "auth",
                "fingerprint",
                "host",
                "id",
                "keyPath",
                "name",
                "octoboarddPath",
                "port",
                "user"
            ]
        );
    }

    #[test]
    fn the_labels_of_a_connection_held_for_the_browser_name_no_window() {
        use crate::daemon_window::REMOTE_LABEL_PREFIX;
        assert!(!BROWSER_LABEL_PREFIX.starts_with(REMOTE_LABEL_PREFIX));
        assert!(!format!("{BROWSER_LABEL_PREFIX}e").starts_with(REMOTE_LABEL_PREFIX));
    }
}
