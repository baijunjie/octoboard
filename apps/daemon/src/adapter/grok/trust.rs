//! Grok Build's trust store. For a project session it is carried from the session's own home to
//! the user's: the entry Grok writes when its confirmation is accepted is put into the user's
//! `trusted_folders.toml` unchanged, under the store's lock file and by replacing the file, as Grok
//! itself writes it. For a console session nothing is carried: the console's directory is trusted
//! in the session's own copy of the store, for that launch alone ([`trust_for_this_launch`]).

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::adapter::files::{read_regular_text, LARGE_FILE_LIMIT};
use crate::protocol::trust_reason;
use crate::session::trust::{CarryFailure, TrustCarrier};
use crate::session::wait::{wait_for, POLL};

/// Grok Build's trust store's lock file stayed held by another writer.
#[derive(Debug)]
struct StoreLocked;

impl std::fmt::Display for StoreLocked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the trust store stayed locked")
    }
}

impl std::error::Error for StoreLocked {}

/// How long Grok has to write its trust entry once its confirmation was pressed, and how long the
/// user's store may stay locked by another writer.
pub(crate) const RECORD_TIMEOUT: Duration = Duration::from_secs(5);
const LOCK_TIMEOUT: Duration = Duration::from_secs(2);

/// Where Grok Build records an accepted confirmation, and where that record has to end up. Grok
/// keeps its trust store in `GROK_HOME` and saves it by replacing the file, and an Octoboard
/// session's `GROK_HOME` is the session's own (`super`), so the entry Grok writes
/// lands only in the session's copy and dies with it. Once the confirmation is accepted — by
/// Octoboard's press or by the person in the terminal — that one entry is carried, unchanged, into
/// the user's own store, every other entry there left as it is.
///
/// Kept in the terminal's holder meta for adoption (`super::adoption`), so it crosses builds: a
/// field added later needs `#[serde(default)]`, and it is kept readable by every build in the
/// compatible range: `crate::holder::compat::FLOOR` only moves by the previous-stable release
/// rule.
#[derive(Serialize, Deserialize)]
pub struct CarriedTrust {
    /// The session's copy of the store, which Grok rewrites when its confirmation is pressed.
    pub session_store: PathBuf,
    /// The user's own store, the one the session's copy was copied from.
    pub user_store: PathBuf,
    /// The folder Grok records the trust for: the session's working directory, canonical.
    pub folder: String,
    /// How long the entry is waited for once the hook that carries it has come:
    /// [`RECORD_TIMEOUT`] for a launch. A test whose stand-in writes the entry gives it as long as
    /// a spawned process may take under load rather than race it.
    pub record_wait: Duration,
}

impl TrustCarrier for CarriedTrust {
    fn carry(&self) -> std::result::Result<(), CarryFailure> {
        carry(self)
    }

    /// Waiting for the entry, then for the lock, with room to spare for the reading and writing
    /// around them.
    fn longest(&self) -> Duration {
        self.record_wait
            .saturating_add(LOCK_TIMEOUT)
            .saturating_add(Duration::from_secs(3))
    }

    fn user_store(&self) -> &Path {
        &self.user_store
    }
}

/// Trusts `folder` for one launch, by an entry in `session_store`, the session's own copy of the
/// trust store, which Grok reads as it starts and which goes with the session's home. Grok Build
/// 1.0.92 then shows no confirmation for the folder and leaves the copy as it is, and the user's
/// own store never holds the entry. A folder the copy already trusts is left alone. Every other
/// entry of the copy stays as it was ([`merge_entry`]); a copy that does not read as a store is
/// refused, and Grok then asks.
pub(super) fn trust_for_this_launch(session_store: &Path, folder: &str) -> Result<()> {
    let current = match read_regular_text(session_store, LARGE_FILE_LIMIT) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => {
            return Err(err).with_context(|| format!("reading {}", session_store.display()))
        }
    };
    let trusts = |store: &toml::Table| {
        folder_entry(store, folder)
            .and_then(|entry| entry.get("trusted"))
            .and_then(toml::Value::as_bool)
            == Some(true)
    };
    if current
        .parse::<toml::Table>()
        .is_ok_and(|store| trusts(&store))
    {
        return Ok(());
    }
    let merged = merge_entry(&current, folder, &launch_entry(folder)?)?;
    write_replacing(session_store, &merged)
}

/// The entry that trusts `folder`, in the form Grok writes one: `trusted` and the time of the
/// decision in Unix seconds, under the folder's own table.
fn launch_entry(folder: &str) -> Result<String> {
    let decided_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default();
    let mut entry = toml::Table::new();
    entry.insert("trusted".to_string(), toml::Value::Boolean(true));
    entry.insert(
        "decided_at".to_string(),
        toml::Value::Integer(i64::try_from(decided_at).unwrap_or(i64::MAX)),
    );
    let mut folders = toml::Table::new();
    folders.insert(folder.to_string(), toml::Value::Table(entry));
    let mut store = toml::Table::new();
    store.insert("folders".to_string(), toml::Value::Table(folders));
    toml::to_string(&store).context("writing the trust entry")
}

/// Waits for the entry Grok writes for the folder, then carries it into the user's store.
fn carry(carried: &CarriedTrust) -> std::result::Result<(), CarryFailure> {
    let entry = wait_recorded(carried).ok_or_else(|| {
        CarryFailure::new(
            trust_reason::NOT_RECORDED,
            "no trust entry for this folder appeared in the session's copy of the store",
            None,
        )
    })?;
    merge_into_user_store(carried, &entry)
}

/// The entry Grok has written for the folder into the session's copy of its store, as its own
/// text, once it says the folder is trusted.
fn recorded_entry(carried: &CarriedTrust) -> Option<String> {
    let text = read_regular_text(&carried.session_store, LARGE_FILE_LIMIT).ok()?;
    let span = entry_span(&text, &carried.folder)?;
    let mut entry = text[span].to_string();
    if !entry.ends_with('\n') {
        entry.push('\n');
    }
    let parsed: toml::Table = entry.parse().ok()?;
    let trusted = folder_entry(&parsed, &carried.folder)?.get("trusted")?;
    (trusted.as_bool() == Some(true)).then_some(entry)
}

/// Waits, up to [`CarriedTrust::record_wait`], for the entry Grok writes into the session's copy of
/// its store once its screen is accepted; `None` when it never says the folder is trusted.
fn wait_recorded(carried: &CarriedTrust) -> Option<String> {
    let mut entry = None;
    wait_for(carried.record_wait, || {
        entry = recorded_entry(carried);
        entry.is_some()
    });
    entry
}

/// Carries `entry`, the one Grok wrote into the session's copy of its store, over to the user's
/// own, exactly as Grok wrote it: added when the user's store has no entry for the folder, put in
/// place of the one it has otherwise. Every other byte of the user's store stays as it was, which
/// is checked by reading the result back before it replaces anything. Holds the store's lock file
/// (`trusted_folders.toml.lock`, which Grok keeps beside it and which the session's own home
/// links to where the host makes file links) while it reads and writes, and writes by replacing
/// the file, as Grok does.
fn merge_into_user_store(
    carried: &CarriedTrust,
    entry: &str,
) -> std::result::Result<(), CarryFailure> {
    let _lock = lock_store(&carried.user_store).map_err(|err| {
        if err.downcast_ref::<StoreLocked>().is_some() {
            CarryFailure::new(
                trust_reason::STORE_LOCKED,
                "the file stayed locked by another program",
                None,
            )
        } else {
            CarryFailure::io(err)
        }
    })?;
    // A store the user keeps elsewhere and links to is written where it really is.
    let target = crate::hostfs::canonical(&carried.user_store)
        .unwrap_or_else(|_| carried.user_store.clone());
    let current = match read_regular_text(&target, LARGE_FILE_LIMIT) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(CarryFailure::io(err)),
    };
    // The outermost account alone: a parse error's own text draws a picture of the line.
    let merged = merge_entry(&current, &carried.folder, entry).map_err(|err| {
        CarryFailure::new(
            trust_reason::STORE_UNREADABLE,
            format!("{err}, so the file was left as it is"),
            None,
        )
    })?;
    if merged != current {
        write_replacing(&target, &merged).map_err(CarryFailure::io)?;
    }
    Ok(())
}

/// `current` with the entry for `folder` set to `entry`, Grok's own text for it. Refused, rather
/// than written, when `current` is not a store this can read, or when the result read back differs
/// from `current` in anything but that one entry.
fn merge_entry(current: &str, folder: &str, entry: &str) -> Result<String> {
    let before: toml::Table = current
        .parse()
        .context("the user's trust store is not valid TOML")?;
    let merged = match entry_span(current, folder) {
        Some(span) => format!("{}{entry}{}", &current[..span.start], &current[span.end..]),
        None => {
            let mut merged = current.to_string();
            if !merged.is_empty() {
                if !merged.ends_with('\n') {
                    merged.push('\n');
                }
                merged.push('\n');
            }
            merged.push_str(entry);
            merged
        }
    };

    let entry_table: toml::Table = entry.parse().context("reading Grok's entry")?;
    let wanted = folder_entry(&entry_table, folder)
        .ok_or_else(|| anyhow!("Grok's entry does not name the folder"))?;
    let mut expected = before;
    let folders = expected
        .entry("folders")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()))
        .as_table_mut()
        .ok_or_else(|| anyhow!("the user's trust store has a `folders` that is not a table"))?;
    folders.insert(folder.to_string(), toml::Value::Table(wanted.clone()));
    let after: toml::Table = merged
        .parse()
        .context("adding the entry would leave the user's trust store unreadable")?;
    if after != expected {
        return Err(anyhow!(
            "adding the entry would change more of the user's trust store than that entry"
        ));
    }
    Ok(merged)
}

/// The `folders.<folder>` table of a parsed store.
fn folder_entry<'a>(store: &'a toml::Table, folder: &str) -> Option<&'a toml::Table> {
    store.get("folders")?.as_table()?.get(folder)?.as_table()
}

/// Where the entry for `folder` stands in a store's text: from its `[folders."<folder>"]` header to
/// the end of its last key line, so that the blank lines and comments before the next table stay
/// with it. A header is recognised by what it parses to, so any quoting of the path is.
fn entry_span(text: &str, folder: &str) -> Option<Range<usize>> {
    let names_the_folder = |line: &str| {
        line.trim().parse::<toml::Table>().is_ok_and(|header| {
            header.len() == 1
                && folder_entry(&header, folder).is_some_and(|table| table.is_empty())
                && header
                    .get("folders")
                    .and_then(toml::Value::as_table)
                    .is_some_and(|folders| folders.len() == 1)
        })
    };
    let mut offset = 0;
    let mut found: Option<Range<usize>> = None;
    for line in text.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        let content = line.trim();
        match &mut found {
            Some(span) => {
                if content.starts_with('[') {
                    break;
                }
                if !content.is_empty() && !content.starts_with('#') {
                    span.end = offset;
                }
            }
            None => {
                if content.starts_with('[') && names_the_folder(line) {
                    found = Some(start..offset);
                }
            }
        }
    }
    found
}

/// Takes the store's lock file, which Grok keeps beside it, for as long as the returned file is
/// held: an exclusive lock, given up after [`LOCK_TIMEOUT`] rather than waited on for ever.
fn lock_store(store: &Path) -> Result<std::fs::File> {
    use crate::platform::fs::{try_lock_file, Busy};

    let mut name = store
        .file_name()
        .ok_or_else(|| anyhow!("the trust store has no file name"))?
        .to_os_string();
    name.push(".lock");
    let path = store.with_file_name(name);
    let deadline = Instant::now() + LOCK_TIMEOUT;
    loop {
        match try_lock_file(&path) {
            Ok(file) => return Ok(file),
            Err(Busy::Held) => {}
            Err(Busy::Open(err)) => {
                return Err(err).with_context(|| format!("opening {}", path.display()))
            }
            Err(Busy::Lock(err)) => {
                return Err(err).with_context(|| format!("locking {}", path.display()))
            }
        }
        if Instant::now() >= deadline {
            return Err(StoreLocked.into());
        }
        std::thread::sleep(POLL);
    }
}

/// Replaces `target` with `text` in one step, keeping the permissions the file had (the owner's
/// alone for a new one).
fn write_replacing(target: &Path, text: &str) -> Result<()> {
    use crate::hostfs::replace::{replace_beside, Permissions};

    replace_beside(target, text.as_bytes(), Permissions::KeptOrPrivate)
        .with_context(|| format!("writing {}", target.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A console's directory is trusted in the session's copy of the store alone: the entry reads
    /// as Grok's own, the entries the copy carried stay byte for byte, a folder already trusted
    /// there is not written again, and a copy that does not read is left as it is.
    #[test]
    fn a_launch_trusts_its_folder_in_the_sessions_copy_only() {
        let dir = crate::test_support::ScratchDir::new("grok-launch-trust");
        let store = dir.join("trusted_folders.toml");
        let folder = "/home/me/.octoboard/consoles/c \"1\"";

        trust_for_this_launch(&store, folder).expect("a store is made");
        let made = std::fs::read_to_string(&store).expect("the store");
        let parsed: toml::Table = made.parse().expect("a store Grok reads");
        let entry = folder_entry(&parsed, folder).expect("the folder's entry");
        assert_eq!(
            entry.get("trusted").and_then(toml::Value::as_bool),
            Some(true)
        );
        assert!(entry.get("decided_at").is_some_and(toml::Value::is_integer));

        trust_for_this_launch(&store, folder).expect("already trusted");
        assert_eq!(std::fs::read_to_string(&store).expect("the store"), made);

        let others = "# mine\n[folders.\"/a\"]\ntrusted = true\ndecided_at = 5\n";
        std::fs::write(&store, others).expect("the user's entries");
        trust_for_this_launch(&store, folder).expect("added");
        let added = std::fs::read_to_string(&store).expect("the store");
        assert!(added.starts_with(others), "{added}");
        let parsed: toml::Table = added.parse().expect("a store Grok reads");
        assert!(folder_entry(&parsed, folder).is_some() && folder_entry(&parsed, "/a").is_some());

        std::fs::write(&store, "[folders\n").expect("a broken store");
        assert!(trust_for_this_launch(&store, folder).is_err());
        assert_eq!(
            std::fs::read_to_string(&store).expect("the store"),
            "[folders\n"
        );
    }

    /// Only Grok's entry changes in the user's store, exactly as Grok wrote it — added, or put in
    /// place of a stale one — and every other byte stays; a store that cannot be read is left
    /// alone.
    #[test]
    fn only_groks_entry_changes_in_the_users_store() {
        let entry = "[folders.\"/work/project\"]\ntrusted = true\ndecided_at = 1791531653\n";
        let others =
            "# mine\n[folders.\"/a\"]\ntrusted = true\n\n[folders.'/b']\ntrusted   =   true\n";
        assert_eq!(
            merge_entry(others, "/work/project", entry).unwrap(),
            format!("{others}\n{entry}")
        );
        let stale =
            "[folders.\"/a\"]\ntrusted = true\n\n[folders.\"/work/project\"]\ntrusted = false\n\
                     decided_at = 5\n\n# about b\n[folders.\"/b\"]\ntrusted = true\n";
        assert_eq!(
            merge_entry(stale, "/work/project", entry).unwrap(),
            stale.replace(
                "trusted = false\ndecided_at = 5\n",
                "trusted = true\ndecided_at = 1791531653\n"
            )
        );
        assert!(merge_entry("[folders\n", "/work/project", entry).is_err());
    }
}
