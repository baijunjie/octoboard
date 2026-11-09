//! The files a copy of the daemon's build holds beside its executable, which differ by host, and
//! the removal of those the Windows installer moved aside while a process held them.

use std::path::Path;

/// The files, relative to the executable's directory, that the executable needs beside it where
/// they are shipped: on Windows the bundled `conpty.dll` and the `OpenConsole.exe` it starts
/// (which it looks for in a subdirectory named after the machine's architecture). A file that is
/// not there is not part of the build. None where the executable stands alone.
pub fn companions() -> &'static [&'static str] {
    if cfg!(windows) {
        &["conpty.dll", "x64/OpenConsole.exe", "arm64/OpenConsole.exe"]
    } else {
        &[]
    }
}

/// What the Windows installer puts between a file's name and a number when it moves aside a file
/// of the installation that a running process holds, so that it can write the new one in its place.
const SET_ASIDE: &str = ".old-";

/// Removes, on Windows, the files the installer moved aside from the executable `exe` and from its
/// companions (see [`remove_set_aside`]); does nothing elsewhere, where no installer moves any.
pub fn remove_installer_set_aside(exe: &Path) {
    if cfg!(windows) {
        remove_set_aside(exe, companions());
    }
}

/// Removes the files the installer moved aside, `<file>.old-<n>` (see [`SET_ASIDE`]), of the
/// executable `exe` and of its `companions` (paths beside it), from the directory each of them is
/// in. One that cannot be removed, such as one a process that kept running through the update
/// still holds, is left for a later start. Nothing else beside the executable is touched.
fn remove_set_aside(exe: &Path, companions: &[&str]) {
    let Some(dir) = exe.parent() else {
        return;
    };
    let shipped = std::iter::once(exe.to_path_buf()).chain(companions.iter().map(|c| dir.join(c)));
    for file in shipped {
        let (Some(parent), Some(name)) = (file.parent(), file.file_name()) else {
            continue;
        };
        let prefix = format!("{}{SET_ASIDE}", name.to_string_lossy());
        let Ok(entries) = std::fs::read_dir(parent) else {
            continue;
        };
        for entry in entries.flatten() {
            let entry_name = entry.file_name();
            let set_aside = entry_name
                .to_string_lossy()
                .strip_prefix(&prefix)
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
            if !set_aside || !entry.file_type().is_ok_and(|kind| kind.is_file()) {
                continue;
            }
            match std::fs::remove_file(entry.path()) {
                Ok(()) => {
                    tracing::info!(file = %entry.path().display(), "removed a file the installer moved aside")
                }
                Err(err) => {
                    tracing::debug!(file = %entry.path().display(), %err, "a file the installer moved aside could not be removed; it is left for a later start")
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;

    const COMPANIONS: &[&str] = &["conpty.dll", "x64/OpenConsole.exe"];

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"held").unwrap();
    }

    #[test]
    fn what_was_moved_aside_for_the_executable_and_its_companions_is_removed() {
        let dir = ScratchDir::new("set-aside-removed");
        let moved = [
            "octoboardd.exe.old-1",
            "octoboardd.exe.old-2",
            "octoboardd.exe.old-10",
            "conpty.dll.old-1",
            "x64/OpenConsole.exe.old-1",
            "x64/OpenConsole.exe.old-37",
        ];
        for name in moved {
            touch(&dir.join(name));
        }
        remove_set_aside(&dir.join("octoboardd.exe"), COMPANIONS);
        for name in moved {
            assert!(!dir.join(name).exists(), "{name} should have been removed");
        }
    }

    #[test]
    fn the_files_themselves_are_left_alone() {
        let dir = ScratchDir::new("set-aside-shipped");
        let shipped = ["octoboardd.exe", "conpty.dll", "x64/OpenConsole.exe"];
        for name in shipped {
            touch(&dir.join(name));
        }
        touch(&dir.join("octoboardd.exe.old-1"));
        remove_set_aside(&dir.join("octoboardd.exe"), COMPANIONS);
        for name in shipped {
            assert!(dir.join(name).is_file(), "{name} should be untouched");
        }
        assert!(!dir.join("octoboardd.exe.old-1").exists());
    }

    #[test]
    fn only_a_name_of_the_file_and_the_mark_and_digits_alone_counts() {
        let dir = ScratchDir::new("set-aside-names");
        let others = [
            "octoboardd.exe.old-",
            "octoboardd.exe.old-1x",
            "octoboardd.exe.old-x1",
            "octoboardd.exe.old-1.bak",
            "octoboardd.exe.old-1-2",
            "octoboardd.exe.old1",
            "octoboardd.exe.old",
            "octoboardd.exe.bak",
            "octoboardd.exe.OLD-1",
            "octoboardd.exe1.old-1",
            "xoctoboardd.exe.old-1",
            "other.exe.old-1",
            "octoboardd.old-1",
            "conpty.dll.bak",
            "conpty.dll.old-1x",
            "conpty.dll.old-",
            "x64/OpenConsole.exe.old-",
            "x64/OpenConsole.exe.old-a",
            "x64/other.exe.old-1",
        ];
        for name in others {
            touch(&dir.join(name));
        }
        remove_set_aside(&dir.join("octoboardd.exe"), COMPANIONS);
        for name in others {
            assert!(dir.join(name).is_file(), "{name} should be untouched");
        }
    }

    #[test]
    fn a_directory_with_a_matching_name_is_not_removed() {
        let dir = ScratchDir::new("set-aside-directory");
        let folder = dir.join("octoboardd.exe.old-1");
        std::fs::create_dir_all(&folder).unwrap();
        touch(&folder.join("inside.txt"));
        remove_set_aside(&dir.join("octoboardd.exe"), COMPANIONS);
        assert!(folder.is_dir());
        assert!(folder.join("inside.txt").is_file());
    }

    #[test]
    fn a_companion_that_is_not_passed_in_is_not_cleaned() {
        let dir = ScratchDir::new("set-aside-companions");
        touch(&dir.join("conpty.dll.old-1"));
        touch(&dir.join("x64/OpenConsole.exe.old-1"));
        touch(&dir.join("octoboardd.exe.old-1"));
        remove_set_aside(&dir.join("octoboardd.exe"), &["conpty.dll"]);
        assert!(!dir.join("conpty.dll.old-1").exists());
        assert!(!dir.join("octoboardd.exe.old-1").exists());
        assert!(dir.join("x64/OpenConsole.exe.old-1").is_file());

        remove_set_aside(&dir.join("octoboardd.exe"), &[]);
        assert!(dir.join("x64/OpenConsole.exe.old-1").is_file());
    }

    #[test]
    fn only_the_same_directory_as_each_file_is_searched() {
        let dir = ScratchDir::new("set-aside-directories");
        touch(&dir.join("sub/octoboardd.exe.old-1"));
        touch(&dir.join("x64/octoboardd.exe.old-1"));
        touch(&dir.join("arm64/OpenConsole.exe.old-1"));
        touch(&dir.join("x64/OpenConsole.exe.old-1"));
        remove_set_aside(&dir.join("octoboardd.exe"), COMPANIONS);
        assert!(dir.join("sub/octoboardd.exe.old-1").is_file());
        assert!(dir.join("x64/octoboardd.exe.old-1").is_file());
        assert!(dir.join("arm64/OpenConsole.exe.old-1").is_file());
        assert!(!dir.join("x64/OpenConsole.exe.old-1").exists());
    }

    #[test]
    fn a_missing_directory_or_executable_is_not_an_error() {
        let dir = ScratchDir::new("set-aside-missing");
        touch(&dir.join("octoboardd.exe.old-1"));
        // No companion directory, and no executable file either: only the entries count.
        remove_set_aside(&dir.join("octoboardd.exe"), COMPANIONS);
        assert!(!dir.join("octoboardd.exe.old-1").exists());

        remove_set_aside(&dir.join("nowhere").join("octoboardd.exe"), COMPANIONS);
        remove_set_aside(Path::new(""), COMPANIONS);
        remove_set_aside(Path::new("/"), COMPANIONS);
    }

    #[cfg(unix)]
    #[test]
    fn a_file_that_cannot_be_removed_is_left_without_error_and_the_others_are_still_removed() {
        use std::os::unix::fs::PermissionsExt;

        let dir = ScratchDir::new("set-aside-held");
        // The executable's entry is processed first, then the companions in order: the first
        // companion's entry is the one held, and the entries before and after it must still go.
        let companions = ["x64/OpenConsole.exe", "conpty.dll"];
        touch(&dir.join("octoboardd.exe.old-1"));
        touch(&dir.join("x64/OpenConsole.exe.old-1"));
        touch(&dir.join("conpty.dll.old-1"));
        // A directory nobody may write to cannot have an entry removed, as a held file cannot.
        let locked = dir.join("x64");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
        let unlock = || {
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        // Permissions do not stop the superuser: return when nothing refuses the write.
        let probe = locked.join("probe");
        if std::fs::write(&probe, b"").is_ok() {
            unlock();
            return;
        }
        remove_set_aside(&dir.join("octoboardd.exe"), &companions);
        unlock();
        assert!(
            dir.join("x64/OpenConsole.exe.old-1").is_file(),
            "the held file stays"
        );
        assert!(!dir.join("octoboardd.exe.old-1").exists());
        assert!(
            !dir.join("conpty.dll.old-1").exists(),
            "the file after the held one is removed"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn the_installer_removal_leaves_everything_where_no_installer_moves_files_aside() {
        let dir = ScratchDir::new("installer-removal-elsewhere");
        touch(&dir.join("octoboardd.exe.old-1"));
        remove_installer_set_aside(&dir.join("octoboardd.exe"));
        assert!(dir.join("octoboardd.exe.old-1").is_file());
    }

    #[cfg(windows)]
    #[test]
    fn the_installer_removal_removes_what_was_moved_aside_for_the_executable_and_its_companions() {
        let dir = ScratchDir::new("installer-removal-windows");
        let moved = [
            "octoboardd.exe.old-1",
            "conpty.dll.old-1",
            "x64/OpenConsole.exe.old-1",
            "arm64/OpenConsole.exe.old-2",
        ];
        for name in moved {
            touch(&dir.join(name));
        }
        touch(&dir.join("octoboardd.exe"));
        remove_installer_set_aside(&dir.join("octoboardd.exe"));
        for name in moved {
            assert!(!dir.join(name).exists(), "{name} should have been removed");
        }
        assert!(dir.join("octoboardd.exe").is_file());
    }

    #[test]
    fn the_installer_hook_names_what_it_moves_aside_the_same_way() {
        let hooks = include_str!("../../../desktop/src-tauri/windows/installer-hooks.nsh");
        let mark = format!("!define OCTOBOARD_SET_ASIDE \"{SET_ASIDE}\"");
        assert!(
            hooks.contains(&mark),
            "the installer hooks must define the mark as {SET_ASIDE:?}"
        );
    }
}
