//! The file a handoff package is written to: where it is for a key, and its removal. It depends on
//! nothing but `std`, so that what removes a session's record (`crate::state`) removes its package
//! without depending on what builds one.

use std::path::{Path, PathBuf};

/// Where the package written under `key` is in `dir`: a known path per key, so whoever needs it
/// later finds it without asking. `None` for a key that is not a plain id, which no package is ever
/// written under.
pub(crate) fn package_path_in(dir: &Path, key: &str) -> Option<PathBuf> {
    let plain = !key.is_empty()
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
    plain.then(|| dir.join(format!("{key}.md")))
}

/// Removes the package written under `key` in `dir`; one that is already gone is no failure.
pub(crate) fn remove_in(dir: &Path, key: &str) -> std::io::Result<()> {
    match package_path_in(dir, key).map(std::fs::remove_file) {
        Some(Err(err)) if err.kind() != std::io::ErrorKind::NotFound => Err(err),
        _ => Ok(()),
    }
}
