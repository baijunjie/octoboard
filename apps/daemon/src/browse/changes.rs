//! A worktree's uncommitted changes, scoped to a project, and the diff of one of them.
//!
//! The listing is one `git status` over the whole worktree, so the index is read once and every
//! entry in it is from that one reading, and so a rename into or out of the project's directory is
//! found against the rest of the repository. Only what touches the project is kept: an entry with
//! either side inside its scope. A side outside it is reported by its repository path alone, and
//! nothing of its content is ever read or returned.
//!
//! A diff is read afresh at the paths it names, from its group's two sources — `HEAD` and the index
//! for a staged change, the index and the disk for an unstaged one — never from the disk in place
//! of the index or a commit. The commit is pinned by its id before anything is read, and blob ids
//! never change meaning, so what can still move is the index entry and the file on disk: both are
//! looked at before and after the patch is made, and a reply whose patch may have been made from
//! another version than the sides it reports is refused as `source_changed` rather than returned.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use anyhow::Result;

use crate::browse::blob::{self, IndexEntry};
use crate::browse::git::{paths_nest, GitEnv, GitError};
use crate::browse::source::{self, ProjectRoot};
use crate::browse::wire_path::{self, RelPath};
use crate::browse::{budget, git_error, git_failed, json_escaped_len, live, Located};
use crate::protocol::{
    error_code, ChangeEntry, ChangeGroup, ChangeRef, ChangeSide, CodedError, ConflictKind,
    ContentSource, SideKind, SideRead, SideRef,
};
use crate::subprocess::BoundedFailure;

/// A worktree's changes that touch the project, in `git status`'s order.
#[derive(Debug)]
pub struct ChangeList {
    /// The commit at `HEAD` the staged changes are against; `None` before the first commit.
    pub head: Option<String>,
    pub changes: Vec<ChangeEntry>,
    /// False when the list was cut at a budget.
    pub complete: bool,
}

/// Lists the uncommitted changes of the worktree `at` names that touch the project's scope.
pub fn list(env: &GitEnv, at: &Located, cancel: &AtomicBool) -> Result<ChangeList> {
    let args: &[&[u8]] = &[
        b"--porcelain=v2",
        b"-z",
        b"--branch",
        b"--no-ahead-behind",
        b"--untracked-files=all",
        b"--renames",
        b"--ignore-submodules=dirty",
    ];
    let stdout = match env.run(
        &at.root,
        "status",
        args,
        budget::CHANGE_STATUS_STDOUT,
        cancel,
    ) {
        Ok(stdout) => stdout,
        Err(err) => return Err(git_error(err)),
    };
    let scope = &at.repository.scope;
    let mut live = LiveSides::new(source::scope_root(&at.root, scope).ok());
    let mut list = ChangeList {
        head: None,
        changes: Vec::new(),
        complete: true,
    };
    let mut path_bytes = 0;
    let mut fields = stdout.split(|&b| b == 0).filter(|f| !f.is_empty());
    while let Some(field) = fields.next() {
        let entries = match field.first() {
            Some(b'#') => {
                if let Some(oid) = field.strip_prefix(b"# branch.oid ") {
                    list.head = (oid != b"(initial)").then(|| String::from_utf8_lossy(oid).into());
                }
                continue;
            }
            Some(b'1') => ordinary(field, &list.head, at, &mut live)?,
            Some(b'2') => {
                let original = fields.next().unwrap_or_default();
                renamed(field, original, &list.head, at, &mut live)?
            }
            Some(b'u') => conflicted(field, scope)?,
            Some(b'?') => untracked(&field[2..], scope, &mut live),
            _ => Vec::new(),
        };
        for entry in entries {
            let bytes = entry_path_bytes(&entry);
            if list.changes.len() == budget::MAX_CHANGE_ENTRIES
                || path_bytes + bytes > budget::MAX_CHANGE_PATH_BYTES
            {
                list.complete = false;
                return Ok(list);
            }
            path_bytes += bytes;
            list.changes.push(entry);
        }
    }
    Ok(list)
}

/// The fields of a `git status --porcelain=v2` record of `kind`, split at its first `count`
/// spaces; the last one is its path, which may hold spaces itself.
fn record_fields(record: &[u8], count: usize) -> Result<Vec<&[u8]>> {
    let fields: Vec<&[u8]> = record.splitn(count + 1, |&b| b == b' ').collect();
    if fields.len() != count + 1 {
        let shown = String::from_utf8_lossy(record);
        return Err(git_failed(&format!(
            "git status printed `{shown}`, which is not a record it writes"
        )));
    }
    Ok(fields)
}

/// `1 XY sub mH mI mW hH hI path`: a path changed in place, staged (`X`), unstaged (`Y`) or both.
fn ordinary(
    record: &[u8],
    head: &Option<String>,
    at: &Located,
    live: &mut LiveSides,
) -> Result<Vec<ChangeEntry>> {
    let f = record_fields(record, 8)?;
    let (xy, modes, oids, path) = (f[1], [f[3], f[4], f[5]], [f[6], f[7]], f[8]);
    let Some(rel) = in_scope(&at.repository.scope, path) else {
        return Ok(Vec::new());
    };
    let mut entries = Vec::new();
    if xy[0] != b'.' {
        entries.push(ChangeEntry::Staged {
            old: head_side(head, modes[0], oids[0], &rel),
            new: index_side(&at.worktree, modes[1], oids[1], &rel),
        });
    }
    if xy[1] != b'.' {
        entries.push(ChangeEntry::Unstaged {
            old: index_side(&at.worktree, modes[1], oids[1], &rel),
            new: live.tracked(&rel, modes[2]),
        });
    }
    Ok(entries)
}

/// `2 XY sub mH mI mW hH hI Xscore path` followed by the path it came from: a rename between
/// `HEAD` and the index (`X` is `R`), or — for a path added with intent to add — between the index
/// and the disk (`Y` is `R`). A copy is listed as what it adds, since its source is unchanged.
fn renamed(
    record: &[u8],
    original: &[u8],
    head: &Option<String>,
    at: &Located,
    live: &mut LiveSides,
) -> Result<Vec<ChangeEntry>> {
    let f = record_fields(record, 9)?;
    let (xy, modes, oids, path) = (f[1], [f[3], f[4], f[5]], [f[6], f[7]], f[9]);
    let scope = &at.repository.scope;
    let (rel, from) = (in_scope(scope, path), in_scope(scope, original));
    let mut entries = Vec::new();
    let outside = |path: &[u8]| ChangeSide::OutOfScope {
        repository_path: wire_path::encode(path),
    };
    match xy[0] {
        b'R' if rel.is_some() || from.is_some() => entries.push(ChangeEntry::Staged {
            old: match &from {
                Some(from) => head_side(head, modes[0], oids[0], from),
                None => outside(original),
            },
            new: match &rel {
                Some(rel) => index_side(&at.worktree, modes[1], oids[1], rel),
                None => outside(path),
            },
        }),
        b'C' => {
            if let Some(rel) = &rel {
                entries.push(ChangeEntry::Staged {
                    old: ChangeSide::Absent,
                    new: index_side(&at.worktree, modes[1], oids[1], rel),
                });
            }
        }
        _ => {}
    }
    match xy[1] {
        b'.' => {}
        // A rename on disk, between a path in the index and one added with intent to add. Across
        // the project's boundary, only its half inside is listed, as the deletion or the addition
        // it is there; unpaired, it is read with no patch of the other half.
        b'R' => match (&from, &rel) {
            (Some(from), Some(rel)) => entries.push(ChangeEntry::Unstaged {
                old: index_side(&at.worktree, modes[1], oids[1], from),
                new: live.tracked(rel, modes[2]),
            }),
            (Some(from), None) => entries.push(ChangeEntry::Unstaged {
                old: index_side(&at.worktree, modes[1], oids[1], from),
                new: ChangeSide::Absent,
            }),
            (None, Some(rel)) => entries.push(ChangeEntry::Unstaged {
                old: ChangeSide::Absent,
                new: live.tracked(rel, modes[2]),
            }),
            (None, None) => {}
        },
        _ => {
            if let Some(rel) = &rel {
                entries.push(ChangeEntry::Unstaged {
                    old: index_side(&at.worktree, modes[1], oids[1], rel),
                    new: live.tracked(rel, modes[2]),
                });
            }
        }
    }
    Ok(entries)
}

/// `u XY sub m1 m2 m3 mW h1 h2 h3 path`: a path with conflicting stages.
fn conflicted(record: &[u8], scope: &RelPath) -> Result<Vec<ChangeEntry>> {
    let f = record_fields(record, 10)?;
    let Some(rel) = in_scope(scope, f[10]) else {
        return Ok(Vec::new());
    };
    let conflict = match f[1] {
        b"DD" => ConflictKind::BothDeleted,
        b"AU" => ConflictKind::AddedByUs,
        b"UD" => ConflictKind::DeletedByThem,
        b"UA" => ConflictKind::AddedByThem,
        b"DU" => ConflictKind::DeletedByUs,
        b"AA" => ConflictKind::BothAdded,
        _ => ConflictKind::BothModified,
    };
    Ok(vec![ChangeEntry::Conflicted {
        path: rel.to_wire(),
        conflict,
    }])
}

/// `? path`: a file Git does not track. A directory, written with a trailing `/`, is a repository
/// of its own inside this one, which Git would add as a submodule.
fn untracked(path: &[u8], scope: &RelPath, live: &mut LiveSides) -> Vec<ChangeEntry> {
    let (path, nested) = match path.strip_suffix(b"/") {
        Some(path) => (path, true),
        None => (path, false),
    };
    let Some(rel) = in_scope(scope, path) else {
        return Vec::new();
    };
    let kind = nested.then_some(SideKind::Submodule);
    match live.side(&rel, kind) {
        ChangeSide::Absent => Vec::new(),
        new => vec![ChangeEntry::Untracked {
            old: ChangeSide::Absent,
            new,
        }],
    }
}

/// `path`, relative to the repository's root, as a path relative to the project's directory —
/// `None` when it is not inside it. The scope directory itself is not a path inside the project.
fn in_scope(scope: &RelPath, path: &[u8]) -> Option<RelPath> {
    if scope.is_root() {
        return RelPath::from_bytes(path.to_vec()).filter(|rel| !rel.is_root());
    }
    let rest = path.strip_prefix(scope.as_bytes())?.strip_prefix(b"/")?;
    RelPath::from_bytes(rest.to_vec()).filter(|rel| !rel.is_root())
}

/// What an entry of mode `mode` is, `None` for a mode of zeros: nothing there. A directory
/// (`040000`, met only where a change names one) is no side of a change either.
fn kind_of(mode: &[u8]) -> Option<SideKind> {
    match mode {
        b"000000" | b"040000" => None,
        b"120000" => Some(SideKind::Symlink),
        b"160000" => Some(SideKind::Submodule),
        _ => Some(SideKind::File),
    }
}

fn head_side(head: &Option<String>, mode: &[u8], oid: &[u8], rel: &RelPath) -> ChangeSide {
    match (head, kind_of(mode)) {
        (Some(commit), Some(kind)) => ChangeSide::Present {
            path: rel.to_wire(),
            kind,
            source: ContentSource::Commit {
                commit: commit.clone(),
                branch: None,
                blob: String::from_utf8_lossy(oid).into(),
            },
        },
        _ => ChangeSide::Absent,
    }
}

fn index_side(worktree: &str, mode: &[u8], oid: &[u8], rel: &RelPath) -> ChangeSide {
    match kind_of(mode) {
        Some(kind) => ChangeSide::Present {
            path: rel.to_wire(),
            kind,
            source: ContentSource::Index {
                worktree: worktree.to_string(),
                blob: String::from_utf8_lossy(oid).into(),
            },
        },
        None => ChangeSide::Absent,
    }
}

/// How many path bytes `entry` puts on the wire, counted as JSON writes them.
fn entry_path_bytes(entry: &ChangeEntry) -> usize {
    let len = |path: &String| json_escaped_len(path.as_bytes());
    let side = |side: &ChangeSide| match side {
        ChangeSide::Present { path, .. } => len(path),
        ChangeSide::OutOfScope { repository_path } => len(repository_path),
        ChangeSide::Absent => 0,
    };
    match entry {
        ChangeEntry::Staged { old, new }
        | ChangeEntry::Unstaged { old, new }
        | ChangeEntry::Untracked { old, new } => side(old) + side(new),
        ChangeEntry::Conflicted { path, .. } => len(path),
    }
}

/// The files on disk under the project's scope in one worktree, as the sides of its changes. A
/// path is looked at only where it is reached through no symbolic link, as `git` itself looks at
/// a worktree: what lies beyond a link is, to `git`, not there, and is never looked at here.
struct LiveSides {
    scope: Option<ProjectRoot>,
    /// Whether each directory holding a listed path is reached through no link, asked once each.
    exact_dirs: HashMap<PathBuf, bool>,
}

impl LiveSides {
    fn new(scope: Option<ProjectRoot>) -> Self {
        Self {
            scope,
            exact_dirs: HashMap::new(),
        }
    }

    /// `rel` on disk, with no link on the way to it; `None` when it cannot be reached that way.
    fn exact_path(&mut self, rel: &RelPath) -> Option<PathBuf> {
        let scope = self.scope.as_ref()?;
        exact_path(scope, rel, &mut self.exact_dirs)
    }

    /// The disk side of a tracked path whose mode on disk `git status` gave as `mode`: absent
    /// where `git` found nothing, whatever may have appeared there since.
    fn tracked(&mut self, rel: &RelPath, mode: &[u8]) -> ChangeSide {
        match kind_of(mode) {
            Some(kind) => self.side(rel, Some(kind)),
            None => ChangeSide::Absent,
        }
    }

    /// The side the file on disk at `rel` is: present with its version, of `kind` as `git` saw it
    /// (or, with none given, as the disk says), or absent when nothing is there now.
    fn side(&mut self, rel: &RelPath, kind: Option<SideKind>) -> ChangeSide {
        let Some(path) = self.exact_path(rel) else {
            return ChangeSide::Absent;
        };
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            return ChangeSide::Absent;
        };
        let kind = kind.unwrap_or(if metadata.file_type().is_symlink() {
            SideKind::Symlink
        } else if metadata.is_dir() {
            SideKind::Submodule
        } else {
            SideKind::File
        });
        let root_id = self
            .scope
            .as_ref()
            .map(|s| s.id.clone())
            .unwrap_or_default();
        ChangeSide::Present {
            path: rel.to_wire(),
            kind,
            source: ContentSource::Live {
                root_id,
                version: live::version(&metadata),
            },
        }
    }
}

/// `rel` joined onto `scope`, when the directory holding it is exactly that directory, reached
/// through no symbolic link; `dirs` remembers the directories already asked about.
fn exact_path(
    scope: &ProjectRoot,
    rel: &RelPath,
    dirs: &mut HashMap<PathBuf, bool>,
) -> Option<PathBuf> {
    let joined = scope.canonical.join(rel.as_path());
    let parent = joined.parent()?.to_path_buf();
    let exact = *dirs
        .entry(parent.clone())
        .or_insert_with(|| fs::canonicalize(&parent).is_ok_and(|real| real == parent));
    exact.then_some(joined)
}

/// One change as read for its diff.
#[derive(Debug)]
pub struct ChangeRead {
    /// The commit a staged change was read against; `None` before the first commit and for the
    /// other groups.
    pub head: Option<String>,
    pub old: SideRead,
    pub new: SideRead,
    /// The unified patch as `git` writes it; `None` when none is made (an untracked file, a side
    /// outside the project).
    pub patch: Option<Vec<u8>>,
}

/// What one side holds now, before its body is read.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Found {
    /// In the index or a commit: its mode and object id.
    Object {
        kind: SideKind,
        oid: String,
    },
    /// On disk: its kind and version.
    Disk {
        kind: SideKind,
        version: String,
    },
    Absent,
}

/// Reads the change `change` names in the worktree `at` names: both sides from its group's
/// sources, and its patch — see the module doc.
pub fn read(
    env: &GitEnv,
    at: &Located,
    change: &ChangeRef,
    cancel: &AtomicBool,
) -> Result<ChangeRead> {
    let (old, new) = (side_path(&change.old)?, side_path(&change.new)?);
    let out_of_scope =
        matches!(change.old, SideRef::OutOfScope) || matches!(change.new, SideRef::OutOfScope);
    let valid = match change.group {
        ChangeGroup::Staged => old.is_some() || new.is_some(),
        ChangeGroup::Unstaged => !out_of_scope && (old.is_some() || new.is_some()),
        ChangeGroup::Untracked => matches!(change.old, SideRef::Absent) && new.is_some(),
    };
    if !valid {
        return Err(CodedError::raised(
            error_code::INVALID_CHANGE,
            "the change names no side to read in its group",
            &[],
        ));
    }
    // A side named absent is the same path as the other, for the change it is part of: an
    // addition's old side is whatever its source holds at the added path now.
    let old_path = old.clone().or_else(|| new.clone());
    let new_path = new.or(old);
    let reader = Reader { env, at, cancel };
    match change.group {
        ChangeGroup::Staged => reader.staged(change, old_path, new_path, out_of_scope),
        ChangeGroup::Unstaged => reader.unstaged(old_path.unwrap(), new_path.unwrap()),
        ChangeGroup::Untracked => reader.untracked(new_path.unwrap()),
    }
}

/// The project-relative path a side names, when it names one.
fn side_path(side: &SideRef) -> Result<Option<RelPath>> {
    let SideRef::Present { path } = side else {
        return Ok(None);
    };
    match RelPath::parse(path).filter(|rel| !rel.is_root()) {
        Some(rel) => Ok(Some(rel)),
        None => Err(CodedError::raised(
            error_code::INVALID_PATH,
            format!("{path} is not a path inside a project"),
            &[("path", path)],
        )),
    }
}

struct Reader<'a> {
    env: &'a GitEnv,
    at: &'a Located,
    cancel: &'a AtomicBool,
}

impl Reader<'_> {
    fn full(&self, rel: &RelPath) -> RelPath {
        self.at.repository.scope.join(rel)
    }

    /// The index entries at `paths` (project-relative), refusing a path in conflict: its stages are
    /// not one side of anything.
    fn index(&self, paths: &[&RelPath]) -> Result<Vec<IndexEntry>> {
        let full: Vec<RelPath> = paths.iter().map(|rel| self.full(rel)).collect();
        let args: Vec<&[u8]> = full.iter().map(RelPath::as_bytes).collect();
        let entries = blob::index_entries(self.env, &self.at.root, &args, self.cancel)?;
        if let Some(entry) = entries.iter().find(|entry| entry.stage != b"0") {
            let rel = paths[full
                .iter()
                .position(|f| f.as_bytes() == entry.path)
                .unwrap_or(0)];
            return Err(live::unsupported(rel, "unmerged"));
        }
        Ok(entries)
    }

    fn in_index(&self, entries: &[IndexEntry], rel: &RelPath) -> Found {
        let full = self.full(rel);
        entries
            .iter()
            .find(|entry| entry.path == full.as_bytes())
            .and_then(|entry| {
                kind_of(&entry.mode).map(|kind| Found::Object {
                    kind,
                    oid: entry.oid.clone(),
                })
            })
            .unwrap_or(Found::Absent)
    }

    fn in_commit(&self, commit: Option<&str>, rel: &RelPath) -> Result<Found> {
        let Some(commit) = commit else {
            return Ok(Found::Absent);
        };
        let found = blob::tree_entry(
            self.env,
            &self.at.root,
            commit,
            self.full(rel).as_bytes(),
            self.cancel,
        )?;
        Ok(
            match found.and_then(|(mode, oid)| kind_of(&mode).map(|kind| (kind, oid))) {
                Some((kind, oid)) => Found::Object { kind, oid },
                None => Found::Absent,
            },
        )
    }

    /// The commit at `HEAD`, `None` before the first one: when `HEAD` names a branch that does
    /// not exist yet. A `HEAD` that resolves to nothing otherwise — detached at a missing object, or
    /// naming a branch that is there but broken — is a failure, not a baseline.
    fn head(&self) -> Result<Option<String>> {
        let limit = budget::GIT_METADATA_STDOUT;
        let run = |subcommand: &str, args: &[&[u8]]| {
            self.env
                .run(&self.at.root, subcommand, args, limit, self.cancel)
        };
        let unresolved = match run("rev-parse", &[b"--verify", b"--quiet", b"HEAD^{commit}"]) {
            Ok(stdout) => return Ok(Some(String::from_utf8_lossy(&stdout).trim().to_string())),
            Err(GitError::Failed(_)) => || git_failed("HEAD names no commit"),
            Err(err) => return Err(git_error(err)),
        };
        let branch = match run("symbolic-ref", &[b"--quiet", b"HEAD"]) {
            Ok(stdout) => stdout.strip_suffix(b"\n").unwrap_or(&stdout).to_vec(),
            Err(GitError::Failed(_)) => return Err(unresolved()),
            Err(err) => return Err(git_error(err)),
        };
        // `show-ref --exists` exits 2 only for a reference that is not there; a broken one is
        // there. Before Git 2.43 it does not know the option (129), and a missing reference cannot
        // be told from a broken one, so a failure to find it is taken as a branch not born yet.
        match run("show-ref", &[b"--exists", &branch]) {
            Err(GitError::Failed(failed)) if failed.code == Some(2) => Ok(None),
            Err(GitError::Failed(failed)) if failed.code == Some(129) => {
                match run("show-ref", &[b"--verify", b"--quiet", &branch]) {
                    Err(GitError::Failed(_)) => Ok(None),
                    Ok(_) => Err(unresolved()),
                    Err(err) => Err(git_error(err)),
                }
            }
            Ok(_) | Err(GitError::Failed(_)) => Err(unresolved()),
            Err(err) => Err(git_error(err)),
        }
    }

    /// The id of the empty tree in this repository's object format, which a change is compared
    /// against before the first commit; `git` knows it without its being stored.
    fn empty_tree(&self) -> Result<String> {
        let args: &[&[u8]] = &[b"-t", b"tree", b"--no-filters", b"/dev/null"];
        let stdout = self
            .env
            .run(
                &self.at.root,
                "hash-object",
                args,
                budget::GIT_METADATA_STDOUT,
                self.cancel,
            )
            .map_err(git_error)?;
        Ok(String::from_utf8_lossy(&stdout).trim().to_string())
    }

    /// The scope directory on disk in this worktree, when it is there as exactly that directory.
    fn scope_dir(&self) -> Option<ProjectRoot> {
        source::scope_root(&self.at.root, &self.at.repository.scope).ok()
    }

    /// What is on disk at `rel`: its kind and version. A directory is a submodule's checkout, or a
    /// repository of its own, only when `repository_dir` says one belongs there; otherwise it is no
    /// file of the change, as to `git`. Nothing reached through a link counts.
    fn on_disk(&self, scope: Option<&ProjectRoot>, rel: &RelPath, repository_dir: bool) -> Found {
        let Some(path) = scope.and_then(|scope| exact_path(scope, rel, &mut HashMap::new())) else {
            return Found::Absent;
        };
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            return Found::Absent;
        };
        let kind = if metadata.file_type().is_symlink() {
            SideKind::Symlink
        } else if metadata.is_dir() {
            if !repository_dir {
                return Found::Absent;
            }
            SideKind::Submodule
        } else {
            SideKind::File
        };
        Found::Disk {
            kind,
            version: live::version(&metadata),
        }
    }

    /// `git diff <args> -- <paths>` with the presentation fixed whatever the user's configuration
    /// says, refused past the patch budget.
    ///
    /// Each path is matched exactly and nothing below it (`GitEnv::run_exact`), except where the
    /// paths nest (a rename from `foo` to `foo/bar`): there `git` sees everything below the outer
    /// path as well, the patch keeps only the sections between the change's own paths
    /// (`own_sections`), and renames are not detected, so that neither path is paired with a file
    /// below it whose section would be dropped. The change then shows as its old path's removal and
    /// its new path's addition. Everything below the outer path still counts against the patch
    /// budget, which refuses rather than cuts.
    fn patch(&self, args: &[&[u8]], paths: &[&RelPath]) -> Result<Vec<u8>> {
        let full: Vec<RelPath> = paths.iter().map(|rel| self.full(rel)).collect();
        let full: Vec<&[u8]> = full.iter().map(RelPath::as_bytes).collect();
        let renames: &[u8] = if paths_nest(&full) {
            b"--no-renames"
        } else {
            b"-M"
        };
        let mut all: Vec<&[u8]> = vec![
            renames,
            b"--submodule=short",
            b"--ignore-submodules=dirty",
            b"--no-relative",
            b"--src-prefix=a/",
            b"--dst-prefix=b/",
        ];
        all.extend_from_slice(args);
        let limit = budget::MAX_PATCH_BYTES;
        match self
            .env
            .run_exact(&self.at.root, "diff", &all, &full, limit, self.cancel)
        {
            Ok(patch) => Ok(own_sections(patch, &full)),
            Err(GitError::Stopped(BoundedFailure::StdoutExceeded(limit))) => {
                let max = limit.to_string();
                Err(CodedError::raised(
                    error_code::LIMIT_EXCEEDED,
                    format!("the change's patch is larger than the {limit}-byte limit"),
                    &[("limit", "patch_bytes"), ("max", &max)],
                ))
            }
            Err(err) => Err(git_error(err)),
        }
    }

    /// The body of a side, for a reply that carries it: a file's, read from its object, or from
    /// the disk at the version the patch was made against.
    fn body(
        &self,
        scope: Option<&ProjectRoot>,
        rel: &RelPath,
        found: &Found,
    ) -> Result<Option<Vec<u8>>> {
        match found {
            Found::Object {
                kind: SideKind::File,
                oid,
            } => Ok(Some(blob::read_blob(
                self.env,
                &self.at.root,
                oid,
                self.cancel,
            )?)),
            Found::Disk {
                kind: SideKind::File,
                version,
            } => {
                let Some(path) = scope.and_then(|s| exact_path(s, rel, &mut HashMap::new())) else {
                    return Err(live::changed(rel));
                };
                let file = live::read_resolved(&path, rel, budget::MAX_FILE_BYTES, self.cancel)?;
                if &file.version != version {
                    return Err(live::changed(rel));
                }
                Ok(Some(file.bytes))
            }
            _ => Ok(None),
        }
    }

    fn staged(
        &self,
        change: &ChangeRef,
        old_path: Option<RelPath>,
        new_path: Option<RelPath>,
        out_of_scope: bool,
    ) -> Result<ChangeRead> {
        let commit = self.head()?;
        let index_paths: Vec<&RelPath> = new_path.iter().chain(old_path.iter()).collect();
        let before = self.index(&index_paths)?;
        let old_found = match (&change.old, &old_path) {
            (SideRef::OutOfScope, _) | (_, None) => None,
            (_, Some(rel)) => Some(self.in_commit(commit.as_deref(), rel)?),
        };
        let new_found = match (&change.new, &new_path) {
            (SideRef::OutOfScope, _) | (_, None) => None,
            (_, Some(rel)) => Some(self.in_index(&before, rel)),
        };
        let source_old = |oid: &str| ContentSource::Commit {
            commit: commit.clone().unwrap_or_default(),
            branch: None,
            blob: oid.to_string(),
        };
        let source_new = |oid: &str| ContentSource::Index {
            worktree: self.at.worktree.clone(),
            blob: oid.to_string(),
        };
        if out_of_scope {
            // Only the side inside the project is read, and no patch is made: a patch would carry
            // the other side's content in its hunks.
            let read = |path: &Option<RelPath>,
                        found: &Option<Found>,
                        source: &dyn Fn(&str) -> ContentSource| {
                self.side(None, path.as_ref(), found.as_ref(), source, true)
            };
            return Ok(ChangeRead {
                head: commit.clone(),
                old: read(&old_path, &old_found, &source_old)?,
                new: read(&new_path, &new_found, &source_new)?,
                patch: None,
            });
        }
        let base = match &commit {
            Some(commit) => commit.clone(),
            None => self.empty_tree()?,
        };
        let paths: Vec<&RelPath> = old_path.iter().chain(new_path.iter()).collect();
        let patch = if old_found == Some(Found::Absent) && new_found == Some(Found::Absent) {
            // Nothing at either path: the change is gone, and there is nothing to compare.
            Vec::new()
        } else {
            self.patch(&[b"--cached", base.as_bytes()], &paths)?
        };
        after_patch();
        if self.index(&index_paths)? != before {
            return Err(live::changed(
                new_path.as_ref().or(old_path.as_ref()).unwrap(),
            ));
        }
        let bodies = carries_bodies(&patch);
        Ok(ChangeRead {
            head: commit.clone(),
            old: self.side(
                None,
                old_path.as_ref(),
                old_found.as_ref(),
                &source_old,
                bodies,
            )?,
            new: self.side(
                None,
                new_path.as_ref(),
                new_found.as_ref(),
                &source_new,
                bodies,
            )?,
            patch: Some(patch),
        })
    }

    fn unstaged(&self, old_path: RelPath, new_path: RelPath) -> Result<ChangeRead> {
        let scope = self.scope_dir();
        let index_paths: Vec<&RelPath> = if old_path == new_path {
            vec![&old_path]
        } else {
            vec![&old_path, &new_path]
        };
        let before = self.index(&index_paths)?;
        let mut old_found = self.in_index(&before, &old_path);
        let submodule = matches!(
            self.in_index(&before, &new_path),
            Found::Object {
                kind: SideKind::Submodule,
                ..
            }
        );
        let new_found = self.on_disk(scope.as_ref(), &new_path, submodule);
        let patch = if old_found == Found::Absent && new_found == Found::Absent {
            Vec::new()
        } else {
            self.patch(&[], &index_paths)?
        };
        after_patch();
        let changed = self.index(&index_paths)? != before
            || self.on_disk(scope.as_ref(), &new_path, submodule) != new_found;
        if changed {
            return Err(live::changed(&new_path));
        }
        // A path added with intent to add has an index entry that holds nothing yet: `git` compares
        // it as a new file, and so does the reply, as the listing does. A type change also has a
        // new file's section, beside its old side's deletion, which an addition never has.
        let has = |prefix: &[u8]| patch.split(|&b| b == b'\n').any(|l| l.starts_with(prefix));
        if old_path == new_path && has(b"new file mode ") && !has(b"deleted file mode ") {
            old_found = Found::Absent;
        }
        let bodies = carries_bodies(&patch);
        let root_id = scope.as_ref().map(|s| s.id.clone()).unwrap_or_default();
        let source_old = |oid: &str| ContentSource::Index {
            worktree: self.at.worktree.clone(),
            blob: oid.to_string(),
        };
        let source_new = |version: &str| ContentSource::Live {
            root_id: root_id.clone(),
            version: version.to_string(),
        };
        Ok(ChangeRead {
            head: None,
            old: self.side(
                scope.as_ref(),
                Some(&old_path),
                Some(&old_found),
                &source_old,
                bodies,
            )?,
            new: self.side(
                scope.as_ref(),
                Some(&new_path),
                Some(&new_found),
                &source_new,
                bodies,
            )?,
            patch: Some(patch),
        })
    }

    fn untracked(&self, path: RelPath) -> Result<ChangeRead> {
        let scope = self.scope_dir();
        // A directory Git lists as untracked is a repository of its own inside this one.
        let found = self.on_disk(scope.as_ref(), &path, true);
        let root_id = scope.as_ref().map(|s| s.id.clone()).unwrap_or_default();
        let source = |version: &str| ContentSource::Live {
            root_id: root_id.clone(),
            version: version.to_string(),
        };
        Ok(ChangeRead {
            head: None,
            old: SideRead::Absent,
            new: self.side(scope.as_ref(), Some(&path), Some(&found), &source, true)?,
            patch: None,
        })
    }

    /// One side of the reply: out of scope when nothing was looked up for it (`found` is `None`),
    /// absent when nothing is there, and otherwise present, with its body when `with_body`.
    fn side(
        &self,
        scope: Option<&ProjectRoot>,
        path: Option<&RelPath>,
        found: Option<&Found>,
        source: &dyn Fn(&str) -> ContentSource,
        with_body: bool,
    ) -> Result<SideRead> {
        let (Some(path), Some(found)) = (path, found) else {
            return Ok(SideRead::OutOfScope);
        };
        let (kind, identity) = match found {
            Found::Absent => return Ok(SideRead::Absent),
            Found::Object { kind, oid } => (*kind, oid.as_str()),
            Found::Disk { kind, version } => (*kind, version.as_str()),
        };
        let file = if with_body {
            self.body(scope, path, found)?
                .map(crate::browse::file_content)
        } else {
            None
        };
        Ok(SideRead::Present {
            path: path.to_wire(),
            kind,
            source: source(identity),
            file,
        })
    }
}

/// `patch` with only the sections between the change's own `paths`. Only where the paths nest
/// (`paths_nest`) is what lies below a path matched too, so only then can the patch hold a section
/// of another path, which is dropped here. A section is told by its `diff --git` header, which `git`
/// writes from the two paths it compares, quoted as `quoted` spells them; any other line starting
/// a section of its own — another `diff` header (`diff --cc` for a path in conflict), a
/// `* Unmerged path` note — ends the one before it and is kept only as part of an own section.
fn own_sections(patch: Vec<u8>, paths: &[&[u8]]) -> Vec<u8> {
    if !paths_nest(paths) {
        return patch;
    }
    let mut headers = Vec::new();
    for old in paths {
        for new in paths {
            let mut header = b"diff --git ".to_vec();
            header.extend(quoted(b"a/", old));
            header.push(b' ');
            header.extend(quoted(b"b/", new));
            headers.push(header);
        }
    }
    let mut kept = Vec::new();
    let mut keeping = false;
    for line in patch.split_inclusive(|&b| b == b'\n') {
        if line.starts_with(b"diff ") || line.starts_with(b"* ") {
            let header = line.strip_suffix(b"\n").unwrap_or(line);
            keeping = headers.iter().any(|h| h == header);
        }
        if keeping {
            kept.extend_from_slice(line);
        }
    }
    kept
}

/// `prefix` and `path` as a patch header names them under `core.quotePath=false`: as they are,
/// unless a control character, a `"` or a `\` makes `git` quote the whole name C-style.
fn quoted(prefix: &[u8], path: &[u8]) -> Vec<u8> {
    let name = [prefix, path].concat();
    if !name
        .iter()
        .any(|&b| b < 0x20 || b == b'"' || b == b'\\' || b == 0x7f)
    {
        return name;
    }
    let mut out = vec![b'"'];
    for &byte in &name {
        match byte {
            0x07 => out.extend_from_slice(b"\\a"),
            0x08 => out.extend_from_slice(b"\\b"),
            b'\t' => out.extend_from_slice(b"\\t"),
            b'\n' => out.extend_from_slice(b"\\n"),
            0x0b => out.extend_from_slice(b"\\v"),
            0x0c => out.extend_from_slice(b"\\f"),
            b'\r' => out.extend_from_slice(b"\\r"),
            b'"' => out.extend_from_slice(b"\\\""),
            b'\\' => out.extend_from_slice(b"\\\\"),
            byte if byte < 0x20 || byte == 0x7f => out.extend(format!("\\{byte:03o}").bytes()),
            byte => out.push(byte),
        }
    }
    out.push(b'"');
    out
}

#[cfg(test)]
thread_local! {
    /// Run between making a patch and checking its sources again, so a test can change them there.
    pub(super) static AFTER_PATCH: std::cell::RefCell<Option<Box<dyn FnMut()>>> =
        const { std::cell::RefCell::new(None) };
}

fn after_patch() {
    #[cfg(test)]
    AFTER_PATCH.with(|hook| {
        if let Some(hook) = hook.borrow_mut().as_mut() {
            hook();
        }
    });
}

/// Whether a reply carries its sides' bodies beside `patch`: only when the patch shows none of
/// their content — a binary change, which `git` writes as one line — and is small enough to sit
/// beside two bodies within the reply's reservation.
fn carries_bodies(patch: &[u8]) -> bool {
    patch.len() <= budget::BODIES_PATCH_BYTES
        && patch
            .split(|&b| b == b'\n')
            .any(|line| line.starts_with(b"Binary files ") && line.ends_with(b" differ"))
}
