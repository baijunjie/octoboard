use std::io;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

/// A directory only the user can enter, for a socket, removed with whatever was left in it.
pub(super) struct SocketDirectory {
    directory: PathBuf,
    socket: PathBuf,
}

impl SocketDirectory {
    pub(super) fn create(prefix: &str, name: &str) -> io::Result<Self> {
        // macOS limits Unix socket paths to 104 bytes, independently of TMPDIR's length.
        let directory =
            Path::new("/tmp").join(format!("{prefix}-{}", uuid::Uuid::new_v4().simple()));
        std::fs::DirBuilder::new().mode(0o700).create(&directory)?;
        let socket = directory.join(name);
        Ok(Self { directory, socket })
    }

    pub(super) fn socket(&self) -> &Path {
        &self.socket
    }

    #[cfg(test)]
    pub(super) fn path(&self) -> &Path {
        &self.directory
    }
}

impl Drop for SocketDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
