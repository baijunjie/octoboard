//! Switching a project's own worktree to an existing local branch at the user's request.

use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use crate::browse::wire_path::RelPath;
use crate::git_status;
use crate::hostfs;
use crate::protocol::{error_code, CodedError, Project};
use crate::state::AppState;

pub fn switch_branch(state: &AppState, project: &Project, branch: &str) -> anyhow::Result<()> {
    let path = Path::new(&project.path);
    if !hostfs::is_git_repo(path) {
        return Err(CodedError::raised(
            error_code::NOT_A_GIT_REPOSITORY,
            format!("{} is not in a Git repository.", project.name),
            &[("project", &project.id)],
        ));
    }
    switch_local(branch, |args, write| {
        if write {
            git_status::run_git_write(path, args).map(|()| String::new())
        } else {
            git_status::run_git_read(path, args)
        }
    })?;
    git_status::publish_reread(state, project);
    Ok(())
}

fn switch_local(
    branch: &str,
    run: impl Fn(&[OsString], bool) -> Result<String, String>,
) -> anyhow::Result<()> {
    let args = switch_args(branch).map_err(|detail| failed(&detail))?;
    // Fully qualified lookup prevents Git's previous-branch syntax and remote guessing from
    // choosing anything other than the local branch the menu named.
    let mut local = OsString::from("refs/heads/");
    local.push(&args[3]);
    run(&["check-ref-format".into(), local.clone()], false).map_err(|detail| {
        failed(if detail.is_empty() {
            "the local branch name is invalid."
        } else {
            &detail
        })
    })?;
    run(
        &[
            "show-ref".into(),
            "--verify".into(),
            "--quiet".into(),
            local,
        ],
        false,
    )
    .map_err(|detail| {
        failed(if detail.is_empty() {
            "the local branch no longer exists."
        } else {
            &detail
        })
    })?;
    run(&args, true).map_err(|detail| failed(&detail))?;
    Ok(())
}

fn switch_args(branch: &str) -> Result<Vec<OsString>, String> {
    let name = RelPath::parse(branch)
        .filter(|name| !name.is_root())
        .ok_or_else(|| "the local branch name is invalid.".to_string())?;
    if name.as_bytes().starts_with(b"-") || name.as_bytes().contains(&b'\n') {
        return Err("the local branch name is invalid.".to_string());
    }
    Ok(vec![
        "switch".into(),
        "--no-guess".into(),
        "--".into(),
        OsStr::from_bytes(name.as_bytes()).to_owned(),
    ])
}

fn failed(detail: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::GIT_SWITCH_FAILED,
        format!("Git could not switch branches: {detail}"),
        &[("detail", detail)],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{git, isolated_git_env, repo_with, SYSTEM_GIT};

    fn run(path: &Path, branch: &str) -> anyhow::Result<()> {
        switch_local(branch, |args, _write| {
            let mut command =
                crate::git_env::non_interactive(Path::new(SYSTEM_GIT), &isolated_git_env());
            let output = command.current_dir(path).args(args).output().unwrap();
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).into_owned())
            } else {
                Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
            }
        })
    }

    #[test]
    fn switching_updates_head_and_preserves_safe_local_changes() {
        let repo = repo_with("switch-local", &[(b"a.txt", b"one\n")]);
        git(&repo, &["branch", "feature/100%"]);
        std::fs::write(repo.join("a.txt"), "local\n").unwrap();
        assert!(run(&repo, "feature/100%25").is_ok());
        assert_eq!(git(&repo, &["branch", "--show-current"]), b"feature/100%\n");
        assert_eq!(std::fs::read(repo.join("a.txt")).unwrap(), b"local\n");
    }

    #[test]
    fn a_conflicting_change_and_branch_used_in_another_worktree_fail_without_discarding() {
        let repo = repo_with("switch-conflict", &[(b"a.txt", b"one\n")]);
        git(&repo, &["switch", "-c", "feature"]);
        std::fs::write(repo.join("a.txt"), "feature\n").unwrap();
        git(&repo, &["commit", "-am", "feature"]);
        git(&repo, &["switch", "main"]);
        std::fs::write(repo.join("a.txt"), "local\n").unwrap();
        let output = run(&repo, "feature");
        let failure = output.unwrap_err();
        let coded = failure.downcast_ref::<CodedError>().unwrap();
        assert_eq!(coded.code, error_code::GIT_SWITCH_FAILED);
        assert!(coded
            .params
            .get("detail")
            .is_some_and(|detail| !detail.is_empty()));
        assert_eq!(std::fs::read(repo.join("a.txt")).unwrap(), b"local\n");
        assert_eq!(git(&repo, &["branch", "--show-current"]), b"main\n");
        git(&repo, &["worktree", "add", "other", "feature"]);
        std::fs::write(repo.join("a.txt"), "one\n").unwrap();
        assert!(!run(&repo, "feature").is_ok());
        assert_eq!(git(&repo, &["branch", "--show-current"]), b"main\n");
    }
    #[test]
    fn only_literal_local_refs_can_be_selected() {
        let repo = repo_with("switch-validation", &[(b"a", b"one")]);
        git(&repo, &["remote", "add", "origin", "."]);
        git(
            &repo,
            &["update-ref", "refs/remotes/origin/remote-only", "HEAD"],
        );
        for branch in [
            "remote-only",
            "@{-1}",
            "main~1",
            "refs/heads/main",
            "--detach",
            "",
            "%6Dain",
            "gone",
        ] {
            let error = run(&repo, branch).unwrap_err();
            assert_eq!(
                error.downcast_ref::<CodedError>().unwrap().code,
                error_code::GIT_SWITCH_FAILED
            );
            assert_eq!(git(&repo, &["branch", "--show-current"]), b"main\n");
        }
        assert!(!String::from_utf8(git(&repo, &["branch"]))
            .unwrap()
            .contains("remote-only"));
    }
    #[test]
    fn the_public_operation_refuses_plain_directories_and_repository_subdirectories() {
        let (state, _scratch) = crate::test_support::app_state("switch-nonrepo");
        let repo = repo_with("switch-parent", &[(b"sub/a", b"one")]);
        for path in [&*_scratch, &repo.join("sub")] {
            let project: Project = serde_json::from_value(serde_json::json!({
                "id": "project", "console_id": "console", "host_id": "host", "name": "Project",
                "path": path, "source": "local", "trust_consent": false, "pinned": false, "tags": []
            }))
            .unwrap();
            let error = switch_branch(&state, &project, "main").unwrap_err();
            assert_eq!(
                error.downcast_ref::<CodedError>().unwrap().code,
                error_code::NOT_A_GIT_REPOSITORY
            );
        }
    }
}
