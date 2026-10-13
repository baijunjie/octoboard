//! Pushing a project's current branch, and only that branch, when the user asks.
//!
//! The arguments are built here and handed to `git` as a list, never through a shell, and they
//! never include `--force` or a bare `git push`. A bare push follows `push.default`, and
//! `matching` would publish every branch whose name exists on the remote. With an upstream the
//! refspec names that one remote branch; without one, the branch is pushed to `origin` when the
//! repository has it, otherwise to its only remote, and that push records the upstream. Several
//! remotes and no `origin` is refused rather than guessed.

use std::path::Path;

use crate::git_remote::{self, Remote};
use crate::git_status::{self, BranchHeader};
use crate::hostfs;
use crate::protocol::{error_code, CodedError, Project};
use crate::state::AppState;

/// What one push of the current branch runs. The refspec is fully qualified so `push.default`
/// cannot add anything else, and so a branch name is never read as an option.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PushPlan {
    /// The branch already tracks `remote` / `remote_branch`.
    Tracking {
        remote: String,
        local: String,
        remote_branch: String,
    },
    /// No upstream yet. The push records one, to `remote` under the local branch's own name.
    SetUpstream { remote: String, local: String },
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PushRefusal {
    Detached,
    NoCommits,
    /// A name that must not be placed in a refspec: empty, starting with `-`, or containing `:`.
    UnsafeName,
    /// `upstream` did not begin with one of the repository's remote names.
    UpstreamNotARemote,
    /// No upstream, and neither `origin` nor exactly one remote to send the branch to.
    NoRemote,
}

impl PushPlan {
    /// The arguments after `git`. `--` keeps a remote name from being read as an option.
    pub(crate) fn args(&self) -> Vec<String> {
        match self {
            Self::Tracking {
                remote,
                local,
                remote_branch,
            } => vec![
                "push".to_string(),
                "--".to_string(),
                remote.clone(),
                format!("refs/heads/{local}:refs/heads/{remote_branch}"),
            ],
            Self::SetUpstream { remote, local } => vec![
                "push".to_string(),
                "-u".to_string(),
                "--".to_string(),
                remote.clone(),
                format!("refs/heads/{local}:refs/heads/{local}"),
            ],
        }
    }
}

/// Pushes the project's current branch. Answered only once `git push` has exited. A success
/// re-reads the local branch header so the badge's ahead count drops without a fetch.
pub fn push_current_branch(state: &AppState, project: &Project) -> Result<(), anyhow::Error> {
    let path = Path::new(&project.path);
    if !hostfs::is_git_repo(path) {
        return Err(CodedError::raised(
            error_code::NOT_A_GIT_REPOSITORY,
            format!("{} is not in a Git repository.", project.name),
            &[("project", &project.id)],
        ));
    }
    let output = git_status::run_git_read(path, git_status::BRANCH_HEADER_ARGS)
        .map_err(|detail| push_failed(&detail))?;
    let header = git_status::parse_branch_header(&output);
    let run = git_remote::runner(path);
    let inspection = git_remote::inspect(&run)
        .ok_or_else(|| push_failed("the repository's remotes could not be read"))?;
    let plan = plan_push(&header, &inspection.remotes).map_err(refusal)?;
    let args = plan.args();
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    git_status::run_git_write(path, &refs).map_err(|detail| push_failed(&detail))?;
    git_status::publish_reread(state, project);
    Ok(())
}

fn push_failed(detail: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::GIT_PUSH_FAILED,
        format!("Git could not push this branch: {detail}"),
        &[("detail", detail)],
    )
}

fn refusal(refusal: PushRefusal) -> anyhow::Error {
    match refusal {
        PushRefusal::Detached => CodedError::raised(
            error_code::NOT_ON_A_BRANCH,
            "This project is not on a branch, so there is nothing to push.",
            &[],
        ),
        PushRefusal::NoCommits => CodedError::raised(
            error_code::NO_COMMITS_TO_PUSH,
            "This branch has no commits to push yet.",
            &[],
        ),
        // The period is part of `detail`, so a client that fills
        // `Git could not push this branch: {detail}` gets the same sentence as `message`.
        PushRefusal::UnsafeName => push_failed("the branch name cannot be used in a refspec."),
        PushRefusal::UpstreamNotARemote => CodedError::raised(
            error_code::UPSTREAM_NOT_A_REMOTE,
            "This branch's upstream does not name a remote of this repository, so it was not pushed.",
            &[],
        ),
        PushRefusal::NoRemote => CodedError::raised(
            error_code::NO_PUSH_REMOTE,
            "This branch has no upstream, and there is no single remote to push it to.",
            &[],
        ),
    }
}

/// The push `header` and `remotes` allow. Pure over the two, so the choice is unit-tested
/// without a repository.
pub(crate) fn plan_push(
    header: &BranchHeader,
    remotes: &[Remote],
) -> Result<PushPlan, PushRefusal> {
    if header.detached {
        return Err(PushRefusal::Detached);
    }
    let Some(local) = header.branch.as_deref() else {
        return Err(PushRefusal::NoCommits);
    };
    if !usable(local) {
        return Err(PushRefusal::UnsafeName);
    }
    if let Some(upstream) = header.upstream.as_deref() {
        let Some((remote, remote_branch)) = split_upstream(upstream, remotes) else {
            return Err(PushRefusal::UpstreamNotARemote);
        };
        if !usable(remote) || !usable(remote_branch) {
            return Err(PushRefusal::UnsafeName);
        }
        return Ok(PushPlan::Tracking {
            remote: remote.to_string(),
            local: local.to_string(),
            remote_branch: remote_branch.to_string(),
        });
    }
    let Some(remote) = push_remote(remotes) else {
        return Err(PushRefusal::NoRemote);
    };
    if !usable(remote) {
        return Err(PushRefusal::UnsafeName);
    }
    Ok(PushPlan::SetUpstream {
        remote: remote.to_string(),
        local: local.to_string(),
    })
}

/// `upstream` is `git`'s short upstream, `origin/main`. The remote is the longest configured
/// name that is a prefix of it followed by `/`, so a remote named `origin` does not take an
/// upstream of `origin-fork/main`, and a remote named `fork/origin` wins over `fork` for
/// `fork/origin/main`.
fn split_upstream<'a>(upstream: &'a str, remotes: &'a [Remote]) -> Option<(&'a str, &'a str)> {
    let mut best: Option<(&str, &str)> = None;
    for remote in remotes {
        let mut prefix = String::new();
        prefix.push_str(&remote.name);
        prefix.push('/');
        let Some(rest) = upstream.strip_prefix(&prefix) else {
            continue;
        };
        if rest.is_empty() {
            continue;
        }
        if best.is_none_or(|(name, _)| remote.name.len() > name.len()) {
            best = Some((remote.name.as_str(), rest));
        }
    }
    best
}

/// `origin` when the repository has it, otherwise its only remote. Several remotes and no
/// `origin` is ambiguous: the branch does not say which one it belongs to.
fn push_remote(remotes: &[Remote]) -> Option<&str> {
    if let Some(origin) = remotes.iter().find(|remote| remote.name == "origin") {
        return Some(origin.name.as_str());
    }
    if let [only] = remotes {
        return Some(only.name.as_str());
    }
    None
}

fn usable(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && !name.contains(':')
        && !name.contains(['\0', '\n'])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{git, ScratchDir};

    fn header(branch: Option<&str>, detached: bool, upstream: Option<&str>) -> BranchHeader {
        BranchHeader {
            branch: branch.map(str::to_string),
            detached,
            upstream: upstream.map(str::to_string),
            ahead: 0,
            behind: 0,
        }
    }

    fn remotes(names: &[&str]) -> Vec<Remote> {
        names
            .iter()
            .map(|name| Remote {
                name: (*name).to_string(),
                url: format!("https://example.com/{name}.git"),
            })
            .collect()
    }

    #[test]
    fn the_plan_names_one_branch_and_never_forces() {
        let cases = [
            (
                header(Some("main"), false, Some("origin/main")),
                remotes(&["origin", "upstream"]),
                Ok(PushPlan::Tracking {
                    remote: "origin".to_string(),
                    local: "main".to_string(),
                    remote_branch: "main".to_string(),
                }),
            ),
            (
                header(Some("feature"), false, Some("upstream/team/feature")),
                remotes(&["origin", "upstream"]),
                Ok(PushPlan::Tracking {
                    remote: "upstream".to_string(),
                    local: "feature".to_string(),
                    remote_branch: "team/feature".to_string(),
                }),
            ),
            (
                header(Some("main"), false, Some("fork/origin/main")),
                remotes(&["fork", "fork/origin"]),
                Ok(PushPlan::Tracking {
                    remote: "fork/origin".to_string(),
                    local: "main".to_string(),
                    remote_branch: "main".to_string(),
                }),
            ),
            (
                header(Some("feature"), false, None),
                remotes(&["upstream", "origin"]),
                Ok(PushPlan::SetUpstream {
                    remote: "origin".to_string(),
                    local: "feature".to_string(),
                }),
            ),
            (
                header(Some("feature"), false, None),
                remotes(&["mine"]),
                Ok(PushPlan::SetUpstream {
                    remote: "mine".to_string(),
                    local: "feature".to_string(),
                }),
            ),
            (
                header(Some("main"), true, None),
                remotes(&["origin"]),
                Err(PushRefusal::Detached),
            ),
            (
                header(None, false, None),
                remotes(&["origin"]),
                Err(PushRefusal::NoCommits),
            ),
            (
                header(Some("main"), false, Some("gone/main")),
                remotes(&["origin"]),
                Err(PushRefusal::UpstreamNotARemote),
            ),
            (
                header(Some("main"), false, None),
                remotes(&[]),
                Err(PushRefusal::NoRemote),
            ),
            (
                header(Some("main"), false, None),
                remotes(&["one", "two"]),
                Err(PushRefusal::NoRemote),
            ),
            (
                header(Some("-main"), false, None),
                remotes(&["origin"]),
                Err(PushRefusal::UnsafeName),
            ),
        ];
        for (header, remotes, expected) in cases {
            let plan = plan_push(&header, &remotes);
            assert_eq!(plan, expected);
            if let Ok(plan) = plan {
                let args = plan.args();
                assert!(args.contains(&"push".to_string()));
                assert!(!args
                    .iter()
                    .any(|arg| arg == "--force" || arg == "--all" || arg.contains(" --")));
                assert_eq!(
                    args.iter()
                        .filter(|arg| arg.starts_with("refs/heads/"))
                        .count(),
                    1
                );
            }
        }
    }

    #[test]
    fn the_push_arguments_publish_only_the_current_branch_and_record_its_upstream() {
        let root = ScratchDir::new("git-push");
        let bare = root.join("remote.git");
        let repo = root.join("repo");
        std::fs::create_dir(&repo).unwrap();
        git(&root, &["init", "--bare", bare.to_str().unwrap()]);
        git(&repo, &["init"]);
        git(&repo, &["commit", "--allow-empty", "-m", "one"]);
        git(&repo, &["branch", "other"]);
        git(&repo, &["remote", "add", "origin", bare.to_str().unwrap()]);

        let plan = plan_push(&header(Some("main"), false, None), &remotes(&["origin"])).unwrap();
        let args = plan.args();
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        git(&repo, &refs);

        let upstream =
            String::from_utf8(git(&repo, &["rev-parse", "--abbrev-ref", "@{upstream}"])).unwrap();
        assert_eq!(upstream.trim(), "origin/main");
        let published = String::from_utf8(git(&bare, &["branch"])).unwrap();
        assert!(published.contains("main"), "{published}");
        assert!(!published.contains("other"), "{published}");
    }
}
