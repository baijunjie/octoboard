use std::ffi::OsStr;
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use super::{
    command_line, posix_quote, split_path, Arg, Dialect, Environment, ResolvedBinary, ShellCommand,
    Shim, Stderr,
};

mod snapshot;

pub use snapshot::snapshot;

/// The user's login shell: `$SHELL`, else `zsh`, which is what macOS gives a new account.
pub fn login_shell() -> ShellCommand {
    ShellCommand {
        program: PathBuf::from(
            std::env::var_os("SHELL")
                .filter(|shell| !shell.is_empty())
                .unwrap_or_else(|| OsStr::new("/bin/zsh").to_owned()),
        ),
        args: vec!["-l".into()],
    }
}

/// Resolves a bare binary name to an absolute path by searching `env`'s `PATH`, mirroring what the
/// login shell would have found: the first entry that is an absolute directory (a relative one
/// stands for whatever directory the process happens to be in) holding an executable file of that
/// name. Deliberately does not leave resolution to `portable_pty`, whose
/// own search tries `cwd.join(exe)` first and accepts it on `.exists()` alone — so a project file
/// named `claude` would silently shadow the real CLI.
pub fn resolve_binary(name: &str, env: &Environment) -> Option<ResolvedBinary> {
    split_path(OsStr::new(env.get("PATH")?))
        .into_iter()
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(name))
        .find(|candidate| crate::platform::fs::is_executable(candidate))
        .map(ResolvedBinary::new)
}

/// The shell that runs a command string, the string to follow its arguments.
pub fn command_runner(_dialect: Dialect) -> ShellCommand {
    ShellCommand {
        program: PathBuf::from("/bin/sh"),
        args: vec!["-c".into()],
    }
}

/// The language of the shell that runs a hook command an agent registers by the path of a script
/// ([`script_command`](super::script_command)): `sh`.
pub fn hook_dialect() -> Dialect {
    Dialect::Posix
}

/// How long an agent lets a command hook run before it kills it, in seconds.
/// A hook's shell and the daemon start warm here and report within a fraction of a second, so the
/// timeout only has to catch a daemon that is unreachable, which costs a turn at most this long
/// where the hook is synchronous (Codex's and Claude's command hooks are registered `async`, so
/// the timeout bounds the hook process alone and never the agent's turn).
pub const COMMAND_HOOK_TIMEOUT_SECS: u32 = 3;

/// How many times as long as a later one an agent's first hook of a launch may take to report: no
/// longer here, where a hook's shell and the daemon start warm.
pub const FIRST_HOOK_SLOWDOWN: u32 = 1;

/// Writes at `path` a script that runs [`command_line`], executable by its owner alone from the
/// first byte, and returns the path to register as the command.
pub fn write_runnable(
    path: &Path,
    exe: &Path,
    args: &[Arg<'_>],
    stderr: Stderr,
) -> io::Result<PathBuf> {
    write_script(path, &command_line(exe, args, stderr))
}

/// Writes at `path` an executable that appends the first argument it is given, as a line, to
/// `log`, and returns its path, which is `path` itself here: what a program that opens a URL by running `$BROWSER <url>` is
/// pointed at.
pub fn write_argument_recorder(path: &Path, log: &Path) -> io::Result<PathBuf> {
    write_script(
        path,
        &format!(
            "printf '%s\\n' \"$1\" >> {}",
            posix_quote(&log.to_string_lossy())
        ),
    )
}

/// What `binary --version` prints, for a binary whose version its install does not tell. Here
/// nothing is run for it: a process start is what the callers are trying to avoid, and an install
/// here tells.
pub fn probe_version(_binary: &ResolvedBinary, _environment: &Environment) -> Option<String> {
    None
}

/// The architectures (as Rust names them) of the programs this machine runs: the daemon's own.
pub fn runnable_architectures() -> Vec<&'static str> {
    vec![std::env::consts::ARCH]
}

/// What `binary` is to the package `package`: never a shim here, since the host runs the
/// package's launcher script as it is.
pub fn shim_package(_binary: &ResolvedBinary, _package: &str) -> Shim {
    Shim::None
}

/// The interpreter of the git hooks' POSIX dispatcher, which every hook git runs starts, several
/// per commit: `dash` where the system has it, which starts in well under half the time of the
/// `bash` behind macOS's `/bin/sh`.
pub fn posix_interpreter() -> Option<PathBuf> {
    let dash = Path::new("/bin/dash");
    Some(if crate::platform::fs::is_executable(dash) {
        dash.to_path_buf()
    } else {
        PathBuf::from("/bin/sh")
    })
}

/// `path` as the shell of the git hooks writes and compares a path.
pub fn posix_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// A POSIX script of the one command `body`, owner-only and executable from the first byte: it may
/// carry a key.
fn write_script(path: &Path, body: &str) -> io::Result<PathBuf> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o700)
        .open(path)?;
    writeln!(file, "#!/bin/sh")?;
    writeln!(file, "{body}")?;
    file.flush()?;
    // A file that was there already keeps the mode it had, which the open above does not set.
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::test_support::ScratchDir;

    #[test]
    fn a_relative_path_entry_is_skipped() {
        let dir = ScratchDir::new("resolve-binary");
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let tool = bin.join("tool");
        std::fs::write(&tool, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();

        // A relative entry that does reach the fixture from the daemon's cwd: enough `..` to get to
        // `/`, then the bin directory's path without its leading `/`.
        let cwd = std::env::current_dir().unwrap();
        let mut relative_bin = PathBuf::new();
        for _ in cwd.components().skip(1) {
            relative_bin.push("..");
        }
        relative_bin.push(bin.strip_prefix("/").unwrap());
        assert!(relative_bin.is_relative() && relative_bin.join("tool").is_file());

        let relative = HashMap::from([("PATH".to_string(), relative_bin.display().to_string())]);
        assert_eq!(
            resolve_binary("tool", &relative),
            None,
            "only absolute entries count"
        );

        let absolute = HashMap::from([("PATH".to_string(), format!("bin:{}", bin.display()))]);
        assert_eq!(
            resolve_binary("tool", &absolute),
            Some(ResolvedBinary::new(&tool))
        );
        assert_eq!(resolve_binary("tool", &HashMap::new()), None);
    }

    #[test]
    fn a_runnable_is_owner_only_and_runs_its_command_line() {
        let dir = ScratchDir::new("write-runnable");
        let path = dir.join("run");
        // Written over a file that was there with looser permissions.
        std::fs::write(&path, "old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let written = write_runnable(
            &path,
            Path::new("/bin/echo"),
            &[Arg::Value("hi")],
            Stderr::Keep,
        )
        .unwrap();
        assert_eq!(written, path);
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "#!/bin/sh\nexec '/bin/echo' 'hi'\n"
        );
        let output = std::process::Command::new(&path).output().unwrap();
        assert_eq!(output.stdout, b"hi\n");
    }

    #[test]
    fn the_argument_recorder_appends_its_first_argument() {
        let dir = ScratchDir::new("argument-recorder");
        let (script, log) = (dir.join("record"), dir.join("log it"));
        write_argument_recorder(&script, &log).unwrap();
        for url in ["https://a/?x=1&y=2", "https://b/"] {
            assert!(std::process::Command::new(&script)
                .arg(url)
                .status()
                .unwrap()
                .success());
        }
        assert_eq!(
            std::fs::read_to_string(&log).unwrap(),
            "https://a/?x=1&y=2\nhttps://b/\n"
        );
    }
}
