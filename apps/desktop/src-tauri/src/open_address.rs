//! Opening an address with the system's handler for its scheme: a System Settings pane
//! (`x-apple.systempreferences:`) on macOS, a Settings page (`ms-settings:`) on Windows, or a
//! remote daemon's own UI in the default browser (`http:` on a loopback port, `remote::browser`).
//! Not through `tauri-plugin-shell`, whose `open` only accepts `http:`, `https:`, `mailto:` and
//! `tel:` addresses, and which the pages call themselves.
//!
//! A downloaded copy of a remote file is opened here as well (`open_file`), by the system's default
//! application for it.
//!
//! A failure never repeats the address: the browser's carries the owner key, and the message may
//! reach a page.

/// Opens `address` with the system's handler for its scheme. Fails with a message when the address
/// cannot be read or the system declines to open it.
pub fn open(address: &str) -> Result<(), String> {
    platform::open(address)
}

/// Opens the file at `path` with this computer's default application for it. Fails with a message
/// when the system declines to open it. The caller has refused what the system would run.
pub fn open_file(path: &std::path::Path) -> Result<(), String> {
    platform::open_file(path)
}

#[cfg(target_os = "macos")]
mod macos {
    pub fn open_file(path: &std::path::Path) -> Result<(), String> {
        use objc2_app_kit::NSWorkspace;
        use objc2_foundation::{NSString, NSURL};

        let path = path
            .to_str()
            .ok_or_else(|| "the path is not valid".to_owned())?;
        if NSWorkspace::sharedWorkspace()
            .openURL(&NSURL::fileURLWithPath(&NSString::from_str(path)))
        {
            Ok(())
        } else {
            Err("the system did not open the file".to_owned())
        }
    }

    pub fn open(address: &str) -> Result<(), String> {
        use objc2_app_kit::NSWorkspace;
        use objc2_foundation::{NSString, NSURL};

        let Some(parsed) = NSURL::URLWithString(&NSString::from_str(address)) else {
            return Err("the address is not valid".to_owned());
        };
        if NSWorkspace::sharedWorkspace().openURL(&parsed) {
            Ok(())
        } else {
            Err("the system did not open the address".to_owned())
        }
    }
}
#[cfg(target_os = "macos")]
use macos as platform;

/// Every target but macOS, Linux included: a development build on Linux uses the desktop's opener.
#[cfg(not(target_os = "macos"))]
mod windows {
    #[cfg(windows)]
    pub fn open(address: &str) -> Result<(), String> {
        use std::ptr::{null, null_mut};
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain([0]).collect() };
        let (verb, address) = (wide("open"), wide(address));
        // SAFETY: both strings are NUL-terminated and outlive the call.
        let result = unsafe {
            ShellExecuteW(
                null_mut(),
                verb.as_ptr(),
                address.as_ptr(),
                null(),
                null(),
                SW_SHOWNORMAL,
            )
        };
        // The call reports success with a value above 32 and an error code at or below it.
        if result as usize > 32 {
            Ok(())
        } else {
            Err(format!(
                "the system did not open the address (code {})",
                result as usize
            ))
        }
    }

    #[cfg(windows)]
    pub fn open_file(path: &std::path::Path) -> Result<(), String> {
        let path = path
            .to_str()
            .ok_or_else(|| "the path is not valid".to_owned())?;
        open(path)
    }

    #[cfg(not(windows))]
    pub fn open_file(path: &std::path::Path) -> Result<(), String> {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map(drop)
            .map_err(|err| format!("the system did not open the file: {err}"))
    }

    #[cfg(not(windows))]
    pub fn open(address: &str) -> Result<(), String> {
        std::process::Command::new("xdg-open")
            .arg(address)
            .spawn()
            .map(drop)
            .map_err(|err| format!("the system did not open the address: {err}"))
    }
}
#[cfg(not(target_os = "macos"))]
use windows as platform;
