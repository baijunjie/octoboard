//! The macOS application: launched through LaunchServices (`open`), which keeps it running after
//! the terminal closes, and asked to quit by `SIGUSR1`.

use std::path::Path;
use std::process::Command;

use crate::platform::proc::{self, ProcessId, ProcessIdentity, Sent};
use crate::platform::proc_table;

/// The argument the application is started with to stay in the background: no window, the menu
/// bar icon alone. The shell's own name for it (`apps/desktop/src-tauri/src/background.rs`).
const BACKGROUND_ARGUMENT: &str = "--start-in-background";

/// The application bundle whose `Contents/MacOS/` holds `exe`.
fn bundle_of(exe: &Path) -> Option<&Path> {
    exe.parent()?.parent()?.parent()
}

pub fn launch_in_background(exe: &Path) -> std::io::Result<()> {
    let bundle = bundle_of(exe).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "this executable is not inside an application bundle",
        )
    })?;
    let status = Command::new("/usr/bin/open")
        .arg("-g")
        .arg(bundle)
        .args(["--args", BACKGROUND_ARGUMENT])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!("open ended with {status}")))
    }
}

pub fn of_daemon(daemon: &ProcessIdentity) -> Option<ProcessIdentity> {
    let parent = proc_table::list()?
        .into_iter()
        .find(|process| process.pid == daemon.pid.0 as i32)?
        .ppid;
    // Launchd adopts a daemon whose application is gone.
    let start = (parent > 1)
        .then(|| proc_table::start_of(parent))
        .flatten()?;
    Some(ProcessIdentity {
        pid: ProcessId(parent as u32),
        start_micros: start,
    })
}

/// Whether `image` is an executable of the application bundle whose `Contents/MacOS/` holds `exe`.
fn is_inside_bundle_of(image: &Path, exe: &Path) -> bool {
    bundle_of(exe).is_some_and(|bundle| image.starts_with(bundle))
}

pub fn runs_bundle_of(app: &ProcessIdentity, exe: &Path) -> bool {
    proc_table::image_of(app.pid.0 as i32).is_some_and(|image| is_inside_bundle_of(&image, exe))
}

pub fn ask_to_quit(app: &ProcessIdentity) -> Sent {
    proc::request_application_quit(app)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundle_is_three_levels_above_the_executable() {
        let exe = Path::new("/Applications/Octoboard.app/Contents/MacOS/octoboardd");
        assert_eq!(
            bundle_of(exe),
            Some(Path::new("/Applications/Octoboard.app"))
        );
    }

    #[test]
    fn an_image_is_the_applications_only_when_it_is_inside_the_bundle() {
        let exe = Path::new("/Applications/Octoboard.app/Contents/MacOS/octoboardd");
        for (image, inside) in [
            ("/Applications/Octoboard.app/Contents/MacOS/Octoboard", true),
            (
                "/Applications/Octoboard.app/Contents/MacOS/octoboardd",
                true,
            ),
            ("/Applications/Other.app/Contents/MacOS/Octoboard", false),
            (
                "/Applications/Octoboard.app.old/Contents/MacOS/Octoboard",
                false,
            ),
            ("/usr/bin/zsh", false),
        ] {
            assert_eq!(
                is_inside_bundle_of(Path::new(image), exe),
                inside,
                "{image}"
            );
        }
    }

    #[test]
    fn a_process_is_asked_about_by_the_path_of_its_image() {
        let me = ProcessIdentity {
            pid: ProcessId(std::process::id()),
            start_micros: proc_table::start_of(std::process::id() as i32).unwrap(),
        };
        let image = proc_table::image_of(me.pid.0 as i32).expect("the test binary's path");
        // An executable two levels below this one's directory has that directory as its bundle.
        let dir = image.parent().unwrap();
        assert!(runs_bundle_of(&me, &dir.join("a/b/octoboardd")));
        assert!(!runs_bundle_of(&me, Path::new("/none/a/b/octoboardd")));
    }

    #[test]
    fn this_process_has_a_parent_that_is_not_launchd() {
        let me = ProcessIdentity {
            pid: ProcessId(std::process::id()),
            start_micros: proc_table::start_of(std::process::id() as i32).unwrap(),
        };
        let parent = of_daemon(&me).expect("the test runner has a parent");
        assert!(parent.is_running());
    }
}
