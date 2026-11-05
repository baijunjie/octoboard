//! What the daemon asks of the user's shell and of how programs are started on this host: the
//! login shell and the environment it gives, finding a binary on that environment's `PATH`, the
//! command line a hook runs and the script that holds one, the shell that runs a user's own
//! command, and the interpreter of the git hooks. Which environment variables belong to an agent
//! session, how long a snapshot may be served, and what a hook registers are for the callers to
//! decide; this module only answers what the host does when asked.
//!
//! The Unix side runs the user's login shell to learn their environment, as a GUI application
//! does not inherit one; a Windows application does inherit the user's environment, so there is no
//! shell to run there.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub use unix::*;

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::*;

#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "exercised by tests off Windows")
)]
mod package_shims;

pub use package_shims::Shim;

/// An environment as a process is started with: names and values.
pub type Environment = HashMap<String, String>;

/// A program and the arguments that come before the caller's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellCommand {
    pub program: PathBuf,
    pub args: Vec<OsString>,
}

/// A binary found on a `PATH`, and what has to be started for it to run: the file itself, or an
/// interpreter and the file as its first argument where the host cannot run the file directly. The
/// agent's own arguments go after `leading_args`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvedBinary {
    pub program: PathBuf,
    pub leading_args: Vec<OsString>,
}

impl ResolvedBinary {
    /// A binary that is started as the file `program`.
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            leading_args: Vec::new(),
        }
    }

    /// The command that starts the binary, with its leading arguments and none of the caller's.
    pub fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.leading_args);
        command
    }
}

/// Which shell language a command string is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// POSIX `sh`: what an agent's hook and a user's own command are written in wherever the host
    /// has one.
    Posix,
    /// PowerShell: what Codex runs a hook command with on Windows.
    #[cfg_attr(
        unix,
        allow(dead_code, reason = "only a Windows host has it as a hook runner")
    )]
    PowerShell,
}

/// One argument of a command line built by [`command_line`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arg<'a> {
    /// Written as it is: an option name, a subcommand, a number.
    Word(&'a str),
    /// Quoted so that the shell passes it on as one argument whatever it holds.
    Value(&'a str),
}

/// What a command line does with the standard error of the program it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stderr {
    Keep,
    Discard,
}

/// A snapshot that did not finish within its deadline.
#[derive(Debug)]
pub struct SnapshotTimeout {
    pub shell: String,
    pub command: String,
    pub timeout: Duration,
}

impl std::fmt::Display for SnapshotTimeout {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "`{} -l -i -c '{}'` did not finish within {:?}",
            self.shell, self.command, self.timeout
        )
    }
}

impl std::error::Error for SnapshotTimeout {}

/// The directories of a `PATH`-style `value`, in order, as the host separates them.
pub fn split_path(value: &OsStr) -> Vec<PathBuf> {
    std::env::split_paths(value).collect()
}

/// The command string that runs `exe` with `args` through a POSIX shell, replacing the shell, so
/// none lingers.
pub fn command_line(exe: &Path, args: &[Arg<'_>], stderr: Stderr) -> String {
    let mut line = format!("exec {}", posix_quote(&exe.to_string_lossy()));
    for arg in args {
        line.push(' ');
        match arg {
            Arg::Word(word) => line.push_str(word),
            Arg::Value(value) => line.push_str(&posix_quote(value)),
        }
    }
    if stderr == Stderr::Discard {
        line.push_str(" 2>/dev/null");
    }
    line
}

/// The command that runs the script at `script` (what [`write_runnable`] returned), written in
/// `dialect`: the path itself for a POSIX shell, which runs an executable file by its path, and a
/// call of the path for PowerShell, which would otherwise take a path with a space for words.
pub fn script_command(script: &Path, dialect: Dialect) -> String {
    let path = script.to_string_lossy();
    match dialect {
        Dialect::Posix => path.into_owned(),
        Dialect::PowerShell => format!("& '{}'", path.replace('\'', "''")),
    }
}

/// `value` quoted for a POSIX shell: one argument whatever it holds.
pub fn posix_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_line_quotes_values_and_leaves_words() {
        assert_eq!(
            command_line(
                Path::new("/opt/it's/bin"),
                &[Arg::Word("hook"), Arg::Value("a b"), Arg::Word("1")],
                Stderr::Discard
            ),
            "exec '/opt/it'\\''s/bin' hook 'a b' 1 2>/dev/null"
        );
        assert_eq!(
            command_line(Path::new("/x"), &[], Stderr::Keep),
            "exec '/x'"
        );
    }

    #[test]
    fn a_script_is_called_as_its_dialect_calls_a_path() {
        let script = Path::new("/a dir/it's/hook");
        assert_eq!(script_command(script, Dialect::Posix), "/a dir/it's/hook");
        assert_eq!(
            script_command(script, Dialect::PowerShell),
            "& '/a dir/it''s/hook'"
        );
    }
}
