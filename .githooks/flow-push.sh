#!/bin/sh
# Push a branch, first making sure it will not overwrite someone else's commits on the remote:
#   sh flow-push.sh <push-remote> <branch> <push-target> [--force]
#
# Handled according to the state of <push-target> on the remote (the remote state is read live with
# ls-remote, not from the last fetch):
# - The remote has no such branch: push -u.
# - The remote tip is an ancestor of the local branch: a plain push.
# - The remote tip is not an ancestor of the local one, but is contained in some position in the local
#   branch's reflog (the local branch once contained it, usually the copy you pushed yourself and later
#   rewrote by squashing or rebasing): this rewrites pushed history. Without --force it stops, so that the
#   agent can confirm the branch is exclusively yours and, after asking the user, re-run with --force; with
#   it, the push uses --force-with-lease pinned to the remote tip just read.
# - Neither: the remote has commits the local branch does not (suggestions accepted on the web, a fix
#   pushed by someone else); stop and do not push.
# Once the reflog has expired (90 days by default) the copy you pushed can no longer be recognized, so it
# stops as in the last case and leaves the judgment to a person; it never pushes by mistake.
# Exit code 1 when the push is rejected by a hook or the remote, with git's output left as is on stderr.
# Exit codes: see flow-lib.sh.
set -eu
. "$(dirname "$0")/flow-lib.sh"

force=0
n=0
for a in "$@"; do
  case $a in
    --force) force=1 ;;
    -*) fatal "unknown argument: $a" ;;
    *)
      n=$((n + 1))
      case $n in
        1) remote=$a ;;
        2) branch=$a ;;
        3) dst=$a ;;
      esac
      ;;
  esac
done
[ "$n" -eq 3 ] || fatal "usage: flow-push.sh <push-remote> <branch> <push-target> [--force]"
in_repo
has_branch "$branch" || fatal "no local branch $branch"
local_c=$(git rev-parse "refs/heads/$branch")
spec="refs/heads/$branch:refs/heads/$dst"

remote_c=$(remote_head "$remote" "$dst")
if [ -z "$remote_c" ]; then
  git push -u "$remote" "$spec" || stop "push rejected (see the output above)"
  say "pushed to $remote/${dst} (first push, upstream set)"
  exit 0
fi
if [ "$remote_c" = "$local_c" ]; then
  say "$remote/$dst is already up to date, nothing to push"
  exit 0
fi
git fetch -q "$remote" "refs/heads/$dst" || fatal "fetch $remote $dst failed"

if is_ancestor "$remote_c" "$local_c"; then
  git push "$remote" "$spec" || stop "push rejected (see the output above)"
  say "pushed to $remote/$dst"
  exit 0
fi

own=0
for pos in $(git reflog show --format=%H "refs/heads/$branch" 2>/dev/null); do
  if is_ancestor "$remote_c" "$pos"; then
    own=1
    break
  fi
done
if [ "$own" = 0 ]; then
  stop "$remote/$dst has commits the local branch does not; not pushing, so as not to overwrite them:" \
    "$(git log --oneline "$local_c..$remote_c")"
fi
if [ "$force" = 0 ]; then
  stop "this would rewrite history already pushed to $remote/$dst: once you have confirmed the branch is exclusively yours and the user has agreed to a force push, re-run with --force"
fi
git push --force-with-lease="refs/heads/$dst:$remote_c" "$remote" "$spec" ||
  stop "push rejected (see the output above)"
say "force-pushed to $remote/${dst} (--force-with-lease, expected value ${remote_c})"
