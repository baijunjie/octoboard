//! Opening project directories and live paths with the daemon host's system applications, or
//! revealing an entry in its file manager. macOS open and Linux ShowItems/ShowFolders return once
//! accepted; Linux xdg-open is detached and reaped because it may live as long as the application
//! it opens. Windows opens through the shell's own call and reveals through a detached
//! `explorer.exe`, whose exit status says nothing.

use std::path::Path;
use std::process::Command;
use std::sync::atomic::AtomicBool;

use super::{start, OPEN_BOUNDS};
use crate::platform::open::paths::{
    directory_launch_for, is_detached, package_probe, package_tree_is_package, path_launch_for,
    shell_open, text_editor_launch, PathAction,
};
use crate::processes::subprocess::run_bounded;
use crate::protocol::{error_code, CodedError};

pub fn open(directory: &Path) -> Result<(), anyhow::Error> {
    validate_directory(directory).map_err(|detail| unavailable(&detail))?;
    if let Some(opened) = shell_open(directory) {
        return opened.map_err(|detail| unavailable(&detail));
    }
    let command = directory_launch_for(std::env::consts::OS, directory, || {
        let env = crate::env_shell::cached_snapshot().map_err(|err| format!("{err:#}"))?;
        crate::env_shell::resolve_program("xdg-open", &env)
            .map_err(|_| "xdg-open was not found".to_string())
    })
    .map_err(|detail| unavailable(&detail))?;
    start(is_detached(std::env::consts::OS, PathAction::Open), command)
        .map_err(|detail| unavailable(&detail))
}

/// Opens a resolved live path, either with its default application or revealed in the file
/// manager: a folder is shown, anything else is selected in its folder.
pub fn open_path(path: &Path, reveal: bool) -> Result<(), anyhow::Error> {
    let action = if !reveal {
        PathAction::Open
    } else if shows_as_folder(std::env::consts::OS, path) {
        PathAction::ShowFolder
    } else {
        PathAction::Select
    };
    if action == PathAction::Open {
        if let Some(opened) = shell_open(path) {
            return opened.map_err(|detail| path_unavailable(reveal, &detail));
        }
    }
    let command = path_launch_for(std::env::consts::OS, path, action, |binary| {
        let env = crate::env_shell::cached_snapshot().map_err(|err| format!("{err:#}"))?;
        crate::env_shell::resolve_program(binary, &env)
            .map_err(|_| format!("{binary} was not found"))
    })
    .map_err(|detail| path_unavailable(reveal, &detail))?;
    // ShowItems and ShowFolders are short D-Bus calls whose failure must reach the client;
    // xdg-open may instead live as long as the application it opens.
    let detached = is_detached(std::env::consts::OS, action);
    start(detached, command)
        .or_else(
            |detail| match fallback_launch(std::env::consts::OS, path, reveal, &detail) {
                Some(command) => start(false, command),
                None => Err(detail),
            },
        )
        .map_err(|detail| path_unavailable(reveal, &detail))
}

/// Whether revealing `path` shows the folder itself rather than selecting it in its parent. A
/// symbolic link is judged by its own entry, so it stays selected whatever it points at. On
/// macOS a folder Finder treats as a package (`Foo.app`, `Foo.xcodeproj`) is selected too:
/// `open -a Finder` on one launches it instead of showing it.
fn shows_as_folder(os: &str, path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir()) && !is_package(os, path)
}

/// Whether macOS types the folder as a package; never on another host. See [`is_package`].
pub(crate) fn is_macos_package(path: &Path) -> bool {
    is_package(std::env::consts::OS, path)
}

/// Whether `os` types the folder as a package, which only macOS does
/// ([`package_probe`] has no probe elsewhere). A package is always named with an extension, so a
/// folder without one is skipped without asking. Spotlight's content type tree is what Finder
/// itself goes by (a bundle `Info.plist` check misses `.xcodeproj` and the like); when it cannot
/// be read the folder counts as a package, since selecting it is harmless where opening it may
/// launch something.
fn is_package(os: &str, path: &Path) -> bool {
    let Some(mut command) = package_probe(os, path) else {
        return false;
    };
    match run_bounded(&mut command, &OPEN_BOUNDS, &AtomicBool::new(false)) {
        Ok(output) if output.status.success() => {
            package_tree_is_package(&String::from_utf8_lossy(&output.stdout))
        }
        _ => true,
    }
}

/// How many leading bytes of an unclaimed file are sniffed for text: judging a large file needs
/// only its head, not a read of the whole file.
const TEXT_SNIFF_BYTES: usize = 8192;

/// Finder opens a text file no application claims in the default text editor; macOS open only
/// does so when asked with -t, so a failure for want of a claiming application is retried that
/// way when the file reads as text.
fn fallback_launch(os: &str, path: &Path, reveal: bool, detail: &str) -> Option<Command> {
    let command = text_editor_launch(os, path)?;
    (!reveal && no_application_claims(detail) && looks_like_text(path)).then_some(command)
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
    let Ok(file) = crate::hostfs::open_regular(path) else {
        return false;
    };
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
    crate::hostfs::require_directory(directory).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::time::Duration;

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
        #[cfg(unix)]
        crate::test_support::mkfifo(&dir.join("fifo"));
        for (name, bytes, text) in [
            ("cut", Some(cut), true),
            ("nul", Some(b"a\0b".to_vec()), false),
            ("latin1", Some(b"caf\xe9 ok".to_vec()), false),
            #[cfg(unix)]
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
    fn revealing_shows_a_folder_but_selects_files_and_links() {
        let dir = ScratchDir::new("reveal-kind");
        let folder = dir.join("folder");
        let file = dir.join("file");
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(&file, "content").unwrap();
        let to_folder = dir.join("to-folder");
        crate::test_support::symlink(&folder, &to_folder).unwrap();
        for os in ["linux", "macos"] {
            assert!(shows_as_folder(os, &folder), "{os}");
            assert!(!shows_as_folder(os, &file), "{os}");
            assert!(!shows_as_folder(os, &to_folder), "{os}");
            assert!(!shows_as_folder(os, &dir.join("missing")), "{os}");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_selects_a_package_instead_of_launching_it() {
        let dir = ScratchDir::new("reveal-package");
        let package = dir.join("Test.app");
        std::fs::create_dir_all(package.join("Contents")).unwrap();
        std::fs::write(
            package.join("Contents/Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>CFBundlePackageType</key><string>APPL</string></dict></plist>"#,
        )
        .unwrap();
        let project = dir.join("Foo.xcodeproj");
        let dotted = dir.join("v1.2");
        std::fs::create_dir(&project).unwrap();
        std::fs::create_dir(&dotted).unwrap();
        assert!(!shows_as_folder("macos", &package));
        assert!(!shows_as_folder("macos", &project));
        // Where Spotlight does not type the folder (indexing off) it is selected, by design.
        let typed = Command::new("/usr/bin/mdls")
            .args(["-raw", "-name", "kMDItemContentTypeTree", "--"])
            .arg(&dotted)
            .output()
            .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).contains("public.directory"));
        if typed {
            assert!(shows_as_folder("macos", &dotted));
        }
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
        #[cfg(unix)]
        let command = {
            let mut command = Command::new("/bin/sh");
            command.args(["-c", "printf 'no desktop session' >&2; exit 1"]);
            command
        };
        #[cfg(windows)]
        let command = {
            let mut command = Command::new("cmd");
            command.args(["/d", "/c", "echo no desktop session 1>&2 & exit /b 1"]);
            command
        };
        assert_eq!(start(false, command).unwrap_err(), "no desktop session");
    }

    #[cfg(unix)]
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
