//! The owner key: a second key a browser may present beside the key of the daemon's launch, which
//! outlives the launch. The launch key is new for every start of the daemon, so an address a
//! browser had bookmarked would stop working at each fresh start or planned restart a service
//! manager makes; the owner key stays in an owner-only file of the data directory, `owner.key`,
//! until it is rotated ([`OwnerKey::rotate`], or `octoboardd address --rotate`).
//!
//! It is a second key rather than the launch key kept across starts because the launch key has to
//! stay new for each launch: the command line recognises the daemon a planned restart started
//! itself by its having recorded the very launch key of the daemon it took over from, and the
//! launch key goes into what the daemon's helpers read, which a key kept for good must not.
//!
//! Only a daemon that serves the browser on its owner-only socket holds one ([`OwnerKey::start`],
//! [`crate::browser_socket`]): the desktop application hands its daemon's launch key to its own
//! window and has no address to keep, and a build that embeds no UI has no page to open. The owner
//! key is the browser's alone, and accepted only on that socket. It is never written to
//! `daemon.key`, a helper's key file or a holder's record, and never handed to anything the daemon
//! starts, so what holds the launch key gains no more by it.
//!
//! Each rotation moves a generation on, which every connection admitted by the owner key watches
//! ([`OwnerKey::admit`]): a rotation alone closes them, so a page holding the old key is cut off
//! at once rather than at its next connection.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};

use tokio::sync::watch;

use crate::hostfs::replace::{replace_beside, Permissions};
use crate::request_guard::{generate_key, is_key, key_matches};

/// What the daemon holds of its owner key: the key and the file it is kept in, or nothing, as a
/// daemon that follows the desktop application, or could not read or write the file, does. Shared
/// by the guard, which accepts it, and the route that rotates it.
pub struct OwnerKey {
    held: RwLock<Option<Held>>,
    /// Serializes rotations, so the file is written outside `held` and a request's read of the
    /// key never waits for the disk.
    rotating: Mutex<()>,
    /// Moved on by every rotation, under `held`'s write lock, so a connection that subscribed
    /// while `held` accepted its key sees every rotation after that one and none before it.
    generation: watch::Sender<u64>,
}

impl Default for OwnerKey {
    fn default() -> Self {
        Self {
            held: RwLock::default(),
            rotating: Mutex::default(),
            generation: watch::Sender::new(0),
        }
    }
}

/// What a connection the owner key admitted watches: it resolves once the key that admitted it has
/// been rotated away. A clone watches from where the original stands.
#[derive(Clone)]
pub struct Revocation(watch::Receiver<u64>);

impl Revocation {
    /// Resolves when the owner key has been rotated since this connection was admitted, and never
    /// otherwise: the key's holder outlives every connection, so its end is no rotation.
    pub async fn revoked(&mut self) {
        if self.0.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

struct Held {
    key: String,
    path: PathBuf,
}

/// Why a rotation did not happen.
#[derive(Debug)]
pub enum RotateError {
    /// The daemon holds no owner key.
    NotHeld,
    /// The new key could not be written; the old one stands, in memory and on disk.
    Write(std::io::Error),
}

impl RotateError {
    /// The failure's name, as `POST /owner-key/rotate` answers a refusal.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotHeld => "no_owner_key",
            Self::Write(_) => "write_failed",
        }
    }

    /// What the failure says, in English, for a log or a caller that shows it as it is.
    pub fn message(&self) -> &'static str {
        match self {
            Self::NotHeld => "this daemon holds no owner key",
            Self::Write(_) => "the new owner key could not be written; the old one stands",
        }
    }
}

impl OwnerKey {
    /// Starts to hold the owner key kept in `path`: the key it holds, or a new one written there
    /// when the file is missing or does not hold a key. A file that cannot be read is left as it
    /// is, whatever it holds, and so is one a new key cannot be written to: the daemon holds none.
    pub fn enable(&self, path: &Path) -> std::io::Result<()> {
        let key = match read_existing(path)? {
            Some(key) => key,
            None => replace_at(path)?,
        };
        *self.held.write().unwrap() = Some(Held {
            key,
            path: path.to_path_buf(),
        });
        Ok(())
    }

    /// Starts to hold the owner key of the data directory `data` where the daemon serves the
    /// browser (`serves_browser`): a daemon that follows the desktop application, or embeds no UI,
    /// or could not open its browser socket, has no page to keep an address for, and never reads,
    /// makes or accepts one. Whether it holds one afterwards: a failure to read or write the key is
    /// logged, and the daemon then accepts none.
    pub fn start(&self, serves_browser: bool, data: &Path) -> bool {
        if !serves_browser {
            return false;
        }
        match self.enable(&path_in(data)) {
            Ok(()) => true,
            Err(err) => {
                tracing::warn!(%err, "reading or writing the owner key failed; no owner key is accepted");
                false
            }
        }
    }

    /// Whether the daemon holds an owner key, which is what its owner record says.
    pub fn is_held(&self) -> bool {
        self.held.read().unwrap().is_some()
    }

    /// Whether the query string carries the owner key as its first `key` parameter. Never, for a
    /// daemon that holds none.
    #[cfg(test)]
    pub fn accepts(&self, query: Option<&str>) -> bool {
        self.admit(query).is_some()
    }

    /// The watch on the key's rotation, when the query string carries the owner key as its first
    /// `key` parameter; `None` otherwise, and always for a daemon that holds none. The key is
    /// compared and the watch taken under one read of it, so no rotation falls between the two.
    pub fn admit(&self, query: Option<&str>) -> Option<Revocation> {
        let held = self.held.read().unwrap();
        held.as_ref()
            .is_some_and(|held| key_matches(query, &held.key))
            .then(|| Revocation(self.generation.subscribe()))
    }

    /// Replaces the owner key with a new one, in its file and in memory. The old key is refused
    /// from then on, and every connection it admitted is told to close ([`Revocation`]).
    /// Blocking. One rotation at a time, so two cannot leave the file and the memory with
    /// different keys; the key itself is locked only for the swap in memory, after the file is
    /// written.
    pub fn rotate(&self) -> Result<(), RotateError> {
        let _rotation = self.rotating.lock().unwrap();
        let path = match self.held.read().unwrap().as_ref() {
            Some(held) => held.path.clone(),
            None => return Err(RotateError::NotHeld),
        };
        let key = replace_at(&path).map_err(RotateError::Write)?;
        let mut held = self.held.write().unwrap();
        if let Some(held) = held.as_mut() {
            held.key = key;
        }
        self.generation.send_modify(|generation| *generation += 1);
        Ok(())
    }
}

/// The file the owner key of the daemon whose data directory is `data` is kept in.
pub fn path_in(data: &Path) -> PathBuf {
    data.join("owner.key")
}

/// The owner key kept in the data directory `data`, or `None` when there is none, it is
/// unreadable or it is not a key.
pub fn read_in(data: &Path) -> Option<String> {
    read_from(&path_in(data))
}

/// Replaces the owner key kept in the data directory `data` with a new one, and returns it, for
/// `octoboardd address --rotate` where no daemon is running or the running one does not take it.
pub fn replace_in(data: &Path) -> std::io::Result<String> {
    replace_at(&path_in(data))
}

/// Writes `key` into the data directory `data` as the daemon would, owner-only, for a test that
/// needs a known owner key.
#[cfg(test)]
pub fn write_in(data: &Path, key: &str) {
    replace_beside(
        &path_in(data),
        format!("{key}\n").as_bytes(),
        Permissions::Private,
    )
    .expect("writing the owner key");
}

fn read_from(path: &Path) -> Option<String> {
    read_existing(path).ok().flatten()
}

/// The key kept in `path`: `None` when the file is missing or does not hold a key, an error when
/// it exists but cannot be read. On Unix a file that is not one the daemon's user alone could have
/// written holds no key either, whatever it says: a symlink (not followed), a file another user
/// owns, or one whose mode lets anyone else read or write it. Such a file is replaced with a new
/// key, as a malformed one is, since a key another user could have read or chosen is no secret.
fn read_existing(path: &Path) -> std::io::Result<Option<String>> {
    let content = match crate::platform::fs::read_private_file(path) {
        Ok(Some(content)) => content,
        Ok(None) => return Ok(None),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };
    // A file that is not UTF-8 is not a key either.
    let Ok(content) = String::from_utf8(content) else {
        return Ok(None);
    };
    let key = content.trim();
    Ok(is_key(key).then(|| key.to_owned()))
}

/// Writes a new key to `path`, whole and owner-only as `daemon.key` is, and returns it.
fn replace_at(path: &Path) -> std::io::Result<String> {
    let key = generate_key()?;
    replace_beside(path, format!("{key}\n").as_bytes(), Permissions::Private)?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;

    fn query(key: &str) -> Option<String> {
        Some(format!("key={key}"))
    }

    /// A daemon that does not serve the browser (it follows the desktop application, embeds no UI
    /// or has no browser socket) neither makes nor reads an owner key, so even the key the file
    /// holds opens nothing there; a daemon that serves it over the same directory holds it.
    #[test]
    fn only_a_daemon_that_serves_the_browser_takes_an_owner_key() {
        let dir = ScratchDir::new("owner-key-desktop");
        let desktop = OwnerKey::default();
        assert!(!desktop.start(false, &dir));
        assert!(!desktop.is_held());
        assert_eq!(read_in(&dir), None, "no owner key was made");

        let serving = OwnerKey::default();
        assert!(serving.start(true, &dir));
        assert!(serving.is_held());
        let key = read_in(&dir).expect("a daemon serving the browser made the key");

        let desktop = OwnerKey::default();
        desktop.start(false, &dir);
        assert!(!desktop.accepts(query(&key).as_deref()));
        assert_eq!(
            read_in(&dir).as_deref(),
            Some(key.as_str()),
            "the key is left as it was"
        );
    }

    /// A daemon started again over the same data directory holds the key the file already has.
    #[test]
    fn a_restart_over_the_same_data_directory_keeps_the_owner_key() {
        let dir = ScratchDir::new("owner-key-restart");
        let path = path_in(&dir);
        let first = OwnerKey::default();
        first.enable(&path).unwrap();
        let key = read_in(&dir).expect("the key was written");
        assert!(first.accepts(query(&key).as_deref()));

        let second = OwnerKey::default();
        second.enable(&path).unwrap();
        assert_eq!(read_in(&dir).as_deref(), Some(key.as_str()));
        assert!(second.accepts(query(&key).as_deref()));
    }

    #[test]
    fn a_missing_or_malformed_file_gets_a_new_key() {
        let dir = ScratchDir::new("owner-key-malformed");
        let path = path_in(&dir);
        for content in [None, Some(""), Some("not a key\n"), Some("ABCDEF\n")] {
            if let Some(content) = content {
                std::fs::write(&path, content).unwrap();
            }
            let held = OwnerKey::default();
            held.enable(&path).unwrap();
            let key = read_in(&dir).unwrap_or_else(|| panic!("no key after {content:?}"));
            assert!(is_key(&key));
            assert!(held.accepts(query(&key).as_deref()), "{content:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = ScratchDir::new("owner-key-mode");
        OwnerKey::default().enable(&path_in(&dir)).unwrap();
        let mode = std::fs::metadata(path_in(&dir))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    /// A file that exists but cannot be read is not replaced: the key in it may be a good one.
    #[cfg(unix)]
    #[test]
    fn an_unreadable_file_is_left_alone_and_no_key_is_held() {
        use std::os::unix::fs::PermissionsExt;

        let dir = ScratchDir::new("owner-key-unreadable");
        let path = path_in(&dir);
        let key = replace_in(&dir).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        // A process that bypasses the mode (root) reads it.
        if std::fs::read(&path).is_ok() {
            return;
        }
        let held = OwnerKey::default();
        assert!(!held.start(true, &dir));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(!held.is_held());
        assert!(!held.accepts(query(&key).as_deref()));
        assert_eq!(
            read_in(&dir).as_deref(),
            Some(key.as_str()),
            "the file is untouched"
        );
    }

    /// A file that is not UTF-8 is not a key: it is replaced with a new one, not an error.
    #[test]
    fn a_file_that_is_not_utf8_is_replaced_with_a_key() {
        let dir = ScratchDir::new("owner-key-not-utf8");
        let path = path_in(&dir);
        std::fs::write(&path, [0xff, 0xfe, 0x00, 0x80]).unwrap();
        // Private, so that only its content is what is wrong with it.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        assert!(matches!(read_existing(&path), Ok(None)));
        let held = OwnerKey::default();
        held.enable(&path).unwrap();
        let key = read_in(&dir).expect("a key replaced the file");
        assert!(held.accepts(query(&key).as_deref()));
    }

    #[test]
    fn a_daemon_that_holds_none_accepts_none() {
        let none = OwnerKey::default();
        assert!(!none.is_held());
        assert!(!none.accepts(query(&"0".repeat(64)).as_deref()));
        assert!(!none.accepts(None));
        assert!(matches!(none.rotate(), Err(RotateError::NotHeld)));
    }

    #[test]
    fn a_failure_to_write_leaves_the_daemon_with_none() {
        let dir = ScratchDir::new("owner-key-unwritable");
        // A file where the directory should be.
        std::fs::write(dir.join("blocker"), "").unwrap();
        let held = OwnerKey::default();
        assert!(held.enable(&dir.join("blocker").join("owner.key")).is_err());
        assert!(!held.is_held());
    }

    #[test]
    fn a_rotation_replaces_the_key_in_the_file_and_in_memory() {
        let dir = ScratchDir::new("owner-key-rotate");
        let held = OwnerKey::default();
        held.enable(&path_in(&dir)).unwrap();
        let old = read_in(&dir).unwrap();

        held.rotate().unwrap();
        let new = read_in(&dir).unwrap();
        assert_ne!(old, new);
        assert!(!held.accepts(query(&old).as_deref()));
        assert!(held.accepts(query(&new).as_deref()));
    }

    /// A key that cannot be written is not taken: the old one stands, in memory and on disk.
    #[cfg(unix)]
    #[test]
    fn a_rotation_that_cannot_write_keeps_the_old_key() {
        use std::os::unix::fs::PermissionsExt;

        let dir = ScratchDir::new("owner-key-rotate-fails");
        let held = OwnerKey::default();
        held.enable(&path_in(&dir)).unwrap();
        let old = read_in(&dir).unwrap();
        let admitted = held.admit(query(&old).as_deref()).expect("admitted");

        // A directory that takes no new file; a process that bypasses the mode (root) would write.
        std::fs::set_permissions(&*dir, std::fs::Permissions::from_mode(0o500)).unwrap();
        let result = held.rotate();
        std::fs::set_permissions(&*dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        if result.is_ok() {
            return;
        }
        assert!(matches!(result, Err(RotateError::Write(_))));
        assert_eq!(read_in(&dir).as_deref(), Some(old.as_str()));
        assert!(held.accepts(query(&old).as_deref()));
        assert!(
            !admitted.0.has_changed().unwrap(),
            "a rotation that did not happen revokes nothing"
        );
    }

    #[test]
    fn replacing_in_a_data_directory_gives_the_key_it_wrote() {
        let dir = ScratchDir::new("owner-key-replace");
        let first = replace_in(&dir).unwrap();
        assert_eq!(read_in(&dir).as_deref(), Some(first.as_str()));
        let second = replace_in(&dir).unwrap();
        assert_ne!(first, second);
        assert_eq!(read_in(&dir).as_deref(), Some(second.as_str()));
    }

    /// A file that is not the daemon's user's own private file holds no key, whatever it says: a
    /// symlink, even to a private file holding a key, is not followed, and a file anyone else may
    /// read or write is not trusted. Each is replaced with a new private file holding a new key.
    #[cfg(unix)]
    #[test]
    fn a_key_file_that_is_not_private_is_replaced() {
        use std::os::unix::fs::PermissionsExt;

        let dir = ScratchDir::new("owner-key-not-private");
        let path = path_in(&dir);
        let planted = "a".repeat(64);

        for mode in [0o644, 0o640, 0o606, 0o620] {
            std::fs::write(&path, format!("{planted}\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
            assert_eq!(read_in(&dir), None, "mode {mode:o}");
            let held = OwnerKey::default();
            held.enable(&path).unwrap();
            assert!(!held.accepts(query(&planted).as_deref()), "mode {mode:o}");
            let new = read_in(&dir).expect("a new key");
            assert_ne!(new, planted);
            assert!(held.accepts(query(&new).as_deref()));
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }

        let elsewhere = dir.join("elsewhere.key");
        std::fs::write(&elsewhere, format!("{planted}\n")).unwrap();
        std::fs::set_permissions(&elsewhere, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &path).unwrap();
        assert_eq!(read_in(&dir), None, "a symlink is not followed");
        let held = OwnerKey::default();
        held.enable(&path).unwrap();
        assert!(!held.accepts(query(&planted).as_deref()));
        assert!(
            !std::fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink(),
            "the symlink itself was replaced"
        );
        assert_eq!(
            std::fs::read_to_string(&elsewhere).unwrap().trim(),
            planted,
            "the file the symlink led to is untouched"
        );
    }

    /// A connection admitted by the owner key learns of a rotation; one admitted before an earlier
    /// rotation is not admitted by the old key afterwards at all.
    #[tokio::test]
    async fn a_rotation_revokes_what_the_old_key_admitted() {
        let dir = ScratchDir::new("owner-key-revoke");
        let held = OwnerKey::default();
        held.enable(&path_in(&dir)).unwrap();
        let old = read_in(&dir).unwrap();
        let mut admitted = held.admit(query(&old).as_deref()).expect("admitted");
        assert!(held.admit(query(&"0".repeat(64)).as_deref()).is_none());

        assert!(
            !admitted.0.has_changed().unwrap(),
            "nothing was rotated yet"
        );

        held.rotate().unwrap();
        tokio::time::timeout(crate::test_support::PATIENCE, admitted.revoked())
            .await
            .expect("the rotation revokes the connection");
        assert!(held.admit(query(&old).as_deref()).is_none());

        let new = read_in(&dir).unwrap();
        let later = held.admit(query(&new).as_deref()).expect("admitted");
        assert!(
            !later.0.has_changed().unwrap(),
            "an earlier rotation does not revoke a later admission"
        );
    }

    /// A connection is cut off by a rotation of the key, and by nothing else: the end of the key's
    /// holder, which outlives every connection, is no rotation. The clock is paused, so the wait
    /// that never ends runs out at once.
    #[tokio::test(start_paused = true)]
    async fn the_end_of_the_key_is_no_rotation() {
        let dir = ScratchDir::new("owner-key-dropped");
        let held = OwnerKey::default();
        held.enable(&path_in(&dir)).unwrap();
        let old = read_in(&dir).unwrap();
        let mut admitted = held.admit(query(&old).as_deref()).expect("admitted");
        drop(held);
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(60), admitted.revoked())
                .await
                .is_err(),
            "the key being dropped revoked the connection"
        );
    }

    /// Every connection one key admitted learns of the rotation, whichever clone of the watch it
    /// holds, and a second rotation does not need a second look.
    #[tokio::test]
    async fn every_connection_the_key_admitted_learns_of_a_rotation() {
        let dir = ScratchDir::new("owner-key-many");
        let held = OwnerKey::default();
        held.enable(&path_in(&dir)).unwrap();
        let old = read_in(&dir).unwrap();
        let mut first = held.admit(query(&old).as_deref()).expect("admitted");
        let mut second = held.admit(query(&old).as_deref()).expect("admitted");
        let mut cloned = first.clone();

        held.rotate().unwrap();
        for watch in [&mut first, &mut second, &mut cloned] {
            tokio::time::timeout(crate::test_support::PATIENCE, watch.revoked())
                .await
                .expect("the rotation revokes every connection");
        }
    }

    /// A daemon that serves the browser but cannot write its key (no data directory) holds none,
    /// and says so.
    #[test]
    fn a_daemon_that_cannot_write_its_key_holds_none() {
        let dir = ScratchDir::new("owner-key-no-dir");
        let held = OwnerKey::default();
        assert!(!held.start(true, &dir.join("missing")));
        assert!(!held.is_held());
    }
}
