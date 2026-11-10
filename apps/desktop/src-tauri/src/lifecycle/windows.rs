//! The Windows side of the application's life. This module serves every target but macOS, Linux
//! included. The window's close request and the tray's Quit go through the shared exit flow alone
//! (`handle_close_request` and `request_quit`), and running in the background hides the window and
//! leaves the tray icon, from which `show_main_window` brings it back. There is no Dock to leave or
//! come back to, so the hooks around them are empty. Windows ends the shell before its daemon at
//! sign-out and shutdown, so the shared exit flow can stop the daemon and save its previews first.
//! A second launch is turned back and brings the running window forward.

use tauri::{AppHandle, RunEvent, WebviewWindow, Window, Wry};

use crate::background::show_main_window;

/// Nothing to keep. Not a unit struct, so `run` manages it the same way on every platform.
#[derive(Default)]
pub struct LifecycleState {}

/// Registers the single-instance plugin, which has to come before every other plugin: a second
/// launch hands the running instance its arguments and ends, and the running instance shows its
/// window. Without a window yet (the daemon is still starting) there is nothing to show.
pub fn register_first(builder: tauri::Builder<Wry>) -> tauri::Builder<Wry> {
    builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
        // A login entry that fires while the app already runs is no one asking for the window.
        if !crate::launch_at_login::has_login_argument(&args) {
            show_main_window(app);
        }
    }))
}

pub fn install(_app: &AppHandle) {
    #[cfg(windows)]
    if let Err(err) = set_session_end_order() {
        crate::diagnostics::diagnostic!("cannot set the desktop's session-end order: {err}");
    }
}

/// The shell must receive its end-session notice before Windows ends the daemon's hidden
/// console. Both otherwise start at the default level, 0x280, so the daemon can be gone before
/// the shell's bounded shutdown request reaches it. Windows ends higher levels first
/// (Microsoft Learn, `SetProcessShutdownParameters`); 0x300 is in the application's first range.
#[cfg(windows)]
fn set_session_end_order() -> std::io::Result<()> {
    use windows_sys::Win32::System::Threading::SetProcessShutdownParameters;

    // SAFETY: changes only this process's shutdown order, before its daemon is started.
    if unsafe { SetProcessShutdownParameters(0x300, 0) } == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub fn on_run_event(_app: &AppHandle, _event: &RunEvent) {}

/// Never: the window is hidden as it is.
pub fn hide_after_leaving_fullscreen(_window: &Window, _shown: u64) -> bool {
    false
}

pub fn window_hidden(_app: &AppHandle, _shown: u64) {}

pub fn before_show(_app: &AppHandle, _window: &WebviewWindow) {}

#[cfg(all(test, windows))]
mod tests {
    use windows_sys::Win32::System::Threading::{
        GetProcessShutdownParameters, SetProcessShutdownParameters,
    };

    fn parameters() -> (u32, u32) {
        let (mut level, mut flags) = (0, 0);
        // SAFETY: both output pointers remain valid for the call.
        assert_ne!(
            unsafe { GetProcessShutdownParameters(&mut level, &mut flags) },
            0
        );
        (level, flags)
    }

    #[test]
    fn the_desktop_precedes_default_processes_at_session_end() {
        struct Restore(u32, u32);
        impl Drop for Restore {
            fn drop(&mut self) {
                // SAFETY: restores only this test process's original shutdown order.
                unsafe { SetProcessShutdownParameters(self.0, self.1) };
            }
        }
        let (level, flags) = parameters();
        let _restore = Restore(level, flags);
        super::set_session_end_order().unwrap();
        let (desktop, flags) = parameters();
        assert!(
            desktop > 0x280,
            "the daemon's default order must follow the desktop"
        );
        assert_eq!(flags, 0, "the system's shutdown policy must stay in force");
    }
}
