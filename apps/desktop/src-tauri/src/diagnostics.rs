//! The shell's own diagnostics: a line goes to stderr, which is what `pnpm tauri dev` shows, and is
//! appended with a timestamp to `desktop.log` under `~/.octoboard/logs/`. A packaged application
//! launched from the Finder has its stderr on `/dev/null`, so the file is the only record there.
//!
//! Writing a diagnostic never fails and never blocks on anything but the file's own I/O: every
//! error here is swallowed, since there is nowhere left to report it.

use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Once;
use std::time::{SystemTime, UNIX_EPOCH};

const FILE_NAME: &str = "desktop.log";

/// How large `desktop.log` may be when it is opened for the first time in a run before it is moved
/// aside to `desktop.log.1`, replacing the one already there.
const ROTATE_AT: u64 = 1024 * 1024;

/// Writes `octoboard: <message>` to stderr and the log file; see the module comment.
macro_rules! diagnostic {
    ($($arg:tt)*) => {
        $crate::diagnostics::record(&format!($($arg)*))
    };
}
pub(crate) use diagnostic;

pub fn record(message: &str) {
    eprintln!("octoboard: {message}");
    let Some(dir) = logs_dir() else {
        return;
    };
    let path = dir.join(FILE_NAME);
    static ROTATED: Once = Once::new();
    ROTATED.call_once(|| {
        let _ = fs::create_dir_all(&dir);
        rotate_if_large(&path, ROTATE_AT);
    });
    let line = format!("{} {message}\n", timestamp(SystemTime::now()));
    let _ = fs::create_dir_all(&dir)
        .and_then(|()| OpenOptions::new().create(true).append(true).open(&path))
        .and_then(|mut file| file.write_all(line.as_bytes()));
}

#[cfg(target_os = "macos")]
mod macos {
    /// The variable the home directory is read from, as the daemon reads it
    /// (`platform::fs::env_home` in `apps/daemon/`).
    pub const HOME_VARIABLE: &str = "HOME";
}
#[cfg(target_os = "macos")]
use macos as platform;

/// Every target but macOS, Linux included.
#[cfg(not(target_os = "macos"))]
mod windows {
    /// The variable the home directory is read from, as the daemon reads it
    /// (`platform::fs::env_home` in `apps/daemon/`): `USERPROFILE` on Windows.
    #[cfg(windows)]
    pub const HOME_VARIABLE: &str = "USERPROFILE";

    /// `HOME` on Linux, as on macOS.
    #[cfg(not(windows))]
    pub const HOME_VARIABLE: &str = "HOME";
}
#[cfg(not(target_os = "macos"))]
use windows as platform;

use platform::HOME_VARIABLE;

/// `~/.octoboard`, which has to be the same directory as `paths::data_dir` in
/// `apps/daemon/src/paths.rs`, with the home directory resolved the way `paths::home_dir` does:
/// the home variable (`HOME_VARIABLE`) when it is absolute, else the system's record of the user's
/// home when it is unset. A relative one leaves it unresolved (`std::env::home_dir` returns a
/// set `HOME` as it is on Unix, so the system's record is not consulted then), so nothing is
/// written into whatever directory the application happens to run in.
pub fn data_dir() -> Option<PathBuf> {
    home_dir(
        std::env::var_os(HOME_VARIABLE).as_deref(),
        std::env::home_dir,
    )
    .map(|home| home.join(".octoboard"))
}

/// `logs` under the data directory, as `paths::logs_dir` in the daemon.
fn logs_dir() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join("logs"))
}

fn home_dir(
    home: Option<&OsStr>,
    system_home: impl FnOnce() -> Option<PathBuf>,
) -> Option<PathBuf> {
    home.map(PathBuf::from)
        .filter(|home| home.is_absolute())
        .or_else(|| system_home().filter(|home| home.is_absolute()))
}

/// Moves `path` to the same name with `.1` added, replacing that one, when it is larger than
/// `limit`.
fn rotate_if_large(path: &Path, limit: u64) {
    if fs::metadata(path).is_ok_and(|meta| meta.len() > limit) {
        let mut rotated = path.as_os_str().to_owned();
        rotated.push(".1");
        let _ = fs::rename(path, rotated);
    }
}

/// `time` in UTC as `2026-10-30T12:34:56Z`.
fn timestamp(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    // Days since 1970-01-01 to a civil date, Howard Hinnant's `civil_from_days`.
    let shifted = days as i64 + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn the_timestamp_is_utc() {
        let at = |seconds| timestamp(UNIX_EPOCH + Duration::from_secs(seconds));
        assert_eq!(at(0), "1970-01-01T00:00:00Z");
        // A leap day, and a time of day.
        assert_eq!(at(1_709_210_096), "2024-02-29T12:34:56Z");
        assert_eq!(at(1_793_363_696), "2026-10-30T12:34:56Z");
    }

    #[test]
    fn the_home_directory_is_absolute() {
        // An absolute path on the platform the test runs on: Windows' needs a drive.
        let (me, db) = if cfg!(windows) {
            (r"C:\Users\me", r"C:\Users\db")
        } else {
            ("/Users/me", "/Users/db")
        };
        let system = || Some(PathBuf::from(db));
        let home = |value: &str| home_dir(Some(OsStr::new(value)), system);
        assert_eq!(home(me), Some(PathBuf::from(me)));
        // Unset: the system's record.
        assert_eq!(home_dir(None, system), Some(PathBuf::from(db)));
        // A path rooted without a drive is not absolute on Windows, and is passed over there.
        if cfg!(windows) {
            assert_eq!(
                home_dir(Some(OsStr::new("/Users/me")), system),
                Some(PathBuf::from(db))
            );
        }
        assert_eq!(home_dir(None, || Some(PathBuf::from("db"))), None);
        // Set but relative: what `std::env::home_dir` returns is that same relative path.
        assert_eq!(
            home_dir(Some(OsStr::new("me")), || Some(PathBuf::from("me"))),
            None
        );
    }

    #[test]
    fn the_home_variable_is_the_one_the_daemon_reads() {
        assert_eq!(
            HOME_VARIABLE,
            if cfg!(target_os = "windows") {
                "USERPROFILE"
            } else {
                "HOME"
            }
        );
    }

    #[test]
    fn a_large_file_is_moved_aside_over_the_previous_one() {
        let dir =
            std::env::temp_dir().join(format!("octoboard-diagnostics-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);
        let rotated = dir.join("desktop.log.1");
        fs::write(&rotated, "older").unwrap();

        fs::write(&path, "12345").unwrap();
        rotate_if_large(&path, 5);
        assert_eq!(fs::read_to_string(&path).unwrap(), "12345");
        assert_eq!(fs::read_to_string(&rotated).unwrap(), "older");

        fs::write(&path, "123456").unwrap();
        rotate_if_large(&path, 5);
        assert!(!path.exists());
        assert_eq!(fs::read_to_string(&rotated).unwrap(), "123456");
        fs::remove_dir_all(&dir).unwrap();
    }
}
