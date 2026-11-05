//! A worktree's uncommitted changes, scoped to a project, and the diff of one of them.
//!
//! The listing is one `git status` over the whole worktree, so the index is read once and every
//! entry in it is from that one reading, and so a rename into or out of the project's directory is
//! found against the rest of the repository. Only what touches the project is kept: an entry with
//! either side inside its scope. A side outside it is reported by its repository path alone, and
//! nothing of its content is ever read or returned.
//!
//! A diff and the bodies of a change are read by `change_read`; here a change is resolved to the
//! paths it reads.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use anyhow::Result;

use crate::browse::budget::{self, ChangeBudget};
use crate::browse::change_read::{
    bodies_of, exact_path, has_out_of_scope_side, head_side, in_scope, kind_of, no_bodies,
    side_path, BodiesRead, ChangeRead, Expected, Reader,
};
use crate::browse::scope::Located;
use crate::browse::source::{self, ProjectRoot};
use crate::browse::{git_error, git_failed, line_counts, live};
use crate::git::isolated::IsolatedGit;
use crate::platform::fs::{self as platform, Links};
use crate::protocol::{
    error_code, ChangeEntry, ChangeGroup, ChangeRef, ChangeSide, CodedError, ConflictKind,
    ContentSource, SideKind, SideRef,
};
use crate::wire_path::{self, RelPath};

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
pub fn list(env: &IsolatedGit, at: &Located, cancel: &AtomicBool) -> Result<ChangeList> {
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
    let mut taken = ChangeBudget::default();
    let mut fields = stdout.split(|&b| b == 0).filter(|f| !f.is_empty());
    'fields: while let Some(field) = fields.next() {
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
            if !taken.admit(&entry) {
                list.complete = false;
                break 'fields;
            }
            list.changes.push(entry);
        }
    }
    // Released before the counts are read, which hold buffers of their own.
    drop(stdout);
    add_line_counts(env, at, &mut list.changes, &mut live, cancel)?;
    Ok(list)
}

/// Sets the line counts of the listed changes: the staged and unstaged ones from `git diff`, and
/// an untracked file's from the disk, where every line is an addition.
fn add_line_counts(
    env: &IsolatedGit,
    at: &Located,
    changes: &mut [ChangeEntry],
    live: &mut LiveSides,
    cancel: &AtomicBool,
) -> Result<()> {
    // Submodules are left out: a count never covers one, and `git` would look inside each.
    count_group(
        env,
        at,
        changes,
        |entry| matches!(entry, ChangeEntry::Staged { .. }),
        &[b"--cached", b"-M", b"--ignore-submodules=all"],
        cancel,
    )?;
    count_group(
        env,
        at,
        changes,
        |entry| matches!(entry, ChangeEntry::Unstaged { .. }),
        &[b"-M", b"--ignore-submodules=all"],
        cancel,
    )?;
    let mut remaining = line_counts::untracked_budget();
    for entry in changes
        .iter_mut()
        .filter(|entry| matches!(entry, ChangeEntry::Untracked { .. }))
    {
        let Some(rel) = line_counts::shape(entry).and_then(|shape| RelPath::parse(&shape.path))
        else {
            continue;
        };
        let Some(path) = live.exact_path(&rel) else {
            continue;
        };
        let stats = line_counts::count_file(&path, &rel, &mut remaining, cancel)?;
        line_counts::set(entry, stats);
    }
    Ok(())
}

/// Sets the counts of the changes of `changes` that `in_group` accepts, from `git diff <args>`.
fn count_group(
    env: &IsolatedGit,
    at: &Located,
    changes: &mut [ChangeEntry],
    in_group: fn(&ChangeEntry) -> bool,
    args: &[&[u8]],
    cancel: &AtomicBool,
) -> Result<()> {
    let wanted = line_counts::wanted(changes.iter().filter(|entry| in_group(entry)));
    if wanted.is_empty() {
        return Ok(());
    }
    let counts = line_counts::read_diff(env, at, "diff", args, &wanted, cancel)?;
    line_counts::apply(changes.iter_mut().filter(|entry| in_group(entry)), &counts);
    Ok(())
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
            old: head_side(head.as_deref(), modes[0], oids[0], &rel),
            new: index_side(&at.worktree, modes[1], oids[1], &rel),
            stats: None,
        });
    }
    if xy[1] != b'.' {
        entries.push(ChangeEntry::Unstaged {
            old: index_side(&at.worktree, modes[1], oids[1], &rel),
            new: live.tracked(&rel, modes[2]),
            stats: None,
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
                Some(from) => head_side(head.as_deref(), modes[0], oids[0], from),
                None => outside(original),
            },
            new: match &rel {
                Some(rel) => index_side(&at.worktree, modes[1], oids[1], rel),
                None => outside(path),
            },
            stats: None,
        }),
        b'C' => {
            if let Some(rel) = &rel {
                entries.push(ChangeEntry::Staged {
                    old: ChangeSide::Absent,
                    new: index_side(&at.worktree, modes[1], oids[1], rel),
                    stats: None,
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
                stats: None,
            }),
            (Some(from), None) => entries.push(ChangeEntry::Unstaged {
                old: index_side(&at.worktree, modes[1], oids[1], from),
                new: ChangeSide::Absent,
                stats: None,
            }),
            (None, Some(rel)) => entries.push(ChangeEntry::Unstaged {
                old: ChangeSide::Absent,
                new: live.tracked(rel, modes[2]),
                stats: None,
            }),
            (None, None) => {}
        },
        _ => {
            if let Some(rel) = &rel {
                entries.push(ChangeEntry::Unstaged {
                    old: index_side(&at.worktree, modes[1], oids[1], rel),
                    new: live.tracked(rel, modes[2]),
                    stats: None,
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
            stats: None,
        }],
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
        let Ok(stat) = platform::stat(&path, Links::Keep) else {
            return ChangeSide::Absent;
        };
        let kind = kind.unwrap_or(if stat.metadata.file_type().is_symlink() {
            SideKind::Symlink
        } else if stat.metadata.is_dir() {
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
                version: live::version(&stat),
            },
        }
    }
}

/// Reads the change `change` names in the worktree `at` names: both sides from its group's
/// sources, and its patch — see the module doc.
pub fn read(
    env: &IsolatedGit,
    at: &Located,
    change: &ChangeRef,
    cancel: &AtomicBool,
) -> Result<ChangeRead> {
    read_with(env, at, change, cancel, true)
}

/// [`read`], its sides carrying their bodies beside the patch where `inline_bodies` allows.
fn read_with(
    env: &IsolatedGit,
    at: &Located,
    change: &ChangeRef,
    cancel: &AtomicBool,
    inline_bodies: bool,
) -> Result<ChangeRead> {
    let (old, new) = (side_path(&change.old)?, side_path(&change.new)?);
    let out_of_scope = has_out_of_scope_side(&change.old, &change.new);
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
    let reader = Reader {
        env,
        at,
        cancel,
        inline_bodies,
    };
    match change.group {
        ChangeGroup::Staged => reader.staged(change, old_path, new_path, out_of_scope),
        ChangeGroup::Unstaged => reader.unstaged(old_path.unwrap(), new_path.unwrap()),
        ChangeGroup::Untracked => reader.untracked(new_path.unwrap()),
    }
}

/// Reads the whole bodies of the change `change` names in the worktree `at` names. Only a change
/// that has a patch has bodies to expand: an untracked file, and a change with a side outside the
/// project, are `invalid_change`. How bodies are matched and read is `change_read`'s.
pub fn read_bodies(
    env: &IsolatedGit,
    at: &Located,
    change: &ChangeRef,
    expected: &Expected,
    cancel: &AtomicBool,
) -> Result<BodiesRead> {
    if change.group == ChangeGroup::Untracked || has_out_of_scope_side(&change.old, &change.new) {
        return Err(no_bodies());
    }
    let read = read_with(env, at, change, cancel, false)?;
    // A change that was read names a side, and a side that is not out of scope names a path.
    let path = (side_path(&change.new)?.or(side_path(&change.old)?)).unwrap();
    bodies_of(env, at, read, &path, expected, cancel)
}
