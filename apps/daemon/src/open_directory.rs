//! Opening a project directory in the system file manager on the daemon's host. The launcher
//! on macOS returns once the directory is accepted. Linux xdg-open may keep running for the
//! file manager's lifetime, so it is detached and reaped in the background.

use std::path::Path;
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use crate::protocol::{error_code, CodedError};
use crate::subprocess::{run_bounded, Bounds};

const OPEN_BOUNDS: Bounds = Bounds {
    timeout: Duration::from_secs(15),
    stdout_limit: 16 * 1024,
    stderr_limit: 16 * 1024,
};

pub fn open(directory: &Path) -> Result<(), anyhow::Error> {
    validate_directory(directory).map_err(|detail| unavailable(&detail))?;
    let command = launch_for(std::env::consts::OS, directory, || {
        let env = crate::env_shell::cached_snapshot().map_err(|err| format!("{err:#}"))?;
        crate::env_shell::resolve_binary("xdg-open", &env)
            .map_err(|_| "xdg-open was not found".to_string())
    })
    .map_err(|detail| unavailable(&detail))?;
    start(std::env::consts::OS, command).map_err(|detail| unavailable(&detail))
}

fn unavailable(detail: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::DIRECTORY_UNAVAILABLE,
        format!("The system file manager could not be opened: {detail}"),
        &[("detail", detail)],
    )
}

fn validate_directory(directory: &Path) -> Result<(), String> {
    if !directory.is_absolute() {
        return Err("the project directory is not an absolute path".to_string());
    }
    if !directory.is_dir() {
        return Err("the project directory does not exist or is not a directory".to_string());
    }
    Ok(())
}

fn launch_for(
    os: &str,
    directory: &Path,
    resolve: impl FnOnce() -> Result<String, String>,
) -> Result<Command, String> {
    let mut command = match os {
        "macos" => {
            let mut command = Command::new("/usr/bin/open");
            command.arg("--");
            command
        }
        "linux" => Command::new(resolve()?),
        _ => return Err("this operating system has no supported system file manager".to_string()),
    };
    // Stored project paths are absolute, so xdg-open cannot interpret one as an option or URL.
    command.arg(directory);
    Ok(command)
}

fn start(os: &str, mut command: Command) -> Result<(), String> {
    if os == "linux" {
        return crate::subprocess::spawn_detached(command);
    }
    let output = run_bounded(&mut command, &OPEN_BOUNDS, &AtomicBool::new(false))
        .map_err(|err| err.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    for bytes in [&output.stderr, &output.stdout] {
        let message = String::from_utf8_lossy(bytes).trim().to_string();
        if !message.is_empty() {
            return Err(message);
        }
    }
    Err(format!("the launcher exited with {}", output.status))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::error_parts;
    use crate::test_support::ScratchDir;

    #[test]
    fn macos_uses_open_without_resolving_a_shell_and_keeps_the_path_as_one_argument() {
        let command = launch_for("macos", Path::new("/work/project '$(name)"), || {
            panic!("macOS does not resolve xdg-open")
        })
        .unwrap();
        assert_eq!(command.get_program(), "/usr/bin/open");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["--", "/work/project '$(name)"]
        );
    }

    #[test]
    fn linux_uses_the_resolved_launcher_and_reports_lookup_failures() {
        let command = launch_for("linux", Path::new("/work/app"), || {
            Ok("/custom/bin/xdg-open".to_string())
        })
        .unwrap();
        assert_eq!(command.get_program(), "/custom/bin/xdg-open");
        assert_eq!(command.get_args().collect::<Vec<_>>(), ["/work/app"]);
        for detail in ["xdg-open was not found", "the login shell did not finish"] {
            assert_eq!(
                launch_for("linux", Path::new("/work/app"), || Err(detail.to_string()))
                    .unwrap_err(),
                detail
            );
        }
        assert!(launch_for("windows", Path::new("/work/app"), || panic!("no lookup")).is_err());
    }

    #[test]
    fn missing_non_directory_and_relative_paths_are_refused_before_launch() {
        let dir = ScratchDir::new("open-directory");
        let file = dir.join("file");
        std::fs::write(&file, "content").unwrap();
        assert!(validate_directory(&dir).is_ok());
        for path in [dir.join("missing"), file, "relative".into()] {
            assert!(validate_directory(&path).is_err());
            let error = open(&path).unwrap_err();
            let (code, params, _) = error_parts(&error);
            assert_eq!(code, error_code::DIRECTORY_UNAVAILABLE);
            assert!(params.contains_key("detail"));
        }
    }

    #[test]
    fn launcher_failures_include_the_process_diagnostic() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "printf 'no desktop session' >&2; exit 1"]);
        assert_eq!(start("macos", command).unwrap_err(), "no desktop session");
    }

    #[test]
    fn linux_acknowledges_a_detached_running_launcher_and_reaps_it_after_exit() {
        use crate::test_support::{assert_gone_by, read_pid_file, PATIENCE};
        use std::time::Instant;

        struct RunningLauncher(i32);
        impl Drop for RunningLauncher {
            fn drop(&mut self) {
                unsafe {
                    libc::kill(-self.0, libc::SIGKILL);
                    libc::kill(self.0, libc::SIGKILL);
                }
                assert_gone_by(self.0, Instant::now() + PATIENCE);
            }
        }

        let dir = ScratchDir::new("detached-directory");
        let pid_file = dir.join("pid");
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "echo $$ > \"$1\"; exec /bin/sleep 30", "launcher"]);
        command.arg(&pid_file);
        start("linux", command).expect("the request finishes while the launcher is running");
        let deadline = Instant::now() + PATIENCE;
        let pid = loop {
            if let Some(pid) = read_pid_file(&pid_file) {
                break pid;
            }
            assert!(Instant::now() < deadline, "the fixture shell did not run");
            std::thread::sleep(Duration::from_millis(10));
        };
        let launcher = RunningLauncher(pid);
        assert_eq!(
            unsafe { libc::getsid(pid) },
            pid,
            "a new session survives the daemon"
        );
        assert_eq!(
            unsafe { libc::kill(pid, 0) },
            0,
            "ack did not wait for exit"
        );
        drop(launcher);
        assert!(start("linux", Command::new(dir.join("missing-launcher"))).is_err());
    }
}
