//! The file system steps sharing is made of: looking an entry up without following it, moving
//! one (across volumes too), merging a directory into another, setting an entry aside under a
//! backup name. Each answers an `io::Error` and leaves deciding what a failure means to its
//! caller.

use std::io;
use std::path::{Path, PathBuf};

use crate::relocate::{modified, remove_any};

/// The entry at `path` itself, a link not followed; `None` when there is none.
pub(super) fn lstat(path: &Path) -> io::Result<Option<std::fs::Metadata>> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

/// Whether the entry [`lstat`] found is a link.
pub(super) fn is_link(metadata: &Option<std::fs::Metadata>) -> bool {
    metadata
        .as_ref()
        .is_some_and(|metadata| metadata.file_type().is_symlink())
}

/// `path` with every link resolved, `None` when it does not lead to anything.
pub(super) fn resolve(path: &Path) -> io::Result<Option<PathBuf>> {
    match std::fs::canonicalize(path) {
        Ok(resolved) => Ok(Some(resolved)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

/// [`resolve`], for a path that has to lead somewhere.
pub(super) fn resolve_existing(path: &Path) -> io::Result<PathBuf> {
    resolve(path)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("`{}` does not exist", path.display()),
        )
    })
}

/// [`resolve`], or `path` as it is spelled when it cannot be resolved.
pub(super) fn resolve_or_spelled(path: &Path) -> PathBuf {
    resolve(path)
        .ok()
        .flatten()
        .unwrap_or_else(|| path.to_path_buf())
}

/// Where the link `entry` leads, made absolute.
pub(super) fn link_target(entry: &Path) -> Option<PathBuf> {
    let target = std::fs::read_link(entry).ok()?;
    Some(match entry.parent() {
        Some(parent) if target.is_relative() => parent.join(target),
        _ => target,
    })
}

/// Puts at `entry` a link to `target`.
pub(super) fn link(entry: &Path, target: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, entry)
}

/// The exact name `entry` is backed up under by the operation stamped `stamp`:
/// `<name>.bak-<stamp>`.
pub(super) fn backup_name(entry: &Path, stamp: &str) -> PathBuf {
    let mut name = entry.file_name().unwrap_or_default().to_owned();
    name.push(format!(".bak-{stamp}"));
    entry.with_file_name(name)
}

/// The name an entry is set aside under: [`backup_name`], or with `-2`, `-3`, … added when that is
/// taken.
pub(super) fn backup_path(entry: &Path, stamp: &str) -> PathBuf {
    let first = backup_name(entry, stamp);
    let mut candidate = first.clone();
    let mut n = 2;
    while std::fs::symlink_metadata(&candidate).is_ok() {
        let mut name = first.file_name().unwrap_or_default().to_owned();
        name.push(format!("-{n}"));
        candidate = first.with_file_name(name);
        n += 1;
    }
    candidate
}

/// A stamp for the backups an operation makes now.
pub(super) fn new_stamp() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0);
    crate::rfc3339::compact_utc_stamp(seconds)
}

/// Renames `from` to `to`, copying it ([`copy_path`]) and removing it only then when the two are
/// on different volumes.
pub(super) fn move_path(from: &Path, to: &Path) -> io::Result<()> {
    match std::fs::rename(from, to) {
        Err(err) if err.kind() == io::ErrorKind::CrossesDevices => {
            copy_path(from, to)?;
            remove_any(from)
        }
        other => other,
    }
}

/// Copies `from` to `to` under a staging name beside `to` first, renamed into place once whole,
/// so `to` never holds half a copy.
pub(super) fn copy_path(from: &Path, to: &Path) -> io::Result<()> {
    let staging = to.with_file_name(format!(
        ".{}.octoboard-copy-{}",
        to.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        std::process::id()
    ));
    let copied = copy_any(from, &staging).and_then(|()| std::fs::rename(&staging, to));
    if copied.is_err() {
        remove_any(&staging).ok();
    }
    copied
}

/// Copies a file, a directory with everything in it, or a link, keeping files' modification
/// times: which copy of a conversation record is the later one is told by them. A relative link
/// is refused, as it would lead elsewhere from its new place.
fn copy_any(from: &Path, to: &Path) -> io::Result<()> {
    let metadata = std::fs::symlink_metadata(from)?;
    if metadata.file_type().is_symlink() {
        let target = std::fs::read_link(from)?;
        if target.is_relative() {
            return Err(io::Error::other(format!(
                "`{}` is a relative link",
                from.display()
            )));
        }
        link(to, &target)
    } else if metadata.is_dir() {
        std::fs::create_dir(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            copy_any(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to)?;
        std::fs::File::options()
            .write(true)
            .open(to)?
            .set_modified(metadata.modified()?)
    }
}

fn same_content(a: &Path, b: &Path) -> io::Result<bool> {
    use std::io::Read;
    if std::fs::metadata(a)?.len() != std::fs::metadata(b)?.len() {
        return Ok(false);
    }
    let (mut a, mut b) = (std::fs::File::open(a)?, std::fs::File::open(b)?);
    let (mut left, mut right) = (vec![0u8; 64 * 1024], vec![0u8; 64 * 1024]);
    loop {
        let read = a.read(&mut left)?;
        if read == 0 {
            return Ok(true);
        }
        b.read_exact(&mut right[..read])?;
        if left[..read] != right[..read] {
            return Ok(false);
        }
    }
}

/// Moves `from` to `into`, where something may be already. A directory is merged entry by entry.
/// Where both hold a file, the two are copies of one conversation record, and the one written to
/// last is the conversation as it went on (a record is only ever appended to): it takes the place
/// at `into`, and the other is kept at `aside`, unless the two are identical.
pub(super) fn merge_move(from: &Path, into: &Path, aside: &Path) -> io::Result<()> {
    merge(from, into, aside, false)
}

/// [`merge_move`], leaving `from` where it is: the copy at `into` is made of it.
pub(super) fn merge_copy(from: &Path, into: &Path, aside: &Path) -> io::Result<()> {
    merge(from, into, aside, true)
}

fn merge(from: &Path, into: &Path, aside: &Path, keep_source: bool) -> io::Result<()> {
    let carry = |from: &Path, to: &Path| {
        if keep_source {
            copy_path(from, to)
        } else {
            move_path(from, to)
        }
    };
    let source = std::fs::symlink_metadata(from)?.file_type();
    match lstat(into)? {
        None => {
            if let Some(parent) = into.parent() {
                std::fs::create_dir_all(parent)?;
            }
            carry(from, into)
        }
        Some(existing) if source.is_dir() && existing.is_dir() => {
            for entry in std::fs::read_dir(from)? {
                let name = entry?.file_name();
                merge(
                    &from.join(&name),
                    &into.join(&name),
                    &aside.join(&name),
                    keep_source,
                )?;
            }
            if keep_source {
                Ok(())
            } else {
                std::fs::remove_dir(from)
            }
        }
        Some(existing) if source.is_file() && existing.is_file() && same_content(from, into)? => {
            if keep_source {
                Ok(())
            } else {
                std::fs::remove_file(from)
            }
        }
        Some(_) => {
            if let Some(parent) = aside.parent() {
                std::fs::create_dir_all(parent)?;
            }
            if source.is_file() && modified(from) > modified(into) {
                move_path(into, aside)?;
                carry(from, into)
            } else if keep_source {
                Ok(())
            } else {
                move_path(from, aside)
            }
        }
    }
}

/// Adds the lines of `from` to the end of `into`, starting on a line of their own, unless `into`
/// already ends with them: a run that stopped after appending them and is being retried.
pub(super) fn append_lines(from: &Path, into: &Path) -> io::Result<()> {
    use std::io::{Read, Seek, SeekFrom, Write};
    let mut added = std::fs::read(from)?;
    if added.is_empty() {
        return Ok(());
    }
    if added.last() != Some(&b'\n') {
        added.push(b'\n');
    }
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .append(true)
        .open(into)?;
    let length = file.metadata()?.len();
    if length >= added.len() as u64 {
        let mut tail = vec![0u8; added.len()];
        file.seek(SeekFrom::Start(length - added.len() as u64))?;
        file.read_exact(&mut tail)?;
        if tail == added {
            return Ok(());
        }
    }
    if length > 0 {
        let mut last = [0u8; 1];
        file.seek(SeekFrom::Start(length - 1))?;
        file.read_exact(&mut last)?;
        if last[0] != b'\n' {
            added.insert(0, b'\n');
        }
    }
    file.write_all(&added)
}

/// SQLite keeps a database's write-ahead log and its index beside it, named after it; the log
/// can hold committed data, so the three move together.
const DATABASE_COMPANIONS: [&str; 2] = ["-wal", "-shm"];

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut text = path.as_os_str().to_owned();
    text.push(suffix);
    PathBuf::from(text)
}

/// Moves a SQLite database with its write-ahead log and index.
pub(super) fn move_database(from: &Path, to: &Path) -> io::Result<()> {
    move_path(from, to)?;
    for suffix in DATABASE_COMPANIONS {
        let companion = with_suffix(from, suffix);
        if lstat(&companion)?.is_some() {
            move_path(&companion, &with_suffix(to, suffix))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a move across volumes copies: a file with its modification time, which tells the
    /// later of two copies of a record; never a relative link, which would lead elsewhere.
    #[test]
    fn a_copy_keeps_modification_times_and_refuses_relative_links() {
        let dir = crate::test_support::ScratchDir::new("shared-config-copy");
        let from = dir.join("from");
        std::fs::create_dir_all(&from).unwrap();
        std::fs::write(from.join("record.jsonl"), "{}\n").unwrap();
        crate::test_support::move_modification_time_back(&from.join("record.jsonl"));
        copy_any(&from, &dir.join("to")).unwrap();
        assert_eq!(
            modified(&dir.join("to/record.jsonl")),
            modified(&from.join("record.jsonl"))
        );

        link(&from.join("relative"), Path::new("record.jsonl")).unwrap();
        assert!(copy_any(&from, &dir.join("again")).is_err());
    }
}
