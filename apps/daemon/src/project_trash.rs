//! Which folder removing a project may move to the Trash: never one whose loss would be drastic,
//! nor one that would take another project's files along.
//!
//! Directories are told apart by device and inode ([`Place`]), never by their paths' text, which
//! misses a directory with two real paths (a bind mount, a macOS firmlink). A match is what keeps
//! another project's files, so a miss destroys something: a path that cannot be resolved (an
//! I/O error other than a refused permission, no inode, a look that outran its deadline) counts as
//! a match. Only another project's path, the home directory's or Octoboard's data directory's,
//! with nothing at it or that cannot be read for want of permission, falls back to the text, as
//! written and as resolved; the folder to be moved itself, when it cannot be read, refuses.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Result;
use uuid::Uuid;

use crate::hostfs::place::{looked_within, Identity, Place};
use crate::hostfs::{self, lexically_normalise};
use crate::protocol::{error_code, CodedError, Params, Project};
use crate::state::AppState;

/// How long looking at one path on the file system may take. Past it the path counts as one that
/// cannot be told apart, which refuses the move: a folder on a mount that does not answer holds the
/// removal up by this much at most, and never lets it take another project's files.
const TRASH_LOOK_DEADLINE: Duration = Duration::from_secs(5);

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

/// What a project's path leads to, as moving that path to the Trash would take it.
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

/// Why a path a move to the Trash depends on could not be resolved, for the refusal's `cause`.
#[derive(Clone, Copy)]
enum Failure {
    /// The look gave no answer in time, or one at the path is still stuck (`unanswered`).
    Unanswered,
    /// Permission to read it was refused (`unreadable`).
    Unreadable,
    /// Anything else: an I/O error, no inode (no `cause`).
    Unresolved,
}

/// The device and inode `metadata` gives, or `None` where the file system reports no inode.
fn file_identity(metadata: &std::fs::Metadata) -> Option<Identity> {
    let identity = hostfs::identity(metadata);
    (identity.1 != 0).then_some(identity)
}

/// What is at another project's path, at the home directory or at Octoboard's data directory.
enum Seen {
    /// A directory, where it really is, with the place of its Git repository's common directory
    /// when it was looked for and it is in one.
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
}

/// The folder moved to the Trash, as far as the paths of what it must not take are compared with
/// it.
struct Trashed {
    entry: TrashedEntry,
    /// Its path as written and, for a directory, as resolved: what a path told only by its text is
    /// compared with.
    texts: Vec<PathBuf>,
}

impl Trashed {
    /// Whether any of `paths` is the same text as this one's or lies inside it, or (with `holding`)
    /// holds it.
    fn text_overlaps(&self, paths: &[PathBuf], holding: bool) -> bool {
        paths.iter().any(|theirs| {
            self.texts
                .iter()
                .any(|ours| theirs.starts_with(ours) || (holding && ours.starts_with(theirs)))
        })
    }

    /// Whether moving this would take `other`'s files along: they are the same directory, one
    /// lies in the other, or `other`'s repository lies in this one.
    fn takes(&self, other: &Seen) -> bool {
        match (&self.entry, other) {
            (_, Seen::Text { paths }) => self.text_overlaps(paths, true),
            (
                TrashedEntry::Directory { place: ours, .. },
                Seen::Directory {
                    place: theirs,
                    repository,
                },
            ) => {
                theirs.lies_in(ours)
                    || ours.lies_in(theirs)
                    || repository
                        .as_ref()
                        .is_some_and(|repository| repository.lies_in(ours))
            }
            (TrashedEntry::Directory { place: ours, .. }, Seen::Entry { parents, .. }) => {
                parents.iter().any(|parent| parent.lies_in(ours))
            }
            (TrashedEntry::Entry { parent, .. }, Seen::Directory { place: theirs, .. }) => {
                parent.lies_in(theirs)
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
        match (&self.entry, home) {
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
        let TrashedEntry::Directory { place: ours, .. } = &self.entry else {
            return false;
        };
        match data {
            Seen::Directory { place, .. } => place.lies_in(ours),
            Seen::Entry { parents, .. } => parents.iter().any(|parent| parent.lies_in(ours)),
            Seen::Text { paths } => self.text_overlaps(paths, false),
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

/// A refusal of a move to the Trash, kept so that it can be said again.
#[derive(Clone, Debug)]
struct Refusal {
    code: &'static str,
    message: String,
    params: Params,
    /// Whether a path that could not be resolved decided it, so that another look may decide
    /// otherwise.
    unresolved: bool,
}

impl Refusal {
    fn new(code: &'static str, message: String, pairs: &[(&str, &str)], unresolved: bool) -> Self {
        Self {
            code,
            message,
            params: pairs
                .iter()
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect(),
            unresolved,
        }
    }

    fn error(&self) -> anyhow::Error {
        anyhow::Error::new(CodedError {
            code: self.code,
            message: self.message.clone(),
            params: self.params.clone(),
        })
    }
}

/// The refusals of removal inspections that a path that could not be resolved decided, each by the
/// token its refusal carried (`inspection`), for the removal confirmed after that inspection to
/// repeat once ([`directory_to_trash`]). Held in memory only. A token goes when a removal uses it,
/// when its project is removed, or when its project holds [`KEPT_PER_PROJECT`] newer ones.
#[derive(Default)]
pub struct UnresolvedRemovals(Mutex<Kept>);

#[derive(Default)]
struct Kept {
    by_token: HashMap<String, KeptRefusal>,
    /// Numbers the refusals in the order they were kept, so the oldest of a project goes first.
    issued: u64,
}

struct KeptRefusal {
    project: String,
    refusal: Refusal,
    issued: u64,
}

/// How many kept refusals one project holds at most: one per window that inspected its removal
/// lately is plenty, and an inspection repeated without a removal must not grow the store.
const KEPT_PER_PROJECT: usize = 4;

impl UnresolvedRemovals {
    /// Drops what is kept for `project`, once it is removed.
    pub fn forget(&self, project: &str) {
        self.lock()
            .by_token
            .retain(|_, kept| kept.project != project);
    }

    /// Keeps `refusal` for `project` under a new token, which it returns, unless `project` is no
    /// longer stored: an inspection that ends after its project was removed keeps nothing. The
    /// project is checked under this store's lock, which its removal takes after deleting it, so
    /// either the check sees it gone or the removal drops what was kept.
    fn keep(&self, state: &AppState, project: &str, refusal: &Refusal) -> Option<String> {
        let mut kept = self.lock();
        if !matches!(state.store.get_project(project), Ok(Some(_))) {
            return None;
        }
        let of_project = |kept: &Kept| {
            kept.by_token
                .iter()
                .filter(|(_, entry)| entry.project == project)
                .map(|(token, entry)| (token.clone(), entry.issued))
                .collect::<Vec<_>>()
        };
        let mut held = of_project(&kept);
        held.sort_by_key(|(_, issued)| *issued);
        for (token, _) in held
            .iter()
            .take((held.len() + 1).saturating_sub(KEPT_PER_PROJECT))
        {
            kept.by_token.remove(token);
        }
        kept.issued += 1;
        let token = Uuid::new_v4().to_string();
        let issued = kept.issued;
        kept.by_token.insert(
            token.clone(),
            KeptRefusal {
                project: project.to_string(),
                refusal: refusal.clone(),
                issued,
            },
        );
        Some(token)
    }

    /// The refusal kept under `token` for `project`, which it no longer keeps.
    fn take(&self, token: &str, project: &str) -> Option<Refusal> {
        let mut kept = self.lock();
        match kept.by_token.get(token) {
            Some(entry) if entry.project == project => {
                kept.by_token.remove(token).map(|entry| entry.refusal)
            }
            _ => None,
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.lock().by_token.len()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Kept> {
        self.0.lock().expect("unresolved removals lock poisoned")
    }
}

/// Which of the paths a move to the Trash depends on did not resolve, for the refusal's words.
#[derive(Clone, Copy)]
enum Unresolved {
    Folder,
    Home,
    Data,
}

/// The directory to move to the Trash for `deleted`, as `inspect_project_removal` decides it
/// ([`judged`]). A refusal that a path that could not be resolved decided is kept, and carries the
/// token it is kept under as its `inspection` param, for the removal confirmed after this
/// inspection to repeat.
pub async fn inspected(
    state: &AppState,
    deleted: &Project,
    projects: &[Project],
) -> Result<Option<PathBuf>> {
    let home = crate::paths::known_home_dir();
    match judged(state, deleted, projects, home, crate::paths::data_dir()).await {
        Err(mut refusal) if refusal.unresolved => {
            if let Some(token) = state.unresolved_removals.keep(state, &deleted.id, &refusal) {
                refusal.params.insert("inspection".to_string(), token);
            }
            Err(refusal.error())
        }
        judged => judged.map_err(|refusal| refusal.error()),
    }
}

/// The directory to move to the Trash for `deleted`, as `delete_project` decides it. With the
/// `inspection` token of an inspection's refusal that a path that could not be resolved decided,
/// that refusal, even once the path resolves: the person confirmed a removal that was shown to take
/// nothing along, and a look now could take along what was never shown. The token is then used up.
/// Otherwise, with no token or one no longer kept, [`judged`].
pub async fn directory_to_trash(
    state: &AppState,
    deleted: &Project,
    projects: &[Project],
    inspection: Option<&str>,
) -> Result<Option<PathBuf>> {
    let kept = inspection.and_then(|token| state.unresolved_removals.take(token, &deleted.id));
    if let Some(refusal) = kept {
        return Err(refusal.error());
    }
    let home = crate::paths::known_home_dir();
    judged(state, deleted, projects, home, crate::paths::data_dir())
        .await
        .map_err(|refusal| refusal.error())
}

/// The directory to move to the Trash for `deleted`, or `None` when nothing is at its path, with
/// `home` and `data` the home directory and Octoboard's data directory. Refused for a directory
/// whose loss would be drastic (see [`PROJECT_DIRECTORY_PROTECTED`]), and while another project's
/// directory is the same, inside it or holds it, or another project's Git repository lies inside
/// it, as a linked worktree's does: trashing would take that project's files too. A non-directory
/// entry (a file, a link that leads nowhere) is trashed as itself and holds nothing, so only
/// another project holding it, or being it, refuses it.
///
/// Another project's path, the home directory's or Octoboard's data directory's, with nothing at
/// it or that cannot be read for want of permission, is compared by its text. Any other path that
/// cannot be resolved — the folder itself when it cannot be read, an I/O error, no inode, a look
/// that outran its deadline or is still stuck from an earlier one — refuses the move: as shared
/// with that project when it is another project's, and with [`PROJECT_DIRECTORY_TRASH_FAILED`]
/// when it is the folder itself, the home directory or Octoboard's data directory, its `cause`
/// `unanswered` where the look gave no answer in time and `unreadable` where permission to read
/// the folder itself was refused ([`Failure`]). The paths are looked at off the runtime
/// after the store's lock is released, a few at a time, each bounded by [`TRASH_LOOK_DEADLINE`]
/// ([`looked_within`]).
///
/// [`PROJECT_DIRECTORY_PROTECTED`]: error_code::PROJECT_DIRECTORY_PROTECTED
/// [`PROJECT_DIRECTORY_TRASH_FAILED`]: error_code::PROJECT_DIRECTORY_TRASH_FAILED
async fn judged(
    state: &AppState,
    deleted: &Project,
    projects: &[Project],
    home: Option<PathBuf>,
    data: PathBuf,
) -> Result<Option<PathBuf>, Refusal> {
    let others: Vec<&Project> = projects
        .iter()
        .filter(|other| other.id != deleted.id)
        .collect();
    let written = PathBuf::from(&deleted.path);
    let kept: Vec<PathBuf> = home.iter().cloned().chain([data.clone()]).collect();
    let (mut entry, seen, kept_seen) = tokio::join!(
        looked_within(
            &state.directory_looks,
            vec![written.clone()],
            TRASH_LOOK_DEADLINE,
            |path| Some(TrashedEntry::of(path)),
        ),
        looked_within(
            &state.directory_looks,
            others
                .iter()
                .map(|other| PathBuf::from(&other.path))
                .collect(),
            TRASH_LOOK_DEADLINE,
            |path| Some(Look::from(Seen::of(path, true))),
        ),
        looked_within(&state.directory_looks, kept, TRASH_LOOK_DEADLINE, |path| {
            Some(Look::from(Seen::of(path, false)))
        }),
    );
    let unlooked = |what: Unresolved, failure: Failure| {
        let (failed, cause) = match failure {
            Failure::Unanswered => ("did not respond in time", Some("unanswered")),
            Failure::Unreadable => ("could not be read", Some("unreadable")),
            Failure::Unresolved => ("could not be looked at", None),
        };
        let detail = match what {
            Unresolved::Folder => format!(
                "it {failed}, so whether another project's files are in it could not be told"
            ),
            Unresolved::Home => {
                format!("the home folder {failed}, so whether this is it could not be told")
            }
            Unresolved::Data => format!(
                "Octoboard's data folder {failed}, so whether it is in this folder could not be told"
            ),
        };
        let mut pairs = vec![("path", deleted.path.as_str()), ("detail", detail.as_str())];
        pairs.extend(cause.map(|cause| ("cause", cause)));
        Refusal::new(
            error_code::PROJECT_DIRECTORY_TRASH_FAILED,
            format!("could not move {} to the Trash: {detail}", deleted.path),
            &pairs,
            true,
        )
    };
    let protected = || {
        Refusal::new(
            error_code::PROJECT_DIRECTORY_PROTECTED,
            "this folder is too important to move to the Trash from here".to_string(),
            &[("path", &deleted.path)],
            false,
        )
    };
    let entry = match entry.remove(&written) {
        Some(Ok(Some(entry))) => entry,
        Some(Ok(None)) => return Ok(None),
        Some(Err(failure)) => return Err(unlooked(Unresolved::Folder, failure)),
        None => return Err(unlooked(Unresolved::Folder, Failure::Unanswered)),
    };
    let mut texts = vec![lexically_normalise(&written)];
    if let TrashedEntry::Directory { real, .. } = &entry {
        texts.push(real.clone());
    }
    let trashed = Trashed { entry, texts };

    if let TrashedEntry::Directory { disk_root, .. } = &trashed.entry {
        // A disk's root is refused for what it is, whatever else can be looked at.
        if *disk_root {
            return Err(protected());
        }
        let found = |path: &PathBuf, what: Unresolved| match kept_seen.get(path) {
            Some(Look::Found(seen)) => Ok(seen),
            Some(Look::Unresolved) => Err(unlooked(what, Failure::Unresolved)),
            None => Err(unlooked(what, Failure::Unanswered)),
        };
        let home = home
            .as_ref()
            .map(|home| found(home, Unresolved::Home))
            .transpose()?;
        let data = found(&data, Unresolved::Data)?;
        if home.is_some_and(|home| trashed.is_home(home)) || trashed.holds_data(data) {
            return Err(protected());
        }
    }

    let looked = |other: &&Project| match seen.get(Path::new(&other.path)) {
        Some(Look::Found(seen)) => Some(seen),
        _ => None,
    };
    // One known to overlap is named before one that only could not be resolved, which is in the
    // way all the same.
    if let Some((other, unresolved)) = others
        .iter()
        .copied()
        .filter_map(|other| match looked(&other) {
            Some(seen) => trashed.takes(seen).then_some((other, false)),
            None => Some((other, true)),
        })
        .min_by_key(|(_, unresolved)| *unresolved)
    {
        return Err(Refusal::new(
            error_code::PROJECT_DIRECTORY_SHARED,
            format!(
                "the folder is shared with project \"{}\"; remove that project first",
                other.name
            ),
            &[("project", &other.name)],
            unresolved,
        ));
    }
    // A symlink is trashed as itself, not as what it points to.
    Ok(Some(written))
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::sync::Arc;

    use super::*;
    use crate::protocol::{ProjectAccount, ProjectSource};
    use crate::store::LOCAL_HOST_ID;
    use crate::test_support::{app_state, git, repo_with};

    /// A project named `id` stored at `path`.
    fn project_at(id: &str, path: &Path) -> Project {
        Project {
            id: id.to_string(),
            console_id: "console-1".to_string(),
            host_id: LOCAL_HOST_ID.to_string(),
            name: id.to_string(),
            path: path.to_string_lossy().into_owned(),
            default_agent: None,
            source: ProjectSource::Local,
            remote_url: None,
            claude_account_id: ProjectAccount::Inherit,
            codex_account_id: ProjectAccount::Inherit,
            grok_account_id: ProjectAccount::Inherit,
            claude_session_model: None,
            claude_session_effort: None,
            codex_session_model: None,
            codex_session_effort: None,
            grok_session_model: None,
            grok_session_effort: None,
            trust_consent: false,
            pinned: false,
            tags: Vec::new(),
            description: None,
            git_workflow: Default::default(),
        }
    }

    /// [`judged`] as an error a request answers with.
    async fn directory_to_trash_with(
        state: &AppState,
        deleted: &Project,
        projects: &[Project],
        home: Option<PathBuf>,
        data: PathBuf,
    ) -> Result<Option<PathBuf>> {
        judged(state, deleted, projects, home, data)
            .await
            .map_err(|refusal| refusal.error())
    }

    /// The refusal `result` holds.
    fn coded(result: Result<Option<PathBuf>>) -> CodedError {
        result.unwrap_err().downcast::<CodedError>().unwrap()
    }

    /// The refusal `result` holds: its code and its message.
    fn refusal(result: Result<Option<PathBuf>>) -> (&'static str, String) {
        let err = coded(result);
        (err.code, err.message)
    }

    /// The code `directory_to_trash` refuses `deleted` with, among `others`.
    async fn trash_refusal(
        state: &AppState,
        deleted: &Project,
        others: &[Project],
    ) -> &'static str {
        refusal(directory_to_trash(state, deleted, others, None).await).0
    }

    /// Records a look at `path` as still stuck for as long as `stuck` lives, so the path counts as
    /// one whose look outran its deadline.
    fn stall(state: &AppState, path: &Path, stuck: &Arc<()>) {
        state
            .directory_looks
            .record(lexically_normalise(path), Arc::downgrade(stuck));
    }

    /// Trashing is refused for a protected directory, and over any other project's directory
    /// that is the same, inside or holding this one, however its path is spelled, whose
    /// repository is inside it, as a linked worktree's is, or that is a file inside it, and a file
    /// reached through a linked folder is the file another project is at; a sibling whose name
    /// merely starts the same, or another file, is not in the way, a directory already gone is
    /// skipped, and a folder reached through a link is trashed as the link.
    #[tokio::test]
    async fn trashing_a_directory_leaves_every_other_projects_files_alone() {
        let (state, dir) = app_state("project-trash-guard");
        let outer = dir.join("outer");
        let inner = outer.join("inner");
        let sibling = dir.join("outer-2");
        for path in [&inner, &sibling] {
            std::fs::create_dir_all(path).unwrap();
        }
        for file in [outer.join("file"), sibling.join("file")] {
            std::fs::write(file, b"").unwrap();
        }
        let linked = dir.join("linked");
        symlink(&outer, &linked).unwrap();
        let outer_project = project_at("outer", &outer);
        let inner_project = project_at("inner", &inner);
        for (deleted, other) in [
            (&outer_project, inner_project.clone()),
            (&inner_project, outer_project.clone()),
            (&outer_project, project_at("twin", &outer)),
            (&outer_project, project_at("linked", &linked)),
            (
                &outer_project,
                project_at("linked-inner", &linked.join("inner")),
            ),
            (&outer_project, project_at("file", &outer.join("file"))),
        ] {
            assert_eq!(
                trash_refusal(&state, deleted, &[deleted.clone(), other.clone()]).await,
                error_code::PROJECT_DIRECTORY_SHARED,
                "{}",
                other.path
            );
        }

        // A linked worktree elsewhere keeps its repository in the folder.
        let repo = repo_with("project-trash-repo", &[(b"a", b"a")]);
        let worktree = dir.join("worktree");
        git(
            &repo,
            &["worktree", "add", "-b", "other", worktree.to_str().unwrap()],
        );
        assert_eq!(
            trash_refusal(
                &state,
                &project_at("repo", &repo),
                &[project_at("worktree", &worktree)]
            )
            .await,
            error_code::PROJECT_DIRECTORY_SHARED
        );

        let home = crate::paths::home_dir();
        for protected in [Path::new("/"), home.as_path()] {
            assert_eq!(
                trash_refusal(&state, &project_at("protected", protected), &[]).await,
                error_code::PROJECT_DIRECTORY_PROTECTED
            );
        }

        // A file reached through a linked folder is the file another project is at.
        let through_link = project_at("through-link", &linked.join("file"));
        assert_eq!(
            trash_refusal(
                &state,
                &through_link,
                &[project_at("file", &outer.join("file"))]
            )
            .await,
            error_code::PROJECT_DIRECTORY_SHARED
        );

        let others = [
            project_at("sibling", &sibling),
            project_at("file-elsewhere", &sibling.join("file")),
        ];
        assert_eq!(
            directory_to_trash(&state, &through_link, &others, None)
                .await
                .unwrap(),
            Some(linked.join("file")),
            "another file is another entry"
        );
        assert_eq!(
            directory_to_trash(&state, &outer_project, &others, None)
                .await
                .unwrap(),
            Some(outer.clone())
        );
        assert_eq!(
            directory_to_trash(&state, &project_at("linked", &linked), &others, None)
                .await
                .unwrap(),
            Some(linked.clone()),
            "the link, not the folder it leads to"
        );
        let gone = project_at("gone", &dir.join("gone"));
        assert_eq!(
            directory_to_trash(&state, &gone, &others, None)
                .await
                .unwrap(),
            None
        );
    }

    /// One directory with two real paths is one directory: on macOS the data volume's firmlinks
    /// give everything under `/private` and `/Users` a second real path below
    /// `/System/Volumes/Data`, as a bind mount does on Linux. That holds for another project's
    /// folder, for the home directory and for Octoboard's data. Run on macOS only, where the
    /// scratch directory always has such a second path.
    #[tokio::test]
    async fn trashing_tells_a_directory_by_what_it_is_not_by_its_real_path() {
        let (state, dir) = app_state("project-trash-firmlink");
        let outer = std::fs::canonicalize(&*dir).unwrap().join("outer");
        let inner = outer.join("inner");
        std::fs::create_dir_all(&inner).unwrap();
        let Some(second) = second_real_path(&outer) else {
            if cfg!(target_os = "macos") {
                panic!("{} has no second real path", outer.display());
            }
            return;
        };
        for (deleted, other) in [
            (&outer, second.join("inner")),
            (&inner, second.clone()),
            (&outer, second.clone()),
        ] {
            let deleted = project_at("deleted", deleted);
            assert_eq!(
                trash_refusal(&state, &deleted, &[project_at("other", &other)]).await,
                error_code::PROJECT_DIRECTORY_SHARED,
                "{} beside {}",
                deleted.path,
                other.display()
            );
        }

        // The home directory, and a folder holding Octoboard's data, given by their second paths.
        let elsewhere = dir.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        let kept = project_at("kept", &outer);
        for (home, data) in [
            (second.clone(), elsewhere.clone()),
            (elsewhere.clone(), second.join("inner")),
        ] {
            let result = directory_to_trash_with(&state, &kept, &[], Some(home), data).await;
            assert_eq!(refusal(result).0, error_code::PROJECT_DIRECTORY_PROTECTED);
        }
        assert_eq!(
            directory_to_trash_with(
                &state,
                &kept,
                &[],
                Some(elsewhere.clone()),
                elsewhere.clone()
            )
            .await
            .unwrap(),
            Some(outer.clone()),
            "neither the home directory nor the data is in the folder"
        );
    }

    /// `/System/Volumes/Data` followed by `real`, where that is the same directory as `real` under
    /// another real path.
    fn second_real_path(real: &Path) -> Option<PathBuf> {
        let second = Path::new("/System/Volumes/Data").join(real.strip_prefix("/").ok()?);
        let identity = |path: &Path| std::fs::metadata(path).ok().map(|m| hostfs::identity(&m));
        let distinct = std::fs::canonicalize(&second).ok()? != std::fs::canonicalize(real).ok()?;
        (distinct && identity(&second).is_some() && identity(&second) == identity(real))
            .then_some(second)
    }

    /// A path whose look outran its deadline counts as overlapping, so the move is refused: as
    /// shared with that project when it is another's, a project known to overlap being named
    /// first, and as a failed move when it is the folder itself, the home directory or the data
    /// directory, unless the folder is a disk's root, which is protected first. Another path with
    /// nothing at it is compared by its text instead.
    #[tokio::test]
    async fn trashing_is_refused_over_a_path_whose_look_outran_its_deadline() {
        let (state, dir) = app_state("project-trash-unresolved");
        let folder = dir.join("folder");
        let elsewhere = dir.join("elsewhere");
        let home = dir.join("home");
        let data = dir.join("data");
        for path in [&folder.join("inner"), &elsewhere, &home, &data] {
            std::fs::create_dir_all(path).unwrap();
        }
        let deleted = project_at("deleted", &folder);
        let with = |others: Vec<Project>, home: &Path, data: &Path| {
            let projects: Vec<Project> = [deleted.clone()].into_iter().chain(others).collect();
            let (home, data) = (home.to_path_buf(), data.to_path_buf());
            let (state, deleted) = (&state, &deleted);
            async move { directory_to_trash_with(state, deleted, &projects, Some(home), data).await }
        };

        let missing_elsewhere = project_at("missing", &dir.join("missing"));
        assert_eq!(
            with(vec![missing_elsewhere], &dir.join("no-home"), &data)
                .await
                .unwrap(),
            Some(folder.clone()),
            "nothing at another project's path or at the home directory, outside the folder"
        );
        let missing_inside = project_at("missing", &folder.join("missing"));
        assert_eq!(
            refusal(with(vec![missing_inside], &home, &data).await).0,
            error_code::PROJECT_DIRECTORY_SHARED,
            "nothing at another project's path, inside the folder by its text"
        );
        assert_eq!(
            refusal(with(vec![], &home, &folder.join("no-data")).await).0,
            error_code::PROJECT_DIRECTORY_PROTECTED,
            "nothing at the data directory, inside the folder by its text"
        );

        let stuck = Arc::new(());
        stall(&state, &elsewhere, &stuck);
        let others = vec![
            project_at("stuck", &elsewhere),
            project_at("inner", &folder.join("inner")),
        ];
        let (code, message) = refusal(with(others, &home, &data).await);
        assert_eq!(code, error_code::PROJECT_DIRECTORY_SHARED);
        assert!(message.contains("\"inner\""), "{message}");
        let (code, message) =
            refusal(with(vec![project_at("stuck", &elsewhere)], &home, &data).await);
        assert_eq!(code, error_code::PROJECT_DIRECTORY_SHARED);
        assert!(message.contains("\"stuck\""), "{message}");

        for (stalled, what) in [(&home, "home"), (&data, "data"), (&folder, "folder")] {
            // Each held alone: the stall ends with its guard.
            let stuck = Arc::new(());
            stall(&state, stalled, &stuck);
            let err = coded(with(vec![], &home, &data).await);
            assert_eq!(
                err.code,
                error_code::PROJECT_DIRECTORY_TRASH_FAILED,
                "{what}"
            );
            assert_eq!(
                err.params.get("cause").map(String::as_str),
                Some("unanswered")
            );
            assert!(
                err.params["detail"].contains("did not respond in time"),
                "{what}"
            );
        }
        // The root of a disk is protected for what it is, whatever cannot be looked at.
        let stuck = Arc::new(());
        stall(&state, &home, &stuck);
        stall(&state, &data, &stuck);
        let root = project_at("root", Path::new("/"));
        let result = directory_to_trash_with(&state, &root, &[], Some(home.clone()), data).await;
        assert_eq!(refusal(result).0, error_code::PROJECT_DIRECTORY_PROTECTED);
    }

    /// Another project at a path that cannot be read, as a folder whose permissions refuse its
    /// owner, or macOS's privacy controls, refuse it, is compared by its text, so it neither
    /// blocks every move nor is taken along when its text lies in the folder; the folder to be
    /// moved itself, when it cannot be read, refuses with the cause `unreadable`. Skipped as root,
    /// who can read it.
    #[tokio::test]
    async fn another_project_that_cannot_be_read_is_compared_by_its_text() {
        if unsafe { libc::geteuid() } == 0 {
            eprintln!("skipped: root reads a folder whatever its permissions");
            return;
        }
        let (state, dir) = app_state("project-trash-unreadable");
        let folder = dir.join("folder");
        let locked = [folder.join("locked"), dir.join("locked")];
        for path in &locked {
            std::fs::create_dir_all(path.join("project")).unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000)).unwrap();
        }
        let deleted = project_at("deleted", &folder);
        let inside = directory_to_trash(
            &state,
            &deleted,
            &[project_at("inside", &locked[0].join("project"))],
            None,
        )
        .await;
        // The folder itself cannot be read: refused, for that cause.
        let own = directory_to_trash(
            &state,
            &project_at("own", &locked[1].join("project")),
            &[],
            None,
        )
        .await;
        let outside = directory_to_trash(
            &state,
            &deleted,
            &[project_at("outside", &locked[1].join("project"))],
            None,
        )
        .await;
        for path in &locked {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert_eq!(refusal(inside).0, error_code::PROJECT_DIRECTORY_SHARED);
        assert_eq!(outside.unwrap(), Some(folder.clone()));
        let own = coded(own);
        assert_eq!(own.code, error_code::PROJECT_DIRECTORY_TRASH_FAILED);
        assert_eq!(
            own.params.get("cause").map(String::as_str),
            Some("unreadable")
        );
        assert!(
            own.params["detail"].contains("could not be read"),
            "{:?}",
            own.params
        );
    }

    /// A removal given the token of an inspection's refusal over a path that could not be resolved
    /// repeats that refusal once the path resolves, and uses the token up; one with no token, or one
    /// no longer kept, looks for itself. A project keeps a few tokens at most, its removal takes them
    /// along, and an inspection that ends after its project is gone keeps none.
    #[tokio::test]
    async fn a_removal_repeats_the_refusal_its_unresolved_inspection_answered() {
        let (state, dir) = app_state("project-trash-kept");
        let folder = dir.join("folder");
        let elsewhere = dir.join("elsewhere");
        for path in [&folder, &elsewhere] {
            std::fs::create_dir_all(path).unwrap();
        }
        let deleted = project_at("deleted", &folder);
        state
            .store
            .insert_console(&crate::test_support::console(&dir))
            .unwrap();
        state.store.insert_project(&deleted).unwrap();
        let projects = [deleted.clone(), project_at("elsewhere", &elsewhere)];
        let removal = |token: Option<&str>| {
            let token = token.map(str::to_string);
            let (state, deleted, projects) = (&state, &deleted, &projects);
            async move { directory_to_trash(state, deleted, projects, token.as_deref()).await }
        };
        let inspect = || async {
            let stuck = Arc::new(());
            stall(&state, &elsewhere, &stuck);
            coded(inspected(&state, &deleted, &projects).await)
        };

        let inspection = inspect().await;
        assert_eq!(inspection.code, error_code::PROJECT_DIRECTORY_SHARED);
        let token = inspection.params["inspection"].as_str();
        assert_eq!(
            removal(None).await.unwrap(),
            Some(folder.clone()),
            "no token"
        );
        let repeated = coded(removal(Some(token)).await);
        assert_eq!(
            (repeated.code, &repeated.message),
            (inspection.code, &inspection.message)
        );
        assert_eq!(
            removal(Some(token)).await.unwrap(),
            Some(folder.clone()),
            "used up"
        );
        assert_eq!(
            removal(Some("unknown")).await.unwrap(),
            Some(folder.clone())
        );

        for _ in 0..KEPT_PER_PROJECT + 2 {
            inspect().await;
        }
        assert_eq!(state.unresolved_removals.len(), KEPT_PER_PROJECT);
        state.unresolved_removals.forget(&deleted.id);
        assert_eq!(state.unresolved_removals.len(), 0);

        state.store.delete_project(&deleted.id).unwrap();
        assert_eq!(inspect().await.params.get("inspection"), None);
        assert_eq!(state.unresolved_removals.len(), 0);
    }
}
