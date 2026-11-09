//! Checking a project's git status against its remote, on request — the daemon keeps no timer of
//! its own for that; the client drives the schedule and asks with `refresh_git_status` (see
//! `apps/daemon/PROTOCOL.md`'s "Daemon behaviour, per project"). Each project is checked at most
//! once at a time, at most `SWEEP_WIDTH` projects at once across every console and client, and at
//! most once every `GIT_CHECK_MIN_INTERVAL`. What a check does not go to the remote for, the local
//! branch header, is also re-read on its own when the project's refs change
//! (`crate::project_git::watch`) and around a push or a switch (`publish_reread`).
//!
//! `pull_project_now` is the manual entry point, driven by `pull_project_branch`: one project's
//! ordinary check, but past the floor above, ahead of the places a sweep waits for, and
//! fast-forwarding whatever the setting says. `recheck_project_now`, driven by
//! `refresh_project_git_status`, is the same check never fast-forwarding, whose outcome it waits
//! for.
//!
//! `pull_behind_projects` is a further entry point, driven by `update_settings` the
//! moment **Automatically pull repositories** is turned on: it only fast-forwards branches already
//! behind their upstream, goes nowhere near the remote, and is therefore outside the floor above —
//! sharing the in-flight claim and the places with the checks, but not their minimum interval.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::git::env::SshConfiguration;
use crate::git::layout::is_git_repo;
use crate::git::run::{Runner, WorkDir};
use crate::host_tools::Pace;
use crate::project_git::header::read_branch_header;
use crate::project_git::remote::{self, Remote};
use crate::project_git::{project_git, StepFailure};
use crate::protocol::{GitActivity, GitStatus, Project};
use crate::state::{AppState, GitCheckClaim, OwedCheck};

/// The floor between the end of one check of a project and the start of its next, on top of the
/// in-flight guard above — a guard that does nothing here, since the daemon owns no timer and each
/// client drives its own 5-minute `refresh_git_status` interval on its own phase. Several windows
/// therefore interleave their sweeps, and without this floor N clients turn one `git fetch` per
/// project every 5 minutes into up to N. The one-minute floor keeps background checks reasonably
/// fresh while preventing 5-minute pollers from multiplying the work.
const GIT_CHECK_MIN_INTERVAL: Duration = Duration::from_secs(60);

/// How long a check runs before its project's badge swaps to the in-flight icon. A check that
/// needs no network answers well inside it, so the badge goes from the last status to the new one
/// in a single event instead of two, which across a console of projects is two events each per
/// sweep.
const CHECKING_PUBLISH_DELAY: Duration = Duration::from_millis(400);

/// How many projects the sweeps work on at once, across every sweep of every client. Each check
/// starts several `git` and `ssh` processes, so a console of dozens of projects all checked at
/// once starts hundreds of them together and holds the blocking pool, which every user-visible path
/// shares, for as long as the slowest remote takes.
const SWEEP_WIDTH: usize = crate::blocking_pass::BLOCKING_WIDTH;

/// How long `recheck_project_now` waits for the check that answers it. Every step of a check is
/// bounded (a login shell by its snapshot timeout, each `git` by two minutes), so this only stops
/// a check stuck past those bounds, on a dead mount say, from holding the request for good; it is
/// long past any one step's bound, enough for a held check and its rerun unless their steps run
/// to their bounds.
const RECHECK_ANSWER_WAIT: Duration = Duration::from_secs(10 * 60);

/// The places `SWEEP_WIDTH` allows, held by a sweep's check from the moment it starts until it is
/// over.
static SWEEP_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(SWEEP_WIDTH);

/// Who asked for a check, which decides how it finds `git`, how soon it shows that it runs, and
/// whether it fast-forwards.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Asked {
    /// A sweep: the cached `git` alone, the in-flight status only once `CHECKING_PUBLISH_DELAY`
    /// has passed, and a fast-forward only where **Automatically pull repositories** is on.
    Sweep,
    /// The user's pull (`pull_project_branch`), or the rerun one is owed: a `git` that cannot be
    /// found looked for again, the in-flight status at once, and a fast-forward whatever the
    /// setting says.
    Pull,
    /// The user's re-check (`refresh_project_git_status`): as a pull, but never fast-forwarding.
    Recheck,
    /// A sweep's check, or a `pull_behind_projects` pass, that a re-check found waiting for a place
    /// and started at once in its stead: as a re-check, but fast-forwarding where the setting says,
    /// as the sweep would have.
    RecheckedSweep,
}

impl Asked {
    /// How a holder runs the check `owed` to it.
    fn owed(owed: OwedCheck) -> Self {
        match owed {
            OwedCheck::Pull => Self::Pull,
            OwedCheck::Recheck => Self::Recheck,
        }
    }

    /// How a queued check runs the check `owed` to it, started in its stead and at once.
    fn in_place_of_queued(owed: OwedCheck) -> Self {
        match owed {
            OwedCheck::Pull => Self::Pull,
            OwedCheck::Recheck => Self::RecheckedSweep,
        }
    }
}

/// Checks every one of `projects`' git status, at most `SWEEP_WIDTH` at a time, the rest waiting
/// for a place; a project already being checked, or one whose last check completed less than
/// `GIT_CHECK_MIN_INTERVAL` ago, is skipped. Fire-and-forget: each check reports itself through
/// `AppState::publish_git_status` as it progresses, and there is nothing here for a caller to wait
/// on. A waiting project holds its claim, so another sweep does not queue it twice.
pub fn refresh(state: &Arc<AppState>, projects: Vec<Project>) {
    for project in projects {
        let Some(claim) = claim_if_due(state, &project, false) else {
            continue;
        };
        tokio::spawn(run_when_placed(state.clone(), project, claim));
    }
}

/// [`refresh`] for a caller that answers with the result, such as an MCP tool: waits until each of
/// `projects` has a check that completed after the call (one already running, or one the floor
/// skipped because it completed within `GIT_CHECK_MIN_INTERVAL`, counts), or `wait` has passed.
/// Returns the ids of the projects whose check had not completed by then; those checks go on
/// running and publish their results as any check does.
pub async fn refresh_and_wait(
    state: &Arc<AppState>,
    projects: Vec<Project>,
    wait: Duration,
) -> Vec<String> {
    let asked = Instant::now();
    // A project the floor skips has a result younger than the floor already, so it is not waited
    // for; every other one is waiting for its completion to differ from what it was.
    let mut pending: Vec<(String, Option<Instant>)> = projects
        .iter()
        .map(|project| {
            (
                project.id.clone(),
                state.git_check_completed_at(&project.id),
            )
        })
        .filter(|(_, before)| due_for_check(*before, asked, GIT_CHECK_MIN_INTERVAL))
        .collect();
    refresh(state, projects);
    let deadline = asked + wait;
    loop {
        pending.retain(|(id, before)| state.git_check_completed_at(id) == *before);
        if pending.is_empty() || Instant::now() >= deadline {
            return pending.into_iter().map(|(id, _)| id).collect();
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Waits for a sweep place, or for a manual pull or a re-check to promote the claim past the wait,
/// then starts the claim. Returns the place (none when promoted), the check a request that arrived
/// meanwhile is owed, and the project read again, since the wait may have been long: a project
/// removed meanwhile must not be fetched, and one that cannot be read back is skipped for the same
/// reason as in `pull_behind_projects`. `None` when the semaphore is closed or the project is gone.
async fn wait_for_place(
    state: &AppState,
    project: &Project,
    claim: &mut GitCheckClaim,
) -> Option<(
    Option<tokio::sync::SemaphorePermit<'static>>,
    Option<OwedCheck>,
    Project,
)> {
    let slot = tokio::select! {
        slot = SWEEP_SLOTS.acquire() => Some(slot.ok()?),
        () = claim.promoted() => None,
    };
    let owed = claim.start();
    let Ok(Some(project)) = state.store.get_project(&project.id) else {
        return None;
    };
    Some((slot, owed, project))
}

/// A sweep's check, waiting for a place before it runs. A manual pull or a re-check that arrives
/// while it waits ends the wait and makes this the check it asked for, with no place of its own,
/// so the request is one fetch and does not queue behind the sweep.
async fn run_when_placed(state: Arc<AppState>, project: Project, mut claim: GitCheckClaim) {
    let Some((slot, owed, project)) = wait_for_place(&state, &project, &mut claim).await else {
        return;
    };
    // The place is held until the check is over, panicked or not: the claim, which the blocking
    // task owns, is what a panic must release, and it does so on drop.
    let asked = owed.map_or(Asked::Sweep, Asked::in_place_of_queued);
    let _ = tokio::task::spawn_blocking(move || run_checks(&state, project, claim, asked)).await;
    drop(slot);
}

/// The project's claim for a check, or `None` when it is not due (unless `force`) or one already
/// holds it. Unless `force`, the claim is a queued one, for a check that waits for a place.
fn claim_if_due(state: &Arc<AppState>, project: &Project, force: bool) -> Option<GitCheckClaim> {
    if !force
        && !due_for_check(
            state.git_check_completed_at(&project.id),
            Instant::now(),
            GIT_CHECK_MIN_INTERVAL,
        )
    {
        return None;
    }
    if force {
        state.claim_git_check_forced(&project.id)
    } else {
        state.claim_git_check_queued(&project.id)
    }
}

/// Runs the project's check on the current (blocking) thread: every step below shells out to
/// `git`, which blocks. The claim is held by this function and released by
/// `release_unless_rerun_owed` after each check, or on drop if a step panics, so a panic still
/// frees the project for the next sweep instead of excluding it forever. The first check is
/// `asked`'s. A manual pull or a re-check that finds a check running leaves it to that check,
/// which runs once more, as what was asked, before releasing the claim; found queued for a place,
/// it makes that check run at once instead. Each check answers the re-checks it was run for.
fn run_checks(state: &Arc<AppState>, project: Project, mut claim: GitCheckClaim, asked: Asked) {
    let mut project = project;
    let mut asked = asked;
    loop {
        let status = check_project(state, &project, asked);
        // A repository made, or reachable, after the daemon started is not watched yet.
        crate::project_git::watch::watch_now(state, &project);
        claim.answer(&status);
        let Some(owed) = claim.release_unless_rerun_owed() else {
            break;
        };
        // The rerun comes after an arbitrary wait: the project may have been removed meanwhile,
        // and a removed project's directory must not be fetched or merged. Dropping the claim
        // here releases it.
        match state.store.get_project(&project.id) {
            Ok(Some(fresh)) => project = fresh,
            _ => break,
        }
        asked = Asked::owed(owed);
    }
}

/// One project's check and fast-forward, started right now at the user's request: the same work
/// `refresh` does, but it ignores `GIT_CHECK_MIN_INTERVAL` and the **Automatically pull
/// repositories** setting — asking for it is the consent the setting would otherwise give. A
/// project already running its check gets one more forced check once that one ends, and one still
/// waiting for a place in a sweep, or in the settings-triggered `pull_behind_projects` pass, runs
/// forced at once in its stead, so the request results in a fast-forward attempt either way.
/// Fire-and-forget, like `refresh`.
pub fn pull_project_now(state: &Arc<AppState>, project: Project) {
    // One project at the user's request, so it does not wait for a place behind a sweep.
    let Some(claim) = claim_if_due(state, &project, true) else {
        return;
    };
    let state = state.clone();
    tokio::task::spawn_blocking(move || run_checks(&state, project, claim, Asked::Pull));
}

/// One project's check, started right now at the user's request for nothing more than its status
/// (the Git-missing mark's Try again): the work `refresh` does, but past `GIT_CHECK_MIN_INTERVAL`,
/// ahead of the places a sweep waits for, looking for a `git` that cannot be found once more, and
/// never fast-forwarding, whatever the setting says. A project whose check is already running gets
/// one such check once that one ends, and one still waiting for a place has it start at once as
/// one ([`AppState::claim_git_recheck`]), so the request is never answered by a check that took the
/// cached `git` alone. Returns the status the check that answers it ends with, `None` when it was
/// abandoned before it ended (its project removed before it ran or reran, a panic) or had not ended within
/// `RECHECK_ANSWER_WAIT` (and then goes on running).
pub async fn recheck_project_now(state: &Arc<AppState>, project: Project) -> Option<GitStatus> {
    let (claim, answered) = state.claim_git_recheck(&project.id);
    if let Some(claim) = claim {
        let state = state.clone();
        tokio::task::spawn_blocking(move || run_checks(&state, project, claim, Asked::Recheck));
    }
    tokio::time::timeout(RECHECK_ANSWER_WAIT, answered)
        .await
        .ok()?
        .ok()
}

/// Whether a project last completed at `last_completed` is due for another check at `now`, given
/// the minimum interval `floor` — a pure function over the three so the decision is unit-tested
/// without a clock or a real repository. A project never checked before (`None`) is always due.
fn due_for_check(last_completed: Option<Instant>, now: Instant, floor: Duration) -> bool {
    match last_completed {
        None => true,
        Some(last) => now.saturating_duration_since(last) >= floor,
    }
}

/// One project's check, steps 1-6 of the daemon behaviour described in `PROTOCOL.md`. Never fails
/// outright: every step that can fail records its message on `GitStatus.error` and the check
/// continues, so a transient fetch failure still leaves the branch name and last known numbers
/// showing. Whether it looks for a missing `git` again, publishes its in-flight status at once and
/// fast-forwards a pullable branch is `asked`'s ([`Asked`]). Returns the final status it published.
fn check_project(state: &Arc<AppState>, project: &Project, asked: Asked) -> GitStatus {
    let path = PathBuf::from(&project.path);

    // Checked before the first publish, not after it: publishing `repository: true` and only then
    // discovering the directory is not a repository would have every plain project blink through
    // an in-flight icon it can never actually earn, twice a sweep.
    if !is_git_repo(&path) {
        remote::record_for_non_repository(state, &project.id, &path);
        // Recorded before the publish below, not after: if the project was removed mid-check,
        // that publish takes `remove_git_status`'s path and clears this same entry — recording it
        // afterwards would resurrect a completion timestamp for a project that no longer exists,
        // with nothing left to ever remove it again.
        state.record_git_check_completed(&project.id);
        let status = GitStatus {
            project: project.id.clone(),
            repository: false,
            branch: None,
            detached: false,
            upstream: None,
            ahead: 0,
            behind: 0,
            activity: GitActivity::Idle,
            error: None,
            error_code: None,
        };
        state.publish_git_status(status.clone());
        return status;
    }

    let mut status = GitStatus {
        project: project.id.clone(),
        repository: true,
        branch: None,
        detached: false,
        upstream: None,
        ahead: 0,
        behind: 0,
        activity: GitActivity::Checking,
        error: None,
        error_code: None,
    };
    // Published once the check has been running for `CHECKING_PUBLISH_DELAY`, not at once — except
    // for the user's own request, whose click is answered by the in-flight icon straight away.
    let delay = if asked == Asked::Sweep {
        CHECKING_PUBLISH_DELAY
    } else {
        Duration::ZERO
    };
    let checking = DelayedChecking::start(state, status.clone(), delay);

    // A `git` that cannot be had fails every step alike: its error is the check's, as a failed
    // branch read's would be, and with nothing read nothing is pullable. Only the user's own
    // request looks for a missing one again; a sweep does not take a login shell per project.
    let pace = if asked == Asked::Sweep {
        Pace::Scheduled
    } else {
        Pace::Person
    };
    let git = match project_git(state, pace) {
        Ok(git) => Some(git),
        Err(err) => {
            tracing::debug!(%err, "no git to check the project with");
            StepFailure::no_git(&err).record(&mut status);
            None
        }
    };
    let mut ssh = SshConfiguration::Unknown;
    if let Some(git) = &git {
        ssh = fetch_remote_of(state, git, &project.id, &path, &mut status);
        read_branch_header(git, &path, &mut status);
    }
    // From here the published statuses are the check's own: Pulling, then the final one.
    checking.cancel();

    let pulls = match asked {
        Asked::Sweep | Asked::RecheckedSweep => auto_pull_enabled(state),
        Asked::Pull => true,
        Asked::Recheck => false,
    };
    if let Some(git) = git.filter(|_| pulls && pullable(&status)) {
        fast_forward(state, &git, &path, &mut status, &ssh);
    }

    status.activity = GitActivity::Idle;
    // Recorded before this final publish, not after, for the same reason as the non-repository
    // early return above: a project removed mid-check must not have its completion timestamp
    // reinserted right after `publish_git_status` just cleared it.
    state.record_git_check_completed(&project.id);
    state.publish_git_status(status.clone());
    status
}

/// Fetches the remote of the repository at `path` that the check fetches from, and keeps the URLs
/// it read along the way as the project's recorded ones. Whatever stops the fetch records its
/// message on `status`, as a failed fast-forward does: a remote configuration that cannot be read
/// is no repository without a remote, and the badge must not show it as a check that succeeded.
/// Returns what that read says of `core.sshCommand`, for the check's later `git` that may reach the
/// remote.
fn fetch_remote_of(
    state: &AppState,
    git: &Runner,
    project_id: &str,
    path: &Path,
    status: &mut GitStatus,
) -> SshConfiguration {
    // The remotes are read with their URLs by the same call that tells which one to fetch, so
    // keeping the recorded URL current costs the check no `git` of its own.
    let run = remote::runner(git, path);
    let inspection = remote::inspect(&run);
    remote::record_inspected(state, project_id, inspection.as_ref().ok());
    let inspection = match inspection {
        Ok(inspection) => inspection,
        Err(err) => {
            tracing::debug!(%err, "reading the remotes failed");
            StepFailure::from(err.to_string()).record(status);
            return SshConfiguration::Unknown;
        }
    };
    if inspection.remotes.is_empty() {
        return inspection.ssh;
    }
    let remote = fetch_remote(&inspection.remotes, inspection.upstream());
    let mut args = vec!["fetch", "--quiet"];
    if let Some(remote) = &remote {
        args.push(remote);
    }
    if let Err(err) = git.write_remote(WorkDir::At(path), &args, &inspection.ssh) {
        tracing::debug!(%err, "fetching the remote failed");
        StepFailure::from(err.to_string()).record(status);
    }
    inspection.ssh
}

/// The in-flight status of a check, published only if the check is still running after the delay
/// it is started with. Only the published in-flight status carries the last check's branch,
/// counts and error, so the badge swaps just its glyph while the check runs instead of blanking for
/// its duration; it is taken from the cache when it is published rather than read from the
/// repository, since one more `git` process would hold the spinner back by however long a process
/// takes to start. The working status of the check stays blank: it is what the fast-forward gate
/// reads, and a `git status` that fails must not leave cached numbers standing as that gate.
struct DelayedChecking {
    cancelled: Arc<Mutex<bool>>,
}

impl DelayedChecking {
    /// Starts the timer from a blocking thread of the runtime; without one (outside the daemon)
    /// the in-flight status is simply never published. `blank` is the in-flight status of a
    /// project with no previous one.
    fn start(state: &Arc<AppState>, blank: GitStatus, delay: Duration) -> Self {
        let cancelled = Arc::new(Mutex::new(false));
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let (state, cancelled) = (state.clone(), cancelled.clone());
            runtime.spawn(async move {
                tokio::time::sleep(delay).await;
                // Held across the publish, so a `cancel` that returns has left nothing of this
                // timer to publish after the check's own statuses.
                let cancelled = cancelled.lock().expect("checking flag lock poisoned");
                if *cancelled {
                    return;
                }
                let in_flight = match state.git_status(&blank.project).filter(|s| s.repository) {
                    Some(previous) => GitStatus {
                        activity: GitActivity::Checking,
                        ..previous
                    },
                    None => blank,
                };
                state.publish_git_status(in_flight);
            });
        }
        Self { cancelled }
    }

    /// Stops the timer from publishing, waiting out a publish already under way.
    fn cancel(&self) {
        *self.cancelled.lock().expect("checking flag lock poisoned") = true;
    }
}

/// A check that panics must not leave its timer to publish an in-flight status nothing will end.
impl Drop for DelayedChecking {
    fn drop(&mut self) {
        if let Ok(mut cancelled) = self.cancelled.lock() {
            *cancelled = true;
        }
    }
}

/// Fast-forwards every project already behind its upstream, at most `SWEEP_WIDTH` at a time — what turning
/// **Automatically pull repositories** on does right away, instead of leaving the branches to wait
/// for the client's next five-minute sweep. The statuses held in `AppState` only pick which
/// projects are worth visiting, and nothing here goes to the remote: a fetch would duplicate the
/// one the next sweep makes anyway, and the branch is already behind refs that were fetched
/// earlier. A project whose check is in flight is left to that check, which reads the setting
/// itself; a project with no status yet has never been checked, so nothing is known to
/// fast-forward it to. Fire-and-forget, like `refresh`.
///
/// Only the caller has established that the setting is on. The fast-forward writes to the
/// repository, so a caller that has not must not call it.
pub fn pull_behind_projects(state: &Arc<AppState>) {
    for status in state.git_statuses() {
        if !pullable(&status) {
            continue;
        }
        // Both an outright store error and a project that is genuinely gone mean skip: the branch
        // below is only ever touched for a project whose path was just read back from the store,
        // the opposite of `AppState::publish_git_status`'s fail-open choice, because that one risks
        // a stale badge and this one risks a merge in a directory nobody vouched for.
        let Ok(Some(project)) = state.store.get_project(&status.project) else {
            continue;
        };
        let Some(claim) = state.claim_git_check_queued(&project.id) else {
            continue;
        };
        tokio::spawn(pull_when_placed(state.clone(), project, status, claim));
    }
}

/// One `pull_behind_projects` project, waiting for a place as a sweep's check does, and for the
/// same reasons: the claim is queued, so a manual pull or a re-check that arrives meanwhile ends
/// the wait and makes this the check it asked for instead of queueing behind the pass. The setting
/// and the project are read again once the place is won, since the wait may have been long: the
/// user may have turned the setting off again, or removed the project. A manual pull or a re-check
/// that arrives while the fast-forward runs is owed its check, which runs before the claim is let
/// go.
async fn pull_when_placed(
    state: Arc<AppState>,
    project: Project,
    status: GitStatus,
    mut claim: GitCheckClaim,
) {
    let Some((slot, owed, project)) = wait_for_place(&state, &project, &mut claim).await else {
        return;
    };
    let _ = tokio::task::spawn_blocking(move || {
        if let Some(owed) = owed {
            return run_checks(&state, project, claim, Asked::in_place_of_queued(owed));
        }
        if auto_pull_enabled(&state) {
            pull_project(&state, &project, status);
        }
        if let Some(owed) = claim.release_unless_rerun_owed() {
            if let Ok(Some(fresh)) = state.store.get_project(&project.id) {
                run_checks(&state, fresh, claim, Asked::owed(owed));
            }
        }
    })
    .await;
    drop(slot);
}

/// One project's immediate fast-forward. The cached `status` picked the project out, but the gate
/// is read again from the repository before anything moves, because an arbitrary amount of time
/// may have passed since the check that filled it: the user may have switched branch, detached
/// `HEAD`, committed, or taken the directory out of git entirely, and `git merge --ff-only` acts on
/// whatever `HEAD` is now rather than on the branch the cached status described. The re-read is a
/// local `git status`, no network, so it costs nothing the next sweep would not pay anyway.
fn pull_project(state: &AppState, project: &Project, mut status: GitStatus) {
    let path = PathBuf::from(&project.path);
    if !is_git_repo(&path) {
        // The directory has left git since its last check. Nothing is published: correcting the
        // cached `repository: true` belongs to a real check, which is what reports that in the
        // first place, and the next sweep does it.
        return;
    }
    // Blanked before the re-read so that a `git status` which fails cannot leave the cached numbers
    // standing as the gate: `read_branch_header` records the failure and writes nothing else, and
    // this gate guards a merge — a locked index or a broken `.git` must fail closed.
    status.upstream = None;
    status.ahead = 0;
    status.behind = 0;
    // A `git` that cannot be had is a re-read that failed, and the gate stays shut the same way.
    let Ok(git) = project_git(state, Pace::Scheduled) else {
        return;
    };
    read_branch_header(&git, &path, &mut status);
    if !pullable(&status) {
        // Nothing to report, whether the branch genuinely moved on since its last check or the
        // re-read above failed and left the gate blank: either way that check's own numbers stay
        // on the badge until the next sweep replaces them.
        return;
    }
    // Any `error` the cached status carries is left on it rather than cleared: it is the last
    // check against the remote that failed, which is exactly what the badge's failure mark stands for
    // (see "The branch badge" in `docs/product/project-git-status.md`), and a local fast-forward
    // says nothing about whether that fetch would succeed now.
    let ssh = git.git().ssh_configuration(&path);
    fast_forward(state, &git, &path, &mut status, &ssh);
    status.activity = GitActivity::Idle;
    // No `record_git_check_completed` here, unlike `check_project`: nothing above went to the
    // remote, so the next sweep's own check must not be held off on this project's account.
    state.publish_git_status(status);
}

/// Whether the **Automatically pull repositories** setting is on; a store that cannot be read
/// counts as off, the same as the default. The fast-forward writes to the repository, so it
/// never happens on a guess.
fn auto_pull_enabled(state: &AppState) -> bool {
    state
        .store
        .get_settings()
        .map(|settings| settings.auto_pull_repositories)
        .unwrap_or(false)
}

/// Whether `status` describes a branch the pull may fast-forward: one behind its upstream with no
/// commits of its own (see "Automatically pulling repositories" in
/// `docs/product/project-git-status.md`). Pure over the status so both callers decide alike and
/// the rule is unit-tested without a repository.
fn pullable(status: &GitStatus) -> bool {
    status.upstream.is_some() && status.behind > 0 && status.ahead == 0
}

/// Fast-forwards the branch onto its upstream and reads the branch header again, reporting the
/// fast-forward in flight on `status` while it runs. `status` must have passed `pullable`. A
/// fast-forward `git` refuses records its message, as a failed fetch does. `ssh` is what the
/// repository's configuration says of `core.sshCommand`: in a partial clone the merge may fetch the
/// objects it needs from the promisor remote, so it runs the user's ssh as a fetch does.
fn fast_forward(
    state: &AppState,
    git: &Runner,
    path: &Path,
    status: &mut GitStatus,
    ssh: &SshConfiguration,
) {
    status.activity = GitActivity::Pulling;
    state.publish_git_status(status.clone());
    // `@{upstream}` rather than the upstream's short name, which a local branch of the same name
    // (`origin/main`) would shadow and fast-forward the branch onto instead.
    if let Err(err) = git.write_remote(
        WorkDir::At(path),
        ["merge", "--ff-only", "@{upstream}"],
        ssh,
    ) {
        tracing::debug!(%err, "fast-forwarding the branch failed");
        StepFailure::from(err.to_string()).record(status);
    }
    read_branch_header(git, path, status);
}

/// Which remote name, if any, to pass to `git fetch` — a pure function over the repository's
/// remotes and its upstream's remote (when it has one), so the derivation is unit-tested with
/// literal strings. `None` means a bare `git fetch`, which lets `git` fall back to the current
/// branch's upstream remote or `origin` on its own; that fallback is trusted only when there is
/// exactly one remote to begin with — a bare fetch on a repository with several and no upstream can
/// fail against a remote named something other than `origin` (a fork workflow's `upstream`) even
/// though fetching is in no way actually broken.
fn fetch_remote(remotes: &[Remote], upstream_remote: Option<&str>) -> Option<String> {
    if let Some(remote) = upstream_remote {
        return Some(remote.to_string());
    }
    if let [only] = remotes {
        return Some(only.name.clone());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_tools::Tools;
    use crate::protocol::error_code;
    use crate::test_support::{app_state, git, git_not_found, repo_with};

    /// `repo` as project `p1` of a console on `state`'s store, whose workdir is under `dir`.
    fn checked_project(state: &AppState, dir: &Path, repo: &Path) -> Project {
        state
            .store
            .insert_console(&crate::test_support::console(dir))
            .unwrap();
        crate::test_support::insert_project(state, repo)
    }

    /// A sweep's check on a machine without `git` records the failure with its code, for the badge
    /// to word, and takes no fresh snapshot to look again.
    #[test]
    fn a_sweeps_check_without_git_records_its_code_and_looks_no_further() {
        let (state, dir) = app_state("git-status-no-git-sweep");
        let repo = repo_with("git-status-no-git-sweep-repo", &[(b"file", b"one")]);
        let project = checked_project(&state, &dir, &repo);
        state.host_tools.replace(Tools {
            git: git_not_found,
            fresh_git: || unreachable!("a sweep took a fresh snapshot"),
            ..state.host_tools.get()
        });
        check_project(&state, &project, Asked::Sweep);
        let status = state.git_status("p1").expect("the check published");
        assert_eq!(status.error_code, Some(error_code::BINARY_NOT_FOUND));
        assert!(status
            .error
            .expect("the failure is recorded")
            .contains("not found"));
    }

    /// The user's own pull looks again, so a `git` installed since the cached snapshot checks the
    /// project.
    #[test]
    fn a_manual_pull_finds_a_git_installed_since_the_cached_snapshot() {
        let (state, dir) = app_state("git-status-no-git-pull");
        let repo = repo_with("git-status-no-git-pull-repo", &[(b"file", b"one")]);
        let project = checked_project(&state, &dir, &repo);
        state.host_tools.replace(Tools {
            git: git_not_found,
            ..state.host_tools.get()
        });
        check_project(&state, &project, Asked::Pull);
        let status = state.git_status("p1").expect("the check published");
        assert_eq!((status.error, status.error_code), (None, None));
        assert!(status.branch.is_some());
    }

    /// The badge's Check again looks again, as a pull does, but never fast-forwards, even with
    /// **Automatically pull repositories** on: a branch behind its upstream stays where it is.
    #[test]
    fn a_recheck_finds_a_git_installed_since_and_never_fast_forwards() {
        let (state, dir) = app_state("git-status-recheck");
        let repo = repo_with("git-status-recheck-repo", &[(b"file", b"one")]);
        let project = checked_project(&state, &dir, &repo);
        let head = || String::from_utf8(git(&repo, &["rev-parse", "HEAD"])).unwrap();
        let branch = String::from_utf8(git(&repo, &["symbolic-ref", "--short", "HEAD"])).unwrap();
        let branch = branch.trim();
        git(&repo, &["commit", "-q", "--allow-empty", "-m", "upstream"]);
        let upstream = head();
        git(&repo, &["reset", "-q", "--hard", "HEAD~"]);
        let before = head();
        git(
            &repo,
            &["update-ref", "refs/remotes/origin/main", upstream.trim()],
        );
        for (key, value) in [
            ("remote.origin.url", "/nonexistent"),
            ("remote.origin.fetch", "+refs/heads/*:refs/remotes/origin/*"),
            (&format!("branch.{branch}.remote"), "origin"),
            (&format!("branch.{branch}.merge"), "refs/heads/main"),
        ] {
            git(&repo, &["config", key, value]);
        }
        state.store.set_auto_pull_repositories(true).unwrap();
        state.host_tools.replace(Tools {
            git: git_not_found,
            ..state.host_tools.get()
        });
        check_project(&state, &project, Asked::Recheck);
        let status = state.git_status("p1").expect("the check published");
        assert_eq!(status.error_code, None);
        assert_eq!((status.behind, head()), (1, before));
        check_project(&state, &project, Asked::Pull);
        assert_eq!(head(), upstream);
    }

    /// A re-check of a project nobody is checking runs its check and answers with the status it
    /// ends with: a `git` neither snapshot finds is the answer's failure.
    #[tokio::test]
    async fn a_recheck_answers_with_the_status_its_check_ends_with() {
        let (state, dir) = app_state("git-status-recheck-answer");
        let repo = repo_with("git-status-recheck-answer-repo", &[(b"file", b"one")]);
        let project = checked_project(&state, &dir, &repo);
        state.host_tools.replace(Tools {
            git: git_not_found,
            fresh_git: git_not_found,
            ..state.host_tools.get()
        });
        let answer = recheck_project_now(&state, project)
            .await
            .expect("the check answered");
        assert_eq!(answer.activity, GitActivity::Idle);
        assert_eq!(answer.error_code, Some(error_code::BINARY_NOT_FOUND));
        let held = state.git_status("p1").expect("published");
        assert_eq!(
            serde_json::to_value(held).unwrap(),
            serde_json::to_value(answer).unwrap(),
            "as published"
        );
    }

    /// A re-check asked for while a sweep's check of the project is running is not answered by
    /// that check, which takes the cached `git` alone, but by the re-check it is owed once that
    /// check ends, which looks again and finds the `git` installed since.
    #[tokio::test]
    async fn a_recheck_during_a_sweeps_check_is_answered_by_its_own_look_once_that_check_ends() {
        let (state, dir) = app_state("git-status-recheck-running");
        let repo = repo_with("git-status-recheck-running-repo", &[(b"file", b"one")]);
        let project = checked_project(&state, &dir, &repo);
        state.host_tools.replace(Tools {
            git: git_not_found,
            ..state.host_tools.get()
        });
        let mut claim = state
            .claim_git_check_queued("p1")
            .expect("the sweep's claim");
        assert_eq!(claim.start(), None, "the sweep's check runs");

        let recheck = recheck_project_now(&state, project.clone());
        tokio::pin!(recheck);
        // One poll registers the request: a zero timeout polls the future once before giving up.
        assert!(tokio::time::timeout(Duration::ZERO, &mut recheck)
            .await
            .is_err());
        let sweeping = state.clone();
        tokio::task::spawn_blocking(move || run_checks(&sweeping, project, claim, Asked::Sweep))
            .await
            .unwrap();
        let answer = tokio::time::timeout(crate::test_support::PATIENCE, recheck)
            .await
            .expect("answered once the rerun ended")
            .expect("the check answered");
        assert_eq!((answer.error, answer.error_code), (None, None));
        assert!(answer.branch.is_some());
        assert!(
            state.claim_git_check_queued("p1").is_some(),
            "the claim is released after the rerun"
        );
    }

    #[test]
    fn only_a_branch_behind_its_upstream_with_no_commits_of_its_own_is_pullable() {
        let status = |upstream: Option<&str>, ahead: u32, behind: u32| GitStatus {
            project: "project-1".to_string(),
            repository: true,
            branch: Some("main".to_string()),
            detached: false,
            upstream: upstream.map(str::to_string),
            ahead,
            behind,
            activity: GitActivity::Idle,
            error: None,
            error_code: None,
        };
        assert!(pullable(&status(Some("origin/main"), 0, 2)));
        // Up to date, ahead as well as behind, and no upstream at all: each left alone.
        assert!(!pullable(&status(Some("origin/main"), 0, 0)));
        assert!(!pullable(&status(Some("origin/main"), 1, 2)));
        assert!(!pullable(&status(None, 0, 2)));
    }

    fn remotes(names: &[&str]) -> Vec<Remote> {
        names
            .iter()
            .map(|name| Remote {
                name: name.to_string(),
                url: format!("u-{name}"),
            })
            .collect()
    }

    #[test]
    fn fetch_remote_prefers_the_upstreams_remote() {
        let remotes = remotes(&["origin", "upstream"]);
        assert_eq!(
            fetch_remote(&remotes, Some("upstream")),
            Some("upstream".to_string())
        );
    }

    #[test]
    fn fetch_remote_falls_back_to_the_lone_remote_with_no_upstream() {
        let remotes = remotes(&["upstream"]);
        assert_eq!(fetch_remote(&remotes, None), Some("upstream".to_string()));
    }

    #[test]
    fn fetch_remote_is_bare_with_several_remotes_and_no_upstream() {
        let remotes = remotes(&["origin", "upstream"]);
        assert_eq!(fetch_remote(&remotes, None), None);
    }

    #[test]
    fn a_project_never_checked_before_is_always_due() {
        assert!(due_for_check(None, Instant::now(), GIT_CHECK_MIN_INTERVAL));
    }

    #[test]
    fn a_project_checked_within_the_floor_is_not_due() {
        let now = Instant::now();
        let last_completed = now - Duration::from_secs(1);
        assert!(!due_for_check(
            Some(last_completed),
            now,
            GIT_CHECK_MIN_INTERVAL
        ));
    }

    #[test]
    fn a_project_checked_exactly_at_the_floor_is_due() {
        let now = Instant::now();
        let last_completed = now - GIT_CHECK_MIN_INTERVAL;
        assert!(due_for_check(
            Some(last_completed),
            now,
            GIT_CHECK_MIN_INTERVAL
        ));
    }

    /// A local branch named like the upstream (`origin/main`) is what a bare `origin/main` names
    /// first; the pull must still fast-forward onto the remote-tracking branch.
    #[test]
    fn the_pull_fast_forwards_onto_the_upstream_not_a_local_branch_of_its_name() {
        let (state, _dir) = app_state("git-status-pull-shadowed");
        let repo = repo_with("git-status-pull-shadowed-repo", &[(b"a.txt", b"one\n")]);
        let rev = |name: &str| String::from_utf8(git(&repo, &["rev-parse", name])).unwrap();
        let branch = String::from_utf8(git(&repo, &["symbolic-ref", "--short", "HEAD"])).unwrap();
        let branch = branch.trim();
        let commit_on = |message: &str| {
            git(&repo, &["commit", "-q", "--allow-empty", "-m", message]);
            let commit = rev("HEAD");
            git(&repo, &["reset", "-q", "--hard", "HEAD~"]);
            commit
        };
        let upstream = commit_on("upstream");
        let shadow = commit_on("shadow");
        git(
            &repo,
            &["update-ref", "refs/remotes/origin/main", upstream.trim()],
        );
        git(
            &repo,
            &["update-ref", "refs/heads/origin/main", shadow.trim()],
        );
        for (key, value) in [
            ("remote.origin.url", "/nonexistent"),
            ("remote.origin.fetch", "+refs/heads/*:refs/remotes/origin/*"),
            (&format!("branch.{branch}.remote"), "origin"),
            (&format!("branch.{branch}.merge"), "refs/heads/main"),
        ] {
            git(&repo, &["config", key, value]);
        }

        let mut status = GitStatus {
            project: "project-1".to_string(),
            repository: true,
            branch: Some(branch.to_string()),
            detached: false,
            upstream: Some("origin/main".to_string()),
            ahead: 0,
            behind: 1,
            activity: GitActivity::Idle,
            error: None,
            error_code: None,
        };
        let git = project_git(&state, Pace::Scheduled).unwrap();
        fast_forward(&state, &git, &repo, &mut status, &SshConfiguration::Unset);
        assert_eq!(status.error, None);
        assert_eq!(rev("HEAD"), upstream);
    }

    /// A merge in a partial clone can fetch from the promisor remote, so it gets the ssh a fetch
    /// gets. The `post-merge` hook of the fast-forward sees what the merge's environment holds.
    #[test]
    fn the_pull_runs_the_ssh_the_repository_is_configured_with() {
        let (state, _dir) = app_state("git-status-pull-ssh");
        let repo = repo_with("git-status-pull-ssh-repo", &[(b"a.txt", b"one\n")]);
        let branch = String::from_utf8(git(&repo, &["symbolic-ref", "--short", "HEAD"])).unwrap();
        let branch = branch.trim();
        git(&repo, &["commit", "-q", "--allow-empty", "-m", "upstream"]);
        let upstream = String::from_utf8(git(&repo, &["rev-parse", "HEAD"])).unwrap();
        git(&repo, &["reset", "-q", "--hard", "HEAD~"]);
        git(
            &repo,
            &["update-ref", "refs/remotes/origin/main", upstream.trim()],
        );
        for (key, value) in [
            ("remote.origin.url", "/nonexistent"),
            ("remote.origin.fetch", "+refs/heads/*:refs/remotes/origin/*"),
            (&format!("branch.{branch}.remote"), "origin"),
            (&format!("branch.{branch}.merge"), "refs/heads/main"),
            ("core.hooksPath", ".git/hooks"),
        ] {
            git(&repo, &["config", key, value]);
        }
        let seen = repo.join("seen-ssh");
        let hook = repo.join(".git/hooks/post-merge");
        std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
        std::fs::write(
            &hook,
            format!(
                "#!/bin/sh\nprintf '%s' \"$GIT_SSH_COMMAND\" > '{}'\n",
                seen.display()
            ),
        )
        .unwrap();
        crate::test_support::make_executable(&hook);

        let mut status = GitStatus {
            project: "project-1".to_string(),
            repository: true,
            branch: Some(branch.to_string()),
            detached: false,
            upstream: Some("origin/main".to_string()),
            ahead: 0,
            behind: 1,
            activity: GitActivity::Idle,
            error: None,
            error_code: None,
        };
        let git = project_git(&state, Pace::Scheduled).unwrap();
        let configured = SshConfiguration::Command("ssh -i /key".to_string());
        fast_forward(&state, &git, &repo, &mut status, &configured);
        assert_eq!(status.error, None);
        let seen = std::fs::read_to_string(seen).expect("the hook ran");
        assert_eq!(seen, "ssh -oBatchMode=yes -i /key");
    }

    /// A remote configuration that cannot be read stops the fetch and is recorded on the status as
    /// `git`'s own message, not shown as a check that went through.
    #[test]
    fn unreadable_remotes_are_recorded_as_the_checks_error() {
        let (state, dir) = app_state("git-status-remotes-unreadable");
        let git = project_git(&state, Pace::Scheduled).unwrap();
        let mut status = GitStatus {
            project: "project-1".to_string(),
            repository: true,
            branch: None,
            detached: false,
            upstream: None,
            ahead: 0,
            behind: 0,
            activity: GitActivity::Checking,
            error: None,
            error_code: None,
        };
        let plain = dir.join("plain");
        std::fs::create_dir(&plain).unwrap();
        fetch_remote_of(&state, &git, "project-1", &plain, &mut status);
        let error = status.error.expect("the failure is recorded");
        assert!(error.contains("not a git repository"), "{error}");
    }
}
