//! The main window's frame: what the platform draws around the page and behind it, put on the
//! window's builder by `decorate`, what the system frame adds around the page (`system_frame`), and
//! the webview's own window-level behaviour that the builder cannot set, put on the built window by
//! `install`.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(not(target_os = "macos"))]
mod windows;
#[cfg(not(target_os = "macos"))]
use windows as platform;

pub use platform::{decorate, install, system_frame};

/// What the system's window frame does to the window's geometry, in logical points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SystemFrame {
    /// What the frame adds around the inner size, as (width, height): the window's outer size is
    /// its inner size plus this, so a frame placed inside a work area has to leave room for it.
    pub extra: (f64, f64),
    /// Whether the system itself fits a new window onto its screen, so a default size larger than
    /// the work area needs no help from the app.
    pub fits_new_window: bool,
}

/// Whether the webview keeps the browser's own accelerator keys (reload, print, find on page and
/// the like): only in a debug build, where reloading the page and finding in it help debugging. A
/// release build turns them off, since a reload drops the UI's state and the rest are the browser's,
/// not the app's; the editing keys (copy, paste, undo) are not among them.
#[cfg(any(windows, test))]
fn browser_accelerator_keys(debug_build: bool) -> bool {
    debug_build
}

#[cfg(test)]
mod tests {
    use super::browser_accelerator_keys;

    #[test]
    fn a_release_build_turns_the_browser_accelerator_keys_off() {
        assert!(!browser_accelerator_keys(false));
    }

    #[test]
    fn a_debug_build_keeps_the_browser_accelerator_keys_for_debugging() {
        assert!(browser_accelerator_keys(true));
    }
}
