//! The saved remote daemons as the application keeps them: the entries' file (`entries`) read and
//! written under one lock, with the device keys that go with the entries (`device_key`) in the
//! credential store. Every change of the saved daemons goes through here: what a connection
//! records (a pinned host key, an installed device key, the owner's no to one), and what the owner
//! asks for (saving, removing, turning the key on or off, taking every key back), each of which
//! takes the device key back from its host first where it gives the key up (`key_removal`).
//!
//! A change that looks at whether an entry's key is installed is made only while no connection to
//! that entry's daemon is being made (`hold`): a connection that is installing the key finishes,
//! and records it, before the entry is removed or pointed elsewhere, so that no line is left on a
//! host that no entry says is installed.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Mutex;
use std::task::Poll;

use serde::Serialize;

use super::connect::{Connecting, Making};
use super::credentials::Store;
use super::entries::{self, Entry, EntryInput};
use super::key_removal::{take_back, KeyRemoval, KeyReport};
use crate::diagnostics::diagnostic;

/// What a change is refused with when its entry is no longer saved.
const REMOVED: &str = "This remote daemon has been removed.";

/// A saved daemon as it was saved, and what became of the device key the save gave up.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedEntry {
    pub entry: Entry,
    pub key_removal: KeyRemoval,
}

/// The saved remote daemons and where their device keys are kept.
pub struct Saved<'a> {
    /// The entries' file.
    pub path: PathBuf,
    /// Serializes the reads and writes of the entries' file; never held while a host is spoken to.
    pub lock: &'a Mutex<()>,
    /// The daemons a connection is being made to, which a change of their entry waits for.
    pub connecting: &'a Connecting,
    pub store: &'a dyn Store,
    /// The credential store service the device keys are kept under.
    pub service: String,
}

impl<'a> Saved<'a> {
    /// The remote daemons the owner has added.
    pub fn list(&self) -> Vec<Entry> {
        let _lock = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        entries::load(&self.path)
    }

    /// The remote daemons the owner has added, or an error where the entries' file is there and
    /// cannot be read ([`entries::read`]): for whoever must not take that for "nothing saved".
    fn read(&self) -> Result<Vec<Entry>, String> {
        let _lock = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        entries::read(&self.path)
    }

    /// Changes the entries as `change` does and writes them, unless it refuses. A file that is there
    /// and cannot be read is refused with the reason, never written over: what it holds (the saved
    /// daemons and which of them have this device's key installed) would be lost.
    fn change<T>(
        &self,
        change: impl FnOnce(&mut Vec<Entry>) -> Result<T, String>,
    ) -> Result<T, String> {
        let _lock = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut all = entries::read(&self.path)?;
        let changed = change(&mut all)?;
        entries::save(&self.path, &all).map_err(|err| err.to_string())?;
        Ok(changed)
    }

    /// Makes the change `record` makes where it says there is one to make, and writes it;
    /// whether it is recorded now. A change that could not be written is logged as `what`, and is
    /// not recorded: the caller undoes what it stood for.
    fn record(&self, what: &str, record: impl FnOnce(&mut Vec<Entry>) -> bool) -> bool {
        let recorded = self.change(|all| {
            if record(all) {
                Ok(())
            } else {
                Err("nothing to record: the entry is gone or names another destination".to_owned())
            }
        });
        match recorded {
            Ok(()) => true,
            Err(err) => {
                diagnostic!("cannot {what}: {err}");
                false
            }
        }
    }

    /// Pins `fingerprint` for the entry `id` once a first connection confirmed by the owner has
    /// authenticated; whether it is pinned now.
    pub fn pin(&self, id: &str, fingerprint: &str) -> bool {
        self.record(&format!("pin the host key of {id}"), |all| {
            entries::pin(all, id, fingerprint)
        })
    }

    /// Switches the entry `id` to the device key installed for `identity` as a line marked `mark`
    /// in the host's `file`; whether the entry says so now. Where it does not, the caller takes
    /// the line back: an installed key no entry records would stay on the host unseen.
    pub fn use_device_key(&self, id: &str, identity: &str, file: &str, mark: &str) -> bool {
        self.record(&format!("switch {id} to its device key"), |all| {
            entries::use_device_key(all, id, identity, file, mark)
        })
    }

    /// Remembers that the owner said no to this device's key for the entry `id`; whether it is
    /// remembered now.
    pub fn decline_device_key(&self, id: &str) -> bool {
        self.record(
            &format!("remember the declined device key of {id}"),
            |all| entries::decline_device_key(all, id),
        )
    }

    /// Notes the owner's no for the entry `id` before anything is done about it, an installed key
    /// still shown as installed; whether it is noted now.
    pub fn note_declined_device_key(&self, id: &str) -> bool {
        self.record(&format!("note the declined device key of {id}"), |all| {
            entries::note_declined_device_key(all, id)
        })
    }

    /// The entry `id` as it is saved, held: no connection to its daemon is being made, and none
    /// begins until what is returned with it is dropped. `None` where it is not saved.
    pub async fn hold(&self, id: &str) -> Option<(Entry, Making<'a>)> {
        loop {
            let entry = self.list().into_iter().find(|entry| entry.id == id)?;
            let making = self.connecting.wait_and_begin(&entry.identity()).await;
            // The entry may have been pointed elsewhere, or removed, while this waited.
            let now = self.list().into_iter().find(|entry| entry.id == id)?;
            if now.identity() == entry.identity() {
                return Some((now, making));
            }
        }
    }

    /// Saves a remote daemon, new or changed. Where the save gives up the device key installed for
    /// the entry (another host, port or user, or another way of signing in), the key is taken back
    /// from the host it was installed on first.
    pub async fn save(&self, input: EntryInput) -> Result<SavedEntry, String> {
        let held = match &input.id {
            Some(id) => self.hold(id).await,
            None => None,
        };
        // What the save would come to, so that an entry that is refused takes no key back.
        let planned = entries::upsert(&mut self.read()?, input.clone(), String::new)?;
        let key_removal = match &held {
            Some((previous, _)) if planned.gives_up_device_key_of(previous) => {
                take_back(self.store, &self.service, previous).await
            }
            // Another destination has no use for a key kept for this one, installed or not.
            Some((previous, _)) if planned.outlives_device_key_of(previous) => {
                take_back(self.store, &self.service, previous).await
            }
            _ => KeyRemoval::nothing(),
        };
        let entry = self.change(|all| entries::upsert(all, input, entries::new_id))?;
        Ok(SavedEntry { entry, key_removal })
    }

    /// Removes the remote daemon `id`, its device key taken back first.
    pub async fn remove(&self, id: &str) -> Result<KeyRemoval, String> {
        let held = self.hold(id).await;
        let removal = match &held {
            Some((entry, _)) => take_back(self.store, &self.service, entry).await,
            None => KeyRemoval::nothing(),
        };
        self.change(|all| {
            all.retain(|entry| entry.id != id);
            Ok(())
        })?;
        Ok(removal)
    }

    /// Turns this device's key on or off for the remote daemon `id`. Off takes an installed key
    /// back and keeps the entry, which signs in with a password until the owner chooses otherwise
    /// and is not asked about the key; on has its next connection ask before installing one.
    pub async fn set_device_key(&self, id: &str, enabled: bool) -> Result<KeyRemoval, String> {
        if enabled {
            return self.change(|all| {
                entries::allow_device_key(all, id)
                    .then(KeyRemoval::nothing)
                    .ok_or_else(|| REMOVED.to_owned())
            });
        }
        // A file that cannot be read says so, rather than that the daemon has been removed.
        self.read()?;
        let (entry, _held) = self.hold(id).await.ok_or_else(|| REMOVED.to_owned())?;
        let removal = take_back(self.store, &self.service, &entry).await;
        self.change(|all| {
            entries::decline_device_key(all, id);
            Ok(())
        })?;
        Ok(removal)
    }

    /// Whether anything of this device's key is kept for `entry`: the key is installed on its
    /// host, or, for an entry that does not sign in with it, the credential store still holds a
    /// private half (one it did not remove when the key was turned off).
    fn keeps_device_key(&self, entry: &Entry) -> bool {
        entry.has_device_key() || matches!(self.store.get(&self.service, &entry.id), Ok(Some(_)))
    }

    /// The saved daemons anything of this device's key is kept for: what
    /// [`Saved::remove_all_device_keys`] takes back. An error where the entries' file is there and
    /// cannot be read: which keys are installed is then not known, which is not "none".
    pub fn with_device_key(&self) -> Result<Vec<Entry>, String> {
        let all = {
            let _lock = self.lock.lock().unwrap_or_else(|e| e.into_inner());
            entries::read(&self.path)?
        };
        Ok(all
            .into_iter()
            .filter(|entry| self.keeps_device_key(entry))
            .collect())
    }

    /// Takes back the device key of every saved daemon that has one installed, the hosts spoken to
    /// side by side so that one that does not answer holds up no other, and turns the key off for
    /// each of them; and removes the private half the credential store still holds for a saved
    /// daemon that does not sign in with the key: what the clean-up before an uninstall runs. The
    /// report has one line per saved daemon anything was kept for, in the order they are saved in;
    /// a saved daemon without is left as it is. An error, with nothing taken back, where the
    /// entries' file cannot be read ([`Saved::with_device_key`]).
    pub async fn remove_all_device_keys(&self) -> Result<Vec<KeyReport>, String> {
        let taking: Vec<Pin<Box<dyn Future<Output = Option<KeyReport>> + Send + '_>>> = self
            .with_device_key()?
            .into_iter()
            .map(|listed| {
                let one: Pin<Box<dyn Future<Output = Option<KeyReport>> + Send + '_>> =
                    Box::pin(async move {
                        // Read again once held: its key may have been taken back meanwhile.
                        let (entry, _held) = self.hold(&listed.id).await?;
                        if !self.keeps_device_key(&entry) {
                            return None;
                        }
                        let on_server = entry.has_device_key();
                        let removal = take_back(self.store, &self.service, &entry).await;
                        if on_server {
                            self.decline_device_key(&entry.id);
                        }
                        Some(KeyReport {
                            id: entry.id,
                            name: entry.name,
                            host: entry.host,
                            user: entry.user,
                            on_server,
                            removal,
                        })
                    });
                one
            })
            .collect();
        Ok(all_of(taking).await.into_iter().flatten().collect())
    }
}

/// Runs `pending` side by side until every one of them has finished; what each came to, in their
/// order.
async fn all_of<T>(mut pending: Vec<Pin<Box<dyn Future<Output = T> + Send + '_>>>) -> Vec<T> {
    let mut finished: Vec<Option<T>> = pending.iter().map(|_| None).collect();
    std::future::poll_fn(|context| {
        let mut waiting = false;
        for (slot, future) in finished.iter_mut().zip(pending.iter_mut()) {
            if slot.is_some() {
                continue;
            }
            match future.as_mut().poll(context) {
                Poll::Ready(value) => *slot = Some(value),
                Poll::Pending => waiting = true,
            }
        }
        if waiting {
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    })
    .await;
    finished.into_iter().flatten().collect()
}

#[cfg(all(test, unix))]
mod tests {
    use super::super::credentials::Memory;
    use super::super::device_key::KeepingStore;
    use super::super::entries::{Auth, DeviceKey, FILE_NAME};
    use super::super::key_removal::test_support::{
        host, installed, keys_file, left, OTHERS, SERVICE,
    };
    use super::super::key_removal::{LineRemoval, PrivateKeyLeft};
    use super::*;
    use crate::config_dir::test_support::Scratch;
    use crate::test_support::ClosedPort;

    const REMOVED_KEY: KeyRemoval = KeyRemoval {
        line: LineRemoval::Removed,
        private_key_left: None,
    };

    /// What a `Saved` borrows, kept by the test: the lock and the connections being made.
    #[derive(Default)]
    struct Kept {
        lock: Mutex<()>,
        connecting: Connecting,
    }

    /// The saved daemons kept in `home`, holding `entries`.
    fn saved<'a>(
        kept: &'a Kept,
        home: &Scratch,
        store: &'a dyn Store,
        entries: &[Entry],
    ) -> Saved<'a> {
        let path = home.0.join(FILE_NAME);
        entries::save(&path, entries).unwrap();
        Saved {
            path,
            lock: &kept.lock,
            connecting: &kept.connecting,
            store,
            service: SERVICE.into(),
        }
    }

    fn input(entry: &Entry) -> EntryInput {
        EntryInput {
            id: Some(entry.id.clone()),
            name: entry.name.clone(),
            host: entry.host.clone(),
            port: entry.port,
            user: entry.user.clone(),
            auth: entry.auth,
            key_path: entry.key_path.clone(),
            fingerprint: entry.fingerprint.clone(),
            octoboardd_path: entry.octoboardd_path.clone(),
        }
    }

    fn held_keys(home: &Scratch) -> String {
        std::fs::read_to_string(keys_file(home)).unwrap()
    }

    #[tokio::test]
    async fn removing_a_saved_daemon_takes_its_key_back_first() {
        let (server, home) = host("remove").await;
        let (store, kept) = (Memory::default(), Kept::default());
        let entry = installed(&store, "a", server.port, &server.fingerprint, Some(&home));
        let saved = saved(&kept, &home, &store, &[entry]);
        assert_eq!(saved.remove("a").await, Ok(REMOVED_KEY));
        assert_eq!(held_keys(&home), OTHERS);
        assert!(saved.list().is_empty());
        assert!(store.accounts().is_empty());
        // One that is no longer saved has nothing to take back.
        assert_eq!(saved.remove("a").await, Ok(KeyRemoval::nothing()));
    }

    #[tokio::test]
    async fn removing_a_saved_daemon_whose_host_is_gone_removes_it_and_tells_of_the_line() {
        let home = Scratch::new("remote-saved", "remove-gone");
        std::fs::create_dir_all(&home.0).unwrap();
        let closed = ClosedPort::new();
        let (store, kept) = (Memory::default(), Kept::default());
        let entry = installed(&store, "a", closed.port(), "SHA256:x", None);
        let saved = saved(&kept, &home, &store, std::slice::from_ref(&entry));
        left(&entry, &saved.remove("a").await.unwrap().line);
        assert!(saved.list().is_empty());
        assert!(store.accounts().is_empty());
    }

    #[tokio::test]
    async fn removing_a_saved_daemon_whose_private_key_the_store_keeps_tells_of_the_item() {
        let (server, home) = host("remove-locked").await;
        let (store, kept) = (KeepingStore::default(), Kept::default());
        let entry = installed(&store, "a", server.port, &server.fingerprint, Some(&home));
        let saved = saved(&kept, &home, &store, &[entry]);
        let removal = saved.remove("a").await.unwrap();
        assert_eq!(removal.line, LineRemoval::Removed);
        // The saved daemon is removed all the same, and the item it leaves is named, not orphaned
        // in silence.
        assert_eq!(
            removal.private_key_left,
            Some(PrivateKeyLeft {
                service: SERVICE.into(),
                account: "a".into(),
                host: "127.0.0.1".into(),
                user: "owner".into(),
                detail: "the keychain is locked".into(),
            })
        );
        assert!(saved.list().is_empty());
    }

    #[tokio::test]
    async fn pointing_a_saved_daemon_elsewhere_takes_the_key_back_from_where_it_was_installed() {
        let (server, home) = host("repoint").await;
        let (store, kept) = (Memory::default(), Kept::default());
        let entry = installed(&store, "a", server.port, &server.fingerprint, Some(&home));
        let saved = saved(&kept, &home, &store, std::slice::from_ref(&entry));
        // A change that keeps the destination keeps the key.
        let mut renamed = input(&entry);
        renamed.name = "Renamed".into();
        let same = saved.save(renamed).await.unwrap();
        assert_eq!(same.key_removal, KeyRemoval::nothing());
        assert_eq!(same.entry.auth, Auth::Device);
        assert_ne!(held_keys(&home), OTHERS);
        assert_eq!(store.accounts(), ["a"]);
        // An entry that is refused takes nothing back, and says what is wrong with it.
        let mut refused = input(&entry);
        refused.user = "other".into();
        refused.host = " ".into();
        assert_eq!(
            saved.save(refused).await,
            Err("The host is missing or contains spaces.".to_owned())
        );
        assert_ne!(held_keys(&home), OTHERS);
        assert_eq!(store.accounts(), ["a"]);
        // Another user is another destination: the key is taken back from the one it left.
        let mut moved = input(&entry);
        moved.user = "other".into();
        let moved = saved.save(moved).await.unwrap();
        assert_eq!(moved.key_removal, REMOVED_KEY);
        assert_eq!(moved.entry.auth, Auth::Password);
        assert_eq!(moved.entry.device_key, DeviceKey::default());
        assert_eq!(held_keys(&home), OTHERS);
        assert!(store.accounts().is_empty());
        assert_eq!(saved.list(), [moved.entry]);
    }

    #[tokio::test]
    async fn saving_an_entry_to_sign_in_another_way_takes_its_key_back() {
        let (server, home) = host("another-way").await;
        let (store, kept) = (Memory::default(), Kept::default());
        let entry = installed(&store, "a", server.port, &server.fingerprint, Some(&home));
        let saved = saved(&kept, &home, &store, std::slice::from_ref(&entry));
        let mut password = input(&entry);
        password.auth = Auth::Password;
        let changed = saved.save(password).await.unwrap();
        assert_eq!(changed.key_removal, REMOVED_KEY);
        assert_eq!(held_keys(&home), OTHERS);
        assert!(store.accounts().is_empty());
    }

    #[tokio::test]
    async fn a_key_kept_for_a_destination_the_entry_leaves_goes_though_it_was_never_installed() {
        let home = Scratch::new("remote-saved", "stray");
        std::fs::create_dir_all(&home.0).unwrap();
        let closed = ClosedPort::new();
        let (store, kept) = (Memory::default(), Kept::default());
        // A key made for an install that did not complete: the entry signs in with a password.
        let entry = Entry {
            auth: Auth::Password,
            device_key: DeviceKey::default(),
            ..installed(&store, "a", closed.port(), "SHA256:x", None)
        };
        let saved = saved(&kept, &home, &store, std::slice::from_ref(&entry));
        let mut moved = input(&entry);
        moved.user = "other".into();
        assert_eq!(
            saved.save(moved).await.unwrap().key_removal,
            KeyRemoval::nothing()
        );
        assert!(store.accounts().is_empty());
    }

    #[tokio::test]
    async fn turning_the_key_off_takes_it_back_keeps_the_saved_daemon_and_on_asks_again() {
        let (server, home) = host("switch").await;
        let (store, kept) = (Memory::default(), Kept::default());
        let entry = installed(&store, "a", server.port, &server.fingerprint, Some(&home));
        let saved = saved(&kept, &home, &store, &[entry]);
        assert_eq!(saved.set_device_key("a", false).await, Ok(REMOVED_KEY));
        assert_eq!(held_keys(&home), OTHERS);
        assert!(store.accounts().is_empty());
        let now = saved.list();
        assert_eq!(now.len(), 1);
        assert_eq!(now[0].auth, Auth::Password);
        assert!(now[0].device_key.declined);
        // Off again, with nothing installed, there is nothing to take back.
        assert_eq!(
            saved.set_device_key("a", false).await,
            Ok(KeyRemoval::nothing())
        );
        // On: the next connection asks, and nothing is installed by the switch itself.
        assert_eq!(
            saved.set_device_key("a", true).await,
            Ok(KeyRemoval::nothing())
        );
        let now = saved.list();
        assert!(!now[0].device_key.declined);
        assert_eq!(now[0].auth, Auth::Password);
        assert_eq!(held_keys(&home), OTHERS);
        // A saved daemon that is gone says so, either way.
        for enabled in [true, false] {
            assert_eq!(
                saved.set_device_key("gone", enabled).await,
                Err("This remote daemon has been removed.".to_owned())
            );
        }
    }

    #[tokio::test]
    async fn every_installed_key_is_taken_back_at_once_and_the_other_saved_daemons_left_alone() {
        let (server, home) = host("all").await;
        let closed = ClosedPort::new();
        let (store, kept) = (Memory::default(), Kept::default());
        let reachable = installed(&store, "a", server.port, &server.fingerprint, Some(&home));
        let gone = installed(&store, "b", closed.port(), "SHA256:x", None);
        // A key an earlier build installed, on the same host as the first: the two are the same
        // daemon, and are taken back one after the other.
        let earlier = Entry {
            device_key: DeviceKey::default(),
            ..installed(&store, "c", server.port, &server.fingerprint, Some(&home))
        };
        // One whose key was turned off earlier, the credential store keeping its private half.
        let left_behind = Entry {
            id: "f".into(),
            auth: Auth::Password,
            device_key: DeviceKey {
                declined: true,
                ..DeviceKey::default()
            },
            ..reachable.clone()
        };
        store.set(SERVICE, "f", b"a private half").unwrap();
        // Two without a key: one never asked, one that said no.
        let unasked = Entry {
            id: "d".into(),
            auth: Auth::Password,
            device_key: DeviceKey::default(),
            ..reachable.clone()
        };
        let declined = Entry {
            id: "e".into(),
            auth: Auth::Key,
            key_path: Some("~/.ssh/id_ed25519".into()),
            device_key: DeviceKey {
                declined: true,
                ..DeviceKey::default()
            },
            ..reachable.clone()
        };
        let all = [
            reachable,
            gone.clone(),
            earlier,
            unasked.clone(),
            declined.clone(),
            left_behind.clone(),
        ];
        let saved = saved(&kept, &home, &store, &all);
        assert_eq!(
            saved
                .with_device_key()
                .unwrap()
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b", "c", "f"]
        );
        let reports = saved.remove_all_device_keys().await.unwrap();
        assert_eq!(
            reports
                .iter()
                .map(|report| (report.id.as_str(), report.on_server))
                .collect::<Vec<_>>(),
            [("a", true), ("b", true), ("c", true), ("f", false)]
        );
        // The private half kept without a key on the host is removed, with no host spoken to.
        assert_eq!(reports[3].removal, KeyRemoval::nothing());
        assert_eq!(reports[0].removal, REMOVED_KEY);
        assert_eq!(
            (reports[0].name.as_str(), reports[0].host.as_str()),
            ("server a", "127.0.0.1")
        );
        left(&gone, &reports[1].removal.line);
        assert_eq!(reports[2].removal, REMOVED_KEY);
        assert_eq!(held_keys(&home), OTHERS);
        assert!(store.accounts().is_empty());
        // The saved daemons stay: those that had a key have it turned off, and the others are as
        // they were, a daemon that was never asked still to be asked.
        let now = saved.list();
        assert_eq!(now.len(), 6);
        assert!(now[..3]
            .iter()
            .all(|entry| entry.auth == Auth::Password && entry.device_key.declined));
        assert_eq!(now[3..], [unasked, declined, left_behind]);
        // Nothing is left to take back.
        assert_eq!(saved.with_device_key(), Ok(vec![]));
        assert_eq!(saved.remove_all_device_keys().await, Ok(vec![]));
    }

    #[tokio::test]
    async fn saved_daemons_that_cannot_be_read_are_not_taken_for_none_with_a_key() {
        let home = Scratch::new("remote-saved", "unreadable");
        std::fs::create_dir_all(&home.0).unwrap();
        let (store, kept) = (Memory::default(), Kept::default());
        let saved = saved(&kept, &home, &store, &[]);
        // No file at all is nothing saved.
        std::fs::remove_file(&saved.path).unwrap();
        assert_eq!(saved.with_device_key(), Ok(vec![]));
        assert_eq!(saved.remove_all_device_keys().await, Ok(vec![]));
        // A file that is there and is not what was saved: which keys are installed is not known.
        std::fs::write(&saved.path, "{ not what was saved").unwrap();
        let unread = saved.with_device_key().unwrap_err();
        assert!(unread.contains("could not be read"), "{unread}");
        assert_eq!(saved.remove_all_device_keys().await, Err(unread));
        // The file is left as it is, and the list a window shows is empty as before.
        assert_eq!(
            std::fs::read_to_string(&saved.path).unwrap(),
            "{ not what was saved"
        );
        assert!(saved.list().is_empty());
    }

    #[tokio::test]
    async fn saved_daemons_that_cannot_be_read_are_never_written_over() {
        let home = Scratch::new("remote-saved", "unreadable-change");
        std::fs::create_dir_all(&home.0).unwrap();
        let (store, kept) = (Memory::default(), Kept::default());
        let entry = Entry {
            auth: Auth::Password,
            device_key: DeviceKey::default(),
            ..installed(&store, "a", ClosedPort::new().port(), "SHA256:x", None)
        };
        let saved = saved(&kept, &home, &store, std::slice::from_ref(&entry));
        const BROKEN: &str = "{ not what was saved";
        std::fs::write(&saved.path, BROKEN).unwrap();
        let unread = |result: Result<(), String>, what: &str| {
            let err = result.expect_err(what);
            assert!(err.contains("could not be read"), "{what}: {err}");
            assert_eq!(
                std::fs::read_to_string(&saved.path).unwrap(),
                BROKEN,
                "{what}"
            );
        };
        // Every change the owner asks for is refused with the reason, the file left as it is.
        let mut new = input(&entry);
        new.id = None;
        unread(saved.save(new).await.map(drop), "a new entry");
        unread(saved.save(input(&entry)).await.map(drop), "a changed entry");
        unread(saved.remove("a").await.map(drop), "a removal");
        for enabled in [true, false] {
            unread(
                saved.set_device_key("a", enabled).await.map(drop),
                "a key switch",
            );
        }
        // What a connection records is not recorded either.
        assert!(!saved.pin("a", "SHA256:abc"));
        assert!(!saved.decline_device_key("a"));
        assert_eq!(std::fs::read_to_string(&saved.path).unwrap(), BROKEN);
        // No file at all is nothing saved, and a change writes a new one.
        std::fs::remove_file(&saved.path).unwrap();
        let mut new = input(&entry);
        new.id = None;
        assert!(saved.save(new).await.is_ok());
        assert_eq!(saved.list().len(), 1);
    }

    #[tokio::test]
    async fn a_saved_daemon_is_not_removed_or_pointed_elsewhere_while_a_connection_to_it_is_made() {
        // The connection asked to add the key is under way when the owner removes the daemon, or
        // changes its user: the change waits, the connection installs the key and records it, and
        // the change then finds the key installed and takes it back. Nothing stays on the host.
        let (server, home) = host("race").await;
        let (store, kept) = (Memory::default(), Kept::default());
        // As saved while the connection is under way: signing in with a password, no key yet.
        let before = Entry {
            auth: Auth::Password,
            device_key: DeviceKey::default(),
            ..installed(&store, "a", server.port, &server.fingerprint, None)
        };
        std::fs::write(keys_file(&home), OTHERS).unwrap();
        let saved = saved(&kept, &home, &store, std::slice::from_ref(&before));
        for change in ["remove", "save"] {
            let making = kept.connecting.begin(&before.identity()).ok().unwrap();
            let mut moved = input(&before);
            moved.user = "other".into();
            let changing: Pin<Box<dyn Future<Output = KeyRemoval> + '_>> = match change {
                "remove" => Box::pin(async { saved.remove("a").await.unwrap() }),
                _ => Box::pin(async { saved.save(moved).await.unwrap().key_removal }),
            };
            tokio::pin!(changing);
            // Asked while the connection is being made, it has not gone ahead.
            let mut asked = std::task::Context::from_waker(std::task::Waker::noop());
            assert!(changing.as_mut().poll(&mut asked).is_pending());
            assert_eq!(saved.list(), std::slice::from_ref(&before));
            // The connection installs the key and records it, then is over.
            let after = installed(&store, "a", server.port, &server.fingerprint, Some(&home));
            assert!(saved.use_device_key(
                "a",
                &before.identity(),
                "~/.ssh/authorized_keys",
                "Octoboard on Desk"
            ));
            assert_eq!(saved.list(), [after]);
            drop(making);
            let removal = tokio::time::timeout(crate::test_support::PATIENCE, changing)
                .await
                .expect("the change goes ahead once the connection is over");
            assert_eq!(removal, REMOVED_KEY, "{change}");
            assert_eq!(held_keys(&home), OTHERS, "{change}");
            assert!(store.accounts().is_empty(), "{change}");
            // Back to how it was saved, for the next change.
            entries::save(&saved.path, std::slice::from_ref(&before)).unwrap();
        }
    }

    #[tokio::test]
    async fn what_a_connection_records_is_refused_once_its_entry_is_gone_or_names_another_place() {
        let home = Scratch::new("remote-saved", "record");
        std::fs::create_dir_all(&home.0).unwrap();
        let closed = ClosedPort::new();
        let (store, kept) = (Memory::default(), Kept::default());
        let entry = Entry {
            auth: Auth::Password,
            fingerprint: None,
            device_key: DeviceKey::default(),
            ..installed(&store, "a", closed.port(), "SHA256:x", None)
        };
        let saved = saved(&kept, &home, &store, std::slice::from_ref(&entry));
        let record = |id: &str, identity: &str| {
            saved.use_device_key(id, identity, "~/.ssh/authorized_keys", "Octoboard on Desk")
        };
        // The caller takes the line back where this says the key was not recorded.
        assert!(!record("gone", &entry.identity()));
        assert!(!record("a", "ssh:owner@elsewhere:22"));
        assert_eq!(saved.list(), std::slice::from_ref(&entry));
        assert!(record("a", &entry.identity()));
        assert_eq!(saved.list()[0].auth, Auth::Device);
        // The owner's no and a pinned host key are recorded the same way.
        assert!(saved.decline_device_key("a"));
        assert!(!saved.decline_device_key("gone"));
        assert!(saved.list()[0].device_key.declined);
        assert!(saved.pin("a", "SHA256:abc"));
        assert!(!saved.pin("a", "SHA256:other"));
        assert_eq!(saved.list()[0].fingerprint.as_deref(), Some("SHA256:abc"));
        // A file that cannot be written records nothing, and says so to the caller: the entry
        // is there to change, and the directory takes no new file.
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&home.0, std::fs::Permissions::from_mode(0o500)).unwrap();
        let recorded = saved.decline_device_key("a");
        std::fs::set_permissions(&home.0, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(!recorded);
    }

    #[tokio::test]
    async fn futures_run_side_by_side_answer_in_their_order_whichever_finishes_first() {
        let (first, last) = tokio::sync::oneshot::channel::<u8>();
        let slow: Pin<Box<dyn Future<Output = u8> + Send>> =
            Box::pin(async move { last.await.unwrap() });
        let quick: Pin<Box<dyn Future<Output = u8> + Send>> = Box::pin(async move {
            // The second finishes first, and lets the first finish.
            first.send(1).unwrap();
            2
        });
        assert_eq!(all_of(vec![slow, quick]).await, [1, 2]);
        assert!(
            all_of(Vec::<Pin<Box<dyn Future<Output = u8> + Send>>>::new())
                .await
                .is_empty()
        );
    }
}
