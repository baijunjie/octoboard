mod local;
mod push;
mod reads;

use std::path::Path;

use super::push::decide_push;
use super::*;
use crate::test_support::{git, isolated_git_env, repo_with, ScratchDir, SYSTEM_GIT};

/// The gate's `git` in `repo`, over the isolated environment the fixtures use.
fn git_in(repo: &Path) -> Git {
    Git::with(
        SYSTEM_GIT,
        repo.to_path_buf(),
        isolated_git_env()
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect(),
    )
}

/// Lines `<word> line 01` to `<word> line <last>`, none of them trivial.
fn numbered(word: &str, range: std::ops::RangeInclusive<u32>) -> String {
    range.map(|n| format!("{word} line {n:02}\n")).collect()
}

fn rev(repo: &Path, spec: &str) -> String {
    String::from_utf8(git(repo, &["rev-parse", spec]))
        .unwrap()
        .trim()
        .to_string()
}

/// Drops `branch`'s reflog, as an expired one: the branch then has no creation point to fall back
/// on.
fn forget_reflog(repo: &Path, branch: &str) {
    git(
        repo,
        &[
            "reflog",
            "expire",
            "--expire=all",
            &format!("refs/heads/{branch}"),
        ],
    );
}

fn commit(repo: &Path, name: &str, content: &str, message: &str) -> String {
    std::fs::write(repo.join(name), content).unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", message]);
    rev(repo, "HEAD")
}

/// A repository whose `a.txt` is ten `alpha` lines.
fn base_repo() -> ScratchDir {
    let repo = repo_with(
        "revert-gate",
        &[
            (b"a.txt", numbered("alpha", 1..=10).as_bytes()),
            (b"other.txt", numbered("other", 1..=6).as_bytes()),
        ],
    );
    // Remote-tracking refs the tests write by hand belong to a remote the repository has.
    git(
        &repo,
        &["remote", "add", "origin", "https://example.invalid/o.git"],
    );
    repo
}

/// A clone of `origin`, with `main` tracking `origin/main`.
fn clone_of(origin: &Path) -> ScratchDir {
    let dir = ScratchDir::new("revert-gate-clone");
    git(&dir, &["clone", "-q", origin.to_str().unwrap(), "."]);
    dir
}

/// What the hook sees at `prepared` for the command `args` run on the checked-out branch: the
/// branch still at its old commit, and the commit the command would move it to.
fn prepared(repo: &Path, args: &[&str]) -> (String, String) {
    let old = rev(repo, "HEAD");
    git(repo, args);
    let new = rev(repo, "HEAD");
    git(repo, &["reset", "-q", "--hard", &old]);
    (old, new)
}

/// The gate's verdict on `branch` moving from `old` to `new` at `state`: `None` to allow, else
/// the rejection. `forks` are the recorded fork points, `(branch, commit)`.
fn gate(
    repo: &Path,
    guard: &[&str],
    forks: &[(&str, &str)],
    state: &str,
    branch: &str,
    old: &str,
    new: &str,
) -> Option<String> {
    let inputs = GateInputs {
        guard: guard.iter().map(|name| name.to_string()).collect(),
        targets: Vec::new(),
        fork_points: forks
            .iter()
            .map(|(name, fork)| (name.to_string(), fork.to_string()))
            .collect(),
    };
    decide_with(repo, &inputs, state, branch, old, new)
}

/// [`gate`] for the given `inputs`.
fn decide_with(
    repo: &Path,
    inputs: &GateInputs,
    state: &str,
    branch: &str,
    old: &str,
    new: &str,
) -> Option<String> {
    let stdin = format!("{old} {new} refs/heads/{branch}\n");
    decide(&git_in(repo), &Inputs::new(inputs), state, stdin.as_bytes()).expect("the gate runs")
}

fn main_move(repo: &Path, forks: &[(&str, &str)], old: &str, new: &str) -> Option<String> {
    gate(repo, &["main"], forks, "prepared", "main", old, new)
}

/// `feature` is squashed after `git reset --soft main`, so its commit has the latest `main` as
/// parent and the content of the old base, so main's own commit `main_change` is undone; the
/// feature also appends to `a.txt`, so that file is no longer exactly the old base's. Returns
/// the repository, the fork point, and main's tip before and the squash after.
fn squash_with_changed_base(
    main_change: impl FnOnce(&Path) -> String,
) -> (ScratchDir, String, String, String) {
    let repo = base_repo();
    let fork = rev(&repo, "HEAD");
    git(&repo, &["switch", "-q", "-c", "feature"]);
    std::fs::write(repo.join("feature.txt"), numbered("feature", 1..=4)).unwrap();
    commit(
        &repo,
        "a.txt",
        &(numbered("alpha", 1..=10) + &numbered("feature", 1..=3)),
        "add the feature",
    );
    git(&repo, &["switch", "-q", "main"]);
    main_change(&repo);
    let old = rev(&repo, "HEAD");
    git(&repo, &["switch", "-q", "feature"]);
    git(&repo, &["reset", "-q", "--soft", "main"]);
    git(&repo, &["commit", "-q", "-m", "add the feature, squashed"]);
    let new = rev(&repo, "HEAD");
    git(&repo, &["switch", "-q", "main"]);
    (repo, fork, old, new)
}

/// The verdict on pushing `local` as `branch`, where the remote held `remote`.
fn push(
    repo: &Path,
    guard: &[&str],
    targets: &[(&str, &str)],
    forks: &[(&str, &str)],
    branch: &str,
    local: &str,
    remote: &str,
) -> Verdict {
    push_lines(
        repo,
        guard,
        targets,
        forks,
        &format!("refs/heads/{branch} {local} refs/heads/{branch} {remote}\n"),
    )
}

/// The verdict on a push whose `pre-push` lines are `stdin`.
fn push_lines(
    repo: &Path,
    guard: &[&str],
    targets: &[(&str, &str)],
    forks: &[(&str, &str)],
    stdin: &str,
) -> Verdict {
    let pairs = |pairs: &[(&str, &str)]| {
        pairs
            .iter()
            .map(|(name, other)| (name.to_string(), other.to_string()))
            .collect()
    };
    let inputs = Inputs::new(&GateInputs {
        guard: guard.iter().map(|name| name.to_string()).collect(),
        targets: pairs(targets),
        fork_points: pairs(forks),
    });
    decide_push(&git_in(repo), &inputs, stdin.as_bytes()).expect("the gate runs")
}
