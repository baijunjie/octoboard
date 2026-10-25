//! Which branch a new branch was created from, the target branch the revert gate holds it
//! against.
//!
//! git writes the start point a branch was created from into the first line of its reflog, as
//! typed (`branch: Created from main`), and `HEAD` for a creation from the checked-out commit
//! (`git switch -c`, `git checkout -b`, a `git worktree add -b` with no start point). `HEAD` names
//! no branch by itself, so the creating hook captures the branch it was on while the transaction
//! was prepared ([`super::RefReport::head`]).
//!
//! A start point that is a local branch (also spelled `refs/heads/<name>` or `heads/<name>`) is
//! the target, and so is the local branch of the same name behind a remote-tracking one
//! (`origin/main`, also spelled `refs/remotes/origin/main` or `remotes/origin/main`). Anything
//! else — a tag, a commit, an expression like `main~2` — names no branch, and the branch gets no
//! target. A reftable repository keeps no reflog to read, so there the branch `HEAD` was on stands
//! in whatever the start point was.

use crate::browse::wire_path;

use super::git_files::{first_reflog_line, reference_exists, uses_reftable};

use std::path::Path;

/// The first line of a reflog message that records a creation, up to the start point.
const CREATED_FROM: &[u8] = b"branch: Created from ";

/// The target branch of `branch` (a wire path) in the repository at `common_dir`, as a wire path.
/// `head` is the branch the creating git's `HEAD` was on. `None` when the start point is no local
/// branch, or is the branch itself.
pub(super) fn target_of(common_dir: &Path, branch: &str, head: Option<&str>) -> Option<String> {
    let target = if uses_reftable(common_dir) {
        head.map(str::to_string)
    } else {
        let (_, message) = first_reflog_line(common_dir, branch)?;
        let start = message.strip_prefix(CREATED_FROM)?;
        if start == b"HEAD" {
            head.map(str::to_string)
        } else {
            local_branch_of(common_dir, start)
        }
    };
    target.filter(|target| target != branch)
}

/// The local branch a start point names: the branch itself, or the one of the same name behind a
/// remote-tracking branch `<remote>/<name>`. A remote whose own name holds a slash is not told
/// apart from a branch name that does.
fn local_branch_of(common_dir: &Path, start: &[u8]) -> Option<String> {
    let local = |name: &[u8]| reference_exists(common_dir, &[b"refs/heads/", name].concat());
    // `refs/heads/main` and `heads/main` name the same branch as `main` does.
    if let Some(name) = start
        .strip_prefix(b"refs/heads/")
        .or_else(|| start.strip_prefix(b"heads/"))
    {
        return local(name).then(|| wire_path::encode(name));
    }
    if local(start) {
        return Some(wire_path::encode(start));
    }
    // `refs/remotes/origin/main` and `remotes/origin/main` name the same as `origin/main` does.
    let tracking = start
        .strip_prefix(b"refs/remotes/")
        .or_else(|| start.strip_prefix(b"remotes/"))
        .unwrap_or(start);
    if !reference_exists(common_dir, &[b"refs/remotes/", tracking].concat()) {
        return None;
    }
    let (_, name) = tracking.split_at(tracking.iter().position(|&b| b == b'/')? + 1);
    local(name).then(|| wire_path::encode(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::branch_origin::repository_of;
    use crate::test_support::{git, repo_with};

    /// The target recorded for `branch`, made by `make` in a repository whose `main` is checked
    /// out, with `HEAD` on `head` as the hook would have captured it.
    fn target_after(make: &[&str], branch: &str, head: Option<&str>) -> Option<String> {
        let repo = repo_with("branch-origin-target", &[(b"a", b"a")]);
        git(&repo, &["branch", "other"]);
        git(&repo, &["update-ref", "refs/remotes/origin/main", "main"]);
        git(&repo, &["update-ref", "refs/remotes/origin/gone", "main"]);
        git(&repo, &["tag", "v1"]);
        git(&repo, make);
        target_of(&repository_of(&repo).unwrap(), branch, head)
    }

    #[test]
    fn a_branch_made_from_a_local_branch_has_it_as_target() {
        assert_eq!(
            target_after(&["branch", "topic", "other"], "topic", Some("main")).as_deref(),
            Some("other")
        );
    }

    #[test]
    fn a_start_point_spelled_as_a_full_reference_is_the_branch_it_names() {
        for start in ["refs/heads/other", "heads/other"] {
            assert_eq!(
                target_after(&["branch", "topic", start], "topic", None).as_deref(),
                Some("other"),
                "{start}"
            );
        }
    }

    #[test]
    fn a_start_point_spelled_as_a_full_remote_reference_is_the_local_branch_behind_it() {
        for start in ["refs/remotes/origin/main", "remotes/origin/main"] {
            assert_eq!(
                target_after(&["branch", "topic", start], "topic", None).as_deref(),
                Some("main"),
                "{start}"
            );
        }
    }

    #[test]
    fn a_start_point_spelled_as_a_local_reference_is_not_looked_for_behind_a_remote() {
        let repo = repo_with("branch-origin-target", &[(b"a", b"a")]);
        git(&repo, &["update-ref", "refs/remotes/origin/main", "main"]);
        let common = repository_of(&repo).unwrap();
        for start in [&b"refs/heads/origin/main"[..], b"heads/origin/main"] {
            assert_eq!(local_branch_of(&common, start), None);
        }
    }

    #[test]
    fn a_branch_made_from_the_checked_out_commit_has_the_branch_head_was_on() {
        for make in [
            &["switch", "-q", "-c", "topic"][..],
            &["checkout", "-q", "-b", "topic"],
            &["branch", "topic"],
        ] {
            assert_eq!(
                target_after(make, "topic", Some("main")).as_deref(),
                Some("main"),
                "{make:?}"
            );
        }
        assert_eq!(
            target_after(&["switch", "-q", "-c", "topic"], "topic", None),
            None,
            "a detached HEAD names no branch"
        );
    }

    #[test]
    fn a_remote_tracking_start_point_is_the_local_branch_of_its_name() {
        assert_eq!(
            target_after(&["branch", "topic", "origin/main"], "topic", None).as_deref(),
            Some("main")
        );
        assert_eq!(
            target_after(&["branch", "topic", "origin/gone"], "topic", None),
            None,
            "there is no local branch of that name"
        );
    }

    #[test]
    fn a_start_point_that_is_no_branch_gives_no_target() {
        for start in ["v1", "HEAD~0", "main~0", "main^{commit}"] {
            let target = target_after(&["branch", "topic", start], "topic", Some("main"));
            assert_eq!(target, None, "{start}");
        }
    }

    #[test]
    fn a_branch_is_never_its_own_target() {
        assert_eq!(
            target_after(&["switch", "-q", "-c", "topic"], "topic", Some("topic")),
            None
        );
    }

    #[test]
    fn a_packed_branch_is_found_too() {
        let repo = repo_with("branch-origin-target-packed", &[(b"a", b"a")]);
        git(&repo, &["branch", "topic", "main"]);
        git(&repo, &["pack-refs", "--all"]);
        assert!(!repo.join(".git/refs/heads/main").exists());
        assert_eq!(
            target_of(&repository_of(&repo).unwrap(), "topic", None).as_deref(),
            Some("main")
        );
    }

    #[test]
    fn a_reftable_repository_falls_back_to_the_branch_head_was_on() {
        let repo = repo_with("branch-origin-target-reftable", &[(b"a", b"a")]);
        let common = repository_of(&repo).unwrap();
        std::fs::create_dir(common.join("reftable")).unwrap();
        assert_eq!(
            target_of(&common, "topic", Some("main")).as_deref(),
            Some("main")
        );
        assert_eq!(target_of(&common, "topic", None), None);
    }

    #[test]
    fn names_with_dot_components_are_no_reference() {
        let repo = repo_with("branch-origin-target-dots", &[(b"a", b"a")]);
        let common = repository_of(&repo).unwrap();
        assert!(
            reference_exists(&common, b"refs/heads/main")
                || reference_exists(&common, b"refs/heads/master")
        );
        assert!(!reference_exists(&common, b"refs/heads/../HEAD"));
        assert!(!reference_exists(&common, b"refs/heads//main"));
        assert!(!reference_exists(&common, b""));
    }
}
