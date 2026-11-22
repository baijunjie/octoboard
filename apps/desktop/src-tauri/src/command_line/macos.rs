//! The macOS link. It goes to `/usr/local/bin`, made directly where the user may write there and
//! through the system's administrator prompt (`osascript`'s `with administrator privileges`)
//! where not, and to `~/.local/bin` when that is refused or fails, where the user may have to add
//! the directory to the PATH themselves, unless the system's directory already holds a link, which
//! the fallback would hide behind the old one: there the refusal is the answer. A file of that name
//! that is not a link, and a link to anything but an application's `octoboardd`, are never replaced.

use std::io::ErrorKind;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{Error, ErrorCode, State};

/// The tool's name, and the sidecar's.
const NAME: &str = "octoboardd";

/// How the system's administrator prompt ended.
#[derive(Debug, PartialEq, Eq)]
enum Admin {
    Done,
    /// Refused or failed, with what the system said.
    Failed(String),
}

/// Where the link may go, in the order tried.
struct Places {
    system: PathBuf,
    user: PathBuf,
}

impl Places {
    fn current() -> Result<Self, Error> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| Error::failed("the home directory is not known"))?;
        Ok(Self::in_home(&home))
    }

    /// The places for the user whose home is `home`.
    fn in_home(home: &Path) -> Self {
        Self {
            system: PathBuf::from("/usr/local/bin"),
            user: home.join(".local").join("bin"),
        }
    }

    fn link(dir: &Path) -> PathBuf {
        dir.join(NAME)
    }
}

/// The `octoboardd` inside this application's bundle, with every link resolved.
fn bundled() -> Result<PathBuf, Error> {
    let exe = std::env::current_exe().map_err(|err| Error::failed(err.to_string()))?;
    let sidecar = exe
        .parent()
        .ok_or_else(|| Error::failed("the application's directory is not known"))?
        .join(NAME);
    sidecar
        .canonicalize()
        .map_err(|_| Error::failed("this build of the application carries no octoboardd to link"))
}

/// Whether `path` names an `octoboardd` inside an application bundle.
fn is_application_copy(path: &Path) -> bool {
    let mut parents = path.ancestors().skip(1);
    path.file_name() == Some(std::ffi::OsStr::new(NAME))
        && parents.next().and_then(Path::file_name) == Some(std::ffi::OsStr::new("MacOS"))
        && parents.next().and_then(Path::file_name) == Some(std::ffi::OsStr::new("Contents"))
        && parents
            .next()
            .is_some_and(|app| app.extension() == Some(std::ffi::OsStr::new("app")))
}

/// What the link at `link` is, when it is a link to an application's `octoboardd`; `None` for
/// anything else (nothing there, a file, a link to something else), none of which is ours.
/// `bundled` is this application's own, `None` where it cannot be resolved: then no link is its own.
fn read_link_at(link: &Path, bundled: Option<&Path>) -> Option<State> {
    let meta = std::fs::symlink_metadata(link).ok()?;
    if !meta.file_type().is_symlink() {
        return None;
    }
    let path = link.display().to_string();
    match link.canonicalize() {
        Ok(target) if Some(target.as_path()) == bundled => Some(State::Installed {
            path,
            path_note: false,
        }),
        Ok(target) => is_application_copy(&target).then_some(State::OtherCopy { path }),
        Err(_) => {
            let target = std::fs::read_link(link).ok()?;
            let ends_in_a_bundle = target.file_name() == Some(std::ffi::OsStr::new(NAME))
                && target
                    .ancestors()
                    .any(|part| part.extension() == Some(std::ffi::OsStr::new("app")));
            ends_in_a_bundle.then_some(State::Unresolved { path })
        }
    }
}

/// Where the tool stands: the first place that holds a link of the application's.
fn state_in(places: &Places, bundled: Option<&Path>) -> State {
    for dir in [&places.system, &places.user] {
        if let Some(state) = read_link_at(&Places::link(dir), bundled) {
            return match state {
                State::Installed { path, .. } => State::Installed {
                    path,
                    path_note: dir == &places.user,
                },
                other => other,
            };
        }
    }
    State::NotInstalled
}

pub fn status() -> Result<State, Error> {
    Ok(state_in(&Places::current()?, Some(&bundled()?)))
}

pub fn install() -> Result<State, Error> {
    let bundled = bundled()?;
    install_in(&Places::current()?, &bundled, &administrator)
}

pub fn remove() -> Result<State, Error> {
    let places = Places::current()?;
    // Whether this build can resolve its own `octoboardd` or not, a link left to a copy that is
    // gone is still ours to remove.
    let bundled = bundled().ok();
    remove_in(&places, bundled.as_deref(), &administrator)?;
    Ok(state_in(&places, bundled.as_deref()))
}

/// Makes the link in `places`: in the system's directory, else the user's.
fn install_in(
    places: &Places,
    bundled: &Path,
    admin: &dyn Fn(&str) -> Admin,
) -> Result<State, Error> {
    let system_link = Places::link(&places.system);
    let user_link = Places::link(&places.user);
    for link in [&system_link, &user_link] {
        let Ok(meta) = std::fs::symlink_metadata(link) else {
            continue;
        };
        if !meta.file_type().is_symlink() {
            return Err(Error::new(
                ErrorCode::NotALink,
                Some(link.display().to_string()),
                format!("{} exists and is not a link", link.display()),
            ));
        }
        if read_link_at(link, Some(bundled)).is_none() {
            let target = std::fs::read_link(link)
                .map(|target| target.display().to_string())
                .unwrap_or_default();
            return Err(Error::new(
                ErrorCode::ForeignLink,
                Some(link.display().to_string()),
                target,
            ));
        }
    }
    // The old link stays first on the PATH where the system's directory holds it, so a link made
    // in the user's directory instead would never be the one that runs.
    let system_holds_link = read_link_at(&system_link, Some(bundled)).is_some();
    let fall_back = |err: Error| -> Result<(), Error> {
        note(&err.detail);
        if system_holds_link {
            return Err(err);
        }
        link_in_user_directory(places, bundled)
    };
    match link_directly(&system_link, bundled) {
        Ok(()) => {}
        Err(err)
            if matches!(
                err.kind(),
                ErrorKind::PermissionDenied | ErrorKind::NotFound
            ) =>
        {
            let script = format!(
                "mkdir -p {dir} && ln -sfn {target} {link}",
                dir = shell_quote(&places.system),
                target = shell_quote(bundled),
                link = shell_quote(&system_link)
            );
            if let Admin::Failed(said) = admin(&script) {
                fall_back(admin_error(&system_link, said))?;
            }
        }
        Err(err) => fall_back(Error::new(
            ErrorCode::Failed,
            Some(system_link.display().to_string()),
            format!("linking in {} failed: {err}", places.system.display()),
        ))?,
    }
    // A link in the user's directory that an earlier install left, now that the system one holds.
    if read_link_at(&system_link, Some(bundled)).is_some()
        && read_link_at(&user_link, Some(bundled)).is_some()
    {
        let _ = std::fs::remove_file(&user_link);
    }
    Ok(state_in(places, Some(bundled)))
}

fn note(message: &str) {
    crate::diagnostics::diagnostic!("{message}");
}

/// What a failed administrator prompt is: a refusal where the user cancelled it (`osascript` says
/// `User canceled.` with error -128), a failure otherwise; `said` is what the system said.
fn admin_error(link: &Path, said: String) -> Error {
    let code = if said.contains("User canceled") || said.contains("(-128)") {
        ErrorCode::Refused
    } else {
        ErrorCode::Failed
    };
    Error::new(code, Some(link.display().to_string()), said)
}

fn link_in_user_directory(places: &Places, bundled: &Path) -> Result<(), Error> {
    let link = Places::link(&places.user);
    let failed =
        |detail: String| Error::new(ErrorCode::Failed, Some(link.display().to_string()), detail);
    std::fs::create_dir_all(&places.user)
        .map_err(|err| failed(format!("cannot make {}: {err}", places.user.display())))?;
    link_directly(&link, bundled)
        .map_err(|err| failed(format!("cannot link in {}: {err}", places.user.display())))
}

/// Makes `link` point at `target`, replacing a link there: a new link beside it, renamed over it.
fn link_directly(link: &Path, target: &Path) -> std::io::Result<()> {
    let dir = link
        .parent()
        .ok_or_else(|| std::io::Error::new(ErrorKind::InvalidInput, "no directory"))?;
    let temporary = dir.join(format!(".{NAME}.{}", std::process::id()));
    let _ = std::fs::remove_file(&temporary);
    symlink(target, &temporary)?;
    std::fs::rename(&temporary, link).inspect_err(|_| {
        let _ = std::fs::remove_file(&temporary);
    })
}

/// Removes every link of the application's in `places`, through the administrator prompt where the
/// directory is not the user's to write.
fn remove_in(
    places: &Places,
    bundled: Option<&Path>,
    admin: &dyn Fn(&str) -> Admin,
) -> Result<(), Error> {
    for dir in [&places.system, &places.user] {
        let link = Places::link(dir);
        if read_link_at(&link, bundled).is_none() {
            continue;
        }
        match std::fs::remove_file(&link) {
            Ok(()) => {}
            Err(err) if err.kind() == ErrorKind::PermissionDenied => {
                if let Admin::Failed(said) = admin(&format!("rm -f {}", shell_quote(&link))) {
                    return Err(admin_error(&link, said));
                }
            }
            Err(err) => {
                return Err(Error::new(
                    ErrorCode::Failed,
                    Some(link.display().to_string()),
                    format!("cannot remove {}: {err}", link.display()),
                ))
            }
        }
    }
    Ok(())
}

/// `path` as one word of a shell command.
fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', r"'\''"))
}

/// `text` as an AppleScript string literal.
fn applescript_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Runs `command` as an administrator, after the system's own prompt.
fn administrator(command: &str) -> Admin {
    let script = format!(
        "do shell script {} with administrator privileges",
        applescript_string(command)
    );
    match Command::new("/usr/bin/osascript")
        .args(["-e", &script])
        .output()
    {
        Ok(output) if output.status.success() => Admin::Done,
        Ok(output) => Admin::Failed(String::from_utf8_lossy(&output.stderr).trim().to_string()),
        Err(err) => Admin::Failed(err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    /// The documentation site names the link's two places as this fixture holds them.
    #[test]
    fn the_links_places_are_the_ones_the_docs_name() {
        let quoted: serde_json::Value =
            serde_json::from_str(include_str!("../../testdata/docs_system_paths.json")).unwrap();
        let places = Places::in_home(Path::new("/home/u"));
        let dirs = &quoted["command_line_dirs"];
        assert_eq!(dirs["system"].as_str(), places.system.to_str());
        let user = dirs["user"].as_str().unwrap().replacen('~', "/home/u", 1);
        assert_eq!(Some(user.as_str()), places.user.to_str());
    }

    /// A directory under the temporary one, which the test removes.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("octoboard-cli-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir.canonicalize().unwrap())
        }

        /// An `octoboardd` file inside a bundle named `app`.
        fn bundle(&self, app: &str) -> PathBuf {
            let dir = self.0.join(format!("{app}.app/Contents/MacOS"));
            std::fs::create_dir_all(&dir).unwrap();
            let file = dir.join(NAME);
            std::fs::write(&file, "").unwrap();
            file
        }

        fn places(&self) -> Places {
            Places {
                system: self.0.join("usr-local-bin"),
                user: self.0.join("home/.local/bin"),
            }
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _spawning = crate::spawning();
            // Directories this test made read-only are made writable to be removed.
            let _ = Command::new("/bin/chmod")
                .arg("-R")
                .arg("u+w")
                .arg(&self.0)
                .status();
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn refused(_: &str) -> Admin {
        Admin::Failed("User canceled.".to_string())
    }

    #[test]
    fn nothing_is_installed_before_an_install() {
        let scratch = Scratch::new("none");
        let bundled = scratch.bundle("Octoboard");
        assert_eq!(
            state_in(&scratch.places(), Some(&bundled)),
            State::NotInstalled
        );
    }

    #[test]
    fn a_writable_system_directory_gets_the_link_with_no_prompt() {
        let scratch = Scratch::new("system");
        let bundled = scratch.bundle("Octoboard");
        let places = scratch.places();
        std::fs::create_dir_all(&places.system).unwrap();
        let state = install_in(&places, &bundled, &|_| panic!("no prompt is needed")).unwrap();
        assert_eq!(
            state,
            State::Installed {
                path: Places::link(&places.system).display().to_string(),
                path_note: false
            }
        );
    }

    #[test]
    fn a_refused_prompt_falls_back_to_the_user_directory_with_a_path_note() {
        let scratch = Scratch::new("fallback");
        let bundled = scratch.bundle("Octoboard");
        let places = scratch.places();
        std::fs::create_dir_all(&places.system).unwrap();
        std::fs::set_permissions(&places.system, std::fs::Permissions::from_mode(0o555)).unwrap();
        let state = install_in(&places, &bundled, &refused).unwrap();
        assert_eq!(
            state,
            State::Installed {
                path: Places::link(&places.user).display().to_string(),
                path_note: true
            }
        );
    }

    #[test]
    fn a_link_to_another_copy_and_a_link_that_leads_nowhere_are_told_apart() {
        let scratch = Scratch::new("states");
        let ours = scratch.bundle("Octoboard");
        let other = scratch.bundle("Octoboard 2");
        let places = scratch.places();
        std::fs::create_dir_all(&places.system).unwrap();
        let link = Places::link(&places.system);
        symlink(&other, &link).unwrap();
        assert!(matches!(
            state_in(&places, Some(&ours)),
            State::OtherCopy { .. }
        ));
        std::fs::remove_file(&link).unwrap();
        let gone = scratch.0.join("Gone.app/Contents/MacOS").join(NAME);
        symlink(&gone, &link).unwrap();
        assert!(matches!(
            state_in(&places, Some(&ours)),
            State::Unresolved { .. }
        ));
    }

    #[test]
    fn a_file_that_is_not_a_link_is_never_replaced_and_a_foreign_link_is_not_ours() {
        let scratch = Scratch::new("foreign");
        let bundled = scratch.bundle("Octoboard");
        let places = scratch.places();
        std::fs::create_dir_all(&places.system).unwrap();
        let link = Places::link(&places.system);
        std::fs::write(&link, "mine").unwrap();
        let err = install_in(&places, &bundled, &refused).unwrap_err();
        assert_eq!(err.code, ErrorCode::NotALink);
        assert_eq!(err.path, Some(link.display().to_string()));
        assert_eq!(std::fs::read_to_string(&link).unwrap(), "mine");
        std::fs::remove_file(&link).unwrap();
        symlink("/bin/ls", &link).unwrap();
        assert_eq!(state_in(&places, Some(&bundled)), State::NotInstalled);
    }

    #[test]
    fn reinstalling_repoints_the_link_and_removing_deletes_it() {
        let scratch = Scratch::new("reinstall");
        let ours = scratch.bundle("Octoboard");
        let other = scratch.bundle("Octoboard 2");
        let places = scratch.places();
        std::fs::create_dir_all(&places.system).unwrap();
        symlink(&other, Places::link(&places.system)).unwrap();
        assert!(matches!(
            install_in(&places, &ours, &refused).unwrap(),
            State::Installed { .. }
        ));
        remove_in(&places, Some(&ours), &refused).unwrap();
        assert_eq!(state_in(&places, Some(&ours)), State::NotInstalled);
    }

    /// Stands in for the administrator prompt: records the script it is given and runs it as the
    /// user, after making `writable` writable, which the prompt's privileges would not need.
    fn prompt_running<'a>(
        seen: &'a std::cell::RefCell<Vec<String>>,
        writable: &'a Path,
    ) -> impl Fn(&str) -> Admin + 'a {
        let writable = writable.to_path_buf();
        move |script| {
            seen.borrow_mut().push(script.to_string());
            let _spawning = crate::spawning();
            if writable.exists() {
                let _ = std::fs::set_permissions(&writable, std::fs::Permissions::from_mode(0o755));
            }
            match Command::new("/bin/sh").arg("-c").arg(script).status() {
                Ok(status) if status.success() => Admin::Done,
                other => Admin::Failed(format!("{other:?}")),
            }
        }
    }

    #[test]
    fn a_system_directory_that_is_missing_or_not_writable_gets_the_link_through_the_prompt() {
        for readonly in [false, true] {
            let scratch = Scratch::new(if readonly {
                "prompt-readonly"
            } else {
                "prompt-missing"
            });
            let bundled = scratch.bundle("Octoboard");
            let places = scratch.places();
            if readonly {
                std::fs::create_dir_all(&places.system).unwrap();
                std::fs::set_permissions(&places.system, std::fs::Permissions::from_mode(0o555))
                    .unwrap();
            }
            let seen = std::cell::RefCell::new(Vec::new());
            let state =
                install_in(&places, &bundled, &prompt_running(&seen, &places.system)).unwrap();
            assert_eq!(
                state,
                State::Installed {
                    path: Places::link(&places.system).display().to_string(),
                    path_note: false
                },
                "readonly: {readonly}"
            );
            let scripts = seen.borrow();
            assert_eq!(scripts.len(), 1, "{scripts:?}");
            assert!(scripts[0].contains("ln -sfn"), "{}", scripts[0]);
            assert!(
                scripts[0].contains(&shell_quote(&bundled))
                    && scripts[0].contains(&shell_quote(&Places::link(&places.system))),
                "{}",
                scripts[0]
            );
            assert!(
                !Places::link(&places.user).exists(),
                "nothing is linked in the user's directory"
            );
        }
    }

    #[test]
    fn a_failure_that_is_not_a_refusal_falls_back_to_the_user_directory_without_a_prompt() {
        let scratch = Scratch::new("other-failure");
        let bundled = scratch.bundle("Octoboard");
        let places = scratch.places();
        // A file where the system directory should be: linking inside it fails, but not by refusal.
        std::fs::write(&places.system, "").unwrap();
        let state =
            install_in(&places, &bundled, &|_| panic!("no prompt for this failure")).unwrap();
        assert_eq!(
            state,
            State::Installed {
                path: Places::link(&places.user).display().to_string(),
                path_note: true
            }
        );
    }

    #[test]
    fn a_file_that_is_not_a_link_in_the_user_directory_is_never_replaced_either() {
        let scratch = Scratch::new("foreign-user");
        let bundled = scratch.bundle("Octoboard");
        let places = scratch.places();
        std::fs::create_dir_all(&places.user).unwrap();
        let link = Places::link(&places.user);
        std::fs::write(&link, "mine").unwrap();
        let err = install_in(&places, &bundled, &refused).unwrap_err();
        assert_eq!(err.code, ErrorCode::NotALink);
        assert_eq!(err.path, Some(link.display().to_string()));
        assert_eq!(std::fs::read_to_string(&link).unwrap(), "mine");
    }

    #[test]
    fn the_link_in_the_system_directory_replaces_one_an_earlier_fallback_left_in_the_users() {
        let scratch = Scratch::new("leftover");
        let bundled = scratch.bundle("Octoboard");
        let places = scratch.places();
        std::fs::create_dir_all(&places.system).unwrap();
        std::fs::create_dir_all(&places.user).unwrap();
        symlink(&bundled, Places::link(&places.user)).unwrap();
        let state = install_in(&places, &bundled, &refused).unwrap();
        assert_eq!(
            state,
            State::Installed {
                path: Places::link(&places.system).display().to_string(),
                path_note: false
            }
        );
        assert!(std::fs::symlink_metadata(Places::link(&places.user)).is_err());
    }

    #[test]
    fn removing_from_a_directory_the_user_cannot_write_goes_through_the_prompt_and_may_be_refused()
    {
        let scratch = Scratch::new("remove-prompt");
        let bundled = scratch.bundle("Octoboard");
        let places = scratch.places();
        std::fs::create_dir_all(&places.system).unwrap();
        let link = Places::link(&places.system);
        symlink(&bundled, &link).unwrap();
        std::fs::set_permissions(&places.system, std::fs::Permissions::from_mode(0o555)).unwrap();

        let err = remove_in(&places, Some(&bundled), &refused).unwrap_err();
        assert_eq!(err.code, ErrorCode::Refused);
        assert_eq!(err.path, Some(link.display().to_string()));
        assert_eq!(err.detail, "User canceled.");
        assert!(
            std::fs::symlink_metadata(&link).is_ok(),
            "the link is still there"
        );

        let seen = std::cell::RefCell::new(Vec::new());
        remove_in(
            &places,
            Some(&bundled),
            &prompt_running(&seen, &places.system),
        )
        .unwrap();
        assert_eq!(*seen.borrow(), [format!("rm -f {}", shell_quote(&link))]);
        assert!(std::fs::symlink_metadata(&link).is_err());
        assert_eq!(state_in(&places, Some(&bundled)), State::NotInstalled);
    }

    #[test]
    fn removing_takes_the_link_out_of_both_directories_and_leaves_nothing_of_it_in_either() {
        let scratch = Scratch::new("remove-both");
        let ours = scratch.bundle("Octoboard");
        let other = scratch.bundle("Octoboard 2");
        let places = scratch.places();
        std::fs::create_dir_all(&places.system).unwrap();
        std::fs::create_dir_all(&places.user).unwrap();
        // A link to this copy in the system's directory and one an earlier copy left in the user's.
        symlink(&ours, Places::link(&places.system)).unwrap();
        symlink(&other, Places::link(&places.user)).unwrap();
        remove_in(&places, Some(&ours), &|_| {
            panic!("both are the user's to write")
        })
        .unwrap();
        assert_eq!(state_in(&places, Some(&ours)), State::NotInstalled);
        for dir in [&places.system, &places.user] {
            assert_eq!(
                std::fs::read_dir(dir).unwrap().count(),
                0,
                "{} is empty",
                dir.display()
            );
        }
    }

    #[test]
    fn installing_leaves_only_the_link_in_its_directory() {
        let scratch = Scratch::new("install-only-link");
        let bundled = scratch.bundle("Octoboard");
        let places = scratch.places();
        std::fs::create_dir_all(&places.system).unwrap();
        install_in(&places, &bundled, &|_| panic!("no prompt is needed")).unwrap();
        let names: Vec<_> = std::fs::read_dir(&places.system)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, [std::ffi::OsString::from(NAME)]);
        assert!(!places.user.exists(), "the user's directory is not made");
    }

    #[test]
    fn removing_leaves_a_file_that_is_not_a_link_and_a_link_to_something_else() {
        let scratch = Scratch::new("remove-foreign");
        let bundled = scratch.bundle("Octoboard");
        let places = scratch.places();
        std::fs::create_dir_all(&places.system).unwrap();
        std::fs::create_dir_all(&places.user).unwrap();
        let file = Places::link(&places.system);
        std::fs::write(&file, "mine").unwrap();
        let foreign = Places::link(&places.user);
        symlink("/bin/ls", &foreign).unwrap();
        remove_in(&places, Some(&bundled), &|_| {
            panic!("nothing of ours to remove")
        })
        .unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "mine");
        assert!(std::fs::symlink_metadata(&foreign).is_ok());
    }

    #[test]
    fn a_link_in_the_users_directory_is_where_the_tool_stands_with_a_path_note() {
        let scratch = Scratch::new("user-state");
        let bundled = scratch.bundle("Octoboard");
        let places = scratch.places();
        std::fs::create_dir_all(&places.user).unwrap();
        symlink(&bundled, Places::link(&places.user)).unwrap();
        assert_eq!(
            state_in(&places, Some(&bundled)),
            State::Installed {
                path: Places::link(&places.user).display().to_string(),
                path_note: true
            }
        );
    }

    #[test]
    fn a_path_is_quoted_for_the_shell_and_for_applescript() {
        assert_eq!(shell_quote(Path::new("/a b/it's")), r"'/a b/it'\''s'");
        assert_eq!(applescript_string(r#"a"b\c"#), r#""a\"b\\c""#);
    }

    #[test]
    fn a_refused_prompt_is_an_error_not_a_fallback_where_the_system_directory_holds_a_link() {
        let scratch = Scratch::new("refused-reinstall");
        let ours = scratch.bundle("Octoboard");
        let other = scratch.bundle("Octoboard 2");
        let places = scratch.places();
        std::fs::create_dir_all(&places.system).unwrap();
        let link = Places::link(&places.system);
        symlink(&other, &link).unwrap();
        std::fs::set_permissions(&places.system, std::fs::Permissions::from_mode(0o555)).unwrap();
        let err = install_in(&places, &ours, &refused).unwrap_err();
        assert_eq!(err.code, ErrorCode::Refused);
        assert_eq!(err.path, Some(link.display().to_string()));
        assert_eq!(err.detail, "User canceled.");
        assert!(std::fs::symlink_metadata(Places::link(&places.user)).is_err());
        assert_eq!(std::fs::read_link(&link).unwrap(), other);
    }

    #[test]
    fn a_prompt_that_failed_otherwise_is_a_failure_with_what_the_system_said() {
        let scratch = Scratch::new("failed-prompt");
        let bundled = scratch.bundle("Octoboard");
        let places = scratch.places();
        std::fs::create_dir_all(&places.system).unwrap();
        symlink(&bundled, Places::link(&places.system)).unwrap();
        std::fs::set_permissions(&places.system, std::fs::Permissions::from_mode(0o555)).unwrap();
        let err =
            install_in(&places, &bundled, &|_| Admin::Failed("no sudo".to_string())).unwrap_err();
        assert_eq!(err.code, ErrorCode::Failed);
        assert_eq!(err.detail, "no sudo");
    }

    #[test]
    fn a_link_to_something_that_is_not_an_octoboardd_is_never_replaced_at_either_location() {
        for in_user_directory in [false, true] {
            let scratch = Scratch::new("foreign-link");
            let bundled = scratch.bundle("Octoboard");
            let places = scratch.places();
            let dir = if in_user_directory {
                &places.user
            } else {
                &places.system
            };
            std::fs::create_dir_all(&places.system).unwrap();
            std::fs::create_dir_all(&places.user).unwrap();
            let link = Places::link(dir);
            symlink("/bin/ls", &link).unwrap();
            let err = install_in(&places, &bundled, &|_| panic!("no prompt")).unwrap_err();
            assert_eq!(err.code, ErrorCode::ForeignLink);
            assert_eq!(err.path, Some(link.display().to_string()));
            assert_eq!(err.detail, "/bin/ls");
            assert_eq!(std::fs::read_link(&link).unwrap(), Path::new("/bin/ls"));
        }
    }

    #[test]
    fn a_link_to_a_copy_that_is_gone_is_removed_though_this_build_cannot_resolve_its_own() {
        let scratch = Scratch::new("remove-unresolved");
        let places = scratch.places();
        std::fs::create_dir_all(&places.system).unwrap();
        let link = Places::link(&places.system);
        symlink(scratch.0.join("Gone.app/Contents/MacOS").join(NAME), &link).unwrap();
        remove_in(&places, None, &refused).unwrap();
        assert!(std::fs::symlink_metadata(&link).is_err());
        assert_eq!(state_in(&places, None), State::NotInstalled);
    }

    #[test]
    fn the_errors_serialise_with_a_code_a_path_and_the_detail() {
        let error = Error::new(ErrorCode::NotALink, Some("/a".to_string()), "x");
        assert_eq!(
            serde_json::to_string(&error).unwrap(),
            r#"{"code":"not_a_link","path":"/a","detail":"x"}"#
        );
        assert_eq!(
            serde_json::to_string(&Error::failed("y")).unwrap(),
            r#"{"code":"failed","path":null,"detail":"y"}"#
        );
        assert_eq!(
            admin_error(
                Path::new("/l"),
                "execution error: User canceled. (-128)".into()
            )
            .code,
            ErrorCode::Refused
        );
        assert_eq!(
            admin_error(Path::new("/l"), "boom".into()).code,
            ErrorCode::Failed
        );
    }
}
