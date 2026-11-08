//! The Windows entry: a value of the current user's `Run` key that starts the app with
//! `--launched-at-login`. Task Manager's Startup apps tab turns it off without removing it, by a
//! value under `StartupApproved\Run`, which is read here and cleared when the user turns the switch
//! on. Linux has no entry.

use super::Status;

/// Whether the `StartupApproved` data (the first byte holds the state: even enabled, odd disabled
/// by the user) says the entry is turned off.
#[cfg_attr(not(windows), allow(dead_code))]
fn approval_disables(data: &[u8]) -> bool {
    data.first().is_some_and(|state| state & 1 == 1)
}

#[cfg(windows)]
mod registry {
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ,
        RRF_RT_REG_BINARY, RRF_RT_REG_SZ,
    };

    pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    pub const APPROVED_KEY: &str =
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain([0]).collect()
    }

    /// The binary or string data of `name` under `key`, `None` when there is none.
    pub fn read(key: &str, name: &str, binary: bool) -> Option<Vec<u8>> {
        let (key, name) = (wide(key), wide(name));
        let flags = if binary {
            RRF_RT_REG_BINARY
        } else {
            RRF_RT_REG_SZ
        };
        let mut size = 0u32;
        // SAFETY: both names are NUL-terminated and outlive the call; no data pointer is given, so
        // the call only reports the size.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                flags,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut size,
            )
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        // SAFETY: `data` holds `size` bytes, which the call is told.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                flags,
                std::ptr::null_mut(),
                data.as_mut_ptr().cast(),
                &mut size,
            )
        };
        (status == ERROR_SUCCESS).then_some(data)
    }

    pub fn write_string(key: &str, name: &str, value: &str) -> Result<(), String> {
        let (key, name, value) = (wide(key), wide(name), wide(value));
        // SAFETY: every string is NUL-terminated, and the size counts the value's bytes with its
        // NUL.
        let status = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                REG_SZ,
                value.as_ptr().cast(),
                (value.len() * 2) as u32,
            )
        };
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(format!(
                "the registry refused the login entry (error {status})"
            ))
        }
    }

    /// Removes `name` under `key`; one that is not there is removed already.
    pub fn delete(key: &str, name: &str) -> Result<(), String> {
        let (key, name) = (wide(key), wide(name));
        // SAFETY: both names are NUL-terminated and outlive the call.
        let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()) };
        if status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(format!(
                "the registry refused to remove the login entry (error {status})"
            ))
        }
    }
}

#[cfg(windows)]
pub fn supported() -> bool {
    true
}

#[cfg(not(windows))]
pub fn supported() -> bool {
    false
}

/// Whether this process was started by the entry: it carries the entry's argument.
pub fn launched_at_login() -> bool {
    super::has_login_argument(std::env::args_os())
}

#[cfg(windows)]
pub fn status() -> Result<Status, String> {
    let name = crate::menu::APP_NAME;
    if registry::read(registry::RUN_KEY, name, false).is_none() {
        return Ok(Status::Disabled);
    }
    let disabled = registry::read(registry::APPROVED_KEY, name, true)
        .is_some_and(|data| approval_disables(&data));
    Ok(if disabled {
        Status::Disabled
    } else {
        Status::Enabled
    })
}

#[cfg(windows)]
pub fn set(enabled: bool) -> Result<Status, String> {
    let name = crate::menu::APP_NAME;
    if enabled {
        let exe = std::env::current_exe().map_err(|err| err.to_string())?;
        let command = format!("\"{}\" {}", exe.display(), super::LOGIN_ARGUMENT);
        registry::write_string(registry::RUN_KEY, name, &command)?;
        // A switch the user turned off in Task Manager would keep the entry from running.
        registry::delete(registry::APPROVED_KEY, name)?;
    } else {
        registry::delete(registry::RUN_KEY, name)?;
    }
    status()
}

pub fn open_settings() -> Result<(), String> {
    Err("this system has no list of login items to open".to_string())
}

#[cfg(not(windows))]
pub fn status() -> Result<Status, String> {
    Ok(Status::Disabled)
}

#[cfg(not(windows))]
pub fn set(_enabled: bool) -> Result<Status, String> {
    Err("this system has no login entry to make".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_manager_turning_the_entry_off_is_read_from_the_approval_data() {
        assert!(!approval_disables(&[0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]));
        assert!(!approval_disables(&[0x06, 0, 0, 0]));
        assert!(approval_disables(&[0x03, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]));
        assert!(approval_disables(&[0x07]));
        assert!(!approval_disables(&[]));
    }
}
