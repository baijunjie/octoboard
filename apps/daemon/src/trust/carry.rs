//! Carrying the entry Grok Build writes into its per-session trust store over to the user's own,
//! unchanged, under the store's lock file and by replacing the file, as Grok itself writes it.

use super::*;

/// How long Grok has to write its trust entry once its confirmation was pressed, and how long the
/// user's store may stay locked by another writer.
pub(super) const RECORD_TIMEOUT: Duration = Duration::from_secs(5);
pub(super) const LOCK_TIMEOUT: Duration = Duration::from_secs(2);

/// The longest a carry can take once the hook that starts it has come: waiting for the entry,
/// then for the lock, with room to spare for the reading and writing around them.
pub(super) const CARRY_LONGEST: Duration = RECORD_TIMEOUT
    .saturating_add(LOCK_TIMEOUT)
    .saturating_add(Duration::from_secs(3));

/// Where Grok Build records an accepted confirmation, and where that record has to end up. Grok
/// keeps its trust store in `GROK_HOME` and saves it by replacing the file, and an Octoboard
/// session's `GROK_HOME` is the session's own (`crate::adapter::grok`), so the entry Grok writes
/// lands only in the session's copy and dies with it. Once the confirmation is accepted — by
/// Octoboard's press or by the person in the terminal — that one entry is carried, unchanged, into
/// the user's own store, every other entry there left as it is.
#[derive(Debug, Clone)]
pub struct CarriedTrust {
    /// The session's copy of the store, which Grok rewrites when its confirmation is pressed.
    pub session_store: PathBuf,
    /// The user's own store, the one the session's copy was copied from.
    pub user_store: PathBuf,
    /// The folder Grok records the trust for: the session's working directory, canonical.
    pub folder: String,
}

/// Why Grok's entry did not reach the user's store: one of [`trust_reason`]'s carry codes, the
/// English account of it, and what that account is filled with, if anything.
#[derive(Debug, Clone)]
pub(super) struct CarryFailure {
    pub(super) reason_code: &'static str,
    pub(super) reason: String,
    pub(super) detail: Option<String>,
}

impl CarryFailure {
    pub(super) fn new(
        reason_code: &'static str,
        reason: impl Into<String>,
        detail: Option<String>,
    ) -> Self {
        Self {
            reason_code,
            reason: reason.into(),
            detail,
        }
    }

    fn io(err: impl std::fmt::Display) -> Self {
        let detail = format!("{err:#}");
        Self::new(
            trust_reason::STORE_IO_FAILED,
            format!("the file could not be read or written: {detail}"),
            Some(detail),
        )
    }
}

/// The trust store's lock file stayed held by another writer.
#[derive(Debug)]
pub(super) struct StoreLocked;

impl std::fmt::Display for StoreLocked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the trust store stayed locked")
    }
}

impl std::error::Error for StoreLocked {}

/// Waits for the entry Grok writes for the folder, then carries it into the user's store.
pub(super) fn carry(carried: &CarriedTrust) -> std::result::Result<(), CarryFailure> {
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
pub(super) fn recorded_entry(carried: &CarriedTrust) -> Option<String> {
    let text = std::fs::read_to_string(&carried.session_store).ok()?;
    let span = entry_span(&text, &carried.folder)?;
    let mut entry = text[span].to_string();
    if !entry.ends_with('\n') {
        entry.push('\n');
    }
    let parsed: toml::Table = entry.parse().ok()?;
    let trusted = folder_entry(&parsed, &carried.folder)?.get("trusted")?;
    (trusted.as_bool() == Some(true)).then_some(entry)
}

/// Waits, up to [`RECORD_TIMEOUT`], for the entry Grok writes into the session's copy of its store
/// once its screen is accepted; `None` when it never says the folder is trusted.
pub(super) fn wait_recorded(carried: &CarriedTrust) -> Option<String> {
    let mut entry = None;
    wait_for(RECORD_TIMEOUT, || {
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
/// links to) while it reads and writes, and writes by replacing the file, as Grok does.
pub(super) fn merge_into_user_store(
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
    let target =
        std::fs::canonicalize(&carried.user_store).unwrap_or_else(|_| carried.user_store.clone());
    let current = match std::fs::read_to_string(&target) {
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
pub(super) fn merge_entry(current: &str, folder: &str, entry: &str) -> Result<String> {
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
pub(super) fn folder_entry<'a>(store: &'a toml::Table, folder: &str) -> Option<&'a toml::Table> {
    store.get("folders")?.as_table()?.get(folder)?.as_table()
}

/// Where the entry for `folder` stands in a store's text: from its `[folders."<folder>"]` header to
/// the end of its last key line, so that the blank lines and comments before the next table stay
/// with it. A header is recognised by what it parses to, so any quoting of the path is.
pub(super) fn entry_span(text: &str, folder: &str) -> Option<Range<usize>> {
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
/// held: an exclusive `flock`, given up after [`LOCK_TIMEOUT`] rather than waited on for ever.
pub(super) fn lock_store(store: &Path) -> Result<std::fs::File> {
    use std::os::fd::AsRawFd;

    let mut name = store
        .file_name()
        .ok_or_else(|| anyhow!("the trust store has no file name"))?
        .to_os_string();
    name.push(".lock");
    let path = store.with_file_name(name);
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("opening {}", path.display()))?;
    let deadline = Instant::now() + LOCK_TIMEOUT;
    loop {
        // SAFETY: `flock` on a descriptor this function owns; the lock goes with the file.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(file);
        }
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() != Some(libc::EWOULDBLOCK) {
            return Err(err).with_context(|| format!("locking {}", path.display()));
        }
        if Instant::now() >= deadline {
            return Err(StoreLocked.into());
        }
        std::thread::sleep(POLL);
    }
}

/// Replaces `target` with `text` in one step: written beside it under a name of its own, with the
/// permissions the file had (the owner's alone for a new one), then renamed over it.
pub(super) fn write_replacing(target: &Path, text: &str) -> Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let directory = target
        .parent()
        .ok_or_else(|| anyhow!("{} has no directory", target.display()))?;
    let name = target
        .file_name()
        .ok_or_else(|| anyhow!("{} has no file name", target.display()))?
        .to_string_lossy();
    let staged = directory.join(format!(".{name}.octoboard-{}", std::process::id()));
    let mode = std::fs::metadata(target)
        .map(|metadata| metadata.permissions().mode() & 0o7777)
        .unwrap_or(0o600);
    let written = (|| -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(mode)
            .open(&staged)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(mode))?;
        std::fs::rename(&staged, target)?;
        Ok(())
    })();
    if written.is_err() {
        std::fs::remove_file(&staged).ok();
    }
    written.with_context(|| format!("writing {}", target.display()))
}
