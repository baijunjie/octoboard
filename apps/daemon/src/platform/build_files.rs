//! The files a copy of the daemon's build holds beside its executable, which differ by host.

/// The files, relative to the executable's directory, that the executable needs beside it where
/// they are shipped: on Windows the bundled `conpty.dll` and the `OpenConsole.exe` it starts
/// (which it looks for in a subdirectory named after the machine's architecture). A file that is
/// not there is not part of the build. None where the executable stands alone.
pub fn companions() -> &'static [&'static str] {
    if cfg!(windows) {
        &["conpty.dll", "x64/OpenConsole.exe", "arm64/OpenConsole.exe"]
    } else {
        &[]
    }
}
