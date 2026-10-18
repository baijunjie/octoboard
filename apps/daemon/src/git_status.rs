//! Checking a project's git status against its remote, on request — the daemon keeps no timer of
//! its own for that; the client drives the schedule and asks with `refresh_git_status` (see
//! `apps/daemon/PROTOCOL.md`'s "Daemon behaviour, per project"). Each project is checked at most
//! once at a time, at most `SWEEP_WIDTH` projects at once across every console and client, and at
//! most once every `GIT_CHECK_MIN_INTERVAL`. What a check does not go to the remote for, the local
//! branch header, is also re-read on its own when the project's refs change (`crate::git_watch`)
//! and around a push or a switch (`publish_reread`).
//!
//! `pull_project_now` is the manual entry point, driven by `pull_project_branch`: one project's
//! ordinary check, but past the floor above, ahead of the places a sweep waits for, and
//! fast-forwarding whatever the setting says.
//!
//! `pull_behind_projects` is the module's third entry point, driven by `update_settings` the
//! moment **Automatically pull repositories** is turned on: it only fast-forwards branches already
//! behind their upstream, goes nowhere near the remote, and is therefore outside the floor above —
//! sharing the in-flight claim and the places with the checks, but not their minimum interval.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Context;

use crate::env_shell;
use crate::git_remote::{self, Remote};
use crate::hostfs;
use crate::protocol::{GitActivity, GitStatus, Project};
use crate::state::{AppState, GitCheckClaim};

/// Upper bound on one `git` subprocess (`fetch`, `status`, `merge`). `GIT_TERMINAL_PROMPT=0` and
/// `ssh -oBatchMode=yes` (see `crate::git_env`) turn a missing credential or an unknown host key
/// into an immediate failure rather than a prompt neither of them ever shows on a headless daemon,
/// but a slow network can still legitimately take tens of seconds on a fetch; two minutes leaves a
/// wide margin above that while still turning a wedged step into a reported error within one sweep
/// instead of leaking a blocking-pool thread, and leaving its project excluded from every later
/// sweep, for the rest of the daemon's life.
const GIT_COMMAND_TIMEOUT: Duration = Duration::from_secs(120);

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
const SWEEP_WIDTH: usize = 4;

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
        crate::git_watch::watch_now(state, &project);
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
    if !hostfs::is_git_repo(&path) {
        git_remote::record_for_non_repository(state, &project.id, &path);
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

    // The remotes are read with their URLs by the same call that tells which one to fetch, so
    // keeping the recorded URL current costs the check no `git` of its own.
    let run = git_remote::runner(&path);
    let inspection = git_remote::inspect(&run);
    git_remote::record_inspected(state, &project.id, inspection.as_ref());
    let remotes = inspection
        .as_ref()
        .map_or(&[][..], |i| i.remotes.as_slice());
    if !remotes.is_empty() {
        let remote = fetch_remote(remotes, inspection.as_ref().and_then(|i| i.upstream()));
        let mut args = vec!["fetch", "--quiet"];
        if let Some(remote) = &remote {
            args.push(remote);
        }
        if let Err(err) = run_git_write(&path, &args) {
            tracing::debug!(%err, "fetching the remote failed");
            status.error = Some(err);
        }
    }
    read_branch_header(&path, &mut status);
    // From here the published statuses are the check's own: Pulling, then the final one.
    checking.cancel();

    if (force_pull || auto_pull_enabled(state)) && pullable(&status) {
        fast_forward(state, &path, &mut status);
    }

    status.activity = GitActivity::Idle;
    // Recorded before this final publish, not after, for the same reason as the non-repository
    // early return above: a project removed mid-check must not have its completion timestamp
    // reinserted right after `publish_git_status` just cleared it.
    state.record_git_check_completed(&project.id);
    state.publish_git_status(status);
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
    if !hostfs::is_git_repo(&path) {
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
    read_branch_header(&path, &mut status);
    if !pullable(&status) {
        // Nothing to report, whether the branch genuinely moved on since its last check or the
        // re-read above failed and left the gate blank: either way that check's own numbers stay
        // on the badge until the next sweep replaces them.
        return;
    }
    // Any `error` the cached status carries is left on it rather than cleared: it is the last
    // check against the remote that failed, which is exactly what the warning triangle stands for
    // (see "The branch badge" in `docs/product/project-git-status.md`), and a local fast-forward
    // says nothing about whether that fetch would succeed now.
    fast_forward(state, &path, &mut status);
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
/// fast-forward `git` refuses records its message, as a failed fetch does.
fn fast_forward(state: &AppState, path: &Path, status: &mut GitStatus) {
    status.activity = GitActivity::Pulling;
    state.publish_git_status(status.clone());
    // `pullable` just checked `upstream` to be `Some`.
    let upstream = status.upstream.clone().expect("upstream is set");
    if let Err(err) = run_git_write(path, &["merge", "--ff-only", &upstream]) {
        tracing::debug!(%err, "fast-forwarding the branch failed");
        status.error = Some(err);
    }
    read_branch_header(path, status);
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

/// The read of the local branch state: `git status`'s branch header, in the porcelain format whose
/// `# branch.*` lines [`parse_branch_header`] takes. One constant rather than a literal at its one
/// call site, so the test that proves this read leaves the repository's index alone runs the very
/// command the check runs.
pub(crate) const BRANCH_HEADER_ARGS: &[&str] = &[
    "status",
    "--porcelain=v2",
    "--branch",
    "--untracked-files=no",
    "--ignore-submodules=all",
];

/// Runs the read [`BRANCH_HEADER_ARGS`] names and applies what it says to `status`; a failure to
/// run it records `status.error` and leaves every other field of `status` exactly as it was. A
/// caller that reads a `GitStatus` it did not just build must therefore blank whatever it is about
/// to rely on before calling this, as `pull_project` does for the fields its gate reads; on a
/// brand-new `GitStatus` the fields left behind are already the empty reading — `None` branch, no
/// upstream, zero ahead and behind.
/// `--untracked-files=no` skips enumerating the worktree's files, which `parse_branch_header`
/// below never reads anyway (it only takes the `# branch.*` header lines, which this flag does not
/// change) — on a large repository that enumeration is otherwise the dominant cost of the whole
/// check, repeated every project every sweep. Keep the flag even once a feature wants a dirty
/// worktree indicator; that reads `git status`'s file-status entries, which call for their own,
/// separate invocation rather than reviving this one's enumeration for every sweep that does not
/// need it. `--ignore-submodules=all` likewise stops `git` from running a status inside every
/// checked-out submodule, which the header does not depend on either.
fn read_branch_header(path: &Path, status: &mut GitStatus) {
    match read_header(path) {
        Ok(header) => apply_header(status, header),
        Err(err) => {
            tracing::debug!(%err, "reading the local branch state failed");
            status.error = Some(err);
        }
    }
}

/// Reads the local branch header with `git status`, without touching the remote.
pub(crate) fn read_header(path: &Path) -> Result<BranchHeader, String> {
    run_git_read(path, BRANCH_HEADER_ARGS).map(|output| parse_branch_header(&output))
}

pub(crate) fn apply_header(status: &mut GitStatus, header: BranchHeader) {
    status.branch = header.branch;
    status.detached = header.detached;
    status.upstream = header.upstream;
    status.ahead = header.ahead;
    status.behind = header.behind;
}

/// Whether a `git` this module runs only reads the project's repository, or is one of the two
/// steps meant to write to it (the fetch and the fast-forward). A read is held back from writing
/// the index, which it otherwise does behind the caller's back — see
/// [`crate::git_env::read_only`]; a write is not, since both of those steps legitimately take the
/// locks they need. Which one a step is follows from the wrapper it is run through, each named for
/// its access, rather than from an argument a call site passes and can get wrong. A read whose
/// only interesting output is its exit status — a dirty-worktree `git diff --quiet`, say —
/// therefore still goes through [`run_git_read`] and drops what it returns, never through
/// [`run_git_write`]: it is exactly such a command that the index guard matters most for.
#[derive(Clone, Copy)]
enum GitAccess {
    Read,
    Write,
}

/// Runs `git` with `args` in `path` and returns its stdout as UTF-8, reading the repository and
/// nothing more.
pub(crate) fn run_git_read(
    path: &Path,
    args: &[impl AsRef<std::ffi::OsStr>],
) -> Result<String, String> {
    run_git_access(path, args, GitAccess::Read)
}

/// Runs `git` with `args` in `path` and discards its stdout, for a step that writes to the
/// repository: a fetch, a fast-forward, or a push.
pub(crate) fn run_git_write(
    path: &Path,
    args: &[impl AsRef<std::ffi::OsStr>],
) -> Result<(), String> {
    run_git_access(path, args, GitAccess::Write).map(|_| ())
}

/// What both of the above run: the cached shell environment and resolved binary, and the command
/// [`git_command`] builds over them. The error is the verbatim stderr of a failed `git`, the
/// operating-system message of a failure to even start it, or a timeout past
/// `GIT_COMMAND_TIMEOUT` — `GitStatus` carries it as is, untranslated, whichever of the two
/// wrappers produced it.
fn run_git_access(
    path: &Path,
    args: &[impl AsRef<std::ffi::OsStr>],
    access: GitAccess,
) -> Result<String, String> {
    let env = env_shell::cached_snapshot().map_err(|err| format!("{err:#}"))?;
    let git = env_shell::resolve_binary("git", &env).map_err(|err| format!("{err:#}"))?;
    run_git_command(&mut git_command(Path::new(&git), &env, path, args, access))
}

/// The `git <args>` command to run in `path`, with the binary and the environment passed in rather
/// than looked up, so a test can point both at something of its own instead of the user's login
/// shell.
fn git_command(
    git: &Path,
    env: &HashMap<String, String>,
    path: &Path,
    args: &[impl AsRef<std::ffi::OsStr>],
    access: GitAccess,
) -> std::process::Command {
    // `git` runs with the user's shell environment, the same one agents are launched with, rather
    // than the daemon's own minimal one — but only the `GIT_*` variables that say which of the
    // user's own material to use are kept: their configuration, and the certificates and TLS
    // settings a fetch needs (see `git_env::user_configuration_only`). A `GIT_DIR` or
    // `GIT_WORK_TREE` exported in the login shell would otherwise send the branch read, the fetch
    // and the fast-forward alike to another repository, and the remote name picked from this
    // project's repository would be fetched there.
    let mut command =
        crate::git_env::non_interactive(git, crate::git_env::user_configuration_only(env));
    if matches!(access, GitAccess::Read) {
        crate::git_env::read_only(&mut command);
    }
    command.current_dir(path);
    command.args(args);
    command
}

/// Runs a command [`git_command`] built, to completion under `GIT_COMMAND_TIMEOUT`, and returns
/// its stdout as UTF-8.
fn run_git_command(command: &mut std::process::Command) -> Result<String, String> {
    // `run_with_timeout` is generic over whatever bounded subprocess it is given, so neither its
    // pipe-read errors nor its timeout message name `git` on their own — this is where that
    // subject is added.
    let output = crate::subprocess::run_with_timeout(command, GIT_COMMAND_TIMEOUT)
        .context("running git")
        .map_err(|err| format!("{err:#}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// What the `# branch.*` header lines of the read [`BRANCH_HEADER_ARGS`] names say about the local
/// branch and its upstream — everything `GitStatus` needs beyond `repository`, `activity` and
/// `error`, which the caller fills in around this.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct BranchHeader {
    pub(crate) branch: Option<String>,
    pub(crate) detached: bool,
    pub(crate) upstream: Option<String>,
    pub(crate) ahead: u32,
    pub(crate) behind: u32,
}

/// Parses the `# branch.*` header lines of the output of the read [`BRANCH_HEADER_ARGS`] names;
/// every other line (the file-status entries this command also prints) is ignored, which is also
/// what makes an unrecognised `# branch.*` line harmless. A pure function over the text, so it is
/// unit-tested with literal strings rather than a real repository fixture.
pub(crate) fn parse_branch_header(output: &str) -> BranchHeader {
    let mut oid = None;
    let mut initial = false;
    let mut head = None;
    let mut detached = false;
    let mut upstream = None;
    let mut ahead = 0;
    let mut behind = 0;

    for line in output.lines() {
        let Some(rest) = line.strip_prefix("# branch.") else {
            continue;
        };
        if let Some(value) = rest.strip_prefix("oid ") {
            if value == "(initial)" {
                initial = true;
            } else {
                oid = Some(value);
            }
        } else if let Some(value) = rest.strip_prefix("head ") {
            if value == "(detached)" {
                detached = true;
            } else {
                head = Some(value);
            }
        } else if let Some(value) = rest.strip_prefix("upstream ") {
            upstream = Some(value.to_string());
        } else if let Some(value) = rest.strip_prefix("ab ") {
            (ahead, behind) = parse_ab(value);
        }
    }

    // No commit yet means nothing to read a branch name off, whatever `branch.head` says — Git
    // still reports the branch a first commit would land on.
    let branch = if initial {
        None
    } else if detached {
        oid.map(|oid| oid.chars().take(7).collect())
    } else {
        head.map(str::to_string)
    };
    // Meaningless without an upstream, whatever the `ab` line (which should not appear without
    // one) said.
    if upstream.is_none() {
        ahead = 0;
        behind = 0;
    }

    BranchHeader {
        branch,
        detached,
        upstream,
        ahead,
        behind,
    }
}

/// Re-reads the local branch header and publishes it. A push moves the ahead count without a
/// fetch, and this is what the badge shows afterwards; nothing here goes to the remote. Only the
/// header, or the error of failing to read it, reaches the held status: see
/// `AppState::publish_git_reread`.
pub(crate) fn publish_reread(state: &AppState, project: &Project) {
    state.publish_git_reread(&project.id, read_header(Path::new(&project.path)));
}

/// Parses `+<ahead> -<behind>` as printed after `# branch.ab `; a part that does not match reads
/// as no movement rather than panicking on output this command has no business producing.
fn parse_ab(text: &str) -> (u32, u32) {
    let mut ahead = 0;
    let mut behind = 0;
    for part in text.split_whitespace() {
        if let Some(value) = part.strip_prefix('+') {
            ahead = value.parse().unwrap_or(0);
        } else if let Some(value) = part.strip_prefix('-') {
            behind = value.parse().unwrap_or(0);
        }
    }
    (ahead, behind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        git, git_dir_state, isolated_git_env, move_modification_time_back, redirecting_overrides,
        repo_pair, repo_with, ScratchDir, SYSTEM_GIT,
    };

    #[test]
    fn a_normal_branch_with_an_upstream_and_no_movement() {
        let header = parse_branch_header(
            "# branch.oid abcdef0123456789\n\
             # branch.head main\n\
             # branch.upstream origin/main\n\
             # branch.ab +0 -0\n",
        );
        assert_eq!(
            header,
            BranchHeader {
                branch: Some("main".to_string()),
                detached: false,
                upstream: Some("origin/main".to_string()),
                ahead: 0,
                behind: 0,
            }
        );
    }

    #[test]
    fn ahead_and_behind_are_read_from_the_ab_line() {
        let header = parse_branch_header(
            "# branch.oid abcdef0123456789\n\
             # branch.head main\n\
             # branch.upstream origin/main\n\
             # branch.ab +3 -5\n",
        );
        assert_eq!((header.ahead, header.behind), (3, 5));
    }

    #[test]
    fn no_upstream_means_no_ahead_or_behind() {
        let header = parse_branch_header("# branch.oid abcdef0123456789\n# branch.head main\n");
        assert_eq!(header.upstream, None);
        assert_eq!((header.ahead, header.behind), (0, 0));
    }

    #[test]
    fn a_detached_head_reads_the_short_oid_as_the_branch() {
        let header =
            parse_branch_header("# branch.oid abcdef0123456789\n# branch.head (detached)\n");
        assert!(header.detached);
        assert_eq!(header.branch.as_deref(), Some("abcdef0"));
    }

    #[test]
    fn an_empty_initial_repository_has_no_branch() {
        let header = parse_branch_header("# branch.oid (initial)\n# branch.head main\n");
        assert_eq!(header.branch, None);
        assert!(!header.detached);
        assert_eq!(header.upstream, None);
    }

    #[test]
    fn an_unrecognised_branch_line_and_a_file_status_entry_are_both_ignored() {
        let header = parse_branch_header(
            "# branch.oid abcdef0123456789\n\
             # branch.head main\n\
             # branch.something-new value\n\
             1 .M N... 100644 100644 100644 aaaa bbbb file.txt\n",
        );
        assert_eq!(header.branch.as_deref(), Some("main"));
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

    #[test]
    fn a_project_checked_past_the_floor_is_due() {
        let now = Instant::now();
        let last_completed = now - GIT_CHECK_MIN_INTERVAL - Duration::from_secs(1);
        assert!(due_for_check(
            Some(last_completed),
            now,
            GIT_CHECK_MIN_INTERVAL
        ));
    }

    /// What the branch header of [`repo_with_a_stale_stat`] reads as: a branch with an upstream it
    /// is one commit ahead of, so that all four of the fields the check reports carry a value.
    fn stale_stat_header() -> BranchHeader {
        BranchHeader {
            branch: Some("main".to_string()),
            detached: false,
            upstream: Some("origin/main".to_string()),
            ahead: 1,
            behind: 0,
        }
    }

    /// A repository whose only tracked file has the content it was committed with but an older
    /// modification time, which is what makes `git status` want to refresh the index: without it
    /// the read writes nothing whatever its environment says, so the test below would pass even
    /// with the guard removed.
    fn repo_with_a_stale_stat(label: &str) -> ScratchDir {
        let repo = repo_with(label, &[(b"f.txt", b"one\n")]);
        // An upstream to report, with the branch one commit ahead of it. The remote is never
        // contacted — these tests run the read and nothing else — but it has to be configured
        // before `refs/remotes/origin/main` counts as the branch's tracking ref.
        git(&repo, &["remote", "add", "origin", "."]);
        git(&repo, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
        git(&repo, &["branch", "--set-upstream-to=origin/main", "main"]);
        std::fs::write(repo.join("f.txt"), b"two\n").expect("the tracked file is writable");
        git(&repo, &["commit", "-q", "-am", "second"]);
        move_modification_time_back(&repo.join("f.txt"));
        repo
    }

    /// Runs the branch-header read in `repo` with the given access, as [`run_git_read`] would
    /// but over [`isolated_git_env`] and the system `git`, and returns what it reported.
    fn read_header(repo: &Path, access: GitAccess) -> BranchHeader {
        parse_branch_header(&run_header_args(repo, BRANCH_HEADER_ARGS, access))
    }

    /// [`read_header`]'s command with `args` in place of [`BRANCH_HEADER_ARGS`], returning its
    /// raw output.
    fn run_header_args(repo: &Path, args: &[&str], access: GitAccess) -> String {
        let mut command = git_command(
            Path::new(SYSTEM_GIT),
            &isolated_git_env(),
            repo,
            args,
            access,
        );
        run_git_command(&mut command).expect("the branch header is read")
    }

    #[test]
    fn environment_overrides_cannot_redirect_a_command_to_another_repository() {
        let (ours, theirs) = repo_pair("git-status");
        let mut env = isolated_git_env();
        env.extend(
            redirecting_overrides(&theirs)
                .into_iter()
                .map(|(key, value)| (key.to_string(), value)),
        );
        // A read of the file's content, which says whose index and object store it came from; the
        // branch name would follow `GIT_DIR` alone. The worktree and the inline configuration are
        // not seen by it: the unit tests in `git_env` pin them by name.
        let args = ["cat-file", "blob", ":a.txt"];

        // The positive control: the same command without the filter, over the whole environment,
        // reads the other repository.
        let mut unfiltered = crate::git_env::non_interactive(Path::new(SYSTEM_GIT), &env);
        unfiltered.current_dir(&*ours).args(args);
        assert_eq!(
            run_git_command(&mut unfiltered).expect("the unfiltered read runs"),
            "theirs\n"
        );

        // One access is enough: the filter is applied before `git_command` tells a read from a
        // write, so a write goes through the very same line.
        let mut command = git_command(Path::new(SYSTEM_GIT), &env, &ours, &args, GitAccess::Read);
        assert_eq!(
            run_git_command(&mut command).expect("the read runs"),
            "ours\n"
        );
    }

    #[test]
    fn reading_the_branch_header_leaves_the_repository_untouched() {
        let repo = repo_with_a_stale_stat("git-status-read");
        let before = git_dir_state(&repo);
        let header = read_header(&repo, GitAccess::Read);
        assert_eq!(git_dir_state(&repo), before, "the read wrote under .git");
        assert_eq!(header, stale_stat_header());
    }

    #[test]
    fn the_same_read_without_the_guard_rewrites_the_index_and_still_reports_the_same_header() {
        let repo = repo_with_a_stale_stat("git-status-read-unguarded");
        let before = git_dir_state(&repo);
        // The positive control for the test above: the fixture's stale stat information really
        // does make this `git status` write, so the untouched `.git` up there is the guard doing
        // its work and not the fixture failing to provoke any. The header settles the other half
        // — the guard costs the check none of what it reports.
        let header = read_header(&repo, GitAccess::Write);
        assert_ne!(
            git_dir_state(&repo),
            before,
            "an unguarded `git status` was expected to rewrite the index"
        );
        assert_eq!(header, stale_stat_header());
    }

    #[test]
    fn reading_the_branch_header_runs_no_status_inside_a_submodule() {
        let sub = repo_with("git-status-sub", &[(b"a.txt", b"sub\n")]);
        let repo = repo_with("git-status-super", &[(b"f.txt", b"one\n")]);
        git(
            &repo,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "-q",
                "add",
                sub.to_str().expect("the scratch path is UTF-8"),
                "s",
            ],
        );
        // A status run inside the submodule consults its fsmonitor command, which leaves a
        // marker; the superproject's own status never does, as it is configured only there.
        let marker = repo.join("marker");
        git(
            &repo.join("s"),
            &[
                "config",
                "core.fsmonitor",
                &format!("touch '{}'; :", marker.display()),
            ],
        );

        read_header(&repo, GitAccess::Read);
        assert!(
            !marker.exists(),
            "the read ran a status inside the submodule"
        );

        // The positive control: without the flag the same read does reach into the submodule.
        let args: Vec<&str> = BRANCH_HEADER_ARGS
            .iter()
            .copied()
            .filter(|arg| !arg.starts_with("--ignore-submodules"))
            .collect();
        run_header_args(&repo, &args, GitAccess::Read);
        assert!(
            marker.exists(),
            "the fixture never provoked a status in the submodule"
        );
    }
}
