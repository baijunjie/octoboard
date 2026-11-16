//! File paths a session's terminal shows, checked and opened on the daemon's host. A path is
//! reachable when it names an existing file or directory inside the session's project (its own
//! directory or its place in a worktree of the same repository) or inside a system temp root,
//! where agents keep scratch files. Symbolic links are resolved before the roots are checked, so a
//! link cannot lead out of one.
//!
//! A click on a file never hands it to the host's opener: the system's default application for a
//! file may run it, so the client reads the file ([`read`]) and shows it in the file viewer, in a
//! project or in a temp root alike. Only a folder is opened by a click, in the file manager.
//!
//! What could be launched rather than viewed is never opened at all: an application or other
//! bundle, a script or installer, a file with an executable bit. A bundle extension counts on any
//! component of a path, an interpreter or installer extension (`.py`, `.exe`, `.deb`) only on a
//! regular file's own name, so a folder named `github.com` is still a folder. A folder of that kind
//! is not reachable at all; a file of that kind is, as `viewer_only`, to be shown by the file
//! viewer, and `open` refuses it. The system decides what a file's default application does with
//! it, which no list of extensions can know, so that is the user's own choice, made with `open`,
//! on a file that is not launchable. On macOS `open` also refuses a folder the system types as a
//! package (`.xcodeproj`, `.playground`), which no list names.
//! Showing a path in the file manager (`reveal`) launches nothing, so it also takes a launchable
//! file and a package folder, and refuses what is not reachable at all.
//! A path under a temp root that lies in a project (or a worktree of it) is the project's, however
//! it is spelled.
//! Inside a temp root, which other users can write to, the file and every directory down to it must
//! also belong to the daemon's user and be closed to writes by anyone else. A file of a project
//! kept under a temp root is exempt where it is only shown, but not where `open` hands it to the
//! host. A relative path is tried against the project's directory and each worktree's place and is
//! reachable only when exactly one of them has it.
//!
//! A path found in a project root also says where the file viewer reads it from (`view`); a file
//! elsewhere (in a temp root) is read by [`read`]. Whether the viewer can show it is for the
//! client to find out by reading it. For a directory the view is its entry file, if it has one
//! (found by listing it): a README, else an `index.*`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::Result;
use tokio::sync::Semaphore;

use crate::protocol::{error_code, CodedError, PathLink, PathLinkKind, PathLinkView};

mod resolve;
use resolve::{own_entry, private_in_temp, reach, resolve, Refusal, Resolved, Unreached};

/// The most paths one `probe_path_links` request may ask about.
pub const MAX_PROBED_PATHS: usize = 64;

/// How many probes run at once; each can touch the file system for up to `MAX_PROBED_PATHS` paths.
pub static PROBE_PERMITS: Semaphore = Semaphore::const_new(crate::blocking_pass::BLOCKING_WIDTH);

/// How long the roots of a project are reused: finding its worktrees runs `git`.
const ROOTS_TTL: Duration = Duration::from_secs(5);

/// The temp roots. `canonical` are the roots as they resolve now; `lexical` are the spellings a
/// path may start with before it is looked at (each root as the host names it, and as resolved).
pub struct TempRoots {
    pub canonical: Vec<PathBuf>,
    pub lexical: Vec<PathBuf>,
    /// Whether ownership and write permission down to a path are checked: off only in tests that
    /// have no way to be another user.
    pub check_owner: bool,
}

/// The temp roots: the directories the host keeps scratch files in
/// ([`crate::platform::fs::temp_roots`]). A root that does not exist is left out, and so is one
/// that is the filesystem root or holds the home directory, which would make everything there a
/// scratch file.
pub fn temp_roots() -> TempRoots {
    let home = crate::paths::known_home_dir().and_then(|home| crate::hostfs::canonical(home).ok());
    let candidates = crate::platform::fs::temp_roots()
        .into_iter()
        .filter_map(|root| Some((crate::hostfs::canonical(&root).ok()?, root)))
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
    let live = crate::browse::scope::project_live_roots(state, project, &|| {
        state.host_tools.isolated_git()
    })
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
    let lookup = || cached_roots(state, project, &roots, false);
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
    let relative = crate::wire_path::RelPath::below(roots.canonical.get(index)?, &target)?;
    Some(PathLinkView {
        worktree: roots.worktrees.get(index)?.clone(),
        path: relative.to_wire(),
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

/// The roots of `project`, looked up once into `cache` however often they are asked for: afresh when
/// `fresh`, else as found within the last few seconds.
fn cached_roots(
    state: &crate::state::AppState,
    project: Option<&str>,
    cache: &OnceLock<ProjectRoots>,
    fresh: bool,
) -> ProjectRoots {
    cache
        .get_or_init(|| {
            project.map_or_else(ProjectRoots::default, |project| {
                project_roots(state, project, fresh)
            })
        })
        .clone()
}

/// The directory a project's place in `worktree` is, from the roots as found within the last few
/// seconds, for a read that runs again for every part of a file and must not run `git` for each;
/// none for a worktree not among them.
pub fn worktree_scope(
    state: &crate::state::AppState,
    project: &str,
    worktree: &str,
) -> Option<PathBuf> {
    let roots = project_roots(state, project, false);
    let index = roots
        .worktrees
        .iter()
        .position(|id| id.as_deref() == Some(worktree))?;
    roots.canonical.into_iter().nth(index)
}

/// What `path` names now, resolved as `probe` does but with the project's roots as `lookup` finds
/// them, or why it is not reachable. A launchable file is reachable here; it is for the caller to
/// refuse.
fn reached(
    state: &crate::state::AppState,
    project: Option<&str>,
    path: &str,
    temp: &TempRoots,
    lookup: &dyn Fn() -> ProjectRoots,
) -> Result<Resolved> {
    known(state, project)?;
    let home = crate::paths::known_home_dir();
    match reach(path, home.as_deref(), temp, lookup) {
        Ok(resolved) => Ok(resolved),
        Err(Unreached::Refused(refusal)) => Err(refusal.into_error(path)),
        Err(Unreached::Gone { in_project }) => Err(CodedError::raised(
            error_code::FILE_NOT_FOUND,
            format!("{path} does not exist"),
            &[
                ("path", path),
                ("root", if in_project { "project" } else { "temporary" }),
            ],
        )),
    }
}

/// Reads the file `path` names, resolved as `open` does, from the project's roots as they are now:
/// what a click on a file shows in the viewer. Anything but a file is refused, and a launchable file
/// is read like any other, since reading launches nothing.
pub fn read(
    state: &crate::state::AppState,
    project: Option<&str>,
    path: &str,
    limit: usize,
    cancel: &AtomicBool,
) -> Result<crate::browse::live::LiveFile> {
    let resolved = locate_file(state, project, path, true)?;
    // The resolved path has no link in it; opening it refuses one swapped in since.
    crate::browse::live::read_resolved(&resolved.path, path, limit, cancel)
}

/// What `path` names now, which must be a file: resolved with the project's roots looked up afresh,
/// or, with `fresh` off, as found within the last few seconds.
fn locate_file(
    state: &crate::state::AppState,
    project: Option<&str>,
    path: &str,
    fresh: bool,
) -> Result<Resolved> {
    let cache = OnceLock::new();
    let resolved = reached(state, project, path, &temp_roots(), &|| {
        cached_roots(state, project, &cache, fresh)
    })?;
    if resolved.kind != PathLinkKind::File {
        return Err(crate::browse::live::unsupported(path, "directory"));
    }
    Ok(resolved)
}

/// What `path` names now, as [`read`] resolves it but from the project's roots as probes see them
/// (found within the last few seconds), for the parts of one file's copy: each part resolves the
/// path again, and a copy of hundreds of parts must not run `git` for each. Anything but a file is
/// refused.
pub fn locate(
    state: &crate::state::AppState,
    project: Option<&str>,
    path: &str,
) -> Result<Resolved> {
    locate_file(state, project, path, false)
}

/// Opens `path` with the host's default application (a directory in its file manager), after
/// resolving it again as `probe` does, from the project's roots as they are now. A launchable
/// file is refused, and on macOS so is a folder the system types as a package; the type is asked
/// only here, where a path is about to be opened.
pub fn open(state: &crate::state::AppState, project: Option<&str>, path: &str) -> Result<()> {
    open_with(
        state,
        project,
        path,
        crate::host_open::paths::is_macos_package,
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
    let cache = OnceLock::new();
    let temp = temp_roots();
    let resolved = reached(state, project, path, &temp, &|| {
        cached_roots(state, project, &cache, true)
    })?;
    if resolved.viewer_only() {
        return Err(Refusal::Launchable.into_error(path));
    }
    if resolved.kind == PathLinkKind::Directory && is_package(&resolved.path) {
        return Err(Refusal::Launchable.into_error(path));
    }
    // A file of a project kept under a temp root was exempt where it is only shown.
    if resolved.kind == PathLinkKind::File
        && temp.check_owner
        && !private_in_temp(&resolved.path, &temp)
    {
        return Err(Refusal::NotPrivate.into_error(path));
    }
    opener(&resolved.path)
}

/// Refuses `canonical`, a path found below the project directory `root`, with `path_link_refused`
/// and the cause `launchable` when the system could launch it rather than show it; `shown` is the
/// path as the client asked for it.
pub fn refuse_launchable(root: &Path, canonical: &Path, shown: &str) -> Result<()> {
    let metadata = crate::platform::fs::stat(canonical, crate::platform::fs::Links::Follow)
        .map_err(|err| crate::browse::live::io_failure(shown, &err))?
        .metadata;
    if is_launchable(root, canonical, &metadata) {
        return Err(Refusal::Launchable.into_error(shown));
    }
    Ok(())
}

/// Whether the system could launch `canonical`, a path found below the project directory `root`
/// with `metadata` as it was opened, rather than show it.
pub fn is_launchable(root: &Path, canonical: &Path, metadata: &std::fs::Metadata) -> bool {
    resolve::launchable(canonical, root, metadata)
}

/// Shows `path` in the host's file manager, resolving it as `open` does: a file is selected in its
/// folder, a folder is opened, and a package folder (on macOS) is selected in its parent. A symbolic
/// link the path ends in is selected by its own name in its folder, not followed, when it lies below
/// the root holding it (and, in a temp root, when its folder and it are the daemon user's and its
/// folder is closed to others; a file in a project is not checked); a root itself, a link whose
/// folder is outside the roots and a path ending in `/`, `/.` or `..` are shown as what they
/// resolve to. Nothing is launched, so a file only the viewer shows is revealed too; whatever a link
/// never reaches still is refused.
pub fn reveal(state: &crate::state::AppState, project: Option<&str>, path: &str) -> Result<()> {
    reveal_with(state, project, path, |path| {
        crate::host_open::paths::open_path(path, true)
    })
}

/// [`reveal`] with what shows a path given, so that a test can see that nothing is shown.
fn reveal_with(
    state: &crate::state::AppState,
    project: Option<&str>,
    path: &str,
    shower: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    let temp = temp_roots();
    let cache = OnceLock::new();
    let lookup = || cached_roots(state, project, &cache, true);
    let resolved = reached(state, project, path, &temp, &lookup)?;
    shower(&own_entry(&resolved, &temp, &lookup).unwrap_or(resolved.path))
}

#[cfg(test)]
mod tests;
