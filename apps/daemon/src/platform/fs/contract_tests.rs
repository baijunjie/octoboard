//! What every host's file system layer must do, written once against the neutral signatures.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::*;
use crate::test_support::ScratchDir;

/// The environment variable that turns [`a_child_process_holds_the_lock`] from a no-op into the
/// holder the lock test starts.
const HOLD_LOCK: &str = "OCTOBOARD_TEST_HOLD_LOCK";

fn make_alias(target: &Path, at: &Path) {
    alias_dir(target, at).expect("alias");
}

/// A directory reached through a link, and by its own path, is one identity; another directory is
/// another; the link itself, looked at without following, is neither.
#[test]
fn identity_is_the_same_across_spellings_of_one_directory() {
    let dir = ScratchDir::new("fs-identity-link");
    let real = dir.join("real");
    std::fs::create_dir(&real).unwrap();
    std::fs::create_dir(dir.join("other")).unwrap();
    make_alias(&real, &dir.join("link"));

    let of = |path: &Path| identity(path, Links::Follow).unwrap();
    assert_eq!(of(&real), of(&dir.join("link")));
    assert_eq!(of(&real), of(&dir.join("./link/../real/")));
    assert_ne!(of(&real), of(&dir.join("other")));
    assert_ne!(identity(&dir.join("link"), Links::Keep).unwrap(), of(&real));
    assert!(of(&real).is_known());
    assert_eq!(of(&real).token(), of(&dir.join("link")).token());
    assert!(identity(&dir.join("missing"), Links::Follow).is_err());
}

/// A case-only respelling is the same file where the file system folds case; where it does not,
/// there is nothing to compare.
#[test]
fn identity_is_the_same_across_a_case_respelling_where_the_file_system_folds_case() {
    let dir = ScratchDir::new("fs-identity-case");
    let real = dir.join("Project");
    std::fs::create_dir(&real).unwrap();
    let respelled = dir.join("pROJECT");
    if !respelled.exists() {
        eprintln!("no case-only respelling: the file system tells case apart");
        return;
    }
    assert_eq!(
        identity(&real, Links::Follow).unwrap(),
        identity(&respelled, Links::Follow).unwrap()
    );
}

/// The identity of an open file is the file's own, whatever path it was opened by.
#[test]
fn identity_of_an_open_file_matches_the_identity_of_its_path() {
    let dir = ScratchDir::new("fs-identity-open");
    let path = dir.join("file");
    std::fs::write(&path, "x").unwrap();
    let file = File::open(&path).unwrap();
    assert_eq!(
        stat_of(&file).unwrap().identity,
        identity(&path, Links::Follow).unwrap()
    );
}

/// A link in a middle component is refused, as is one at the end, and a link swapped in after the
/// path was looked at; a path with no link opens.
#[test]
fn open_without_links_refuses_a_link_anywhere_in_the_path() {
    let dir = ScratchDir::new("fs-open-links");
    // The scope is canonical, as the browse scopes are: a link above it would be refused too.
    let dir = crate::platform::names::canonical(&dir).unwrap();
    let scope = dir.join("scope");
    let outside = dir.join("outside");
    std::fs::create_dir_all(scope.join("sub")).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(scope.join("sub/file"), "inside").unwrap();
    std::fs::write(outside.join("file"), "outside").unwrap();

    open_without_links(&scope, Path::new("sub/file"), Opens::File).expect("no link");
    open_without_links(&scope, Path::new("sub"), Opens::Directory).expect("a directory");
    open_without_links(&scope, Path::new(""), Opens::Directory).expect("the scope itself");

    make_alias(&outside, &scope.join("through"));
    let err = open_without_links(&scope, Path::new("through/file"), Opens::File).unwrap_err();
    assert!(is_symlink_loop(&err), "{err}");

    alias_file(&outside.join("file"), &scope.join("last")).unwrap();
    let err = open_without_links(&scope, Path::new("last"), Opens::File).unwrap_err();
    assert!(is_symlink_loop(&err), "{err}");

    // The path was fine when looked at; a link takes the place of `sub` before the open.
    assert!(!std::fs::symlink_metadata(scope.join("sub/file"))
        .unwrap()
        .file_type()
        .is_symlink());
    std::fs::rename(scope.join("sub"), dir.join("moved")).unwrap();
    make_alias(&outside, &scope.join("sub"));
    let err = open_without_links(&scope, Path::new("sub/file"), Opens::File).unwrap_err();
    assert!(is_symlink_loop(&err), "{err}");
}

/// A FIFO is refused rather than waited for, whatever the caller meant to read.
#[cfg(unix)]
#[test]
fn open_regular_refuses_a_fifo_without_waiting() {
    let dir = ScratchDir::new("fs-open-regular");
    let fifo = dir.join("fifo");
    let name = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
    // SAFETY: a valid NUL-terminated path.
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    assert!(open_regular(&fifo).is_err());
    assert!(open_regular_to_append(&fifo).is_err());
    std::fs::write(dir.join("file"), "x").unwrap();
    open_regular(&dir.join("file")).unwrap();
}

/// A directory is refused as no regular file, not as an access the user lacks.
#[test]
fn open_regular_refuses_a_directory_as_no_regular_file() {
    let dir = ScratchDir::new("fs-open-regular-directory");
    let err = open_regular(&dir).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput, "{err}");
}

/// What is created private is not readable or writable by group or others, even over a file that
/// was open to them; the mode stands in for another principal.
#[cfg(unix)]
#[test]
fn private_files_are_closed_to_other_principals() {
    use std::os::unix::fs::PermissionsExt;
    let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    let dir = ScratchDir::new("fs-private");

    let created = dir.join("created");
    drop(create_private(&created, OpenOptions::new().write(true).create(true)).expect("created"));
    assert_eq!(mode(&created) & 0o077, 0);

    let replaced = dir.join("replaced");
    std::fs::write(&replaced, "x").unwrap();
    std::fs::set_permissions(&replaced, std::fs::Permissions::from_mode(0o666)).unwrap();
    drop(create_replacement(&replaced, &replaced, Privacy::Private).expect("replacement"));
    assert_eq!(mode(&replaced) & 0o077, 0);

    let kept = dir.join("kept");
    std::fs::write(&kept, "x").unwrap();
    std::fs::set_permissions(&kept, std::fs::Permissions::from_mode(0o640)).unwrap();
    drop(create_replacement(&kept, &kept, Privacy::KeptOrPrivate).expect("kept"));
    assert_eq!(mode(&kept), 0o640);
    let fresh = dir.join("fresh");
    drop(create_replacement(&fresh, &fresh, Privacy::KeptOrPrivate).expect("fresh"));
    assert_eq!(mode(&fresh) & 0o077, 0);

    let owned = dir.join("owned");
    std::fs::create_dir(&owned).unwrap();
    restrict_to_owner(&owned).unwrap();
    assert_eq!(mode(&owned) & 0o077, 0);

    let nested = dir.join("made").join("nested");
    create_private_dir(&nested).unwrap();
    assert_eq!(mode(&nested) & 0o077, 0);
    assert_eq!(mode(&dir.join("made")) & 0o077, 0);
    create_private_dir(&nested).unwrap();
}

/// What the user's process made in a directory of its own is private to the user; the directory
/// made writable by everyone is not.
#[cfg(unix)]
#[test]
fn a_directory_open_to_others_is_not_private() {
    use std::os::unix::fs::PermissionsExt;
    let dir = ScratchDir::new("fs-private-to-user");
    let shared = dir.join("shared");
    std::fs::create_dir(&shared).unwrap();
    std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(is_private_to_user(&shared));
    std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o777)).unwrap();
    assert!(!is_private_to_user(&shared));
    assert!(!is_private_to_user(&dir.join("missing")));
}

/// A directory the user's process made, or restricted to the user, is private to the user; one that
/// grants everyone write access is not, and neither is what is not there.
#[cfg(windows)]
#[test]
fn a_directory_open_to_others_is_not_private_on_windows() {
    let dir = ScratchDir::new("fs-private-to-user");
    let shared = dir.join("shared");
    std::fs::create_dir(&shared).unwrap();
    restrict_to_owner(&shared).unwrap();
    assert!(is_private_to_user(&shared));
    let granted = Command::new("icacls")
        .arg(&shared)
        .args(["/grant", "*S-1-1-0:(M)"])
        .output()
        .unwrap();
    assert!(granted.status.success(), "{granted:?}");
    assert!(!is_private_to_user(&shared));
    assert!(!is_private_to_user(&dir.join("missing")));
}

/// A published file or directory is read-only, and writable again once it is said to be; only a
/// regular file with execute permission is executable.
#[cfg(unix)]
#[test]
fn read_only_and_executable_follow_what_was_set() {
    use std::os::unix::fs::PermissionsExt;
    let dir = ScratchDir::new("fs-read-only");
    let file = dir.join("script");
    std::fs::write(&file, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(!is_executable(&file));
    set_read_only(&file, true).unwrap();
    assert!(is_executable(&file));
    assert!(std::fs::metadata(&file).unwrap().permissions().readonly());
    set_read_only(&file, false).unwrap();
    assert!(!std::fs::metadata(&file).unwrap().permissions().readonly());
    assert!(!is_executable(&dir));
    assert!(!is_executable(&dir.join("missing")));
}

/// A directory is the root of a volume where it has no parent; one that cannot be read counts as
/// one (unknown never destroys), and an ordinary directory is not one.
#[test]
fn an_unreadable_directory_counts_as_a_volume_root() {
    let dir = ScratchDir::new("fs-volume-root");
    assert!(is_volume_root(&dir.join("missing")));
    assert!(!is_volume_root(&dir));
    assert!(is_volume_root(Path::new(std::path::MAIN_SEPARATOR_STR)));
}

/// An alias leads where it was made to, relative targets stay as written, and something that is
/// no alias reads as none. A host that cannot make a file link says so, and refuses to make one.
#[test]
fn an_alias_reads_back_as_it_was_made() {
    let dir = ScratchDir::new("fs-alias");
    std::fs::write(dir.join("target"), "x").unwrap();
    std::fs::create_dir(dir.join("directory")).unwrap();
    let linked_dir = dir.join("linked-dir");
    alias_dir(&dir.join("directory"), &linked_dir).unwrap();
    assert_eq!(
        read_alias(&linked_dir).unwrap(),
        Some(dir.join("directory"))
    );
    assert!(linked_dir.join(".").is_dir());
    let at = dir.join("at");
    if !file_links_available() {
        let err = alias_file(Path::new("target"), &at).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::Unsupported);
        return;
    }
    alias_file(Path::new("target"), &at).unwrap();
    assert_eq!(read_alias(&at).unwrap(), Some(PathBuf::from("target")));
    assert_eq!(std::fs::read_to_string(&at).unwrap(), "x");
    assert_eq!(read_alias(&dir.join("target")).unwrap(), None);
    assert!(read_alias(&dir.join("missing")).is_err());
    assert!(alias_file(Path::new("target"), &at).is_err());
}

/// A link is removed as itself whatever it leads to, what it leads to is kept, and a directory of
/// its own is refused.
#[test]
fn a_link_is_removed_as_itself_and_a_directory_is_refused() {
    let dir = ScratchDir::new("fs-remove-link");
    std::fs::create_dir(dir.join("directory")).unwrap();
    std::fs::write(dir.join("directory").join("kept"), "x").unwrap();
    std::fs::write(dir.join("file"), "x").unwrap();
    alias_dir(&dir.join("directory"), &dir.join("linked-dir")).unwrap();
    remove_file_or_link(&dir.join("linked-dir")).unwrap();
    assert!(std::fs::symlink_metadata(dir.join("linked-dir")).is_err());
    assert!(dir.join("directory").join("kept").is_file());
    if file_links_available() {
        alias_file(&dir.join("file"), &dir.join("linked-file")).unwrap();
        remove_file_or_link(&dir.join("linked-file")).unwrap();
        assert!(std::fs::symlink_metadata(dir.join("linked-file")).is_err());
    }
    remove_file_or_link(&dir.join("file")).unwrap();
    assert!(!dir.join("file").exists());
    assert!(remove_file_or_link(&dir.join("directory")).is_err());
    assert!(dir.join("directory").join("kept").is_file());
}

/// A second take of a held lock is refused, and the lock is free once its holder lets go.
#[test]
fn the_lock_is_refused_while_held_and_free_once_released() {
    let dir = ScratchDir::new("fs-lock-same-process");
    let path = dir.join("daemon.lock");
    let held = try_lock_file(&path).expect("first");
    assert!(matches!(try_lock_file(&path), Err(Busy::Held)));
    drop(held);
    // A process another test is starting may hold the descriptor between the fork and the exec
    // that closes it, which a stalled `exec` can make last seconds.
    let free = crate::session::wait::wait_for(crate::test_support::PATIENCE, || {
        try_lock_file(&path).is_ok()
    });
    assert!(free, "the lock was not free once released");
}

/// Not a test of its own: the process the lock test starts takes the lock and holds it until its
/// standard input closes.
#[test]
fn a_child_process_holds_the_lock() {
    let Some(path) = std::env::var_os(HOLD_LOCK) else {
        return;
    };
    let _held = try_lock_file(Path::new(&path)).expect("the child takes the lock");
    println!("locked");
    let mut line = String::new();
    while std::io::stdin()
        .read_line(&mut line)
        .is_ok_and(|read| read > 0)
    {}
}

/// The lock a process holds is refused to another process, and given up with the process.
#[test]
fn the_lock_is_refused_to_another_process_while_it_holds_it() {
    let dir = ScratchDir::new("fs-lock-processes");
    let path = dir.join("daemon.lock");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "platform::fs::contract_tests::a_child_process_holds_the_lock",
            "--nocapture",
        ])
        .env(HOLD_LOCK, &path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("the test binary starts");
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let locked = lines
        .by_ref()
        .any(|line| line.is_ok_and(|line| line == "locked"));
    assert!(locked, "the child never reported the lock");

    assert!(matches!(try_lock_file(&path), Err(Busy::Held)));

    drop(child.stdin.take());
    assert!(child.wait().unwrap().success());
    try_lock_file(&path).expect("released with the holder");
}

/// A listing read through the directory's handle names every entry with its kind, and a regular
/// file's entry carries the version a stat of the file reports.
#[test]
fn a_listing_tells_kinds_and_versions_as_a_stat_does() {
    let dir = ScratchDir::new("fs-listing");
    let dir = crate::platform::names::canonical(&dir).unwrap();
    std::fs::write(dir.join("file"), "contents").unwrap();
    std::fs::create_dir(dir.join("sub")).unwrap();
    make_alias(&dir.join("sub"), &dir.join("link"));

    let directory = open_without_links(&dir, Path::new(""), Opens::Directory).unwrap();
    let mut reader = DirReader::new(directory).unwrap();
    let mut seen = Vec::new();
    while let Some(listed) = reader.next_entry().unwrap() {
        let stat = reader.stat(&listed).expect("a listed entry has a stat");
        if listed.name == b"file" {
            assert_eq!(stat.size, 8);
            let file = stat_of(&File::open(dir.join("file")).unwrap()).unwrap();
            assert_eq!(stat.version(), version(&file));
        }
        seen.push((String::from_utf8(listed.name).unwrap(), stat.kind));
    }
    seen.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        seen,
        [
            ("file".to_string(), EntryType::File),
            ("link".to_string(), EntryType::Link),
            ("sub".to_string(), EntryType::Directory),
        ]
    );
}

/// A file's version moves when the file is written, and stays while it is not. The write changes
/// the size, since a host's file times may not tick between two writes in a row.
#[test]
fn a_version_moves_with_a_write() {
    let dir = ScratchDir::new("fs-version");
    let path = dir.join("file");
    std::fs::write(&path, "one").unwrap();
    let before = version(&stat(&path, Links::Keep).unwrap());
    assert_eq!(before, version(&stat(&path, Links::Keep).unwrap()));
    std::fs::write(&path, "three").unwrap();
    assert_ne!(before, version(&stat(&path, Links::Keep).unwrap()));
}

/// Windows runs a file by its extension, and the read-only attribute is what `set_read_only`
/// sets.
#[cfg(windows)]
#[test]
fn executable_is_the_extension_and_read_only_is_the_attribute() {
    let dir = ScratchDir::new("fs-windows-executable");
    let script = dir.join("hook.CMD");
    std::fs::write(&script, "@exit /b 0\r\n").unwrap();
    std::fs::write(dir.join("notes.txt"), "x").unwrap();
    assert!(is_executable(&script));
    assert!(!is_executable(&dir.join("notes.txt")));
    assert!(!is_executable(&dir.join("missing.exe")));
    std::fs::create_dir(dir.join("folder.exe")).unwrap();
    assert!(!is_executable(&dir.join("folder.exe")));

    set_read_only(&script, true).unwrap();
    assert!(std::fs::metadata(&script).unwrap().permissions().readonly());
    set_read_only(&script, false).unwrap();
    assert!(!std::fs::metadata(&script).unwrap().permissions().readonly());
}

/// A drive's root is a volume root; a folder on it is not.
#[cfg(windows)]
#[test]
fn a_drive_root_is_a_volume_root() {
    let dir = ScratchDir::new("fs-windows-volume-root");
    let root: PathBuf = dir.ancestors().last().unwrap().to_path_buf();
    assert!(is_volume_root(&root), "{root:?}");
    assert!(!is_volume_root(&dir));
}

/// A directory restricted to its owner has a protected list of two entries, the user and the
/// system, which what is created in it inherits.
#[cfg(windows)]
#[test]
fn a_restricted_directory_is_the_users_and_the_systems_alone() {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT};
    use windows_sys::Win32::Security::{
        AclSizeInformation, GetAclInformation, GetSecurityDescriptorControl, ACL,
        ACL_SIZE_INFORMATION, DACL_SECURITY_INFORMATION, SE_DACL_PROTECTED,
    };

    /// The number of entries in the list of `path`, and whether the list is protected.
    fn entries_of(path: &Path) -> (u32, bool) {
        let name: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        let mut acl: *mut ACL = std::ptr::null_mut();
        let mut descriptor = std::ptr::null_mut();
        // SAFETY: the name is NUL-terminated; the outputs are written by the call and the
        // descriptor, which holds the list, is freed below.
        let status = unsafe {
            GetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut acl,
                std::ptr::null_mut(),
                &mut descriptor,
            )
        };
        assert_eq!(status, 0, "reading the list of {path:?}");
        let mut size = ACL_SIZE_INFORMATION::default();
        let (mut control, mut revision) = (0u16, 0u32);
        // SAFETY: the list and descriptor are alive until the free below.
        unsafe {
            assert_ne!(
                GetAclInformation(
                    acl,
                    (&mut size as *mut ACL_SIZE_INFORMATION).cast(),
                    std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
                    AclSizeInformation,
                ),
                0
            );
            assert_ne!(
                GetSecurityDescriptorControl(descriptor, &mut control, &mut revision),
                0
            );
            LocalFree(descriptor);
        }
        (size.AceCount, control & SE_DACL_PROTECTED != 0)
    }

    let dir = ScratchDir::new("fs-windows-private");
    let owned = dir.join("owned");
    std::fs::create_dir(&owned).unwrap();
    std::fs::write(owned.join("before"), "x").unwrap();
    restrict_to_owner(&owned).unwrap();
    std::fs::write(owned.join("after"), "x").unwrap();
    assert_eq!(entries_of(&owned), (2, true));
    for inside in ["before", "after"] {
        assert_eq!(entries_of(&owned.join(inside)), (2, false), "{inside}");
    }
}
