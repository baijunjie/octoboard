//! File paths a session's terminal shows, checked and opened on the daemon's host. A path is
//! reachable when it names an existing file or directory inside the session's project (its own
//! directory or its place in a worktree of the same repository) or inside a system temp root,
//! where agents keep scratch files. Symbolic links are resolved before the roots are checked, so a
//! link cannot lead out of one.
//!
//! What could be launched rather than viewed (an application or other bundle, a script or installer
//! by extension, a file with an executable bit) is never opened by a link click: inside a temp root
//! it is not reachable at all, and in a project a folder of that kind is not either, while a file
//! is, as `viewer_only`, to be shown by the file viewer and nothing else (`open` refuses it).
//! Inside a temp root, which other users can write to, the file and every directory down to it must
//! also belong to the daemon's user and be closed to writes by anyone else. A relative path is
//! tried against the project's directory and each worktree's place and is reachable only when
//! exactly one of them has it.
//!
//! A path found in a project root also says where the file viewer reads it from (`view`); whether
//! the viewer can show it is for the client to find out by reading it, so no file is read here. For
//! a directory the view is its entry file, if it has one (found by listing it): a README, else an
//! `index.*`.

use std::collections::HashMap;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::Result;
use tokio::sync::Semaphore;

use crate::hostfs::{is_not_found, lexically_normalise};
use crate::protocol::{error_code, CodedError, PathLink, PathLinkKind, PathLinkView};

/// The most paths one `probe_path_links` request may ask about.
pub const MAX_PROBED_PATHS: usize = 64;

/// How many probes run at once; each can touch the file system for up to `MAX_PROBED_PATHS` paths.
pub static PROBE_PERMITS: Semaphore = Semaphore::const_new(4);

/// How long the roots of a project are reused: finding its worktrees runs `git`.
const ROOTS_TTL: Duration = Duration::from_secs(5);

/// Extensions of things the system launches instead of showing, matched without case on every
/// component of a path below its root (a bundle is a directory carrying one).
const LAUNCHABLE_EXTENSIONS: &[&str] = &[
    "app",
    "appex",
    "bundle",
    "framework",
    "plugin",
    "kext",
    "xpc",
    "prefpane",
    "saver",
    "workflow",
    "action",
    "command",
    "tool",
    "terminal",
    "scpt",
    "scptd",
    "pkg",
    "mpkg",
    "dmg",
    "sh",
    "bash",
    "zsh",
    "desktop",
    "webloc",
    "inetloc",
    "jar",
    "term",
    "fileloc",
    "afploc",
    "ftploc",
    "mailloc",
    "newsloc",
    "atloc",
    "nslloc",
    "url",
    "csh",
    "tcsh",
    "ksh",
    "fish",
    "mobileconfig",
    "shortcut",
];

/// The temp roots. `canonical` are the roots as they resolve now; `lexical` are the spellings a
/// path may start with before it is looked at (`/tmp` and `/private/tmp`, the daemon's temp
/// directory as given and as resolved).
pub struct TempRoots {
    pub canonical: Vec<PathBuf>,
    pub lexical: Vec<PathBuf>,
    /// Whether ownership and write permission down to a path are checked: off only in tests that
    /// have no way to be another user.
    pub check_owner: bool,
}

/// The temp roots: `/tmp`, `/private/tmp` and the platform's temp directory (`$TMPDIR`). A root
/// that does not exist is left out, and so is one that is the filesystem root or holds the home
/// directory, which would make everything there a scratch file.
pub fn temp_roots() -> TempRoots {
    let home = crate::paths::known_home_dir().and_then(|home| std::fs::canonicalize(home).ok());
    let candidates = [
        PathBuf::from("/tmp"),
        PathBuf::from("/private/tmp"),
        std::env::temp_dir(),
    ]
    .into_iter()
    .filter_map(|root| Some((std::fs::canonicalize(&root).ok()?, root)))
    .collect::<Vec<_>>();
    usable_temp_roots(&candidates, home.as_deref())
}

/// The roots among `candidates`, each a resolved directory with the way it was written, that may be
/// scratch space: not the filesystem root, and not one holding `home`.
fn usable_temp_roots(candidates: &[(PathBuf, PathBuf)], home: Option<&Path>) -> TempRoots {
    let mut roots = TempRoots {
        canonical: Vec::new(),
        lexical: Vec::new(),
        check_owner: true,
    };
    for (canonical, written) in candidates {
        if canonical.parent().is_none() || home.is_some_and(|home| home.starts_with(canonical)) {
            continue;
        }
        for form in [written, canonical] {
            if !roots.lexical.contains(form) {
                roots.lexical.push(form.clone());
            }
        }
        if !roots.canonical.contains(canonical) {
            roots.canonical.push(canonical.clone());
        }
    }
    roots
}

/// The directories a project's paths can name: `canonical` as they resolve now, and `lexical`, which
/// adds how the project's directory is stored, the spelling an absolute path may start with.
#[derive(Clone, Default)]
pub struct ProjectRoots {
    pub canonical: Vec<PathBuf>,
    /// The worktree id each of `canonical` is read by, none for the project's own directory.
    pub worktrees: Vec<Option<String>>,
    pub lexical: Vec<PathBuf>,
}

impl ProjectRoots {
    /// The root `canonical` lies in, the innermost of those that hold it: a worktree kept below the
    /// project's directory is its own root, not a path of the project's.
    fn innermost(&self, canonical: &Path) -> Option<usize> {
        (0..self.canonical.len())
            .filter(|&i| canonical.starts_with(&self.canonical[i]))
            .max_by_key(|&i| self.canonical[i].components().count())
    }
}

/// A path the terminal's text names, as it resolves.
#[derive(Debug, PartialEq, Eq)]
pub struct Resolved {
    pub path: PathBuf,
    pub kind: PathLinkKind,
    /// A file in a project root that the system could launch: to be viewed, and refused by `open`.
    pub launchable: bool,
    /// The project root it was found in (an index of `ProjectRoots::canonical`); none in a temp root.
    pub project_root: Option<usize>,
}

/// What `raw` names, if it is reachable. An absolute path or one starting with `~/` is taken as
/// written, any other is looked for under each of the project's roots. `project` finds those roots,
/// and is only called for a path not under a temp root, or one gone from under it.
pub fn resolve(
    raw: &str,
    home: Option<&Path>,
    temp: &TempRoots,
    project: &dyn Fn() -> ProjectRoots,
) -> Option<Resolved> {
    reach(raw, home, temp, project).ok()
}

/// Why [`reach`] found nothing.
#[derive(Debug, PartialEq, Eq)]
pub enum Unreached {
    /// Spelled inside a root, but nothing is there now: `in_project` when spelled inside a
    /// project's root, even one that lies under a temp root.
    Gone { in_project: bool },
    /// Anything else not to show, and why.
    Refused(Refusal),
}

/// Why a path that is not gone is not to be shown.
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Outside every root: spelled outside them, or resolving outside them (a link leading out).
    Outside,
    /// A relative path found in more than one of the project's roots.
    Ambiguous,
    /// Something the system would run, which a link never opens.
    Launchable,
    /// In a temp root, but not solely the daemon user's, or writable by group or others.
    NotPrivate,
    /// Exists, but is neither a file nor a folder (a socket, a device).
    NotAFileOrFolder,
    /// The system denied reading the path or a directory on the way to it; holds its text.
    Denied(String),
    /// Any other I/O failure looking at the path; holds the system's text.
    Unreadable(String),
    /// A path that cannot be looked up at all (empty, containing NUL, or `~/` with no known home).
    Other,
}

impl Refusal {
    /// What an I/O failure on the way to a path means.
    fn of_io(err: &std::io::Error) -> Self {
        if err.kind() == std::io::ErrorKind::PermissionDenied {
            Self::Denied(err.to_string())
        } else {
            Self::Unreadable(err.to_string())
        }
    }

    /// The error a refused request for `path` is answered with.
    fn into_error(self, path: &str) -> anyhow::Error {
        let (cause, detail) = match self {
            Self::Denied(detail) => {
                return CodedError::raised(
                    error_code::PERMISSION_DENIED,
                    format!("{path} could not be read: {detail}"),
                    &[("path", path), ("detail", &detail)],
                )
            }
            Self::Outside => {
                return CodedError::raised(
                    error_code::OUTSIDE_SCOPE,
                    format!("{path} leads outside the project and the temp roots"),
                    &[("path", path), ("action", "open")],
                )
            }
            Self::Ambiguous => ("ambiguous", None),
            Self::Launchable => ("launchable", None),
            Self::NotPrivate => ("not_private", None),
            Self::NotAFileOrFolder => ("not_a_file_or_folder", None),
            Self::Unreadable(detail) => ("unreadable", Some(detail)),
            Self::Other => ("other", None),
        };
        let mut params = vec![("path", path), ("cause", cause)];
        if let Some(detail) = &detail {
            params.push(("detail", detail));
        }
        CodedError::raised(
            error_code::PATH_LINK_REFUSED,
            format!("{path} is not opened from a link: {cause}"),
            &params,
        )
    }
}

/// [`resolve`], saying why it found nothing.
pub fn reach(
    raw: &str,
    home: Option<&Path>,
    temp: &TempRoots,
    project: &dyn Fn() -> ProjectRoots,
) -> Result<Resolved, Unreached> {
    if raw.is_empty() || raw.contains('\0') {
        return Err(Unreached::Refused(Refusal::Other));
    }
    let absolute = if let Some(rest) = raw.strip_prefix("~/") {
        Some(home.ok_or(Unreached::Refused(Refusal::Other))?.join(rest))
    } else if Path::new(raw).is_absolute() {
        Some(PathBuf::from(raw))
    } else {
        None
    };
    // Folded lexically first, so that a `..` cannot lead `canonicalize` out of the roots the
    // spelling was checked against.
    let Some(absolute) = absolute.map(|path| lexically_normalise(&path)) else {
        let roots = project();
        let mut found: Vec<PathBuf> = Vec::new();
        let (mut spelled, mut missing) = (0, 0);
        let mut failure: Option<Refusal> = None;
        for root in &roots.canonical {
            let joined = lexically_normalise(&root.join(raw));
            if !roots.lexical.iter().any(|root| joined.starts_with(root)) {
                continue;
            }
            spelled += 1;
            let canonical = match std::fs::canonicalize(joined) {
                Ok(canonical) => canonical,
                Err(err) => {
                    if is_not_found(&err) {
                        missing += 1;
                    } else {
                        let refusal = Refusal::of_io(&err);
                        // A denied read is the more telling of two failures.
                        if failure.is_none() || matches!(refusal, Refusal::Denied(_)) {
                            failure = Some(refusal);
                        }
                    }
                    continue;
                }
            };
            if roots
                .canonical
                .iter()
                .any(|root| canonical.starts_with(root))
                && !found.contains(&canonical)
            {
                found.push(canonical);
            }
        }
        let [canonical] = found.as_slice() else {
            return Err(if found.len() > 1 {
                Unreached::Refused(Refusal::Ambiguous)
            } else if spelled > 0 && missing == spelled {
                Unreached::Gone { in_project: true }
            } else {
                // Nothing found: a failure looking, else every spelling led outside the roots.
                Unreached::Refused(failure.unwrap_or(Refusal::Outside))
            });
        };
        let index = roots
            .innermost(canonical)
            .ok_or(Unreached::Refused(Refusal::Outside))?;
        let mut resolved = reachable(canonical.clone(), &roots.canonical[index], false, true)
            .map_err(Unreached::Refused)?;
        resolved.project_root = Some(index);
        return Ok(resolved);
    };
    if temp.lexical.iter().any(|root| absolute.starts_with(root)) {
        // A gone path is the project's when it is spelled inside it too, as for a project kept
        // under a temp root: the project's files are where it is looked for again.
        let canonical = canonical_or_gone(&absolute, || {
            project()
                .lexical
                .iter()
                .any(|root| absolute.starts_with(root))
        })?;
        let root = temp
            .canonical
            .iter()
            .find(|root| canonical.starts_with(root))
            .ok_or(Unreached::Refused(Refusal::Outside))?;
        return reachable(canonical, root, temp.check_owner, false).map_err(Unreached::Refused);
    }
    let roots = project();
    if !roots.lexical.iter().any(|root| absolute.starts_with(root)) {
        return Err(Unreached::Refused(Refusal::Outside));
    }
    let canonical = canonical_or_gone(&absolute, || true)?;
    let index = roots
        .innermost(&canonical)
        .ok_or(Unreached::Refused(Refusal::Outside))?;
    let mut resolved =
        reachable(canonical, &roots.canonical[index], false, true).map_err(Unreached::Refused)?;
    resolved.project_root = Some(index);
    Ok(resolved)
}

/// `path` with its links resolved, or why not: gone when nothing is there (see [`is_not_found`]),
/// `in_project` asked only then.
fn canonical_or_gone(path: &Path, in_project: impl FnOnce() -> bool) -> Result<PathBuf, Unreached> {
    std::fs::canonicalize(path).map_err(|err| {
        if is_not_found(&err) {
            Unreached::Gone {
                in_project: in_project(),
            }
        } else {
            Unreached::Refused(Refusal::of_io(&err))
        }
    })
}

/// `canonical`, found inside `root`, with what it is, unless it is not something to show: not a file
/// or folder, or (with `check_owner`) not solely the daemon's user's from `root` down. What could be
/// launched is refused too, except a file in a project (`in_project`), which is kept for the viewer.
fn reachable(
    canonical: PathBuf,
    root: &Path,
    check_owner: bool,
    in_project: bool,
) -> Result<Resolved, Refusal> {
    let metadata = std::fs::metadata(&canonical).map_err(|err| Refusal::of_io(&err))?;
    let kind = if metadata.is_dir() {
        PathLinkKind::Directory
    } else if metadata.is_file() {
        PathLinkKind::File
    } else {
        return Err(Refusal::NotAFileOrFolder);
    };
    let launchable = launchable(&canonical, root, &metadata);
    if launchable && !(in_project && kind == PathLinkKind::File) {
        return Err(Refusal::Launchable);
    }
    if check_owner && !closed_to_others(&canonical, root) {
        return Err(Refusal::NotPrivate);
    }
    Ok(Resolved {
        path: canonical,
        kind,
        launchable,
        project_root: None,
    })
}

/// Whether every entry from the one below `root` down to `canonical` belongs to the daemon's user
/// and cannot be written by its group or others, as read without following links.
fn closed_to_others(canonical: &Path, root: &Path) -> bool {
    let Ok(below) = canonical.strip_prefix(root) else {
        return false;
    };
    // SAFETY: `geteuid` takes no arguments and cannot fail.
    let me = unsafe { libc::geteuid() };
    let mut current = root.to_path_buf();
    below.components().all(|component| {
        current.push(component);
        std::fs::symlink_metadata(&current)
            .is_ok_and(|metadata| metadata.uid() == me && metadata.mode() & 0o022 == 0)
    })
}

/// Whether the system could run what `canonical` names, or a bundle it sits in, rather than show it.
fn launchable(canonical: &Path, root: &Path, metadata: &std::fs::Metadata) -> bool {
    if metadata.is_file() && metadata.permissions().mode() & 0o111 != 0 {
        return true;
    }
    let below = canonical.strip_prefix(root).unwrap_or(canonical);
    below.components().any(|component| {
        let Component::Normal(name) = component else {
            return false;
        };
        Path::new(name).extension().is_some_and(|extension| {
            let extension = extension.to_string_lossy().to_ascii_lowercase();
            LAUNCHABLE_EXTENSIONS.contains(&extension.as_str())
        })
    })
}

fn roots_cache() -> &'static Mutex<HashMap<String, (Instant, ProjectRoots)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (Instant, ProjectRoots)>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// The roots of `project`, as they were found within the last few seconds unless `fresh`. A
/// project whose directory cannot be resolved has none, which leaves its paths to the temp roots.
fn project_roots(state: &crate::state::AppState, project: &str, fresh: bool) -> ProjectRoots {
    let record = state.store.get_project(project).ok().flatten();
    // Keyed by the stored path as well, so a project pointed at another directory is not answered
    // from the roots of the old one.
    let key = format!(
        "{project}\0{}",
        record.as_ref().map_or("", |record| &record.path)
    );
    if !fresh {
        let cache = roots_cache().lock().unwrap();
        if let Some((at, roots)) = cache.get(&key) {
            if at.elapsed() < ROOTS_TTL {
                return roots.clone();
            }
        }
    }
    let live =
        crate::browse::project_live_roots(state, project, &crate::browse::git::GitEnv::from_shell)
            .unwrap_or_default();
    let canonical: Vec<PathBuf> = live.iter().map(|live| live.root.clone()).collect();
    let worktrees = live.into_iter().map(|live| live.worktree).collect();
    let mut lexical = canonical.clone();
    if let Some(record) = record {
        lexical.push(PathBuf::from(record.path));
    }
    let roots = ProjectRoots {
        canonical,
        worktrees,
        lexical,
    };
    roots_cache()
        .lock()
        .unwrap()
        .insert(key, (Instant::now(), roots.clone()));
    roots
}

/// Refuses a project the daemon does not know, the one failure that is the request's own.
fn known(state: &crate::state::AppState, project: Option<&str>) -> Result<()> {
    match project {
        Some(project) if state.store.get_project(project)?.is_none() => {
            Err(CodedError::unknown_project(project))
        }
        _ => Ok(()),
    }
}

/// Answers each of `paths`, in order, with what it names.
pub fn probe(
    state: &crate::state::AppState,
    project: Option<&str>,
    paths: Vec<String>,
) -> Result<Vec<PathLink>> {
    if paths.len() > MAX_PROBED_PATHS {
        let max = MAX_PROBED_PATHS.to_string();
        return Err(CodedError::raised(
            error_code::LIMIT_EXCEEDED,
            format!("at most {max} paths may be probed at once"),
            &[("limit", "probed_paths"), ("max", &max)],
        ));
    }
    known(state, project)?;
    let temp = temp_roots();
    let home = crate::paths::known_home_dir();
    // Found at most once, and only for a path not under a temp root, or one gone from under it.
    let roots = OnceLock::new();
    let lookup = || {
        roots
            .get_or_init(|| {
                project.map_or_else(ProjectRoots::default, |project| {
                    project_roots(state, project, false)
                })
            })
            .clone()
    };
    Ok(paths
        .into_iter()
        .map(|path| {
            let resolved = resolve(&path, home.as_deref(), &temp, &lookup);
            let kind = resolved.as_ref().map_or(PathLinkKind::None, |r| r.kind);
            let viewer_only = resolved.as_ref().is_some_and(|r| r.launchable);
            let view = resolved.and_then(|resolved| {
                // Only a path found in the project's roots has a view, and only those need them again.
                resolved.project_root?;
                view_of(&resolved, &lookup())
            });
            PathLink {
                path,
                kind,
                view,
                viewer_only,
            }
        })
        .collect())
}

/// Where the viewer reads `resolved` from, when it is a file in a project root, or a directory
/// there that has an entry file (see [`entry_file`]).
fn view_of(resolved: &Resolved, roots: &ProjectRoots) -> Option<PathLinkView> {
    let index = resolved.project_root?;
    let target = match resolved.kind {
        PathLinkKind::File => resolved.path.clone(),
        PathLinkKind::Directory => entry_file(&resolved.path)?,
        PathLinkKind::None => return None,
    };
    let relative = target.strip_prefix(roots.canonical.get(index)?).ok()?;
    Some(PathLinkView {
        worktree: roots.worktrees.get(index)?.clone(),
        path: crate::browse::wire_path::encode(relative.as_os_str().as_bytes()),
    })
}

/// The file a click on a directory shows instead of opening it: a regular file directly in it
/// (a link is not followed) named `README` or `README.*`, else one named `index.*`, ignoring case.
/// Among several, `.md` comes first, then `.markdown`, then any other name, each group by lowercase
/// name.
fn entry_file(directory: &Path) -> Option<PathBuf> {
    let mut readmes = Vec::new();
    let mut indexes = Vec::new();
    for entry in std::fs::read_dir(directory).ok()?.flatten() {
        if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            continue;
        }
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let lower = name.to_ascii_lowercase();
        let rank = |lower: String, name: String| {
            // Lower sorts first: `.md`, then `.markdown`, then any other name.
            let markdown = if lower.ends_with(".md") {
                0
            } else if lower.ends_with(".markdown") {
                1
            } else {
                2
            };
            (markdown, lower, name)
        };
        if lower == "readme" || lower.starts_with("readme.") {
            readmes.push(rank(lower, name));
        } else if lower.len() > "index.".len() && lower.starts_with("index.") {
            indexes.push(rank(lower, name));
        }
    }
    let best = readmes
        .into_iter()
        .min()
        .or_else(|| indexes.into_iter().min());
    best.map(|(_, _, name)| directory.join(name))
}

/// Opens `path` with the host's default application (a directory in its file manager), after
/// resolving it again as `probe` does, from the project's roots as they are now.
pub fn open(state: &crate::state::AppState, project: Option<&str>, path: &str) -> Result<()> {
    open_with(state, project, path, |path| {
        crate::open_directory::open_path(path, false)
    })
}

/// [`open`] with what opens a resolved path given, so that a test can see that nothing is.
fn open_with(
    state: &crate::state::AppState,
    project: Option<&str>,
    path: &str,
    opener: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    known(state, project)?;
    let home = crate::paths::known_home_dir();
    let lookup = || {
        project.map_or_else(ProjectRoots::default, |project| {
            project_roots(state, project, true)
        })
    };
    let resolved = reach(path, home.as_deref(), &temp_roots(), &lookup);
    let resolved = match resolved {
        Ok(resolved) if !resolved.launchable => resolved,
        Ok(_) => return Err(Refusal::Launchable.into_error(path)),
        Err(Unreached::Refused(refusal)) => return Err(refusal.into_error(path)),
        Err(Unreached::Gone { in_project }) => {
            return Err(CodedError::raised(
                error_code::FILE_NOT_FOUND,
                format!("{path} does not exist"),
                &[
                    ("path", path),
                    ("root", if in_project { "project" } else { "temporary" }),
                ],
            ))
        }
    };
    opener(&resolved.path)
}

#[cfg(test)]
mod tests;
