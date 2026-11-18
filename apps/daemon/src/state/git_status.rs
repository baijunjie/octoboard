//! Each project's live git status: the cache that is broadcast and replayed in a snapshot, the
//! claim that keeps two checks of one project from racing, the marker of a push in flight and the
//! time of the last check.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;

use tokio::sync::oneshot;

use super::AppState;
use crate::protocol::{CodedError, Event, GitActivity, GitStatus, PullOutcome};
use crate::time::now_millis;

pub(crate) fn blank_git_status(project_id: &str) -> GitStatus {
    GitStatus {
        project: project_id.to_string(),
        repository: true,
        branch: None,
        detached: false,
        upstream: None,
        ahead: 0,
        behind: 0,
        activity: GitActivity::Idle,
        error: None,
        error_code: None,
    }
}

/// The base check status and its push overlay share a lock so an overlapping publication cannot
/// briefly clear the pushing glyph, and broadcasts retain the same order as cached snapshots.
#[derive(Default)]
pub(super) struct GitStatuses {
    statuses: HashMap<String, GitStatus>,
    /// Projects with a push in flight, each with the token of the push that marked it, so a
    /// finishing push removes only its own marker.
    pushes: HashMap<String, u64>,
    next_push_token: u64,
}

impl GitStatuses {
    fn visible(&self, mut status: GitStatus) -> GitStatus {
        if status.repository && self.pushes.contains_key(&status.project) {
            status.activity = GitActivity::Pushing;
        }
        status
    }
}

pub(crate) struct GitPushActivity<'a> {
    state: &'a AppState,
    project_id: String,
    /// The token of the marker this push set; `None` for a project no longer in the store, which
    /// holds no marker to release.
    token: Option<u64>,
}

impl Drop for GitPushActivity<'_> {
    fn drop(&mut self) {
        let Some(token) = self.token else {
            return;
        };
        let mut statuses = self
            .state
            .git_statuses
            .write()
            .expect("git statuses lock poisoned");
        // The marker may be gone, or a newer push's: the project was removed and registered again
        // while this push ran.
        if statuses.pushes.get(&self.project_id) == Some(&token) {
            statuses.pushes.remove(&self.project_id);
        }
        if let Some(status) = statuses.statuses.get(&self.project_id) {
            self.state.broadcast(Event::ProjectGitStatus {
                status: statuses.visible(status.clone()),
            });
        }
    }
}

/// What a check run for a `pull_project_branch` answers it with: the outcome, or the refusal.
pub type PullAnswer = Result<PullOutcome, CodedError>;

/// What the in-flight map holds for one project's claim.
pub(super) struct GitCheck {
    /// Waiting for a place to run behind a sweep: true only for a claim made by
    /// `AppState::claim_git_check_queued` and not yet started ([`GitCheckClaim::start`]); every
    /// other claim runs from the start. Only a request that finds it true has anyone to wake.
    queued: bool,
    /// The check a request that arrived while the check was queued or running is owed: a queued
    /// check then starts at once as that check, a running one runs it once more before letting go.
    owed: Option<OwedCheck>,
    /// The re-check requests (`refresh_project_git_status`) waiting for the end of the owed check,
    /// each answered with the status it ends with.
    waiting: Vec<oneshot::Sender<GitStatus>>,
    /// The pull requests (`pull_project_branch`) waiting for the end of the owed check, each
    /// answered with what that check's pull did.
    pulls: Vec<oneshot::Sender<PullAnswer>>,
    /// Woken when a pull or a re-check finds the check queued.
    promote: Arc<tokio::sync::Notify>,
}

/// The check a request owes the holder of a project's claim, once its current check ends (or at
/// once, if it is still waiting for a place). Both look for a `git` that cannot be found once more;
/// a pull, which also pulls the branch and answers its own requests, covers a re-check, hence the
/// order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum OwedCheck {
    /// A re-check (`refresh_project_git_status`).
    Recheck,
    /// A manual pull (`pull_project_branch`).
    Pull,
}

/// Who waits for the check a request owes, and how it is answered.
enum Waiter {
    Recheck(oneshot::Sender<GitStatus>),
    Pull(oneshot::Sender<PullAnswer>),
}

/// The right to check one project's git status right now, released by `release_unless_rerun_owed`
/// or, when a panic unwinds through the holder, on drop, which is what keeps a wedged step from
/// excluding its project from every later sweep for the rest of the daemon's life. See
/// `AppState::claim_git_check_queued`.
pub struct GitCheckClaim {
    state: Arc<AppState>,
    project_id: String,
    promote: Arc<tokio::sync::Notify>,
    /// The re-check requests the holder's next check answers ([`Self::answer`]); dropped
    /// unanswered with the claim, which ends their wait without a status.
    answering: Vec<oneshot::Sender<GitStatus>>,
    /// The pull requests the holder's next check answers ([`Self::answer_pull`]); dropped
    /// unanswered with the claim, like `answering`.
    answering_pulls: Vec<oneshot::Sender<PullAnswer>>,
    released: bool,
}

impl GitCheckClaim {
    /// Resolves when a manual pull or a re-check finds this claim still queued, for the holder to
    /// stop waiting for a place and run at once.
    pub async fn promoted(&self) {
        self.promote.notified().await;
    }

    /// Marks a queued claim as running, for the holder that has stopped waiting, and returns the
    /// check a request that arrived meanwhile is owed, which the check then runs as, owing no rerun
    /// for it; the re-checks and pulls waiting for it are answered by this check. Done under the
    /// lock requests are marked under, so one arriving after this is owed a rerun instead.
    pub fn start(&mut self) -> Option<OwedCheck> {
        let mut checks = self
            .state
            .git_checks
            .lock()
            .expect("git check lock poisoned");
        let check = checks.get_mut(&self.project_id)?;
        check.queued = false;
        self.answering.append(&mut check.waiting);
        self.answering_pulls.append(&mut check.pulls);
        check.owed.take()
    }

    /// Releases the claim, unless a request arrived while it was held: then the claim is kept and
    /// the check that request is owed is returned, for the holder to run once more, and the
    /// re-checks and pulls waiting for it are answered by that run. Done under the lock requests
    /// are marked under, so a request is either seen here or finds the project unclaimed.
    pub fn release_unless_rerun_owed(&mut self) -> Option<OwedCheck> {
        let mut checks = self
            .state
            .git_checks
            .lock()
            .expect("git check lock poisoned");
        if let Some(check) = checks.get_mut(&self.project_id) {
            if let Some(owed) = check.owed.take() {
                self.answering.append(&mut check.waiting);
                self.answering_pulls.append(&mut check.pulls);
                return Some(owed);
            }
        }
        checks.remove(&self.project_id);
        self.released = true;
        None
    }

    /// Answers the re-check requests this check was run for with `status`, the status it ended
    /// with.
    pub fn answer(&mut self, status: &GitStatus) {
        for waiting in self.answering.drain(..) {
            // A request that stopped waiting has nothing left to answer.
            let _ = waiting.send(status.clone());
        }
    }

    /// Answers the pull requests this check was run for with `answer`, what its pull did.
    pub fn answer_pull(&mut self, answer: &PullAnswer) {
        for waiting in self.answering_pulls.drain(..) {
            // A request that stopped waiting has nothing left to answer.
            let _ = waiting.send(answer.clone());
        }
    }
}

impl Drop for GitCheckClaim {
    fn drop(&mut self) {
        if !self.released {
            self.state.release_git_check(&self.project_id);
        }
    }
}

impl AppState {
    /// Claims the right to check this project's git status, for a check that waits for a place
    /// before it runs (a sweep's, or `crate::project_git::status::pull_behind_projects`'s).
    /// Released by [`GitCheckClaim::release_unless_rerun_owed`] or, if the check panics, when the
    /// returned guard is dropped, so a wedged step cannot leave its project excluded from every
    /// later sweep for the rest of the daemon's life. `None` means a check for it is already
    /// queued or running and the caller must not start another. The claim is held from the moment
    /// it is queued, so another sweep does not queue the project twice, and a pull or a re-check
    /// that finds it still waiting takes it over instead of waiting behind the sweep (see
    /// [`GitCheckClaim::start`]).
    pub fn claim_git_check_queued(self: &Arc<Self>, project_id: &str) -> Option<GitCheckClaim> {
        self.claim_git_check(project_id, true)
    }

    /// As [`Self::claim_git_check_queued`], for work that starts at once and waits for no place,
    /// such as the confirmed merge of `pull_project_branch`: the claim is not queued, so a request
    /// that arrives meanwhile is owed a check, handed back by
    /// [`GitCheckClaim::release_unless_rerun_owed`], and wakes no one.
    pub fn claim_git_check_now(self: &Arc<Self>, project_id: &str) -> Option<GitCheckClaim> {
        self.claim_git_check(project_id, false)
    }

    /// The re-check of `refresh_project_git_status`: a claim for a check that starts at once, when
    /// the project has none, and the receiver of the status the check that answers the request
    /// ends with. When a check already holds the claim, nothing is claimed: the holder is owed a
    /// re-check, which it runs once its current check ends, or at once in its stead while it still
    /// waits for a place, and that run answers. Owed even to a check that already looks for a
    /// missing `git` again, which may have looked before the person installed one. The receiver
    /// gets nothing (its sender dropped) when the check is abandoned: its project removed, a panic.
    pub fn claim_git_recheck(
        self: &Arc<Self>,
        project_id: &str,
    ) -> (Option<GitCheckClaim>, oneshot::Receiver<GitStatus>) {
        let (answer, answered) = oneshot::channel();
        let claim = self.claim_git_owing(project_id, OwedCheck::Recheck, Waiter::Recheck(answer));
        (claim, answered)
    }

    /// The pull of `pull_project_branch` without `merge`: as [`Self::claim_git_recheck`], for a
    /// check that pulls the branch, and the receiver of what that pull did. The holder owes the
    /// pull a check of its own that starts after the request, so the answer always comes from a
    /// fetch made after it. The receiver gets nothing (its sender dropped) when the check is
    /// abandoned.
    pub fn claim_git_pull(
        self: &Arc<Self>,
        project_id: &str,
    ) -> (Option<GitCheckClaim>, oneshot::Receiver<PullAnswer>) {
        let (answer, answered) = oneshot::channel();
        let claim = self.claim_git_owing(project_id, OwedCheck::Pull, Waiter::Pull(answer));
        (claim, answered)
    }

    /// What a request for the check `owed` does: claims the project for a check that starts at
    /// once when it has none, with `waiter` answered by that check, and otherwise claims nothing
    /// but marks the holder's claim as owing that check (never a lesser one than it already owes)
    /// with `waiter` waiting for it, and wakes a queued holder to run it at once. The marking and
    /// the claim share one lock, so a request cannot slip in between the owner's last look and its
    /// release.
    fn claim_git_owing(
        self: &Arc<Self>,
        project_id: &str,
        owed: OwedCheck,
        waiter: Waiter,
    ) -> Option<GitCheckClaim> {
        let mut checks = self.git_checks.lock().expect("git check lock poisoned");
        if let Some(check) = checks.get_mut(project_id) {
            check.owed = check.owed.max(Some(owed));
            match waiter {
                Waiter::Recheck(answer) => check.waiting.push(answer),
                Waiter::Pull(answer) => check.pulls.push(answer),
            }
            if check.queued {
                check.promote.notify_one();
            }
            return None;
        }
        let mut claim = self.insert_git_check(&mut checks, project_id, false);
        match waiter {
            Waiter::Recheck(answer) => claim.answering.push(answer),
            Waiter::Pull(answer) => claim.answering_pulls.push(answer),
        }
        Some(claim)
    }

    fn claim_git_check(self: &Arc<Self>, project_id: &str, queued: bool) -> Option<GitCheckClaim> {
        let mut checks = self.git_checks.lock().expect("git check lock poisoned");
        if checks.contains_key(project_id) {
            return None;
        }
        Some(self.insert_git_check(&mut checks, project_id, queued))
    }

    /// Enters `project_id` in the in-flight map `checks`, which the caller holds locked and has
    /// found without it, and returns the claim to it.
    fn insert_git_check(
        self: &Arc<Self>,
        checks: &mut HashMap<String, GitCheck>,
        project_id: &str,
        queued: bool,
    ) -> GitCheckClaim {
        let promote = Arc::new(tokio::sync::Notify::new());
        checks.insert(
            project_id.to_string(),
            GitCheck {
                queued,
                owed: None,
                waiting: Vec::new(),
                pulls: Vec::new(),
                promote: promote.clone(),
            },
        );
        GitCheckClaim {
            state: self.clone(),
            project_id: project_id.to_string(),
            promote,
            answering: Vec::new(),
            answering_pulls: Vec::new(),
            released: false,
        }
    }

    fn release_git_check(&self, project_id: &str) {
        self.git_checks
            .lock()
            .expect("git check lock poisoned")
            .remove(project_id);
    }

    /// Records a project's current git status and broadcasts it — unless the project is no longer
    /// in the store, in which case the entry is dropped instead of reinserted. This is what covers
    /// both a project removed while its check was in flight and a whole console deleted out from
    /// under one: either way nothing resurrects a status for a project that no longer exists, and
    /// `PROTOCOL.md` promises no `project_git_status` for one either.
    pub fn publish_git_status(&self, status: GitStatus) {
        // A store error says nothing about whether the project is actually gone, so this fails
        // open rather than risk dropping a status that is still perfectly valid.
        if let Ok(None) = self.store.get_project(&status.project) {
            self.remove_git_status(&status.project);
            return;
        }
        let mut statuses = self
            .git_statuses
            .write()
            .expect("git statuses lock poisoned");
        statuses
            .statuses
            .insert(status.project.clone(), status.clone());
        self.broadcast(Event::ProjectGitStatus {
            status: statuses.visible(status),
        });
    }

    /// Records a local re-read of the branch header and broadcasts the result. Only what the read
    /// produced is merged into the held status, under the lock: a read that finished after an
    /// overlapping check or pull published must not bring back that check's older `error` or end
    /// its `activity`. A failed read records its own `error` and leaves the header as it was. A read
    /// that succeeded clears a held failure to find `git` at all (`error_code` `binary_not_found`),
    /// which its own run has just disproved, though no other: those stand until their own step
    /// succeeds. As with [`Self::publish_git_status`], a project no longer in the store is dropped
    /// instead.
    pub(crate) fn publish_git_reread(
        &self,
        project_id: &str,
        header: Result<crate::project_git::header::BranchHeader, crate::project_git::StepFailure>,
    ) {
        if let Ok(None) = self.store.get_project(project_id) {
            self.remove_git_status(project_id);
            return;
        }
        let mut statuses = self
            .git_statuses
            .write()
            .expect("git statuses lock poisoned");
        let status = statuses
            .statuses
            .entry(project_id.to_string())
            .or_insert_with(|| blank_git_status(project_id));
        status.repository = true;
        match header {
            Ok(header) => {
                crate::project_git::header::apply_header(status, header);
                if status.error_code == Some(crate::protocol::error_code::BINARY_NOT_FOUND) {
                    status.error = None;
                    status.error_code = None;
                }
            }
            Err(err) => err.record(status),
        }
        let status = status.clone();
        self.broadcast(Event::ProjectGitStatus {
            status: statuses.visible(status),
        });
    }

    /// Every git status the daemon currently holds, for `Event::Snapshot`.
    pub fn git_statuses(&self) -> Vec<GitStatus> {
        let statuses = self
            .git_statuses
            .read()
            .expect("git statuses lock poisoned");
        statuses
            .statuses
            .values()
            .cloned()
            .map(|status| statuses.visible(status))
            .collect()
    }

    /// Marks one push in flight, without excluding a background check. `None` when the project
    /// already has a push in flight: one repository runs one `pre-push` hook at a time, so the
    /// second push is refused rather than started alongside. The guard restores the latest
    /// underlying check activity on every exit, including a failure.
    pub(crate) fn begin_git_push(&self, project_id: &str) -> Option<GitPushActivity<'_>> {
        if let Ok(None) = self.store.get_project(project_id) {
            return Some(GitPushActivity {
                state: self,
                project_id: project_id.to_string(),
                token: None,
            });
        }
        let mut statuses = self
            .git_statuses
            .write()
            .expect("git statuses lock poisoned");
        if statuses.pushes.contains_key(project_id) {
            return None;
        }
        let token = statuses.next_push_token;
        statuses.next_push_token += 1;
        statuses.pushes.insert(project_id.to_string(), token);
        let status = statuses
            .statuses
            .entry(project_id.to_string())
            .or_insert_with(|| blank_git_status(project_id));
        status.repository = true;
        let status = status.clone();
        self.broadcast(Event::ProjectGitStatus {
            status: statuses.visible(status),
        });
        Some(GitPushActivity {
            state: self,
            project_id: project_id.to_string(),
            token: Some(token),
        })
    }

    /// The underlying check status for one project, without the push overlay.
    pub fn git_status(&self, project_id: &str) -> Option<GitStatus> {
        self.git_statuses
            .read()
            .expect("git statuses lock poisoned")
            .statuses
            .get(project_id)
            .cloned()
    }

    /// One project's git status as clients see it, the push overlay included, and how long ago its
    /// last check completed (`None` when none has since the daemon started), counted on the wall
    /// clock so a sleep in between is part of it. `None` for the status when the daemon holds none
    /// for the project. Read from memory: nothing is checked.
    pub fn git_status_with_age(&self, project_id: &str) -> Option<(GitStatus, Option<Duration>)> {
        let statuses = self
            .git_statuses
            .read()
            .expect("git statuses lock poisoned");
        let status = statuses.statuses.get(project_id).cloned()?;
        let completed_ms = self
            .git_check_completed
            .lock()
            .expect("git check completed lock poisoned")
            .get(project_id)
            .map(|(_, wall)| *wall);
        // A wall clock set back since reads as no time at all, not as a negative age.
        let age = completed_ms
            .map(|wall| Duration::from_millis(u64::try_from(now_millis() - wall).unwrap_or(0)));
        Some((statuses.visible(status), age))
    }

    /// Drops a project's git status. Called when the project itself is removed, and when a whole
    /// console is — nothing else ever cleans up these entries, since the daemon keeps no timer of
    /// its own that would otherwise notice a project is gone.
    pub fn remove_git_status(&self, project_id: &str) {
        let mut statuses = self
            .git_statuses
            .write()
            .expect("git statuses lock poisoned");
        statuses.statuses.remove(project_id);
        statuses.pushes.remove(project_id);
        drop(statuses);
        self.git_check_completed
            .lock()
            .expect("git check completed lock poisoned")
            .remove(project_id);
    }

    /// When this project's check last completed, for `crate::project_git::status::due_for_check`.
    /// `None` means it has never been checked, which that function always treats as due.
    pub fn git_check_completed_at(&self, project_id: &str) -> Option<Instant> {
        self.git_check_completed
            .lock()
            .expect("git check completed lock poisoned")
            .get(project_id)
            .map(|(instant, _)| *instant)
    }

    /// Records that this project's check just completed, now.
    pub fn record_git_check_completed(&self, project_id: &str) {
        self.git_check_completed
            .lock()
            .expect("git check completed lock poisoned")
            .insert(project_id.to_string(), (Instant::now(), now_millis()));
    }

    /// Moves the wall-clock time of a project's recorded check, to test the age it reports without
    /// waiting for it.
    #[cfg(test)]
    pub(crate) fn set_git_check_wall_time_for_test(&self, project_id: &str, wall_ms: i64) {
        if let Some(entry) = self
            .git_check_completed
            .lock()
            .expect("git check completed lock poisoned")
            .get_mut(project_id)
        {
            entry.1 = wall_ms;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::blank_git_status;
    use crate::protocol::{
        error_code, CodedError, Event, GitActivity, GitStatus, PullOutcome, SessionStatus,
    };
    use crate::state::test_support::insert_record;
    use crate::state::{AppState, OwedCheck};
    use crate::test_support::app_state;

    /// A check that panics must still release its project's claim, or a wedged step excludes that
    /// project from every later sweep for the rest of the daemon's life — `run_checks`
    /// (`crate::project_git::status`) relies on unwinding through the claim's `Drop` rather than an
    /// explicit `end_git_check` call.
    #[test]
    fn a_git_check_claim_is_released_even_if_the_check_panics() {
        let (state, _dir) = app_state("state-git-claim-panic");
        let claim = state.claim_git_check_queued("p1").expect("the first claim");
        assert!(state.claim_git_check_queued("p1").is_none());

        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _claim = claim;
            panic!("simulated check failure");
        }));
        assert!(unwound.is_err());

        // The panic unwound through the claim's `Drop`, releasing it for the next sweep.
        assert!(state.claim_git_check_queued("p1").is_some());
    }

    /// A manual pull that finds a check running must not be lost: it makes the holder run once
    /// more, once, and only then release.
    #[test]
    fn a_manual_pull_during_a_check_is_owed_one_rerun() {
        let (state, _dir) = app_state("state-git-claim-rerun");
        let mut claim = state.claim_git_check_queued("p1").expect("the first claim");
        assert!(state.claim_git_pull("p1").0.is_none());
        assert!(state.claim_git_pull("p1").0.is_none());

        assert_eq!(
            claim.release_unless_rerun_owed(),
            Some(OwedCheck::Pull),
            "the request is handed back"
        );
        assert!(
            state.claim_git_check_queued("p1").is_none(),
            "the claim is still held for the rerun"
        );
        assert_eq!(
            claim.release_unless_rerun_owed(),
            None,
            "owed once, however many asked"
        );
        assert!(
            state.claim_git_check_queued("p1").is_some(),
            "then it is released"
        );
    }

    /// A manual pull that finds a sweep's check still waiting for a place takes it over: the
    /// waiting claim is woken and starts as the pull, and nothing more is owed afterwards.
    #[tokio::test]
    async fn a_manual_pull_promotes_a_queued_check() {
        let (state, _dir) = app_state("state-git-claim-queued");
        let mut claim = state.claim_git_check_queued("p1").expect("the first claim");
        assert!(state.claim_git_check_queued("p1").is_none());
        assert!(state.claim_git_pull("p1").0.is_none());

        tokio::time::timeout(crate::test_support::PATIENCE, claim.promoted())
            .await
            .expect("the queued claim is woken");
        assert_eq!(claim.start(), Some(OwedCheck::Pull), "it runs as the pull");
        assert_eq!(
            claim.release_unless_rerun_owed(),
            None,
            "the request is not owed a second run"
        );
        assert!(state.claim_git_check_queued("p1").is_some());
    }

    /// A manual pull arriving after the queued check started is owed a rerun, as for any running
    /// check.
    #[test]
    fn a_manual_pull_after_a_queued_check_started_is_owed_a_rerun() {
        let (state, _dir) = app_state("state-git-claim-started");
        let mut claim = state.claim_git_check_queued("p1").expect("the first claim");
        assert_eq!(claim.start(), None, "nothing asked yet");
        assert!(state.claim_git_pull("p1").0.is_none());
        assert_eq!(claim.release_unless_rerun_owed(), Some(OwedCheck::Pull));
    }

    /// A re-check of a project nobody is checking claims it, to run at once, and is answered by
    /// that check.
    #[test]
    fn a_recheck_of_an_unclaimed_project_runs_it_and_is_answered_by_it() {
        let (state, _dir) = app_state("state-git-recheck-unclaimed");
        let (claim, mut answered) = state.claim_git_recheck("p1");
        let mut claim = claim.expect("the project was free");
        assert!(state.claim_git_check_queued("p1").is_none(), "it runs");
        assert_eq!(claim.start(), None, "already running, nothing owed");
        claim.answer(&blank_git_status("p1"));
        assert_eq!(answered.try_recv().unwrap().project, "p1");
        assert_eq!(claim.release_unless_rerun_owed(), None);
        assert!(state.claim_git_check_queued("p1").is_some());
    }

    /// A re-check that finds a check running is owed a rerun, answered by that rerun rather than
    /// by the check it found, however many re-checks asked.
    #[test]
    fn a_recheck_during_a_running_check_is_answered_by_the_rerun_it_is_owed() {
        let (state, _dir) = app_state("state-git-recheck-running");
        let mut claim = state.claim_git_check_queued("p1").expect("the first claim");
        assert_eq!(claim.start(), None);
        let (none, mut first) = state.claim_git_recheck("p1");
        assert!(none.is_none());
        let (_, mut second) = state.claim_git_recheck("p1");

        claim.answer(&blank_git_status("p1"));
        assert!(first.try_recv().is_err(), "not by the check it found");
        assert_eq!(claim.release_unless_rerun_owed(), Some(OwedCheck::Recheck));
        claim.answer(&GitStatus {
            branch: Some("main".to_string()),
            ..blank_git_status("p1")
        });
        for answered in [&mut first, &mut second] {
            assert_eq!(answered.try_recv().unwrap().branch.as_deref(), Some("main"));
        }
        assert_eq!(claim.release_unless_rerun_owed(), None, "owed once");
    }

    /// A re-check that finds a sweep's check still waiting for a place takes it over, as a pull
    /// does, and is answered by it.
    #[tokio::test]
    async fn a_recheck_promotes_a_queued_check_and_is_answered_by_it() {
        let (state, _dir) = app_state("state-git-recheck-queued");
        let mut claim = state.claim_git_check_queued("p1").expect("the first claim");
        let (_, mut answered) = state.claim_git_recheck("p1");
        tokio::time::timeout(crate::test_support::PATIENCE, claim.promoted())
            .await
            .expect("the queued claim is woken");
        assert_eq!(claim.start(), Some(OwedCheck::Recheck));
        claim.answer(&blank_git_status("p1"));
        assert!(answered.try_recv().is_ok());
        assert_eq!(claim.release_unless_rerun_owed(), None);
    }

    /// A pull covers a re-check: both asked during one check are owed one rerun, a pull, which
    /// answers the re-check.
    #[test]
    fn a_pull_and_a_recheck_during_one_check_are_owed_one_pull() {
        let (state, _dir) = app_state("state-git-recheck-and-pull");
        let mut claim = state.claim_git_check_queued("p1").expect("the first claim");
        assert_eq!(claim.start(), None);
        let (_, mut answered) = state.claim_git_recheck("p1");
        assert!(state.claim_git_pull("p1").0.is_none());
        let (_, mut later) = state.claim_git_recheck("p1");
        assert_eq!(claim.release_unless_rerun_owed(), Some(OwedCheck::Pull));
        claim.answer(&blank_git_status("p1"));
        assert!(answered.try_recv().is_ok() && later.try_recv().is_ok());
        assert_eq!(claim.release_unless_rerun_owed(), None);
    }

    /// A check abandoned with re-checks waiting on it (its project removed, a panic) ends their
    /// wait without a status rather than leaving them waiting.
    #[test]
    fn a_dropped_claim_ends_the_wait_of_the_rechecks_it_owed() {
        let (state, _dir) = app_state("state-git-recheck-dropped");
        let (claim, mut own) = state.claim_git_recheck("p1");
        let (_, mut owed) = state.claim_git_recheck("p1");
        drop(claim);
        for answered in [&mut own, &mut owed] {
            assert!(matches!(
                answered.try_recv(),
                Err(tokio::sync::oneshot::error::TryRecvError::Closed)
            ));
        }
    }

    /// A pull of a project nobody is checking claims it, to run at once, and is answered by that
    /// check with what its pull did.
    #[test]
    fn a_pull_of_an_unclaimed_project_runs_it_and_is_answered_by_it() {
        let (state, _dir) = app_state("state-git-pull-unclaimed");
        let (claim, mut answered) = state.claim_git_pull("p1");
        let mut claim = claim.expect("the project was free");
        assert!(state.claim_git_check_queued("p1").is_none(), "it runs");
        assert_eq!(claim.start(), None, "already running, nothing owed");
        assert!(answered.try_recv().is_err(), "not before the check ends");
        claim.answer_pull(&Ok(PullOutcome::FastForwarded));
        assert_eq!(answered.try_recv().unwrap(), Ok(PullOutcome::FastForwarded));
        assert_eq!(claim.release_unless_rerun_owed(), None);
        assert!(state.claim_git_check_queued("p1").is_some());
    }

    /// A pull that finds a check running is owed a pull of its own, which answers it, not the check
    /// it found: the answer comes from a fetch that started after the request.
    #[test]
    fn a_pull_during_a_running_check_is_answered_by_the_rerun_it_is_owed() {
        let (state, _dir) = app_state("state-git-pull-running");
        let mut claim = state.claim_git_check_queued("p1").expect("the first claim");
        assert_eq!(claim.start(), None);
        let (none, mut first) = state.claim_git_pull("p1");
        assert!(none.is_none());
        let (_, mut second) = state.claim_git_pull("p1");

        claim.answer_pull(&Ok(PullOutcome::UpToDate));
        assert!(first.try_recv().is_err(), "not by the check it found");
        assert_eq!(claim.release_unless_rerun_owed(), Some(OwedCheck::Pull));
        claim.answer_pull(&Ok(PullOutcome::FastForwarded));
        for answered in [&mut first, &mut second] {
            assert_eq!(answered.try_recv().unwrap(), Ok(PullOutcome::FastForwarded));
        }
        assert_eq!(claim.release_unless_rerun_owed(), None, "owed once");
    }

    /// A pull that finds a sweep's check still waiting for a place takes it over, as a re-check
    /// does, and is answered by it.
    #[tokio::test]
    async fn a_pull_promotes_a_queued_check_and_is_answered_by_it() {
        let (state, _dir) = app_state("state-git-pull-queued");
        let mut claim = state.claim_git_check_queued("p1").expect("the first claim");
        let (_, mut answered) = state.claim_git_pull("p1");
        tokio::time::timeout(crate::test_support::PATIENCE, claim.promoted())
            .await
            .expect("the queued claim is woken");
        assert_eq!(claim.start(), Some(OwedCheck::Pull));
        claim.answer_pull(&Ok(PullOutcome::UpToDate));
        assert_eq!(answered.try_recv().unwrap(), Ok(PullOutcome::UpToDate));
        assert_eq!(claim.release_unless_rerun_owed(), None);
    }

    /// A refusal reaches every pull the check answers, as the outcome does, and a re-check waiting
    /// on the same check is answered with its status.
    #[test]
    fn a_check_answers_a_refusal_to_every_pull_and_a_recheck_alike() {
        let (state, _dir) = app_state("state-git-pull-refusal");
        let mut claim = state.claim_git_check_queued("p1").expect("the first claim");
        assert_eq!(claim.start(), None);
        let (_, mut pull) = state.claim_git_pull("p1");
        let (_, mut recheck) = state.claim_git_recheck("p1");
        assert_eq!(claim.release_unless_rerun_owed(), Some(OwedCheck::Pull));
        let refusal = CodedError::of(&CodedError::raised(
            error_code::NO_UPSTREAM_TO_PULL,
            "no upstream",
            &[],
        ));
        claim.answer(&blank_git_status("p1"));
        claim.answer_pull(&Err(refusal.clone()));
        assert_eq!(pull.try_recv().unwrap(), Err(refusal));
        assert_eq!(recheck.try_recv().unwrap().project, "p1");
    }

    /// A check abandoned with pulls waiting on it (its project removed, a panic) ends their wait
    /// without an answer rather than leaving them waiting.
    #[test]
    fn a_dropped_claim_ends_the_wait_of_the_pulls_it_owed() {
        let (state, _dir) = app_state("state-git-pull-dropped");
        let (claim, mut own) = state.claim_git_pull("p1");
        let (_, mut owed) = state.claim_git_pull("p1");
        drop(claim);
        for answered in [&mut own, &mut owed] {
            assert!(matches!(
                answered.try_recv(),
                Err(tokio::sync::oneshot::error::TryRecvError::Closed)
            ));
        }
    }

    /// A claim for work that starts at once is exclusive with every other claim, the queued kind
    /// included, in both directions, and is released like any other.
    #[test]
    fn a_claim_for_work_that_starts_at_once_excludes_and_is_excluded_by_every_other() {
        let (state, _dir) = app_state("state-git-claim-now-exclusive");
        let mut now = state
            .claim_git_check_now("p1")
            .expect("the project was free");
        assert!(state.claim_git_check_now("p1").is_none());
        assert!(state.claim_git_check_queued("p1").is_none());
        assert!(state.claim_git_pull("p1").0.is_none());
        assert_eq!(now.release_unless_rerun_owed(), Some(OwedCheck::Pull));
        assert_eq!(now.release_unless_rerun_owed(), None);
        assert!(state.claim_git_check_now("p1").is_some(), "released");

        let queued = state.claim_git_check_queued("p1").expect("free again");
        assert!(
            state.claim_git_check_now("p1").is_none(),
            "a queued check holds it"
        );
        drop(queued);
        let claim = state.claim_git_check_now("p1").expect("free once more");
        drop(claim);
        assert!(state.claim_git_check_queued("p1").is_some());
    }

    /// A claim made for work that starts at once is not woken by the requests that arrive during
    /// it, which have no place to take over: they are owed a check, handed back at the release.
    #[tokio::test(start_paused = true)]
    async fn a_claim_for_work_that_starts_at_once_is_never_woken_and_hands_back_what_is_owed() {
        let (state, _dir) = app_state("state-git-claim-now-not-queued");
        let mut claim = state
            .claim_git_check_now("p1")
            .expect("the project was free");
        assert_eq!(claim.start(), None, "nothing is owed yet");
        let (_, mut recheck) = state.claim_git_recheck("p1");
        let (_, mut pull) = state.claim_git_pull("p1");
        // Nothing else is pending, so the paused clock runs the wait out.
        assert!(
            tokio::time::timeout(crate::test_support::PATIENCE, claim.promoted())
                .await
                .is_err(),
            "no one waits for a place behind it"
        );
        assert!(recheck.try_recv().is_err() && pull.try_recv().is_err());
        assert_eq!(claim.release_unless_rerun_owed(), Some(OwedCheck::Pull));
        claim.answer(&blank_git_status("p1"));
        claim.answer_pull(&Ok(PullOutcome::UpToDate));
        assert_eq!(recheck.try_recv().unwrap().project, "p1");
        assert_eq!(pull.try_recv().unwrap(), Ok(PullOutcome::UpToDate));
        assert_eq!(claim.release_unless_rerun_owed(), None);
    }

    /// A queued claim is woken by a request only while it still waits for its place: once it has
    /// started, a request is owed a rerun instead and wakes no one.
    #[tokio::test(start_paused = true)]
    async fn a_queued_claim_that_has_started_is_no_longer_woken() {
        let (state, _dir) = app_state("state-git-claim-started-not-woken");
        let mut claim = state.claim_git_check_queued("p1").expect("the first claim");
        assert_eq!(claim.start(), None);
        assert!(state.claim_git_pull("p1").0.is_none());
        assert!(
            tokio::time::timeout(crate::test_support::PATIENCE, claim.promoted())
                .await
                .is_err(),
            "it runs, so it is not waiting for a place"
        );
        assert_eq!(claim.release_unless_rerun_owed(), Some(OwedCheck::Pull));
    }

    /// The claims a pull or a re-check takes for itself start at once, so they are not queued
    /// either.
    #[tokio::test(start_paused = true)]
    async fn the_claim_a_request_takes_for_itself_is_not_woken_by_the_next() {
        let (state, _dir) = app_state("state-git-claim-own-not-woken");
        let (claim, _answered) = state.claim_git_pull("p1");
        let claim = claim.expect("the project was free");
        let (_, _later) = state.claim_git_recheck("p1");
        assert!(
            tokio::time::timeout(crate::test_support::PATIENCE, claim.promoted())
                .await
                .is_err()
        );
    }

    /// A request that stopped waiting leaves the check nothing to fail on when it answers.
    #[test]
    fn answering_a_pull_nobody_waits_for_any_more_is_harmless() {
        let (state, _dir) = app_state("state-git-pull-gave-up");
        let (claim, answered) = state.claim_git_pull("p1");
        let mut claim = claim.expect("the project was free");
        drop(answered);
        claim.answer_pull(&Ok(PullOutcome::UpToDate));
        assert_eq!(claim.release_unless_rerun_owed(), None);
    }

    #[test]
    fn a_push_overlays_checks_until_it_ends() {
        let (state, dir) = app_state("state-git-push-overlap");
        git_project(&state, &dir);
        state.publish_git_status(git_reading());
        let mut window = state.subscribe();
        let push = state.begin_git_push("p1").unwrap();
        state.publish_git_status(GitStatus {
            activity: GitActivity::Checking,
            ..git_reading()
        });
        state.publish_git_reread(
            "p1",
            Ok(crate::project_git::header::BranchHeader {
                branch: Some("main".to_string()),
                upstream: Some("origin/main".to_string()),
                ahead: 0,
                behind: 1,
                ..Default::default()
            }),
        );
        let status = state.git_statuses().pop().unwrap();
        assert_eq!(status.activity, GitActivity::Pushing);
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!((status.ahead, status.behind), (0, 1));
        assert_eq!(status.error.as_deref(), Some("fetch failed"));
        drop(push);
        assert_eq!(state.git_statuses()[0].activity, GitActivity::Checking);
        for expected in [
            GitActivity::Pushing,
            GitActivity::Pushing,
            GitActivity::Pushing,
            GitActivity::Checking,
        ] {
            let Event::ProjectGitStatus { status } = window.try_recv().unwrap() else {
                panic!("git status event")
            };
            assert_eq!(status.activity, expected);
        }
        assert!(window.try_recv().is_err());
    }

    #[test]
    fn a_second_push_is_refused_until_the_first_ends() {
        let (state, dir) = app_state("state-git-push-exclusive");
        git_project(&state, &dir);
        state.publish_git_status(git_reading());
        let first = state.begin_git_push("p1").unwrap();
        let mut window = state.subscribe();
        assert!(state.begin_git_push("p1").is_none());
        // The refused request leaves the first push's marker and broadcasts alone.
        assert_eq!(state.git_statuses()[0].activity, GitActivity::Pushing);
        assert!(window.try_recv().is_err());
        drop(first);
        assert_eq!(state.git_statuses()[0].activity, GitActivity::Idle);
        drop(state.begin_git_push("p1").unwrap());
    }

    #[test]
    fn a_push_finishing_after_its_project_is_registered_again_leaves_the_newer_marker() {
        let (state, dir) = app_state("state-git-push-reregistered");
        git_project(&state, &dir);
        let old = state.begin_git_push("p1").unwrap();
        state.remove_git_status("p1");
        let newer = state.begin_git_push("p1").unwrap();
        drop(old);
        assert_eq!(state.git_statuses()[0].activity, GitActivity::Pushing);
        assert!(state.begin_git_push("p1").is_none());
        drop(newer);
        assert_eq!(state.git_statuses()[0].activity, GitActivity::Idle);
    }

    #[test]
    fn a_push_request_during_a_push_is_answered_push_in_progress() {
        let (state, _dir) = app_state("state-git-push-request-refused");
        let repo =
            crate::test_support::repo_with("state-git-push-refused-repo", &[(b"file", b"one")]);
        let project = git_project(&state, &repo);
        let first = state.begin_git_push("p1").unwrap();
        let err = crate::project_git::push::push_current_branch(&state, &project).unwrap_err();
        let coded = err.downcast_ref::<crate::protocol::CodedError>().unwrap();
        assert_eq!(coded.code, crate::protocol::error_code::PUSH_IN_PROGRESS);
        assert_eq!(state.git_statuses()[0].activity, GitActivity::Pushing);
        drop(first);
        assert_eq!(state.git_statuses()[0].activity, GitActivity::Idle);
    }

    #[test]
    fn a_push_a_switch_or_a_restore_without_git_is_refused_as_git_not_found() {
        let (state, _dir) = app_state("state-git-push-no-git");
        let repo = crate::test_support::repo_with("state-git-push-no-git-repo", &[(b"f", b"one")]);
        let project = git_project(&state, &repo);
        state.host_tools.replace(crate::host_tools::Tools {
            git: crate::test_support::git_not_found,
            fresh_git: crate::test_support::git_not_found,
            ..state.host_tools.get()
        });
        let pushed = crate::project_git::push::push_current_branch(&state, &project);
        let switched = crate::project_git::switch::switch_branch(&state, &project, "main");
        let restored = crate::project_git::restore::restore_project(&state, &project);
        for err in [
            pushed.unwrap_err(),
            switched.unwrap_err(),
            restored.unwrap_err(),
        ] {
            let (code, params, _) = crate::protocol::error_parts(&err);
            assert_eq!(code, crate::protocol::error_code::BINARY_NOT_FOUND);
            assert_eq!(params["binary"], "git");
        }
    }

    /// Any other failure to have a `git` (a login shell that timed out) is the push's or the
    /// switch's own failure, as before a missing `git` had a code of its own, and takes no fresh
    /// snapshot.
    #[test]
    fn a_push_or_a_switch_whose_git_cannot_be_had_fails_as_the_action() {
        let (state, _dir) = app_state("state-git-push-shell-failed");
        let repo =
            crate::test_support::repo_with("state-git-push-shell-failed-repo", &[(b"f", b"one")]);
        let project = git_project(&state, &repo);
        state.host_tools.replace(crate::host_tools::Tools {
            git: || anyhow::bail!("the login shell timed out"),
            fresh_git: || unreachable!("a fresh snapshot for another failure"),
            ..state.host_tools.get()
        });
        let pushed = crate::project_git::push::push_current_branch(&state, &project);
        let switched = crate::project_git::switch::switch_branch(&state, &project, "main");
        for (err, expected) in [
            (
                pushed.unwrap_err(),
                crate::protocol::error_code::GIT_PUSH_FAILED,
            ),
            (
                switched.unwrap_err(),
                crate::protocol::error_code::GIT_SWITCH_FAILED,
            ),
        ] {
            let (code, params, _) = crate::protocol::error_parts(&err);
            assert_eq!(code, expected);
            assert!(params["detail"].contains("timed out"));
        }
    }

    #[test]
    fn a_reread_records_a_missing_git_with_its_code_until_one_runs() {
        let (state, dir) = app_state("state-git-reread-no-git");
        let project = git_project(&state, &dir);
        state.publish_git_status(git_reading());
        let tools = state.host_tools.get();
        state.host_tools.replace(crate::host_tools::Tools {
            git: crate::test_support::git_not_found,
            ..tools
        });
        crate::project_git::header::publish_reread(&state, &project);
        let status = state.git_statuses().pop().unwrap();
        assert_eq!(
            status.error_code,
            Some(crate::protocol::error_code::BINARY_NOT_FOUND)
        );
        assert_eq!(status.branch.as_deref(), Some("main"));

        let header = || {
            Ok(crate::project_git::header::BranchHeader {
                branch: Some("main".to_string()),
                ..Default::default()
            })
        };
        state.publish_git_reread("p1", header());
        let status = state.git_statuses().pop().unwrap();
        assert_eq!((status.error, status.error_code), (None, None));
        // A failure of a step that ran `git` stands until that step succeeds.
        state.publish_git_status(git_reading());
        state.publish_git_reread("p1", header());
        let status = state.git_statuses().pop().unwrap();
        assert_eq!(status.error.as_deref(), Some("fetch failed"));
    }

    #[test]
    fn a_push_guard_restores_the_latest_status_even_on_unwind() {
        let (state, dir) = app_state("state-git-push-unwind");
        git_project(&state, &dir);
        state.publish_git_status(git_reading());
        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _push = state.begin_git_push("p1").unwrap();
            state.publish_git_status(GitStatus {
                activity: GitActivity::Pulling,
                ..git_reading()
            });
            panic!("simulated push panic");
        }));
        assert!(unwound.is_err());
        assert_eq!(state.git_statuses()[0].activity, GitActivity::Pulling);
    }

    #[test]
    fn push_success_and_failure_both_clear_the_in_flight_activity() {
        let (state, dir) = app_state("state-git-push-outcomes");
        let repo = crate::test_support::repo_with("state-git-push-repo", &[(b"file", b"one")]);
        let project = git_project(&state, &repo);
        let remote = dir.join("remote.git");
        crate::test_support::git(
            &repo,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        state.publish_git_status(git_reading());
        let mut events = state.subscribe();
        assert!(crate::project_git::push::push_current_branch(&state, &project).is_err());
        // The header re-read first, ahead of the push being marked in flight.
        for expected in [GitActivity::Idle, GitActivity::Pushing, GitActivity::Idle] {
            let Event::ProjectGitStatus { status } = events.try_recv().unwrap() else {
                panic!("git status event")
            };
            assert_eq!(status.activity, expected);
        }
        assert_eq!(
            state.git_statuses()[0].error.as_deref(),
            Some("fetch failed")
        );
        crate::test_support::git(&dir, &["init", "--bare", remote.to_str().unwrap()]);
        crate::project_git::push::push_current_branch(&state, &project).unwrap();
        for expected in [
            GitActivity::Idle,
            GitActivity::Pushing,
            GitActivity::Pushing,
            GitActivity::Idle,
        ] {
            let Event::ProjectGitStatus { status } = events.try_recv().unwrap() else {
                panic!("git status event")
            };
            assert_eq!(status.activity, expected);
        }
        let status = state.git_statuses().pop().unwrap();
        assert_eq!(status.upstream.as_deref(), Some("origin/main"));
        assert_eq!((status.ahead, status.behind), (0, 0));
        assert_eq!(status.error.as_deref(), Some("fetch failed"));
    }

    #[test]
    fn a_push_finishing_after_its_project_is_removed_does_not_restore_a_status() {
        let (state, dir) = app_state("state-git-push-removed");
        git_project(&state, &dir);
        let push = state.begin_git_push("p1").unwrap();
        state.remove_git_status("p1");
        let mut events = state.subscribe();
        drop(push);
        assert!(state.git_statuses().is_empty());
        assert!(events.try_recv().is_err());
        let _missing = state.begin_git_push("ghost").unwrap();
        assert!(state.git_statuses().is_empty());
    }

    /// A status published for a project no longer in the store must be dropped rather than
    /// reinserted — the race `DeleteProject` leaves open when its check is already in flight, and
    /// the gap `DeleteConsole` leaves for every project of a deleted console.
    #[test]
    fn publishing_a_status_for_a_project_no_longer_in_the_store_drops_it() {
        let (state, _dir) = app_state("state-git-status-missing-project");
        state.publish_git_status(GitStatus {
            project: "ghost".to_string(),
            repository: true,
            branch: Some("main".to_string()),
            detached: false,
            upstream: None,
            ahead: 0,
            behind: 0,
            activity: crate::protocol::GitActivity::Idle,
            error: None,
            error_code: None,
        });
        assert!(state.git_statuses().is_empty());
    }

    /// A completion recorded for a project, then dropped by the publish that follows it because
    /// the project is gone, must stay dropped — `record_git_check_completed` running first (as
    /// `crate::project_git::status::check_project` does at both of its exit points) and then losing
    /// to `remove_git_status` is the order that matters; recording it after the publish would
    /// resurrect the very entry `remove_git_status` just cleared, with nothing left to ever remove
    /// it again.
    #[test]
    fn a_completion_recorded_then_published_for_a_gone_project_leaves_no_entry() {
        let (state, _dir) = app_state("state-git-status-missing-project-completion");
        state.record_git_check_completed("ghost");
        state.publish_git_status(GitStatus {
            project: "ghost".to_string(),
            repository: false,
            branch: None,
            detached: false,
            upstream: None,
            ahead: 0,
            behind: 0,
            activity: crate::protocol::GitActivity::Idle,
            error: None,
            error_code: None,
        });
        assert!(state.git_statuses().is_empty());
        assert!(state.git_check_completed_at("ghost").is_none());
    }

    fn git_project(state: &AppState, dir: &std::path::Path) -> crate::protocol::Project {
        insert_record(state, dir, "s", SessionStatus::Idle);
        crate::test_support::insert_project(state, dir)
    }

    fn git_reading() -> GitStatus {
        GitStatus {
            project: "p1".to_string(),
            repository: true,
            branch: Some("main".to_string()),
            detached: false,
            upstream: Some("origin/main".to_string()),
            ahead: 2,
            behind: 1,
            activity: GitActivity::Idle,
            error: Some("fetch failed".to_string()),
            error_code: None,
        }
    }
}
