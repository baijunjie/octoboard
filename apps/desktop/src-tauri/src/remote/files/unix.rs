use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;

/// Refuses `root` and its parent unless they are private directories owned by the user who
/// created `ours`. The directory above them (the application's local data directory) may be open
/// to others, so another user could have made either directory first.
pub(in super::super) fn check_private(root: &Path, ours: &Path) -> std::io::Result<()> {
    let me = fs::symlink_metadata(ours)?.uid();
    for folder in [root.parent(), Some(root)].into_iter().flatten() {
        let metadata = fs::symlink_metadata(folder)?;
        if !metadata.is_dir() || metadata.uid() != me || metadata.permissions().mode() & 0o077 != 0
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("{} is not a private folder of this user", folder.display()),
            ));
        }
    }
    Ok(())
}

/// Makes a directory accessible only to its owner, including any missing parents if recursive.
pub(in super::super) fn create_private_dir(path: &Path, recursive: bool) -> std::io::Result<()> {
    fs::DirBuilder::new()
        .recursive(recursive)
        .mode(0o700)
        .create(path)
}

pub(in super::super) fn private_file_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.write(true).mode(0o600);
    options
}

/// Opens `path` for reading without waiting for anything and without following a link there, and
/// answers the file only where what was opened is a plain file. A pipe opened the usual way waits
/// for a writer; looking at the path first and opening it after would leave a moment for it to be
/// replaced, so it is the opened file that is looked at.
pub(in super::super) fn open_plain_file(path: &Path) -> std::io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(path)?;
    if file.metadata()?.is_file() {
        Ok(file)
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{} is not a plain file", path.display()),
        ))
    }
}
