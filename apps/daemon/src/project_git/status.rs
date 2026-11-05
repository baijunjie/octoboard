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
//! fast-forwarding whatever the setting says.
//!
//! `pull_behind_projects` is the module's third entry point, driven by `update_settings` the
//! moment **Automatically pull repositories** is turned on: it only fast-forwards branches already
//! behind their upstream, goes nowhere near the remote, and is therefore outside the floor above —
//! sharing the in-flight claim and the places with the checks, but not their minimum interval.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::git::env::SshConfiguration;
use crate::git::layout::is_git_repo;
use crate::git::run::{Runner, WorkDir};
use crate::project_git::header::read_branch_header;
use crate::project_git::project_git;
use crate::project_git::remote::{self, Remote};
use crate::protocol::{GitActivity, GitStatus, Project};
use crate::state::{AppState, GitCheckClaim};

/// The floor between the end of one check of a project and the start of its next, on top of the
/// in-flight guard above — a guard that does nothing here, since the daemon owns no timer and each
/// client drives its own 5-minute `refresh_git_status` interval on its own phase. Several windows
/// open on the same console therefore do not overlap their sweeps, they interleave them, and
/// without this floor N clients turn one `git fetch` per project every 5 minutes into up to N. 60
/// seconds is short enough that the check a console switch asks for still reflects the remote from
/// within the last minute even when another client's sweep just finished the same project, so the
/// switch-triggered check `PROTOCOL.md` promises is not defeated, and long enough that no number of
/// 5-minute pollers can multiply the work.
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

/// The places `SWEEP_WIDTH` allows, held by a sweep's check from the moment it starts until it is
/// over.
static SWEEP_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(SWEEP_WIDTH);

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

/// Waits for a sweep place, or for a manual pull to promote the claim past the wait, then starts
/// the claim. Returns the place (none when promoted), whether the check is forced, and the project
/// read again, since the wait may have been long: a project removed meanwhile must not be
/// fetched, and one that cannot be read back is skipped for the same reason as in
/// `pull_behind_projects`. `None` when the semaphore is closed or the project is gone.
async fn wait_for_place(
    state: &AppState,
    project: &Project,
    claim: &mut GitCheckClaim,
) -> Option<(Option<tokio::sync::SemaphorePermit<'static>>, bool, Project)> {
    let slot = tokio::select! {
        slot = SWEEP_SLOTS.acquire() => Some(slot.ok()?),
        () = claim.promoted() => None,
    };
    let force = claim.start();
    let Ok(Some(project)) = state.store.get_project(&project.id) else {
        return None;
    };
    Some((slot, force, project))
}

/// A sweep's check, waiting for a place before it runs. A manual pull that arrives while it waits
/// ends the wait and makes this the forced check, with no place of its own, so the pull is one
/// fetch and does not queue behind the sweep.
async fn run_when_placed(state: Arc<AppState>, project: Project, mut claim: GitCheckClaim) {
    let Some((slot, force, project)) = wait_for_place(&state, &project, &mut claim).await else {
        return;
    };
    // The place is held until the check is over, panicked or not: the claim, which the blocking
    // task owns, is what a panic must release, and it does so on drop.
    let _ = tokio::task::spawn_blocking(move || run_checks(&state, project, claim, force)).await;
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
/// frees the project for the next sweep instead of excluding it forever. The fast-forward follows
/// the setting unless `force`, the manual pull, which ignores it. A forced request that finds a
/// check running leaves it to that check, which runs once more, forced, before releasing the claim;
/// found queued for a place, it makes that check run forced at once instead.
fn run_checks(state: &Arc<AppState>, project: Project, mut claim: GitCheckClaim, force: bool) {
    let mut project = project;
    let mut force = force;
    loop {
        check_project(state, &project, force);
        // A repository made, or reachable, after the daemon started is not watched yet.
        crate::project_git::watch::watch_now(state, &project);
        if !claim.release_unless_rerun_owed() {
            break;
        }
        // The rerun comes after an arbitrary wait: the project may have been removed meanwhile,
        // and a removed project's directory must not be fetched or merged. Dropping the claim
        // here releases it.
        match state.store.get_project(&project.id) {
            Ok(Some(fresh)) => project = fresh,
            _ => break,
        }
        force = true;
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
    tokio::task::spawn_blocking(move || run_checks(&state, project, claim, true));
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
/// showing. `force_pull` fast-forwards a pullable branch whether or not the setting is on.
fn check_project(state: &Arc<AppState>, project: &Project, force_pull: bool) {
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
        state.publish_git_status(GitStatus {
            project: project.id.clone(),
            repository: false,
            branch: None,
            detached: false,
            upstream: None,
            ahead: 0,
            behind: 0,
            activity: GitActivity::Idle,
            error: None,
        });
        return;
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
    };
    // Published once the check has been running for `CHECKING_PUBLISH_DELAY`, not at once — except
    // for the user's own request, whose click is answered by the in-flight icon straight away.
    let delay = if force_pull {
        Duration::ZERO
    } else {
        CHECKING_PUBLISH_DELAY
    };
    let checking = DelayedChecking::start(state, status.clone(), delay);

    // A `git` that cannot be had fails every step alike: its error is the check's, as a failed
    // branch read's would be, and with nothing read nothing is pullable.
    let git = match project_git(state) {
        Ok(git) => Some(git),
        Err(err) => {
            tracing::debug!(%err, "no git to check the project with");
            status.error = Some(err);
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

    if let Some(git) = git.filter(|_| (force_pull || auto_pull_enabled(state)) && pullable(&status))
    {
        fast_forward(state, &git, &path, &mut status, &ssh);
    }

    status.activity = GitActivity::Idle;
    // Recorded before this final publish, not after, for the same reason as the non-repository
    // early return above: a project removed mid-check must not have its completion timestamp
    // reinserted right after `publish_git_status` just cleared it.
    state.record_git_check_completed(&project.id);
    state.publish_git_status(status);
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
            status.error = Some(err.to_string());
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
        status.error = Some(err.to_string());
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
/// same reasons: the claim is queued, so a manual pull that arrives meanwhile ends the wait and
/// makes this its forced check instead of queueing behind the pass. The setting and the project
/// are read again once the place is won, since the wait may have been long: the user may have
/// turned the setting off again, or removed the project. A manual pull that arrives while the
/// fast-forward runs is owed a forced check, which runs before the claim is let go.
async fn pull_when_placed(
    state: Arc<AppState>,
    project: Project,
    status: GitStatus,
    mut claim: GitCheckClaim,
) {
    let Some((slot, force, project)) = wait_for_place(&state, &project, &mut claim).await else {
        return;
    };
    let _ = tokio::task::spawn_blocking(move || {
        if force {
            return run_checks(&state, project, claim, true);
        }
        if auto_pull_enabled(&state) {
            pull_project(&state, &project, status);
        }
        if claim.release_unless_rerun_owed() {
            if let Ok(Some(fresh)) = state.store.get_project(&project.id) {
                run_checks(&state, fresh, claim, true);
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
    let Ok(git) = project_git(state) else {
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
        status.error = Some(err.to_string());
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
    use crate::test_support::{app_state, git, repo_with};

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
        };
        let git = project_git(&state).unwrap();
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
        };
        let git = project_git(&state).unwrap();
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
        let git = project_git(&state).unwrap();
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
        };
        let plain = dir.join("plain");
        std::fs::create_dir(&plain).unwrap();
        fetch_remote_of(&state, &git, "project-1", &plain, &mut status);
        let error = status.error.expect("the failure is recorded");
        assert!(error.contains("not a git repository"), "{error}");
    }
}
