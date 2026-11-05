//! The notification-area icon on Windows: the plain items muda builds from the same menu, with
//! nothing to finish or lay over them, and a coloured picture instead of the macOS template
//! octopus, which a dark taskbar would swallow. Attention swaps the picture for one with the dot in
//! its corner, and a quota is not drawn. The system tints no template picture, so a single-colour
//! one in the menu is coloured here for the theme the menu is drawn in (`menu_picture`), and the
//! menu is built again when that theme changes. While the menu is open, a menu the UI sends is held
//! back (`follow_menu_tracking`). This module serves every target but macOS, Linux included, where
//! the menu is not followed and a template picture stays black.

use std::borrow::Cow;

use tauri::image::Image;
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Wry};

use super::icon;
use super::model::TrayMenu;
use super::MENU_OPENED_EVENT;

/// Plain items need no finishing.
pub(super) type Unfinished = ();

/// The picture carries its own colours.
pub(super) const TEMPLATE: bool = false;

/// The app icon, with the dot when attention is up; the quota has no place in it.
pub(super) fn picture(attention: bool, _quota: Option<icon::Quota>) -> Image<'static> {
    icon::app_icon(attention)
}

/// A line's picture for the menu: a template one, which the UI draws black for its alpha alone, is
/// redrawn in the colour of the menu's text, as macOS tints it; a coloured one is kept.
pub(super) fn menu_picture(picture: Image<'static>, template: bool) -> Image<'static> {
    if !template {
        return picture;
    }
    let (width, height) = (picture.width(), picture.height());
    let mut rgba = picture.rgba().to_vec();
    tint(&mut rgba, theme::mark_colour());
    Image::new_owned(rgba, width, height)
}

/// Gives every pixel of `rgba` the colour `rgb`, keeping its alpha, which is all a template carries.
fn tint(rgba: &mut [u8], rgb: [u8; 3]) {
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel[..3].copy_from_slice(&rgb);
    }
}

/// The theme the tray's menu is drawn in. The window library asks the system to allow dark menus
/// for the app (tao's `SetPreferredAppMode(AllowDark)`), and the system then draws a popup menu
/// dark when the user's default app mode is dark (`AppsUseLightTheme` 0), which uxtheme's
/// `ShouldAppsUseDarkMode` reads too; the Windows mode (`SystemUsesLightTheme`) is the taskbar's,
/// and the app's own theme, which only the web page follows, is neither. High contrast draws menus
/// in its own colours instead.
mod theme {
    /// The colour of a template picture in the menu: the menu's text colour under high contrast
    /// (`high_contrast_text`), otherwise white on a dark menu and black on a light one.
    /// `apps_use_light` is the `AppsUseLightTheme` value, where absent reads as light, as Windows
    /// shows it.
    pub(super) fn colour_for(
        apps_use_light: Option<u32>,
        high_contrast_text: Option<[u8; 3]>,
    ) -> [u8; 3] {
        match (high_contrast_text, apps_use_light) {
            (Some(text), _) => text,
            (None, Some(0)) => [0xff; 3],
            (None, _) => [0; 3],
        }
    }

    #[cfg(windows)]
    pub(super) fn mark_colour() -> [u8; 3] {
        colour_for(apps_use_light_theme(), high_contrast_text())
    }

    /// Off Windows a template picture stays as the UI drew it.
    #[cfg(not(windows))]
    pub(super) fn mark_colour() -> [u8; 3] {
        [0; 3]
    }

    /// `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize` `AppsUseLightTheme`.
    #[cfg(windows)]
    fn apps_use_light_theme() -> Option<u32> {
        use windows_sys::Win32::Foundation::ERROR_SUCCESS;
        use windows_sys::Win32::System::Registry::{
            RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD,
        };

        let key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0"
            .encode_utf16()
            .collect();
        let name: Vec<u16> = "AppsUseLightTheme\0".encode_utf16().collect();
        let mut value: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        // SAFETY: both names are NUL-terminated and outlive the call, and `value` holds the `size`
        // bytes a DWORD takes.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                (&mut value as *mut u32).cast(),
                &mut size,
            )
        };
        (status == ERROR_SUCCESS).then_some(value)
    }

    /// The menu's text colour, while high contrast is on.
    #[cfg(windows)]
    fn high_contrast_text() -> Option<[u8; 3]> {
        use windows_sys::Win32::Graphics::Gdi::{GetSysColor, COLOR_MENUTEXT};
        use windows_sys::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SystemParametersInfoW, SPI_GETHIGHCONTRAST,
        };

        let mut contrast = HIGHCONTRASTW {
            cbSize: std::mem::size_of::<HIGHCONTRASTW>() as u32,
            dwFlags: 0,
            lpszDefaultScheme: std::ptr::null_mut(),
        };
        // SAFETY: `contrast` is the structure the action fills, its size set as it asks.
        let read = unsafe {
            SystemParametersInfoW(
                SPI_GETHIGHCONTRAST,
                contrast.cbSize,
                (&mut contrast as *mut HIGHCONTRASTW).cast(),
                0,
            )
        };
        if read == 0 || contrast.dwFlags & HCF_HIGHCONTRASTON == 0 {
            return None;
        }
        // SAFETY: a plain query of a system colour.
        let colour = unsafe { GetSysColor(COLOR_MENUTEXT) };
        // A `COLORREF` is 0x00BBGGRR.
        Some([colour as u8, (colour >> 8) as u8, (colour >> 16) as u8])
    }
}

/// The bidi isolates the UI wraps names and values in (U+2066 to U+2069) are drawn as boxes by the
/// Win32 menu and the tooltip, which do not lay them out, so a label loses them here. The order of
/// a left-to-right value inside right-to-left text is then the system's to decide.
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

/// The system raises no event when the menu opens, so the UI is told of the click that opens it,
/// as macOS's menu tracking tells it, and asks for fresh quotas. The menu opens as the left or the
/// right button is released (`show_menu_on_left_click` in `create`).
pub(super) fn customize(builder: TrayIconBuilder<Wry>) -> TrayIconBuilder<Wry> {
    builder.on_tray_icon_event(|tray, event| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left | MouseButton::Right,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            let _ = tray.app_handle().emit(MENU_OPENED_EVENT, ());
        }
    })
}

/// Nothing to finish; the menu last set is kept, to be built again in the colours of a theme the
/// system changes to (`WM_SETTINGCHANGE` in `menu_loop_subclass`).
pub(super) fn finish_when_opened(_app: &AppHandle, menu: &TrayMenu) {
    *last_menu() = Some(menu.clone());
}

/// The menu last set on the icon.
fn last_menu() -> std::sync::MutexGuard<'static, Option<TrayMenu>> {
    static LAST: std::sync::Mutex<Option<TrayMenu>> = std::sync::Mutex::new(None);
    LAST.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Keeps `MenuTracking` up to date while the menu is open. tray-icon shows the menu with
/// `TrackPopupMenu`, whose modal loop keeps serving the app, IPC included: a menu the UI sent
/// meanwhile would otherwise replace the `HMENU` the loop is showing. Neither tray-icon nor muda
/// says when the menu opens or closes, but the system tells the window that owns it, tray-icon's
/// own, which this subclasses: `WM_ENTERMENULOOP` marks the menu open, and `WM_EXITMENULOOP` starts
/// a timer whose `WM_TIMER` marks it closed and applies the held menu. The same subclass hears the
/// system's theme and contrast changes (`WM_SETTINGCHANGE` for `ImmersiveColorSet` or
/// `SPI_SETHIGHCONTRAST`, and `WM_SYSCOLORCHANGE`), and builds the menu last set
/// again for its pictures to take the new theme's colour. A timer, because the choice
/// made in the menu arrives as a `WM_COMMAND` posted as the loop ends, which muda maps to its item
/// only while that menu is the tray's, and a timer message is not raised while a posted message
/// waits. The held menu is then applied from the event loop rather than inside this window's
/// procedure, which muda's subclass and tray-icon's are part of.
#[cfg(windows)]
pub(super) fn follow_menu_tracking(app: &AppHandle) {
    use windows_sys::Win32::UI::Shell::SetWindowSubclass;

    use crate::diagnostics::diagnostic;

    let Some(tray) = app.tray_by_id(super::TRAY_ID) else {
        return;
    };
    // The handle as a number, as the closure's result has to be `Send`.
    let Ok(hwnd) = tray.with_inner_tray_icon(|inner| inner.window_handle() as usize) else {
        diagnostic!("cannot follow the tray menu: its window is not reachable");
        return;
    };
    // Lives as long as the process, as the subclass does.
    let app = Box::into_raw(Box::new(app.clone())) as usize;
    // SAFETY: called on the main thread, which created tray-icon's window; the procedure and its
    // data stay valid for the window's life.
    let installed = unsafe {
        SetWindowSubclass(
            hwnd as _,
            Some(menu_loop_subclass),
            MENU_LOOP_SUBCLASS_ID,
            app,
        )
    };
    if installed == 0 {
        diagnostic!("cannot follow the tray menu: the window could not be subclassed");
    }
}

/// Nothing to follow off Windows.
#[cfg(not(windows))]
pub(super) fn follow_menu_tracking(_app: &AppHandle) {}

/// Names this module's subclass of tray-icon's window, which muda subclasses too.
#[cfg(windows)]
const MENU_LOOP_SUBCLASS_ID: usize = 0x4f42_4d4c;

/// The timer `WM_EXITMENULOOP` starts on tray-icon's window, whose own timer is 6007.
#[cfg(windows)]
const MENU_CLOSED_TIMER_ID: usize = 0x4f42_4d43;

#[cfg(windows)]
unsafe extern "system" fn menu_loop_subclass(
    hwnd: windows_sys::Win32::Foundation::HWND,
    message: u32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
    _id: usize,
    data: usize,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::Shell::DefSubclassProc;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        KillTimer, SetTimer, SPI_SETHIGHCONTRAST, WM_ENTERMENULOOP, WM_EXITMENULOOP,
        WM_SETTINGCHANGE, WM_SYSCOLORCHANGE, WM_TIMER,
    };

    use super::menu_tracking::tracking;

    // SAFETY: `data` is the handle `follow_menu_tracking` leaked for this subclass.
    let app = unsafe { &*(data as *const AppHandle) };
    match message {
        WM_ENTERMENULOOP => {
            // A menu opened again within the timer's wait is still open: the timer of its last
            // close must not mark it closed.
            // SAFETY: a timer on the window this procedure serves, on its own thread.
            unsafe { KillTimer(hwnd, MENU_CLOSED_TIMER_ID) };
            tracking(app).opened();
        }
        WM_EXITMENULOOP => {
            // SAFETY: a timer on the window this procedure serves, on its own thread.
            unsafe { SetTimer(hwnd, MENU_CLOSED_TIMER_ID, 10, None) };
        }
        WM_TIMER if wparam == MENU_CLOSED_TIMER_ID => {
            // SAFETY: as above.
            unsafe { KillTimer(hwnd, MENU_CLOSED_TIMER_ID) };
            if tracking(app).closed() {
                apply_later(app.clone(), |app| super::menu_tracking::tracking(app).due());
            }
            return 0;
        }
        // SAFETY: the system sends `WM_SETTINGCHANGE` with a NUL-terminated name, or none, in
        // `lparam`.
        WM_SETTINGCHANGE
            if wparam == SPI_SETHIGHCONTRAST as usize || unsafe { names_colour_set(lparam) } =>
        {
            rebuild_later(app.clone());
        }
        WM_SYSCOLORCHANGE => rebuild_later(app.clone()),
        _ => {}
    }
    // SAFETY: passes the message on to the rest of the window's procedures.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

/// Whether the setting `WM_SETTINGCHANGE` names in `lparam` is `ImmersiveColorSet`, the one the
/// system names when the light or dark mode changes.
///
/// # Safety
///
/// `lparam` is null or points to a NUL-terminated UTF-16 string.
#[cfg(windows)]
unsafe fn names_colour_set(lparam: windows_sys::Win32::Foundation::LPARAM) -> bool {
    let name = lparam as *const u16;
    if name.is_null() {
        return false;
    }
    let expected = "ImmersiveColorSet\0".encode_utf16();
    for (at, unit) in expected.enumerate() {
        // SAFETY: the string is read no further than its NUL, which ends the comparison.
        let found = unsafe { *name.add(at) };
        if found != unit {
            return false;
        }
    }
    true
}

/// Builds the menu last set again from the event loop, or leaves it to the menu's close while one
/// is open (`MenuTracking::rebuild`). It is read when the task runs, so a newer one set meanwhile
/// is the one built.
#[cfg(windows)]
fn rebuild_later(app: AppHandle) {
    apply_later(app, |app| {
        let menu = last_menu().clone()?;
        super::menu_tracking::tracking(app).rebuild(menu)
    });
}

/// Applies the menu `due` gives, from the event loop; `due` runs when the task does, so a newer
/// menu sent meanwhile, or a menu opened again, is respected. `run_on_main_thread` runs a task at
/// once when it is called on the main thread, so it is called from the async runtime. The menu
/// held while the menu was open is applied through `MenuTracking::due`.
#[cfg(windows)]
fn apply_later(app: AppHandle, due: impl FnOnce(&AppHandle) -> Option<TrayMenu> + Send + 'static) {
    tauri::async_runtime::spawn(async move {
        let handle = app.clone();
        let _ = handle.run_on_main_thread(move || {
            if let Some(menu) = due(&app) {
                let _ = super::apply(&app, &menu);
            }
        });
    });
}

/// The label is the icon's tooltip, which the notification area also reads out as its name.
pub(super) fn set_icon_label(tray: &TrayIcon, label: &str) {
    let _ = tray.set_tooltip(Some(native_text(label)));
}

/// The dot is part of the picture, so it is up whenever the picture says attention.
pub(super) fn show_dot(_tray: &TrayIcon, _shown: bool) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::{native_text, theme, tint};

    #[test]
    fn a_template_keeps_its_alpha_and_takes_the_colour() {
        let mut rgba = [0, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 128];
        tint(&mut rgba, [255, 255, 255]);
        assert_eq!(
            rgba,
            [255, 255, 255, 255, 255, 255, 255, 0, 255, 255, 255, 128]
        );
    }

    #[test]
    fn a_dark_app_mode_gives_white_marks_and_a_light_or_unread_one_black() {
        assert_eq!(theme::colour_for(Some(0), None), [255; 3]);
        assert_eq!(theme::colour_for(Some(1), None), [0; 3]);
        assert_eq!(theme::colour_for(None, None), [0; 3]);
    }

    #[test]
    fn high_contrast_gives_the_menus_own_text_colour() {
        assert_eq!(
            theme::colour_for(Some(0), Some([255, 255, 0])),
            [255, 255, 0]
        );
        assert_eq!(theme::colour_for(Some(1), Some([0, 0, 0])), [0, 0, 0]);
    }

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
