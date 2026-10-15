//! Discarding tracked changes in one project's own worktree, without moving its branch.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use crate::env_shell;
use crate::git_status;
use crate::hostfs;
use crate::protocol::{error_code, CodedError, Project};
use crate::state::AppState;
use crate::subprocess::{self, Bounds};

pub fn restore_project(state: &AppState, project: &Project) -> anyhow::Result<()> {
    let env = env_shell::cached_snapshot()?;
    let git = env_shell::resolve_binary("git", &env)?;
    restore(Path::new(&project.path), &project.id, &git, &env)?;
    git_status::publish_reread(state, project);
    Ok(())
}

fn restore(
    path: &Path,
    project: &str,
    git: &str,
    env: &HashMap<String, String>,
) -> anyhow::Result<()> {
    if !hostfs::is_git_repo(path) {
        return Err(CodedError::raised(
            error_code::NOT_A_GIT_REPOSITORY,
            "This project is not a Git repository.",
            &[("project", project)],
        ));
    }
    let run = |args: &[&[u8]], write| run_git(path, git, env, args, write);
    let header = run(
        &[
            b"status",
            b"--porcelain=v2",
            b"--branch",
            b"--untracked-files=no",
        ],
        false,
    )?;
    let header = git_status::parse_branch_header(&String::from_utf8_lossy(&header));
    if header.detached {
        return Err(CodedError::raised(
            error_code::RESTORE_NOT_ON_BRANCH,
            "This project is not on a branch, so its changes were not restored.",
            &[],
        ));
    }
    if header.branch.is_none() {
        return Err(CodedError::raised(
            error_code::NO_COMMITS_TO_RESTORE,
            "This branch has no commit to restore files from yet.",
            &[],
        ));
    }
    let directory = run(&[b"rev-parse", b"--absolute-git-dir"], false)?;
    let directory = directory.strip_suffix(b"\n").unwrap_or(&directory);
    let directory = PathBuf::from(OsStr::from_bytes(directory));
    for marker in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-merge",
        "rebase-apply",
        "sequencer",
        "BISECT_LOG",
    ] {
        if directory
            .join(marker)
            .try_exists()
            .map_err(|err| failed(&err.to_string()))?
        {
            return Err(CodedError::raised(
                error_code::GIT_OPERATION_IN_PROGRESS,
                "Finish or abort the Git operation in progress before restoring changes.",
                &[],
            ));
        }
    }

    // A non-recursive restore still rewrites gitlinks in the index. Exclude the union of HEAD
    // and the index so an added, removed or changed submodule is preserved on both sides.
    let index = run(&[b"ls-files", b"--stage", b"-z"], false)?;
    let head = run(&[b"ls-tree", b"-r", b"-z", b"HEAD"], false)?;
    let indexed_paths: std::collections::HashSet<&[u8]> = index
        .split(|byte| *byte == 0)
        .filter_map(|entry| {
            entry
                .iter()
                .position(|byte| *byte == b'\t')
                .map(|tab| &entry[tab + 1..])
        })
        .collect();
    for entry in head.split(|byte| *byte == 0) {
        if entry.starts_with(b"160000 ") {
            continue;
        }
        if let Some(tab) = entry.iter().position(|byte| *byte == b'\t') {
            let name = &entry[tab + 1..];
            if !indexed_paths.contains(name) {
                match std::fs::symlink_metadata(path.join(OsStr::from_bytes(name))) {
                    Ok(_) => return Err(failed("a file removed from the index still exists on disk; move it aside before restoring changes.")),
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound
                        || err.kind() == std::io::ErrorKind::NotADirectory => {}
                    Err(err) => return Err(failed(&err.to_string())),
                }
            }
        }
    }
    let mut exclusions = Vec::new();
    let mut has_files = false;
    for entry in index
        .split(|byte| *byte == 0)
        .chain(head.split(|byte| *byte == 0))
    {
        if !entry.is_empty() && !entry.starts_with(b"160000 ") {
            if let Some(tab) = entry.iter().position(|byte| *byte == b'\t') {
                has_files = true;
                protect_untracked_collisions(path, &entry[tab + 1..])?;
            }
        }
        if entry.starts_with(b"160000 ") {
            if let Some(tab) = entry.iter().position(|byte| *byte == b'\t') {
                let mut spec = b":(exclude,literal)".to_vec();
                spec.extend_from_slice(&entry[tab + 1..]);
                if !exclusions.contains(&spec) {
                    exclusions.push(spec);
                }
            }
        }
    }
    if !has_files {
        return Ok(());
    }
    let mut args: Vec<&[u8]> = vec![
        b"restore",
        b"--source=HEAD",
        b"--staged",
        b"--worktree",
        b"--no-recurse-submodules",
        b"--",
        b".",
    ];
    args.extend(exclusions.iter().map(Vec::as_slice));
    run(&args, true)?;
    Ok(())
}

// Git restore removes a directory, including its untracked and ignored children, when HEAD
// expects a file there. Refuse such collisions rather than silently widening the discard.
fn protect_untracked_collisions(root: &Path, name: &[u8]) -> anyhow::Result<()> {
    let file = root.join(OsStr::from_bytes(name));
    let mut candidate = file.as_path();
    while candidate != root {
        match std::fs::symlink_metadata(candidate) {
            Ok(metadata) => {
                let collision = if candidate == file {
                    metadata.is_dir()
                } else {
                    !metadata.is_dir() || metadata.is_symlink()
                };
                if collision {
                    return Err(failed("a tracked path is blocked by a directory, file or symbolic link; resolve the path collision before restoring changes."));
                }
            }
            Err(err)
                if err.kind() == std::io::ErrorKind::NotFound
                    || err.kind() == std::io::ErrorKind::NotADirectory => {}
            Err(err) => return Err(failed(&err.to_string())),
        }
        let Some(parent) = candidate.parent() else {
            return Err(failed("a tracked path is outside the project directory."));
        };
        candidate = parent;
    }
    Ok(())
}

fn run_git(
    path: &Path,
    git: &str,
    env: &HashMap<String, String>,
    args: &[&[u8]],
    write: bool,
) -> anyhow::Result<Vec<u8>> {
    let mut command =
        crate::git_env::non_interactive(git, crate::git_env::user_configuration_only(env));
    command.current_dir(path);
    command.args([
        "--no-pager",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "submodule.recurse=false",
    ]);
    command.env("GIT_NO_REPLACE_OBJECTS", "1");
    command.env("GIT_NO_LAZY_FETCH", "1");
    if !write {
        crate::git_env::read_only(&mut command);
    }
    command.args(args.iter().map(|arg| OsStr::from_bytes(arg)));
    let bounds = Bounds {
        timeout: Duration::from_secs(120),
        stdout_limit: 16 * 1024 * 1024,
        stderr_limit: 64 * 1024,
    };
    let output = subprocess::run_bounded(&mut command, &bounds, &AtomicBool::new(false))
        .map_err(|err| failed(&format!("git {err}")))?;
    if !output.status.success() {
        return Err(failed(String::from_utf8_lossy(&output.stderr).trim()));
    }
    Ok(output.stdout)
}

fn failed(detail: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::GIT_RESTORE_FAILED,
        format!("Git could not restore this project's changes: {detail}"),
        &[("detail", detail)],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{git, isolated_git_env, repo_with, ScratchDir, SYSTEM_GIT};

    fn discard(path: &Path) -> anyhow::Result<()> {
        restore(path, "project", SYSTEM_GIT, &isolated_git_env())
    }

    #[test]
    fn restores_staged_unstaged_deleted_renamed_and_new_tracked_files_without_moving_head() {
        let repo = repo_with(
            "restore-changes",
            &[
                (b"a.txt", b"original"),
                (b"deleted.txt", b"deleted"),
                (b"staged-deleted.txt", b"staged deleted"),
                (b"renamed.txt", b"renamed"),
                (b".gitignore", b"ignored\n"),
            ],
        );
        let head = git(&repo, &["rev-parse", "HEAD"]);
        let branch = git(&repo, &["symbolic-ref", "HEAD"]);
        std::fs::write(repo.join("a.txt"), "staged").unwrap();
        git(&repo, &["add", "a.txt"]);
        std::fs::write(repo.join("a.txt"), "unstaged").unwrap();
        std::fs::remove_file(repo.join("deleted.txt")).unwrap();
        git(&repo, &["rm", "staged-deleted.txt"]);
        git(&repo, &["mv", "renamed.txt", "new-name.txt"]);
        std::fs::write(repo.join("added.txt"), "new staged file").unwrap();
        git(&repo, &["add", "added.txt"]);
        std::fs::write(repo.join("untracked"), "keep").unwrap();
        std::fs::write(repo.join("ignored"), "keep ignored").unwrap();
        discard(&repo).unwrap();
        assert_eq!(std::fs::read(repo.join("a.txt")).unwrap(), b"original");
        assert_eq!(std::fs::read(repo.join("deleted.txt")).unwrap(), b"deleted");
        assert_eq!(std::fs::read(repo.join("renamed.txt")).unwrap(), b"renamed");
        assert_eq!(
            std::fs::read(repo.join("staged-deleted.txt")).unwrap(),
            b"staged deleted"
        );
        assert!(!repo.join("new-name.txt").exists());
        assert!(!repo.join("added.txt").exists());
        assert_eq!(std::fs::read(repo.join("untracked")).unwrap(), b"keep");
        assert_eq!(
            std::fs::read(repo.join("ignored")).unwrap(),
            b"keep ignored"
        );
        assert!(git(&repo, &["status", "--porcelain", "--untracked-files=no"]).is_empty());
        assert_eq!(git(&repo, &["rev-parse", "HEAD"]), head);
        assert_eq!(git(&repo, &["symbolic-ref", "HEAD"]), branch);
        discard(&repo).unwrap();
    }

    #[test]
    fn linked_worktree_changes_are_isolated_and_subdirectory_projects_are_refused() {
        let repo = repo_with("restore-worktree", &[(b"sub/a", b"original")]);
        let holder = ScratchDir::new("restore-linked");
        let worktree = holder.join("worktree");
        git(
            &repo,
            &["worktree", "add", "-b", "other", worktree.to_str().unwrap()],
        );
        let head = git(&worktree, &["rev-parse", "HEAD"]);
        std::fs::write(repo.join("sub/a"), "main change").unwrap();
        std::fs::write(worktree.join("sub/a"), "other change").unwrap();
        assert!(discard(&worktree.join("sub")).is_err());
        assert_eq!(
            std::fs::read(worktree.join("sub/a")).unwrap(),
            b"other change"
        );
        discard(&worktree).unwrap();
        assert_eq!(std::fs::read(worktree.join("sub/a")).unwrap(), b"original");
        assert_eq!(std::fs::read(repo.join("sub/a")).unwrap(), b"main change");
        assert_eq!(git(&worktree, &["rev-parse", "HEAD"]), head);
        assert_eq!(
            git(&worktree, &["symbolic-ref", "HEAD"]),
            b"refs/heads/other\n"
        );
    }

    #[test]
    fn refuses_non_repository_unborn_detached_and_in_progress_operations() {
        let empty = ScratchDir::new("restore-empty");
        assert!(discard(&empty).is_err());
        git(&empty, &["init", "-q"]);
        std::fs::write(empty.join("new"), "keep").unwrap();
        git(&empty, &["add", "new"]);
        assert!(discard(&empty).is_err());
        assert_eq!(std::fs::read(empty.join("new")).unwrap(), b"keep");
        let repo = repo_with("restore-refusals", &[(b"a", b"original")]);
        std::fs::write(repo.join("a"), "keep").unwrap();
        for marker in [
            "MERGE_HEAD",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
            "rebase-merge",
            "rebase-apply",
            "sequencer",
            "BISECT_LOG",
        ] {
            std::fs::write(repo.join(".git").join(marker), b"state").unwrap();
            assert!(discard(&repo).is_err(), "{marker}");
            assert_eq!(std::fs::read(repo.join("a")).unwrap(), b"keep");
            std::fs::remove_file(repo.join(".git").join(marker)).unwrap();
        }
        git(&repo, &["checkout", "--detach"]);
        assert!(discard(&repo).is_err());
        assert_eq!(std::fs::read(repo.join("a")).unwrap(), b"keep");
    }

    #[test]
    fn tracked_symlinks_are_restored_without_touching_their_targets() {
        let repo = repo_with("restore-symlink", &[(b"a", b"original")]);
        let outside = ScratchDir::new("restore-outside");
        std::fs::write(outside.join("target"), "outside").unwrap();
        std::os::unix::fs::symlink(outside.join("target"), repo.join("link")).unwrap();
        git(&repo, &["add", "link"]);
        git(&repo, &["commit", "-m", "link"]);
        std::fs::remove_file(repo.join("link")).unwrap();
        std::fs::write(repo.join("link"), "replacement").unwrap();
        std::fs::remove_file(repo.join("a")).unwrap();
        std::os::unix::fs::symlink(outside.join("target"), repo.join("a")).unwrap();
        discard(&repo).unwrap();
        assert!(repo.join("link").is_symlink());
        assert_eq!(std::fs::read(repo.join("a")).unwrap(), b"original");
        assert_eq!(std::fs::read(outside.join("target")).unwrap(), b"outside");
    }
    #[test]
    fn preserves_submodule_index_and_worktree_even_when_recursion_is_configured() {
        let repo = repo_with("restore-submodule", &[(b"a", b"original")]);
        let module = repo_with("restore-module", &[(b"file", b"module")]);
        git(
            &repo,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                module.to_str().unwrap(),
                "sub*module",
            ],
        );
        git(&repo, &["commit", "-m", "submodule"]);
        git(&repo, &["config", "submodule.recurse", "true"]);
        let submodule = repo.join("sub*module");
        std::fs::write(submodule.join("file"), "new module commit").unwrap();
        git(&submodule, &["add", "file"]);
        git(&submodule, &["commit", "-m", "module update"]);
        git(&repo, &["add", "sub*module"]);
        std::fs::write(submodule.join("file"), "keep dirty module").unwrap();
        let index = git(&repo, &["ls-files", "--stage", "--", "sub*module"]);
        std::fs::write(repo.join("a"), "discard").unwrap();
        discard(&repo).unwrap();
        assert_eq!(
            git(&repo, &["ls-files", "--stage", "--", "sub*module"]),
            index
        );
        assert_eq!(
            std::fs::read(submodule.join("file")).unwrap(),
            b"keep dirty module"
        );
        assert_eq!(std::fs::read(repo.join("a")).unwrap(), b"original");
    }

    #[test]
    fn does_not_delete_untracked_or_ignored_children_blocking_a_tracked_file() {
        for child in ["untracked", "ignored"] {
            let repo = repo_with(
                "restore-directory",
                &[(b"a", b"original"), (b".gitignore", b"ignored\n")],
            );
            std::fs::remove_file(repo.join("a")).unwrap();
            std::fs::create_dir(repo.join("a")).unwrap();
            std::fs::write(repo.join("a").join(child), "keep").unwrap();
            let _ = discard(&repo);
            assert_eq!(std::fs::read(repo.join("a").join(child)).unwrap(), b"keep");
        }
    }
    #[test]
    fn refuses_symlink_or_file_ancestors_without_changing_other_tracked_files() {
        for symlink in [false, true] {
            let repo = repo_with(
                "restore-parent",
                &[(b"dir/a", b"original"), (b"other", b"original")],
            );
            let outside = ScratchDir::new("restore-parent-outside");
            std::fs::write(outside.join("a"), "outside").unwrap();
            std::fs::remove_file(repo.join("dir/a")).unwrap();
            std::fs::remove_dir(repo.join("dir")).unwrap();
            if symlink {
                std::os::unix::fs::symlink(&*outside, repo.join("dir")).unwrap();
            } else {
                std::fs::write(repo.join("dir"), "keep untracked replacement").unwrap();
            }
            std::fs::write(repo.join("other"), "keep because preflight fails").unwrap();
            assert!(discard(&repo).is_err());
            assert_eq!(
                std::fs::read(repo.join("other")).unwrap(),
                b"keep because preflight fails"
            );
            assert_eq!(std::fs::read(outside.join("a")).unwrap(), b"outside");
            if !symlink {
                assert_eq!(
                    std::fs::read(repo.join("dir")).unwrap(),
                    b"keep untracked replacement"
                );
            }
        }
    }

    #[test]
    fn an_empty_commit_has_no_files_to_restore() {
        let repo = ScratchDir::new("restore-empty-commit");
        git(&repo, &["init", "-q"]);
        git(&repo, &["commit", "--allow-empty", "-m", "empty"]);
        std::fs::write(repo.join("untracked"), "keep").unwrap();
        discard(&repo).unwrap();
        assert_eq!(std::fs::read(repo.join("untracked")).unwrap(), b"keep");
    }

    #[test]
    fn shell_git_overrides_cannot_redirect_the_restore_to_another_repository() {
        let repo = repo_with("restore-env-ours", &[(b"a", b"ours")]);
        let other = repo_with("restore-env-theirs", &[(b"a", b"theirs")]);
        std::fs::write(repo.join("a"), "discard").unwrap();
        std::fs::write(other.join("a"), "keep").unwrap();
        let mut env = isolated_git_env();
        env.extend(
            crate::test_support::redirecting_overrides(&other)
                .into_iter()
                .map(|(key, value)| (key.to_string(), value)),
        );
        env.insert("GIT_LITERAL_PATHSPECS".to_string(), "1".to_string());
        restore(&repo, "project", SYSTEM_GIT, &env).unwrap();
        assert_eq!(std::fs::read(repo.join("a")).unwrap(), b"ours");
        assert_eq!(std::fs::read(other.join("a")).unwrap(), b"keep");
    }
    #[test]
    fn does_not_overwrite_files_removed_from_the_index_but_kept_on_disk() {
        let repo = repo_with("restore-untracked-head", &[(b"a", b"original")]);
        git(&repo, &["rm", "--cached", "a"]);
        std::fs::write(repo.join("a"), "keep untracked").unwrap();
        let index = git(&repo, &["ls-files", "--stage"]);
        assert!(discard(&repo).is_err());
        assert_eq!(std::fs::read(repo.join("a")).unwrap(), b"keep untracked");
        assert_eq!(git(&repo, &["ls-files", "--stage"]), index);
    }
}
