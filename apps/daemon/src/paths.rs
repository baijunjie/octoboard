//! Where Octoboard keeps its own files. Everything lives under `~/.octoboard`; nothing is ever
//! written inside a project or into the user's agent configuration.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::platform::fs as host_fs;

/// The home directory, absolute and written one way (see `hostfs::lexically_normalise`): `HOME`,
/// else the user's entry in the system user database when `HOME` is unset or relative, else `/`.
/// Every path built on it, Octoboard's own files and a clone into `~/Projects` among them, must
/// be absolute, and a relative `HOME` would place them under whatever directory the daemon
/// happens to run in.
pub fn home_dir() -> PathBuf {
    home_from(host_fs::env_home().as_deref(), host_fs::account_home)
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

/// The registry files of the holders that run terminals for the daemon; see `crate::holder`.
pub fn holders_dir() -> PathBuf {
    holders_dir_in(&data_dir())
}

/// The registry files' directory of the daemon whose data directory is `data`.
pub fn holders_dir_in(data: &Path) -> PathBuf {
    data.join("holders")
}

/// The copies of the daemon's builds that holders run from, one directory per build; see
/// `crate::holder::build`.
pub fn builds_dir() -> PathBuf {
    data_dir().join("builds")
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

/// Each ended session's last terminal output, one file per session; see
/// `crate::terminal::saved_output`. Unlike `run_dir`, it outlives the daemon: it is what an
/// archived session shows.
pub fn saved_output_dir() -> PathBuf {
    data_dir().join("output")
}

/// The handoff packages, one file per receiving session. They are kept outside `run_dir`, which
/// is cleared at startup, because a package is read by a session that may be resumed long after.
pub fn handoff_dir() -> PathBuf {
    data_dir().join("handoff")
}

/// The screens a planned stop saved for the next daemon, which that daemon loads as it starts and
/// rewrites as it takes each; see `crate::state::restart_screens`.
pub fn restart_screens_path() -> PathBuf {
    data_dir().join("restart-screens.json")
}

/// The background work of each running session, which every daemon keeps up to date as it changes
/// and the next one reads as it starts; see `crate::state::background_store`.
pub fn background_tasks_path() -> PathBuf {
    data_dir().join("background-tasks.json")
}

/// The messages a planned stop left queued for the next daemon, which that daemon queues again and
/// deletes as it starts; see `crate::messaging::restart_outbox`.
pub fn restart_outbox_path() -> PathBuf {
    data_dir().join("restart-outbox.json")
}

/// The record the desktop application creates when it quits (`{"version":1}`, or an empty file
/// from an older application): the holders it left are ended by the next daemon, which scopes the
/// record to those the registry lists and removes each target as it is gone (see
/// `crate::holder::quit_targets`). Created by the application (`apps/desktop/src-tauri`), which
/// has to place it at this same path.
pub fn application_quit_record() -> PathBuf {
    data_dir().join("application-quit")
}

/// The output of the last update run of each agent's CLI, one file per agent, overwritten by each
/// run; see `crate::agents::cli`. Kept outside `run_dir`, which is cleared at startup: an update
/// may still be running when the daemon is replaced, and its log is where to look afterwards.
pub fn agent_cli_updates_dir() -> PathBuf {
    data_dir().join("agent-cli-updates")
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
        let host = |path: &str| PathBuf::from(crate::test_support::host_path(path));
        let recorded = || Some(host("/Users/db/"));
        let none = || None;
        let home = |value: &str| home_from(Some(OsStr::new(&host(value))), recorded);
        assert_eq!(home("/Users/me"), host("/Users/me"));
        assert_eq!(home("/Users/me/./"), host("/Users/me"));
        assert_eq!(home("/Users/x/../me"), host("/Users/me"));
        let relative = |value: &str| home_from(Some(OsStr::new(value)), recorded);
        assert_eq!(relative("me"), host("/Users/db"));
        assert_eq!(relative(""), host("/Users/db"));
        assert_eq!(home_from(None, recorded), host("/Users/db"));
        assert_eq!(home_from(Some(OsStr::new("me")), none), PathBuf::from("/"));
        assert_eq!(
            home_from(None, || Some(PathBuf::from("db"))),
            PathBuf::from("/")
        );
    }
}
