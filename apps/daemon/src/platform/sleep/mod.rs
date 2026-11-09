//! Holding the machine awake: a [`Hold`] keeps the system from idle sleep, and the display from
//! idle sleep where the host ties the two together, until it is dropped. Whether to hold is
//! `keep_awake`'s to decide; what a hold takes on each host is here, and the system releases it
//! with the process, so a crash needs no cleanup.
//!
//! macOS takes two IOKit power assertions, Windows a power request and Linux a logind inhibitor
//! lock, with the session bus's screen saver inhibition added where the desktop offers one. A host
//! where [`available`] is false holds nothing: [`acquire`] is refused there.

use anyhow::Result;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as host;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as host;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as host;

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod unsupported;
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
use unsupported as host;

/// What the daemon says the hold is for, which each host lists beside Octoboard's name (`pmset -g
/// assertions`, `powercfg /requests`, `systemd-inhibit --list`).
const REASON: &str = "Octoboard is keeping the computer awake";

/// A hold on the machine's idle sleep, released when it is dropped.
pub(crate) struct Hold {
    _held: host::Held,
}

/// Starts a hold. Blocks for as long as the host's calls take, which on Linux is a call to the
/// system bus and then one to the session bus, each bounded by its own timeout, so async code
/// calls it from a blocking thread.
pub(crate) fn acquire() -> Result<Hold> {
    Ok(Hold {
        _held: host::Held::acquire(REASON)?,
    })
}

/// Whether this host can hold at all: always on macOS and Windows, and on Linux only when logind
/// answers on the system bus. Probed on the first call and kept, so the first call may block; the
/// daemon makes it once before it serves (`main`), so a later call only reads the answer.
pub(crate) fn available() -> bool {
    static AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *AVAILABLE.get_or_init(host::available)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_that_is_not_available_refuses_to_hold() {
        if !available() {
            assert!(acquire().is_err());
        }
    }

    #[test]
    fn macos_and_windows_are_always_available() {
        if cfg!(any(target_os = "macos", windows)) {
            assert!(available());
        }
    }
}
