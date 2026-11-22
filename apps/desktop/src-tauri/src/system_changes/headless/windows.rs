//! The Windows side of the clean-up before the uninstall. This module serves every target but
//! macOS, Linux included, where nothing starts the clean-up and no application is found running.

/// The installer leaves an uninstaller, which starts the executable for the clean-up before it
/// removes anything.
pub fn has_uninstaller() -> bool {
    true
}

/// Whether the application, whose bundle identifier is `identifier`, is running: its
/// single-instance lock is there. A lock that cannot be asked for counts as held.
pub fn application_running(identifier: &str) -> bool {
    #[cfg(windows)]
    {
        lock_exists(&super::single_instance_lock_name(identifier))
    }
    #[cfg(not(windows))]
    {
        let _ = identifier;
        false
    }
}

/// Whether a mutex named `name` exists in this session. Opening it takes nothing: the handle is
/// closed at once and the mutex is never waited for.
#[cfg(windows)]
fn lock_exists(name: &str) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_FILE_NOT_FOUND};
    use windows_sys::Win32::System::Threading::{OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE};

    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `wide` is a NUL-terminated string that outlives the call, and the handle, when one
    // is returned, is closed here and nowhere else.
    unsafe {
        let handle = OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, 0, wide.as_ptr());
        if handle.is_null() {
            return GetLastError() != ERROR_FILE_NOT_FOUND;
        }
        CloseHandle(handle);
        true
    }
}
