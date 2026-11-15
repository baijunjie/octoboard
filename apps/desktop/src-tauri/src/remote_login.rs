//! Whether the computer's SSH server is on (macOS Remote Login, Windows OpenSSH Server), for the
//! Remote access section of Settings: a browser on another machine reaches the application's daemon
//! through an SSH login to this computer, so remote access is of use only with the SSH server on.
//! The section shows where it stands and opens the system's settings where it is turned on; turning
//! it on needs an administrator, which the application never asks for itself.
//!
//! On macOS, read from `launchctl print-disabled system`, which any user may run (`systemsetup
//! -getremotelogin` needs an administrator): Remote Login on is the system's `com.openssh.sshd`
//! service enabled. On Windows, read from the Service Control Manager, which any user may ask:
//! OpenSSH Server on is its `sshd` service running, and off when the service is stopped or not
//! installed (it is an optional feature of Windows).

/// Whether the SSH server is on: `None` where it cannot be told (Linux, or the system did not
/// answer). Async, since it waits for a process or the Service Control Manager.
#[tauri::command(async)]
pub fn remote_login_status() -> Option<bool> {
    platform::remote_login_status()
}

/// Opens the system's settings where the SSH server is turned on: the Sharing pane of System
/// Settings on macOS, the Optional features page of Settings on Windows, where OpenSSH Server is
/// added. Fails with a message when the system declines to open it, and on Linux.
#[tauri::command]
pub fn open_remote_login_settings() -> Result<(), String> {
    platform::open_remote_login_settings()
}

#[cfg(target_os = "macos")]
mod platform {
    /// Sharing, in System Settings (macOS 13 and later) and System Preferences alike, the pane
    /// that holds Remote Login.
    const SHARING_PANE_URL: &str = "x-apple.systempreferences:com.apple.preferences.sharing";

    pub fn remote_login_status() -> Option<bool> {
        let output = std::process::Command::new("/bin/launchctl")
            .args(["print-disabled", "system"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        Some(super::enabled_in(&String::from_utf8_lossy(&output.stdout)))
    }

    pub fn open_remote_login_settings() -> Result<(), String> {
        crate::open_address::open(SHARING_PANE_URL)
    }
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_SERVICE_DOES_NOT_EXIST};
    use windows_sys::Win32::System::Services::{
        CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceStatus, SC_HANDLE,
        SC_MANAGER_CONNECT, SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_STATUS,
    };

    /// The Settings page where optional features, OpenSSH Server among them, are added.
    const OPTIONAL_FEATURES_PAGE: &str = "ms-settings:optionalfeatures";

    /// The service OpenSSH Server runs as.
    const SERVICE_NAME: &str = "sshd";

    /// Closes a service handle when dropped.
    struct Handle(SC_HANDLE);

    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: the handle was opened and is closed once, here.
            unsafe { CloseServiceHandle(self.0) };
        }
    }

    pub fn remote_login_status() -> Option<bool> {
        let name: Vec<u16> = SERVICE_NAME.encode_utf16().chain([0]).collect();
        // SAFETY: null names the local computer and its active database; the handles are checked
        // before use and closed by `Handle`; `name` is NUL-terminated and outlives the call;
        // `status` is a plain structure the call fills.
        unsafe {
            let manager = OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT);
            if manager.is_null() {
                return None;
            }
            let manager = Handle(manager);
            let service = OpenServiceW(manager.0, name.as_ptr(), SERVICE_QUERY_STATUS);
            if service.is_null() {
                // Not installed is off; anything else cannot be told.
                return (GetLastError() == ERROR_SERVICE_DOES_NOT_EXIST).then_some(false);
            }
            let service = Handle(service);
            let mut status: SERVICE_STATUS = std::mem::zeroed();
            if QueryServiceStatus(service.0, &mut status) == 0 {
                return None;
            }
            Some(status.dwCurrentState == SERVICE_RUNNING)
        }
    }

    pub fn open_remote_login_settings() -> Result<(), String> {
        crate::open_address::open(OPTIONAL_FEATURES_PAGE)
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod platform {
    pub fn remote_login_status() -> Option<bool> {
        None
    }

    pub fn open_remote_login_settings() -> Result<(), String> {
        Err("this system's SSH server is not set up from here".to_owned())
    }
}

/// Whether `listing`, the output of `launchctl print-disabled system`, has the SSH server enabled.
/// A service is listed as `"<label>" => enabled` or `disabled` (`=> false` and `=> true` in
/// earlier releases, the answer to "disabled?"); an unlisted one keeps its own default, which for
/// `com.openssh.sshd` is off.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn enabled_in(listing: &str) -> bool {
    listing.lines().any(|line| {
        let Some((label, value)) = line.split_once("=>") else {
            return false;
        };
        label.trim().trim_matches('"') == "com.openssh.sshd"
            && matches!(value.trim(), "enabled" | "false")
    })
}

#[cfg(test)]
mod tests {
    use super::enabled_in;

    #[test]
    fn remote_login_is_on_only_where_the_ssh_server_is_enabled() {
        let listing = |sshd: &str| {
            format!(
                "disabled services = {{\n\t\"com.apple.ftpd\" => disabled\n\t\"com.openssh.sshd\" => {sshd}\n}}\n"
            )
        };
        assert!(enabled_in(&listing("enabled")));
        assert!(enabled_in(&listing("false")));
        assert!(!enabled_in(&listing("disabled")));
        assert!(!enabled_in(&listing("true")));
        assert!(!enabled_in(
            "disabled services = {\n\t\"com.apple.ftpd\" => enabled\n}\n"
        ));
        assert!(!enabled_in(""));
    }

    #[test]
    fn only_the_ssh_server_s_own_service_counts() {
        // Another service whose label begins with it is not the SSH server.
        assert!(!super::enabled_in(
            "disabled services = {\n\t\"com.openssh.sshd-keygen\" => enabled\n}\n"
        ));
        assert!(!super::enabled_in("\"com.openssh.sshd\" enabled"));
        // Whatever precedes it in the listing is no matter.
        assert!(super::enabled_in(
            "x = {\n\t\"com.apple.a\" => disabled\n\t\"com.openssh.sshd\" => enabled\n\t\"com.apple.z\" => disabled\n}"
        ));
    }
}
