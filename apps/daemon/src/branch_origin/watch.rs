//! When the hook path looks at a repository's branches, and what it remembers between looks.
//!
//! Looks are rationed: a filesystem stamp of the refs rules out nearly every hook, a floor per
//! repository bounds the rest, and a look held back by the floor, or one that left a branch
//! undecided, is followed by one trailing look.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};

use super::activity::SessionActivity;
use crate::protocol::now_millis;

/// The least time between the end of one enumeration of a repository and the start of the next
/// one the hook path makes. A commit rewrites the checked-out branch's ref, which changes the
/// [`RefsStamp`] as a new branch does, so a session committing in a loop would otherwise run a
/// `git for-each-ref` per tool call; agents' calls come seconds apart and a branch is only ever
/// attributed to the window it appeared in, so five seconds bounds the cost without moving
/// attribution anywhere that matters. It stays short enough that the trailing look after a held
/// back hook lands while the session is still in the same piece of work.
pub(super) const FLOOR: Duration = Duration::from_secs(5);

/// How many directories below `refs/heads` a [`RefsStamp`] looks at. A branch named `a/b/c` lives
/// in directories of its own, and a repository with more than this has its stamp cut: it still
/// changes whenever the part it covers does.
const MAX_STAMPED_DIRS: usize = 4_096;

/// The daemon's memory of each repository's branches between looks, and of what each session's
/// hooks said it was doing. Held on `AppState`; none of it is persisted, so a restart begins every
/// repository with no previous look.
#[derive(Default)]
pub struct Watch {
    repos: Mutex<HashMap<PathBuf, RepoLooks>>,
    activity: Mutex<SessionActivity>,
}

/// What the hook path remembers about one repository between its looks.
#[derive(Default)]
struct RepoLooks {
    /// The stamp taken before the last successful enumeration.
    stamp: Option<RefsStamp>,
    /// The branches the last full enumeration found; `None` before the first one, when nothing
    /// can be told apart as new.
    known: Option<HashSet<String>>,
    /// When the last enumeration ended, however it ended.
    completed: Option<Instant>,
    /// When the last enumeration began, in seconds since the epoch.
    looked_at_secs: Option<i64>,
    running: bool,
    /// A trailing look is already waiting.
    trailing: bool,
    /// The branches the Git mode's listings have seen appear since the last look after a hook,
    /// which the next such look treats as new whatever `known` now holds. A listing moves `known`
    /// on to what it found, and it runs every few seconds while the panel is open: without this,
    /// a listing landing between a branch's creation and the look that would attribute it buries
    /// the branch for good.
    fresh: HashSet<String>,
    /// The branches a look found new but could not decide yet; see [`UndecidedBranch`]. A listing
    /// leaves them alone, since one taken before a look is no evidence about them; the next look
    /// drops any that is gone.
    undecided: HashMap<String, UndecidedBranch>,
    /// When the last reconcile that changed this repository's rows ran; see [`Watch::wrote_at`].
    wrote_at: Option<Instant>,
    /// Serializes this repository's reconciles; see [`Watch::gate`].
    gate: Arc<Mutex<()>>,
}

/// A branch new in a look that the look could not decide yet: a tool call that ran when it was
/// born may have had its start announced by a hook still on its way. It is decided by a later look,
/// against the window it was found in, once every such hook has had time to land.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UndecidedBranch {
    /// The start of the window it was found new in, in seconds since the epoch.
    pub(super) since_secs: i64,
    /// When a listing first had it, in milliseconds since the epoch: the latest it can have been
    /// born.
    pub(super) seen_ms: i64,
    /// From when it can be decided, in milliseconds since the epoch.
    pub(super) due_ms: i64,
}

/// What the filesystem says about a repository's refs, compared for equality between two hooks.
///
/// A new branch, a deleted one and a moved one each rewrite a ref by creating and renaming a file,
/// which changes the modification time of the directory holding it — `refs/heads` itself, or the
/// directory of a name with a slash in it, so the whole tree of directories is taken (never the
/// files in it). Packed refs and the reftable list are the other places a ref can change.
///
/// The resolution of those times is the filesystem's. On one that keeps modification times to the
/// whole second, a branch made in the same second as the previous look's stamp leaves the stamp
/// equal and the hook sees nothing to look at; the branch is then attributed by the Git mode's
/// listing from its reflog, or not at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RefsStamp {
    directories: usize,
    newest: Option<SystemTime>,
    packed_refs: Option<SystemTime>,
    reftable: Option<SystemTime>,
}

impl RefsStamp {
    pub(super) fn read(common_dir: &Path) -> Self {
        let modified = |path: &Path| fs::metadata(path).and_then(|m| m.modified()).ok();
        let mut stamp = RefsStamp {
            directories: 0,
            newest: None,
            packed_refs: modified(&common_dir.join("packed-refs")),
            reftable: modified(&common_dir.join("reftable/tables.list")),
        };
        let mut pending = vec![common_dir.join("refs/heads")];
        while let Some(dir) = pending.pop() {
            let Ok(metadata) = fs::symlink_metadata(&dir) else {
                continue;
            };
            if stamp.directories == MAX_STAMPED_DIRS {
                break;
            }
            if !metadata.is_dir() {
                continue;
            }
            stamp.directories += 1;
            stamp.newest = stamp.newest.max(metadata.modified().ok());
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            // In a fixed order, since the walk stops counting past the limit: `read_dir` gives no
            // order of its own, so without this two walks of a repository nothing changed in could
            // cover different directories and disagree.
            let mut children: Vec<PathBuf> = entries
                .flatten()
                .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                .map(|entry| entry.path())
                .collect();
            children.sort();
            pending.extend(children);
        }
        stamp
    }
}

/// Who is asking for a look.
pub(super) enum Caller {
    Hook,
    /// The look scheduled for when the floor is over, or for when an undecided branch is due.
    Trailing,
}

pub(super) enum Claim<'a> {
    /// Nothing to look at, or somebody else is already looking.
    Nothing,
    /// Look at the repository after this long.
    Later(Duration),
    Run(ClaimedLook<'a>),
}

/// The right to enumerate one repository, released on `Drop` even if the enumeration panics: a
/// repository whose claim is never released would never be looked at again.
pub(super) struct ClaimedLook<'a> {
    watch: &'a Watch,
    pub(super) repo: PathBuf,
    pub(super) known: Option<HashSet<String>>,
    pub(super) fresh: HashSet<String>,
    pub(super) undecided: HashMap<String, UndecidedBranch>,
    pub(super) previous_secs: Option<i64>,
    finished: bool,
}

impl ClaimedLook<'_> {
    /// Records the look as done: `stamp` as it stood before the enumeration, `known` when the
    /// enumeration was complete, and what it left `undecided`. Returns how long until the trailing
    /// look an undecided branch needs, when no trailing look is waiting already.
    pub(super) fn finish(
        mut self,
        stamp: RefsStamp,
        known: Option<HashSet<String>>,
        undecided: HashMap<String, UndecidedBranch>,
    ) -> Option<Duration> {
        let mut repos = self.watch.lock();
        self.finished = true;
        let entry = repos.get_mut(&self.repo)?;
        entry.stamp = Some(stamp);
        if known.is_some() {
            entry.known = known;
        }
        entry.running = false;
        entry.completed = Some(Instant::now());
        entry.undecided = undecided;
        if entry.trailing {
            return None;
        }
        let due = entry.undecided.values().map(|branch| branch.due_ms).min()?;
        entry.trailing = true;
        Some(until(due, now_millis()))
    }
}

impl Drop for ClaimedLook<'_> {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        // A look that failed leaves the next one everything this one was given: the branches a
        // listing handed it are still unattributed, what it was to decide is still undecided, and
        // the time it started from is still the baseline a branch made since has to be newer
        // than.
        let mut repos = self.watch.lock();
        if let Some(entry) = repos.get_mut(&self.repo) {
            entry.running = false;
            entry.completed = Some(Instant::now());
            entry.fresh.extend(self.fresh.drain());
            entry.looked_at_secs = self.previous_secs;
        }
    }
}

fn until(due_ms: i64, now_ms: i64) -> Duration {
    Duration::from_millis(u64::try_from(due_ms - now_ms).unwrap_or(0))
}

impl Watch {
    fn lock(&self) -> MutexGuard<'_, HashMap<PathBuf, RepoLooks>> {
        self.repos.lock().expect("branch watch lock poisoned")
    }

    pub(super) fn activity(&self) -> MutexGuard<'_, SessionActivity> {
        self.activity
            .lock()
            .expect("session activity lock poisoned")
    }

    /// Whether to look at the repository now, `now` and `now_ms` being the same moment on the two
    /// clocks: a look is owed when the stamp changed, or when an undecided branch is due.
    pub(super) fn claim(
        &self,
        repo: &Path,
        caller: Caller,
        stamp: &RefsStamp,
        now: Instant,
        now_ms: i64,
    ) -> Claim<'_> {
        let mut repos = self.lock();
        let entry = repos.entry(repo.to_path_buf()).or_default();
        if let Caller::Trailing = caller {
            entry.trailing = false;
        }
        let next_due = entry.undecided.values().map(|branch| branch.due_ms).min();
        if entry.stamp.as_ref() == Some(stamp) && next_due.is_none_or(|due| due > now_ms) {
            // A trailing look that came early for an undecided branch waits for it again.
            return match (caller, next_due) {
                (Caller::Trailing, Some(due)) => {
                    entry.trailing = true;
                    Claim::Later(until(due, now_ms))
                }
                _ => Claim::Nothing,
            };
        }
        if entry.running {
            return Claim::Nothing;
        }
        if let Some(done) = entry.completed {
            let since = now.saturating_duration_since(done);
            if since < FLOOR {
                if entry.trailing {
                    return Claim::Nothing;
                }
                entry.trailing = true;
                return Claim::Later(FLOOR - since);
            }
        }
        entry.running = true;
        Claim::Run(ClaimedLook {
            watch: self,
            repo: repo.to_path_buf(),
            known: entry.known.clone(),
            fresh: std::mem::take(&mut entry.fresh),
            undecided: entry.undecided.clone(),
            previous_secs: entry.looked_at_secs.replace(now_ms / 1000),
            finished: false,
        })
    }

    /// Takes a full listing the Git mode made as the repository's branches, and hands the names it
    /// is the first to see to the next look after a hook. The listing is not a window: it says
    /// which branches exist, not who was running a tool, so a branch it sees first is still the
    /// next look's to attribute. A name this listing no longer has is dropped rather than kept
    /// waiting for a look that may never come, and loses nothing: a name outside `known` is new to
    /// the next look on `known` alone.
    pub(super) fn observe(&self, repo: &Path, names: HashSet<String>) {
        let mut repos = self.lock();
        let entry = repos.entry(repo.to_path_buf()).or_default();
        if let Some(known) = &entry.known {
            entry.fresh.retain(|name| names.contains(name));
            entry
                .fresh
                .extend(names.iter().filter(|name| !known.contains(*name)).cloned());
        }
        entry.known = Some(names);
    }

    /// The gate serializing every write to one repository's rows. Both reconciling paths read
    /// every row of the repository, decide from that snapshot and then write, so two of them at
    /// once lose an attribution or delete a row the other just wrote; the hook path's own claim
    /// serializes hook looks against each other only, and several clients can be listing at the
    /// same time. The clearing a project's deletion does takes it for the same reason.
    ///
    /// It guards an order rather than any state, so a panic inside it poisons nothing worth
    /// keeping: a poisoned gate is taken all the same, since refusing it would leave the
    /// repository unattributable for the rest of the daemon's life.
    pub(super) fn gate(&self, repo: &Path) -> Arc<Mutex<()>> {
        Arc::clone(&self.lock().entry(repo.to_path_buf()).or_default().gate)
    }

    /// When a reconcile last changed the repository's rows. A branch listing taken before then is
    /// no evidence about which branches exist now, however complete it is, since what that
    /// reconcile recorded cannot be in it. A reconcile that changed nothing leaves this alone: a
    /// listing older than that one is as good as a newer one.
    ///
    /// Only the order of the two matters, so both are [`Instant`]s: a wall clock stepped back —
    /// by NTP, or by the user — would otherwise put every listing before the last write for as
    /// long as the step, which stops a row ever being removed for a deleted branch.
    pub(super) fn wrote_at(&self, repo: &Path) -> Option<Instant> {
        self.lock().get(repo).and_then(|entry| entry.wrote_at)
    }

    pub(super) fn wrote(&self, repo: &Path, at: Instant) {
        self.lock().entry(repo.to_path_buf()).or_default().wrote_at = Some(at);
    }

    pub(super) fn forget(&self, repo: &Path) {
        self.lock().remove(repo);
    }

    #[cfg(test)]
    pub(super) fn holds_no_repository(&self) -> bool {
        self.lock().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::branch_origin::git_files::repository_of;
    use crate::test_support::{git, move_directory_time_back, repo_with};

    fn stamp(n: usize) -> RefsStamp {
        RefsStamp {
            directories: n,
            newest: None,
            packed_refs: None,
            reftable: None,
        }
    }

    #[test]
    fn the_stamp_sees_a_branch_below_a_directory_that_already_existed() {
        let repo = repo_with("branch-origin-stamp", &[(b"a", b"a")]);
        let common_dir = repository_of(&repo).unwrap();
        git(&repo, &["branch", "feat/one"]);
        // Every directory the stamp covers, not just the top one, so that what the next branch
        // changes is the directory below rather than the top one.
        for dir in ["refs/heads", "refs/heads/feat"] {
            move_directory_time_back(&common_dir.join(dir));
        }
        let before = RefsStamp::read(&common_dir);
        assert_eq!(before, RefsStamp::read(&common_dir));

        git(&repo, &["branch", "feat/two"]);

        assert_ne!(before, RefsStamp::read(&common_dir));
    }

    #[test]
    fn a_hook_inside_the_floor_waits_for_it() {
        let watch = Watch::default();
        let repo = Path::new("/repo");
        let now = Instant::now();
        let ms = now_millis();

        let Claim::Run(look) = watch.claim(repo, Caller::Hook, &stamp(1), now, ms) else {
            panic!("the first hook looks");
        };
        look.finish(stamp(1), Some(HashSet::new()), HashMap::new());
        assert!(matches!(
            watch.claim(repo, Caller::Hook, &stamp(1), now, ms),
            Claim::Nothing
        ));

        let Claim::Later(wait) = watch.claim(repo, Caller::Hook, &stamp(2), Instant::now(), ms)
        else {
            panic!("a changed stamp inside the floor waits");
        };
        assert!(wait <= FLOOR);
        assert!(
            matches!(
                watch.claim(repo, Caller::Hook, &stamp(2), Instant::now(), ms),
                Claim::Nothing
            ),
            "one trailing look is enough"
        );

        let later = Instant::now() + FLOOR;
        assert!(
            matches!(
                watch.claim(repo, Caller::Trailing, &stamp(2), later, ms),
                Claim::Run(_)
            ),
            "the trailing look runs"
        );
    }

    /// The refs do not change again after a branch is left undecided, so its look has to come
    /// from the watch itself.
    #[test]
    fn a_branch_left_undecided_gets_a_look_though_the_refs_stay_still() {
        let watch = Watch::default();
        let repo = Path::new("/repo");
        let ms = now_millis();
        let past_the_floor = || Instant::now() + FLOOR;
        let Claim::Run(look) = watch.claim(repo, Caller::Hook, &stamp(1), past_the_floor(), ms)
        else {
            panic!("the first hook looks");
        };
        let due_ms = ms + 8_000;
        let undecided = HashMap::from([(
            "feature".to_string(),
            UndecidedBranch {
                since_secs: ms / 1000,
                seen_ms: ms,
                due_ms,
            },
        )]);

        let wait = look.finish(stamp(1), Some(HashSet::new()), undecided.clone());

        assert!(wait.is_some_and(|wait| wait <= Duration::from_millis(8_000)));
        assert!(matches!(
            watch.claim(repo, Caller::Hook, &stamp(1), past_the_floor(), ms),
            Claim::Nothing
        ));
        assert!(
            matches!(
                watch.claim(repo, Caller::Trailing, &stamp(1), past_the_floor(), ms),
                Claim::Later(_)
            ),
            "a trailing look that comes early waits again"
        );
        let Claim::Run(look) =
            watch.claim(repo, Caller::Trailing, &stamp(1), past_the_floor(), due_ms)
        else {
            panic!("the due branch is looked at");
        };
        assert_eq!(look.undecided, undecided);
    }

    #[test]
    fn a_look_that_failed_leaves_the_next_one_its_whole_window() {
        let watch = Watch::default();
        let repo = Path::new("/repo");
        let ms = now_millis();
        let main = HashSet::from(["main".to_string()]);
        let Claim::Run(first) = watch.claim(repo, Caller::Hook, &stamp(1), Instant::now(), ms)
        else {
            panic!("the first hook looks");
        };
        first.finish(stamp(1), Some(main.clone()), HashMap::new());
        watch.observe(
            repo,
            HashSet::from(["main".to_string(), "feature".to_string()]),
        );
        // A baseline well in the past, so advancing it would be visible inside one second.
        let baseline = Some(1_700_000_000);
        watch.lock().get_mut(repo).unwrap().looked_at_secs = baseline;

        let later = Instant::now() + FLOOR;
        let Claim::Run(failed) = watch.claim(repo, Caller::Hook, &stamp(2), later, ms) else {
            panic!("the second hook looks");
        };
        assert_eq!(failed.previous_secs, baseline);
        assert_eq!(failed.fresh, HashSet::from(["feature".to_string()]));
        drop(failed);

        // The failed look ended just now, so the next one comes once its own floor is over.
        let past_the_floor = Instant::now() + FLOOR;
        let Claim::Run(next) = watch.claim(repo, Caller::Hook, &stamp(2), past_the_floor, ms)
        else {
            panic!("the next hook looks");
        };
        assert_eq!(
            next.previous_secs, baseline,
            "a branch made before the failed look is still inside the window"
        );
        assert_eq!(next.fresh, HashSet::from(["feature".to_string()]));
    }
}
