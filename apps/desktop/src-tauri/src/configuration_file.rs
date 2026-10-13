//! Saving and opening the configuration document as a file the user picks in the system's own
//! dialog. The webview cannot do either by itself (its downloads are not handled and its file input
//! cannot hand back where a file was saved), and the UI reaches the file system only through the
//! platform adapter, so the two commands are the whole of it: the picker is the system's, the file
//! is read or written here, and the UI is handed its name and its text. The shell is given no path
//! to read or write and takes no path in a file name, so the page can touch only what the user
//! chose in the dialog. Both dialogs filter to JSON, the document's format.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, FilePath};

/// The most an opened file may hold. A configuration document carries each console's avatar (at
/// most 256 KiB), so even a large one is far below this; the cap only keeps a wrong file, such as
/// a disk image, from being read into memory whole.
const MAX_OPEN_BYTES: u64 = 64 * 1024 * 1024;

/// What the dialog offers when the page's suggestion leaves nothing usable.
const FALLBACK_NAME: &str = "configuration.json";

/// Why a file could not be saved or opened. The first two are named for the UI to word in the
/// user's language; `Io` carries the operating system's own text, which cannot be localized.
#[derive(Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum FileError {
    TooLarge,
    NotText,
    Io(String),
}

impl FileError {
    fn io(err: impl ToString) -> Self {
        Self::Io(err.to_string())
    }
}

#[derive(Serialize)]
pub struct OpenedFile {
    /// The file's name, without its directory.
    name: String,
    text: String,
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The page's suggested file name with anything that is not a name taken out: a directory part,
/// separators and control characters. A name that leaves nothing becomes `FALLBACK_NAME`.
fn suggested_file_name(suggested: &str) -> String {
    let last = suggested.rsplit(['/', '\\']).next().unwrap_or_default();
    let name: String = last
        .chars()
        .filter(|c| !c.is_control() && *c != ':')
        .collect();
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." {
        FALLBACK_NAME.to_string()
    } else {
        name.to_string()
    }
}

fn picked_path(picked: FilePath) -> Result<PathBuf, FileError> {
    picked.into_path().map_err(FileError::io)
}

/// Asks the user where to save `text`, offering `suggested_name`, and writes it there. Answers the
/// name of the file written, or nothing when the user dismissed the dialog.
#[tauri::command]
pub async fn save_configuration_file(
    app: AppHandle,
    suggested_name: String,
    text: String,
) -> Result<Option<String>, FileError> {
    // The dialog blocks until it is answered, which must not be on the thread that serves the
    // command.
    tauri::async_runtime::spawn_blocking(move || {
        let Some(picked) = app
            .dialog()
            .file()
            .set_file_name(suggested_file_name(&suggested_name))
            .add_filter("JSON", &["json"])
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = picked_path(picked)?;
        fs::write(&path, text).map_err(FileError::io)?;
        Ok(Some(file_name(&path)))
    })
    .await
    .map_err(FileError::io)?
}

/// Asks the user for a file and reads it as UTF-8 text. Answers nothing when the user dismissed the
/// dialog, and an error for a file that is too large or is not text.
#[tauri::command]
pub async fn open_configuration_file(app: AppHandle) -> Result<Option<OpenedFile>, FileError> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(picked) = app
            .dialog()
            .file()
            .add_filter("JSON", &["json"])
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = picked_path(picked)?;
        let file = fs::File::open(&path).map_err(FileError::io)?;
        // One byte past the cap tells a file over it from one exactly at it, whatever its metadata
        // says.
        let mut bytes = Vec::new();
        file.take(MAX_OPEN_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(FileError::io)?;
        if bytes.len() as u64 > MAX_OPEN_BYTES {
            return Err(FileError::TooLarge);
        }
        let text = String::from_utf8(bytes).map_err(|_| FileError::NotText)?;
        Ok(Some(OpenedFile {
            name: file_name(&path),
            text,
        }))
    })
    .await
    .map_err(FileError::io)?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_suggested_name_keeps_only_its_last_component_without_separators_or_control_characters() {
        for (suggested, expected) in [
            ("octoboard-Work.json", "octoboard-Work.json"),
            ("../../etc/passwd", "passwd"),
            ("C:\\Users\\me\\a.json", "a.json"),
            ("a\u{7}b.json", "ab.json"),
            ("a:b.json", "ab.json"),
            ("dir/", FALLBACK_NAME),
            ("..", FALLBACK_NAME),
            ("", FALLBACK_NAME),
        ] {
            assert_eq!(suggested_file_name(suggested), expected, "{suggested:?}");
        }
    }

    #[test]
    fn an_error_is_told_apart_by_its_kind() {
        let kind = |e: FileError| serde_json::to_value(e).unwrap();
        assert_eq!(
            kind(FileError::TooLarge),
            serde_json::json!({ "kind": "too_large" })
        );
        assert_eq!(
            kind(FileError::NotText),
            serde_json::json!({ "kind": "not_text" })
        );
        assert_eq!(
            kind(FileError::Io("denied".into())),
            serde_json::json!({ "kind": "io", "detail": "denied" })
        );
    }
}
