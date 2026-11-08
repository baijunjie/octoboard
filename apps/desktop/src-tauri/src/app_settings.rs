//! The two settings the shell itself must know before any page has loaded, kept in `settings.json`
//! under the application's config directory, next to `window-state.json`: whether closing the
//! window keeps the app running in the background (`background.rs`) and whether the menu bar icon
//! is shown (`tray/`). The shell needs them before the page loads, so that a hidden icon never
//! shows and a window closed before the page has loaded hides instead of quitting the app.
//!
//! The file is read once, in `setup`, and its values live in managed state (`AppSettingsState`).
//! They reach the page as the `?settings=` query parameter of the window's URL (`current_json`),
//! next to `?host=`, so the page starts from them rather than from a copy of its own; the page
//! changes them through `set_app_settings`, which applies a change to the shell, keeps it in
//! memory and writes the file whole, atomically. A value missing from the file takes its default,
//! so a file written by an older or newer build is read as far as it goes, and a file that cannot
//! be read at all counts as no file.
//!
//! `stored` in the query says whether there was a file to read. The page uses it to carry over the
//! values it kept in its own storage before the file existed, once.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::background;
use crate::config_dir;
use crate::diagnostics::diagnostic;
use crate::tray;

const FILE_NAME: &str = "settings.json";

/// What `settings.json` holds. Either field may be missing from the file, and a field this build
/// does not know is ignored.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub run_in_background: bool,
    pub show_menu_bar_icon: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            run_in_background: false,
            show_menu_bar_icon: true,
        }
    }
}

/// A change the page asks for: only the fields it names are changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct AppSettingsUpdate {
    pub run_in_background: Option<bool>,
    pub show_menu_bar_icon: Option<bool>,
}

impl AppSettings {
    fn updated(self, update: AppSettingsUpdate) -> Self {
        Self {
            run_in_background: update.run_in_background.unwrap_or(self.run_in_background),
            show_menu_bar_icon: update.show_menu_bar_icon.unwrap_or(self.show_menu_bar_icon),
        }
    }
}

/// The settings now in force, and whether they are on disk.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Current {
    settings: AppSettings,
    /// False from a launch that found no usable file until a write has succeeded.
    stored: bool,
}

/// The managed state holding the settings now in force. Locked across a change and its write, so
/// two changes cannot write the file out of order.
pub struct AppSettingsState(Mutex<Current>);

/// What the window's `?settings=` carries: the values and the `stored` marker.
#[derive(Serialize)]
struct QueryValue {
    #[serde(flatten)]
    settings: AppSettings,
    stored: bool,
}

/// Reads the file, puts the settings in managed state and applies them to the shell's own state
/// (the close button's answer), so they hold before the page loads. Called from `setup`, before the
/// icon is created, which reads them too.
pub fn install(app: &AppHandle) {
    let stored = match config_dir::config_path(app, FILE_NAME) {
        Ok(path) => read_settings(&path),
        Err(err) => {
            diagnostic!("cannot locate the saved settings: {err}");
            None
        }
    };
    let current = Current {
        settings: stored.unwrap_or_default(),
        stored: stored.is_some(),
    };
    background::set_run_in_background(app, current.settings.run_in_background);
    app.manage(AppSettingsState(Mutex::new(current)));
}

/// The settings now in force.
pub fn current(app: &AppHandle) -> AppSettings {
    app.state::<AppSettingsState>()
        .0
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .settings
}

/// The JSON the window's `?settings=` carries (percent-encoding is the caller's), as the settings
/// stand now: the window is opened with it, and a window reloaded for a restarted daemon is given
/// it again so it does not start from values older than a change made since.
pub fn current_json(app: &AppHandle) -> String {
    let current = *app
        .state::<AppSettingsState>()
        .0
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    query_json(current.settings, current.stored)
}

fn query_json(settings: AppSettings, stored: bool) -> String {
    serde_json::to_string(&QueryValue { settings, stored }).unwrap_or_else(|_| "{}".to_string())
}

/// Changes the settings the update names: in memory and in the shell first, so the choice holds
/// whether or not the file can be written, then on disk. The error is for a write or a change of
/// the icon that failed; each failure is logged here too, and the page ignores the error.
#[tauri::command]
pub fn set_app_settings(app: AppHandle, settings: AppSettingsUpdate) -> Result<(), String> {
    let state = app.state::<AppSettingsState>();
    let written = {
        let mut current = state.0.lock().unwrap_or_else(|err| err.into_inner());
        current.settings = current.settings.updated(settings);
        let now = current.settings;
        background::set_run_in_background(&app, now.run_in_background);
        let written = config_dir::config_path(&app, FILE_NAME)
            .map_err(anyhow::Error::from)
            .and_then(|path| config_dir::write_json(&path, &now));
        current.stored |= written.is_ok();
        written
    };
    // Outside the lock, so it stays safe should the command ever run off the main thread: the
    // icon changes on the main thread, and the window's opening reads the settings there, so
    // waiting for it while holding the lock could deadlock the two.
    let icon = match settings.show_menu_bar_icon {
        Some(shown) => tray::set_icon_visible(&app, shown).map_err(|err| {
            let message = format!("cannot change the menu bar icon: {err}");
            diagnostic!("{message}");
            message
        }),
        None => Ok(()),
    };
    let saved = written.map_err(|err| {
        let message = format!("cannot save the settings: {err}");
        diagnostic!("{message}");
        message
    });
    icon.and(saved)
}

/// The settings in the file, or `None` when there is none or it cannot be read.
fn read_settings(path: &Path) -> Option<AppSettings> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == ErrorKind::NotFound => return None,
        Err(err) => {
            diagnostic!("cannot read {}: {err}", path.display());
            return None;
        }
    };
    match serde_json::from_slice(&bytes) {
        Ok(settings) => Some(settings),
        Err(err) => {
            diagnostic!("ignoring {}: {err}", path.display());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_dir::test_support::Scratch;

    const SHOWN_AND_STAYING: AppSettings = AppSettings {
        run_in_background: true,
        show_menu_bar_icon: true,
    };

    #[test]
    fn defaults_run_in_the_foreground_with_the_icon_shown() {
        assert_eq!(
            AppSettings::default(),
            AppSettings {
                run_in_background: false,
                show_menu_bar_icon: true
            }
        );
    }

    #[test]
    fn missing_file_reads_as_none() {
        let scratch = Scratch::new("app-settings", "missing");
        assert_eq!(read_settings(&scratch.0.join(FILE_NAME)), None);
    }

    #[test]
    fn a_whole_file_is_read_as_written() {
        let scratch = Scratch::new("app-settings", "whole");
        fs::create_dir_all(&scratch.0).unwrap();
        let path = scratch.0.join(FILE_NAME);
        fs::write(
            &path,
            br#"{"run_in_background": true, "show_menu_bar_icon": false}"#,
        )
        .unwrap();
        assert_eq!(
            read_settings(&path),
            Some(AppSettings {
                run_in_background: true,
                show_menu_bar_icon: false
            })
        );
    }

    #[test]
    fn a_missing_field_takes_its_default_and_an_unknown_one_is_ignored() {
        let scratch = Scratch::new("app-settings", "partial");
        fs::create_dir_all(&scratch.0).unwrap();
        let path = scratch.0.join(FILE_NAME);
        fs::write(
            &path,
            br#"{"run_in_background": true, "from_the_future": 3}"#,
        )
        .unwrap();
        assert_eq!(read_settings(&path), Some(SHOWN_AND_STAYING));
        fs::write(&path, br#"{"show_menu_bar_icon": false}"#).unwrap();
        assert_eq!(
            read_settings(&path),
            Some(AppSettings {
                run_in_background: false,
                show_menu_bar_icon: false
            })
        );
        fs::write(&path, b"{}").unwrap();
        assert_eq!(read_settings(&path), Some(AppSettings::default()));
    }

    #[test]
    fn corrupt_file_reads_as_none() {
        let scratch = Scratch::new("app-settings", "corrupt");
        fs::create_dir_all(&scratch.0).unwrap();
        let path = scratch.0.join(FILE_NAME);
        fs::write(&path, b"{\"run_in_background\": tr").unwrap();
        assert_eq!(read_settings(&path), None);
        fs::write(&path, br#"{"run_in_background": "yes"}"#).unwrap();
        assert_eq!(read_settings(&path), None);
        fs::write(&path, b"null").unwrap();
        assert_eq!(read_settings(&path), None);
    }

    #[test]
    fn settings_round_trip_through_a_directory_created_on_write() {
        let scratch = Scratch::new("app-settings", "round-trip");
        let path = scratch.0.join("nested").join(FILE_NAME);
        config_dir::write_json(&path, &SHOWN_AND_STAYING).unwrap();
        assert_eq!(read_settings(&path), Some(SHOWN_AND_STAYING));

        // Overwriting goes through the same rename, and leaves nothing else behind.
        let second = AppSettings {
            run_in_background: false,
            show_menu_bar_icon: false,
        };
        config_dir::write_json(&path, &second).unwrap();
        assert_eq!(read_settings(&path), Some(second));
        let names: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, [FILE_NAME]);
    }

    #[test]
    fn an_update_changes_only_the_fields_it_names() {
        let base = AppSettings::default();
        assert_eq!(base.updated(AppSettingsUpdate::default()), base);
        assert_eq!(
            base.updated(AppSettingsUpdate {
                run_in_background: Some(true),
                show_menu_bar_icon: None
            }),
            SHOWN_AND_STAYING
        );
        assert_eq!(
            SHOWN_AND_STAYING.updated(AppSettingsUpdate {
                run_in_background: None,
                show_menu_bar_icon: Some(false)
            }),
            AppSettings {
                run_in_background: true,
                show_menu_bar_icon: false
            }
        );
    }

    #[test]
    fn an_update_from_the_page_names_fields_in_snake_case() {
        let update: AppSettingsUpdate =
            serde_json::from_str(r#"{"show_menu_bar_icon": false}"#).unwrap();
        assert_eq!(
            update,
            AppSettingsUpdate {
                run_in_background: None,
                show_menu_bar_icon: Some(false)
            }
        );
    }

    #[test]
    fn the_query_value_carries_the_settings_and_whether_they_are_stored() {
        assert_eq!(
            query_json(AppSettings::default(), false),
            r#"{"run_in_background":false,"show_menu_bar_icon":true,"stored":false}"#
        );
        assert_eq!(
            query_json(SHOWN_AND_STAYING, true),
            r#"{"run_in_background":true,"show_menu_bar_icon":true,"stored":true}"#
        );
    }
}
