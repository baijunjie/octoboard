//! The JSON files the shell keeps under the application's config directory (`window-state.json`,
//! `settings.json`): where they live and how they are written.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager};

/// Where the file called `name` lives.
pub fn config_path(app: &AppHandle, name: &str) -> tauri::Result<PathBuf> {
    Ok(app.path().app_config_dir()?.join(name))
}

/// Written to a sibling file, flushed to disk and renamed over the old one, so a write cut short
/// never leaves a half-written file behind. The sibling is named after this process, since more
/// than one build of the application (a development one, an installed one) can share the
/// directory.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut partial = path.as_os_str().to_owned();
    partial.push(format!(".{}.partial", std::process::id()));
    let partial = PathBuf::from(partial);
    let json = serde_json::to_vec_pretty(value)?;
    let written = write_synced(&partial, &json).and_then(|()| fs::rename(&partial, path));
    if written.is_err() {
        let _ = fs::remove_file(&partial);
    }
    Ok(written?)
}

fn write_synced(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
pub mod test_support {
    use super::*;

    /// A fresh directory of this test's own, removed on drop.
    pub struct Scratch(pub PathBuf);

    impl Scratch {
        pub fn new(module: &str, name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("octoboard-{module}-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            Self(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::Scratch;
    use super::*;

    #[test]
    fn failed_write_leaves_no_partial_file_behind() {
        let scratch = Scratch::new("config-dir", "failed-write");
        // A directory where the file should be: writing the sibling succeeds, renaming a file over
        // a directory fails.
        let path = scratch.0.join("file.json");
        fs::create_dir_all(&path).unwrap();
        assert!(write_json(&path, &[1, 2, 3]).is_err());
        let partials: Vec<_> = fs::read_dir(&scratch.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .filter(|name| name.ends_with(".partial"))
            .collect();
        assert!(partials.is_empty(), "left behind: {partials:?}");
    }
}
