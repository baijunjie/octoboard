//! Choosing a local directory in the system's dialog. The suggested path only sets where the
//! dialog opens; the returned path is always one the user chose.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

use crate::configuration_file::FileError;

fn starting_directory(initial: &str, home: &Path) -> io::Result<PathBuf> {
    let initial = initial.trim();
    let from_home = initial == "~" || initial.starts_with("~/");
    let mut path = if initial == "~" {
        home.to_path_buf()
    } else if let Some(rest) = initial.strip_prefix("~/") {
        home.join(rest)
    } else if Path::new(initial).is_absolute() {
        PathBuf::from(initial)
    } else {
        home.to_path_buf()
    };
    loop {
        // Only a missing entry walks upwards. A directory the shell cannot inspect is still a
        // useful hint to the system dialog, which can grant access as the user chooses a place.
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                let metadata = if metadata.is_symlink() {
                    match fs::metadata(&path) {
                        Ok(metadata) => metadata,
                        Err(error) if error.kind() == io::ErrorKind::NotFound => return Err(error),
                        Err(_) => return Ok(path),
                    }
                } else {
                    metadata
                };
                if !metadata.is_dir() {
                    return Err(io::Error::new(
                        io::ErrorKind::NotADirectory,
                        "The path is not a directory",
                    ));
                }
                return Ok(path);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if (from_home && path == home) || !path.pop() {
                    return Err(error);
                }
            }
            Err(_) => return Ok(path),
        }
    }
}

#[tauri::command]
pub async fn pick_directory(
    app: AppHandle,
    window: tauri::Window,
    title: String,
    initial_path: String,
) -> Result<Option<String>, FileError> {
    tauri::async_runtime::spawn_blocking(move || {
        let home = app.path().home_dir().map_err(FileError::io)?;
        let initial = starting_directory(&initial_path, &home).map_err(FileError::io)?;
        let Some(picked) = app
            .dialog()
            .file()
            .set_parent(&window)
            .set_title(title)
            .set_directory(initial)
            .blocking_pick_folder()
        else {
            return Ok(None);
        };
        let path = picked.into_path().map_err(FileError::io)?;
        Ok(Some(path.to_string_lossy().into_owned()))
    })
    .await
    .map_err(FileError::io)?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_home_and_missing_descendants_and_refuses_files() {
        let root = std::env::temp_dir().join(format!("octoboard-picker-{}", std::process::id()));
        fs::create_dir_all(root.join("home/code")).unwrap();
        let home = root.join("home");
        for (initial, expected) in [
            ("~".to_string(), home.clone()),
            ("".to_string(), home.clone()),
            ("relative".to_string(), home.clone()),
            (" ~/code/missing/child ".to_string(), home.join("code")),
            (
                root.join("missing/child").to_string_lossy().into_owned(),
                root.clone(),
            ),
        ] {
            assert_eq!(starting_directory(&initial, &home).unwrap(), expected);
        }
        fs::write(home.join("file"), "text").unwrap();
        assert_eq!(
            starting_directory("~/file", &home).unwrap_err().kind(),
            io::ErrorKind::NotADirectory
        );
        assert_eq!(
            starting_directory("~", &root.join("absent"))
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(home.join("absent"), home.join("dangling")).unwrap();
            assert_eq!(
                starting_directory("~/dangling", &home).unwrap_err().kind(),
                io::ErrorKind::NotFound
            );
        }
        fs::remove_dir_all(root).unwrap();
    }
}
