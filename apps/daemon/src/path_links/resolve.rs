//! Finding what a path from a terminal's text names, and refusing what a link must not reach: a
//! path that leaves the roots, one in a temp root that others can change, and what the system
//! could launch rather than show. This is the one place those rules are decided.

use std::path::{Component, Path, PathBuf};

use anyhow::Result;

use super::{ProjectRoots, TempRoots};
use crate::hostfs::{is_not_found, lexically_normalise, lexically_within};
use crate::protocol::{error_code, CodedError, PathLinkKind};

/// A path the terminal's text names, as it resolves.
#[derive(Debug, PartialEq, Eq)]
pub struct Resolved {
    pub path: PathBuf,
    /// The path as spelled, with its `.` and `..` folded, for [`own_entry`] to look at the entry it
    /// ends in; none when the spelling names a folder (it ends in `/`, `/.` or `..`).
    pub spelled: Option<PathBuf>,
    pub kind: PathLinkKind,
    /// What is found could be launched by the system. A file of that kind is kept to be viewed
    /// rather than refused outright; a folder of that kind is not reachable at all.
    pub launchable: bool,
    /// The project root it was found in (an index of `ProjectRoots::canonical`); none in a temp root
    /// that is not also inside a project.
    pub project_root: Option<usize>,
}

impl Resolved {
    /// Whether the host never opens this: what the system could launch rather than show, which a
    /// link shows only in the file viewer.
    pub fn viewer_only(&self) -> bool {
        self.launchable
    }
}

/// What `raw` names, if it is reachable. An absolute path or one starting with `~/` is taken as
/// written, any other is looked for under each of the project's roots. `project` finds those roots;
/// a path under a temp root asks for them to tell a project kept there from scratch space.
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
    pub(super) fn into_error(self, path: &str) -> anyhow::Error {
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
    // A spelling ending in `/`, `.` or `..` names a folder, so a link spelled that way is followed.
    let names_directory = matches!(raw.rsplit('/').next(), Some("" | "." | ".."));
    let spelled_as = |spelled: PathBuf| (!names_directory).then_some(spelled);
    // Folded lexically first, so that a `..` cannot lead `canonicalize` out of the roots the
    // spelling was checked against.
    let Some(absolute) = absolute.map(|path| lexically_normalise(&path)) else {
        let roots = project();
        let mut found: Vec<(PathBuf, PathBuf)> = Vec::new();
        let (mut spellings, mut missing) = (0, 0);
        let mut failure: Option<Refusal> = None;
        for root in &roots.canonical {
            let joined = lexically_normalise(&root.join(raw));
            if !roots
                .lexical
                .iter()
                .any(|root| lexically_within(&joined, root))
            {
                continue;
            }
            spellings += 1;
            let canonical = match crate::hostfs::canonical(&joined) {
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
                && !found.iter().any(|(seen, _)| *seen == canonical)
            {
                found.push((canonical, joined));
            }
        }
        let [(canonical, joined)] = found.as_slice() else {
            return Err(if found.len() > 1 {
                Unreached::Refused(Refusal::Ambiguous)
            } else if spellings > 0 && missing == spellings {
                Unreached::Gone { in_project: true }
            } else {
                // Nothing found: a failure looking, else every spelling led outside the roots.
                Unreached::Refused(failure.unwrap_or(Refusal::Outside))
            });
        };
        let index = roots
            .innermost(canonical)
            .ok_or(Unreached::Refused(Refusal::Outside))?;
        return in_project(
            canonical.clone(),
            spelled_as(joined.clone()),
            &roots,
            index,
            temp,
        );
    };
    if temp
        .lexical
        .iter()
        .any(|root| lexically_within(&absolute, root))
    {
        // A gone path is the project's when it is spelled inside it too, as for a project kept
        // under a temp root: the project's files are where it is looked for again.
        let canonical = canonical_or_gone(&absolute, || {
            project()
                .lexical
                .iter()
                .any(|root| lexically_within(&absolute, root))
        })?;
        let root = temp
            .canonical
            .iter()
            .find(|root| canonical.starts_with(root))
            .ok_or(Unreached::Refused(Refusal::Outside))?;
        // A project kept under a temp root (or a worktree of it) is still the project's: its files
        // are found there however they are spelled.
        let roots = project();
        if let Some(index) = roots.innermost(&canonical) {
            return in_project(canonical, spelled_as(absolute), &roots, index, temp);
        }
        return reachable(canonical, spelled_as(absolute), root, temp.check_owner)
            .map_err(Unreached::Refused);
    }
    let roots = project();
    if !roots
        .lexical
        .iter()
        .any(|root| lexically_within(&absolute, root))
    {
        return Err(Unreached::Refused(Refusal::Outside));
    }
    let canonical = canonical_or_gone(&absolute, || true)?;
    let index = roots
        .innermost(&canonical)
        .ok_or(Unreached::Refused(Refusal::Outside))?;
    in_project(canonical, spelled_as(absolute), &roots, index, temp)
}

/// `canonical`, found in the project root `index` of `roots`, as [`reachable`] judges it there. A
/// folder of a project kept under a temp root is still handed to the host opener, so the temp
/// root's rule that nobody else could have placed it holds for it however it was spelled; a file
/// of one is only shown, and `open` checks it when it is to be opened (see
/// [`super::open_with`]).
fn in_project(
    canonical: PathBuf,
    spelled: Option<PathBuf>,
    roots: &ProjectRoots,
    index: usize,
    temp: &TempRoots,
) -> Result<Resolved, Unreached> {
    let mut resolved = reachable(canonical, spelled, &roots.canonical[index], false)
        .map_err(Unreached::Refused)?;
    if resolved.kind == PathLinkKind::Directory
        && temp.check_owner
        && !private_in_temp(&resolved.path, temp)
    {
        return Err(Unreached::Refused(Refusal::NotPrivate));
    }
    resolved.project_root = Some(index);
    Ok(resolved)
}

/// `path` with its links resolved, or why not: gone when nothing is there (see [`is_not_found`]),
/// `in_project` asked only then.
fn canonical_or_gone(path: &Path, in_project: impl FnOnce() -> bool) -> Result<PathBuf, Unreached> {
    crate::hostfs::canonical(path).map_err(|err| {
        if is_not_found(&err) {
            Unreached::Gone {
                in_project: in_project(),
            }
        } else {
            Unreached::Refused(Refusal::of_io(&err))
        }
    })
}

/// `canonical`, spelled `spelled` (none when the spelling names the directory) and found inside
/// `root`, with what it is, unless it is not something to show: not a file or folder, or (with
/// `check_owner`) not solely the daemon's user's from `root` down. A folder that could be launched
/// is refused too; a file that could is kept for the viewer, wherever it lies.
fn reachable(
    canonical: PathBuf,
    spelled: Option<PathBuf>,
    root: &Path,
    check_owner: bool,
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
    if launchable && kind != PathLinkKind::File {
        return Err(Refusal::Launchable);
    }
    if check_owner && !closed_to_others(&canonical, root) {
        return Err(Refusal::NotPrivate);
    }
    Ok(Resolved {
        path: canonical,
        spelled,
        kind,
        launchable,
        project_root: None,
    })
}

/// The symbolic link `resolved` was spelled to end in, by its own name, for a reveal to select in
/// its folder instead of following it: the folder it sits in resolved, joined with its final
/// component. None unless that entry is a link leading to `resolved.path` that lies strictly below
/// the root holding it (the innermost project root, or the temp root for a path found in one only; a
/// root, or a link whose folder is outside the roots, is shown as what it resolves to) and, in a
/// temp root, is private (see [`private_in_temp`]); a file in a project is not checked there, as
/// [`in_project`] does not check it.
pub fn own_entry(
    resolved: &Resolved,
    temp: &TempRoots,
    project: &dyn Fn() -> ProjectRoots,
) -> Option<PathBuf> {
    let spelled = resolved.spelled.as_ref()?;
    let own = crate::hostfs::canonical(spelled.parent()?)
        .ok()?
        .join(spelled.file_name()?);
    let in_project = resolved.project_root.is_some();
    let root = if in_project {
        let roots = project();
        roots.canonical[roots.innermost(&own)?].clone()
    } else {
        temp.canonical
            .iter()
            .find(|root| own.starts_with(root))?
            .clone()
    };
    let below_root = own.starts_with(&root) && own != root;
    let checked = temp.check_owner && (!in_project || resolved.kind == PathLinkKind::Directory);
    (below_root
        && std::fs::symlink_metadata(&own).ok()?.is_symlink()
        && crate::hostfs::canonical(&own).ok()? == resolved.path
        && (!checked || private_in_temp(&own, temp)))
    .then_some(own)
}

/// Whether `path`, a resolved path or a link in a resolved folder, is private if it lies in a temp
/// root: every entry from the one below that root down is the daemon user's and closed to others.
/// Always so outside a temp root.
pub(super) fn private_in_temp(path: &Path, temp: &TempRoots) -> bool {
    temp.canonical
        .iter()
        .find(|root| path.starts_with(root))
        .is_none_or(|root| closed_to_others(path, root))
}

/// Whether every entry from the one below `root` down to `canonical` belongs to the daemon's user
/// and cannot be written by anyone else, as read without following links (see
/// [`crate::platform::fs::is_private_to_user`]).
fn closed_to_others(canonical: &Path, root: &Path) -> bool {
    let Ok(below) = canonical.strip_prefix(root) else {
        return false;
    };
    let mut current = root.to_path_buf();
    below.components().all(|component| {
        current.push(component);
        crate::platform::fs::is_private_to_user(&current)
    })
}

/// Whether the system could run what `canonical` names, or a bundle it sits in, rather than show it.
pub(super) fn launchable(canonical: &Path, root: &Path, metadata: &std::fs::Metadata) -> bool {
    if crate::platform::fs::is_executable(canonical) {
        return true;
    }
    let below = canonical.strip_prefix(root).unwrap_or(canonical);
    let lists = crate::platform::open::launchable_extensions();
    let has_extension = |name: &std::ffi::OsStr, list: &[String]| {
        Path::new(name).extension().is_some_and(|extension| {
            let extension = extension.to_string_lossy().to_ascii_lowercase();
            list.contains(&extension)
        })
    };
    // What follows the last dot of a file's own name, even a leading one: the system's opener takes `.bat` for a
    // batch file, though `Path::extension` says it has none.
    let ends_in = |name: &std::ffi::OsStr, list: &[String]| {
        let name = name.to_string_lossy();
        name.trim_end_matches(['.', ' '])
            .rsplit_once('.')
            .is_some_and(|(_, extension)| list.contains(&extension.to_ascii_lowercase()))
    };
    let in_bundle = below.components().any(|component| {
        matches!(component, Component::Normal(name) if has_extension(name, &lists.any_component))
    });
    in_bundle
        || (metadata.is_file()
            && below.file_name().is_some_and(|name| {
                ends_in(name, &lists.final_file)
                    || ends_in(name, &lists.any_component)
                    || ends_in(name, crate::platform::open::launched_file_extensions())
            }))
}
