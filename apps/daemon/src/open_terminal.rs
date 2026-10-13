//! Opening the system terminal in a project's directory, on the machine the daemon runs on.
//!
//! The desktop window is only a client, and a browser client may be another machine, so the
//! terminal is started here rather than in the window. On macOS that program is Terminal.app:
//! `open -a Terminal` hands it the directory and returns once Terminal has accepted it, which
//! starts a shell there. On Linux there is no single system terminal, so the first program in a
//! fixed list that can be given a directory is used. A terminal emulator stays running for the
//! life of its window, so it is started in its own session and reaped in the background: waiting
//! for it would hold the request open until the user closed the window, and leaving it in the
//! daemon's process group would close it when the daemon exits.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::protocol::{error_code, CodedError};

/// How long `open` may take to hand a directory to Terminal.app. It returns as soon as Terminal
/// accepts the open, so this only bounds a wedged `open`, not the window the user then has.
const OPEN_TIMEOUT: Duration = Duration::from_secs(15);

/// What to run, and whether to wait for it. `wait` is only for a launcher that exits once the
/// terminal has accepted the directory (`open`). A terminal emulator itself has `wait` false.
#[derive(Debug, PartialEq, Eq)]
struct Launch {
    program: String,
    args: Vec<String>,
    /// The process's working directory, for a program that starts its shell there and has no
    /// directory argument of its own.
    directory: Option<PathBuf>,
    wait: bool,
}

/// A Linux terminal that takes the directory as an argument. Listed before the programs that only
/// inherit a working directory, because several common emulators start in the home directory
/// whatever they were launched from.
struct FlaggedTerminal {
    name: &'static str,
    /// Arguments before the directory, already a complete flag (`--working-directory`, `--cwd`).
    before: &'static [&'static str],
}

const FLAGGED: &[FlaggedTerminal] = &[
    FlaggedTerminal {
        name: "gnome-terminal",
        before: &["--working-directory"],
    },
    FlaggedTerminal {
        name: "konsole",
        before: &["--workdir"],
    },
    FlaggedTerminal {
        name: "xfce4-terminal",
        before: &["--working-directory"],
    },
    FlaggedTerminal {
        name: "mate-terminal",
        before: &["--working-directory"],
    },
    FlaggedTerminal {
        name: "kitty",
        before: &["--directory"],
    },
    FlaggedTerminal {
        name: "alacritty",
        before: &["--working-directory"],
    },
    FlaggedTerminal {
        name: "wezterm",
        before: &["start", "--cwd"],
    },
    FlaggedTerminal {
        name: "foot",
        before: &["--working-directory"],
    },
];

/// Programs that start a shell in their own working directory, used only when nothing in
/// [`FLAGGED`] is installed. `x-terminal-emulator` is the Debian alternatives entry for the
/// system terminal.
const INHERITING: &[&str] = &["x-terminal-emulator", "xterm"];

/// Opens the system terminal at `directory`. `directory` is a project's stored path.
pub fn open_directory(directory: &Path) -> Result<(), anyhow::Error> {
    if !directory.is_dir() {
        return Err(unavailable("the project directory does not exist"));
    }
    // The Linux list is resolved against one login-shell snapshot. A snapshot that did not run
    // is that failure: treating it as "not installed" would report a missing emulator when the
    // lookup never happened. macOS does not consult the snapshot; Terminal.app is `/usr/bin/open`.
    let launch = if std::env::consts::OS == "linux" {
        let snapshot = crate::env_shell::cached_snapshot().map_err(|err| format!("{err:#}"));
        linux_launch(directory, snapshot, |name, env| {
            crate::env_shell::resolve_binary(name, env).ok()
        })
    } else {
        launch_for(std::env::consts::OS, directory, |_| None)
    }
    .map_err(|detail| unavailable(&detail))?;
    start(&launch).map_err(|detail| unavailable(&detail))
}

fn unavailable(detail: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::TERMINAL_UNAVAILABLE,
        format!("The system terminal could not be opened: {detail}"),
        &[("detail", detail)],
    )
}

/// The launch for `os`. `resolve` maps a program name to its absolute path, for the Linux list;
/// macOS does not use it, because Terminal.app is reached through `/usr/bin/open`. A pure function
/// over those two, so the choice is unit-tested without starting a terminal.
fn launch_for(
    os: &str,
    directory: &Path,
    resolve: impl Fn(&str) -> Option<String>,
) -> Result<Launch, String> {
    let path = directory
        .to_str()
        .ok_or("the project path is not valid Unicode")?
        .to_string();
    if os == "macos" {
        return Ok(Launch {
            program: "/usr/bin/open".to_string(),
            args: vec![
                "-a".to_string(),
                "Terminal".to_string(),
                "--".to_string(),
                path,
            ],
            directory: None,
            wait: true,
        });
    }
    if os == "linux" {
        for candidate in FLAGGED {
            if let Some(program) = resolve(candidate.name) {
                let mut args = candidate
                    .before
                    .iter()
                    .map(|arg| (*arg).to_string())
                    .collect::<Vec<_>>();
                args.push(path);
                return Ok(Launch {
                    program,
                    args,
                    directory: None,
                    wait: false,
                });
            }
        }
        for name in INHERITING {
            if let Some(program) = resolve(name) {
                return Ok(Launch {
                    program,
                    args: Vec::new(),
                    directory: Some(directory.to_path_buf()),
                    wait: false,
                });
            }
        }
        return Err("no terminal emulator was found".to_string());
    }
    Err("this operating system has no system terminal Octoboard can open".to_string())
}

/// The Linux launch for one shell snapshot. `resolve` maps a program name through that snapshot.
/// The snapshot's own error is returned unchanged, and "no emulator" comes only from
/// [`launch_for`] after a snapshot that ran.
fn linux_launch(
    directory: &Path,
    snapshot: Result<HashMap<String, String>, String>,
    resolve: impl Fn(&str, &HashMap<String, String>) -> Option<String>,
) -> Result<Launch, String> {
    let env = snapshot?;
    launch_for("linux", directory, |name| resolve(name, &env))
}

fn start(launch: &Launch) -> Result<(), String> {
    let mut command = Command::new(&launch.program);
    command.args(&launch.args);
    if let Some(directory) = &launch.directory {
        command.current_dir(directory);
    }
    if launch.wait {
        let output = crate::subprocess::run_with_timeout(&mut command, OPEN_TIMEOUT)
            .map_err(|err| format!("{err:#}"))?;
        if output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if !stderr.is_empty() {
            return Err(stderr);
        }
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !stdout.is_empty() {
            return Err(stdout);
        }
        return Err(format!("the launcher exited with {}", output.status));
    }
    spawn_detached(command)
}

/// Starts `command` in a new session and reaps it on another thread. The new session is what
/// keeps the terminal alive after the daemon exits; the thread is what keeps it from staying a
/// zombie until then. A failure to exec is reported by `spawn` itself.
fn spawn_detached(mut command: Command) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: `setsid` takes no arguments and only affects this child, which has not returned
    // from `exec` yet. If it fails, the closure returns that error, `spawn` ends the child
    // before `exec` and returns the error, and no terminal is started.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command.spawn().map_err(|err| err.to_string())?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(names: &'static [&'static str]) -> impl Fn(&str) -> Option<String> {
        move |name| names.contains(&name).then(|| format!("/usr/bin/{name}"))
    }

    #[test]
    fn macos_asks_terminal_app_for_the_directory_and_waits_only_for_open() {
        let launch = launch_for("macos", Path::new("/work/app"), resolved(&[])).unwrap();
        assert_eq!(
            launch,
            Launch {
                program: "/usr/bin/open".to_string(),
                args: vec![
                    "-a".to_string(),
                    "Terminal".to_string(),
                    "--".to_string(),
                    "/work/app".to_string(),
                ],
                directory: None,
                wait: true,
            }
        );
    }

    #[test]
    fn linux_uses_the_first_installed_program_that_takes_a_directory() {
        let launch = launch_for(
            "linux",
            Path::new("/work/app"),
            resolved(&["xterm", "kitty"]),
        )
        .unwrap();
        assert_eq!(launch.program, "/usr/bin/kitty");
        assert_eq!(
            launch.args,
            vec!["--directory".to_string(), "/work/app".to_string()]
        );
        assert_eq!(launch.directory, None);
        assert!(!launch.wait);
    }

    #[test]
    fn linux_falls_back_to_a_program_that_inherits_its_working_directory() {
        let launch = launch_for("linux", Path::new("/work/app"), resolved(&["xterm"])).unwrap();
        assert_eq!(launch.program, "/usr/bin/xterm");
        assert!(launch.args.is_empty());
        assert_eq!(launch.directory.as_deref(), Some(Path::new("/work/app")));
    }

    #[test]
    fn linux_reports_a_failed_shell_snapshot_instead_of_a_missing_emulator() {
        let err = linux_launch(
            Path::new("/work/app"),
            Err("the login shell did not finish".to_string()),
            |_, _| Some("/usr/bin/kitty".to_string()),
        )
        .unwrap_err();
        assert_eq!(err, "the login shell did not finish");
    }

    #[test]
    fn linux_reports_no_emulator_only_after_the_snapshot_ran() {
        let err =
            linux_launch(Path::new("/work/app"), Ok(HashMap::new()), |_, _| None).unwrap_err();
        assert_eq!(err, "no terminal emulator was found");
    }

    #[test]
    fn linux_with_no_terminal_and_any_other_operating_system_are_refused() {
        assert!(launch_for("linux", Path::new("/work/app"), resolved(&[])).is_err());
        assert!(launch_for("windows", Path::new("/work/app"), resolved(&["kitty"])).is_err());
    }
}
