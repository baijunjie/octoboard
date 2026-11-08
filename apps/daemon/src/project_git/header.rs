//! The local branch header of a project's repository: the `git status` read that names the branch,
//! its upstream and how far they have moved apart, its parse, and what is done with the result —
//! applied to a `GitStatus`, or re-read and published on its own (`publish_reread`) around a push,
//! a switch or a change of the project's refs.

use std::path::Path;

use crate::git::run::Runner;
use crate::host_tools::Pace;
use crate::project_git::{project_git, StepFailure};
use crate::protocol::{GitStatus, Project};
use crate::state::AppState;

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
pub(crate) fn read_branch_header(git: &Runner, path: &Path, status: &mut GitStatus) {
    match read_header(git, path) {
        Ok(header) => apply_header(status, header),
        Err(err) => {
            tracing::debug!(%err, "reading the local branch state failed");
            StepFailure::from(err).record(status);
        }
    }
}

/// Reads the local branch header with `git status`, without touching the remote.
pub(crate) fn read_header(git: &Runner, path: &Path) -> Result<BranchHeader, String> {
    git.read(path, BRANCH_HEADER_ARGS)
        .map(|output| parse_branch_header(&String::from_utf8_lossy(&output)))
        .map_err(|err| err.to_string())
}

pub(crate) fn apply_header(status: &mut GitStatus, header: BranchHeader) {
    status.branch = header.branch;
    status.detached = header.detached;
    status.upstream = header.upstream;
    status.ahead = header.ahead;
    status.behind = header.behind;
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
/// `AppState::publish_git_reread`. It follows on from something else (a push, a switch, a change of
/// the refs), so a `git` that cannot be found is not looked for again.
pub(crate) fn publish_reread(state: &AppState, project: &Project) {
    let header = project_git(state, Pace::Scheduled)
        .map_err(|err| StepFailure::no_git(&err))
        .and_then(|git| read_header(&git, Path::new(&project.path)).map_err(StepFailure::from));
    state.publish_git_reread(&project.id, header);
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
    use crate::git::run::{Access, WorkDir};
    use crate::project_git::test_git;
    use crate::test_support::{
        git, git_dir_state, move_modification_time_back, repo_with, ScratchDir,
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

    /// Runs the branch-header read in `repo` with the given access, as the check does but over
    /// [`test_git`], and returns what it reported.
    fn read_header(repo: &Path, access: Access) -> BranchHeader {
        parse_branch_header(&run_header_args(repo, BRANCH_HEADER_ARGS, access))
    }

    /// [`read_header`]'s command with `args` in place of [`BRANCH_HEADER_ARGS`], returning its
    /// raw output.
    fn run_header_args(repo: &Path, args: &[&str], access: Access) -> String {
        let git = test_git();
        let mut command = git.command(access, WorkDir::At(repo), args);
        let output = git.run(&mut command).expect("the branch header is read");
        String::from_utf8_lossy(&output).into_owned()
    }

    #[test]
    fn reading_the_branch_header_leaves_the_repository_untouched() {
        let repo = repo_with_a_stale_stat("git-status-read");
        let before = git_dir_state(&repo);
        let header = read_header(&repo, Access::Read);
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
        let header = read_header(&repo, Access::Write);
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
        // A status run inside the submodule hashes its changed file through a clean filter that
        // leaves a marker; the superproject's own status never does, as the filter and the
        // attribute naming it are configured only there.
        let marker = repo.join("marker");
        let attributes = repo.join("sub-attributes");
        std::fs::write(&attributes, "a.txt filter=mark\n").expect("the attributes are written");
        let inside = repo.join("s");
        git(
            &inside,
            &[
                "config",
                "core.attributesFile",
                attributes.to_str().expect("the scratch path is UTF-8"),
            ],
        );
        git(
            &inside,
            &[
                "config",
                "filter.mark.clean",
                &format!("touch '{}'; cat", marker.display()),
            ],
        );
        // The same size with another modification time, so only hashing the file tells.
        std::fs::write(inside.join("a.txt"), b"bus\n").expect("the file is writable");
        move_modification_time_back(&inside.join("a.txt"));

        read_header(&repo, Access::Read);
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
        run_header_args(&repo, &args, Access::Read);
        assert!(
            marker.exists(),
            "the fixture never provoked a status in the submodule"
        );
    }
}
