//! One window per daemon: the main window shows the local daemon, the sidecar, and each other
//! daemon the application is connected to has a window of its own, labelled `daemon-<n>`, which is
//! told the daemon's address, the name to show for it and the identity what it remembers of that
//! daemon is kept under. This module hands out those labels and keeps which window the user
//! focused last, which a reopen of the application and the menu's Settings and About go to.
//!
//! A remote window is opened only for a connection that is already held. The windows themselves
//! are built in `window_factory`.
//!
//! The application's own features stay with the main window: a remote window is given fewer host
//! facts (`host_facts::for_remote_window`) and fewer shell commands (`capabilities/remote.json`),
//! and the shell's events for the application (the exit flow, the menu bar icon's Check for
//! updates, the local daemon ending) are sent to the main window alone.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use tauri::{AppHandle, Manager, WebviewWindow};

use crate::MAIN_WINDOW_LABEL;

/// What a remote window's label starts with; `capabilities/remote.json` names its windows by it.
pub const REMOTE_LABEL_PREFIX: &str = "daemon-";

/// A daemon other than the local one, as its window is opened on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteDaemon {
    /// What the window remembers of the daemon (its size and position here, its preferences in the
    /// page's storage) is kept under; stable across connections to the same daemon.
    pub identity: String,
    /// The name the window shows for the daemon: the remote host's.
    pub name: String,
    /// The loopback port the window reaches the daemon at, and the key it sends there.
    pub port: u16,
    pub key: String,
}

#[derive(Default)]
pub struct DaemonWindows {
    next: AtomicU64,
    /// The label of the window the user focused last, while it exists.
    last_focused: Mutex<Option<String>>,
}

impl DaemonWindows {
    /// A label for a new remote window.
    pub fn register(&self) -> String {
        format!(
            "{REMOTE_LABEL_PREFIX}{}",
            self.next.fetch_add(1, Ordering::SeqCst) + 1
        )
    }

    /// Records that the user focused the window labelled `label`.
    fn note_focused(&self, label: &str) {
        *self.last_focused.lock().unwrap_or_else(|e| e.into_inner()) = Some(label.to_owned());
    }

    /// Forgets the window labelled `label`, which has closed or never opened.
    pub fn forget(&self, label: &str) {
        let mut last = self.last_focused.lock().unwrap_or_else(|e| e.into_inner());
        if last.as_deref() == Some(label) {
            *last = None;
        }
    }
}

/// Whether the window labelled `label` is the main one, the local daemon's.
pub fn is_main(label: &str) -> bool {
    label == MAIN_WINDOW_LABEL
}

/// Takes a label for a new remote window.
pub fn register(app: &AppHandle) -> String {
    app.state::<DaemonWindows>().register()
}

/// Forgets the window labelled `label`, which has closed.
pub fn forget(app: &AppHandle, label: &str) {
    app.state::<DaemonWindows>().forget(label);
}

/// Records that the user focused the window labelled `label`.
pub fn note_focused(app: &AppHandle, label: &str) {
    app.state::<DaemonWindows>().note_focused(label);
}

/// The window the user focused last, while it exists, else the main window.
pub fn last_focused_or_main(app: &AppHandle) -> Option<WebviewWindow> {
    let last = app
        .state::<DaemonWindows>()
        .last_focused
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    last.and_then(|label| app.get_webview_window(&label))
        .or_else(|| app.get_webview_window(MAIN_WINDOW_LABEL))
}

/// The window a menu item that asks the UI for something goes to: the window focused last, when
/// it is on screen, else the main window.
pub fn menu_target(app: &AppHandle) -> Option<WebviewWindow> {
    last_focused_or_main(app)
        .filter(|window| window.is_visible().unwrap_or(false))
        .or_else(|| app.get_webview_window(MAIN_WINDOW_LABEL))
}

/// Whether a window other than the one labelled `except` is on screen (visible, minimized or not).
pub fn another_visible(app: &AppHandle, except: &str) -> bool {
    app.webview_windows()
        .iter()
        .any(|(label, window)| label != except && window.is_visible().unwrap_or(false))
}

/// The query parameters that tell a remote window which daemon it shows: `daemon`, the identity
/// what it remembers is kept under, and `remote`, the name to show, both percent-encoded.
pub fn remote_params(remote: &RemoteDaemon) -> String {
    format!(
        "daemon={}&remote={}",
        crate::percent_encode_query_value(&remote.identity),
        crate::percent_encode_query_value(&remote.name)
    )
}

/// The title of the window that shows `remote`.
pub fn remote_title(app_name: &str, remote: &RemoteDaemon) -> String {
    format!("{} — {app_name}", remote.name)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn a_remote_window_is_told_its_daemon_s_identity_and_name() {
        let remote = RemoteDaemon {
            identity: "me@host:22".into(),
            name: "host & co".into(),
            port: 1,
            key: "aa".into(),
        };
        assert_eq!(
            remote_params(&remote),
            "daemon=me%40host%3A22&remote=host%20%26%20co"
        );
        assert_eq!(remote_title("Octoboard", &remote), "host & co — Octoboard");
    }

    #[test]
    fn only_the_main_window_is_the_local_daemon_s() {
        assert!(is_main(MAIN_WINDOW_LABEL));
        assert!(!is_main("daemon-1"));
    }

    /// What the capability file `name` grants: the windows it names and the permission identifiers.
    fn capability(name: &str) -> (Vec<String>, Vec<String>) {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("capabilities")
            .join(name);
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let strings = |key: &str| -> Vec<String> {
            json[key]
                .as_array()
                .unwrap_or_else(|| panic!("{name} has no {key}"))
                .iter()
                .map(|value| value.as_str().unwrap().to_owned())
                .collect()
        };
        (strings("windows"), strings("permissions"))
    }

    /// Each permission file's identifiers with the commands they allow.
    fn app_permissions() -> HashMap<String, Vec<String>> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("permissions");
        let mut all = HashMap::new();
        for file in std::fs::read_dir(dir).unwrap() {
            let path = file.unwrap().path();
            if path.extension().is_none_or(|extension| extension != "toml") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            let mut identifier = None;
            for line in text.lines() {
                if let Some(value) = line.strip_prefix("identifier = ") {
                    identifier = Some(value.trim().trim_matches('"').to_owned());
                } else if let Some(list) = line.strip_prefix("commands.allow = [") {
                    let commands = list
                        .trim_end_matches(']')
                        .split(',')
                        .map(|command| command.trim().trim_matches('"').to_owned())
                        .filter(|command| !command.is_empty())
                        .collect();
                    all.insert(identifier.clone().expect("an identifier first"), commands);
                }
            }
        }
        all
    }

    /// The application's own commands a window of `capability` may call.
    fn reachable(name: &str) -> std::collections::BTreeSet<String> {
        let permissions = app_permissions();
        capability(name)
            .1
            .iter()
            .filter_map(|identifier| permissions.get(identifier))
            .flatten()
            .cloned()
            .collect()
    }

    #[test]
    fn labels_are_handed_out_once_each() {
        let windows = DaemonWindows::default();
        let first = windows.register();
        let second = windows.register();
        assert_ne!(first, second);
        assert!(first.starts_with(REMOTE_LABEL_PREFIX));
    }

    #[test]
    fn the_window_focused_last_is_forgotten_once_it_is_gone() {
        let windows = DaemonWindows::default();
        windows.note_focused("daemon-1");
        windows.note_focused("daemon-2");
        let last = |windows: &DaemonWindows| windows.last_focused.lock().unwrap().clone();
        assert_eq!(last(&windows).as_deref(), Some("daemon-2"));
        // Another window's closing leaves it be.
        windows.forget("daemon-1");
        assert_eq!(last(&windows).as_deref(), Some("daemon-2"));
        windows.forget("daemon-2");
        assert_eq!(last(&windows), None);
    }

    /// Every command `lib.rs` registers with the invoke handler.
    fn registered_commands() -> Vec<String> {
        let source = include_str!("lib.rs");
        let start = source.find("generate_handler![").expect("a handler list") + 18;
        let end = start + source[start..].find("])").unwrap();
        source[start..end]
            .split(',')
            .map(|path| path.trim().rsplit("::").next().unwrap().to_owned())
            .filter(|name| !name.is_empty())
            .collect()
    }

    #[test]
    fn the_capabilities_name_the_windows_the_shell_opens() {
        let (main, _) = capability("default.json");
        assert_eq!(main, [MAIN_WINDOW_LABEL]);
        // A remote window is `daemon-<n>`; the main window and the flyout are not among them.
        let (remote, _) = capability("remote.json");
        assert_eq!(remote, [format!("{REMOTE_LABEL_PREFIX}*")]);
        assert!(!is_main("daemon-1"));
        assert!(!MAIN_WINDOW_LABEL.starts_with(REMOTE_LABEL_PREFIX));
        assert!(!crate::tray::flyout::FLYOUT_LABEL.starts_with(REMOTE_LABEL_PREFIX));
        let (flyout, _) = capability("flyout.json");
        assert_eq!(flyout, [crate::tray::flyout::FLYOUT_LABEL]);
    }

    #[test]
    fn what_acts_on_a_daemon_s_connection_is_for_the_daemon_windows_alone() {
        let (windows, permissions) = capability("remote-window.json");
        assert_eq!(windows, [format!("{REMOTE_LABEL_PREFIX}*")]);
        let own = reachable("remote-window.json");
        for command in [
            "remote_link_address",
            "remote_forwards_list",
            "remote_forward_stop",
            "remote_file_begin",
            "remote_file_write",
            "remote_file_open",
            "remote_file_discard",
        ] {
            assert!(own.contains(command), "{command}");
        }
        for name in ["default.json", "remote.json", "flyout.json"] {
            let (_, others) = capability(name);
            assert!(
                permissions
                    .iter()
                    .all(|permission| !others.contains(permission)),
                "{name}"
            );
            assert!(reachable(name).is_disjoint(&own), "{name}");
        }
    }

    #[test]
    fn a_remote_window_is_allowed_none_of_the_applications_own_commands() {
        let remote = reachable("remote.json");
        for command in [
            // Quitting and the exit flow.
            "begin_quit",
            "confirm_quit",
            "frontend_exit_heartbeat",
            "daemon_exit_detail",
            // The settings the shell keeps, launch at login and the privacy grants.
            "set_app_settings",
            "launch_at_login_status",
            "set_launch_at_login",
            "open_login_items_settings",
            "permissions_status",
            "request_permission",
            "open_privacy_settings",
            // The updater, the directory picker (the daemon does not share this machine's files)
            // and the SSH server of this computer (the Remote access section is the local one's).
            "check_for_update",
            "install_update",
            "pick_directory",
            "remote_login_status",
            "open_remote_login_settings",
            // The remote daemons, which the Remote devices page of the main window keeps.
            "remote_daemons_list",
            "remote_daemon_save",
            "remote_daemon_remove",
            "remote_daemon_connect",
            "remote_daemons_status",
            "remote_daemon_disconnect",
            "remote_connect_pending_take",
            "remote_page_requested_take",
            "main_window_show",
            // The flyout's own.
            "tray_flyout_model",
            "tray_flyout_size",
            "tray_flyout_choose",
            "tray_flyout_close",
        ] {
            assert!(!remote.contains(command), "{command}");
        }
    }

    #[test]
    fn a_remote_window_is_allowed_what_it_needs_of_the_shell() {
        let remote = reachable("remote.json");
        for command in [
            "set_tray_menu",
            "set_keep_awake_shown",
            "bring_to_front",
            "set_badge_count",
            "show_notification",
            "set_menu_labels",
            "reveal_main_window",
            "open_notification_settings",
            "save_configuration_file",
            "open_configuration_file",
        ] {
            assert!(remote.contains(command), "{command}");
        }
    }

    #[test]
    fn the_flyout_is_allowed_its_own_commands_alone_and_no_other_window_those() {
        let flyout = reachable("flyout.json");
        let own: std::collections::BTreeSet<String> = [
            "tray_flyout_model",
            "tray_flyout_size",
            "tray_flyout_choose",
            "tray_flyout_close",
        ]
        .map(str::to_owned)
        .into();
        assert_eq!(flyout, own);
        for name in ["default.json", "remote.json"] {
            assert!(reachable(name).is_disjoint(&own), "{name}");
        }
    }

    #[test]
    fn the_local_window_may_call_every_command_but_the_flyout_s_and_the_remote_window_s_own() {
        let main = reachable("default.json");
        // The forwarding and file commands act on a remote window's own connection, which the local window has none of.
        let remote_only = [
            "remote_link_address",
            "remote_forwards_list",
            "remote_forward_stop",
            "remote_file_begin",
            "remote_file_write",
            "remote_file_open",
            "remote_file_discard",
        ];
        for command in registered_commands() {
            if command.starts_with("tray_flyout_") || remote_only.contains(&command.as_str()) {
                assert!(!main.contains(&command), "{command}");
            } else {
                assert!(
                    main.contains(&command),
                    "{command} is registered but not allowed"
                );
            }
        }
    }

    #[test]
    fn every_command_the_shell_registers_is_allowed_to_some_window() {
        let allowed: std::collections::BTreeSet<String> = [
            "default.json",
            "remote.json",
            "remote-window.json",
            "flyout.json",
        ]
        .into_iter()
        .flat_map(reachable)
        .collect();
        let registered = registered_commands();
        assert!(registered.len() > 30, "{registered:?}");
        for command in &registered {
            assert!(allowed.contains(command), "{command}");
        }
        // A permission names no command the shell does not register.
        for command in &allowed {
            assert!(registered.contains(command), "{command}");
        }
    }
}
