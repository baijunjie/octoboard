use std::path::Path;

#[cfg(unix)]
#[path = "unix.rs"]
mod unix;
#[cfg(unix)]
pub(super) use unix::{check_private, create_private_dir, open_plain_file, private_file_options};

#[cfg(not(unix))]
pub(super) use private::{
    check_private, create_private_dir, open_plain_file, private_file_options,
};

#[cfg(not(unix))]
mod private {
    use std::fs::{self, File, OpenOptions};
    use std::path::Path;

    // The local data directory is inside this user's profile, which is the user's alone.
    pub(in super::super) fn check_private(_root: &Path, _ours: &Path) -> std::io::Result<()> {
        Ok(())
    }

    pub(in super::super) fn create_private_dir(
        path: &Path,
        recursive: bool,
    ) -> std::io::Result<()> {
        fs::DirBuilder::new().recursive(recursive).create(path)
    }

    pub(in super::super) fn private_file_options() -> OpenOptions {
        let mut options = OpenOptions::new();
        options.write(true);
        options
    }

    /// Opens `path` for reading and answers the file only where what was opened is a plain file.
    /// Opening waits for nothing here: the temporary directory is the user's own, and a file
    /// system pipe is not something a path in it names.
    pub(in super::super) fn open_plain_file(path: &Path) -> std::io::Result<File> {
        let file = File::open(path)?;
        if file.metadata()?.is_file() {
            Ok(file)
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("{} is not a plain file", path.display()),
            ))
        }
    }
}

#[cfg(windows)]
pub(super) fn mark_downloaded(path: &Path) -> std::io::Result<()> {
    let mut stream = path.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    std::fs::write(stream, "[ZoneTransfer]\r\nZoneId=3\r\n")
}

/// Nothing marks a download elsewhere: a development build on Linux has no such mechanism.
#[cfg(not(windows))]
pub(super) fn mark_downloaded(_path: &Path) -> std::io::Result<()> {
    Ok(())
}
