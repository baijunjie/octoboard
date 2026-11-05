//! A second barrier under the report panel's sanitizer: the window's webview blocks `http(s)`
//! requests other than image loads. How it does is the platform's own. Where it has to be in place
//! before the window is built, `prepare` hands over what the window's builder takes (possibly only
//! once something has been compiled) and `apply` puts it on the builder; where it is installed on the
//! webview itself, `install` does that once the window is built. Each platform does one of the two and
//! leaves the other a no-op.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(not(target_os = "macos"))]
mod windows;
#[cfg(not(target_os = "macos"))]
use windows as platform;

// The decision the Windows filter makes, compiled elsewhere only for its tests.
#[cfg(any(windows, test))]
mod policy;

pub use platform::{apply, install, prepare, Configuration};
