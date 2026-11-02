//! File paths a session's terminal shows, checked and opened on the daemon's host. A path is
//! reachable when it names an existing file or directory inside the session's project (its own
//! directory or its place in a worktree of the same repository) or inside a system temp root,
//! where agents keep scratch files. Symbolic links are resolved before the roots are checked, so a
//! link cannot lead out of one.
//!
//! What could be launched rather than viewed is never opened by a link click: an application or
//! other bundle, a script or installer, a file with an executable bit. A bundle extension counts on
//! any component of a path, an interpreter or installer extension (`.py`, `.exe`, `.deb`) only on a
//! regular file's own name, so a folder named `github.com` is still a folder. Inside a temp root
//! what could be launched is not reachable at all; in a project a folder of that kind is not
//! either. The system decides what a file's default application does with it, which no list of
//! extensions can know, so no file in a project is handed to the host's opener at all: each is
//! `viewer_only`, to be shown by the file viewer, and `open` refuses it. On macOS `open` also
//! refuses a folder the system types as a package (`.xcodeproj`, `.playground`), which no list
//! names. Only a file in a temp root, a folder in a project or a folder in a temp root is opened.
//! A path under a temp root that lies in a project (or a worktree of it) is the project's, however
//! it is spelled.
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
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::Result;
use tokio::sync::Semaphore;

use crate::protocol::{error_code, CodedError, PathLink, PathLinkKind, PathLinkView};

mod resolve;
use resolve::{reach, resolve, Refusal, Resolved, Unreached};

/// The most paths one `probe_path_links` request may ask about.
pub const MAX_PROBED_PATHS: usize = 64;

/// How many probes run at once; each can touch the file system for up to `MAX_PROBED_PATHS` paths.
pub static PROBE_PERMITS: Semaphore = Semaphore::const_new(crate::blocking_pass::BLOCKING_WIDTH);

/// How long the roots of a project are reused: finding its worktrees runs `git`.
const ROOTS_TTL: Duration = Duration::from_secs(5);

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

/// The roots of each project as last found, with when: `project_roots` reuses them for
/// [`ROOTS_TTL`]. Held in memory only, on `AppState`, and dropped with the project.
#[derive(Default)]
pub struct RootsCache(Mutex<HashMap<(String, String), (Instant, ProjectRoots)>>);

impl RootsCache {
    /// What was found for `project` at `path` within [`ROOTS_TTL`].
    fn get(&self, project: &str, path: &str) -> Option<ProjectRoots> {
        let cache = self.0.lock().expect("path link roots cache poisoned");
        let (at, roots) = cache.get(&(project.to_owned(), path.to_owned()))?;
        (at.elapsed() < ROOTS_TTL).then(|| roots.clone())
    }

    fn insert(&self, project: &str, path: &str, roots: ProjectRoots) {
        self.0
            .lock()
            .expect("path link roots cache poisoned")
            .insert(
                (project.to_owned(), path.to_owned()),
                (Instant::now(), roots),
            );
    }

    /// Drops everything held for `project`, whichever directory it was found at.
    pub fn forget(&self, project: &str) {
        self.0
            .lock()
            .expect("path link roots cache poisoned")
            .retain(|(id, _), _| id != project);
    }
}

/// The roots of `project`, as they were found within the last few seconds unless `fresh`. A
/// project whose directory cannot be resolved has none, which leaves its paths to the temp roots.
fn project_roots(state: &crate::state::AppState, project: &str, fresh: bool) -> ProjectRoots {
    let record = state.store.get_project(project).ok().flatten();
    // Keyed by the stored path as well, so a project pointed at another directory is not answered
    // from the roots of the old one.
    let path = record
        .as_ref()
        .map_or_else(String::new, |record| record.path.clone());
    if !fresh {
        if let Some(roots) = state.path_link_roots.get(project, &path) {
            return roots;
        }
    }
    let live = crate::browse::scope::project_live_roots(
        state,
        project,
        &crate::git::isolated::IsolatedGit::from_shell,
    )
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
    state.path_link_roots.insert(project, &path, roots.clone());
    roots
}

/// Refuses a project the daemon does not know, the one failure that is the request's own.
fn known(state: &crate::state::AppState, project: Option<&str>) -> Result<()> {
    if let Some(project) = project {
        state.store.require_project(project)?;
    }
    Ok(())
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
    // Found at most once, however many paths ask for them.
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
            let viewer_only = resolved.as_ref().is_some_and(Resolved::viewer_only);
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
        path: crate::wire_path::encode(relative.as_os_str().as_bytes()),
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
/// resolving it again as `probe` does, from the project's roots as they are now. On macOS a folder
/// the system types as a package is refused like a launchable one; the type is asked only here,
/// where a path is about to be opened.
pub fn open(state: &crate::state::AppState, project: Option<&str>, path: &str) -> Result<()> {
    open_with(
        state,
        project,
        path,
        |path| cfg!(target_os = "macos") && crate::host_open::paths::is_macos_package(path),
        |path| crate::host_open::paths::open_path(path, false),
    )
}

/// [`open`] with what says a folder is a package and what opens a resolved path given, so that a
/// test can see that nothing is opened.
fn open_with(
    state: &crate::state::AppState,
    project: Option<&str>,
    path: &str,
    is_package: impl FnOnce(&Path) -> bool,
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
        Ok(resolved) if !resolved.viewer_only() => resolved,
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
    if resolved.kind == PathLinkKind::Directory && is_package(&resolved.path) {
        return Err(Refusal::Launchable.into_error(path));
    }
    opener(&resolved.path)
}

#[cfg(test)]
mod tests;
