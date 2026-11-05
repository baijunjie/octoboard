//! The window frame on Windows: the system's own titlebar and frame above the page, which is what
//! the window's builder gives by default and what `host_facts.rs` announces as the `native` chrome.
//! This module serves every target but macOS, Linux included.

use super::SystemFrame;
use crate::MainWindowBuilder;

/// What the frame adds around the inner size at 96 DPI (the titlebar and borders of a standard
/// Windows 10 or 11 window), for when the system cannot be asked, and for the targets where there is
/// no system frame to ask about.
#[cfg(windows)]
const FALLBACK_EXTRA: (f64, f64) = (16.0, 39.0);
#[cfg(not(windows))]
const FALLBACK_EXTRA: (f64, f64) = (0.0, 0.0);

pub fn decorate(builder: MainWindowBuilder<'_>) -> MainWindowBuilder<'_> {
    builder
}

/// Turns WebView2's browser accelerator keys off in a release build (`browser_accelerator_keys`
/// says why). Tauri's builder has no setting for them, so they are set on the built webview. A
/// failure is logged and the window keeps the keys.
#[cfg(windows)]
pub fn install(window: &tauri::WebviewWindow) {
    use crate::diagnostics::diagnostic;

    let enabled = super::browser_accelerator_keys(cfg!(debug_assertions));
    // Runs at once when called on the main thread, which is where the window is built.
    let queued = window.with_webview(move |webview| {
        if let Err(err) = set_browser_accelerator_keys(&webview.controller(), enabled) {
            diagnostic!("cannot set the webview's browser accelerator keys: {err}");
        }
    });
    if let Err(err) = queued {
        diagnostic!("cannot reach the window's webview to set its browser accelerator keys: {err}");
    }
}

#[cfg(not(windows))]
pub fn install(_window: &tauri::WebviewWindow) {}

#[cfg(windows)]
fn set_browser_accelerator_keys(
    controller: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Controller,
    enabled: bool,
) -> windows::core::Result<()> {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
    use windows::core::Interface;

    // SAFETY: COM calls on the main thread, which owns the webview.
    unsafe {
        controller
            .CoreWebView2()?
            .Settings()?
            .cast::<ICoreWebView2Settings3>()?
            .SetAreBrowserAcceleratorKeysEnabled(enabled)
    }
}

/// Win32 does not fit a new window onto its screen, so a default size that does not fit the work
/// area has to be fitted by the app. `extra` is what `AdjustWindowRectExForDpi` grows a zero-sized
/// client area by for the builder's default style, asked at 96 DPI so the result is already in
/// logical points whatever the display's scale is.
pub fn system_frame() -> SystemFrame {
    SystemFrame {
        extra: system_extra().unwrap_or(FALLBACK_EXTRA),
        fits_new_window: false,
    }
}

#[cfg(windows)]
fn system_extra() -> Option<(f64, f64)> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::HiDpi::AdjustWindowRectExForDpi;
    use windows_sys::Win32::UI::WindowsAndMessaging::WS_OVERLAPPEDWINDOW;

    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: `rect` is a valid, exclusively borrowed `RECT` for the duration of the call.
    let adjusted = unsafe { AdjustWindowRectExForDpi(&mut rect, WS_OVERLAPPEDWINDOW, 0, 0, 96) };
    (adjusted != 0).then(|| {
        (
            f64::from(rect.right - rect.left),
            f64::from(rect.bottom - rect.top),
        )
    })
}

#[cfg(not(windows))]
fn system_extra() -> Option<(f64, f64)> {
    None
}
