//! Where Octoboard keeps its own files. Everything lives under `~/.octoboard`; nothing is ever
//! written inside a project or into the user's agent configuration.

use std::path::{Path, PathBuf};

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// The home directory when it is really known: `HOME` is set, absolute and not the filesystem
/// root. `home_dir` falls back to `/` when it cannot tell, which is right for placing Octoboard's
/// own files but would make a client shorten every path to `~/...`, so what is reported to a
/// client goes through this instead.
pub fn known_home_dir() -> Option<PathBuf> {
    known_home(Path::new(&std::env::var_os("HOME")?))
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

pub fn db_path() -> PathBuf {
    data_dir().join("octoboard.db")
}

/// Working directory of a console session.
pub fn console_workdir(console_id: &str) -> PathBuf {
    data_dir().join("consoles").join(console_id)
}

/// Parent of the per-session scratch directories. Cleared at startup: every directory under it
/// belongs to a session of a daemon that is no longer running.
pub fn run_dir() -> PathBuf {
    data_dir().join("run")
}

pub fn session_scratch(session_id: &str) -> PathBuf {
    run_dir().join(session_id)
}
