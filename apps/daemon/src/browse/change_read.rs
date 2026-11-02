//! The reading of one change, for the worktree's changes (`changes`) and for the changes between
//! commits (`compare`, `commits`).
//!
//! A diff is read afresh at the paths it names, from its group's two sources — `HEAD` and the index
//! for a staged change, the index and the disk for an unstaged one — never from the disk in place
//! of the index or a commit. The commit is pinned by its id before anything is read, and blob ids
//! never change meaning, so what can still move is the index entry and the file on disk: both are
//! looked at before and after the patch is made, and a reply whose patch may have been made from
//! another version than the sides it reports is refused as `source_changed` rather than returned.
//!
//! A change whose two paths nest (a rename from `foo` to `foo/bar`) cannot be asked of `git` over
//! pathspecs without everything below the outer path coming along. When both of its sides are
//! files or links, `git` is asked for the diff between the two versions themselves, named by
//! revision and path, and nothing else in the repository is read; any other such change is read
//! without rename detection and cut down to its own sections.
//!
//! The whole bodies of a change's two sides (`bodies_of`) are read for a client expanding the
//! lines its patch collapsed. The change is resolved exactly as for its patch, the sides found are
//! compared with the versions the client names, and only a match is read; a body is then read from
//! the very object or file version that matched, so nothing mixes revisions. A side read from the
//! disk at a path with a clean filter in effect is refused instead of read: `git` made the patch
//! from what that filter produces, which the file's own bytes are not.
//!
//! A reply whose patch shows a change's content also carries the line count of each side that is a
//! file object, so a client can tell whether a last hunk reaches the end of a file; see
//! `Reader::object_lines`. A side on the disk is never counted from the disk: it is counted only
//! where the reply carries its body, from those very bytes.
//!
//! A change between two commits — one of a branch comparison's (`read_between`) — is read the way a
//! staged one is, its two sides from the two commits; nothing it reads can move, so nothing is
//! looked at again.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use anyhow::Result;

use crate::browse::blob::{self, IndexEntry};
use crate::browse::content::{file_content, line_count};
use crate::browse::scope::Located;
use crate::browse::source::{self, ProjectRoot};
use crate::browse::{budget, git_error, git_failed, live};
use crate::git::isolated::{paths_nest, IsolatedGit};
use crate::git::run::GitError;
use crate::processes::subprocess::BoundedFailure;
use crate::protocol::{
    error_code, ChangeRef, ChangeSide, CodedError, ContentSource, SideKind, SideRead, SideRef,
    SideVersion,
};
use crate::wire_path::RelPath;

/// `path`, relative to the repository's root, as a path relative to the project's directory —
/// `None` when it is not inside it. The scope directory itself is not a path inside the project.
pub(super) fn in_scope(scope: &RelPath, path: &[u8]) -> Option<RelPath> {
    if scope.is_root() {
        return RelPath::from_bytes(path.to_vec()).filter(|rel| !rel.is_root());
    }
    let rest = path.strip_prefix(scope.as_bytes())?.strip_prefix(b"/")?;
    RelPath::from_bytes(rest.to_vec()).filter(|rel| !rel.is_root())
}

/// What an entry of mode `mode` is, `None` for a mode of zeros: nothing there. A directory
/// (`040000`, met only where a change names one) is no side of a change either.
pub(super) fn kind_of(mode: &[u8]) -> Option<SideKind> {
    match mode {
        b"000000" | b"040000" => None,
        b"120000" => Some(SideKind::Symlink),
        b"160000" => Some(SideKind::Submodule),
        _ => Some(SideKind::File),
    }
}

pub(super) fn head_side(head: Option<&str>, mode: &[u8], oid: &[u8], rel: &RelPath) -> ChangeSide {
    match (head, kind_of(mode)) {
        (Some(commit), Some(kind)) => ChangeSide::Present {
            path: rel.to_wire(),
            kind,
            source: ContentSource::Commit {
                commit: commit.to_string(),
                branch: None,
                blob: String::from_utf8_lossy(oid).into(),
            },
        },
        _ => ChangeSide::Absent,
    }
}

/// `rel` joined onto `scope`, when the directory holding it is exactly that directory, reached
/// through no symbolic link; `dirs` remembers the directories already asked about.
pub(super) fn exact_path(
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

/// What a client names of the diff it expands: the versions its two sides were reported at.
#[derive(Debug)]
pub struct Expected<'a> {
    pub old: &'a SideVersion,
    pub new: &'a SideVersion,
}

/// The whole bodies of a change's two sides.
#[derive(Debug)]
pub struct BodiesRead {
    pub old: SideRead,
    pub new: SideRead,
}

/// The whole bodies of a change between two commits, read as [`read_between`] reads it; a change
/// with a side outside the project has none.
pub(super) fn read_bodies_between(
    env: &IsolatedGit,
    at: &Located,
    commits: (&str, &str),
    sides: (&SideRef, &SideRef),
    expected: &Expected,
    cancel: &AtomicBool,
) -> Result<BodiesRead> {
    if has_out_of_scope_side(sides.0, sides.1) {
        return Err(no_bodies());
    }
    let read = read_between_with(env, at, commits, sides, cancel, false)?;
    // A change that was read names a side, and a side that is not out of scope names a path.
    let path = (side_path(sides.1)?.or(side_path(sides.0)?)).unwrap();
    bodies_of(env, at, read, &path, expected, cancel)
}

/// Whether either side of a change lies outside the project: such a change has no patch, so its
/// bodies are refused before any side is read.
pub(super) fn has_out_of_scope_side(old: &SideRef, new: &SideRef) -> bool {
    matches!(old, SideRef::OutOfScope) || matches!(new, SideRef::OutOfScope)
}

pub(super) fn no_bodies() -> anyhow::Error {
    CodedError::raised(
        error_code::INVALID_CHANGE,
        "the change has no patch whose lines could be expanded",
        &[],
    )
}

/// A side on the disk whose path has a clean filter in effect, named by the filter driver: `git`
/// made the patch from the content that filter produces, so the file's own bytes are not the lines
/// the patch left out, and no `git` command writes a path's cleaned bytes out to be read instead
/// (`hash-object -w` would store them as an object, which no browse read may do).
fn clean_filtered(path: &RelPath, filter: &str) -> anyhow::Error {
    let shown = path.to_wire();
    CodedError::raised(
        error_code::CLEAN_FILTERED,
        format!(
            "{shown} passes through the {filter} clean filter, so the content its patch was made \
             from cannot be read"
        ),
        &[("path", &shown), ("filter", filter)],
    )
}

/// The bodies of the sides `read` found, once their versions are those `expected` names; a side
/// that is not has changed under the client, at `path`. The patch the read made is dropped first,
/// so it is not held beside the bodies.
pub(super) fn bodies_of(
    env: &IsolatedGit,
    at: &Located,
    read: ChangeRead,
    path: &RelPath,
    expected: &Expected,
    cancel: &AtomicBool,
) -> Result<BodiesRead> {
    let ChangeRead { old, new, .. } = read;
    let (old_expected, new_expected) = (expected.old, expected.new);
    for (side, expected) in [(&old, old_expected), (&new, new_expected)] {
        let same = match (side, expected) {
            (SideRead::Absent, SideVersion::Absent) => true,
            (SideRead::Present { source, .. }, SideVersion::Present { source: named }) => {
                source == named
            }
            _ => false,
        };
        if !same {
            return Err(live::changed(path));
        }
    }
    let reader = Reader {
        env,
        at,
        cancel,
        inline_bodies: false,
    };
    let scope = reader.scope_dir();
    for side in [&old, &new] {
        reader.refuse_clean_filtered(side)?;
    }
    Ok(BodiesRead {
        old: reader.whole(scope.as_ref(), old)?,
        new: reader.whole(scope.as_ref(), new)?,
    })
}

/// Reads a change between two commits, `old_commit` holding its old side and `new_commit` its new
/// one, as the sides `old` and `new` name it: both from those commits alone, and its patch made
/// between exactly them. Both are immutable, so nothing is looked at again once the patch is made.
pub(super) fn read_between(
    env: &IsolatedGit,
    at: &Located,
    commits: (&str, &str),
    sides: (&SideRef, &SideRef),
    cancel: &AtomicBool,
) -> Result<ChangeRead> {
    read_between_with(env, at, commits, sides, cancel, true)
}

/// [`read_between`], its sides carrying their bodies beside the patch where `inline_bodies` allows.
fn read_between_with(
    env: &IsolatedGit,
    at: &Located,
    commits: (&str, &str),
    (old, new): (&SideRef, &SideRef),
    cancel: &AtomicBool,
    inline_bodies: bool,
) -> Result<ChangeRead> {
    let (old_named, new_named) = (side_path(old)?, side_path(new)?);
    if old_named.is_none() && new_named.is_none() {
        return Err(CodedError::raised(
            error_code::INVALID_CHANGE,
            "the change names no side to read",
            &[],
        ));
    }
    let out_of_scope = has_out_of_scope_side(old, new);
    // A side named absent is looked up at the other side's path.
    let old_path = old_named.clone().or_else(|| new_named.clone());
    let new_path = new_named.or(old_named);
    let reader = Reader {
        env,
        at,
        cancel,
        inline_bodies,
    };
    reader.between(commits, (old, new), old_path, new_path, out_of_scope)
}

/// The ids of the empty tree in the two object formats, SHA-1 and SHA-256.
const EMPTY_TREE_IDS: [&str; 2] = [
    "4b825dc642cb6eb9a060e54bf8d69288fbee4904",
    "6ef19b41225c5369f1c104d45d8d85efa9b057b53b14b4b9b939dd74decc5321",
];

/// Whether `id` is the id of the empty tree, in either object format.
pub(super) fn is_empty_tree(id: &str) -> bool {
    EMPTY_TREE_IDS.contains(&id)
}

/// The id of the empty tree in this repository's object format, which a change is compared
/// against before the first commit, or a commit with no parent against; `git` knows it without its
/// being stored.
pub(super) fn empty_tree(env: &IsolatedGit, at: &Located, cancel: &AtomicBool) -> Result<String> {
    let args: &[&[u8]] = &[b"-t", b"tree", b"--no-filters", b"/dev/null"];
    let stdout = env
        .run(
            &at.root,
            "hash-object",
            args,
            budget::GIT_METADATA_STDOUT,
            cancel,
        )
        .map_err(git_error)?;
    Ok(String::from_utf8_lossy(&stdout).trim().to_string())
}

/// The project-relative path a side names, when it names one.
pub(super) fn side_path(side: &SideRef) -> Result<Option<RelPath>> {
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

pub(super) struct Reader<'a> {
    pub(super) env: &'a IsolatedGit,
    pub(super) at: &'a Located,
    pub(super) cancel: &'a AtomicBool,
    /// Whether a reply may carry its sides' bodies beside the patch (`carries_bodies`); false
    /// when the bodies are asked for apart.
    pub(super) inline_bodies: bool,
}

impl Reader<'_> {
    fn full(&self, rel: &RelPath) -> RelPath {
        self.at.repository.scope.join(rel)
    }

    /// The index entries at `paths` (project-relative), refusing a path in conflict: its stages are
    /// not one side of anything.
    ///
    /// Paths that nest are looked up one at a time: asked for together, `git` lists everything
    /// below the outer path as well, and all of it would count against the metadata budget. The
    /// entries then come from separate reads of the index, which the callers' comparison of the
    /// entries before and after their patch covers.
    fn index(&self, paths: &[&RelPath]) -> Result<Vec<IndexEntry>> {
        let full: Vec<RelPath> = paths.iter().map(|rel| self.full(rel)).collect();
        let args: Vec<&[u8]> = full.iter().map(RelPath::as_bytes).collect();
        let lookups: Vec<&[&[u8]]> = match paths_nest(&args) {
            true => args.chunks(1).collect(),
            false => vec![&args],
        };
        let mut entries = Vec::new();
        for lookup in lookups {
            entries.extend(blob::index_entries(
                self.env,
                &self.at.root,
                lookup,
                self.cancel,
            )?);
        }
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
            Err(GitError::Exited { .. }) => || git_failed("HEAD names no commit"),
            Err(err) => return Err(git_error(err)),
        };
        let branch = match run("symbolic-ref", &[b"--quiet", b"HEAD"]) {
            Ok(stdout) => stdout.strip_suffix(b"\n").unwrap_or(&stdout).to_vec(),
            Err(GitError::Exited { .. }) => return Err(unresolved()),
            Err(err) => return Err(git_error(err)),
        };
        // `show-ref --exists` exits 2 only for a reference that is not there; a broken one is
        // there. Before Git 2.43 it does not know the option (129), and a missing reference cannot
        // be told from a broken one, so a failure to find it is taken as a branch not born yet.
        match run("show-ref", &[b"--exists", &branch]) {
            Err(GitError::Exited { code: Some(2), .. }) => Ok(None),
            Err(GitError::Exited {
                code: Some(129), ..
            }) => match run("show-ref", &[b"--verify", b"--quiet", &branch]) {
                Err(GitError::Exited { .. }) => Ok(None),
                Ok(_) => Err(unresolved()),
                Err(err) => Err(git_error(err)),
            },
            Ok(_) | Err(GitError::Exited { .. }) => Err(unresolved()),
            Err(err) => Err(git_error(err)),
        }
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
    /// Each path is matched exactly and nothing below it (`IsolatedGit::run_exact`), except where
    /// the paths nest (a rename from `foo` to `foo/bar`): there `git` sees everything below the
    /// outer path as well, the patch keeps only the sections between the change's own paths
    /// (`own_sections`), and renames are not detected. A pathspec-restricted diff sees another set
    /// of rename candidates than the whole-repository `git status` that listed the change, and
    /// could pair one of its paths with a third path below the outer one, whose section would then
    /// be dropped, leaving a side with no counterpart. The change then shows as its old path's
    /// removal and its new path's addition. Everything below the outer path still counts against
    /// the patch budget, which refuses rather than cuts.
    fn patch(&self, args: &[&[u8]], paths: &[&RelPath]) -> Result<Vec<u8>> {
        let full: Vec<RelPath> = paths.iter().map(|rel| self.full(rel)).collect();
        let full: Vec<&[u8]> = full.iter().map(RelPath::as_bytes).collect();
        let renames: &[u8] = if paths_nest(&full) {
            b"--no-renames"
        } else {
            b"-M"
        };
        let mut all: Vec<&[u8]> = vec![renames, b"--submodule=short", b"--ignore-submodules=dirty"];
        all.extend_from_slice(PATCH_PRESENTATION);
        all.extend_from_slice(args);
        let limit = budget::MAX_PATCH_BYTES;
        let run = self
            .env
            .run_exact(&self.at.root, "diff", &all, &full, limit, self.cancel);
        Ok(own_sections(within_budget(run)?, &full))
    }

    /// The diff `git` computes between the two versions a nesting change names, each as
    /// `<revision>:<path>`, or `None` when the change does not meet the guard and has to be read by
    /// [`Reader::patch`].
    ///
    /// The guard: the two paths nest, and both sides resolved to an object that is a file or a
    /// link. The kinds may differ: `git` writes a type change as two sections here as well, which
    /// the client joins just as it joins the other route's, so one route serves both. A submodule
    /// named that way resolves to its commit, and two of them make `git` diff the commits' whole
    /// trees, content the change never named; a side that is absent takes the other route.
    /// A nesting pair in the Unstaged group never meets it: the index cannot hold a path as both a
    /// file and a directory, so one side is always absent there, and that group does not ask.
    /// `revisions` are the old side's and the new side's, as `git` spells them (a commit, or `:0`
    /// for the index); without an old one there is no old side to name.
    fn pair_operands(
        &self,
        revisions: (Option<&str>, &str),
        paths: (&Option<RelPath>, &Option<RelPath>),
        found: (&Option<Found>, &Option<Found>),
    ) -> Option<(Vec<u8>, Vec<u8>)> {
        let is_blob_or_link = |found: &Option<Found>| {
            matches!(
                found,
                Some(Found::Object {
                    kind: SideKind::File | SideKind::Symlink,
                    ..
                })
            )
        };
        let (old, new) = (self.full(paths.0.as_ref()?), self.full(paths.1.as_ref()?));
        if !paths_nest(&[old.as_bytes(), new.as_bytes()])
            || !is_blob_or_link(found.0)
            || !is_blob_or_link(found.1)
        {
            return None;
        }
        let operand =
            |revision: &str, path: &RelPath| [revision.as_bytes(), b":", path.as_bytes()].concat();
        Some((operand(revisions.0?, &old), operand(revisions.1, &new)))
    }

    /// `git diff` between the versions `operands` name (see [`Reader::pair_operands`]), refused
    /// past the patch budget. It is run as `git` writes it for two operands and nothing else: no
    /// pathspec is given, so the literal-pathspec setting stays in force, and the list ends with a
    /// `--` so that a file in the worktree named like an operand cannot be taken for a path. Naming
    /// revision and path rather than object ids keeps the repository's attributes in effect.
    fn patch_pair(&self, operands: &(Vec<u8>, Vec<u8>)) -> Result<Vec<u8>> {
        let mut args: Vec<&[u8]> = PATCH_PRESENTATION.to_vec();
        args.extend_from_slice(&[&operands.0, &operands.1, b"--"]);
        within_budget(self.env.run(
            &self.at.root,
            "diff",
            &args,
            budget::MAX_PATCH_BYTES,
            self.cancel,
        ))
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

    pub(super) fn staged(
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
                self.side(
                    None,
                    path.as_ref(),
                    found.as_ref(),
                    source,
                    self.inline_bodies,
                    &HashMap::new(),
                )
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
            None => empty_tree(self.env, self.at, self.cancel)?,
        };
        let paths: Vec<&RelPath> = old_path.iter().chain(new_path.iter()).collect();
        let pair = self.pair_operands(
            (commit.as_deref(), ":0"),
            (&old_path, &new_path),
            (&old_found, &new_found),
        );
        let patch = if old_found == Some(Found::Absent) && new_found == Some(Found::Absent) {
            // Nothing at either path: the change is gone, and there is nothing to compare.
            Vec::new()
        } else if let Some(operands) = &pair {
            self.patch_pair(operands)?
        } else {
            self.patch(&[b"--cached", base.as_bytes()], &paths)?
        };
        after_patch();
        if self.index(&index_paths)? != before {
            return Err(live::changed(
                new_path.as_ref().or(old_path.as_ref()).unwrap(),
            ));
        }
        let bodies = self.inline_bodies && carries_bodies(&patch);
        let lines = self.object_lines(
            Some(&patch),
            bodies,
            [old_found.as_ref(), new_found.as_ref()],
        )?;
        Ok(ChangeRead {
            head: commit.clone(),
            old: self.side(
                None,
                old_path.as_ref(),
                old_found.as_ref(),
                &source_old,
                bodies,
                &lines,
            )?,
            new: self.side(
                None,
                new_path.as_ref(),
                new_found.as_ref(),
                &source_new,
                bodies,
                &lines,
            )?,
            patch: Some(patch),
        })
    }

    fn between(
        &self,
        (old_commit, new_commit): (&str, &str),
        (old, new): (&SideRef, &SideRef),
        old_path: Option<RelPath>,
        new_path: Option<RelPath>,
        out_of_scope: bool,
    ) -> Result<ChangeRead> {
        let found = |side: &SideRef, path: &Option<RelPath>, commit: &str| match (side, path) {
            (SideRef::OutOfScope, _) | (_, None) => Ok(None),
            (_, Some(rel)) => self.in_commit(Some(commit), rel).map(Some),
        };
        let old_found = found(old, &old_path, old_commit)?;
        let new_found = found(new, &new_path, new_commit)?;
        let source = |commit: &str, oid: &str| ContentSource::Commit {
            commit: commit.to_string(),
            branch: None,
            blob: oid.to_string(),
        };
        let source_old = |oid: &str| source(old_commit, oid);
        let source_new = |oid: &str| source(new_commit, oid);
        // As for a staged change: across the project's boundary only the side inside is read, and
        // no patch is made, since one would carry the other side's content in its hunks.
        let patch = if out_of_scope {
            None
        } else if old_found == Some(Found::Absent) && new_found == Some(Found::Absent) {
            Some(Vec::new())
        } else {
            let pair = self.pair_operands(
                (Some(old_commit), new_commit),
                (&old_path, &new_path),
                (&old_found, &new_found),
            );
            Some(match &pair {
                Some(operands) => self.patch_pair(operands)?,
                None => {
                    let paths: Vec<&RelPath> = old_path.iter().chain(new_path.iter()).collect();
                    let commits: &[&[u8]] = &[old_commit.as_bytes(), new_commit.as_bytes()];
                    self.patch(commits, &paths)?
                }
            })
        };
        let bodies = self.inline_bodies && patch.as_deref().is_none_or(carries_bodies);
        let lines = self.object_lines(
            patch.as_deref(),
            bodies,
            [old_found.as_ref(), new_found.as_ref()],
        )?;
        Ok(ChangeRead {
            head: None,
            old: self.side(
                None,
                old_path.as_ref(),
                old_found.as_ref(),
                &source_old,
                bodies,
                &lines,
            )?,
            new: self.side(
                None,
                new_path.as_ref(),
                new_found.as_ref(),
                &source_new,
                bodies,
                &lines,
            )?,
            patch,
        })
    }

    pub(super) fn unstaged(&self, old_path: RelPath, new_path: RelPath) -> Result<ChangeRead> {
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
        let bodies = self.inline_bodies && carries_bodies(&patch);
        let lines = self.object_lines(Some(&patch), bodies, [Some(&old_found), None])?;
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
                &lines,
            )?,
            new: self.side(
                scope.as_ref(),
                Some(&new_path),
                Some(&new_found),
                &source_new,
                bodies,
                &lines,
            )?,
            patch: Some(patch),
        })
    }

    pub(super) fn untracked(&self, path: RelPath) -> Result<ChangeRead> {
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
            new: self.side(
                scope.as_ref(),
                Some(&path),
                Some(&found),
                &source,
                self.inline_bodies,
                &HashMap::new(),
            )?,
            patch: None,
        })
    }

    /// `side`, a present file's whole body read from the version it names, which is checked once
    /// more as it is read: a link or a submodule has no body. Anything else comes back as it is.
    /// A side on the disk is read only once [`Reader::refuse_clean_filtered`] has passed it.
    ///
    /// The side's `lines` are counted from the body read, which makes this the one place a side
    /// read from the disk gets a count: with a clean filter's paths refused, the count is the one
    /// `git` numbered the patch from. The conversions left all keep a file's line boundaries, and
    /// so its count: `ident`'s `$Id$` and a `working-tree-encoding` rewrite bytes within a line,
    /// and a worktree's CRLF endings only lose their `\r`.
    fn whole(&self, scope: Option<&ProjectRoot>, side: SideRead) -> Result<SideRead> {
        let SideRead::Present {
            path, kind, source, ..
        } = side
        else {
            return Ok(side);
        };
        let rel = RelPath::parse(&path).ok_or_else(|| git_failed("the side's path is invalid"))?;
        if kind != SideKind::File {
            let name = match kind {
                SideKind::Symlink => "symlink",
                _ => "submodule",
            };
            return Err(live::unsupported(&rel, name));
        }
        let found = match &source {
            ContentSource::Live { version, .. } => Found::Disk {
                kind,
                version: version.clone(),
            },
            ContentSource::Index { blob, .. } | ContentSource::Commit { blob, .. } => {
                Found::Object {
                    kind,
                    oid: blob.clone(),
                }
            }
        };
        let body = self.body(scope, &rel, &found)?;
        let lines = body.as_deref().map(line_count);
        Ok(SideRead::Present {
            path,
            kind,
            source,
            file: body.map(file_content),
            lines,
        })
    }

    /// Refuses a side read from the disk at a path with a clean filter in effect
    /// (`clean_filtered`): `git` numbered the patch from what that filter produces, so the file's
    /// own bytes are not the lines the patch left out, and nothing reads a path's cleaned bytes
    /// back out — `hash-object -w` would store them, which no browse read may do. The cleaned
    /// content does stand in a wide-context `git diff`'s own context lines, so a body could be
    /// assembled from a second patch; these replies deliver a body read whole from one object or
    /// file version instead, and the client keeps the collapsed lines as they are.
    ///
    /// Both sides are put through this before either body is read, so a refused expansion never
    /// reads a whole blob it then throws away.
    fn refuse_clean_filtered(&self, side: &SideRead) -> Result<()> {
        let SideRead::Present {
            path,
            kind: SideKind::File,
            source: ContentSource::Live { .. },
            ..
        } = side
        else {
            return Ok(());
        };
        let rel = RelPath::parse(path).ok_or_else(|| git_failed("the side's path is invalid"))?;
        match self.clean_filter(&rel)? {
            Some(filter) => Err(clean_filtered(&rel, &filter)),
            None => Ok(()),
        }
    }

    /// The name of the filter driver that cleans `rel` on its way into the repository, when the
    /// path has one in effect: a `filter` attribute naming a driver, and that driver configured
    /// with a `clean` command or with the long-running `process` one Git LFS uses. A `filter`
    /// attribute whose driver configures neither cleans nothing, so `git` passes the file's own
    /// bytes through and there is nothing to refuse.
    ///
    /// Both reads leave the repository alone: `check-attr` reads the attribute files and the index,
    /// `config` the configuration files.
    fn clean_filter(&self, rel: &RelPath) -> Result<Option<String>> {
        let full = self.full(rel);
        let args: &[&[u8]] = &[b"-z", b"filter", b"--", full.as_bytes()];
        let stdout = self
            .env
            .run(
                &self.at.root,
                "check-attr",
                args,
                budget::GIT_METADATA_STDOUT,
                self.cancel,
            )
            .map_err(git_error)?;
        // `<path>\0<attribute>\0<value>\0`, the value being a driver name or one of `git`'s own
        // markers for an attribute that is unset, set without a value, or not mentioned at all.
        let mut fields = stdout.split(|&byte| byte == 0);
        let (Some(_path), Some(_attribute), Some(value)) =
            (fields.next(), fields.next(), fields.next())
        else {
            return Ok(None);
        };
        let Ok(driver) = std::str::from_utf8(value) else {
            return Ok(None);
        };
        if matches!(driver, "unspecified" | "unset" | "set" | "") {
            return Ok(None);
        }
        let args: &[&[u8]] = &[b"--list", b"-z"];
        let stdout = self
            .env
            .run(
                &self.at.root,
                "config",
                args,
                budget::GIT_METADATA_STDOUT,
                self.cancel,
            )
            .map_err(git_error)?;
        let cleans = [
            format!("filter.{driver}.clean"),
            format!("filter.{driver}.process"),
        ];
        // `<key>\n<value>\0` per entry, or `<key>\0` for a key set without one. The value has to be
        // read rather than the key alone: a command configured as the empty string is listed like
        // any other and filters nothing.
        let configured = stdout
            .split(|&byte| byte == 0)
            .filter_map(|entry| std::str::from_utf8(entry).ok())
            .filter_map(|entry| entry.split_once('\n'))
            .any(|(key, value)| !value.is_empty() && cleans.iter().any(|clean| clean == key));
        Ok(configured.then(|| driver.to_string()))
    }

    /// One side of the reply: out of scope when nothing was looked up for it (`found` is `None`),
    /// absent when nothing is there, and otherwise present, with its body when `with_body`.
    ///
    /// Its line count is its body's where the body is carried, and otherwise `counts`' for the
    /// object a file side is: a side on the disk is never counted from the disk, since `git`
    /// numbers the lines of its cleaned content (a clean filter such as Git LFS's), not of the
    /// file.
    fn side(
        &self,
        scope: Option<&ProjectRoot>,
        path: Option<&RelPath>,
        found: Option<&Found>,
        source: &dyn Fn(&str) -> ContentSource,
        with_body: bool,
        counts: &HashMap<String, u64>,
    ) -> Result<SideRead> {
        let (Some(path), Some(found)) = (path, found) else {
            return Ok(SideRead::OutOfScope);
        };
        let (kind, identity) = match found {
            Found::Absent => return Ok(SideRead::Absent),
            Found::Object { kind, oid } => (*kind, oid.as_str()),
            Found::Disk { kind, version } => (*kind, version.as_str()),
        };
        let body = if with_body {
            self.body(scope, path, found)?
        } else {
            None
        };
        let lines = match (&body, found) {
            (Some(body), _) => Some(line_count(body)),
            (
                None,
                Found::Object {
                    kind: SideKind::File,
                    oid,
                },
            ) => counts.get(oid).copied(),
            _ => None,
        };
        Ok(SideRead::Present {
            path: path.to_wire(),
            kind,
            source: source(identity),
            file: body.map(file_content),
            lines,
        })
    }

    /// The line counts of the file objects among `found`, for a reply whose patch shows the
    /// change's content: none for a reply that carries its bodies (it counts those), has no patch
    /// or one without a hunk (a change of mode or a pure rename: with no hunk there is no
    /// separator bar, so a count has no use), or is read for its bodies apart (`inline_bodies` is
    /// false).
    ///
    /// It is read after the patch and the check of the sources that follows it, which costs
    /// nothing in consistency: an object id never changes meaning, so these are the counts of the
    /// very blobs the patch was made from whenever they are asked. Only a file's object is named
    /// (see [`blob::line_counts`]); the disk side of an unstaged change is left out on purpose.
    fn object_lines(
        &self,
        patch: Option<&[u8]>,
        bodies: bool,
        found: [Option<&Found>; 2],
    ) -> Result<HashMap<String, u64>> {
        let has_hunk = |patch: &[u8]| patch.split(|&b| b == b'\n').any(|l| l.starts_with(b"@@ "));
        if !self.inline_bodies || bodies || !patch.is_some_and(has_hunk) {
            return Ok(HashMap::new());
        }
        let oids: Vec<&str> = found
            .into_iter()
            .filter_map(|found| match found {
                Some(Found::Object {
                    kind: SideKind::File,
                    oid,
                }) => Some(oid.as_str()),
                _ => None,
            })
            .collect();
        blob::line_counts(self.env, &self.at.root, &oids, self.cancel)
    }
}

/// What every patch is presented with whichever route makes it: paths as written from the
/// repository's root, and the same two prefixes for the sides.
const PATCH_PRESENTATION: &[&[u8]] = &[b"--no-relative", b"--src-prefix=a/", b"--dst-prefix=b/"];

/// A `git diff`'s output, or the refusal a patch past the budget gets: it is never cut, since part
/// of a patch is not the change.
fn within_budget(run: Result<Vec<u8>, GitError>) -> Result<Vec<u8>> {
    match run {
        Ok(patch) => Ok(patch),
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
