//! Discarding tracked changes in one project's own worktree, without moving its branch.

use std::path::Path;
use std::time::Duration;

use crate::git::layout::is_git_repo;
use crate::git::pathspec;
use crate::git::repo_files::operation_in_progress;
use crate::git::run::{Access, Limits, Policy, Runner, WorkDir};
use crate::git::tree_entries::entries;
use crate::host_tools::Pace;
use crate::platform::names;
use crate::project_git::header;
use crate::protocol::{error_code, CodedError, Project};
use crate::state::AppState;

/// No configured filesystem monitor, for the restore's writes as for its reads, and no recursion
/// into submodules, which a discard must leave alone. Objects are read only under their own ids
/// (no replacement objects) and one a partial clone lacks is never fetched.
const RESTORE: Policy = Policy::new(
    &[
        "-c",
        "core.fsmonitor=false",
        "-c",
        "submodule.recurse=false",
    ],
    &[("GIT_NO_REPLACE_OBJECTS", "1"), ("GIT_NO_LAZY_FETCH", "1")],
);

pub fn restore_project(state: &AppState, project: &Project) -> anyhow::Result<()> {
    let git = Runner::new(
        state.host_tools.get().git_for(Pace::Person)?,
        RESTORE,
        LIMITS,
    );
    restore(Path::new(&project.path), &project.id, &git)?;
    header::publish_reread(state, project);
    Ok(())
}

fn restore(path: &Path, project: &str, git: &Runner) -> anyhow::Result<()> {
    if !is_git_repo(path) {
        return Err(CodedError::raised(
            error_code::NOT_A_GIT_REPOSITORY,
            "This project is not a Git repository.",
            &[("project", project)],
        ));
    }
    let run = |args: &[&[u8]], access| run_git(path, git, args, access);
    let status_out = run(
        &[
            b"status",
            b"--porcelain=v2",
            b"--branch",
            b"--untracked-files=no",
        ],
        Access::Read,
    )?;
    let header = header::parse_branch_header(&String::from_utf8_lossy(&status_out));
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
    let directory = run(&[b"rev-parse", b"--absolute-git-dir"], Access::Read)?;
    let directory = directory.strip_suffix(b"\n").unwrap_or(&directory);
    let directory = names::path_from_name_bytes(directory)
        .ok_or_else(|| failed("git printed a git directory this host cannot use."))?;
    if operation_in_progress(&directory).map_err(|err| failed(&err.to_string()))? {
        return Err(CodedError::raised(
            error_code::GIT_OPERATION_IN_PROGRESS,
            "Finish or abort the Git operation in progress before restoring changes.",
            &[],
        ));
    }

    // A non-recursive restore still rewrites gitlinks in the index. Exclude the union of HEAD
    // and the index so an added, removed or changed submodule is preserved on both sides.
    let index = run(&[b"ls-files", b"--stage", b"-z"], Access::Read)?;
    let head = run(&[b"ls-tree", b"-r", b"-z", b"HEAD"], Access::Read)?;
    let indexed_paths: std::collections::HashSet<&[u8]> =
        entries(&index).map(|entry| entry.path).collect();
    for entry in entries(&head).filter(|entry| !entry.is_gitlink) {
        if !indexed_paths.contains(entry.path) {
            let entry_path = names::path_from_name_bytes(entry.path)
                .ok_or_else(|| failed("git listed a path this host cannot use."))?;
            match std::fs::symlink_metadata(path.join(entry_path)) {
                Ok(_) => return Err(failed("a file removed from the index still exists on disk; move it aside before restoring changes.")),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound
                    || err.kind() == std::io::ErrorKind::NotADirectory => {}
                Err(err) => return Err(failed(&err.to_string())),
            }
        }
    }
    let mut exclusions = Vec::new();
    let mut has_files = false;
    for entry in entries(&index).chain(entries(&head)) {
        if entry.is_gitlink {
            // From the root, which the project's directory is (a restore requires its `.git`).
            let spec = pathspec::excluded(entry.path);
            if !exclusions.contains(&spec) {
                exclusions.push(spec);
            }
        } else {
            has_files = true;
            protect_untracked_collisions(path, entry.path)?;
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
    run(&args, Access::Write)?;
    Ok(())
}

// Git restore removes a directory, including its untracked and ignored children, when HEAD
// expects a file there. Refuse such collisions rather than silently widening the discard.
fn protect_untracked_collisions(root: &Path, name: &[u8]) -> anyhow::Result<()> {
    let file = root.join(
        names::path_from_name_bytes(name)
            .ok_or_else(|| anyhow::anyhow!("git listed a path this host cannot use."))?,
    );
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

/// What each of the restore's `git` runs is held to.
const LIMITS: Limits = Limits {
    timeout: Duration::from_secs(120),
    stdout: 16 * 1024 * 1024,
    stderr: 64 * 1024,
};

fn run_git(path: &Path, git: &Runner, args: &[&[u8]], access: Access) -> anyhow::Result<Vec<u8>> {
    git.run(&mut restore_command(path, git, args, access))
        .map_err(|err| failed(&err.to_string()))
}

/// The `git <args>` command [`run_git`] runs in `path`.
fn restore_command(
    path: &Path,
    git: &Runner,
    args: &[&[u8]],
    access: Access,
) -> std::process::Command {
    git.command(
        access,
        WorkDir::At(path),
        args.iter().map(|arg| crate::git::run::arg_from_bytes(arg)),
    )
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
    use crate::git::env::BASE_OPTIONS;
    use crate::git::run::UserGit;
    use crate::test_support::{
        git, isolated_git_env, pinned_shell_env, repo_with, CommandLine, ScratchDir, SYSTEM_GIT,
    };

    /// The restore's runner over `git`.
    fn restore_git(git: UserGit) -> Runner {
        Runner::new(git, RESTORE, LIMITS)
    }

    fn discard(path: &Path) -> anyhow::Result<()> {
        restore(
            path,
            "project",
            &restore_git(UserGit::new(SYSTEM_GIT, isolated_git_env())),
        )
    }

    /// The command line of a read and of the restore itself, pinned whole: neither pages, runs a
    /// monitor or recurses into a submodule, replacement objects and lazy fetches are off, and
    /// only the read gets the index guard.
    #[test]
    fn a_read_and_the_restore_build_their_command_lines() {
        let git = restore_git(UserGit::new("/opt/git", pinned_shell_env()));
        let mut leading = BASE_OPTIONS.to_vec();
        leading.extend([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "submodule.recurse=false",
        ]);
        let objects = [("GIT_NO_REPLACE_OBJECTS", "1"), ("GIT_NO_LAZY_FETCH", "1")];
        let read = restore_command(
            Path::new("/repo"),
            &git,
            &[b"ls-files", b"--stage", b"-z"],
            Access::Read,
        );
        let mut args = leading.to_vec();
        args.extend([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "diff.autoRefreshIndex=false",
            "ls-files",
            "--stage",
            "-z",
        ]);
        let mut extra = objects.to_vec();
        extra.push(("GIT_OPTIONAL_LOCKS", "0"));
        assert_eq!(
            CommandLine::of(&read),
            CommandLine::pinned("/opt/git", &args, &extra, Some("/repo"))
        );
        let write = restore_command(
            Path::new("/repo"),
            &git,
            &[b"restore", b"--source=HEAD", b"--", b"."],
            Access::Write,
        );
        let mut args = leading.to_vec();
        args.extend(["restore", "--source=HEAD", "--", "."]);
        assert_eq!(
            CommandLine::of(&write),
            CommandLine::pinned("/opt/git", &args, &objects, Some("/repo"))
        );
    }

    /// A `git` that outlasts its deadline fails the restore with why it was stopped.
    #[test]
    fn a_git_that_outlasts_its_deadline_fails_the_restore_with_the_stop() {
        let repo = repo_with("restore-timeout", &[(b"a.txt", b"ours\n")]);
        let git = restore_git(UserGit::new(SYSTEM_GIT, isolated_git_env()))
            .with_limits(Limits::deadline(Duration::from_millis(100)));
        // An alias standing in for a `git` that is slow to finish.
        let args: [&[u8]; 3] = [b"-c", b"alias.nap=!sleep 1", b"nap"];
        let err = run_git(&repo, &git, &args, Access::Write).expect_err("it is stopped");
        let coded = err.downcast_ref::<CodedError>().unwrap();
        assert_eq!(coded.code, error_code::GIT_RESTORE_FAILED);
        assert!(
            coded.message.contains("git did not finish within 100ms"),
            "{}",
            coded.message
        );
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
        // Git for Windows checks a link out as a plain file unless told the host makes links.
        git(&repo, &["config", "core.symlinks", "true"]);
        let outside = ScratchDir::new("restore-outside");
        std::fs::write(outside.join("target"), "outside").unwrap();
        crate::test_support::symlink(outside.join("target"), repo.join("link")).unwrap();
        git(&repo, &["add", "link"]);
        git(&repo, &["commit", "-m", "link"]);
        std::fs::remove_file(repo.join("link")).unwrap();
        std::fs::write(repo.join("link"), "replacement").unwrap();
        std::fs::remove_file(repo.join("a")).unwrap();
        crate::test_support::symlink(outside.join("target"), repo.join("a")).unwrap();
        discard(&repo).unwrap();
        assert!(repo.join("link").is_symlink());
        assert_eq!(std::fs::read(repo.join("a")).unwrap(), b"original");
        assert_eq!(std::fs::read(outside.join("target")).unwrap(), b"outside");
    }
    #[test]
    fn preserves_submodule_index_and_worktree_even_when_recursion_is_configured() {
        let repo = repo_with("restore-submodule", &[(b"a", b"original")]);
        let module = repo_with("restore-module", &[(b"file", b"module")]);
        // A name a pathspec would read as a pattern; Windows forbids `*` in a name.
        let name = if crate::test_support::creatable("sub*module") {
            "sub*module"
        } else {
            "sub[m]odule"
        };
        git(
            &repo,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                module.to_str().unwrap(),
                name,
            ],
        );
        git(&repo, &["commit", "-m", "submodule"]);
        git(&repo, &["config", "submodule.recurse", "true"]);
        let submodule = repo.join(name);
        std::fs::write(submodule.join("file"), "new module commit").unwrap();
        git(&submodule, &["add", "file"]);
        git(&submodule, &["commit", "-m", "module update"]);
        git(&repo, &["add", name]);
        std::fs::write(submodule.join("file"), "keep dirty module").unwrap();
        let index = git(&repo, &["ls-files", "--stage", "--", name]);
        std::fs::write(repo.join("a"), "discard").unwrap();
        discard(&repo).unwrap();
        assert_eq!(git(&repo, &["ls-files", "--stage", "--", name]), index);
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
                crate::test_support::symlink(&*outside, repo.join("dir")).unwrap();
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
        restore(
            &repo,
            "project",
            &restore_git(UserGit::new(SYSTEM_GIT, env)),
        )
        .unwrap();
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
