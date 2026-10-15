#!/bin/sh
# After the target branch has been rebased (lifting the unpushed local commits on top of the latest
# remote when aligning with the remote), move the branches cut from it to the new position one by one:
# sh flow-restack.sh <target-branch> <old-position> [--allow-pushed <branch>]...
#
# <old-position> is the commit of the target branch before the rebase. The branches cut from the
# pre-rewrite commits still carry the old copy, and a plain rebase would move those commits a second
# time, so only the branch's own commits are moved: rebase --onto <target-branch> <old-position>.
# - It looks for branches whose branch.<branch>.targetBranch equals <target-branch>.
# - Whether to move one: if the merge-base of the branch and <old-position> is not on the new target
#   branch, the branch contains pre-rewrite commits. A branch that has been moved no longer meets this,
#   so a re-run skips it.
# - A branch that has been pushed (it has an upstream, and the upstream is not a remote-tracking branch
#   of the target branch) would need a force push after the move, and one that is not exclusively yours
#   should not be touched: stop on it, and once the user confirms it is exclusively theirs, let it
#   through by name with --allow-pushed <branch> (may be given several times).
# - The move has to happen in the branch's own worktree: stop when it has no worktree, when the worktree
#   is not clean, or when the last rebase was not finished. These and conflicts all stop with exit code 1
#   without rolling back: branches already moved stay moved, and on a conflict the rebase state is kept;
#   after resolving the conflict and `rebase --continue`, re-run this script.
# - After the move (including a re-run after the conflicts are resolved), forkPoint is updated to the
#   merge-base of the branch and the target branch: the old fork point points at the rewritten old
#   commits, and leaving it would make the pre-merge check count them.
# Exit codes: see flow-lib.sh.
set -eu
. "$(dirname "$0")/flow-lib.sh"

usage="usage: flow-restack.sh <target-branch> <old-position> [--allow-pushed <branch>]..."
[ $# -ge 2 ] || fatal "$usage"
target=$1
old_arg=$2
shift 2
allowed=" "
while [ $# -gt 0 ]; do
  [ "$1" = --allow-pushed ] && [ $# -ge 2 ] || fatal "$usage"
  allowed="$allowed$2 "
  shift 2
done
in_repo
has_branch "$target" || fatal "no local branch $target"
new=$(git rev-parse "refs/heads/$target")
old=$(commit_of "$old_arg") || fatal "not a commit: $old_arg"

branches=$(git config --get-regexp '^branch\..*\.targetbranch$' |
  awk -v t="$target" '$2 == t { k = $1; sub(/^branch\./, "", k); sub(/\.[^.]*$/, "", k); print k }') || true
moved=
for b in $branches; do
  [ "$b" != "$target" ] && has_branch "$b" || continue
  mb=$(git merge-base "refs/heads/$b" "$old") || continue
  if ! is_ancestor "$mb" "$new"; then
    up=$(git for-each-ref --format='%(upstream)' "refs/heads/$b")
    case $up in
      "" | refs/remotes/*/"$target") ;;
      *)
        case $allowed in
          *" $b "*) ;;
          *) stop "branch $b has been pushed (upstream ${up#refs/remotes/}), and moving it needs a force push: once you have confirmed it is exclusively yours, re-run with --allow-pushed $b" \
            "already moved: ${moved:-none}" ;;
        esac
        ;;
    esac
    wt=$(worktree_of "$b")
    [ -n "$wt" ] || stop "branch $b contains pre-rewrite commits, but it is not checked out in any worktree, not moved" \
      "already moved: ${moved:-none}"
    [ -d "$wt" ] || stop "directory of branch $b's worktree (${wt}) no longer exists, run git worktree prune first" "already moved: ${moved:-none}"
    if in_progress "$wt"; then
      stop "branch $b's worktree (${wt}) has an unfinished rebase / merge, finish it first and re-run" "already moved: ${moved:-none}"
    fi
    dirty=$(git -C "$wt" status --porcelain)
    [ -z "$dirty" ] || stop "branch $b's worktree (${wt}) is not clean, not moved" "$dirty" "already moved: ${moved:-none}"
    git -C "$wt" rebase -q --onto "$new" "$old" ||
      stop "rebase of branch $b conflicts in $wt: resolve the conflicts, run git rebase --continue, then re-run this script" \
        "already moved: ${moved:-none}"
    moved="$moved $b"
  fi
  fork=$(git config --get "branch.$b.forkPoint") || fork=
  if [ -n "$fork" ] && ! is_ancestor "$fork" "$new"; then
    git config "branch.$b.forkPoint" "$(git merge-base "refs/heads/$b" "$new")"
  fi
done
say "moved: ${moved:-none}"
