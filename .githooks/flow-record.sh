#!/bin/sh
# Record a branch's target branch and fork point: sh flow-record.sh <branch> <target-branch> <fork-point-commit>
#
# - branch.<branch>.targetBranch is written every time; the value is a branch name without a remote
#   prefix (main, not origin/main).
# - branch.<branch>.forkPoint is not overwritten once it exists: after a rebase the fork point can no
#   longer be computed, so overwriting it would lose it for good. Only when it is missing is the passed
#   commit written (stored as a commit hash). How the fork point is computed is up to the caller; it
#   must be an ancestor of the branch.
# The two keys are shared by git-pr and git-worktree, and the revert gate reads them too. Exit codes: see flow-lib.sh.
set -eu
. "$(dirname "$0")/flow-lib.sh"

[ $# -eq 3 ] || fatal "usage: flow-record.sh <branch> <target-branch> <fork-point-commit>"
branch=$1
target=$2
fork=$3
in_repo
has_branch "$branch" || fatal "no local branch $branch"
[ -n "$target" ] || fatal "the target branch must not be empty"
for remote in $(git remote); do
  case $target in
    "$remote"/*) fatal "write the target branch as a branch name without the remote prefix: $target" ;;
  esac
done

# Validate everything before writing: on an error neither key is touched.
if old=$(git config --get "branch.$branch.forkPoint"); then
  fork_c=
else
  fork_c=$(commit_of "$fork") || fatal "not a commit: $fork"
  is_ancestor "$fork_c" "refs/heads/$branch" || fatal "fork point $fork is not an ancestor of branch $branch"
fi

git config "branch.$branch.targetBranch" "$target"
say "target branch: $target"
if [ -z "$fork_c" ]; then
  say "fork point already recorded as ${old}, not overwritten"
else
  git config "branch.$branch.forkPoint" "$fork_c"
  say "fork point: $fork_c"
fi
