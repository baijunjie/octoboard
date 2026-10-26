//! Opening project directories and live paths with the daemon host's system applications, or
//! revealing an entry in its file manager. macOS open and Linux ShowItems return once accepted;
//! Linux xdg-open is detached and reaped because it may live as long as the application it opens.

use std::path::Path;
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use crate::protocol::{error_code, CodedError};
use crate::subprocess::{run_bounded, Bounds, Reach};

const OPEN_BOUNDS: Bounds = Bounds {
    timeout: Duration::from_secs(15),
    stdout_limit: 16 * 1024,
    stderr_limit: 16 * 1024,
    reach: Reach::ProcessGroup,
};

pub fn open(directory: &Path) -> Result<(), anyhow::Error> {
    validate_directory(directory).map_err(|detail| unavailable(&detail))?;
    let command = launch_for(std::env::consts::OS, directory, || {
        let env = crate::env_shell::cached_snapshot().map_err(|err| format!("{err:#}"))?;
        crate::env_shell::resolve_binary("xdg-open", &env)
            .map_err(|_| "xdg-open was not found".to_string())
    })
    .map_err(|detail| unavailable(&detail))?;
    start(std::env::consts::OS == "linux", command).map_err(|detail| unavailable(&detail))
}

/// Opens a resolved live path, either with its default application or selected in its folder.
pub fn open_path(path: &Path, reveal: bool) -> Result<(), anyhow::Error> {
    let command = path_launch_for(std::env::consts::OS, path, reveal, |binary| {
        let env = crate::env_shell::cached_snapshot().map_err(|err| format!("{err:#}"))?;
        crate::env_shell::resolve_binary(binary, &env)
            .map_err(|_| format!("{binary} was not found"))
    })
    .map_err(|detail| path_unavailable(reveal, &detail))?;
    // ShowItems is a short D-Bus call whose failure must reach the client; xdg-open may instead
    // live as long as the application it opens.
    let detached = std::env::consts::OS == "linux" && !reveal;
    start(detached, command)
        .or_else(
            |detail| match fallback_launch(std::env::consts::OS, path, reveal, &detail) {
                Some(command) => start(false, command),
                None => Err(detail),
            },
        )
        .map_err(|detail| path_unavailable(reveal, &detail))
}

/// How many leading bytes of an unclaimed file are sniffed for text: judging a large file needs
/// only its head, not a read of the whole file.
const TEXT_SNIFF_BYTES: usize = 8192;

/// Finder opens a text file no application claims in the default text editor; macOS open only
/// does so when asked with -t, so a failure for want of a claiming application is retried that
/// way when the file reads as text.
fn fallback_launch(os: &str, path: &Path, reveal: bool, detail: &str) -> Option<Command> {
    if os != "macos" || reveal || !no_application_claims(detail) || !looks_like_text(path) {
        return None;
    }
    let mut command = Command::new("/usr/bin/open");
    command.args(["-t", "--"]);
    command.arg(path);
    command.stdin(std::process::Stdio::null());
    Some(command)
}

/// Whether macOS open failed because Launch Services binds no application to the file's type,
/// as with an unregistered extension such as `.env.secret`.
fn no_application_claims(detail: &str) -> bool {
    detail.contains("Code=-10814")
}

/// Sniffs the head of a regular file: no NUL byte and valid UTF-8, bar a sequence cut at the end.
/// The open does not block, so a FIFO swapped in for the file is refused rather than waited on.
fn looks_like_text(path: &Path) -> bool {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let Ok(file) = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY)
        .open(path)
    else {
        return false;
    };
    if !file.metadata().is_ok_and(|meta| meta.is_file()) {
        return false;
    }
    let mut head = Vec::with_capacity(TEXT_SNIFF_BYTES);
    if file
        .take(TEXT_SNIFF_BYTES as u64)
        .read_to_end(&mut head)
        .is_err()
        || head.contains(&0)
    {
        return false;
    }
    match std::str::from_utf8(&head) {
        Ok(_) => true,
        Err(error) => error.error_len().is_none() && head.len() == TEXT_SNIFF_BYTES,
    }
}

fn path_unavailable(reveal: bool, detail: &str) -> anyhow::Error {
    if reveal {
        return unavailable(detail);
    }
    CodedError::raised(
        error_code::PATH_OPEN_FAILED,
        format!("The file or folder could not be opened: {detail}"),
        &[("detail", detail)],
    )
}

fn path_launch_for(
    os: &str,
    path: &Path,
    reveal: bool,
    resolve: impl FnOnce(&str) -> Result<String, String>,
) -> Result<Command, String> {
    let mut command = match os {
        "macos" => {
            let mut command = Command::new("/usr/bin/open");
            if reveal {
                command.arg("-R");
            }
            command.arg("--");
            command.arg(path);
            command
        }
        "linux" if reveal => {
            let mut command = Command::new(resolve("gdbus")?);
            command.args([
                "call",
                "--session",
                "--dest",
                "org.freedesktop.FileManager1",
                "--object-path",
                "/org/freedesktop/FileManager1",
                "--method",
                "org.freedesktop.FileManager1.ShowItems",
            ]);
            command.arg(format!("['{}']", file_uri(path)));
            command.arg("");
            command
        }
        "linux" => {
            let mut command = Command::new(resolve("xdg-open")?);
            command.arg(path);
            command
        }
        _ => return Err("this operating system has no supported system file manager".to_string()),
    };
    command.stdin(std::process::Stdio::null());
    Ok(command)
}

fn file_uri(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut uri = String::from("file://");
    for &byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~') {
            uri.push(byte as char);
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
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

fn start(detached: bool, mut command: Command) -> Result<(), String> {
    if detached {
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
    fn open_and_reveal_failures_name_the_action_that_failed() {
        for (reveal, expected) in [
            (false, error_code::PATH_OPEN_FAILED),
            (true, error_code::DIRECTORY_UNAVAILABLE),
        ] {
            let error = path_unavailable(reveal, "no default application");
            let (code, params, _) = error_parts(&error);
            assert_eq!(code, expected);
            assert_eq!(
                params.get("detail").map(String::as_str),
                Some("no default application")
            );
        }
    }

    #[test]
    fn only_a_text_file_no_application_claims_is_retried_in_the_text_editor_on_macos() {
        let unclaimed = "No application knows how to open URL file:///a.secret (Error Domain=NSOSStatusErrorDomain Code=-10814 \"kLSApplicationNotFoundErr\")";
        let dir = ScratchDir::new("open-text-fallback");
        let text = dir.join("-.env.secret");
        std::fs::write(&text, "TOKEN=é\n").unwrap();

        let command = fallback_launch("macos", &text, false, unclaimed).unwrap();
        assert_eq!(command.get_program(), "/usr/bin/open");
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(
            args,
            [
                std::ffi::OsStr::new("-t"),
                std::ffi::OsStr::new("--"),
                text.as_os_str()
            ]
        );
        assert!(fallback_launch("macos", &text, true, unclaimed).is_none());
        assert!(fallback_launch("linux", &text, false, unclaimed).is_none());
        assert!(
            fallback_launch("macos", &text, false, "The file /a-10814 does not exist.").is_none()
        );

        let mut cut = vec![b'a'; TEXT_SNIFF_BYTES - 1];
        cut.extend_from_slice("é".as_bytes());
        let fifo = dir.join("fifo");
        let c_fifo = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c_fifo.as_ptr(), 0o600) }, 0);
        for (name, bytes, text) in [
            ("cut", Some(cut), true),
            ("nul", Some(b"a\0b".to_vec()), false),
            ("latin1", Some(b"caf\xe9 ok".to_vec()), false),
            ("fifo", None, false),
            ("missing", None, false),
        ] {
            let path = dir.join(name);
            if let Some(bytes) = bytes {
                std::fs::write(&path, bytes).unwrap();
            }
            let retried = fallback_launch("macos", &path, false, unclaimed).is_some();
            assert_eq!(retried, text, "{name}");
        }
        assert!(fallback_launch("macos", &dir, false, unclaimed).is_none());
    }

    #[test]
    fn path_actions_keep_paths_literal_and_reveal_selects_the_item() {
        let path = Path::new("/work/a '$(file)%é");
        for reveal in [false, true] {
            let command = path_launch_for("macos", path, reveal, |_| panic!("no lookup")).unwrap();
            assert_eq!(command.get_program(), "/usr/bin/open");
            let expected = if reveal {
                vec![
                    std::ffi::OsStr::new("-R"),
                    std::ffi::OsStr::new("--"),
                    path.as_os_str(),
                ]
            } else {
                vec![std::ffi::OsStr::new("--"), path.as_os_str()]
            };
            assert_eq!(command.get_args().collect::<Vec<_>>(), expected);
        }
        let command = path_launch_for("linux", path, true, |binary| {
            assert_eq!(binary, "gdbus");
            Ok("/usr/bin/gdbus".into())
        })
        .unwrap();
        let args = command.get_args().collect::<Vec<_>>();
        assert_eq!(args[7], "org.freedesktop.FileManager1.ShowItems");
        assert_eq!(args[8], "['file:///work/a%20%27%24%28file%29%25%C3%A9']");
        assert_eq!(args[9], "");
        let command = path_launch_for("linux", path, false, |binary| {
            assert_eq!(binary, "xdg-open");
            Ok("/usr/bin/xdg-open".into())
        })
        .unwrap();
        assert_eq!(command.get_args().collect::<Vec<_>>(), [path.as_os_str()]);
        assert!(path_launch_for("linux", path, true, |_| Err("missing gdbus".into())).is_err());
    }

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
        assert_eq!(start(false, command).unwrap_err(), "no desktop session");
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
        start(true, command).expect("the request finishes while the launcher is running");
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
        assert!(start(true, Command::new(dir.join("missing-launcher"))).is_err());
    }
}
