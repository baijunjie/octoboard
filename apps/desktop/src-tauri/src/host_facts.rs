//! What the UI cannot learn from the webview about the machine the shell runs on, declared by the shell
//! and handed to the window as the `host` query parameter (a JSON object in the shape of `HostFacts` in
//! `packages/ui/src/platform/host.ts`, which this mirrors field for field). The UI takes every platform
//! difference from these facts instead of inferring it from the user agent.

use serde::Serialize;

/// What the top bar must keep clear at the window's edges.
#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Chrome {
    /// No titlebar of its own: the bar is the titlebar and keeps the window controls clear at the left.
    OverlayLeft,
    /// The same, with the controls at the right.
    #[allow(dead_code)]
    OverlayRight,
    /// The system's own titlebar above the page.
    Native,
    /// No window controls at all.
    #[allow(dead_code)]
    None,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StatusItem {
    /// A menu with custom rows and pictures.
    Rich,
    /// Plain text items.
    Plain,
    None,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum NotificationSettings {
    /// The system has a pane or page for the app's notifications, which the UI links to.
    SystemPane,
    None,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Updater {
    /// The new version replaces the running application.
    InPlace,
    /// The new version is an installer that is run.
    Installer,
    None,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Trash {
    Trash,
    RecycleBin,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SettingsApp {
    SystemSettings,
    Settings,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MenuBarIcon {
    MenuBar,
    NotificationArea,
}

/// The words the system uses for the things the UI's copy names.
#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Terms {
    pub trash: Trash,
    pub settings: SettingsApp,
    pub menu_bar_icon: MenuBarIcon,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Os {
    Macos,
    Windows,
    Linux,
}

/// The primary shortcut modifier.
#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Modifier {
    Meta,
    Control,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HostFacts {
    pub os: Os,
    pub modifier: Modifier,
    pub chrome: Chrome,
    /// The width, in points, the window controls cover at the bar's leading edge; only with an overlay chrome.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traffic_light_inset: Option<u32>,
    /// A translucent native material sits behind a transparent webview.
    pub translucent: bool,
    /// A native menu bar exists.
    pub app_menu: bool,
    pub status_item: StatusItem,
    /// The application's icon can carry a count.
    pub badge: bool,
    /// Closing the window can keep the application running.
    pub run_in_background: bool,
    /// The menu bar icon can show that the daemon is keeping the computer awake (`tray::set_keep_awake_shown`).
    pub keep_awake: bool,
    /// The system can open the application when the user logs in (`launch_at_login/`).
    pub launch_at_login: bool,
    /// The system has the privacy grants the app lets the user give on purpose.
    pub permissions_pane: bool,
    pub notification_settings: NotificationSettings,
    pub updater: Updater,
    pub terms: Terms,
}

/// What the traffic lights cover at the bar's leading edge: `TRAFFIC_LIGHT_X` in `chrome/macos.rs`
/// plus three buttons and a gap.
const TRAFFIC_LIGHT_INSET: u32 = 80;

fn macos() -> HostFacts {
    HostFacts {
        os: Os::Macos,
        modifier: Modifier::Meta,
        chrome: Chrome::OverlayLeft,
        traffic_light_inset: Some(TRAFFIC_LIGHT_INSET),
        translucent: true,
        app_menu: true,
        status_item: StatusItem::Rich,
        badge: true,
        run_in_background: true,
        keep_awake: true,
        launch_at_login: crate::launch_at_login::supported(),
        permissions_pane: true,
        notification_settings: NotificationSettings::SystemPane,
        updater: Updater::InPlace,
        terms: Terms {
            trash: Trash::Trash,
            settings: SettingsApp::SystemSettings,
            menu_bar_icon: MenuBarIcon::MenuBar,
        },
    }
}

/// Native decorations, no application menu (Settings, About and Quit are reachable from the rail and the
/// tray) and a plain tray menu. The Settings page for notifications is where the user turns the app's off
/// or restyles them, and `notification_settings.rs` opens it.
fn windows() -> HostFacts {
    HostFacts {
        os: Os::Windows,
        modifier: Modifier::Control,
        chrome: Chrome::Native,
        traffic_light_inset: None,
        translucent: false,
        app_menu: false,
        status_item: StatusItem::Plain,
        badge: false,
        run_in_background: true,
        keep_awake: true,
        launch_at_login: true,
        permissions_pane: false,
        notification_settings: NotificationSettings::SystemPane,
        updater: Updater::Installer,
        terms: Terms {
            trash: Trash::RecycleBin,
            settings: SettingsApp::Settings,
            menu_bar_icon: MenuBarIcon::NotificationArea,
        },
    }
}

/// A build target with no desktop port of its own: the system's own titlebar and nothing else the shell
/// would have to provide.
fn linux() -> HostFacts {
    HostFacts {
        os: Os::Linux,
        modifier: Modifier::Control,
        chrome: Chrome::Native,
        traffic_light_inset: None,
        translucent: false,
        app_menu: false,
        status_item: StatusItem::None,
        badge: false,
        run_in_background: false,
        keep_awake: false,
        launch_at_login: false,
        permissions_pane: false,
        notification_settings: NotificationSettings::None,
        updater: Updater::None,
        terms: Terms {
            trash: Trash::Trash,
            settings: SettingsApp::Settings,
            menu_bar_icon: MenuBarIcon::NotificationArea,
        },
    }
}

/// The facts of the machine this shell was built for.
pub fn current() -> HostFacts {
    if cfg!(target_os = "macos") {
        macos()
    } else if cfg!(target_os = "windows") {
        windows()
    } else {
        linux()
    }
}

/// `current()` as the JSON the window's query carries.
pub fn current_json() -> String {
    serde_json::to_string(&current()).expect("host facts serialise")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(facts: &HostFacts) -> serde_json::Value {
        serde_json::to_value(facts).unwrap()
    }

    #[test]
    fn macos_reports_what_the_ui_has_always_assumed_there() {
        assert_eq!(
            json(&macos()),
            serde_json::json!({
                "os": "macos",
                "modifier": "meta",
                "chrome": "overlay-left",
                "trafficLightInset": 80,
                "translucent": true,
                "appMenu": true,
                "statusItem": "rich",
                "badge": true,
                "runInBackground": true,
                "keepAwake": true,
                "launchAtLogin": crate::launch_at_login::supported(),
                "permissionsPane": true,
                "notificationSettings": "system-pane",
                "updater": "in-place",
                "terms": {
                    "trash": "trash",
                    "settings": "systemSettings",
                    "menuBarIcon": "menuBar"
                }
            })
        );
    }

    #[test]
    fn windows_takes_native_chrome_and_no_application_menu() {
        assert_eq!(
            json(&windows()),
            serde_json::json!({
                "os": "windows",
                "modifier": "control",
                "chrome": "native",
                "translucent": false,
                "appMenu": false,
                "statusItem": "plain",
                "badge": false,
                "runInBackground": true,
                "keepAwake": true,
                "launchAtLogin": true,
                "permissionsPane": false,
                "notificationSettings": "system-pane",
                "updater": "installer",
                "terms": {
                    "trash": "recycleBin",
                    "settings": "settings",
                    "menuBarIcon": "notificationArea"
                }
            })
        );
    }

    #[test]
    fn the_overlay_inset_is_reported_only_with_an_overlay() {
        for facts in [macos(), windows(), linux()] {
            let overlay = matches!(facts.chrome, Chrome::OverlayLeft | Chrome::OverlayRight);
            assert_eq!(facts.traffic_light_inset.is_some(), overlay);
        }
    }

    #[test]
    fn the_current_facts_are_those_of_the_build_target() {
        let os = json(&current())["os"].clone();
        let expected = if cfg!(target_os = "macos") {
            "macos"
        } else if cfg!(target_os = "windows") {
            "windows"
        } else {
            "linux"
        };
        assert_eq!(os, expected);
        assert!(current_json().starts_with('{'));
    }
}
