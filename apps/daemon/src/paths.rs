//! Where Octoboard keeps its own files. Everything lives under `~/.octoboard`; nothing is ever
//! written inside a project or into the user's agent configuration.

use std::ffi::{CStr, OsStr};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

/// The home directory, absolute and written one way (see `hostfs::lexically_normalise`): `HOME`,
/// else the user's entry in the system user database when `HOME` is unset or relative, else `/`.
/// Every path built on it, Octoboard's own files and a clone into `~/Projects` among them, must
/// be absolute, and a relative `HOME` would place them under whatever directory the daemon
/// happens to run in.
pub fn home_dir() -> PathBuf {
    home_from(std::env::var_os("HOME").as_deref(), user_database_home)
}

fn home_from(home: Option<&OsStr>, user_database: impl FnOnce() -> Option<PathBuf>) -> PathBuf {
    let absolute = |home: PathBuf| {
        let home = crate::hostfs::lexically_normalise(&home);
        home.is_absolute().then_some(home)
    };
    home.and_then(|home| absolute(PathBuf::from(home)))
        .or_else(|| user_database().and_then(absolute))
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// The home directory the system user database records for the effective user.
fn user_database_home() -> Option<PathBuf> {
    let mut buffer = vec![0 as libc::c_char; 4096];
    loop {
        // SAFETY: an all-zero `passwd` is a valid value, being only pointers and integers.
        // `entry`, `found` and `buffer` outlive the call, which is told `buffer.len()` as the size
        // it may write into. `pw_dir` is read only once the call succeeded and found an entry,
        // when it is null or a NUL-terminated string inside `buffer`, and it is copied out before
        // `buffer` is grown or dropped.
        let (status, dir) = unsafe {
            let mut entry: libc::passwd = std::mem::zeroed();
            let mut found: *mut libc::passwd = std::ptr::null_mut();
            let status = libc::getpwuid_r(
                libc::geteuid(),
                &mut entry,
                buffer.as_mut_ptr(),
                buffer.len(),
                &mut found,
            );
            let dir = (status == 0 && !found.is_null() && !entry.pw_dir.is_null())
                .then(|| PathBuf::from(OsStr::from_bytes(CStr::from_ptr(entry.pw_dir).to_bytes())));
            (status, dir)
        };
        if status == libc::ERANGE && buffer.len() < 1 << 20 {
            buffer.resize(buffer.len() * 2, 0);
            continue;
        }
        return dir;
    }
}

/// The home directory when it is really known: [`home_dir`] is not the filesystem root.
/// `home_dir` falls back to `/` when it cannot tell, which is right for placing Octoboard's own
/// files but would make a client shorten every path to `~/...`, so what is reported to a client
/// goes through this instead.
pub fn known_home_dir() -> Option<PathBuf> {
    known_home(&home_dir())
}

/// `home` written one way (see `hostfs::lexically_normalise`), or `None` when it says nothing
/// usable about where home is: a relative path, or the filesystem root, under which every path
/// would count as being inside home.
pub fn known_home(home: &Path) -> Option<PathBuf> {
    let home = crate::hostfs::lexically_normalise(home);
    (home.is_absolute() && home.parent().is_some()).then_some(home)
}

pub fn data_dir() -> PathBuf {
    home_dir().join(".octoboard")
}

/// The log files, `daemon.log` (the daemon's, see `crate::log_file`) and `desktop.log` (the
/// desktop application's own, written by `apps/desktop/src-tauri/src/diagnostics.rs`, which has to
/// place it at this same path).
pub fn logs_dir() -> PathBuf {
    data_dir().join("logs")
}

pub fn db_path() -> PathBuf {
    data_dir().join("octoboard.db")
}

/// Parent of the consoles' working directories, one per console.
pub fn consoles_dir() -> PathBuf {
    data_dir().join("consoles")
}

/// Parent of the per-session scratch directories. Cleared at startup: every directory under it
/// belongs to a session of a daemon that is no longer running.
pub fn run_dir() -> PathBuf {
    data_dir().join("run")
}

pub fn session_scratch(session_id: &str) -> PathBuf {
    run_dir().join(session_id)
}

/// Each ended session's last terminal output, one file per session; see `crate::saved_output`.
/// Unlike `run_dir`, it outlives the daemon: it is what an archived session shows.
pub fn saved_output_dir() -> PathBuf {
    data_dir().join("output")
}

/// The handoff packages, one file per receiving session. They are kept outside `run_dir`, which
/// is cleared at startup, because a package is read by a session that may be resumed long after.
pub fn handoff_dir() -> PathBuf {
    data_dir().join("handoff")
}

#[cfg(test)]
mod tests {
    use super::home_from;
    use std::ffi::OsStr;
    use std::path::PathBuf;

    /// `HOME` wins when it is absolute; otherwise the user database's entry, else `/`, each
    /// written one way.
    #[test]
    fn home_is_absolute_and_written_one_way() {
        let recorded = || Some(PathBuf::from("/Users/db/"));
        let none = || None;
        let home = |value: &str| home_from(Some(OsStr::new(value)), recorded);
        assert_eq!(home("/Users/me"), PathBuf::from("/Users/me"));
        assert_eq!(home("/Users/me/./"), PathBuf::from("/Users/me"));
        assert_eq!(home("/Users/x/../me"), PathBuf::from("/Users/me"));
        assert_eq!(home("me"), PathBuf::from("/Users/db"));
        assert_eq!(home(""), PathBuf::from("/Users/db"));
        assert_eq!(home_from(None, recorded), PathBuf::from("/Users/db"));
        assert_eq!(home_from(Some(OsStr::new("me")), none), PathBuf::from("/"));
        assert_eq!(
            home_from(None, || Some(PathBuf::from("db"))),
            PathBuf::from("/")
        );
    }

    /// The effective user has an entry with an absolute home on any machine the tests run on.
    #[test]
    fn the_user_database_records_an_absolute_home() {
        assert!(super::user_database_home().is_some_and(|home| home.is_absolute()));
    }
}
