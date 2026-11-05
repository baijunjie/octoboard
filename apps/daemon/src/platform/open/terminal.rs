//! The system terminal's launch, on the machine the daemon runs on. On macOS that program is
//! Terminal.app: `open -a Terminal` hands it the directory and returns once Terminal has accepted
//! it, which starts a shell there. On Linux there is no single system terminal, so the first
//! program in a fixed list that can be given a directory is used. On Windows it is Windows Terminal
//! (`wt.exe`) when it is installed, else a console window of `cmd.exe`. Choosing the launch is a pure
//! function of an OS name and a program lookup, so every host's choice is tested on any machine;
//! starting it is the caller's.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// What to run, and whether to wait for it. `wait` is only for a launcher that exits once the
/// terminal has accepted the directory (`open`). A terminal emulator itself has `wait` false.
#[derive(Debug, PartialEq, Eq)]
pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
    /// The process's working directory, for a program that starts its shell there and has no
    /// directory argument of its own.
    pub directory: Option<PathBuf>,
    pub wait: bool,
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

/// The launch for `os`. `resolve` maps a program name to its absolute path, for the Linux list;
/// macOS does not use it, because Terminal.app is reached through `/usr/bin/open`. A pure function
/// over those two, so the choice is unit-tested without starting a terminal.
pub fn launch_for(
    os: &str,
    directory: &Path,
    resolve: impl Fn(&str) -> Option<String>,
) -> Result<Launch, String> {
    // Only a launch that puts the directory on its command line needs it as text; Windows hands it
    // over as the working directory, which takes any path.
    let path = || {
        directory
            .to_str()
            .map(str::to_string)
            .ok_or("the project path is not valid Unicode")
    };
    if os == "macos" {
        return Ok(Launch {
            program: "/usr/bin/open".to_string(),
            args: vec![
                "-a".to_string(),
                "Terminal".to_string(),
                "--".to_string(),
                path()?,
            ],
            directory: None,
            wait: true,
        });
    }
    if os == "linux" {
        let path = path()?;
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
    if os == "windows" {
        // The directory is the launch's working directory rather than an argument, since both
        // programs parse their command line: Windows Terminal splits it into commands at a `;`,
        // and `cmd` reads `&`, `|`, `^`, `<`, `>` and `(` in it. `-d .` is that directory.
        if let Some(program) = resolve("wt.exe") {
            return Ok(Launch {
                program,
                args: vec!["-d".to_string(), ".".to_string()],
                directory: Some(directory.to_path_buf()),
                wait: false,
            });
        }
        // `start` gives the shell a console window of its own, which a program started without a
        // console would not otherwise have; the shell starts in the directory `start` runs in.
        return Ok(Launch {
            program: "cmd.exe".to_string(),
            args: ["/d", "/c", "start", "", "cmd.exe", "/k"]
                .map(str::to_string)
                .to_vec(),
            directory: Some(directory.to_path_buf()),
            wait: false,
        });
    }
    Err("this operating system has no system terminal Octoboard can open".to_string())
}

/// The launch for `os`, looking programs up on the shell snapshot `snapshot` gives only where `os`
/// looks a terminal up at all: macOS reaches Terminal.app through `/usr/bin/open` and never asks
/// for one, so a snapshot that cannot be taken does not stand in its way.
pub fn host_launch(
    os: &str,
    directory: &Path,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>, String>,
    resolve: impl Fn(&str, &HashMap<String, String>) -> Option<String>,
) -> Result<Launch, String> {
    if os == "macos" {
        return launch_for(os, directory, |_| None);
    }
    snapshot_launch(os, directory, snapshot(), resolve)
}

/// The launch for `os` where programs are looked up on one shell snapshot's `PATH`. `resolve` maps
/// a program name through that snapshot. The snapshot's own error is returned unchanged, and "no
/// emulator" comes only from [`launch_for`] after a snapshot that ran.
pub fn snapshot_launch(
    os: &str,
    directory: &Path,
    snapshot: Result<HashMap<String, String>, String>,
    resolve: impl Fn(&str, &HashMap<String, String>) -> Option<String>,
) -> Result<Launch, String> {
    let env = snapshot?;
    launch_for(os, directory, |name| resolve(name, &env))
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
        let err = snapshot_launch(
            "linux",
            Path::new("/work/app"),
            Err("the login shell did not finish".to_string()),
            |_, _| Some("/usr/bin/kitty".to_string()),
        )
        .unwrap_err();
        assert_eq!(err, "the login shell did not finish");
    }

    #[test]
    fn linux_reports_no_emulator_only_after_the_snapshot_ran() {
        let err = snapshot_launch(
            "linux",
            Path::new("/work/app"),
            Ok(HashMap::new()),
            |_, _| None,
        )
        .unwrap_err();
        assert_eq!(err, "no terminal emulator was found");
    }

    #[test]
    fn linux_with_no_terminal_and_any_other_operating_system_are_refused() {
        assert!(launch_for("linux", Path::new("/work/app"), resolved(&[])).is_err());
        assert!(launch_for("freebsd", Path::new("/work/app"), resolved(&["kitty"])).is_err());
    }

    #[test]
    fn windows_uses_windows_terminal_when_it_is_installed_and_a_new_console_otherwise() {
        let dir = Path::new(r"C:\work\my app");
        let terminal = |name: &str| (name == "wt.exe").then(|| r"C:\wt\wt.exe".to_string());
        assert_eq!(
            launch_for("windows", dir, terminal).unwrap(),
            Launch {
                program: r"C:\wt\wt.exe".to_string(),
                args: vec!["-d".to_string(), ".".to_string()],
                directory: Some(dir.to_path_buf()),
                wait: false,
            }
        );
        assert_eq!(
            launch_for("windows", dir, |_| None).unwrap(),
            Launch {
                program: "cmd.exe".to_string(),
                args: ["/d", "/c", "start", "", "cmd.exe", "/k"]
                    .map(str::to_string)
                    .to_vec(),
                directory: Some(dir.to_path_buf()),
                wait: false,
            }
        );
    }

    /// A directory that is not valid Unicode is no obstacle where it is only the working
    /// directory; one that would go on a command line is refused.
    #[cfg(unix)]
    #[test]
    fn only_a_launch_that_puts_the_directory_on_its_command_line_needs_it_as_unicode() {
        use std::os::unix::ffi::OsStrExt;
        let directory = Path::new(std::ffi::OsStr::from_bytes(b"/work/\xff"));
        for found in [&["wt.exe"][..], &[]] {
            let launch = launch_for("windows", directory, resolved(found)).unwrap();
            assert_eq!(launch.directory.as_deref(), Some(directory));
        }
        assert!(launch_for("macos", directory, resolved(&[])).is_err());
        assert!(launch_for("linux", directory, resolved(&["kitty"])).is_err());
    }

    /// A directory whose name a command line would read as syntax — Windows Terminal's `;`, the
    /// characters `cmd` acts on — never reaches either command line.
    #[test]
    fn a_windows_directory_never_reaches_a_command_line() {
        let dir = Path::new(r"C:\work\a;b & c|d^e<f>g(h)");
        for resolve in [Some(r"C:\wt\wt.exe"), None] {
            let launch = launch_for("windows", dir, |name: &str| {
                (name == "wt.exe")
                    .then(|| resolve.map(str::to_string))
                    .flatten()
            })
            .unwrap();
            assert_eq!(launch.directory.as_deref(), Some(dir));
            assert!(
                launch.args.iter().all(|arg| !arg.contains("a;b")),
                "{launch:?}"
            );
        }
    }

    /// macOS never takes a snapshot, so one that fails does not stop Terminal.app; elsewhere the
    /// snapshot's failure is the launch's.
    #[test]
    fn only_a_host_that_looks_a_terminal_up_takes_a_snapshot() {
        let failed = || Err("the login shell did not finish".to_string());
        let anything = |_: &str, _: &HashMap<String, String>| Some("/usr/bin/kitty".to_string());
        assert_eq!(
            host_launch("macos", Path::new("/work/app"), failed, anything)
                .unwrap()
                .program,
            "/usr/bin/open"
        );
        for os in ["linux", "windows"] {
            assert_eq!(
                host_launch(os, Path::new("/work/app"), failed, anything).unwrap_err(),
                "the login shell did not finish",
                "{os}"
            );
        }
    }
}
