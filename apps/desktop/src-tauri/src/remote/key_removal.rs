//! Taking a device key back: the line the key was installed as is taken out of the host's
//! authorized keys, over a connection made with the key itself, and the private key is then
//! removed from the credential store. That is done when a saved daemon is removed, when it is
//! saved to another destination or another way of signing in, when its key is turned off, and for
//! every saved daemon with a key at once before the application is uninstalled (`saved`). A host
//! that cannot be reached, that refuses the key, or whose file does not hold the line does not
//! hold any of these up: the private key is removed all the same, and the owner is told which line
//! stays on that host. A credential store that does not remove the private key is told too.

use russh::keys::PrivateKey;
use serde::Serialize;

use super::authorized_keys::{self, ChangeError, KeyChange};
use super::connect::shell_of;
use super::credentials::Store;
use super::device_key;
use super::entries::Entry;
use super::octoboardd::Shell;
use super::ssh::{self, Secret, Session, Target};

/// What became of the line a device key was installed as on its host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(
    tag = "result",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum LineRemoval {
    /// No key was installed for the saved daemon.
    NotInstalled,
    /// The line was taken out of the host's file.
    Removed,
    /// The line stays on the host, which could not be reached, refused the key, or does not hold
    /// the line where the application looks for it: the owner removes it there.
    LeftOnHost {
        /// What the line is marked with.
        mark: String,
        /// The file it is in, where the application knows it: not for a key an earlier build
        /// installed.
        file: Option<String>,
        host: String,
        user: String,
        /// Why the host's file was not changed.
        detail: String,
    },
    /// The host's file may be incomplete: it was being rewritten when the write failed, and its
    /// content could not be put back. Other keys may be missing from it, so the owner restores it
    /// from the copy the host kept.
    FileIncomplete {
        host: String,
        user: String,
        /// Where the host keeps the content the file had before.
        copy: String,
        /// What the host said.
        detail: String,
    },
}

/// A private key the credential store did not remove, named so that the owner can remove the item
/// there.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PrivateKeyLeft {
    /// The credential store service the item is kept under.
    pub service: String,
    /// The item's account: the saved daemon's id.
    pub account: String,
    pub host: String,
    pub user: String,
    /// What the credential store said.
    pub detail: String,
}

/// What taking the device key of a saved daemon back came to: its line on the host, and its
/// private half on this computer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyRemoval {
    pub line: LineRemoval,
    /// The private key, where the credential store did not remove it; `None` where it is gone, or
    /// none was kept.
    pub private_key_left: Option<PrivateKeyLeft>,
}

impl KeyRemoval {
    /// Nothing was installed, and nothing is kept.
    pub fn nothing() -> Self {
        Self {
            line: LineRemoval::NotInstalled,
            private_key_left: None,
        }
    }
}

/// What became of the device key of one saved daemon, when every key is taken back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyReport {
    pub id: String,
    pub name: String,
    pub host: String,
    pub user: String,
    /// Whether the key was installed on the host; without, only its private half was kept here.
    pub on_server: bool,
    pub removal: KeyRemoval,
}

/// Takes the line that carries `key` out of the remote user's authorized keys over `session`, on
/// a host whose commands `shell` runs. The one way the application removes a key's line, whoever
/// asks: a key taken back, a key the owner said no to, a key the host did not accept.
pub async fn remove_line(
    session: &Session,
    shell: Shell,
    key: &PrivateKey,
) -> Result<(), ChangeError> {
    let body = device_key::public_body(key).map_err(ChangeError::Failed)?;
    let remove = KeyChange::Remove { body: &body };
    authorized_keys::change_authorized_keys(session, shell, remove).await
}

/// Removes the private key kept for `entry` from `store`; what is left where the store does not
/// remove it.
pub fn forget_private_key(
    store: &dyn Store,
    service: &str,
    entry: &Entry,
) -> Option<PrivateKeyLeft> {
    let detail = device_key::forget(store, service, &entry.id).err()?;
    Some(PrivateKeyLeft {
        service: service.to_owned(),
        account: entry.id.clone(),
        host: entry.host.clone(),
        user: entry.user.clone(),
        detail,
    })
}

/// Takes the device key of `entry` back: its line out of the host's authorized keys, then the
/// private key out of `store`. The private key is removed whatever the host answered.
pub async fn take_back(store: &dyn Store, service: &str, entry: &Entry) -> KeyRemoval {
    if !entry.has_device_key() {
        // A key kept for an entry that does not sign in with it is not kept either.
        return KeyRemoval {
            line: LineRemoval::NotInstalled,
            private_key_left: forget_private_key(store, service, entry),
        };
    }
    let removed = remove_from_host(store, service, entry).await;
    let private_key_left = forget_private_key(store, service, entry);
    KeyRemoval {
        line: line_of(entry, removed),
        private_key_left,
    }
}

/// Why the line of a device key was not taken out of its host's file.
#[derive(Debug, PartialEq, Eq)]
enum NotRemoved {
    /// The file is as it was: why.
    Left(String),
    /// The file may be incomplete: where its earlier content is kept, and what the host said.
    Incomplete { copy: String, detail: String },
}

/// What became of the line of `entry`'s key, from how its removal ended.
fn line_of(entry: &Entry, removed: Result<(), NotRemoved>) -> LineRemoval {
    match removed {
        Ok(()) => LineRemoval::Removed,
        Err(NotRemoved::Left(detail)) => LineRemoval::LeftOnHost {
            mark: entry
                .device_key
                .mark
                .clone()
                .unwrap_or_else(|| device_key::comment(&device_key::device_name())),
            file: entry.device_key.file.clone(),
            host: entry.host.clone(),
            user: entry.user.clone(),
            detail,
        },
        Err(NotRemoved::Incomplete { copy, detail }) => LineRemoval::FileIncomplete {
            host: entry.host.clone(),
            user: entry.user.clone(),
            copy,
            detail,
        },
    }
}

/// Signs in to the host of `entry` with its device key and takes the key's line out of the remote
/// user's authorized keys. A file that does not hold the line is a failure too: the key has just
/// signed in, so its line is somewhere the application does not look.
async fn remove_from_host(
    store: &dyn Store,
    service: &str,
    entry: &Entry,
) -> Result<(), NotRemoved> {
    let left = NotRemoved::Left;
    let key = device_key::load(store, service, &entry.id)
        .ok_or_else(|| left("this computer no longer holds the key to sign in with".into()))?;
    let session = ssh::connect(&Target {
        host: entry.host.clone(),
        port: entry.port,
        user: entry.user.clone(),
        secret: Secret::DeviceKey(key.clone()),
        pinned: entry.fingerprint.clone(),
    })
    .await
    .map_err(|err| left(format!("{err:?}")))?;
    let removed = match shell_of(&session).await {
        Ok(shell) => remove_line(&session, shell, &key)
            .await
            .map_err(|err| match err {
                ChangeError::Incomplete { copy, detail } => NotRemoved::Incomplete { copy, detail },
                other => left(other.to_string()),
            }),
        Err(err) => Err(left(err.to_string())),
    };
    session.disconnect().await;
    removed
}

/// What the tests of taking a key back share: a stand-in host with a home, and entries whose key
/// is installed there.
#[cfg(all(test, unix))]
pub(super) mod test_support {
    use super::super::credentials::Store;
    use super::super::entries::{Auth, DeviceKey, Entry};
    use super::super::ssh::test_server::{Behaviour, TestServer};
    use super::super::{authorized_keys, device_key};
    use crate::config_dir::test_support::Scratch;

    pub const SERVICE: &str = "svc";
    /// What a host's file holds besides the application's keys: another key, a comment, a blank
    /// line and a last key.
    pub const OTHERS: &str = "ssh-rsa AAAAother laptop\n# a note\n\nssh-ed25519 AAAAlast phone\n";

    /// A stand-in host whose home is a scratch directory, with an `.ssh` in it.
    pub async fn host(name: &str) -> (TestServer, Scratch) {
        let scratch = Scratch::new("remote-key-removal", name);
        std::fs::create_dir_all(scratch.0.join(".ssh")).unwrap();
        let server = TestServer::start(Behaviour {
            home: Some(scratch.0.clone()),
            ..Behaviour::default()
        })
        .await;
        (server, scratch)
    }

    pub fn keys_file(home: &Scratch) -> std::path::PathBuf {
        home.0.join(".ssh/authorized_keys")
    }

    /// An entry that signs in to `port` with a device key, which is made and kept in `store`;
    /// where `home` is given, the key is installed there among `OTHERS`' lines.
    pub fn installed(
        store: &dyn Store,
        id: &str,
        port: u16,
        fingerprint: &str,
        home: Option<&Scratch>,
    ) -> Entry {
        let key = device_key::load_or_make(store, SERVICE, id).unwrap().key;
        if let Some(home) = home {
            let line = device_key::authorized_line(&key, "Octoboard on Desk").unwrap();
            let (first, rest) = OTHERS.split_at(OTHERS.find('#').unwrap());
            let mut text = std::fs::read_to_string(keys_file(home)).unwrap_or_default();
            if text.is_empty() {
                text = format!("{first}{line}\n{rest}");
            } else {
                text.push_str(&format!("{line}\n"));
            }
            std::fs::write(keys_file(home), text).unwrap();
        }
        Entry {
            id: id.into(),
            name: format!("server {id}"),
            host: "127.0.0.1".into(),
            port,
            user: "owner".into(),
            auth: Auth::Device,
            key_path: None,
            fingerprint: Some(fingerprint.into()),
            octoboardd_path: None,
            device_key: DeviceKey {
                declined: false,
                file: Some(authorized_keys::POSIX_FILE.into()),
                mark: Some("Octoboard on Desk".into()),
            },
        }
    }

    /// Why the line of `entry` was left on its host, where `line` says it was, checked to name the
    /// line as the entry keeps it.
    pub fn left(entry: &Entry, line: &super::LineRemoval) -> String {
        let super::LineRemoval::LeftOnHost {
            mark,
            file,
            host,
            user,
            detail,
        } = line
        else {
            panic!("expected the line to be left on the host, got {line:?}");
        };
        assert_eq!(mark, "Octoboard on Desk");
        assert_eq!(file.as_deref(), Some("~/.ssh/authorized_keys"));
        assert_eq!((host, user), (&entry.host, &entry.user));
        detail.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_left_incomplete_is_told_apart_from_a_line_left_on_the_host() {
        use super::super::entries::{Auth, DeviceKey};
        let entry = Entry {
            id: "e".into(),
            name: "n".into(),
            host: "h".into(),
            port: 22,
            user: "u".into(),
            auth: Auth::Device,
            key_path: None,
            fingerprint: None,
            octoboardd_path: None,
            device_key: DeviceKey {
                declined: false,
                file: Some("~/.ssh/authorized_keys".into()),
                mark: Some("Octoboard on Desk".into()),
            },
        };
        assert_eq!(line_of(&entry, Ok(())), LineRemoval::Removed);
        assert_eq!(
            line_of(&entry, Err(NotRemoved::Left("refused".into()))),
            LineRemoval::LeftOnHost {
                mark: "Octoboard on Desk".into(),
                file: Some("~/.ssh/authorized_keys".into()),
                host: "h".into(),
                user: "u".into(),
                detail: "refused".into(),
            }
        );
        let incomplete = line_of(
            &entry,
            Err(NotRemoved::Incomplete {
                copy: "/tmp/tmp.x/before".into(),
                detail: "full".into(),
            }),
        );
        assert_eq!(
            incomplete,
            LineRemoval::FileIncomplete {
                host: "h".into(),
                user: "u".into(),
                copy: "/tmp/tmp.x/before".into(),
                detail: "full".into(),
            }
        );
        assert_eq!(
            serde_json::to_value(incomplete).unwrap(),
            serde_json::json!({
                "result": "file_incomplete", "host": "h", "user": "u",
                "copy": "/tmp/tmp.x/before", "detail": "full"
            })
        );
    }

    #[test]
    fn what_became_of_a_key_is_named_for_the_ui() {
        let json = |removal: KeyRemoval| serde_json::to_value(removal).unwrap();
        assert_eq!(
            json(KeyRemoval::nothing()),
            serde_json::json!({"line": {"result": "not_installed"}, "privateKeyLeft": null})
        );
        assert_eq!(
            json(KeyRemoval {
                line: LineRemoval::Removed,
                private_key_left: Some(PrivateKeyLeft {
                    service: "app.remote-daemon-key".into(),
                    account: "e".into(),
                    host: "h".into(),
                    user: "u".into(),
                    detail: "locked".into(),
                }),
            }),
            serde_json::json!({
                "line": {"result": "removed"},
                "privateKeyLeft": {
                    "service": "app.remote-daemon-key", "account": "e", "host": "h", "user": "u",
                    "detail": "locked"
                }
            })
        );
        assert_eq!(
            json(KeyRemoval {
                line: LineRemoval::LeftOnHost {
                    mark: "Octoboard on Desk".into(),
                    file: None,
                    host: "h".into(),
                    user: "u".into(),
                    detail: "d".into(),
                },
                private_key_left: None,
            }),
            serde_json::json!({
                "line": {
                    "result": "left_on_host", "mark": "Octoboard on Desk", "file": null,
                    "host": "h", "user": "u", "detail": "d"
                },
                "privateKeyLeft": null
            })
        );
    }
}

#[cfg(all(test, unix))]
mod host_tests {
    use super::super::credentials::Memory;
    use super::super::device_key::KeepingStore;
    use super::super::entries::{Auth, DeviceKey};
    use super::test_support::{host, installed, keys_file, left, OTHERS, SERVICE};
    use super::*;
    use crate::test_support::ClosedPort;

    const REMOVED: KeyRemoval = KeyRemoval {
        line: LineRemoval::Removed,
        private_key_left: None,
    };

    #[tokio::test]
    async fn taking_a_key_back_removes_its_line_alone_and_then_the_private_key() {
        let (server, home) = host("line").await;
        let store = Memory::default();
        let entry = installed(&store, "a", server.port, &server.fingerprint, Some(&home));
        assert_ne!(std::fs::read_to_string(keys_file(&home)).unwrap(), OTHERS);
        assert_eq!(take_back(&store, SERVICE, &entry).await, REMOVED);
        // Every other line, the comment and the blank one included, is as it was.
        assert_eq!(std::fs::read_to_string(keys_file(&home)).unwrap(), OTHERS);
        assert!(store.accounts().is_empty());
    }

    #[tokio::test]
    async fn a_file_the_key_was_the_only_line_of_stays_empty_with_its_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let (server, home) = host("only").await;
        let store = Memory::default();
        let key = device_key::load_or_make(&store, SERVICE, "a").unwrap().key;
        let line = device_key::authorized_line(&key, "Octoboard on Desk").unwrap();
        std::fs::write(keys_file(&home), format!("{line}\n")).unwrap();
        std::fs::set_permissions(keys_file(&home), std::fs::Permissions::from_mode(0o640)).unwrap();
        let entry = installed(&store, "a", server.port, &server.fingerprint, None);
        assert_eq!(take_back(&store, SERVICE, &entry).await, REMOVED);
        assert_eq!(std::fs::read_to_string(keys_file(&home)).unwrap(), "");
        let mode = std::fs::metadata(keys_file(&home))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o640);
    }

    #[tokio::test]
    async fn a_host_that_cannot_be_reached_keeps_its_line_and_the_private_key_goes_all_the_same() {
        let closed = ClosedPort::new();
        let store = Memory::default();
        let entry = installed(&store, "a", closed.port(), "SHA256:x", None);
        let removal = take_back(&store, SERVICE, &entry).await;
        assert!(left(&entry, &removal.line).contains("Unreachable"));
        assert_eq!(removal.private_key_left, None);
        assert!(store.accounts().is_empty());
    }

    #[tokio::test]
    async fn a_host_that_refuses_the_key_keeps_what_it_holds_and_the_owner_is_told() {
        let (server, home) = host("refused").await;
        std::fs::write(keys_file(&home), OTHERS).unwrap();
        let store = Memory::default();
        // The key was never added to the host's file, or was removed there.
        let entry = installed(&store, "a", server.port, &server.fingerprint, None);
        let removal = take_back(&store, SERVICE, &entry).await;
        assert!(left(&entry, &removal.line).contains("AuthFailed"));
        assert_eq!(std::fs::read_to_string(keys_file(&home)).unwrap(), OTHERS);
        assert!(store.accounts().is_empty());
    }

    #[tokio::test]
    async fn a_key_that_signs_in_but_whose_line_is_not_in_the_file_is_told_as_left_there() {
        // The stand-in host accepts one user key whatever its file lists, as a host that reads
        // its keys from another file does: the removal finds no line, and must not say Removed.
        let (server, home) = host("elsewhere").await;
        std::fs::write(keys_file(&home), OTHERS).unwrap();
        let store = Memory::default();
        let accepted = super::super::ssh::test_server::user_key_openssh();
        store.set(SERVICE, "a", accepted.as_bytes()).unwrap();
        let entry = installed(&store, "a", server.port, &server.fingerprint, None);
        let removal = take_back(&store, SERVICE, &entry).await;
        assert_eq!(
            left(&entry, &removal.line),
            ChangeError::NotFound.to_string()
        );
        assert_eq!(std::fs::read_to_string(keys_file(&home)).unwrap(), OTHERS);
        assert!(store.accounts().is_empty());
    }

    #[tokio::test]
    async fn a_key_this_computer_no_longer_holds_is_told_as_left_on_the_host() {
        let (server, _home) = host("lost").await;
        let store = Memory::default();
        let entry = installed(&store, "a", server.port, &server.fingerprint, None);
        device_key::forget(&store, SERVICE, "a").unwrap();
        let removal = take_back(&store, SERVICE, &entry).await;
        assert_eq!(
            left(&entry, &removal.line),
            "this computer no longer holds the key to sign in with"
        );
        assert_eq!(server.connections(), 0);
    }

    #[tokio::test]
    async fn a_key_an_earlier_build_installed_is_taken_back_the_same_way() {
        let (server, home) = host("earlier").await;
        let store = Memory::default();
        // An earlier build kept nothing about where the line is.
        let earlier = |entry: Entry| Entry {
            device_key: DeviceKey::default(),
            ..entry
        };
        let entry = earlier(installed(
            &store,
            "a",
            server.port,
            &server.fingerprint,
            Some(&home),
        ));
        assert_eq!(take_back(&store, SERVICE, &entry).await, REMOVED);
        assert_eq!(std::fs::read_to_string(keys_file(&home)).unwrap(), OTHERS);
        // On a host that cannot be reached, the line is named by the mark this computer gives its
        // keys, and the file is not known.
        let closed = ClosedPort::new();
        let entry = earlier(installed(&store, "b", closed.port(), "SHA256:x", None));
        match take_back(&store, SERVICE, &entry).await.line {
            LineRemoval::LeftOnHost { mark, file, .. } => {
                assert_eq!(mark, device_key::comment(&device_key::device_name()));
                assert_eq!(file, None);
            }
            other => panic!("{other:?}"),
        }
        assert!(store.accounts().is_empty());
    }

    #[tokio::test]
    async fn an_entry_without_a_key_has_none_to_take_back_and_no_host_is_spoken_to() {
        let (server, _home) = host("none").await;
        let store = Memory::default();
        let entry = Entry {
            auth: Auth::Password,
            ..installed(&store, "a", server.port, &server.fingerprint, None)
        };
        assert_eq!(
            take_back(&store, SERVICE, &entry).await,
            KeyRemoval::nothing()
        );
        assert_eq!(server.connections(), 0);
        // A key kept for an entry that does not sign in with it is not kept.
        assert!(store.accounts().is_empty());
    }

    #[tokio::test]
    async fn a_credential_store_that_does_not_remove_the_private_key_is_told_with_the_item_s_name()
    {
        let (server, home) = host("locked").await;
        let store = KeepingStore::default();
        let entry = installed(&store, "a", server.port, &server.fingerprint, Some(&home));
        let removal = take_back(&store, SERVICE, &entry).await;
        // The line is gone from the host, and the owner is not told the private key went with it.
        assert_eq!(removal.line, LineRemoval::Removed);
        assert_eq!(std::fs::read_to_string(keys_file(&home)).unwrap(), OTHERS);
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
        assert_eq!(store.0.accounts(), ["a"]);
        // The same where the host could not be reached: both are told.
        let closed = ClosedPort::new();
        let gone = installed(&store, "b", closed.port(), "SHA256:x", None);
        let removal = take_back(&store, SERVICE, &gone).await;
        left(&gone, &removal.line);
        assert_eq!(
            removal.private_key_left.map(|left| left.account),
            Some("b".to_owned())
        );
    }
}
