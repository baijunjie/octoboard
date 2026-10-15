#!/bin/sh
# Clean up a branch after merging or opening a PR: check the preconditions first, and only when they hold delete the
# worktree and the local branch.
#   sh flow-cleanup.sh <branch> --merged-into <target-branch> [--ignored-ok]             worktree flow, after the local merge
#   sh flow-cleanup.sh <branch> --pushed-to <push-remote>/<push-target> [--ignored-ok]   PR flow, after the PR is created
#
# Run in the main working copy (the script switches there itself; if the caller's current directory is
# inside the worktree being deleted, the caller has to leave it afterwards).
# 1. Preconditions: --merged-into requires the branch to already be an ancestor of the target branch;
#    --pushed-to requires the local branch and the live state of the remote to point at the same commit,
#    otherwise rewrites that were never pushed would be lost with branch -D. If they do not hold, stop
#    and delete nothing.
# 2. If the branch has its own worktree: stop on uncommitted or untracked files; on ignored files, list
#    them and stop unless --ignored-ok was given (git worktree remove deletes them along with the
#    worktree without prompting; which ones can be regenerated is for the agent to judge or to ask the
#    user). Then git worktree remove, without --force.
# 3. If the branch is checked out in the main working copy: switch back to branch.<branch>.targetBranch
#    (if there is no local one, git switch creates a tracking branch from the single same-named remote
#    branch); stop on uncommitted changes or when the target branch is occupied by another worktree.
# 4. git branch -D (the branch.<branch>.* records are deleted with it). If it fails, stop with 1, but
#    the worktree already deleted and the main working copy already switched back are not undone; fix the
#    cause and re-run to carry on deleting.
# Exit codes: see flow-lib.sh.
set -eu
. "$(dirname "$0")/flow-lib.sh"

usage="usage: flow-cleanup.sh <branch> --merged-into <target-branch> | --pushed-to <push-remote>/<push-target> [--ignored-ok]"
[ $# -ge 3 ] || fatal "$usage"
branch=$1
mode=$2
ref=$3
shift 3
ignored_ok=0
case "${1:-}" in
  --ignored-ok) ignored_ok=1; shift ;;
  "") ;;
  *) fatal "$usage" ;;
esac
[ $# -eq 0 ] || fatal "$usage"
in_repo
main=$(main_worktree)
cd "$main" || fatal "cannot enter the main working copy: $main"

if ! has_branch "$branch"; then
  say "no local branch ${branch} any more, nothing to clean up"
  exit 0
fi
case $mode in
  --merged-into)
    has_branch "$ref" || fatal "no local branch $ref"
    is_ancestor "refs/heads/$branch" "refs/heads/$ref" || stop "branch $branch is not merged into ${ref} yet, not cleaning up"
    ;;
  --pushed-to)
    remote=${ref%%/*}
    dst=${ref#*/}
    [ -n "$remote" ] && [ -n "$dst" ] && [ "$dst" != "$ref" ] || fatal "$usage"
    git remote | grep -Fqx -- "$remote" || fatal "no remote $remote"
    remote_c=$(remote_head "$remote" "$dst")
    [ -n "$remote_c" ] || stop "remote $remote has no branch ${dst}, not cleaning up"
    [ "$remote_c" = "$(git rev-parse "refs/heads/$branch")" ] ||
      stop "local $branch and $remote/$dst are not at the same commit, not cleaning up: changes that were not pushed would be lost with branch -D"
    ;;
  *) fatal "$usage" ;;
esac

wt=$(worktree_of "$branch")
if [ -n "$wt" ] && [ "$wt" != "$main" ]; then
  [ -d "$wt" ] || stop "directory of worktree $wt no longer exists, run git worktree prune first and re-run"
  if in_progress "$wt"; then
    stop "worktree $wt has an unfinished rebase / merge, not cleaning up"
  fi
  dirty=$(git -C "$wt" status --porcelain)
  [ -z "$dirty" ] || stop "worktree $wt has uncommitted or untracked files, not cleaning up" "$dirty"
  ignored=$(git -C "$wt" status --porcelain --ignored | sed -n 's/^!! //p')
  if [ -n "$ignored" ] && [ "$ignored_ok" = 0 ]; then
    stop "worktree $wt has ignored files, and git worktree remove deletes them along with it: once you have confirmed they can all be regenerated (or the user agrees to delete them), re-run with --ignored-ok" "$ignored"
  fi
  git worktree remove "$wt" || stop "git worktree remove failed (see the output above), branch not deleted"
  say "deleted worktree $wt"
elif [ "$wt" = "$main" ]; then
  if in_progress "$main"; then
    stop "the main working copy has an unfinished rebase / merge, not cleaning up"
  fi
  target=$(git config --get "branch.$branch.targetBranch") || stop "branch $branch is checked out in the main working copy, but its target branch cannot be read, so it is unknown which branch to switch back to"
  dirty=$(git status --porcelain --untracked-files=no)
  [ -z "$dirty" ] || stop "the main working copy has uncommitted changes, did not switch back to $target" "$dirty"
  git switch -q "$target" || stop "cannot switch back to ${target} (see the output above; if another worktree occupies it, the user has to handle that), branch not deleted"
  say "main working copy switched back to $target"
fi
git branch -q -D "$branch" || stop "git branch -D failed (see the output above), branch $branch is still there; the cleanup before it is done and not undone, re-run once fixed"
say "deleted local branch $branch"
