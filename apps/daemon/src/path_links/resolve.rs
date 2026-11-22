//! Finding what a path from a terminal's text names, and refusing what a link must not reach: a
//! network or device path, one in a temp root that others can change, and what the system could
//! launch rather than show. This is the one place those rules are decided.

use std::path::{Component, Path, PathBuf};

use anyhow::Result;

use super::{ProjectRoots, TempRoots};
use crate::hostfs::{is_not_found, lexically_normalise, lexically_within};
use crate::platform::names::reaches_network_or_device;
use crate::protocol::{error_code, CodedError, PathLinkKind};

/// A path the terminal's text names, as it resolves.
#[derive(Debug, PartialEq, Eq)]
pub struct Resolved {
    pub path: PathBuf,
    /// The path as spelled, with its `.` and `..` folded, for [`own_entry`] to look at the entry it
    /// ends in; none when the spelling names a folder (see [`names_directory`]).
    pub spelled: Option<PathBuf>,
    pub kind: PathLinkKind,
    /// What is found could be launched by the system. A file of that kind is kept to be viewed
    /// rather than refused outright; a folder of that kind is not reachable at all.
    pub launchable: bool,
    /// The project root it was found in (an index of `ProjectRoots::canonical`); none for a path
    /// found outside the project, in a temp root or elsewhere on the host.
    pub project_root: Option<usize>,
}

impl Resolved {
    /// Whether the host never opens this: what the system could launch rather than show, which a
    /// link shows only in the file viewer.
    pub fn viewer_only(&self) -> bool {
        self.launchable
    }
}

/// What `raw` names, if it is reachable. An absolute path or one starting with `~` and a
/// separator is taken as written, wherever on the host it lies; any other is looked for from each
/// of the project's roots. `project` finds those roots.
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
    /// Nothing is there now; `was` says where it was spelled.
    Gone { was: Whereabouts },
    /// Anything else not to show, and why.
    Refused(Refusal),
}

/// Where a path that is gone was spelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Whereabouts {
    /// Inside a project's root, even one that lies under a temp root.
    Project,
    /// Inside a temp root, outside the project.
    Temporary,
    /// Anywhere else on the host.
    Elsewhere,
}

impl Whereabouts {
    /// The value of the `root` parameter a `file_not_found` error carries.
    pub(super) fn as_param(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Temporary => "temporary",
            Self::Elsewhere => "host",
        }
    }
}

/// Why a path that is not gone is not to be shown.
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
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
    /// The host did not answer a look within its deadline, or the place is known to be stuck.
    Unanswered,
    /// A path that cannot be looked up at all: empty, containing NUL, `~` below no known home, a
    /// relative path with no project root to look from, or a network or device path, which is
    /// refused before the disk is touched.
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
            Self::Ambiguous => ("ambiguous", None),
            Self::Launchable => ("launchable", None),
            Self::NotPrivate => ("not_private", None),
            Self::NotAFileOrFolder => ("not_a_file_or_folder", None),
            Self::Unreadable(detail) => ("unreadable", Some(detail)),
            Self::Unanswered => ("unanswered", None),
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
    let absolute = absolute_spelling(raw, home).map_err(|()| Unreached::Refused(Refusal::Other))?;
    let spelled_as = |spelled: PathBuf| (!names_directory(raw)).then_some(spelled);
    // Folded lexically first, so that the network and device check sees where a `..` leads.
    let Some(absolute) = absolute.map(|path| lexically_normalise(&path)) else {
        // A drive-relative spelling (`C:foo`) is no relative path: joined to a root it would replace
        // the root and resolve against the daemon's working directory.
        if Path::new(raw)
            .components()
            .any(|component| matches!(component, Component::Prefix(_)))
        {
            return Err(Unreached::Refused(Refusal::Other));
        }
        let roots = project();
        let mut found: Vec<(PathBuf, PathBuf)> = Vec::new();
        let mut missing = 0;
        let mut failure: Option<Refusal> = None;
        for root in &roots.canonical {
            let joined = lexically_normalise(&root.join(raw));
            if refused_spelling(&joined, &|| roots.clone()) {
                return Err(Unreached::Refused(Refusal::Other));
            }
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
            if !found.iter().any(|(seen, _)| *seen == canonical) {
                found.push((canonical, joined));
            }
        }
        let [(canonical, joined)] = found.as_slice() else {
            return Err(if found.len() > 1 {
                Unreached::Refused(Refusal::Ambiguous)
            } else if !roots.canonical.is_empty() && missing == roots.canonical.len() {
                Unreached::Gone {
                    was: Whereabouts::Project,
                }
            } else {
                // Nothing found: a failure looking, else no root to look from.
                Unreached::Refused(failure.unwrap_or(Refusal::Other))
            });
        };
        return placed(canonical.clone(), spelled_as(joined.clone()), &roots, temp);
    };
    if refused_spelling(&absolute, project) {
        return Err(Unreached::Refused(Refusal::Other));
    }
    let canonical = canonical_or_gone(&absolute, || {
        // A gone path is the project's when it is spelled inside it, even for a project kept under
        // a temp root: the project's files are where it is looked for again.
        if spelled_in(&absolute, &project().lexical) {
            Whereabouts::Project
        } else if spelled_in(&absolute, &temp.lexical) {
            Whereabouts::Temporary
        } else {
            Whereabouts::Elsewhere
        }
    })?;
    placed(canonical, spelled_as(absolute), &project(), temp)
}

/// Whether `spelling`, folded and absolute, is refused before the disk is touched because it
/// reaches the network or a device (see [`reaches_network_or_device`]). A spelling inside one of the
/// project's roots is looked at as it is, even on a share the project is kept on, since the daemon
/// looks at those roots already; one a relative path's join leads out of the roots into a share is
/// refused. `project` finds the roots, asked only when the spelling reaches the network.
pub(super) fn refused_as_network(spelling: &Path, project: &dyn Fn() -> ProjectRoots) -> bool {
    reaches_network_or_device(spelling) && !spelled_in(spelling, &project().lexical)
}

/// Whether `spelling`, absolute and folded, is refused before the place it names is touched: as
/// written ([`refused_as_network`]), or because a symbolic link on the way leads to such a place.
/// The links are followed one by one without ever looking at a refused place, since resolving them
/// would make the host reach out ([`crate::hostfs::links::leads_into`]).
fn refused_spelling(spelling: &Path, project: &dyn Fn() -> ProjectRoots) -> bool {
    refused_as_network(spelling, project)
        || crate::hostfs::links::leads_into(
            spelling,
            &|prefix, more| {
                // The roots are asked only for a prefix that reaches the network.
                reaches_network_or_device(prefix)
                    && refused_prefix(prefix, more, &project().lexical)
            },
            &crate::platform::names::names_local_volume,
        )
}

/// Whether `prefix`, a spelling the link walk has reached, reaches the network or a device and is
/// not allowed by the project's own `roots`: one inside a root is looked at as it is, and a folder
/// that holds a root is allowed only as a place passed through (`more` says components follow it),
/// never as where the walk ends, so a link to the folder above a root on a share is refused like
/// the same spelling written out.
pub(super) fn refused_prefix(prefix: &Path, more: bool, roots: &[PathBuf]) -> bool {
    reaches_network_or_device(prefix)
        && !spelled_in(prefix, roots)
        && !(more && roots.iter().any(|root| lexically_within(root, prefix)))
}

/// Whether `raw` names a folder: it ends in a separator the host's paths take, `.` or `..`. A link
/// spelled so is followed, and there is no entry of its own to select.
pub(super) fn names_directory(raw: &str) -> bool {
    matches!(
        raw.rsplit(std::path::is_separator).next(),
        Some("" | "." | "..")
    )
}

/// `raw` as an absolute path: written so, or from `~` and a separator the host's paths take (`~/`,
/// and `~\` on Windows). `None` for a relative one; an error for a `~` path with no known home.
pub(super) fn absolute_spelling(raw: &str, home: Option<&Path>) -> Result<Option<PathBuf>, ()> {
    let below_home = raw
        .strip_prefix('~')
        .filter(|rest| rest.starts_with(std::path::is_separator));
    if let Some(rest) = below_home {
        // Leading separators are dropped, or the join would replace the home directory.
        Ok(Some(
            home.ok_or(())?
                .join(rest.trim_start_matches(std::path::is_separator)),
        ))
    } else if Path::new(raw).is_absolute() {
        Ok(Some(PathBuf::from(raw)))
    } else {
        Ok(None)
    }
}

/// The place `raw` lies on, as text alone (nothing is read from disk), the grouping of what a probe
/// may block on alike: the deepest of these that holds the folded absolute spelling.
/// - The project root it is spelled in (the innermost of `roots.lexical` that holds it), so each
///   project is a place of its own, and a relative path is the place of the root it is joined to.
/// - The home directory with one more name (`/Users/me/nas`), so that a link under home to a mount
///   that hangs does not stall the rest of home.
/// - Otherwise its root with the first two names (`/Volumes/Backup` for everything below it).
///
/// A relative path is joined to the first project root, and one with no root to try (or `~` and a
/// separator with no home) is no place at all. After a Windows UNC prefix (server and share) the
/// same two names count; such a spelling outside a project is refused before anything is looked
/// at.
pub fn place_of(raw: &str, home: Option<&Path>, roots: &ProjectRoots) -> PathBuf {
    let absolute = match absolute_spelling(raw, home) {
        Ok(Some(absolute)) => absolute,
        Ok(None) => match roots.canonical.first() {
            Some(root) => root.join(raw),
            None => return PathBuf::new(),
        },
        Err(()) => return PathBuf::new(),
    };
    let folded = lexically_normalise(&absolute);
    let innermost = roots
        .lexical
        .iter()
        .filter(|root| lexically_within(&folded, root))
        .max_by_key(|root| root.components().count());
    if let Some(root) = innermost {
        return root.clone();
    }
    // Home plus one name, or a root and two.
    let (base, names) = match home.filter(|home| lexically_within(&folded, home)) {
        Some(home) => (home.components().count(), 1),
        None => (0, 2),
    };
    let mut place = PathBuf::new();
    let mut taken = 0;
    for (at, component) in folded.components().enumerate() {
        match component {
            Component::Normal(_) if at >= base && taken < names => {
                taken += 1;
                place.push(component);
            }
            Component::Normal(_) if at >= base => break,
            other => place.push(other),
        }
    }
    place
}

/// Whether `path`, as spelled, lies inside one of `roots`.
fn spelled_in(path: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| lexically_within(path, root))
}

/// `canonical` as it is judged where it lies: in a project root (a project kept under a temp root,
/// or a worktree of it, is still the project's, however its files are spelled), else in a temp
/// root, else anywhere else on the host, where nothing but what [`reachable`] refuses everywhere
/// keeps it from being shown.
fn placed(
    canonical: PathBuf,
    spelled: Option<PathBuf>,
    roots: &ProjectRoots,
    temp: &TempRoots,
) -> Result<Resolved, Unreached> {
    if let Some(index) = roots.innermost(&canonical) {
        return in_project(canonical, spelled, roots, index, temp);
    }
    if let Some(root) = temp
        .canonical
        .iter()
        .find(|root| canonical.starts_with(root))
    {
        return reachable(canonical, spelled, root, temp.check_owner).map_err(Unreached::Refused);
    }
    let root = top_of_tree(&canonical);
    reachable(canonical, spelled, &root, false).map_err(Unreached::Refused)
}

/// The top of the tree `canonical` lies in (`/` on Unix, the drive or share on Windows), the root a
/// path found outside the project and the temp roots is judged below: every folder on the way to it
/// counts for what could be launched.
fn top_of_tree(canonical: &Path) -> PathBuf {
    canonical
        .ancestors()
        .last()
        .unwrap_or(canonical)
        .to_path_buf()
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
/// `was` asked only then.
fn canonical_or_gone(path: &Path, was: impl FnOnce() -> Whereabouts) -> Result<PathBuf, Unreached> {
    crate::hostfs::canonical(path).map_err(|err| {
        if is_not_found(&err) {
            Unreached::Gone { was: was() }
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
/// the root holding it (the innermost project root, the temp root for a path found in one only,
/// or the top of the tree for one found elsewhere; a root is shown as what it resolves to) and,
/// in a temp root, is private (see [`private_in_temp`]); a file in a project is not checked
/// there, as [`in_project`] does not check it.
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
        // The root is where the path was found, not where the link is written: a temp root's own
        // spelling (`/tmp`) is a link outside that root, shown as what it resolves to.
        temp.canonical
            .iter()
            .find(|root| resolved.path.starts_with(root))
            .cloned()
            .unwrap_or_else(|| top_of_tree(&own))
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
