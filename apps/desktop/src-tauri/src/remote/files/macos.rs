use std::path::Path;

#[path = "unix.rs"]
mod unix;
pub(super) use unix::{check_private, create_private_dir, private_file_options};

/// Applies the quarantine attribute so macOS checks the downloaded copy when it is opened.
pub(super) fn mark_downloaded(path: &Path) -> std::io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    // The flags (a download the user has not yet approved), the time in hex, the downloading
    // application and no event id.
    let value = format!("0083;{seconds:x};Octoboard;");
    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    // SAFETY: the path and the name are NUL-terminated and the value is as long as it says, all
    // outliving the call.
    let status = unsafe {
        libc::setxattr(
            path.as_ptr(),
            c"com.apple.quarantine".as_ptr(),
            value.as_ptr().cast(),
            value.len(),
            0,
            0,
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}
