# shellcheck shell=sh
# Shared functions for the flow scripts (flow-*.sh); each script sources this file, it is never run on its own.
#
# The flow scripts only cover the mechanical operations in the git-pr / git-worktree skills whose steps are
# fixed and which fail silently when done wrong; judgment and platform operations (looking up a PR,
# creating a PR) are left to the agent. Conventions:
# - Depend only on POSIX sh and git, not on python3.
# - Exit codes: 0 done; 1 stopped and handed to the agent, with `[flow] <reason>` on the first line of
#   stderr and the situation and next step in the output after it (in most cases nothing was changed or
#   it was rolled back automatically; exceptions are noted at the top of each script); 2 usage or
#   environment error, nothing was written.
# - No --no-verify and no REVERT_GATE_SKIP: when the repository's checks block something, report it as is.

# stop <reason> [<extra line>...]: stop and hand over to the agent, exit code 1.
stop() {
  printf '[flow] %s\n' "$1" >&2
  shift
  for line in "$@"; do
    [ -z "$line" ] || printf '%s\n' "$line" >&2
  done
  exit 1
}

# fatal <message>: usage or environment error, exit code 2.
fatal() {
  printf '[flow] %s\n' "$*" >&2
  exit 2
}

say() {
  printf '[flow] %s\n' "$*"
}

in_repo() {
  git rev-parse --git-dir >/dev/null 2>&1 || fatal "the current directory is not inside a git repository"
}

commit_of() {
  git rev-parse --verify --quiet "$1^{commit}"
}

is_ancestor() {
  git merge-base --is-ancestor "$1" "$2"
}

has_branch() {
  git show-ref --verify --quiet "refs/heads/$1"
}

# Path of the main working copy (the first worktree).
main_worktree() {
  git worktree list --porcelain | sed -n '1s/^worktree //p'
}

# Path of the worktree a branch lives in: the one that has it checked out, or the one rebasing it
# (HEAD is detached mid-rebase, so the branch name does not show in the worktree list). Prints nothing
# when there is none.
worktree_of() {
  hit=$(git worktree list --porcelain | awk -v ref="refs/heads/$1" '
    /^worktree / { path = substr($0, 10) }
    $0 == "branch " ref { print path; exit }')
  if [ -n "$hit" ]; then
    printf '%s\n' "$hit"
    return 0
  fi
  git worktree list --porcelain | sed -n 's/^worktree //p' | while IFS= read -r path; do
    if (cd "$path" 2>/dev/null && for d in rebase-merge rebase-apply; do
      f=$(git rev-parse --git-path "$d/head-name") && [ -f "$f" ] && [ "$(cat "$f")" = "refs/heads/$1" ] && exit 0
    done; exit 1); then
      printf '%s\n' "$path"
      break
    fi
  done
}

# Whether a working copy has an unfinished rebase / merge / cherry-pick.
# The relative path given by --git-path is relative to that working copy, so go in before checking.
in_progress() {
  (
    cd "$1" || exit 1
    for p in rebase-merge rebase-apply MERGE_HEAD CHERRY_PICK_HEAD; do
      [ ! -e "$(git rev-parse --git-path "$p")" ] || exit 0
    done
    exit 1
  )
}

# Current commit of a branch on the remote; prints nothing when the remote has no such branch; exits 2 when the remote
# cannot be reached.
remote_head() { # remote_head <remote> <branch>
  set +e
  out=$(git ls-remote --exit-code --heads "$1" "refs/heads/$2")
  rc=$?
  set -e
  case $rc in
    0) printf '%s\n' "$out" | awk 'NR == 1 { print $1 }' ;;
    2) ;;
    *) fatal "cannot read branch $2 on remote $1 (git ls-remote exit code ${rc})" ;;
  esac
}
