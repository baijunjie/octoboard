//! Where Octoboard keeps its own files. Everything lives under `~/.octoboard`; nothing is ever
//! written inside a project or into the user's agent configuration.

use std::path::PathBuf;

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

pub fn data_dir() -> PathBuf {
    home_dir().join(".octoboard")
}

pub fn db_path() -> PathBuf {
    data_dir().join("octoboard.db")
}

/// Working directory of a console's hub session.
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
