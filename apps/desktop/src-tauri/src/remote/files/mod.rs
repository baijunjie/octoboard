//! Copies of a remote daemon's files kept on this computer, for a remote window's "Download and
//! open": the page reads a file from its daemon in parts (`read_file_range`) and hands each part to
//! the commands here, which only store and open; nothing here speaks the daemon protocol.
//!
//! A copy lives in a folder of its own, `<local data directory>/remote-files/copies/<random id>/<name>`,
//! so two files of one name never meet and the file keeps the name that picks its application. The
//! application's local data directory is one of its own, so nothing of a copy is left anywhere else
//! on the computer, and it is the one that stays on this computer (`config_dir::local_data_path`):
//! a copy has no size limit and is not for a roaming profile to carry to a server. A build before
//! this one kept the copies under the temporary directory, and what it left there is removed at
//! the start (`clear_earlier_place`). The folders
//! and the copy are readable by the user alone and the copy has no executable bit. A name this
//! computer could launch rather than show (`config/launchable-extensions.json`, the one list the
//! daemon and the UI read as well, the Windows part of it on Windows only) is refused before
//! anything is stored, and so is a file the daemon reported as having an executable bit: what the
//! system would run is for this computer to say of a name, the remote machine knowing only its own
//! system and being the less trusted side. Before it is opened the copy is marked as downloaded (the
//! quarantine attribute on macOS, the Internet zone's `Zone.Identifier` stream on Windows), which is
//! the system's own mechanism for what no list of extensions can know.
//!
//! A copy stays after it is opened, so that whatever opened it can go on reading it, and every copy
//! is removed when the application next starts (`clear`). One that is cancelled or fails is removed
//! at once, and so are the unopened copies of a window that closes. Nothing is removed at quit: an
//! application still showing a copy would lose it.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use super::RemoteState;
use crate::diagnostics::diagnostic;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod windows;

#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(not(target_os = "macos"))]
use windows as platform;

use platform::{
    check_private, create_private_dir, mark_downloaded, open_plain_file, private_file_options,
};

/// The folder of the application's local data directory that holds the copies' folder and its
/// lock, made for the user alone (the directory itself may be readable by others). A build that
/// kept the copies under the temporary directory named their folder so, below the application's
/// identifier.
const FOLDER: &str = "remote-files";

/// The copies' folder, inside [`FOLDER`].
const COPIES: &str = "copies";

/// The file beside the copies' folder that the running instance holds locked (`INSTANCE_LOCK`).
const LOCK_FILE: &str = "remote-files.lock";

/// The longest name a copy is given, in bytes, well inside what a file system takes.
const MAX_NAME_BYTES: usize = 200;

/// How many random ids are tried for a folder that does not exist yet.
const ID_TRIES: usize = 8;

/// The extensions of what the system could launch rather than show, from the file the daemon and
/// the UI read as well. Every list applies whatever the remote machine's system is, except
/// `windows_final_file`, which applies only where this computer is Windows.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Launchable {
    any_component: Vec<String>,
    final_file: Vec<String>,
    windows_final_file: Vec<String>,
}

fn launchable_extensions() -> &'static Launchable {
    static LISTS: LazyLock<Launchable> = LazyLock::new(|| {
        serde_json::from_str(include_str!(
            "../../../../../../config/launchable-extensions.json"
        ))
        .expect("config/launchable-extensions.json is the lists of launchable extensions")
    });
    &LISTS
}

/// Whether what follows the last dot of `name` is on a list of what this computer's system launches, compared without
/// regard to case, a leading dot included: the system's opener takes `.bat` for a batch file though `Path::extension`
/// says it has none. The lists that hold for every system apply here; the Windows list (`.js`, `.lnk`, ...) only on
/// Windows, where such a copy would run when opened and elsewhere is plain data. A name ending in dots or spaces is
/// judged as the file system will keep it (Windows drops them).
fn is_launchable_name(name: &str) -> bool {
    is_launchable_name_on(name, cfg!(windows))
}

fn is_launchable_name_on(name: &str, windows: bool) -> bool {
    let Some((_, extension)) = name.trim_end_matches(['.', ' ']).rsplit_once('.') else {
        return false;
    };
    let extension = extension.to_ascii_lowercase();
    let lists = launchable_extensions();
    [&lists.any_component, &lists.final_file]
        .into_iter()
        .chain(windows.then_some(&lists.windows_final_file))
        .any(|list| list.contains(&extension))
}

/// Why a copy could not be stored or opened, as the page is told.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    /// What this computer would run: nothing was stored.
    Launchable,
    /// The copy could not be written, or is not one of this window's.
    Storage,
    /// The parts that arrived are not the file's size.
    Incomplete,
    /// The copy is whole but the system could not mark it or open it.
    Open,
}

#[derive(Debug, Serialize)]
pub struct FileFailure {
    pub kind: FailureKind,
    /// The system's text, which the page shows as the details of a failure it has no words for and reads a full
    /// disk or a denied access off (`(os error N)`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl FileFailure {
    fn new(kind: FailureKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: Some(detail.into()),
        }
    }

    fn of_io(kind: FailureKind, err: &std::io::Error) -> Self {
        Self::new(kind, err.to_string())
    }

    fn unknown() -> Self {
        Self::new(FailureKind::Storage, "there is no such copy")
    }
}

/// A copy being written.
struct Copy {
    /// The label of the window that began it, the only one that may write or open it.
    window: String,
    /// The folder of its own, removed with the copy.
    folder: PathBuf,
    path: PathBuf,
    file: File,
    size: u64,
    written: u64,
}

/// The copies being written, by id: those begun and not yet opened or discarded.
#[derive(Default)]
pub struct Copies(Mutex<HashMap<String, Copy>>);

impl Copies {
    fn held(&self) -> std::sync::MutexGuard<'_, HashMap<String, Copy>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// [`Self::begin_while`] for a window that is taken to hold its connection.
    #[cfg(test)]
    fn begin(
        &self,
        root: &Path,
        window: &str,
        name: &str,
        size: u64,
        launchable: bool,
    ) -> Result<String, FileFailure> {
        self.begin_while(root, window, name, size, launchable, || true)
    }

    /// Begins a copy of `size` bytes named `name` for `window` under `root`, and gives its id, if
    /// `holds_connection` says the window still holds its connection. That is asked and the copy
    /// recorded under one hold of the lock that `window_closed` takes to remove the window's copies, which
    /// runs once the window's connection is gone: a copy begun as its window closes is either refused
    /// or removed with it, never orphaned.
    fn begin_while(
        &self,
        root: &Path,
        window: &str,
        name: &str,
        size: u64,
        launchable: bool,
        holds_connection: impl FnOnce() -> bool,
    ) -> Result<String, FileFailure> {
        let name = local_name(name);
        if launchable || is_launchable_name(&name) {
            return Err(FileFailure::new(
                FailureKind::Launchable,
                "this computer would run the file",
            ));
        }
        let storage = |err: std::io::Error| FileFailure::of_io(FailureKind::Storage, &err);
        let mut held = self.held();
        if !holds_connection() {
            return Err(FileFailure::unknown());
        }
        create_private_dir(root, true).map_err(storage)?;
        let (id, folder) = (0..ID_TRIES)
            .find_map(|_| {
                let id = random_id();
                let folder = root.join(&id);
                match create_private_dir(&folder, false) {
                    Ok(()) => Some(Ok((id, folder))),
                    Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => None,
                    Err(err) => Some(Err(err)),
                }
            })
            .unwrap_or_else(|| {
                Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "no unused folder was found",
                ))
            })
            .map_err(storage)?;
        // What this process just made is its own: the folders above it must be too, and shut to others.
        if let Err(err) = check_private(root, &folder) {
            remove(&folder);
            return Err(storage(err));
        }
        let path = folder.join(name);
        let file = match create_private_file(&path) {
            Ok(file) => file,
            Err(err) => {
                remove(&folder);
                return Err(storage(err));
            }
        };
        held.insert(
            id.clone(),
            Copy {
                window: window.to_owned(),
                folder,
                path,
                file,
                size,
                written: 0,
            },
        );
        Ok(id)
    }

    /// Appends `part` to the copy `id` of `window`. A part that passes the file's size is refused, and
    /// a write that fails removes the copy.
    fn write(&self, window: &str, id: &str, part: &[u8]) -> Result<(), FileFailure> {
        let mut held = self.held();
        let copy = held
            .get_mut(id)
            .filter(|copy| copy.window == window)
            .ok_or_else(FileFailure::unknown)?;
        if copy.written + part.len() as u64 > copy.size {
            return Err(FileFailure::new(
                FailureKind::Incomplete,
                "more bytes arrived than the file has",
            ));
        }
        if let Err(err) = copy.file.write_all(part) {
            if let Some(copy) = held.remove(id) {
                remove(&copy.folder);
            }
            return Err(FileFailure::of_io(FailureKind::Storage, &err));
        }
        copy.written += part.len() as u64;
        Ok(())
    }

    /// Closes the copy `id` of `window`, which has all its bytes, marks it as downloaded and opens it
    /// with `opener`; its record is forgotten and the copy stays. A copy that cannot be marked or
    /// opened is removed.
    fn open(
        &self,
        window: &str,
        id: &str,
        opener: impl FnOnce(&Path) -> Result<(), String>,
    ) -> Result<(), FileFailure> {
        let mut held = self.held();
        let complete = held
            .get(id)
            .filter(|copy| copy.window == window)
            .ok_or_else(FileFailure::unknown)?
            .written
            == held[id].size;
        if !complete {
            return Err(FileFailure::new(
                FailureKind::Incomplete,
                "fewer bytes arrived than the file has",
            ));
        }
        let copy = held.remove(id).expect("the copy was found above");
        drop(held);
        let Copy {
            folder, path, file, ..
        } = copy;
        let opened = file
            .sync_all()
            .and_then(|()| {
                drop(file);
                mark_downloaded(&path)
            })
            .map_err(|err| FileFailure::of_io(FailureKind::Open, &err))
            .and_then(|()| {
                opener(&path).map_err(|detail| FileFailure::new(FailureKind::Open, detail))
            });
        if opened.is_err() {
            remove(&folder);
        }
        opened
    }

    /// Removes the copy `id` of `window`, if it has one.
    fn discard(&self, window: &str, id: &str) {
        let mut held = self.held();
        if held.get(id).is_some_and(|copy| copy.window == window) {
            if let Some(copy) = held.remove(id) {
                remove(&copy.folder);
            }
        }
    }

    /// Removes every copy `window` began and has not opened.
    pub fn window_closed(&self, window: &str) {
        let mut held = self.held();
        let ids: Vec<String> = held
            .iter()
            .filter(|(_, copy)| copy.window == window)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            if let Some(copy) = held.remove(&id) {
                remove(&copy.folder);
            }
        }
    }
}

/// Removes a copy's folder; a failure is logged and otherwise ignored, the next launch clears it.
fn remove(folder: &Path) {
    if let Err(err) = fs::remove_dir_all(folder) {
        if err.kind() != std::io::ErrorKind::NotFound {
            diagnostic!("removing a downloaded copy failed: {err}");
        }
    }
}

/// The folder the copies are kept in.
fn root(app: &AppHandle) -> tauri::Result<PathBuf> {
    Ok(crate::config_dir::local_data_path(app, FOLDER)?.join(COPIES))
}

/// The copies' folder as a command answers it: one that is not known is a failure to store.
fn known(root: tauri::Result<PathBuf>) -> Result<PathBuf, FileFailure> {
    root.map_err(|err| FileFailure::new(FailureKind::Storage, err.to_string()))
}

/// The lock an instance of the application holds for as long as it runs, which tells a later one that the copies
/// are in use.
static INSTANCE_LOCK: OnceLock<File> = OnceLock::new();

/// Removes every copy of an earlier run, before any window opens, unless another running instance of the
/// application holds the copies' lock (a second start, where the system keeps no single instance), whose copies
/// they are. A failure is logged and ignored.
pub fn clear(app: &AppHandle) {
    clear_earlier_place(&std::env::temp_dir().join(&app.config().identifier));
    match root(app) {
        Ok(root) => {
            if let Some(lock) = clear_in(&root) {
                let _ = INSTANCE_LOCK.set(lock);
            }
        }
        Err(err) => {
            diagnostic!("the folder of the downloaded copies is not known: {err}");
        }
    }
}

/// Removes what a build that kept the copies under the temporary directory left in `parent`, its
/// folder there (`<temp>/<identifier>`): the copies' folder, the lock beside it, and `parent` itself
/// once it is empty. Nothing is removed while an instance of such a build still runs and holds the
/// lock, or where `parent` is not a folder (a link there is not followed). The lock is opened
/// without waiting and counts only where what was opened is a plain file (`open_plain_file`): the
/// temporary directory may be shared, and what another user put there under that name (a pipe)
/// would otherwise hold the start up for ever. A failure is ignored: the next start tries again.
fn clear_earlier_place(parent: &Path) {
    if !fs::symlink_metadata(parent).is_ok_and(|found| found.is_dir()) {
        return;
    }
    let lock_file = parent.join(LOCK_FILE);
    let held = open_plain_file(&lock_file)
        .is_ok_and(|lock| matches!(lock.try_lock(), Err(fs::TryLockError::WouldBlock)));
    if held {
        return;
    }
    remove(&parent.join(FOLDER));
    let _ = fs::remove_file(&lock_file);
    // Fails where anything else is in it, which then stays.
    let _ = fs::remove_dir(parent);
}

/// [`clear`] for the folder `root`: takes the lock beside it, which the caller keeps for as long as the copies it
/// goes on to make are in use, and removes `root`; with none to give, another holds it, or it could not be taken,
/// and nothing is removed.
fn clear_in(root: &Path) -> Option<File> {
    let parent = root.parent()?;
    let lock = create_private_dir(parent, true)
        .and_then(|()| {
            let mut options = private_file_options();
            options.create(true).truncate(false);
            options.open(parent.join(LOCK_FILE))
        })
        .and_then(|lock| {
            lock.try_lock().map(|()| lock).map_err(|err| match err {
                fs::TryLockError::Error(err) => err,
                fs::TryLockError::WouldBlock => std::io::ErrorKind::WouldBlock.into(),
            })
        });
    match lock {
        Ok(lock) => {
            remove(root);
            Some(lock)
        }
        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => None,
        Err(err) => {
            diagnostic!("clearing the downloaded copies of an earlier run failed: {err}");
            None
        }
    }
}

/// An id nobody can guess, from the randomness the standard library seeds its hash maps with.
fn random_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    let part = || {
        std::collections::hash_map::RandomState::new()
            .build_hasher()
            .finish()
    };
    format!("{:016x}{:016x}", part(), part())
}

/// Creates a new, non-executable file with the platform's private-file options.
fn create_private_file(path: &Path) -> std::io::Result<File> {
    private_file_options().create_new(true).open(path)
}

/// The name a copy is given: the last component of `name`, with separators, control characters and
/// what Windows reserves in a name replaced by `_`, cut to a length the file system takes, and
/// `file` where nothing is left.
fn local_name(name: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let replaced: String = last
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect();
    // Windows drops trailing dots and spaces from a name, which would change what it ends in.
    let trimmed = replaced.trim_end_matches(['.', ' ']).trim_start();
    if trimmed.is_empty() {
        return "file".to_owned();
    }
    let mut name = cut(trimmed);
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.ends_with(|c: char| c.is_ascii_digit() && c != '0'));
    if reserved {
        name.insert(0, '_');
    }
    name
}

/// `name` cut to [`MAX_NAME_BYTES`] on a character boundary, keeping a short extension.
fn cut(name: &str) -> String {
    if name.len() <= MAX_NAME_BYTES {
        return name.to_owned();
    }
    let extension = name
        .rfind('.')
        .map(|dot| &name[dot..])
        .filter(|extension| extension.len() <= 20)
        .unwrap_or("");
    let mut end = MAX_NAME_BYTES - extension.len();
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{extension}", &name[..end])
}

/// Whether the calling window holds a remote connection; the page of any other has no daemon files
/// to copy.
fn holds_connection(app: &AppHandle, label: &str) -> bool {
    app.state::<RemoteState>().windows().contains_key(label)
}

/// Begins a copy of a remote file for the calling window and gives its id. `launchable` is the
/// daemon's word for the file; the name is judged here as well, and either refuses the copy before
/// anything is stored.
#[tauri::command(async)]
pub fn remote_file_begin(
    app: AppHandle,
    webview_window: tauri::WebviewWindow,
    name: String,
    size: u64,
    launchable: bool,
) -> Result<String, FileFailure> {
    let label = webview_window.label();
    let root = known(root(&app))?;
    app.state::<RemoteState>()
        .files
        .begin_while(&root, label, &name, size, launchable, || {
            holds_connection(&app, label)
        })
}

/// Appends a part to a copy. The body of the call is the copy's id, preceded by its length in one byte, then
/// the part (`framed`): the id travels in the bytes, not in a header, so it arrives by whichever channel the
/// webview delivers the call.
#[tauri::command(async)]
pub fn remote_file_write(
    app: AppHandle,
    webview_window: tauri::WebviewWindow,
    request: tauri::ipc::Request<'_>,
) -> Result<(), FileFailure> {
    let body = bytes_of(request.body())?;
    let (id, part) = framed(&body)?;
    app.state::<RemoteState>()
        .files
        .write(webview_window.label(), id, part)
}

/// The copy's id and the part a body of `remote_file_write` holds: one byte with the id's length, the id, and
/// the rest the part.
fn framed(body: &[u8]) -> Result<(&str, &[u8]), FileFailure> {
    let unframed = || FileFailure::new(FailureKind::Storage, "the part does not name its copy");
    let (&length, rest) = body.split_first().ok_or_else(unframed)?;
    let (id, part) = rest
        .split_at_checked(usize::from(length))
        .ok_or_else(unframed)?;
    let id = std::str::from_utf8(id).map_err(|_| unframed())?;
    Ok((id, part))
}

/// The bytes of a call's body: the raw body, or, where the webview delivers the call by the message channel
/// instead of the custom protocol (WebView2 does whenever the page's policy refuses the protocol's address),
/// the same bytes as a JSON array of numbers.
fn bytes_of(body: &tauri::ipc::InvokeBody) -> Result<std::borrow::Cow<'_, [u8]>, FileFailure> {
    use tauri::ipc::InvokeBody;
    let unreadable = || FileFailure::new(FailureKind::Storage, "the part is not bytes");
    match body {
        InvokeBody::Raw(bytes) => Ok(bytes.as_slice().into()),
        InvokeBody::Json(serde_json::Value::Array(items)) => items
            .iter()
            .map(|item| item.as_u64().and_then(|byte| u8::try_from(byte).ok()))
            .collect::<Option<Vec<u8>>>()
            .map(Into::into)
            .ok_or_else(unreadable),
        InvokeBody::Json(_) => Err(unreadable()),
    }
}

/// Opens a copy that has all its bytes with this computer's default application for it, marked as
/// downloaded.
#[tauri::command(async)]
pub fn remote_file_open(
    app: AppHandle,
    webview_window: tauri::WebviewWindow,
    id: String,
) -> Result<(), FileFailure> {
    app.state::<RemoteState>()
        .files
        .open(webview_window.label(), &id, |path| open_on_main(&app, path))
}

/// Opens the file with the system's opener on the main thread, which the system's calls to open one expect, and
/// waits for what it says.
fn open_on_main(app: &AppHandle, path: &Path) -> Result<(), String> {
    let (answer, answered) = std::sync::mpsc::channel();
    let path = path.to_owned();
    app.run_on_main_thread(move || {
        let _ = answer.send(crate::open_address::open_file(&path));
    })
    .map_err(|err| err.to_string())?;
    answered.recv().map_err(|err| err.to_string())?
}

/// Removes a copy that was cancelled or failed.
#[tauri::command(async)]
pub fn remote_file_discard(app: AppHandle, webview_window: tauri::WebviewWindow, id: String) {
    app.state::<RemoteState>()
        .files
        .discard(webview_window.label(), &id);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The copies' folder of a scratch folder, which holds it as the local data directory's `remote-files` does: the
    /// folder above it is the one `check_private` also looks at, so it cannot be the shared temporary directory.
    fn scratch(name: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "octoboard-remote-files-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        base.join(FOLDER)
    }

    #[test]
    fn a_name_is_reduced_to_one_the_file_system_takes() {
        for (given, expected) in [
            ("report.pdf", "report.pdf"),
            ("/etc/passwd", "passwd"),
            ("C:\\Users\\a\\b.txt", "b.txt"),
            ("a/../..", "file"),
            ("..", "file"),
            ("", "file"),
            ("a:b*c?.txt", "a_b_c_.txt"),
            ("tab\there.txt", "tab_here.txt"),
            ("name. . ", "name"),
            ("CON", "_CON"),
            ("nul.txt", "_nul.txt"),
            ("COM1.log", "_COM1.log"),
            ("COM0.log", "COM0.log"),
            ("console.txt", "console.txt"),
        ] {
            assert_eq!(local_name(given), expected, "{given:?}");
        }
        let long = format!("{}.pdf", "é".repeat(300));
        let cut = local_name(&long);
        assert!(cut.len() <= MAX_NAME_BYTES);
        assert!(cut.ends_with(".pdf"));
    }

    #[test]
    fn what_this_computer_could_launch_is_known_by_the_lists_of_its_system_without_regard_to_case()
    {
        for windows in [false, true] {
            for name in [
                "a.sh",
                "a.EXE",
                "a.pdf.exe",
                "A.App",
                "a.py",
                "a.exe.",
                "a.exe ",
                ".bat",
                ".EXE",
                ".cmd",
                ".hta",
                ".sh",
            ] {
                assert!(is_launchable_name_on(name, windows), "{name} {windows}");
            }
            for name in [
                "a.pdf",
                "exe",
                "a",
                "a.exe.pdf",
                "a.txt",
                ".",
                "..",
                ".gitignore",
            ] {
                assert!(!is_launchable_name_on(name, windows), "{name} {windows}");
            }
        }
    }

    #[test]
    fn what_only_a_windows_system_runs_is_refused_there_and_copied_elsewhere() {
        for name in [
            "x.lnk",
            "x.iso",
            "x.js",
            "x.vsmacros",
            ".lnk",
            ".js",
            "X.JS",
        ] {
            assert!(is_launchable_name_on(name, true), "{name}");
            assert!(!is_launchable_name_on(name, false), "{name}");
        }
        assert_eq!(is_launchable_name("x.js"), cfg!(windows));
    }

    #[test]
    fn a_copy_is_begun_written_and_opened_with_what_arrived() {
        let root = scratch("flow");
        let copies = Copies::default();
        let id = copies.begin(&root, "w", "dir/note.txt", 5, false).unwrap();
        copies.write("w", &id, b"he").unwrap();
        copies.write("w", &id, b"llo").unwrap();
        let mut opened = None;
        copies
            .open("w", &id, |path| {
                opened = Some(path.to_owned());
                Ok(())
            })
            .unwrap();
        let path = opened.unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"hello");
        assert_eq!(path.parent().unwrap().parent().unwrap(), root);
        assert_eq!(path.file_name().unwrap(), "note.txt");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&path), 0o600);
            assert_eq!(mode(path.parent().unwrap()), 0o700);
            assert_eq!(mode(&root), 0o700);
        }
        // The record is forgotten and the copy stays.
        assert!(copies.held().is_empty());
        copies.window_closed("w");
        assert!(path.exists());
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn a_launchable_copy_stores_nothing() {
        let root = scratch("launchable");
        let copies = Copies::default();
        for (name, flag) in [("a.exe", false), ("tool", true), ("a.pdf.exe", false)] {
            let failure = copies.begin(&root, "w", name, 1, flag).unwrap_err();
            assert_eq!(failure.kind, FailureKind::Launchable, "{name}");
        }
        assert!(!root.exists());
    }

    #[test]
    fn a_part_past_the_size_an_early_open_and_another_window_s_id_are_refused() {
        let root = scratch("refused");
        let copies = Copies::default();
        let id = copies.begin(&root, "w", "a.txt", 3, false).unwrap();
        assert_eq!(
            copies.write("w", &id, b"abcd").unwrap_err().kind,
            FailureKind::Incomplete
        );
        copies.write("w", &id, b"ab").unwrap();
        assert_eq!(
            copies.open("w", &id, |_| Ok(())).unwrap_err().kind,
            FailureKind::Incomplete
        );
        assert_eq!(
            copies.write("other", &id, b"c").unwrap_err().kind,
            FailureKind::Storage
        );
        assert_eq!(
            copies.open("other", &id, |_| Ok(())).unwrap_err().kind,
            FailureKind::Storage
        );
        copies.discard("other", &id);
        assert_eq!(copies.held().len(), 1);
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn a_discarded_copy_a_failed_open_and_a_closed_window_remove_the_folder() {
        let root = scratch("removed");
        let copies = Copies::default();
        let folders = || fs::read_dir(&root).unwrap().count();
        let id = copies.begin(&root, "w", "a.txt", 1, false).unwrap();
        assert_eq!(folders(), 1);
        copies.discard("w", &id);
        assert_eq!(folders(), 0);
        let id = copies.begin(&root, "w", "a.txt", 1, false).unwrap();
        copies.write("w", &id, b"x").unwrap();
        let failure = copies
            .open("w", &id, |_| Err("no application".to_owned()))
            .unwrap_err();
        assert_eq!(failure.kind, FailureKind::Open);
        assert_eq!(folders(), 0);
        copies.begin(&root, "w", "a.txt", 1, false).unwrap();
        copies.begin(&root, "v", "b.txt", 1, false).unwrap();
        copies.window_closed("w");
        assert_eq!(folders(), 1);
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_copy_is_quarantined_on_macos() {
        let _spawning = crate::spawning();
        let root = scratch("mark");
        let copies = Copies::default();
        let id = copies.begin(&root, "w", "a.txt", 1, false).unwrap();
        copies.write("w", &id, b"x").unwrap();
        let mut marked = false;
        copies
            .open("w", &id, |path| {
                let output = std::process::Command::new("xattr")
                    .arg(path)
                    .output()
                    .unwrap();
                marked = String::from_utf8_lossy(&output.stdout).contains("com.apple.quarantine");
                Ok(())
            })
            .unwrap();
        assert!(marked);
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn the_same_name_copied_twice_lands_in_two_folders_and_an_empty_file_opens_without_a_part() {
        let root = scratch("twice");
        let copies = Copies::default();
        let first = copies.begin(&root, "w", "a.txt", 0, false).unwrap();
        let second = copies.begin(&root, "w", "a.txt", 0, false).unwrap();
        assert_ne!(first, second);
        let mut paths = Vec::new();
        for id in [&first, &second] {
            copies
                .open("w", id, |path| {
                    paths.push(path.to_owned());
                    Ok(())
                })
                .unwrap();
        }
        assert_ne!(paths[0], paths[1]);
        for path in &paths {
            assert_eq!(path.file_name().unwrap(), "a.txt");
            assert_eq!(fs::read(path).unwrap(), b"");
        }
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn an_opened_copy_is_no_longer_the_copies_to_write_open_or_discard() {
        let root = scratch("opened");
        let copies = Copies::default();
        let id = copies.begin(&root, "w", "a.txt", 1, false).unwrap();
        copies.write("w", &id, b"x").unwrap();
        let mut opened = None;
        copies
            .open("w", &id, |path| {
                opened = Some(path.to_owned());
                Ok(())
            })
            .unwrap();
        let path = opened.unwrap();
        assert_eq!(
            copies.write("w", &id, b"y").unwrap_err().kind,
            FailureKind::Storage
        );
        assert_eq!(
            copies.open("w", &id, |_| Ok(())).unwrap_err().kind,
            FailureKind::Storage
        );
        // A late discard (the viewer closing after the copy was opened) leaves what was opened.
        copies.discard("w", &id);
        assert_eq!(fs::read(&path).unwrap(), b"x");
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn clearing_removes_every_copy_under_the_folder_and_a_folder_that_is_not_there_is_no_failure() {
        let root = scratch("clear");
        let copies = Copies::default();
        let id = copies.begin(&root, "w", "a.txt", 1, false).unwrap();
        copies.write("w", &id, b"x").unwrap();
        copies.begin(&root, "v", "b.txt", 1, false).unwrap();
        remove(&root);
        assert!(!root.exists());
        remove(&root);
    }

    #[test]
    fn a_copy_is_refused_for_a_window_without_its_connection_and_removed_with_one_that_closes() {
        let root = scratch("closing");
        let copies = Copies::default();
        let refused = copies
            .begin_while(&root, "w", "a.txt", 1, false, || false)
            .unwrap_err();
        assert_eq!(refused.kind, FailureKind::Storage);
        assert!(!root.exists());
        // The connection is asked for with the lock held, so the window's cleanup cannot run between the
        // question and the copy being recorded.
        copies
            .begin_while(&root, "w", "a.txt", 1, false, || {
                assert!(copies.0.try_lock().is_err());
                true
            })
            .unwrap();
        copies.window_closed("w");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn a_body_is_the_raw_body_or_a_json_array_of_bytes_and_nothing_else() {
        use tauri::ipc::InvokeBody;
        let part = |body: InvokeBody| bytes_of(&body).map(|bytes| bytes.into_owned());
        assert_eq!(part(InvokeBody::Raw(vec![0, 1, 255])).unwrap(), [0, 1, 255]);
        assert_eq!(
            part(InvokeBody::Json(serde_json::json!([0, 1, 255]))).unwrap(),
            [0, 1, 255]
        );
        assert_eq!(
            part(InvokeBody::Json(serde_json::json!([]))).unwrap(),
            Vec::<u8>::new()
        );
        for bad in [
            serde_json::json!([256]),
            serde_json::json!([-1]),
            serde_json::json!([1.5]),
            serde_json::json!(["a"]),
            serde_json::json!("abc"),
            serde_json::json!({ "a": 1 }),
            serde_json::Value::Null,
        ] {
            assert_eq!(
                part(InvokeBody::Json(bad)).unwrap_err().kind,
                FailureKind::Storage
            );
        }
    }

    #[test]
    fn a_body_names_its_copy_ahead_of_the_part_by_either_channel() {
        use tauri::ipc::InvokeBody;
        let id = "0123456789abcdef0123456789abcdef";
        let mut body = vec![32];
        body.extend_from_slice(id.as_bytes());
        body.extend_from_slice(&[0, 1, 255]);
        let json = InvokeBody::Json(serde_json::Value::from(body.clone()));
        for body in [InvokeBody::Raw(body), json] {
            let bytes = bytes_of(&body).unwrap();
            assert_eq!(framed(&bytes).unwrap(), (id, &[0, 1, 255][..]));
        }
        // A part may be empty; the id may not be cut short or be other than text.
        assert_eq!(framed(&[2, b'i', b'd']).unwrap(), ("id", &[][..]));
        for bad in [&[][..], &[3, b'i', b'd'][..], &[1, 0xff, 7][..]] {
            assert_eq!(framed(bad).unwrap_err().kind, FailureKind::Storage);
        }
    }

    #[test]
    fn a_copies_folder_that_is_not_known_is_a_failure_to_store_with_the_reason() {
        let unknown: tauri::Result<PathBuf> =
            Err(std::io::Error::other("no local data directory").into());
        let failure = known(unknown).unwrap_err();
        assert_eq!(failure.kind, FailureKind::Storage);
        assert!(
            failure
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("no local data directory")),
            "{:?}",
            failure.detail
        );
        assert_eq!(
            known(Ok(PathBuf::from("/a/copies"))).unwrap(),
            PathBuf::from("/a/copies")
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_lock_of_an_earlier_build_that_is_not_a_plain_file_is_not_opened() {
        let _spawning = crate::spawning();
        let parent = scratch("earlier-pipe").parent().unwrap().to_owned();
        fs::create_dir_all(parent.join(FOLDER)).unwrap();
        // A pipe nobody writes to: opening it for reading would wait for ever.
        let made = std::process::Command::new("mkfifo")
            .arg(parent.join(LOCK_FILE))
            .status()
            .unwrap();
        assert!(made.success());
        clear_earlier_place(&parent);
        assert!(!parent.exists());
    }

    #[test]
    fn what_an_earlier_build_left_under_the_temporary_directory_is_removed_with_its_folder() {
        let parent = scratch("earlier").parent().unwrap().to_owned();
        fs::create_dir_all(parent.join(FOLDER).join("id")).unwrap();
        fs::write(parent.join(FOLDER).join("id").join("a.txt"), "x").unwrap();
        fs::write(parent.join(LOCK_FILE), "").unwrap();
        clear_earlier_place(&parent);
        assert!(!parent.exists());
        // Nothing there is no failure, and nothing is made.
        clear_earlier_place(&parent);
        assert!(!parent.exists());
    }

    #[test]
    fn a_folder_of_an_earlier_build_that_holds_something_else_stays_with_it() {
        let parent = scratch("earlier-other").parent().unwrap().to_owned();
        fs::create_dir_all(parent.join(FOLDER)).unwrap();
        fs::write(parent.join("other"), "kept").unwrap();
        clear_earlier_place(&parent);
        assert!(!parent.join(FOLDER).exists());
        assert_eq!(fs::read_to_string(parent.join("other")).unwrap(), "kept");
        let _ = fs::remove_dir_all(&parent);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_where_the_earlier_builds_folder_was_is_not_followed() {
        let base = scratch("earlier-link").parent().unwrap().to_owned();
        let target = base.join("target");
        fs::create_dir_all(target.join(FOLDER)).unwrap();
        let link = base.join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        clear_earlier_place(&link);
        assert!(target.join(FOLDER).exists());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn what_an_earlier_build_left_stays_while_an_instance_of_it_holds_the_lock() {
        // On the system's own temporary folder, as the test of the copies' lock below is.
        let _spawning = crate::spawning();
        let temporary = if cfg!(unix) {
            PathBuf::from("/tmp")
        } else {
            std::env::temp_dir()
        };
        let parent = temporary.join(format!(
            "octoboard-remote-files-earlier-lock-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&parent);
        let root = parent.join(FOLDER);
        fs::create_dir_all(root.join("copy")).unwrap();
        let held = clear_in(&root).unwrap();
        fs::create_dir_all(root.join("copy")).unwrap();
        clear_earlier_place(&parent);
        assert!(root.join("copy").exists());
        drop(held);
        clear_earlier_place(&parent);
        assert!(!parent.exists());
    }

    #[test]
    fn a_second_instance_clears_nothing_while_the_first_holds_the_copies_lock() {
        // On the system's own temporary folder: a lock taken on a volume that is not the system's (an external
        // disk a scratch folder may be on) does not always conflict with another.
        let _spawning = crate::spawning();
        let temporary = if cfg!(unix) {
            PathBuf::from("/tmp")
        } else {
            std::env::temp_dir()
        };
        let base = temporary.join(format!(
            "octoboard-remote-files-lock-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let root = base.join(FOLDER);
        fs::create_dir_all(root.join("copy")).unwrap();
        let first = clear_in(&root);
        assert!(first.is_some());
        assert!(!root.exists());
        fs::create_dir_all(root.join("copy")).unwrap();
        assert!(clear_in(&root).is_none());
        assert!(root.join("copy").exists());
        drop(first);
        assert!(clear_in(&root).is_some());
        assert!(!root.exists());
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn folders_that_are_not_shut_to_others_or_are_links_are_not_used() {
        use std::os::unix::fs::PermissionsExt;
        let root = scratch("private");
        let base = root.parent().unwrap().to_owned();
        let copies = Copies::default();
        copies.begin(&root, "w", "a.txt", 1, false).unwrap();
        for folder in [&root, &base] {
            let mode = fs::metadata(folder).unwrap().permissions().mode();
            fs::set_permissions(folder, fs::Permissions::from_mode(0o755)).unwrap();
            let failure = copies.begin(&root, "w", "a.txt", 1, false).unwrap_err();
            assert_eq!(failure.kind, FailureKind::Storage, "{folder:?}");
            fs::set_permissions(folder, fs::Permissions::from_mode(mode & 0o777)).unwrap();
        }
        copies.begin(&root, "w", "a.txt", 1, false).unwrap();
        // The folder for the copies replaced by a link to one that is shut to others.
        let elsewhere = base.join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        fs::set_permissions(&elsewhere, fs::Permissions::from_mode(0o700)).unwrap();
        fs::remove_dir_all(&root).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &root).unwrap();
        assert_eq!(
            copies
                .begin(&root, "w", "a.txt", 1, false)
                .unwrap_err()
                .kind,
            FailureKind::Storage
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn the_page_is_told_a_failure_by_kind_with_the_systems_text_when_there_is_one() {
        for (kind, name) in [
            (FailureKind::Launchable, "launchable"),
            (FailureKind::Storage, "storage"),
            (FailureKind::Incomplete, "incomplete"),
            (FailureKind::Open, "open"),
        ] {
            let told = serde_json::to_value(FileFailure::new(kind, "text")).unwrap();
            assert_eq!(told, serde_json::json!({ "kind": name, "detail": "text" }));
        }
    }

    #[test]
    fn the_permission_is_in_the_remote_window_capability_alone() {
        assert!(include_str!("../../../capabilities/remote-window.json")
            .contains("\"allow-remote-files\""));
        for other in [
            include_str!("../../../capabilities/default.json"),
            include_str!("../../../capabilities/remote.json"),
            include_str!("../../../capabilities/flyout.json"),
        ] {
            assert!(!other.contains("allow-remote-files"));
        }
        let permission = include_str!("../../../permissions/remote-files.toml");
        for command in [
            "remote_file_begin",
            "remote_file_write",
            "remote_file_open",
            "remote_file_discard",
        ] {
            assert!(permission.contains(&format!("\"{command}\"")), "{command}");
        }
    }
}
