use std::iter::once;
use std::ptr;

use windows_sys::Win32::Foundation::{GetLastError, ERROR_NOT_FOUND, WIN32_ERROR};
use windows_sys::Win32::Security::Credentials::{CredFree, CredReadW, CRED_TYPE_GENERIC};

use super::Presence;

pub(super) fn lookup(target: &str) -> Presence {
    let target: Vec<u16> = target.encode_utf16().chain(once(0)).collect();
    let mut credential = ptr::null_mut();
    // SAFETY: The target is NUL-terminated. On success WinCred owns the allocation until
    // CredFree, and only the returned pointer is used. The credential blob is never accessed.
    let result = unsafe {
        if CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) != 0 {
            CredFree(credential.cast());
            Ok(())
        } else {
            Err(GetLastError())
        }
    };
    presence_of(result)
}

fn presence_of(result: Result<(), WIN32_ERROR>) -> Presence {
    match result {
        Ok(()) => Presence::Present,
        Err(ERROR_NOT_FOUND) => Presence::Absent,
        Err(_) => Presence::CannotTell,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Foundation::ERROR_NO_SUCH_LOGON_SESSION;

    #[test]
    fn only_a_missing_target_proves_absence() {
        assert_eq!(presence_of(Ok(())), Presence::Present);
        assert_eq!(presence_of(Err(ERROR_NOT_FOUND)), Presence::Absent);
        assert_eq!(
            presence_of(Err(ERROR_NO_SUCH_LOGON_SESSION)),
            Presence::CannotTell
        );
    }
}
