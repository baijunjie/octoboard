//! The system's credential store, where each remote daemon's device key is kept: the login keychain
//! on macOS, as a generic password, and the Credential Manager on Windows, as a generic credential.
//! An item is named by a service (the application's identifier, so that two builds with their own
//! identifiers keep apart) and an account (the remote daemon entry's id). Nothing else of the
//! remote daemons is kept here, and no password ever is.

/// A place secrets are kept, by service and account.
pub trait Store: Send + Sync {
    /// The secret kept for `account`; `None` when there is none.
    fn get(&self, service: &str, account: &str) -> Result<Option<Vec<u8>>, String>;
    /// Keeps `secret` for `account`, replacing what was kept for it.
    fn set(&self, service: &str, account: &str, secret: &[u8]) -> Result<(), String>;
    /// Forgets what is kept for `account`; nothing kept is no error.
    fn delete(&self, service: &str, account: &str) -> Result<(), String>;
}

/// The service device keys are kept under, for the application whose identifier is `identifier`.
pub fn service(identifier: &str) -> String {
    format!("{identifier}.remote-daemon-key")
}

/// The system's credential store.
pub fn system() -> &'static dyn Store {
    &System
}

struct System;

impl Store for System {
    fn get(&self, service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
        platform::get(service, account)
    }

    fn set(&self, service: &str, account: &str, secret: &[u8]) -> Result<(), String> {
        platform::set(service, account, secret)
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), String> {
        platform::delete(service, account)
    }
}

/// The login keychain, as generic passwords.
#[cfg(target_os = "macos")]
mod macos {
    use security_framework::passwords::{
        delete_generic_password, get_generic_password, set_generic_password,
    };

    /// `errSecItemNotFound`.
    const NOT_FOUND: i32 = -25300;

    pub fn get(service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
        match get_generic_password(service, account) {
            Ok(secret) => Ok(Some(secret)),
            Err(err) if err.code() == NOT_FOUND => Ok(None),
            Err(err) => Err(err.to_string()),
        }
    }

    pub fn set(service: &str, account: &str, secret: &[u8]) -> Result<(), String> {
        set_generic_password(service, account, secret).map_err(|err| err.to_string())
    }

    pub fn delete(service: &str, account: &str) -> Result<(), String> {
        match delete_generic_password(service, account) {
            Ok(()) => Ok(()),
            Err(err) if err.code() == NOT_FOUND => Ok(()),
            Err(err) => Err(err.to_string()),
        }
    }
}
#[cfg(target_os = "macos")]
use macos as platform;

/// The Credential Manager, as generic credentials named `<service>:<account>`.
///
/// Every target but macOS, Linux included: a development build on Linux has no store, so nothing is
/// kept and a device key is never installed.
#[cfg(not(target_os = "macos"))]
mod windows {
    #[cfg(windows)]
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_NOT_FOUND};
    #[cfg(windows)]
    use windows_sys::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
        CRED_TYPE_GENERIC,
    };

    /// The credential's target name, NUL-terminated UTF-16.
    #[cfg(windows)]
    fn target(service: &str, account: &str) -> Vec<u16> {
        format!("{service}:{account}")
            .encode_utf16()
            .chain(Some(0))
            .collect()
    }

    #[cfg(windows)]
    fn last_error(what: &str) -> String {
        // SAFETY: reads the calling thread's last error code.
        let code = unsafe { GetLastError() };
        format!("{what} failed with Windows error {code}")
    }

    #[cfg(windows)]
    pub fn get(service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
        let target = target(service, account);
        let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: `target` is NUL-terminated; on success `credential` points at a block the system
        // allocated, read here and freed with `CredFree`.
        unsafe {
            if CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) == 0 {
                if GetLastError() == ERROR_NOT_FOUND {
                    return Ok(None);
                }
                return Err(last_error("CredReadW"));
            }
            let read = &*credential;
            let secret = if read.CredentialBlob.is_null() {
                Vec::new()
            } else {
                std::slice::from_raw_parts(read.CredentialBlob, read.CredentialBlobSize as usize)
                    .to_vec()
            };
            CredFree(credential.cast());
            Ok(Some(secret))
        }
    }

    #[cfg(windows)]
    pub fn set(service: &str, account: &str, secret: &[u8]) -> Result<(), String> {
        let mut target = target(service, account);
        let mut blob = secret.to_vec();
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: target.as_mut_ptr(),
            CredentialBlobSize: u32::try_from(blob.len()).map_err(|err| err.to_string())?,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            ..Default::default()
        };
        // SAFETY: every pointer in `credential` points at a buffer alive for the call.
        if unsafe { CredWriteW(&credential, 0) } == 0 {
            return Err(last_error("CredWriteW"));
        }
        Ok(())
    }

    #[cfg(windows)]
    pub fn delete(service: &str, account: &str) -> Result<(), String> {
        let target = target(service, account);
        // SAFETY: `target` is NUL-terminated.
        unsafe {
            if CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) == 0
                && GetLastError() != ERROR_NOT_FOUND
            {
                return Err(last_error("CredDeleteW"));
            }
        }
        Ok(())
    }

    #[cfg(not(windows))]
    pub fn get(_service: &str, _account: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(None)
    }

    #[cfg(not(windows))]
    pub fn set(_service: &str, _account: &str, _secret: &[u8]) -> Result<(), String> {
        Err("this system has no credential store the application can use".into())
    }

    #[cfg(not(windows))]
    pub fn delete(_service: &str, _account: &str) -> Result<(), String> {
        Ok(())
    }
}
#[cfg(not(target_os = "macos"))]
use windows as platform;

/// A store in memory, for the tests: the system's own is never touched by them.
#[cfg(test)]
#[derive(Default)]
pub struct Memory(std::sync::Mutex<std::collections::HashMap<(String, String), Vec<u8>>>);

#[cfg(test)]
impl Memory {
    pub fn accounts(&self) -> Vec<String> {
        let mut accounts: Vec<String> = self
            .0
            .lock()
            .unwrap()
            .keys()
            .map(|(_, account)| account.clone())
            .collect();
        accounts.sort();
        accounts
    }
}

#[cfg(test)]
impl Store for Memory {
    fn get(&self, service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .get(&(service.to_owned(), account.to_owned()))
            .cloned())
    }

    fn set(&self, service: &str, account: &str, secret: &[u8]) -> Result<(), String> {
        self.0
            .lock()
            .unwrap()
            .insert((service.to_owned(), account.to_owned()), secret.to_vec());
        Ok(())
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), String> {
        self.0
            .lock()
            .unwrap()
            .remove(&(service.to_owned(), account.to_owned()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_builds_with_their_own_identifiers_keep_their_keys_apart() {
        let release = service("app.octoboard.desktop");
        let development = service("app.octoboard.desktop.dev");
        assert_ne!(release, development);
        assert!(release.starts_with("app.octoboard.desktop"), "{release}");
        assert!(release.ends_with("remote-daemon-key"), "{release}");
        let store = Memory::default();
        store.set(&release, "entry", b"release key").unwrap();
        assert_eq!(store.get(&development, "entry").unwrap(), None);
        assert_eq!(
            store.get(&release, "entry").unwrap().as_deref(),
            Some(&b"release key"[..])
        );
    }

    #[test]
    fn a_kept_secret_is_replaced_by_the_next_and_forgetting_what_is_not_kept_is_no_error() {
        let store = Memory::default();
        store.set("svc", "a", b"one").unwrap();
        store.set("svc", "a", b"two").unwrap();
        assert_eq!(store.get("svc", "a").unwrap().as_deref(), Some(&b"two"[..]));
        store.delete("svc", "a").unwrap();
        store.delete("svc", "a").unwrap();
        assert_eq!(store.get("svc", "a").unwrap(), None);
    }
}
