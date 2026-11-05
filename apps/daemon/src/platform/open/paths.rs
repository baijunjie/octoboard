//! The commands that open a project directory or a live path with the host's own applications,
//! or reveal one in its file manager. Which one a host takes is chosen by an OS name passed in
//! (`std::env::consts::OS` at the call site), so every host's choice is tested on any machine;
//! what may be opened, and starting the command, are the caller's. Windows opens a file or a folder
//! through the shell's own call rather than a command ([`shell_open`]) and reveals through
//! `explorer.exe`.

use std::path::Path;
use std::process::Command;

/// What the host is asked to do with a resolved live path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathAction {
    /// Open it with its default application, or a folder in the file manager.
    Open,
    /// Select it in its containing folder.
    Select,
    /// Show a folder's own contents.
    ShowFolder,
}

pub fn path_launch_for(
    os: &str,
    path: &Path,
    action: PathAction,
    resolve: impl FnOnce(&str) -> Result<String, String>,
) -> Result<Command, String> {
    let mut command = match os {
        "macos" => {
            let mut command = Command::new("/usr/bin/open");
            match action {
                PathAction::Open => {}
                PathAction::Select => {
                    command.arg("-R");
                }
                // Without -a, open hands a folder to the application that claims it.
                PathAction::ShowFolder => {
                    command.args(["-a", "Finder"]);
                }
            }
            command.arg("--");
            command.arg(path);
            command
        }
        "linux" if action != PathAction::Open => {
            let mut command = Command::new(resolve("gdbus")?);
            command.args([
                "call",
                "--session",
                "--dest",
                "org.freedesktop.FileManager1",
                "--object-path",
                "/org/freedesktop/FileManager1",
                "--method",
            ]);
            command.arg(if action == PathAction::ShowFolder {
                "org.freedesktop.FileManager1.ShowFolders"
            } else {
                "org.freedesktop.FileManager1.ShowItems"
            });
            command.arg(format!("['{}']", file_uri(path)));
            command.arg("");
            command
        }
        "linux" => {
            let mut command = Command::new(resolve("xdg-open")?);
            command.arg(path);
            command
        }
        "windows" => {
            let mut command = Command::new("explorer.exe");
            match action {
                PathAction::Select => {
                    let select = explorer_select_argument(path)
                        .ok_or("a path holding a quotation mark cannot be selected")?;
                    // As written: quoting it whole, as the standard library quotes an argument
                    // with a space or a comma in it, makes Explorer miss the switch.
                    #[cfg(windows)]
                    std::os::windows::process::CommandExt::raw_arg(&mut command, select);
                    #[cfg(not(windows))]
                    command.arg(select);
                }
                PathAction::Open | PathAction::ShowFolder => {
                    command.arg(path);
                }
            }
            command
        }
        _ => return Err("this operating system has no supported system file manager".to_string()),
    };
    command.stdin(std::process::Stdio::null());
    Ok(command)
}

/// The command-line text `explorer.exe` takes to select `path` in its folder: the switch
/// `/select,` bare, then the path in quotes. Explorer reads its command line as a list split at
/// commas outside quotes, so the path is quoted to keep a comma in it, and the switch is left out of
/// the quotes, or Explorer reads the whole as one path and opens its default folder instead. A run
/// of backslashes before the closing quote is doubled, as the Windows C runtime's rule for a quoted
/// argument asks, so a path ending in one (a drive's root) cannot escape the quote. `None` for a
/// path holding a quotation mark, which a Windows path never does.
fn explorer_select_argument(path: &Path) -> Option<std::ffi::OsString> {
    let bytes = path.as_os_str().as_encoded_bytes();
    if bytes.contains(&b'"') {
        return None;
    }
    let trailing = bytes
        .iter()
        .rev()
        .take_while(|&&byte| byte == b'\\')
        .count();
    let mut argument = std::ffi::OsString::from("/select,\"");
    argument.push(path);
    argument.push("\\".repeat(trailing));
    argument.push("\"");
    Some(argument)
}

/// Whether the launcher `path_launch_for` or `directory_launch_for` gives for `action` is started
/// detached instead of waited for: one that lives as long as the application it opens (`xdg-open`),
/// or whose exit status says nothing about whether it worked (`explorer.exe` exits 1 on success).
pub fn is_detached(os: &str, action: PathAction) -> bool {
    match os {
        "linux" => action == PathAction::Open,
        "windows" => true,
        _ => false,
    }
}

/// Asks the host's shell to open `path` as a double-click would, where the host has such a call
/// (`ShellExecute` on Windows), and says what it answered; `None` where it has not, and a command
/// opens it instead.
pub fn shell_open(path: &Path) -> Option<Result<(), String>> {
    #[cfg(windows)]
    {
        let path = path.to_path_buf();
        Some(answered_within(SHELL_ANSWER_WAIT, move || {
            shell_execute(&path)
        }))
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        None
    }
}

/// How long the shell's answer to an open is waited for. `ShellExecuteW` returns once the
/// application is started or the open has failed, but for a file of a type no application is
/// associated with it shows the "Open with" chooser instead and returns only when that is closed,
/// however long the user leaves it up.
#[cfg(windows)]
const SHELL_ANSWER_WAIT: std::time::Duration = std::time::Duration::from_secs(3);

/// What `call` answers, run on a thread of its own, or success once `wait` has passed without an
/// answer: a call still running then is the shell showing a window of its own, which is the open
/// handed over. The thread is left to finish by itself, so neither the request nor the daemon's
/// exit waits on a window the user has left open.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "only Windows opens through a shell call")
)]
fn answered_within(
    wait: std::time::Duration,
    call: impl FnOnce() -> Result<(), String> + Send + 'static,
) -> Result<(), String> {
    let (answer, answered) = std::sync::mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("shell-open".into())
        .spawn(move || {
            let _ = answer.send(call());
        })
        .map_err(|err| format!("the shell could not be asked to open it: {err}"))?;
    match answered.recv_timeout(wait) {
        Ok(result) => result,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(()),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            Err("the shell call ended without an answer".to_string())
        }
    }
}

#[cfg(windows)]
fn shell_execute(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Com::{
        CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
    };
    use windows_sys::Win32::UI::Shell::ShellExecuteW;

    /// `SW_SHOWNORMAL` (winuser.h).
    const SHOW_NORMAL: i32 = 1;
    let verb: Vec<u16> = "open\0".encode_utf16().collect();
    let file: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // `ShellExecuteW` may hand the open to a shell extension, which is COM, and asks for the thread
    // to be initialised for it this way. This runs on a `shell-open` thread of its own, so nothing
    // else on that thread relies on COM staying initialised.
    // SAFETY: the reserved pointer must be null, and it is.
    let com = unsafe {
        CoInitializeEx(
            std::ptr::null(),
            (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
        )
    };
    // SAFETY: both strings are NUL-terminated and outlive the call; the other pointers are null.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SHOW_NORMAL,
        )
    } as usize;
    // Every initialisation that succeeded, including one that found COM already set up, is ended.
    if com >= 0 {
        // SAFETY: paired with the successful `CoInitializeEx` above, on the same thread.
        unsafe { CoUninitialize() };
    }
    // A value above 32 is success; below it, the error.
    match result {
        0..=32 => Err(match result {
            2 | 3 => "the file or folder was not found".to_string(),
            5 => "access to the file or folder was denied".to_string(),
            // Never answered where the shell shows its "Open with" chooser for a type with no
            // application instead; kept for a shell that answers it.
            31 => "no application is associated with this file type".to_string(),
            code => format!("the shell could not open it (error {code})"),
        }),
        _ => Ok(()),
    }
}

fn file_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    for &byte in path.as_os_str().as_encoded_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~') {
            uri.push(byte as char);
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}

pub fn directory_launch_for(
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
        "windows" => Command::new("explorer.exe"),
        _ => return Err("this operating system has no supported system file manager".to_string()),
    };
    // Stored project paths are absolute, so xdg-open cannot interpret one as an option or URL.
    command.arg(directory);
    Ok(command)
}

/// The command that asks Spotlight for the content type tree of `path`, or `None` for a path
/// whose name has no extension: a package is always named with one, so such a folder is skipped
/// without asking, and `None` on any `os` but macOS, which alone has packages. Its output goes to
/// [`package_tree_is_package`].
pub fn package_probe(os: &str, path: &Path) -> Option<Command> {
    if os != "macos" {
        return None;
    }
    path.extension()?;
    let mut command = Command::new("/usr/bin/mdls");
    command.args(["-raw", "-name", "kMDItemContentTypeTree", "--"]);
    command.arg(path);
    command.stdin(std::process::Stdio::null());
    Some(command)
}

/// Whether the content type tree `package_probe` printed types the folder as a package. A tree
/// without `public.directory` (Spotlight's "(null)") says nothing about it, so it counts as one:
/// selecting a folder is harmless where opening it may launch something.
pub fn package_tree_is_package(tree: &str) -> bool {
    tree.contains("com.apple.package") || !tree.contains("public.directory")
}

/// The macOS command that opens `path` in the default text editor, which `open` only does when
/// asked with `-t`; `None` on any other OS.
pub fn text_editor_launch(os: &str, path: &Path) -> Option<Command> {
    if os != "macos" {
        return None;
    }
    let mut command = Command::new("/usr/bin/open");
    command.args(["-t", "--"]);
    command.arg(path);
    command.stdin(std::process::Stdio::null());
    Some(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_actions_keep_paths_literal_per_action() {
        let path = Path::new("/work/a '$(file)%é");
        for action in [PathAction::Open, PathAction::Select, PathAction::ShowFolder] {
            let command = path_launch_for("macos", path, action, |_| panic!("no lookup")).unwrap();
            assert_eq!(command.get_program(), "/usr/bin/open");
            let expected = match action {
                PathAction::Open => vec![std::ffi::OsStr::new("--"), path.as_os_str()],
                PathAction::Select => vec![
                    std::ffi::OsStr::new("-R"),
                    std::ffi::OsStr::new("--"),
                    path.as_os_str(),
                ],
                PathAction::ShowFolder => vec![
                    std::ffi::OsStr::new("-a"),
                    std::ffi::OsStr::new("Finder"),
                    std::ffi::OsStr::new("--"),
                    path.as_os_str(),
                ],
            };
            assert_eq!(command.get_args().collect::<Vec<_>>(), expected);
        }
        for (action, method) in [
            (PathAction::Select, "org.freedesktop.FileManager1.ShowItems"),
            (
                PathAction::ShowFolder,
                "org.freedesktop.FileManager1.ShowFolders",
            ),
        ] {
            let command = path_launch_for("linux", path, action, |binary| {
                assert_eq!(binary, "gdbus");
                Ok("/usr/bin/gdbus".into())
            })
            .unwrap();
            let args = command.get_args().collect::<Vec<_>>();
            assert_eq!(args[7], method);
            assert_eq!(args[8], "['file:///work/a%20%27%24%28file%29%25%C3%A9']");
            assert_eq!(args[9], "");
        }
        let command = path_launch_for("linux", path, PathAction::Open, |binary| {
            assert_eq!(binary, "xdg-open");
            Ok("/usr/bin/xdg-open".into())
        })
        .unwrap();
        assert_eq!(command.get_args().collect::<Vec<_>>(), [path.as_os_str()]);
        for action in [PathAction::Select, PathAction::ShowFolder] {
            assert!(
                path_launch_for("linux", path, action, |_| Err("missing gdbus".into())).is_err()
            );
        }
    }

    #[test]
    fn macos_uses_open_without_resolving_a_shell_and_keeps_the_path_as_one_argument() {
        let command = directory_launch_for("macos", Path::new("/work/project '$(name)"), || {
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
        let command = directory_launch_for("linux", Path::new("/work/app"), || {
            Ok("/custom/bin/xdg-open".to_string())
        })
        .unwrap();
        assert_eq!(command.get_program(), "/custom/bin/xdg-open");
        assert_eq!(command.get_args().collect::<Vec<_>>(), ["/work/app"]);
        for detail in ["xdg-open was not found", "the login shell did not finish"] {
            assert_eq!(
                directory_launch_for("linux", Path::new("/work/app"), || Err(detail.to_string()))
                    .unwrap_err(),
                detail
            );
        }
        assert!(
            directory_launch_for("freebsd", Path::new("/work/app"), || panic!("no lookup"))
                .is_err()
        );
    }

    #[test]
    fn windows_reveals_through_explorer_and_keeps_the_path_as_one_argument() {
        let path = Path::new(r"C:\work\a b,c\file.txt");
        let select =
            path_launch_for("windows", path, PathAction::Select, |_| panic!("no lookup")).unwrap();
        assert_eq!(select.get_program(), "explorer.exe");
        assert_eq!(
            select.get_args().collect::<Vec<_>>(),
            [r#"/select,"C:\work\a b,c\file.txt""#]
        );
        let folder = path_launch_for("windows", path, PathAction::ShowFolder, |_| {
            panic!("no lookup")
        })
        .unwrap();
        assert_eq!(folder.get_program(), "explorer.exe");
        assert_eq!(folder.get_args().collect::<Vec<_>>(), [path.as_os_str()]);
        let directory = directory_launch_for("windows", path, || panic!("no lookup")).unwrap();
        assert_eq!(directory.get_program(), "explorer.exe");
        for action in [PathAction::Open, PathAction::Select, PathAction::ShowFolder] {
            assert!(is_detached("windows", action));
        }
        assert!(is_detached("linux", PathAction::Open));
        assert!(!is_detached("linux", PathAction::Select));
        assert!(!is_detached("macos", PathAction::Open));
    }

    #[test]
    fn explorer_is_asked_to_select_with_the_switch_bare_and_the_path_quoted() {
        let select = |path: &str| explorer_select_argument(Path::new(path));
        assert_eq!(
            select(r"C:\Users\测试用户\Documents\spaces, commas.txt").unwrap(),
            r#"/select,"C:\Users\测试用户\Documents\spaces, commas.txt""#
        );
        assert_eq!(
            select(r"C:\plain.txt").unwrap(),
            r#"/select,"C:\plain.txt""#
        );
        // The root's backslash is doubled, so the closing quote stays a quote.
        assert_eq!(select(r"C:\").unwrap(), r#"/select,"C:\\""#);
        assert_eq!(
            select(r"\\server\share\").unwrap(),
            r#"/select,"\\server\share\\""#
        );
        assert!(select(r#"C:\a"b.txt"#).is_none());
        assert!(path_launch_for(
            "windows",
            Path::new(r#"C:\a"b.txt"#),
            PathAction::Select,
            |_| panic!("no lookup")
        )
        .is_err());
    }

    #[cfg(windows)]
    #[test]
    fn the_shell_refuses_a_path_that_is_not_there() {
        let missing = std::env::temp_dir().join("octoboard-no-such-file.txt");
        assert!(shell_open(&missing).unwrap().is_err());
    }

    #[test]
    fn a_shell_call_that_does_not_answer_in_time_is_taken_as_handed_over() {
        let quick = std::time::Duration::from_millis(50);
        assert_eq!(answered_within(quick, || Ok(())), Ok(()));
        assert_eq!(
            answered_within(crate::test_support::PATIENCE, || Err("refused".into())),
            Err("refused".to_string())
        );
        let (release, held) = std::sync::mpsc::channel::<()>();
        let started = std::time::Instant::now();
        assert_eq!(
            answered_within(quick, move || {
                let _ = held.recv();
                Err("answered too late to matter".into())
            }),
            Ok(())
        );
        assert!(started.elapsed() < crate::test_support::PATIENCE);
        drop(release);
        assert!(answered_within(quick, || panic!("the call failed")).is_err());
    }

    #[cfg(not(windows))]
    #[test]
    fn only_windows_has_a_shell_call_to_open_with() {
        assert!(shell_open(Path::new("/work/app")).is_none());
    }

    #[test]
    fn the_text_editor_launch_is_macos_open_with_t() {
        let path = Path::new("/work/-.env.secret");
        let command = text_editor_launch("macos", path).unwrap();
        assert_eq!(command.get_program(), "/usr/bin/open");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                std::ffi::OsStr::new("-t"),
                std::ffi::OsStr::new("--"),
                path.as_os_str()
            ]
        );
        assert!(text_editor_launch("linux", path).is_none());
    }

    #[test]
    fn a_folder_without_an_extension_is_not_asked_about_and_a_tree_is_read_for_a_package() {
        assert!(package_probe("macos", Path::new("/work/folder")).is_none());
        assert!(package_probe("macos", Path::new("/work/Foo.xcodeproj")).is_some());
        assert!(package_probe("linux", Path::new("/work/Foo.xcodeproj")).is_none());
        assert!(package_tree_is_package(
            "(\n    \"com.apple.package\",\n    \"public.directory\"\n)"
        ));
        assert!(package_tree_is_package("(null)"));
        assert!(!package_tree_is_package("(\n    \"public.directory\"\n)"));
    }
}
