//! This device's own SSH key for a remote daemon: after a first login with the remote user's own
//! password or key, and once the owner has agreed in the window, the application makes an Ed25519
//! key pair for that entry, keeps the private key in the system's credential store
//! (`credentials`), appends the public key to the remote user's authorized keys
//! (`authorized_keys`), marked with this device's name, and connects with it from then on. The
//! private key never leaves the device: only the public key is sent, and the credential store is
//! the only place it is written to. The line is taken out of the host's file again, and the
//! private key out of the credential store, when the key is given up (`key_removal`).

use std::sync::Arc;

use russh::keys::ssh_key::LineEnding;
use russh::keys::{Algorithm, PrivateKey};

use super::credentials::Store;
use crate::diagnostics::diagnostic;

/// The longest device name an installed key is marked with.
const MAX_NAME_CHARS: usize = 64;

/// A new key pair.
pub fn generate() -> Result<PrivateKey, String> {
    PrivateKey::random(&mut russh::keys::key::safe_rng(), Algorithm::Ed25519)
        .map_err(|err| err.to_string())
}

/// The device key kept for the entry `id`; `None` when none is, or it cannot be read.
pub fn load(store: &dyn Store, service: &str, id: &str) -> Option<Arc<PrivateKey>> {
    let bytes = match store.get(service, id) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return None,
        Err(err) => {
            diagnostic!("cannot read the device key of {id}: {err}");
            return None;
        }
    };
    let decoded = std::str::from_utf8(&bytes)
        .map_err(|err| err.to_string())
        .and_then(|text| PrivateKey::from_openssh(text).map_err(|err| err.to_string()));
    match decoded {
        Ok(key) => Some(Arc::new(key)),
        Err(err) => {
            diagnostic!("ignoring the unreadable device key of {id}: {err}");
            None
        }
    }
}

/// A device key for an entry, and whether it was made now rather than found kept.
pub struct Kept {
    pub key: Arc<PrivateKey>,
    /// Made and put in the credential store by this call: whoever asked for it removes it again
    /// where the install it was made for does not complete.
    pub made: bool,
}

/// The device key kept for the entry `id`, made and kept now when there is none.
pub fn load_or_make(store: &dyn Store, service: &str, id: &str) -> Result<Kept, String> {
    if let Some(key) = load(store, service, id) {
        return Ok(Kept { key, made: false });
    }
    let key = generate()?;
    let encoded = key
        .to_openssh(LineEnding::LF)
        .map_err(|err| err.to_string())?;
    store.set(service, id, encoded.as_bytes())?;
    Ok(Kept {
        key: Arc::new(key),
        made: true,
    })
}

/// Removes the device key of the entry `id` from the credential store; an entry with none kept is
/// no failure. A store that does not remove it (a locked keychain, say) says why, so that nobody
/// is told a key is gone that is still there.
pub fn forget(store: &dyn Store, service: &str, id: &str) -> Result<(), String> {
    store.delete(service, id)
}

/// What the key is marked with in `authorized_keys`: `Octoboard on <device>`, on one line.
pub fn comment(device: &str) -> String {
    let name: String = device
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_NAME_CHARS)
        .collect();
    match name.trim() {
        "" => "Octoboard".to_owned(),
        name => format!("Octoboard on {name}"),
    }
}

/// The public key's base64 body, which tells an installed copy apart whatever it is marked with.
pub fn public_body(key: &PrivateKey) -> Result<String, String> {
    let line = key
        .public_key()
        .to_openssh()
        .map_err(|err| err.to_string())?;
    line.split_whitespace()
        .nth(1)
        .map(str::to_owned)
        .ok_or_else(|| "the public key has no body".to_owned())
}

/// The `authorized_keys` line of `key`, marked with `mark` (`comment`).
pub fn authorized_line(key: &PrivateKey, mark: &str) -> Result<String, String> {
    let body = public_body(key)?;
    Ok(format!("{} {body} {mark}", key.algorithm().as_str()))
}

/// This device's name, as the owner knows it: the computer name on macOS and Windows, else the
/// host name.
pub fn device_name() -> String {
    platform::computer_name()
        .or_else(|| command_line("hostname", &[]))
        .unwrap_or_default()
}

#[cfg(target_os = "macos")]
mod macos {
    /// The computer name System Settings shows.
    pub fn computer_name() -> Option<String> {
        super::command_line("/usr/sbin/scutil", &["--get", "ComputerName"])
    }
}
#[cfg(target_os = "macos")]
use macos as platform;

/// Every target but macOS, Linux included, where the host name stands for the computer name.
#[cfg(not(target_os = "macos"))]
mod windows {
    /// The computer name Windows shows.
    #[cfg(windows)]
    pub fn computer_name() -> Option<String> {
        std::env::var("COMPUTERNAME")
            .ok()
            .filter(|name| !name.trim().is_empty())
    }

    #[cfg(not(windows))]
    pub fn computer_name() -> Option<String> {
        None
    }
}
#[cfg(not(target_os = "macos"))]
use windows as platform;

/// The first line `program` prints, when it succeeds and prints one.
fn command_line(program: &str, args: &[&str]) -> Option<String> {
    #[cfg(test)]
    let _spawning = crate::spawning();
    let output = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next()?.trim();
    (!line.is_empty()).then(|| line.to_owned())
}

/// A credential store that keeps what it is given and reads it back, and refuses to remove
/// anything, as a keychain that has locked since would.
#[cfg(test)]
#[derive(Default)]
pub struct KeepingStore(pub super::credentials::Memory);

#[cfg(test)]
impl Store for KeepingStore {
    fn get(&self, service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
        self.0.get(service, account)
    }
    fn set(&self, service: &str, account: &str, secret: &[u8]) -> Result<(), String> {
        self.0.set(service, account, secret)
    }
    fn delete(&self, _: &str, _: &str) -> Result<(), String> {
        Err("the keychain is locked".into())
    }
}

#[cfg(test)]
mod tests {
    use super::super::credentials::Memory;
    use super::*;

    /// A store whose every call fails, as a locked keychain's would.
    struct Failing;

    impl Store for Failing {
        fn get(&self, _: &str, _: &str) -> Result<Option<Vec<u8>>, String> {
            Err("locked".into())
        }
        fn set(&self, _: &str, _: &str, _: &[u8]) -> Result<(), String> {
            Err("locked".into())
        }
        fn delete(&self, _: &str, _: &str) -> Result<(), String> {
            Err("locked".into())
        }
    }

    #[test]
    fn a_key_is_made_once_kept_and_read_back() {
        let store = Memory::default();
        assert!(load(&store, "svc", "a").is_none());
        let made = load_or_make(&store, "svc", "a").unwrap();
        assert!(made.made);
        assert_eq!(made.key.algorithm(), Algorithm::Ed25519);
        // Found kept the next time, which says so: it is not this caller's to remove.
        let again = load_or_make(&store, "svc", "a").unwrap();
        assert!(!again.made);
        assert_eq!(made.key.public_key(), again.key.public_key());
        let other = load_or_make(&store, "svc", "b").unwrap();
        assert_ne!(made.key.public_key(), other.key.public_key());
        assert_eq!(forget(&store, "svc", "a"), Ok(()));
        assert!(load(&store, "svc", "a").is_none());
        assert_eq!(store.accounts(), vec!["b".to_owned()]);
        // Forgetting what is not kept is no failure.
        assert_eq!(forget(&store, "svc", "a"), Ok(()));
    }

    #[test]
    fn an_unreadable_kept_key_counts_as_none_and_is_replaced() {
        let store = Memory::default();
        store.set("svc", "a", b"not a key").unwrap();
        assert!(load(&store, "svc", "a").is_none());
        let made = load_or_make(&store, "svc", "a").unwrap();
        assert!(made.made);
        assert_eq!(
            load(&store, "svc", "a").unwrap().public_key(),
            made.key.public_key()
        );
    }

    #[test]
    fn the_installed_line_is_marked_with_the_device_name_on_one_line() {
        assert_eq!(
            comment("  Ann's\tMacBook\nPro "),
            "Octoboard on Ann's MacBook Pro"
        );
        assert_eq!(comment(" \u{7} "), "Octoboard");
        assert_eq!(comment(&"x".repeat(100)).len(), "Octoboard on ".len() + 64);
        let key = generate().unwrap();
        let line = authorized_line(&key, &comment("Desk")).unwrap();
        let body = public_body(&key).unwrap();
        assert_eq!(line, format!("ssh-ed25519 {body} Octoboard on Desk"));
        assert!(!line.contains('\n'));
    }

    #[test]
    fn this_device_has_a_name() {
        assert!(!device_name().trim().is_empty());
    }

    #[test]
    fn a_credential_store_that_cannot_be_used_leaves_no_key_and_says_why_nothing_was_removed() {
        assert!(load(&Failing, "svc", "a").is_none());
        // The error reaches the installing step, whose caller opens the window all the same.
        assert_eq!(
            load_or_make(&Failing, "svc", "a").err(),
            Some("locked".to_owned())
        );
        // A key that could not be removed is said to be still there.
        assert_eq!(forget(&Failing, "svc", "a"), Err("locked".to_owned()));
        let keeping = KeepingStore::default();
        let kept = load_or_make(&keeping, "svc", "a").unwrap();
        assert_eq!(
            forget(&keeping, "svc", "a"),
            Err("the keychain is locked".to_owned())
        );
        assert_eq!(
            load(&keeping, "svc", "a").unwrap().public_key(),
            kept.key.public_key()
        );
    }

    #[test]
    fn the_key_is_kept_by_the_credential_store_and_only_its_public_half_is_in_its_line() {
        let store = Memory::default();
        let key = load_or_make(&store, "svc", "a").unwrap().key;
        let kept = store.get("svc", "a").unwrap().expect("kept");
        assert!(String::from_utf8(kept).unwrap().contains("PRIVATE KEY"));
        let line = authorized_line(&key, &comment("Desk")).unwrap();
        assert!(!line.contains("PRIVATE"), "{line}");
        assert_eq!(line.split_whitespace().next(), Some("ssh-ed25519"));
        assert_eq!(
            line.split_whitespace().nth(1),
            Some(public_body(&key).unwrap().as_str())
        );
    }

    #[test]
    fn each_remote_daemon_has_a_key_of_its_own_per_credential_store_service() {
        let store = Memory::default();
        let first = load_or_make(&store, "svc", "a").unwrap().key;
        let other_service = load_or_make(&store, "svc.dev", "a").unwrap().key;
        assert_ne!(first.public_key(), other_service.public_key());
    }
}
