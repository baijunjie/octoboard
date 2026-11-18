//! Whether the host has an application registered for a URL scheme, and the launch that opens a
//! URL with it. A desktop application of an agent registers its scheme when it is installed, so
//! asking the host about the scheme finds the application wherever it lives, without knowing its
//! path. macOS asks Launch Services, Windows the shell's association lookup for a protocol (which
//! also finds a packaged application); no other host reports one. Choosing the launch is a pure
//! function of an OS name, so every host's choice is tested on any machine; starting it is the
//! caller's, and Windows opens a URL through the shell's own call (`paths::shell_open`).

use std::process::Command;

/// Whether some application on this host handles URLs of `scheme` (without the colon).
pub fn url_scheme_handled(scheme: &str) -> bool {
    host::handled(scheme)
}

/// The command that opens `url` with the application registered for its scheme on `os`: macOS
/// `open`, with the URL as one argument after the end of options. `Err` for a host that has none
/// (Windows opens through the shell's own call instead, and no other host reports an application).
pub fn url_launch(url: &str) -> Result<Command, String> {
    url_launch_for(std::env::consts::OS, url)
}

fn url_launch_for(os: &str, url: &str) -> Result<Command, String> {
    match os {
        "macos" => {
            let mut command = Command::new("/usr/bin/open");
            command.arg("--").arg(url);
            Ok(command)
        }
        other => Err(format!(
            "opening an application's URL is not supported on {other}"
        )),
    }
}

#[cfg(target_os = "macos")]
mod host {
    use std::ffi::c_void;

    use crate::platform::cf::Object;

    #[link(name = "CoreServices", kind = "framework")]
    extern "C" {
        /// The application that opens `url` in `roles`, retained, or null when none does.
        fn LSCopyDefaultApplicationURLForURL(
            url: *const c_void,
            roles: u32,
            error: *mut *const c_void,
        ) -> *const c_void;
    }

    /// `kLSRolesAll`.
    const ROLES_ALL: u32 = 0xFFFF_FFFF;

    pub(super) fn handled(scheme: &str) -> bool {
        let Some(url) = Object::string(&format!("{scheme}://")).and_then(|string| string.url())
        else {
            return false;
        };
        let mut error = std::ptr::null();
        // SAFETY: the URL outlives the call and `error` is a valid place for an error it may make.
        let application =
            unsafe { LSCopyDefaultApplicationURLForURL(url.as_ptr(), ROLES_ALL, &mut error) };
        // SAFETY: both are null or retained objects the call made, which nothing else releases.
        let (application, _error) = unsafe { (Object::own(application), Object::own(error)) };
        application.is_some()
    }
}

#[cfg(windows)]
mod host {
    use windows_sys::Win32::UI::Shell::{
        AssocQueryStringW, ASSOCF_IS_PROTOCOL, ASSOCSTR, ASSOCSTR_APPID, ASSOCSTR_COMMAND,
    };

    /// What the shell can say of a protocol's handler, in the order asked. A classic application
    /// registers a command; a packaged one (MSIX) registers none, being activated by its
    /// application model id, so the command alone would miss it and its id answers instead. The
    /// handler's display name is not asked: a name can be answered for a scheme nothing handles.
    const ANSWERS: [ASSOCSTR; 2] = [ASSOCSTR_COMMAND, ASSOCSTR_APPID];

    /// Whether the shell knows a handler for the protocol `scheme`: any of what [`ANSWERS`] names
    /// is answered for it. Each is asked for its size only, which a handler that has it answers
    /// with a success code (`S_FALSE`, and the length). Not verified on a machine with a packaged
    /// handler.
    pub(super) fn handled(scheme: &str) -> bool {
        let protocol: Vec<u16> = scheme.encode_utf16().chain(std::iter::once(0)).collect();
        ANSWERS.into_iter().any(|what| {
            let mut length = 0u32;
            // SAFETY: the protocol is NUL-terminated and outlives the call, the extra string and
            // the output buffer are null (asking for the length), and `length` is a valid place
            // for it.
            let result = unsafe {
                AssocQueryStringW(
                    ASSOCF_IS_PROTOCOL,
                    what,
                    protocol.as_ptr(),
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    &mut length,
                )
            };
            result >= 0
        })
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod host {
    pub(super) fn handled(_scheme: &str) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_opens_the_url_as_one_argument_after_the_end_of_options() {
        let command = url_launch_for("macos", "example://thread/abc").unwrap();
        assert_eq!(command.get_program(), "/usr/bin/open");
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args, ["--", "example://thread/abc"]);
    }

    #[test]
    fn windows_has_no_command_for_it_and_opens_through_the_shell() {
        assert!(url_launch_for("windows", "example://thread/abc").is_err());
    }

    #[test]
    fn another_host_has_no_launch() {
        assert!(url_launch_for("linux", "example://thread/abc").is_err());
    }

    #[test]
    fn a_scheme_nothing_registers_is_not_handled() {
        assert!(!url_scheme_handled("octoboard-test-no-such-scheme"));
    }
}
