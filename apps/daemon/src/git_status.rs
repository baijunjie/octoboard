//! Checking a project's git status against its remote, on request — the daemon keeps no timer of
//! its own; the client drives the schedule and asks with `refresh_git_status` (see
//! `apps/daemon/PROTOCOL.md`'s "Daemon behaviour, per project"). Each project is checked at most
//! once at a time, concurrently with the others of its console, and at most once every
//! `GIT_CHECK_MIN_INTERVAL`.
//!
//! `sync_behind_projects` is the module's second entry point, driven by `update_settings` the
//! moment **Automatically sync repositories** is turned on: it only fast-forwards branches already
//! behind their upstream, goes nowhere near the remote, and is therefore outside the floor above —
//! sharing the in-flight claim with the checks, but not their minimum interval.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;

use crate::env_shell;
use crate::hostfs;
use crate::protocol::{GitActivity, GitStatus, Project};
use crate::state::AppState;

/// Upper bound on one `git` subprocess (`fetch`, `status`, `merge`). `GIT_TERMINAL_PROMPT=0` and
/// `ssh -oBatchMode=yes` (see `git_command`) turn a missing credential or an unknown host key
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

/// Checks every one of `projects`' git status, concurrently; a project already being checked, or
/// one whose last check completed less than `GIT_CHECK_MIN_INTERVAL` ago, is skipped. Fire-and-
/// forget: each check reports itself through `AppState::publish_git_status` as it progresses, and
/// there is nothing here for a caller to wait on.
pub fn refresh(state: &Arc<AppState>, projects: Vec<Project>) {
    for project in projects {
        spawn_check(state, project);
    }
}

/// Claims the project's check, or does nothing if one is already running or the minimum interval
/// has not elapsed since the last one completed, then runs it on a blocking thread: every step
/// below shells out to `git`, which blocks. The claim is held by the closure itself and released on
/// drop, panic included, so a step that panics still frees the project for the next sweep instead of
/// excluding it forever.
fn spawn_check(state: &Arc<AppState>, project: Project) {
    if !due_for_check(
        state.git_check_completed_at(&project.id),
        Instant::now(),
        GIT_CHECK_MIN_INTERVAL,
    ) {
        return;
    }
    let Some(claim) = state.claim_git_check(&project.id) else {
        return;
    };
    let state = state.clone();
    tokio::task::spawn_blocking(move || {
        let _claim = claim;
        check_project(&state, &project);
    });
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
/// showing.
fn check_project(state: &AppState, project: &Project) {
    let path = PathBuf::from(&project.path);

    // Checked before the first publish, not after it: publishing `repository: true` and only then
    // discovering the directory is not a repository would have every plain project blink through
    // an in-flight icon it can never actually earn, twice a sweep.
    if !hostfs::is_git_repo(&path) {
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
    // Only the published in-flight status carries the last check's branch, counts and error, so
    // the badge swaps just its glyph while the check runs instead of blanking for its duration.
    // The working `status` stays blank: it is what the fast-forward gate reads, and a `git status`
    // that fails below must not leave cached numbers standing as that gate. Taken from the cache
    // rather than read from the repository, since one more `git` process here would hold the
    // spinner back by however long a process takes to start.
    let in_flight = match state.git_status(&project.id).filter(|s| s.repository) {
        Some(previous) => GitStatus {
            activity: GitActivity::Checking,
            ..previous
        },
        None => status.clone(),
    };
    state.publish_git_status(in_flight);

    let remotes = list_remotes(&path);
    if !remotes.is_empty() {
        let upstream_remote = upstream_remote(&path);
        let remote = fetch_remote(&remotes, upstream_remote.as_deref());
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

    if auto_sync_enabled(state) && syncable(&status) {
        fast_forward(state, &path, &mut status);
    }

    status.activity = GitActivity::Idle;
    // Recorded before this final publish, not after, for the same reason as the non-repository
    // early return above: a project removed mid-check must not have its completion timestamp
    // reinserted right after `publish_git_status` just cleared it.
    state.record_git_check_completed(&project.id);
    state.publish_git_status(status);
}

/// Fast-forwards every project already behind its upstream, concurrently — what turning
/// **Automatically sync repositories** on does right away, instead of leaving the branches to wait
/// for the client's next five-minute sweep. The statuses held in `AppState` only pick which
/// projects are worth visiting, and nothing here goes to the remote: a fetch would duplicate the
/// one the next sweep makes anyway, and the branch is already behind refs that were fetched
/// earlier. A project whose check is in flight is left to that check, which reads the setting
/// itself; a project with no status yet has never been checked, so nothing is known to
/// fast-forward it to. Fire-and-forget, like `refresh`.
///
/// Only the caller has established that the setting is on — this is the one thing the daemon does
/// inside a project's directory, so a caller that has not must not call it.
pub fn sync_behind_projects(state: &Arc<AppState>) {
    for status in state.git_statuses() {
        if !syncable(&status) {
            continue;
        }
        // Both an outright store error and a project that is genuinely gone mean skip: the branch
        // below is only ever touched for a project whose path was just read back from the store,
        // the opposite of `AppState::publish_git_status`'s fail-open choice, because that one risks
        // a stale badge and this one risks a merge in a directory nobody vouched for.
        let Ok(Some(project)) = state.store.get_project(&status.project) else {
            continue;
        };
        let Some(claim) = state.claim_git_check(&project.id) else {
            continue;
        };
        let state = state.clone();
        tokio::task::spawn_blocking(move || {
            let _claim = claim;
            sync_project(&state, &project, status);
        });
    }
}

/// One project's immediate fast-forward. The cached `status` picked the project out, but the gate
/// is read again from the repository before anything moves, because an arbitrary amount of time
/// may have passed since the check that filled it: the user may have switched branch, detached
/// `HEAD`, committed, or taken the directory out of git entirely, and `git merge --ff-only` acts on
/// whatever `HEAD` is now rather than on the branch the cached status described. The re-read is a
/// local `git status`, no network, so it costs nothing the next sweep would not pay anyway.
fn sync_project(state: &AppState, project: &Project, mut status: GitStatus) {
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
    if !syncable(&status) {
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

/// Whether the **Automatically sync repositories** setting is on; a store that cannot be read
/// counts as off, the same as the default — the fast-forward below is the one thing the daemon
/// does inside a project's directory, so it never happens on a guess.
fn auto_sync_enabled(state: &AppState) -> bool {
    state
        .store
        .get_settings()
        .map(|settings| settings.auto_sync_repositories)
        .unwrap_or(false)
}

/// Whether `status` describes a branch the sync may fast-forward: one behind its upstream with no
/// commits of its own (see "Automatically syncing repositories" in
/// `docs/product/project-git-status.md`). Pure over the status so both callers decide alike and
/// the rule is unit-tested without a repository.
fn syncable(status: &GitStatus) -> bool {
    status.upstream.is_some() && status.behind > 0 && status.ahead == 0
}

/// Fast-forwards the branch onto its upstream and reads the branch header again, reporting the
/// fast-forward in flight on `status` while it runs. `status` must have passed `syncable`. A
/// fast-forward `git` refuses records its message, as a failed fetch does.
fn fast_forward(state: &AppState, path: &Path, status: &mut GitStatus) {
    status.activity = GitActivity::Syncing;
    state.publish_git_status(status.clone());
    // `syncable` just checked `upstream` to be `Some`.
    let upstream = status.upstream.clone().expect("upstream is set");
    if let Err(err) = run_git_write(path, &["merge", "--ff-only", &upstream]) {
        tracing::debug!(%err, "fast-forwarding the branch failed");
        status.error = Some(err);
    }
    read_branch_header(path, status);
}

/// The repository's configured remotes, in the order `git remote` prints them, or empty when it
/// has none at all — not one of the steps whose failure is reported on `GitStatus`: a repository
/// with no remote is the ordinary case this exists to tell apart from one the fetch below should
/// actually attempt, not a failure of its own.
fn list_remotes(path: &Path) -> Vec<String> {
    run_git_read(path, &["remote"])
        .map(|output| {
            output
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The branch's upstream remote, e.g. `origin` for an upstream of `origin/main` — `None` when the
/// branch has no upstream, which is the ordinary case for a fresh branch and not reported as a
/// failure.
fn upstream_remote(path: &Path) -> Option<String> {
    let output = run_git_read(
        path,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
    )
    .ok()?;
    remote_from_upstream_ref(&output).map(str::to_string)
}

/// The remote name from an `@{upstream}` ref such as `origin/main` — a pure function over the text
/// so the split is unit-tested with literal strings rather than a real repository.
fn remote_from_upstream_ref(ref_text: &str) -> Option<&str> {
    ref_text.trim().split_once('/').map(|(remote, _)| remote)
}

/// Which remote name, if any, to pass to `git fetch` — a pure function over the repository's
/// remotes and its upstream's remote (when it has one), so the derivation is unit-tested with
/// literal strings. `None` means a bare `git fetch`, which lets `git` fall back to the current
/// branch's upstream remote or `origin` on its own; that fallback is trusted only when there is
/// exactly one remote to begin with — a bare fetch on a repository with several and no upstream can
/// fail against a remote named something other than `origin` (a fork workflow's `upstream`) even
/// though fetching is in no way actually broken.
fn fetch_remote(remotes: &[String], upstream_remote: Option<&str>) -> Option<String> {
    if let Some(remote) = upstream_remote {
        return Some(remote.to_string());
    }
    if let [only] = remotes {
        return Some(only.clone());
    }
    None
}

/// The read of the local branch state: `git status`'s branch header, in the porcelain format whose
/// `# branch.*` lines [`parse_branch_header`] takes. One constant rather than a literal at its one
/// call site, so the test that proves this read leaves the repository's index alone runs the very
/// command the check runs.
const BRANCH_HEADER_ARGS: &[&str] = &[
    "status",
    "--porcelain=v2",
    "--branch",
    "--untracked-files=no",
];

/// Runs `git status --porcelain=v2 --branch --untracked-files=no` and applies what it says to
/// `status`; a failure to run it records `status.error` and leaves every other field of `status`
/// exactly as it was. A caller that reads a `GitStatus` it did not just build must therefore blank
/// whatever it is about to rely on before calling this, as `sync_project` does for the fields its
/// gate reads; on a brand-new `GitStatus` the fields left behind are already the empty reading —
/// `None` branch, no upstream, zero ahead and behind.
/// `--untracked-files=no` skips enumerating the worktree's files, which `parse_branch_header`
/// below never reads anyway (it only takes the `# branch.*` header lines, which this flag does not
/// change) — on a large repository that enumeration is otherwise the dominant cost of the whole
/// check, repeated every project every sweep. Keep the flag even once a feature wants a dirty
/// worktree indicator; that reads `git status`'s file-status entries, which call for their own,
/// separate invocation rather than reviving this one's enumeration for every sweep that does not
/// need it.
fn read_branch_header(path: &Path, status: &mut GitStatus) {
    match run_git_read(path, BRANCH_HEADER_ARGS) {
        Ok(output) => {
            let header = parse_branch_header(&output);
            status.branch = header.branch;
            status.detached = header.detached;
            status.upstream = header.upstream;
            status.ahead = header.ahead;
            status.behind = header.behind;
        }
        Err(err) => {
            tracing::debug!(%err, "reading the local branch state failed");
            status.error = Some(err);
        }
    }
}

/// Whether a `git` this module runs only reads the project's repository, or is one of the two
/// steps meant to write to it (the fetch and the fast-forward). A read is held back from writing
/// the index, which it otherwise does behind the caller's back — see [`git_command`]; a write is
/// not, since both of those steps legitimately take the locks they need. Which one a step is
/// follows from the wrapper it is run through, each named for its access, rather than from an
/// argument a call site passes and can get wrong. A read whose only interesting output is its exit
/// status — a dirty-worktree `git diff --quiet`, say — therefore still goes through
/// [`run_git_read`] and drops what it returns, never through [`run_git_write`]: it is exactly such
/// a command that the index guard matters most for.
#[derive(Clone, Copy)]
enum GitAccess {
    Read,
    Write,
}

/// Runs `git` with `args` in `path` and returns its stdout as UTF-8, reading the repository and
/// nothing more.
fn run_git_read(path: &Path, args: &[&str]) -> Result<String, String> {
    run_git_access(path, args, GitAccess::Read)
}

/// Runs `git` with `args` in `path` and discards its stdout, for one of the two steps that write
/// to the repository.
fn run_git_write(path: &Path, args: &[&str]) -> Result<(), String> {
    run_git_access(path, args, GitAccess::Write).map(|_| ())
}

/// What both of the above run: the cached shell environment and resolved binary, and the command
/// [`git_command`] builds over them. The error is the verbatim stderr of a failed `git`, the
/// operating-system message of a failure to even start it, or a timeout past
/// `GIT_COMMAND_TIMEOUT` — `GitStatus` carries it as is, untranslated, whichever of the two
/// wrappers produced it.
fn run_git_access(path: &Path, args: &[&str], access: GitAccess) -> Result<String, String> {
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
    args: &[&str],
    access: GitAccess,
) -> std::process::Command {
    let mut command = std::process::Command::new(git);
    // `git` runs with the user's shell environment, the same one agents are launched with, rather
    // than the daemon's own minimal one.
    command.env_clear();
    command.envs(env);
    // `git` and `ssh` read a missing credential, an unknown host key or a passphrase-protected
    // key with no agent from the controlling terminal, not stdin — a null stdin (which
    // `run_with_timeout` sets) does nothing to stop that prompt. This is what turns all three into
    // an immediate failure instead of a daemon launched from a terminal blocking forever on them.
    command.env("GIT_TERMINAL_PROMPT", "0");
    command.env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes");
    command.env_remove("GIT_ASKPASS");
    command.env_remove("SSH_ASKPASS");
    if matches!(access, GitAccess::Read) {
        // A porcelain read is not read-only on its own: `git status` refreshes the stat
        // information of a tracked file whose timestamps moved while its content did not, and
        // rewrites `.git/index` to keep it — holding `index.lock` while it does, which is what
        // fails the `git add` or `git commit` an agent is running in that same repository.
        // `GIT_OPTIONAL_LOCKS=0` drops that write-back, and nothing of what the read reports.
        // `-c diff.autoRefreshIndex=false` is the same guarantee for a diff-family read, which
        // refreshes the index whatever `GIT_OPTIONAL_LOCKS` says; both go on every read, so one
        // added later cannot take half of the guarantee.
        command.env("GIT_OPTIONAL_LOCKS", "0");
        command.args(["-c", "diff.autoRefreshIndex=false"]);
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

/// What `git status --porcelain=v2 --branch --untracked-files=no`'s `# branch.*` header lines say
/// about the local branch and its upstream — everything `GitStatus` needs beyond `repository`,
/// `activity` and `error`, which the caller fills in around this.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct BranchHeader {
    branch: Option<String>,
    detached: bool,
    upstream: Option<String>,
    ahead: u32,
    behind: u32,
}

/// Parses the `# branch.*` header lines of `git status --porcelain=v2 --branch --untracked-files=no`'s
/// output; every other line (the file-status entries this command also prints) is ignored, which is also what
/// makes an unrecognised `# branch.*` line harmless. A pure function over the text, so it is
/// unit-tested with literal strings rather than a real repository fixture.
fn parse_branch_header(output: &str) -> BranchHeader {
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
    use crate::browse::git::tests::{git, repo_with};
    use crate::test_support::ScratchDir;

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
    fn only_a_branch_behind_its_upstream_with_no_commits_of_its_own_is_syncable() {
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
        assert!(syncable(&status(Some("origin/main"), 0, 2)));
        // Up to date, ahead as well as behind, and no upstream at all: each left alone.
        assert!(!syncable(&status(Some("origin/main"), 0, 0)));
        assert!(!syncable(&status(Some("origin/main"), 1, 2)));
        assert!(!syncable(&status(None, 0, 2)));
    }

    #[test]
    fn the_remote_name_splits_off_the_first_path_segment_of_an_upstream_ref() {
        assert_eq!(remote_from_upstream_ref("origin/main"), Some("origin"));
        // A branch name containing a slash of its own still yields the remote alone: a remote
        // name never contains one.
        assert_eq!(
            remote_from_upstream_ref("upstream/feature/thing"),
            Some("upstream")
        );
        assert_eq!(remote_from_upstream_ref("origin"), None);
    }

    #[test]
    fn fetch_remote_prefers_the_upstreams_remote() {
        let remotes = vec!["origin".to_string(), "upstream".to_string()];
        assert_eq!(
            fetch_remote(&remotes, Some("upstream")),
            Some("upstream".to_string())
        );
    }

    #[test]
    fn fetch_remote_falls_back_to_the_lone_remote_with_no_upstream() {
        let remotes = vec!["upstream".to_string()];
        assert_eq!(fetch_remote(&remotes, None), Some("upstream".to_string()));
    }

    #[test]
    fn fetch_remote_is_bare_with_several_remotes_and_no_upstream() {
        let remotes = vec!["origin".to_string(), "upstream".to_string()];
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

    /// Every file under `.git`, by path, with its length, a digest of its bytes and its
    /// modification time — what a read must leave untouched. The modification time is part of it
    /// because an index rewritten with the same stat information it already held would compare
    /// equal by content alone; the digest stands in for the bytes so that a failure prints an
    /// entry per file rather than tens of kilobytes of byte literals.
    fn git_dir_state(repo: &Path) -> Vec<(PathBuf, u64, u64, std::time::SystemTime)> {
        use std::hash::{Hash, Hasher};

        let mut state = Vec::new();
        let mut stack = vec![repo.join(".git")];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("the .git directory is readable") {
                let path = entry.expect("its entries are readable").path();
                let meta = std::fs::symlink_metadata(&path).expect("an entry has metadata");
                if meta.is_dir() {
                    stack.push(path);
                    continue;
                }
                let body = std::fs::read(&path).expect("an entry is readable");
                let mut digest = std::hash::DefaultHasher::new();
                body.hash(&mut digest);
                let modified = meta.modified().expect("an entry has a modification time");
                state.push((path, body.len() as u64, digest.finish(), modified));
            }
        }
        state.sort_by(|a, b| a.0.cmp(&b.0));
        state
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

    /// Moves `path`'s modification time a minute back, leaving its content alone.
    fn move_modification_time_back(path: &Path) {
        let file = std::fs::File::options()
            .write(true)
            .open(path)
            .expect("the tracked file is writable");
        file.set_modified(std::time::SystemTime::now() - Duration::from_secs(60))
            .expect("its modification time is settable");
    }

    /// Runs the branch-header read in `repo` with the given access, as [`run_git_read`] would
    /// but over the test process's own environment and the system `git`, and returns what it
    /// reported.
    fn read_header(repo: &Path, access: GitAccess) -> BranchHeader {
        let mut env: HashMap<String, String> = std::env::vars().collect();
        env.retain(|key, _| !key.starts_with("GIT_"));
        // The user's own configuration is left out, as it is for every other `git` a test runs.
        env.insert("GIT_CONFIG_GLOBAL".to_string(), "/dev/null".to_string());
        env.insert("GIT_CONFIG_NOSYSTEM".to_string(), "1".to_string());
        let mut command = git_command(
            Path::new("/usr/bin/git"),
            &env,
            repo,
            BRANCH_HEADER_ARGS,
            access,
        );
        let output = run_git_command(&mut command).expect("the branch header is read");
        parse_branch_header(&output)
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
}
