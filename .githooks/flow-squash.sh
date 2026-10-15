#!/bin/sh
# Squash the current branch on its original base, verify afterwards, and roll back automatically on error.
# Run in the branch's own working copy:
#   sh flow-squash.sh <base-ref> <commit-message-file>      squash the commits ahead of the base into one
#   sh flow-squash.sh --check <base-ref> <pre-squash-commit>   after splitting into several commits yourself, only verify
#
# The original base is `git merge-base HEAD <base-ref>`. Do not reset to the base's latest commit: with
# a different base and unmerged content, the old code would be squashed in as a new change and silently
# revert whatever landed on the target branch after that base.
# Squash mode:
# 1. Preconditions: HEAD is on a branch, the working tree is clean (including untracked files), no
#    rebase / merge is half done, the branch is at least one commit ahead of the base, and none of those
#    commits is a merge commit. If they do not hold, stop and change nothing.
# 2. Reverts declared in the original commits (`Reverts: <sha>` or `This reverts commit <sha>`): stop
#    for any that is missing from the commit message file; dropping it would turn the revert into an
#    undeclared one, which the revert gate would block. A reverted commit that is itself inside this
#    squash (the branch committed it and reverted it itself) does not count, since it never reaches the
#    target branch.
# 3. After reset --soft to the original base, commit again with the commit message file. The pre-squash
#    commit is recorded by the caller (it is also printed in the output); the script does not save it
#    anywhere else, to avoid leaving a stale record.
# 4. Verify: the new commit's parent is the original base and its content matches the pre-squash commit.
#    If not (or if the commit itself fails, for example rejected by a commit-msg hook), reset --hard back
#    to the pre-squash commit and exit with 1.
# Check mode only does the verification in step 4, with the criterion changed to "the merge-base of HEAD
# and the base is still the original base"; on failure it does not roll back automatically.
# Exit codes: see flow-lib.sh.
set -eu
. "$(dirname "$0")/flow-lib.sh"

check_only=0
if [ "${1:-}" = --check ]; then
  check_only=1
  shift
fi
[ $# -eq 2 ] || fatal "usage: flow-squash.sh <base-ref> <commit-message-file> or flow-squash.sh --check <base-ref> <pre-squash-commit>"
in_repo
base_c=$(commit_of "$1") || fatal "not a commit: $1"
branch=$(git symbolic-ref --quiet --short HEAD) || fatal "HEAD is not on a branch (detached), not squashing"

if [ "$check_only" = 1 ]; then
  pre=$(commit_of "$2") || fatal "not a commit: $2"
  mb=$(git merge-base "$pre" "$base_c") || fatal "the pre-squash commit and $1 have no common ancestor"
  if [ "$(git merge-base HEAD "$base_c")" != "$mb" ]; then
    stop "check failed: the base changed, the merge-base of HEAD and $1 is no longer the original base $mb" \
      "go back to the pre-squash commit and redo it: git reset --hard $pre"
  fi
  if [ -n "$(git rev-list --merges "$mb..HEAD")" ]; then
    stop "check failed: the squashed commits include a merge commit" "go back to the pre-squash commit and redo it: git reset --hard $pre"
  fi
  if ! git diff --quiet "$pre" HEAD; then
    stop "check failed: content differs from the pre-squash commit" "$(git diff --stat "$pre" HEAD)" \
      "go back to the pre-squash commit and redo it: git reset --hard $pre"
  fi
  say "check passed"
  exit 0
fi

msg=$2
[ -f "$msg" ] && [ -s "$msg" ] || fatal "commit message file does not exist or is empty: $msg"
if in_progress .; then
  stop "a rebase / merge / cherry-pick is half done, finish it first"
fi
dirty=$(git status --porcelain)
[ -z "$dirty" ] || stop "working tree is not clean (including untracked files), not squashing: uncommitted changes would get mixed into the commit" "$dirty"
head=$(git rev-parse HEAD)
mb=$(git merge-base HEAD "$base_c") || fatal "the current branch and $1 have no common ancestor"
[ "$(git rev-list --count "$mb..HEAD")" -gt 0 ] || stop "no commits ahead of $1, nothing to squash"
[ -z "$(git rev-list --merges "$mb..HEAD")" ] ||
  stop "the branch contains a merge commit: squashing would squash in the changes of the other branch that was merged in, not squashing"

declared=$(git log --format=%B "$mb..HEAD" |
  grep -Eio '^(reverts:[[:space:]]*|this reverts commit )[0-9a-f]{7,64}' |
  grep -Eio '[0-9a-f]{7,64}$' | tr 'A-F' 'a-f' | sort -u) || true
missing=
for sha in $declared; do
  if c=$(commit_of "$sha" 2>/dev/null) && is_ancestor "$c" HEAD && ! is_ancestor "$c" "$mb"; then
    continue
  fi
  grep -Eiq "^(reverts:[[:space:]]*|this reverts commit )$sha" "$msg" ||
    missing="$missing${missing:+
}Reverts: $sha"
done
[ -z "$missing" ] || stop "the commit message is missing reverts declared in the original commits, add these lines and squash again:" "$missing"

git reset -q --soft "$mb" || stop "reset was rejected (see the output above), nothing was squashed"

restore() {
  git reset -q --hard "$head"
  stop "$1, rolled back to the pre-squash commit $head" "$2"
}
git commit -q -F "$msg" || restore "commit failed (see git's output above)" ""
[ "$(git rev-parse HEAD^)" = "$mb" ] || restore "check failed: the new commit's parent is not the original base $mb" ""
git diff --quiet "$head" HEAD || restore "check failed: content differs from the pre-squash commit" ""
say "squashed into one commit $(git rev-parse --short HEAD), original base ${mb}, pre-squash commit $head"
