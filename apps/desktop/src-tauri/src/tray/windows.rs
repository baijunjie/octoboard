//! The notification-area icon on Windows: a click opens the application's own flyout (`flyout.rs`),
//! which shows the daemons' tabs as the macOS menu does (`tabs::Layout::Tabs`), and the icon is a
//! coloured picture instead of the macOS template octopus, which a dark taskbar would swallow.
//! Attention swaps the picture for one with the dot in its corner, and a quota is not drawn. This
//! module serves every target but macOS, Linux included, where a development build has no flyout:
//! its popup menu can hold no tabs, so each remote daemon's lines are in a submenu of their own
//! (`tabs::Layout::Nested`), its items are plain, and it follows neither the system's theme (a
//! template picture stays black) nor the menu's tracking.

use std::borrow::Cow;

use tauri::image::Image;
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Wry};

use super::finish::Finish;
use super::icon;
use super::model::{AppLabels, TrayMenu};
use super::tabs::{Layout, Tab};
use super::{menu_opened, TrayState};

/// Plain items need no finishing.
pub(super) type Unfinished = ();

/// Windows draws the menu as the application's own flyout (`flyout.rs`), with the tabs of macOS;
/// elsewhere (a development build on Linux) the popup menu holds no tabs, and a remote daemon's
/// lines are in a submenu.
#[cfg(windows)]
pub(super) const LAYOUT: Layout = Layout::Tabs;
#[cfg(not(windows))]
pub(super) const LAYOUT: Layout = Layout::Nested;

/// Whether the icon opens the flyout rather than a popup menu.
pub(super) const FLYOUT: bool = cfg!(windows);

/// The picture carries its own colours.
pub(super) const TEMPLATE: bool = false;

/// The app icon, with the dot when attention is up and larger eyes while the computer is kept
/// awake; the quota has no place in it.
pub(super) fn picture(attention: bool, awake: bool, _quota: Option<icon::Quota>) -> Image<'static> {
    icon::app_icon(attention, awake)
}

/// The bidi isolates the UI wraps names and values in (U+2066 to U+2069) are drawn as boxes by the
/// tooltip, which does not lay them out, so a label loses them here; the Linux popup menu's items
/// take the same text. The order of a left-to-right value inside right-to-left text is then the
/// system's to decide.
pub(super) fn native_text(label: &str) -> Cow<'_, str> {
    if label.contains(is_isolate) {
        Cow::Owned(label.chars().filter(|c| !is_isolate(*c)).collect())
    } else {
        Cow::Borrowed(label)
    }
}

fn is_isolate(c: char) -> bool {
    ('\u{2066}'..='\u{2069}').contains(&c)
}

/// On Windows a click of the left or the right button, as it is released, opens the flyout, or
/// closes it (`flyout::toggle`); the icon has no popup menu. Elsewhere the popup menu opens as the
/// button is released (`show_menu_on_left_click` in `create`), and as the system raises no event
/// when it opens, the UI is told of the click, as macOS's menu tracking tells it, and asks for
/// fresh quotas.
pub(super) fn customize(builder: TrayIconBuilder<Wry>) -> TrayIconBuilder<Wry> {
    builder.on_tray_icon_event(|tray, event| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left | MouseButton::Right,
            button_state: MouseButtonState::Up,
            rect,
            ..
        } = event
        {
            if FLYOUT {
                super::flyout::toggle(tray.app_handle(), physical_rect(&rect));
            } else {
                menu_opened(tray.app_handle());
            }
        }
    })
}

/// The icon's rectangle as the click reports it, in physical pixels (which a click on Windows
/// reports it in).
fn physical_rect(rect: &tauri::Rect) -> super::flyout::Rect {
    let position = rect.position.to_physical::<i32>(1.0);
    let size = rect.size.to_physical::<i32>(1.0);
    super::flyout::Rect {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    }
}

/// Nothing to finish: the plain items are the menu as it is shown.
pub(super) fn finish_when_opened(
    _state: &TrayState,
    _finish: Finish,
    _tabs: &[Tab],
    _menus: &std::collections::HashMap<String, TrayMenu>,
    _labels: &AppLabels,
) {
}

/// Nothing to follow here: the flyout reports its own opening and closing (`flyout.rs`), and the
/// Linux popup menu reports nothing.
pub(super) fn follow_menu_tracking(_app: &AppHandle) {}

/// The label is the icon's tooltip, which the notification area also reads out as its name.
pub(super) fn set_icon_label(tray: &TrayIcon, label: &str) {
    let _ = tray.set_tooltip(Some(native_text(label)));
}

/// Shows or hides the icon in the notification area, keeping its picture and tooltip.
pub(super) fn set_visible(tray: &TrayIcon, visible: bool) -> tauri::Result<()> {
    tray.set_visible(visible)
}

/// The dot and the eyes are part of the picture, so they are up whenever the picture says so.
pub(super) fn show_layers(_tray: &TrayIcon, _dot: bool, _eyes: bool) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::native_text;

    #[test]
    fn a_label_loses_its_bidi_isolates() {
        assert_eq!(
            native_text("Claude Code \u{2068}work\u{2069}: 5-hour 42%"),
            "Claude Code work: 5-hour 42%"
        );
        assert_eq!(native_text("\u{2066}a\u{2069}\u{2067}b\u{2069}"), "ab");
    }

    #[test]
    fn a_label_without_isolates_is_kept_as_it_is() {
        let label = "Update to 1.2.0 \u{200f}\u{202b}";
        assert_eq!(native_text(label), label);
        assert!(matches!(native_text(label), std::borrow::Cow::Borrowed(_)));
    }
}
