//! Whether a directory may be moved to the Trash, or a linked worktree removed, without taking
//! away what belongs to something else: never one whose loss would be drastic, nor one that would
//! take along, or leave without its folder, the files of a path the caller says must stay (another
//! project's folder, another account's directory). The refusals' wording and what each caller does
//! with an answer are the caller's: [`looked_at_for_trash`] and [`Worktrees`] only say what stands
//! in the way.
//!
//! Directories are told apart by device and inode ([`Place`]), never by their paths' text, which
//! misses a directory with two real paths (a bind mount, a macOS firmlink). A match is what keeps
//! another path's files, so a miss destroys something: a path that cannot be resolved (an I/O
//! error other than a refused permission, no inode, a look that outran its deadline) counts as in
//! the way. Only another path, the home directory's or Octoboard's data directory's, with nothing
//! at it or that cannot be read for want of permission, falls back to the text, as written and as
//! resolved; the directory to be moved itself, when it cannot be read, refuses. How a path is in
//! the way is its [`Overlap`]. The home directory, the root of a disk and a directory holding
//! Octoboard's data are protected whatever else is found.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::hostfs::place::{looked_within_stuck, Identity, Looking, Place};
use crate::hostfs::{self, lexically_normalise};
use crate::state::AppState;

/// How long looking at one path on the file system may take. Past it the path counts as one that
/// cannot be told apart, which refuses the move rather than let it take what belongs to something
/// else, and as stuck while its look is still running.
pub(crate) const TRASH_LOOK_DEADLINE: Duration = Duration::from_secs(5);

/// Whether `directory` is the root of a disk: `/`, or a mount point such as `/Volumes/Backup`,
/// which sits on a different device from its parent.
fn is_disk_root(directory: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let Some(parent) = directory.parent() else {
        return true;
    };
    match (std::fs::metadata(directory), std::fs::metadata(parent)) {
        (Ok(this), Ok(above)) => this.dev() != above.dev(),
        _ => false,
    }
}

/// What the path to be moved leads to, as moving that path to the Trash, or removing the worktree
/// there, would take it.
enum TrashedEntry {
    /// A directory, link or not, where it really is: its place, its real path, and whether it is
    /// the root of a disk.
    Directory {
        place: Place,
        real: PathBuf,
        disk_root: bool,
    },
    /// An entry that leads to no directory — a file, or a link that leads to one or nowhere —
    /// which holds nothing; it lies in the directory above it. Told apart by its own device and
    /// inode where the file system reports one.
    Entry {
        parent: Place,
        identity: Option<Identity>,
    },
}

impl TrashedEntry {
    /// What is at `path`, `None` when nothing is there. Fails as [`Failure::Unreadable`] where
    /// permission to read the path, or what a link there leads to, is refused, and as
    /// [`Failure::Unresolved`] where it cannot be told otherwise: the path is not absolute, another
    /// I/O error, or a directory that cannot be resolved (no inode). Blocking.
    fn of(path: &Path) -> Result<Option<Self>, Failure> {
        let refused = |err: &std::io::Error| {
            if err.kind() == std::io::ErrorKind::PermissionDenied {
                Failure::Unreadable
            } else {
                Failure::Unresolved
            }
        };
        let path = lexically_normalise(path);
        if !path.is_absolute() {
            return Err(Failure::Unresolved);
        }
        let own = match std::fs::symlink_metadata(&path) {
            Err(err) if hostfs::is_not_found(&err) => return Ok(None),
            Err(err) => return Err(refused(&err)),
            Ok(metadata) => metadata,
        };
        if let Some((place, real)) = Place::resolved(&path) {
            let disk_root = is_disk_root(&real);
            return Ok(Some(Self::Directory {
                place,
                real,
                disk_root,
            }));
        }
        match std::fs::metadata(&path) {
            Ok(metadata) if !metadata.is_dir() => {}
            Err(err) if hostfs::is_not_found(&err) => {}
            Err(err) => return Err(refused(&err)),
            Ok(_) => return Err(Failure::Unresolved),
        }
        let parent = path
            .parent()
            .and_then(Place::of)
            .ok_or(Failure::Unresolved)?;
        Ok(Some(Self::Entry {
            parent,
            identity: file_identity(&own),
        }))
    }
}

/// Why a path a move to the Trash, or a worktree's removal, depends on could not be resolved, for
/// the refusal's words and `cause`.
#[derive(Clone, Copy)]
pub(crate) enum Failure {
    /// The look gave no answer in time, or one at the path is still stuck (`unanswered`).
    Unanswered,
    /// Permission to read it was refused (`unreadable`).
    Unreadable,
    /// Anything else: an I/O error, no inode (no `cause`).
    Unresolved,
}

impl Failure {
    /// How the failure reads in a refusal's `detail` ("it did not respond in time") and its
    /// `cause` param, where it has one.
    pub(crate) fn described(self) -> (&'static str, Option<&'static str>) {
        match self {
            Failure::Unanswered => ("did not respond in time", Some("unanswered")),
            Failure::Unreadable => ("could not be read", Some("unreadable")),
            Failure::Unresolved => ("could not be looked at", None),
        }
    }
}

/// The device and inode `metadata` gives, or `None` where the file system reports no inode.
fn file_identity(metadata: &std::fs::Metadata) -> Option<Identity> {
    let identity = hostfs::identity(metadata);
    (identity.1 != 0).then_some(identity)
}

/// What is at another path that must keep its files, at the home directory or at Octoboard's data
/// directory.
enum Seen {
    /// A directory, where it really is, with the place of its Git repository's common directory
    /// when it was looked for ([`Overlap::looks_for_repository`]) and it is in one.
    Directory {
        place: Place,
        repository: Option<Place>,
    },
    /// Something other than a directory — a file, or a link to one — lying in the directory that
    /// holds its path and in the one that holds what it resolves to, with both those paths and the
    /// identities of the entry at its path and of what that resolves to, where the file system
    /// reports them.
    Entry {
        parents: [Place; 2],
        paths: [PathBuf; 2],
        identities: Vec<Identity>,
    },
    /// Nothing there, or something that cannot be read: only its path says where it is, as
    /// written and, where it resolves, as resolved.
    Text { paths: Vec<PathBuf> },
}

impl Seen {
    /// `None` when it cannot be told: the path is not absolute, an error other than nothing being
    /// there or a refused permission reading it, or a directory or repository that cannot be
    /// resolved. With `repository`, a directory's repository is looked for too. Blocking.
    fn of(path: &Path, repository: bool) -> Option<Self> {
        let path = lexically_normalise(path);
        if !path.is_absolute() {
            return None;
        }
        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(err)
                if hostfs::is_not_found(&err)
                    || err.kind() == std::io::ErrorKind::PermissionDenied =>
            {
                let real = std::fs::canonicalize(&path).ok();
                return Some(Self::Text {
                    paths: [path].into_iter().chain(real).collect(),
                });
            }
            Err(_) => return None,
        };
        if !metadata.is_dir() {
            let real = std::fs::canonicalize(&path).ok()?;
            let own = std::fs::symlink_metadata(&path).ok()?;
            return Some(Self::Entry {
                parents: [Place::of(path.parent()?)?, Place::of(real.parent()?)?],
                identities: [&own, &metadata]
                    .into_iter()
                    .filter_map(file_identity)
                    .collect(),
                paths: [path, real],
            });
        }
        let place = Place::of(&path)?;
        let repository = match repository
            .then(|| crate::branch_origin::repository_of(&path))
            .flatten()
        {
            Some(common_dir) => Some(Place::of(&common_dir)?),
            None => None,
        };
        Some(Self::Directory { place, repository })
    }

    /// Whether this is the directory at `place` or lies in it: a directory by its own place, an
    /// entry by the directory holding its path or the one holding what that resolves to, and one
    /// told only by its text by whether that text starts with one of `texts`, the directory's path
    /// as written and as resolved. Without a `place`, where nothing is at the directory, only a
    /// text can lie in it.
    fn lies_in(&self, place: Option<&Place>, texts: &[PathBuf]) -> bool {
        match (self, place) {
            (Self::Directory { place: ours, .. }, Some(place)) => ours.lies_in(place),
            (Self::Entry { parents, .. }, Some(place)) => {
                parents.iter().any(|parent| parent.lies_in(place))
            }
            (Self::Text { paths }, _) => paths
                .iter()
                .any(|ours| texts.iter().any(|theirs| ours.starts_with(theirs))),
            (_, None) => false,
        }
    }

    /// Whether this is a directory whose Git repository's common directory, looked for with
    /// [`Seen::of`], lies in the directory at `place`.
    fn repository_lies_in(&self, place: &Place) -> bool {
        matches!(self, Self::Directory { repository: Some(repository), .. }
            if repository.lies_in(place))
    }
}

/// The directory moved to the Trash, or the worktree removed, as far as the paths of what it must
/// not take are compared with it.
struct Trashed<'a> {
    entry: &'a TrashedEntry,
    /// Its path as written and, for a directory, as resolved: what a path told only by its text is
    /// compared with.
    texts: Vec<PathBuf>,
}

impl<'a> Trashed<'a> {
    /// What is at `written`, found to be `entry`.
    fn new(written: &Path, entry: &'a TrashedEntry) -> Self {
        let mut texts = vec![lexically_normalise(written)];
        if let TrashedEntry::Directory { real, .. } = entry {
            texts.push(real.clone());
        }
        Self { entry, texts }
    }

    /// Whether any of `paths` is the same text as this one's or lies inside it, or (with `holding`)
    /// holds it.
    fn text_overlaps(&self, paths: &[PathBuf], holding: bool) -> bool {
        paths.iter().any(|theirs| {
            self.texts
                .iter()
                .any(|ours| theirs.starts_with(ours) || (holding && ours.starts_with(theirs)))
        })
    }

    /// Whether moving or removing this would take `other`'s files along, or leave what holds it
    /// without it, as `overlap` counts: they are the same directory, one lies in the other, or
    /// `other`'s repository, where one was looked for, lies in this one.
    fn takes(&self, other: &Seen, overlap: Overlap) -> bool {
        let inside_only = overlap.inside_only();
        match (self.entry, other) {
            (_, Seen::Text { paths }) => self.text_overlaps(paths, !inside_only),
            (
                TrashedEntry::Directory { place: ours, .. },
                Seen::Directory { place: theirs, .. },
            ) => {
                theirs.lies_in(ours)
                    || (!inside_only && ours.lies_in(theirs))
                    || (overlap.looks_for_repository() && other.repository_lies_in(ours))
            }
            (TrashedEntry::Directory { place: ours, .. }, Seen::Entry { parents, .. }) => {
                parents.iter().any(|parent| parent.lies_in(ours))
            }
            (TrashedEntry::Entry { parent, .. }, Seen::Directory { place: theirs, .. }) => {
                !inside_only && parent.lies_in(theirs)
            }
            // Ours is theirs, or what theirs leads to; by text only where an identity is missing.
            (
                TrashedEntry::Entry { identity, .. },
                Seen::Entry {
                    paths, identities, ..
                },
            ) => match identity {
                Some(ours) if identities.len() == 2 => identities.contains(ours),
                _ => paths.iter().any(|theirs| self.texts.contains(theirs)),
            },
        }
    }

    /// Whether this is the home directory `home`.
    fn is_home(&self, home: &Seen) -> bool {
        match (self.entry, home) {
            (TrashedEntry::Directory { place, .. }, Seen::Directory { place: home, .. }) => {
                place.is(home)
            }
            (TrashedEntry::Directory { .. }, Seen::Text { paths }) => {
                paths.iter().any(|home| self.texts.contains(home))
            }
            _ => false,
        }
    }

    /// Whether this holds Octoboard's data directory `data`.
    fn holds_data(&self, data: &Seen) -> bool {
        let TrashedEntry::Directory { place: ours, .. } = self.entry else {
            return false;
        };
        data.lies_in(Some(ours), &self.texts)
    }

    /// Why this may not go, as [`looked_at_for_trash`] says, given what each of `others` was found
    /// to be (`seen`) and what `home` and `data` were (`kept`), a path absent from those where its
    /// look gave no answer in time.
    fn blocked(
        &self,
        others: &[(PathBuf, Overlap)],
        seen: &HashMap<PathBuf, Look<Seen>>,
        home: Option<&PathBuf>,
        data: &PathBuf,
        kept: &HashMap<PathBuf, Look<Seen>>,
    ) -> Result<(), Blocked> {
        if let TrashedEntry::Directory { disk_root, .. } = self.entry {
            // A disk's root is refused for what it is, whatever else can be looked at.
            if *disk_root {
                return Err(Blocked::Protected);
            }
            let found = |path: &PathBuf, what: Unresolved| match kept.get(path) {
                Some(Look::Found(seen)) => Ok(seen),
                Some(Look::Unresolved) => Err(Blocked::Unlooked(what, Failure::Unresolved)),
                None => Err(Blocked::Unlooked(what, Failure::Unanswered)),
            };
            let home = home.map(|home| found(home, Unresolved::Home)).transpose()?;
            let data = found(data, Unresolved::Data)?;
            if home.is_some_and(|home| self.is_home(home)) || self.holds_data(data) {
                return Err(Blocked::Protected);
            }
        }

        // One known to overlap is named before one that only could not be resolved, which is in
        // the way all the same.
        let in_the_way = others
            .iter()
            .enumerate()
            .filter_map(|(index, (path, overlap))| match seen.get(path) {
                Some(Look::Found(seen)) => self.takes(seen, *overlap).then_some((index, None)),
                Some(Look::Unresolved) => Some((index, Some(Failure::Unresolved))),
                None => Some((index, Some(Failure::Unanswered))),
            })
            .min_by_key(|(_, unresolved)| unresolved.is_some());
        match in_the_way {
            Some((index, unresolved)) => Err(Blocked::InTheWay { index, unresolved }),
            None => Ok(()),
        }
    }
}

/// How a look at a path that came back in time ended.
enum Look<T> {
    /// The path could not be resolved: it cannot be read, an I/O error, no inode, a directory or
    /// repository that cannot be resolved.
    Unresolved,
    Found(T),
}

impl<T> From<Option<T>> for Look<T> {
    fn from(found: Option<T>) -> Self {
        found.map_or(Self::Unresolved, Self::Found)
    }
}

/// Which of the paths a move to the Trash, or a worktree's removal, depends on did not resolve, for
/// the refusal's words.
#[derive(Clone, Copy)]
pub(crate) enum Unresolved {
    Folder,
    Home,
    Data,
}

/// How a path that is in the way of a move to the Trash, or of a worktree's removal, is in it.
#[derive(Clone, Copy)]
pub(crate) enum Overlap {
    /// It is the directory moved, lies inside it or holds it, or its Git repository lies inside it
    /// (as a linked worktree's does): moving would take its files, or what it holds would lose the
    /// directory. Its repository is looked for.
    Whole,
    /// It is the directory moved, lies inside it or holds it. No repository is looked for.
    Nested,
    /// It is the directory moved or lies inside it: only what the move would take along.
    Inside,
    /// It is the directory moved or lies inside it, or its Git repository lies inside it: only what
    /// the move would take along, a repository another folder works from among it. Its repository
    /// is looked for.
    InsideWithRepository,
}

impl Overlap {
    /// Whether a directory's Git repository is looked for.
    fn looks_for_repository(self) -> bool {
        matches!(self, Overlap::Whole | Overlap::InsideWithRepository)
    }

    /// Whether only what the move would take along counts, not a path holding the directory.
    fn inside_only(self) -> bool {
        matches!(self, Overlap::Inside | Overlap::InsideWithRepository)
    }
}

/// Why a directory may not be moved to the Trash, from [`looked_at_for_trash`], or a linked
/// worktree removed, from [`Worktrees::judged`].
pub(crate) enum Blocked {
    /// A path the decision depends on could not be resolved.
    Unlooked(Unresolved, Failure),
    /// The home directory, the root of a disk, or a directory holding Octoboard's data.
    Protected,
    /// The path at `index` of those given is in the way; `unresolved` is why it could not be
    /// resolved, where that and nothing known about it is what put it in the way.
    InTheWay {
        index: usize,
        unresolved: Option<Failure>,
    },
}

/// The home directory, where it is known, and Octoboard's data directory: what a directory moved
/// must not be or hold.
fn kept_paths(home: Option<&PathBuf>, data: &Path) -> Vec<PathBuf> {
    home.cloned()
        .into_iter()
        .chain([data.to_path_buf()])
        .collect()
}

/// What is at a path of [`kept_paths`], whose repository is not looked for. Blocking.
fn kept_look(path: &Path) -> Look<Seen> {
    Look::from(Seen::of(path, false))
}

/// What the look at the directory to be moved found (`None` where nothing is at it), from its
/// answer: `None` where the look gave none in time, and blocked where the directory could not be
/// resolved.
fn entry_found(
    found: Option<&Result<Option<TrashedEntry>, Failure>>,
) -> Result<Option<&TrashedEntry>, Blocked> {
    match found {
        Some(Ok(entry)) => Ok(entry.as_ref()),
        Some(Err(failure)) => Err(Blocked::Unlooked(Unresolved::Folder, *failure)),
        None => Err(Blocked::Unlooked(Unresolved::Folder, Failure::Unanswered)),
    }
}

/// Whether the directory `written` may be moved to the Trash given the `others` paths that must
/// keep their files, with `home` and `data` the home directory and Octoboard's data directory:
/// `Ok(None)` where nothing is at `written`, `Ok(Some(path))` where it may be moved, as itself
/// where it is a link.
///
/// Blocked as [`Blocked::Protected`] for the home directory, the root of a disk or a directory
/// holding Octoboard's data, and as [`Blocked::InTheWay`] while another path is, in the way its
/// [`Overlap`] says, the directory moved: moving would take that path's files too. A non-directory
/// entry (a file, a link that leads nowhere) is moved as itself and holds nothing, so only
/// another path holding it, or being it, blocks it.
///
/// Another path, the home directory's or Octoboard's data directory's, with nothing at it or that
/// cannot be read for want of permission, is compared by its text. Any other path that cannot be
/// resolved — `written` itself when it cannot be read, an I/O error, no inode, a look that outran
/// its deadline or is still stuck from an earlier one — blocks the move: as
/// [`Blocked::Unlooked`] when it is `written`, the home directory or Octoboard's data directory,
/// and as [`Blocked::InTheWay`] with the `unresolved` failure when it is another path. One known
/// to overlap is named before one that only could not be resolved. The paths are looked at off the
/// runtime, a few at a time ([`looked_within_stuck`]), as `looking` says: how long each is waited
/// for ([`TRASH_LOOK_DEADLINE`] for a move, shorter for a look ahead of one) and what a path whose
/// earlier look is still stuck gets.
pub(crate) async fn looked_at_for_trash(
    state: &AppState,
    looking: Looking,
    written: &Path,
    others: &[(PathBuf, Overlap)],
    home: Option<PathBuf>,
    data: PathBuf,
) -> Result<Option<PathBuf>, Blocked> {
    let written = written.to_path_buf();
    let kept = kept_paths(home.as_ref(), &data);
    let with_repository: HashSet<PathBuf> = others
        .iter()
        .filter(|(_, overlap)| overlap.looks_for_repository())
        .map(|(path, _)| path.clone())
        .collect();
    let (entry, seen, kept_seen) = tokio::join!(
        looked_within_stuck(
            &state.directory_looks,
            vec![written.clone()],
            looking,
            |path| Some(TrashedEntry::of(path)),
        ),
        looked_within_stuck(
            &state.directory_looks,
            others.iter().map(|(path, _)| path.clone()).collect(),
            looking,
            move |path| Some(Look::from(Seen::of(path, with_repository.contains(path)))),
        ),
        looked_within_stuck(&state.directory_looks, kept, looking, |path| {
            Some(kept_look(path))
        }),
    );
    let Some(entry) = entry_found(entry.get(&written))? else {
        return Ok(None);
    };
    Trashed::new(&written, entry).blocked(others, &seen, home.as_ref(), &data, &kept_seen)?;
    Ok(Some(written))
}

/// What one path of [`Worktrees::look`] was found to be, in each role it was looked at for.
struct Looked {
    root: Option<Result<Option<TrashedEntry>, Failure>>,
    other: Option<Look<Seen>>,
    kept: Option<Look<Seen>>,
}

/// Linked worktrees a cleanup may remove, each judged as [`looked_at_for_trash`] judges a
/// directory against the same other paths that must keep their files, the home directory and
/// Octoboard's data directory, with each of those paths looked at once however many worktrees are
/// judged.
///
/// Another path is in the way of a worktree as [`Overlap::InsideWithRepository`] says: being the
/// worktree, lying inside it or having its Git repository inside it, never holding it, as a
/// project's folder holds the worktrees laid out under it. A root that leads to no directory — a
/// file, a link to one or to nothing — cannot be told, and blocks as [`Blocked::Unlooked`]. A
/// root with nothing at it holds no files, though git still takes its record: it is blocked only
/// by another path told by its text that lies in it, and a path that cannot be resolved does not
/// block it.
pub(crate) struct Worktrees {
    others: Vec<(PathBuf, Overlap)>,
    home: Option<PathBuf>,
    data: PathBuf,
    /// Whether `others`, `home` and `data` have been looked at.
    others_looked: bool,
    roots_looked: HashSet<PathBuf>,
    roots: HashMap<PathBuf, Result<Option<TrashedEntry>, Failure>>,
    seen: HashMap<PathBuf, Look<Seen>>,
    kept: HashMap<PathBuf, Look<Seen>>,
}

impl Worktrees {
    /// For the `others` paths that must keep their files, with `home` and `data` the home
    /// directory and Octoboard's data directory, none of them looked at yet.
    pub(crate) fn new(others: Vec<PathBuf>, home: Option<PathBuf>, data: PathBuf) -> Self {
        Self {
            others: others
                .into_iter()
                .map(|path| (path, Overlap::InsideWithRepository))
                .collect(),
            home,
            data,
            others_looked: false,
            roots_looked: HashSet::new(),
            roots: HashMap::new(),
            seen: HashMap::new(),
            kept: HashMap::new(),
        }
    }

    /// Looks at the worktree roots `roots` not looked at yet and, the first time, at the other
    /// paths, the home directory and Octoboard's data directory, as `looking` says
    /// ([`looked_within_stuck`]). Nothing is looked at without a root to look at.
    pub(crate) async fn look(
        &mut self,
        state: &AppState,
        looking: Looking,
        roots: impl IntoIterator<Item = PathBuf>,
    ) {
        let as_root: HashSet<PathBuf> = roots
            .into_iter()
            .filter(|root| !self.roots_looked.contains(root))
            .collect();
        if as_root.is_empty() {
            return;
        }
        let (as_other, as_kept): (HashSet<PathBuf>, HashSet<PathBuf>) = if self.others_looked {
            Default::default()
        } else {
            (
                self.others.iter().map(|(path, _)| path.clone()).collect(),
                kept_paths(self.home.as_ref(), &self.data)
                    .into_iter()
                    .collect(),
            )
        };
        self.others_looked = true;
        self.roots_looked.extend(as_root.iter().cloned());
        let paths: Vec<PathBuf> = as_root
            .iter()
            .chain(&as_other)
            .chain(&as_kept)
            .cloned()
            .collect();
        let found = looked_within_stuck(&state.directory_looks, paths, looking, move |path| {
            Some(Looked {
                root: as_root.contains(path).then(|| TrashedEntry::of(path)),
                other: as_other
                    .contains(path)
                    .then(|| Look::from(Seen::of(path, true))),
                kept: as_kept.contains(path).then(|| kept_look(path)),
            })
        })
        .await;
        for (path, looked) in found {
            if let Some(root) = looked.root {
                self.roots.insert(path.clone(), root);
            }
            if let Some(other) = looked.other {
                self.seen.insert(path.clone(), other);
            }
            if let Some(kept) = looked.kept {
                self.kept.insert(path, kept);
            }
        }
    }

    /// Whether the worktree at `root`, looked at through [`Worktrees::look`], may be removed; a
    /// root that was not looked at counts as one whose look gave no answer in time.
    pub(crate) fn judged(&self, root: &Path) -> Result<(), Blocked> {
        let entry = match entry_found(self.roots.get(root))? {
            Some(entry @ TrashedEntry::Directory { .. }) => entry,
            Some(TrashedEntry::Entry { .. }) => {
                return Err(Blocked::Unlooked(Unresolved::Folder, Failure::Unresolved))
            }
            None => {
                let texts = [lexically_normalise(root)];
                let lies_in_root = |path: &PathBuf| match self.seen.get(path) {
                    Some(Look::Found(seen)) => seen.lies_in(None, &texts),
                    _ => false,
                };
                return match self.others.iter().position(|(path, _)| lies_in_root(path)) {
                    Some(index) => Err(Blocked::InTheWay {
                        index,
                        unresolved: None,
                    }),
                    None => Ok(()),
                };
            }
        };
        Trashed::new(root, entry).blocked(
            &self.others,
            &self.seen,
            self.home.as_ref(),
            &self.data,
            &self.kept,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{app_state, git, repo_with};

    /// A directory whose linked worktree lives elsewhere keeps its repository in it: that is in
    /// the way for [`Overlap::Whole`], which looks for the repository, and not for
    /// [`Overlap::Nested`], which does not.
    #[tokio::test]
    async fn only_a_whole_overlap_looks_for_the_repository() {
        let (state, dir) = app_state("trash-safety-overlap");
        let repo = repo_with("trash-safety-repo", &[(b"a", b"a")]);
        let worktree = dir.join("worktree");
        git(
            &repo,
            &["worktree", "add", "-b", "other", worktree.to_str().unwrap()],
        );
        let home = Some(dir.join("no-home"));
        let data = dir.join("no-data");
        for (overlap, blocked) in [(Overlap::Whole, true), (Overlap::Nested, false)] {
            let result = looked_at_for_trash(
                &state,
                Looking::within(TRASH_LOOK_DEADLINE),
                &repo,
                &[(worktree.clone(), overlap)],
                home.clone(),
                data.clone(),
            )
            .await;
            assert_eq!(
                matches!(
                    result,
                    Err(Blocked::InTheWay {
                        unresolved: None,
                        ..
                    })
                ),
                blocked
            );
        }
    }
}
