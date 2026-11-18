use std::fs::{self, OpenOptions};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;

/// Refuses `root` and its parent unless they are private directories owned by the user who
/// created `ours`. The temporary directory can be shared, so another user could have made
/// either directory first.
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
