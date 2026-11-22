//! The remote daemons the owner has added, kept in `remote-daemons.json` under the application's
//! config directory: for each, the name its window shows, the SSH host, port and user, how to
//! authenticate, the host key fingerprint pinned for it and the path of `octoboardd` where it is
//! not found by itself. No password or passphrase is ever written here: one is asked for at a
//! connection and kept in memory for that connection's reconnections alone. Once this device's own
//! key is installed on the remote host (`device_key`), the entry authenticates with it, and its
//! private key is kept in the system's credential store, not here. What the owner answered when
//! asked about that key, and where its line was added, is kept with the entry (`DeviceKey`).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config_dir::write_json;
use crate::diagnostics::diagnostic;

/// The file the entries are kept in, under the application's config directory.
pub const FILE_NAME: &str = "remote-daemons.json";

/// How the application authenticates to the remote host.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Auth {
    /// The remote user's password, asked for at every connection.
    Password,
    /// A private key file on this computer, its passphrase asked for when it has one.
    Key,
    /// This device's own key, installed on the host after a login with one of the others and kept
    /// in the system's credential store. An entry that authenticates so has its key installed,
    /// whichever build installed it.
    Device,
}

/// What an entry keeps about this device's key.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DeviceKey {
    /// The owner said no to installing the key, or turned it off: nothing is installed and
    /// nothing asked until it is turned on again.
    pub declined: bool,
    /// The file on the host the key's line was added to, as the owner was told of it; `None` with
    /// no key installed, and for a key an earlier build installed.
    pub file: Option<String>,
    /// What the line is marked with there (`device_key::comment`); `None` as `file` is.
    pub mark: Option<String>,
}

/// One remote daemon, as the UI sees and edits it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: Auth,
    pub key_path: Option<String>,
    /// The pinned host key fingerprint, `SHA256:<base64>` as OpenSSH prints it; `None` until the
    /// owner confirms the one shown at a first connection.
    pub fingerprint: Option<String>,
    pub octoboardd_path: Option<String>,
    /// Absent from a file an earlier build wrote.
    #[serde(default)]
    pub device_key: DeviceKey,
}

/// An entry as the UI sends it to be saved: `id` is `None` for a new one. What the entry keeps
/// about this device's key is not the form's to send; it is changed by a connection's answer and
/// by the entry's switch.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryInput {
    pub id: Option<String>,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: Auth,
    pub key_path: Option<String>,
    pub fingerprint: Option<String>,
    pub octoboardd_path: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct Stored {
    entries: Vec<Entry>,
}

impl Entry {
    /// What the window of this daemon remembers its frame and preferences under: the SSH
    /// destination, which stays the same across connections and across entries naming the same
    /// daemon.
    pub fn identity(&self) -> String {
        format!("ssh:{}@{}:{}", self.user, self.host, self.port)
    }

    /// Whether the device key installed for `previous` is of no use to this entry: it names another
    /// destination, where the key was never installed.
    pub fn outlives_device_key_of(&self, previous: &Entry) -> bool {
        self.identity() != previous.identity()
    }

    /// Whether this device's key is installed for the entry.
    pub fn has_device_key(&self) -> bool {
        self.auth == Auth::Device
    }

    /// Whether saving this entry in place of `previous` gives up the device key installed for
    /// `previous`, which is then taken back from its host first.
    pub fn gives_up_device_key_of(&self, previous: &Entry) -> bool {
        previous.has_device_key() && !self.has_device_key()
    }
}

/// The entries kept at `path`: none when there is no such file, and an error, with the system's
/// or the decoder's own words, when the file is there and cannot be read or is not what `save`
/// writes. For whoever must not take a file it cannot read for "nothing saved".
pub fn read(path: &Path) -> Result<Vec<Entry>, String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(format!("{} could not be read: {err}", path.display())),
    };
    serde_json::from_slice::<Stored>(&bytes)
        .map(|stored| stored.entries)
        .map_err(|err| format!("{} could not be read: {err}", path.display()))
}

/// The entries kept at `path`; none when the file is missing or unreadable.
pub fn load(path: &Path) -> Vec<Entry> {
    read(path).unwrap_or_else(|err| {
        diagnostic!("ignoring the saved remote daemons: {err}");
        Vec::new()
    })
}

/// Writes `entries` to `path`.
pub fn save(path: &Path, entries: &[Entry]) -> anyhow::Result<()> {
    write_json(
        path,
        &Stored {
            entries: entries.to_vec(),
        },
    )
}

/// Trims what the owner typed and refuses an entry the application could not connect with.
fn validated(input: EntryInput) -> Result<EntryInput, String> {
    let trimmed = |value: Option<String>| {
        value
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    };
    let host = input.host.trim().to_owned();
    let user = input.user.trim().to_owned();
    let name = match input.name.trim() {
        "" => host.clone(),
        name => name.to_owned(),
    };
    if host.is_empty() || host.chars().any(char::is_whitespace) {
        return Err("The host is missing or contains spaces.".into());
    }
    if user.is_empty() || user.chars().any(char::is_whitespace) {
        return Err("The user is missing or contains spaces.".into());
    }
    if input.port == 0 {
        return Err("The SSH port must be between 1 and 65535.".into());
    }
    let key_path = trimmed(input.key_path);
    if input.auth == Auth::Key && key_path.is_none() {
        return Err("A key file is needed to authenticate with a key.".into());
    }
    Ok(EntryInput {
        id: input.id,
        name,
        host,
        port: input.port,
        user,
        auth: input.auth,
        key_path: if input.auth == Auth::Key {
            key_path
        } else {
            None
        },
        fingerprint: trimmed(input.fingerprint),
        octoboardd_path: trimmed(input.octoboardd_path),
    })
}

/// Saves `input` into `entries`, replacing the entry of its id or adding a new one under the id
/// `new_id` makes, and returns the entry as saved. The device key is kept only by an entry that
/// already has one installed at the same destination; any other entry asking for it is saved to
/// authenticate with a password, which installs a key again.
pub fn upsert(
    entries: &mut Vec<Entry>,
    input: EntryInput,
    new_id: impl FnOnce() -> String,
) -> Result<Entry, String> {
    let input = validated(input)?;
    let id = match input.id {
        Some(id) if entries.iter().any(|entry| entry.id == id) => id,
        Some(_) => return Err("This remote daemon has been removed.".into()),
        None => new_id(),
    };
    let mut entry = Entry {
        id: id.clone(),
        name: input.name,
        host: input.host,
        port: input.port,
        user: input.user,
        auth: input.auth,
        key_path: input.key_path,
        fingerprint: input.fingerprint,
        octoboardd_path: input.octoboardd_path,
        device_key: entries
            .iter()
            .find(|existing| existing.id == id)
            .map(|existing| existing.device_key.clone())
            .unwrap_or_default(),
    };
    if entry.auth == Auth::Device {
        let installed = entries.iter().any(|existing| {
            existing.id == id
                && existing.auth == Auth::Device
                && !entry.outlives_device_key_of(existing)
        });
        if !installed {
            entry.auth = Auth::Password;
        }
    }
    // Where the line is belongs to an installed key alone; the owner's no stays with the entry.
    if entry.auth != Auth::Device {
        entry.device_key.file = None;
        entry.device_key.mark = None;
    }
    match entries.iter_mut().find(|existing| existing.id == id) {
        Some(existing) => *existing = entry.clone(),
        None => entries.push(entry.clone()),
    }
    Ok(entry)
}

/// Pins `fingerprint` for the entry `id`, unless the owner has pinned another one since; whether
/// it is pinned now.
pub fn pin(entries: &mut [Entry], id: &str, fingerprint: &str) -> bool {
    let Some(entry) = entries.iter_mut().find(|entry| entry.id == id) else {
        return false;
    };
    match &entry.fingerprint {
        Some(pinned) => same_fingerprint(pinned, fingerprint),
        None => {
            entry.fingerprint = Some(fingerprint.to_owned());
            true
        }
    }
}

/// Switches the entry `id` to this device's key, installed for `identity` as a line marked `mark`
/// in the host's `file`, unless the owner has pointed it at another destination since; whether it
/// is switched now.
pub fn use_device_key(
    entries: &mut [Entry],
    id: &str,
    identity: &str,
    file: &str,
    mark: &str,
) -> bool {
    let Some(entry) = entries
        .iter_mut()
        .find(|entry| entry.id == id && entry.identity() == identity)
    else {
        return false;
    };
    entry.auth = Auth::Device;
    entry.key_path = None;
    entry.device_key = DeviceKey {
        declined: false,
        file: Some(file.to_owned()),
        mark: Some(mark.to_owned()),
    };
    true
}

/// Turns this device's key off for the entry `id`, which signs in with a password from now on
/// where it signed in with the key, and is not asked about the key again; whether there is such
/// an entry.
pub fn decline_device_key(entries: &mut [Entry], id: &str) -> bool {
    let Some(entry) = entries.iter_mut().find(|entry| entry.id == id) else {
        return false;
    };
    if entry.auth == Auth::Device {
        entry.auth = Auth::Password;
    }
    entry.device_key = DeviceKey {
        declined: true,
        file: None,
        mark: None,
    };
    true
}

/// Notes the owner's no to this device's key for the entry `id` and changes nothing else: an
/// entry whose key is installed goes on saying so until the key has been taken out of its host
/// (`decline_device_key`). Whether there is such an entry.
pub fn note_declined_device_key(entries: &mut [Entry], id: &str) -> bool {
    let Some(entry) = entries.iter_mut().find(|entry| entry.id == id) else {
        return false;
    };
    entry.device_key.declined = true;
    true
}

/// Turns this device's key on again for the entry `id`: its next connection asks before
/// installing it. Whether there is such an entry.
pub fn allow_device_key(entries: &mut [Entry], id: &str) -> bool {
    let Some(entry) = entries.iter_mut().find(|entry| entry.id == id) else {
        return false;
    };
    entry.device_key.declined = false;
    true
}

/// Whether two fingerprints name the same key, however the owner copied them: with or without the
/// `SHA256:` prefix and the base64 padding.
pub fn same_fingerprint(a: &str, b: &str) -> bool {
    fn bare(value: &str) -> &str {
        let value = value.trim();
        value
            .strip_prefix("SHA256:")
            .unwrap_or(value)
            .trim_end_matches('=')
    }
    let (a, b) = (bare(a), bare(b));
    !a.is_empty() && a == b
}

/// A new entry's id: unique among the entries of this computer, which is all it needs to be.
pub fn new_id() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    format!("{nanos:x}{:x}", COUNTER.fetch_add(1, Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_dir::test_support::Scratch;

    /// Switches the entry to a key installed in the usual file under the usual mark.
    fn install(entries: &mut [Entry], id: &str, identity: &str) -> bool {
        use_device_key(
            entries,
            id,
            identity,
            "~/.ssh/authorized_keys",
            "Octoboard on Desk",
        )
    }

    fn input(id: Option<&str>) -> EntryInput {
        EntryInput {
            id: id.map(str::to_owned),
            name: " ".into(),
            host: " server.example ".into(),
            port: 22,
            user: "me".into(),
            auth: Auth::Password,
            key_path: Some("~/.ssh/id_ed25519".into()),
            fingerprint: Some("  ".into()),
            octoboardd_path: None,
        }
    }

    #[test]
    fn a_new_entry_is_trimmed_named_after_its_host_and_keeps_no_key_path_for_a_password() {
        let mut entries = Vec::new();
        let entry = upsert(&mut entries, input(None), || "a".into()).unwrap();
        assert_eq!(entry.id, "a");
        assert_eq!(entry.name, "server.example");
        assert_eq!(entry.host, "server.example");
        assert_eq!(entry.key_path, None);
        assert_eq!(entry.fingerprint, None);
        assert_eq!(entries, vec![entry.clone()]);
        assert_eq!(entry.identity(), "ssh:me@server.example:22");
    }

    #[test]
    fn saving_an_existing_entry_replaces_it_and_a_removed_one_is_refused() {
        let mut entries = Vec::new();
        upsert(&mut entries, input(None), || "a".into()).unwrap();
        let mut changed = input(Some("a"));
        changed.port = 2222;
        upsert(&mut entries, changed, || unreachable!()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].port, 2222);
        assert!(upsert(&mut entries, input(Some("gone")), || unreachable!()).is_err());
    }

    #[test]
    fn an_entry_that_cannot_be_connected_with_is_refused() {
        let mut entries = Vec::new();
        let mut no_host = input(None);
        no_host.host = "  ".into();
        assert!(upsert(&mut entries, no_host, new_id).is_err());
        let mut no_user = input(None);
        no_user.user = "".into();
        assert!(upsert(&mut entries, no_user, new_id).is_err());
        let mut no_port = input(None);
        no_port.port = 0;
        assert!(upsert(&mut entries, no_port, new_id).is_err());
        let mut no_key = input(None);
        no_key.auth = Auth::Key;
        no_key.key_path = Some(" ".into());
        assert!(upsert(&mut entries, no_key, new_id).is_err());
        assert!(entries.is_empty());
    }

    #[test]
    fn a_fingerprint_is_pinned_only_where_none_is() {
        let mut entries = Vec::new();
        upsert(&mut entries, input(None), || "a".into()).unwrap();
        assert!(pin(&mut entries, "a", "SHA256:abc"));
        assert_eq!(entries[0].fingerprint.as_deref(), Some("SHA256:abc"));
        assert!(pin(&mut entries, "a", "abc="));
        assert!(!pin(&mut entries, "a", "SHA256:other"));
        assert_eq!(entries[0].fingerprint.as_deref(), Some("SHA256:abc"));
        assert!(!pin(&mut entries, "missing", "SHA256:abc"));
    }

    #[test]
    fn an_entry_switches_to_the_device_key_installed_for_its_destination_alone() {
        let mut entries = Vec::new();
        let mut keyed = input(None);
        keyed.auth = Auth::Key;
        let saved = upsert(&mut entries, keyed, || "a".into()).unwrap();
        assert_eq!(saved.key_path.as_deref(), Some("~/.ssh/id_ed25519"));
        assert!(!install(&mut entries, "a", "ssh:me@elsewhere:22"));
        assert!(!install(&mut entries, "missing", &saved.identity()));
        assert_eq!(entries[0].auth, Auth::Key);
        assert!(install(&mut entries, "a", &saved.identity()));
        assert_eq!(entries[0].auth, Auth::Device);
        assert_eq!(entries[0].key_path, None);
        // Where the line is, and what it is marked with, is kept for the owner to be told.
        assert_eq!(
            entries[0].device_key,
            DeviceKey {
                declined: false,
                file: Some("~/.ssh/authorized_keys".into()),
                mark: Some("Octoboard on Desk".into()),
            }
        );
    }

    #[test]
    fn the_device_key_is_kept_by_an_entry_that_has_it_at_the_same_destination_alone() {
        let mut entries = Vec::new();
        let mut asked = input(None);
        asked.auth = Auth::Device;
        // A new entry has no key installed yet.
        let saved = upsert(&mut entries, asked, || "a".into()).unwrap();
        assert_eq!(saved.auth, Auth::Password);
        assert!(install(&mut entries, "a", &saved.identity()));
        // Renamed, it keeps the key.
        let mut renamed = input(Some("a"));
        renamed.name = "Renamed".into();
        renamed.auth = Auth::Device;
        let kept = upsert(&mut entries, renamed, || unreachable!()).unwrap();
        assert_eq!(kept.auth, Auth::Device);
        assert!(!kept.outlives_device_key_of(&saved));
        assert!(!kept.gives_up_device_key_of(&entries[0].clone()));
        assert_eq!(kept.device_key.mark.as_deref(), Some("Octoboard on Desk"));
        // Pointed at another destination, it signs in with a password again.
        let mut moved = input(Some("a"));
        moved.user = "other".into();
        moved.auth = Auth::Device;
        let moved = upsert(&mut entries, moved, || unreachable!()).unwrap();
        assert_eq!(moved.auth, Auth::Password);
        assert!(moved.outlives_device_key_of(&kept));
        // The key installed at the destination it left is given up, and nothing says where a line
        // is any more; it was never declined, so the new destination is asked about.
        assert!(moved.gives_up_device_key_of(&kept));
        assert_eq!(moved.device_key, DeviceKey::default());
    }

    #[test]
    fn an_entry_saved_to_sign_in_another_way_gives_up_its_device_key() {
        let mut entries = Vec::new();
        let saved = upsert(&mut entries, input(None), || "a".into()).unwrap();
        assert!(install(&mut entries, "a", &saved.identity()));
        let installed = entries[0].clone();
        let changed = upsert(&mut entries, input(Some("a")), || unreachable!()).unwrap();
        assert_eq!(changed.auth, Auth::Password);
        assert!(changed.gives_up_device_key_of(&installed));
        assert_eq!(changed.device_key, DeviceKey::default());
        // An entry without a key gives none up.
        assert!(!changed.gives_up_device_key_of(&changed));
    }

    #[test]
    fn a_declined_key_is_remembered_until_it_is_turned_on_again() {
        let mut entries = Vec::new();
        let saved = upsert(&mut entries, input(None), || "a".into()).unwrap();
        assert!(!decline_device_key(&mut entries, "missing"));
        assert!(decline_device_key(&mut entries, "a"));
        assert!(entries[0].device_key.declined);
        assert_eq!(entries[0].auth, Auth::Password);
        // Saving the form keeps the answer, whatever the form sent.
        let mut renamed = input(Some("a"));
        renamed.name = "Renamed".into();
        assert!(
            upsert(&mut entries, renamed, || unreachable!())
                .unwrap()
                .device_key
                .declined
        );
        assert!(!allow_device_key(&mut entries, "missing"));
        assert!(allow_device_key(&mut entries, "a"));
        assert!(!entries[0].device_key.declined);
        // A no that is only noted leaves an installed key shown as installed, where its line is
        // included, until the key has been taken out of its host.
        assert!(install(&mut entries, "a", &saved.identity()));
        assert!(!note_declined_device_key(&mut entries, "missing"));
        assert!(note_declined_device_key(&mut entries, "a"));
        assert_eq!(entries[0].auth, Auth::Device);
        assert_eq!(
            entries[0].device_key,
            DeviceKey {
                declined: true,
                file: Some("~/.ssh/authorized_keys".into()),
                mark: Some("Octoboard on Desk".into()),
            }
        );
        assert!(allow_device_key(&mut entries, "a"));
        // Turned off where it is installed, the entry signs in with a password and no longer says
        // where a line is.
        assert!(install(&mut entries, "a", &saved.identity()));
        assert!(decline_device_key(&mut entries, "a"));
        assert_eq!(entries[0].auth, Auth::Password);
        assert_eq!(
            entries[0].device_key,
            DeviceKey {
                declined: true,
                file: None,
                mark: None
            }
        );
    }

    #[test]
    fn a_file_an_earlier_build_wrote_reads_with_no_answer_about_the_key() {
        let stored: Stored = serde_json::from_value(serde_json::json!({"entries": [{
            "id": "a", "name": "n", "host": "h", "port": 22, "user": "u", "auth": "device",
            "keyPath": null, "fingerprint": null, "octoboarddPath": null
        }]}))
        .unwrap();
        assert_eq!(stored.entries[0].device_key, DeviceKey::default());
        // Its key counts as installed all the same.
        assert!(stored.entries[0].has_device_key());
    }

    #[test]
    fn fingerprints_compare_without_prefix_or_padding() {
        assert!(same_fingerprint("SHA256:abc", "abc"));
        assert!(same_fingerprint("SHA256:abc=", " SHA256:abc "));
        assert!(!same_fingerprint("SHA256:abc", "SHA256:abd"));
        assert!(!same_fingerprint("", "SHA256:"));
    }

    #[test]
    fn entries_survive_a_save_and_a_missing_or_broken_file_is_empty() {
        let scratch = Scratch::new("remote", "entries");
        let path = scratch.0.join(FILE_NAME);
        assert!(load(&path).is_empty());
        let mut entries = Vec::new();
        upsert(&mut entries, input(None), || "a".into()).unwrap();
        save(&path, &entries).unwrap();
        assert_eq!(load(&path), entries);
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(!written.contains("password\":"), "{written}");
        std::fs::write(&path, b"{not json").unwrap();
        assert!(load(&path).is_empty());
    }

    #[test]
    fn a_host_or_user_with_a_space_inside_is_refused() {
        let mut entries = Vec::new();
        let mut host = input(None);
        host.host = "my server".into();
        assert!(upsert(&mut entries, host, new_id).is_err());
        let mut user = input(None);
        user.user = "my user".into();
        assert!(upsert(&mut entries, user, new_id).is_err());
        assert!(entries.is_empty());
    }

    #[test]
    fn what_is_typed_is_saved_trimmed_and_an_empty_optional_field_is_none() {
        let mut entries = Vec::new();
        let mut typed = input(None);
        typed.name = "  Build box ".into();
        typed.user = " me ".into();
        typed.fingerprint = Some(" SHA256:abc ".into());
        typed.octoboardd_path = Some("  /opt/octoboardd ".into());
        let saved = upsert(&mut entries, typed, || "a".into()).unwrap();
        assert_eq!(saved.name, "Build box");
        assert_eq!(saved.user, "me");
        assert_eq!(saved.fingerprint.as_deref(), Some("SHA256:abc"));
        assert_eq!(saved.octoboardd_path.as_deref(), Some("/opt/octoboardd"));
        let mut blank = input(Some("a"));
        blank.octoboardd_path = Some("   ".into());
        let saved = upsert(&mut entries, blank, || unreachable!()).unwrap();
        assert_eq!(saved.octoboardd_path, None);
    }

    #[test]
    fn a_key_file_entry_keeps_its_trimmed_path_and_a_failed_save_changes_nothing() {
        let mut entries = Vec::new();
        let mut keyed = input(None);
        keyed.auth = Auth::Key;
        keyed.key_path = Some("  ~/.ssh/id_rsa ".into());
        let saved = upsert(&mut entries, keyed, || "a".into()).unwrap();
        assert_eq!(saved.key_path.as_deref(), Some("~/.ssh/id_rsa"));
        let before = entries.clone();
        let mut bad = input(Some("a"));
        bad.port = 0;
        assert!(upsert(&mut entries, bad, || unreachable!()).is_err());
        assert_eq!(entries, before);
    }

    #[test]
    fn an_entry_that_signed_in_with_a_password_asking_for_the_device_key_is_not_given_it() {
        // The key is installed by a login with a password or key file and then switched to
        // (`use_device_key`); saving the form does not claim one is installed.
        let mut entries = Vec::new();
        upsert(&mut entries, input(None), || "a".into()).unwrap();
        let mut asked = input(Some("a"));
        asked.auth = Auth::Device;
        let saved = upsert(&mut entries, asked, || unreachable!()).unwrap();
        assert_eq!(saved.auth, Auth::Password);
    }

    #[test]
    fn two_entries_for_one_destination_are_kept_apart_by_their_ids() {
        let mut entries = Vec::new();
        let first = upsert(&mut entries, input(None), || "a".into()).unwrap();
        let second = upsert(&mut entries, input(None), || "b".into()).unwrap();
        assert_eq!(entries.len(), 2);
        assert_ne!(first.id, second.id);
        // They name one window identity, so one daemon has one window however it was added.
        assert_eq!(first.identity(), second.identity());
    }

    #[test]
    fn new_ids_are_never_empty_and_never_repeat() {
        let ids: std::collections::HashSet<String> = (0..1000).map(|_| new_id()).collect();
        assert_eq!(ids.len(), 1000);
        assert!(ids.iter().all(|id| !id.is_empty()));
    }

    #[test]
    fn an_entry_is_handed_to_the_ui_by_camel_case_names_and_the_auth_by_a_lowercase_one() {
        let mut keyed = input(None);
        keyed.auth = Auth::Key;
        let mut entries = Vec::new();
        let saved = upsert(&mut entries, keyed, || "a".into()).unwrap();
        let json = serde_json::to_value(&saved).unwrap();
        assert_eq!(json["auth"], "key");
        assert_eq!(json["keyPath"], "~/.ssh/id_ed25519");
        assert!(json["fingerprint"].is_null());
        for auth in [Auth::Password, Auth::Device] {
            let expected = if auth == Auth::Password {
                "password"
            } else {
                "device"
            };
            assert_eq!(serde_json::to_value(auth).unwrap(), expected);
        }
        // What the UI sends to be saved is read by the same names.
        let sent: EntryInput = serde_json::from_value(serde_json::json!({
            "id": null, "name": "n", "host": "h", "port": 22, "user": "u", "auth": "device",
            "keyPath": null, "fingerprint": null, "octoboarddPath": "/x"
        }))
        .unwrap();
        assert_eq!(sent.auth, Auth::Device);
        assert_eq!(sent.octoboardd_path.as_deref(), Some("/x"));
    }

    #[test]
    fn a_saved_file_holds_no_secret_whatever_the_entry_signs_in_with() {
        let scratch = Scratch::new("remote", "entries-secret");
        let path = scratch.0.join(FILE_NAME);
        let mut entries = Vec::new();
        for (index, auth) in [Auth::Password, Auth::Key, Auth::Device]
            .into_iter()
            .enumerate()
        {
            let mut typed = input(None);
            typed.auth = auth;
            typed.host = format!("host{index}");
            let saved = upsert(&mut entries, typed, || format!("e{index}")).unwrap();
            if auth == Auth::Device {
                assert!(install(&mut entries, &saved.id, &saved.identity()));
            }
        }
        save(&path, &entries).unwrap();
        let written = std::fs::read_to_string(&path).unwrap().to_lowercase();
        for forbidden in ["secret", "passphrase", "\"password\":", "private"] {
            assert!(!written.contains(forbidden), "{forbidden} in {written}");
        }
    }
}
