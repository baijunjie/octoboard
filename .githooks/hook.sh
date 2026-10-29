#!/bin/sh
# revert-gate hook entry — this line identifies the gate entry point; keep it when editing.
#
# Hook entry point for the repository's three hook stages. Usage: drop it into the hooks directory under
# the name pre-commit / reference-transaction / pre-push, or have another hook system call
# `sh .githooks/hook.sh <pre-commit|reference-transaction|pre-push> "$@"`.
#   - pre-commit: formats the staged source files (`scripts/format-staged.mjs`).
#   - pre-push: runs the revert gate, then the push verification (`scripts/pre-push.mjs`).
#   - reference-transaction: the revert gate alone.
# The two scripts are run from the checkout that is committing or pushing. A checkout that does not
# have one (a branch cut before the script existed) gets a warning and that stage is skipped; a script
# that exists but fails fails the hook.
#
# It runs the gate script as committed on a guarded branch, not the file in the working tree: a merge
# writes the working tree first and only moves the ref at the end, so reading the working tree would let
# the branch being merged decide how the gate judges it. Resident guarded branches (usually the main
# branch) come first, so that an old or modified script on some stale branch cannot become the rule for
# every branch. Repositories with no resident guarded branch (ones that only have the PR flow installed)
# anchor on the target branch recorded by the branch being pushed: take its local branch first, falling
# back to its remote-tracking branch if absent — either way, the published version.
# reference-transaction is called at every stage of every ref transaction — rebase, fetch and each commit
# all trigger it — so the shell filters down to "a guarded branch moved" before starting Python. Guarded
# branches = every branch.*.targetBranch value plus every revert-gate.branch value; pre-push additionally
# filters for "the branch being pushed itself records a branch.*.targetBranch".
# If python3 or the script cannot be found, let the update through: a failing hook would fail every ref
# update.

case "$1" in
  pre-commit|reference-transaction|pre-push) mode=$1; shift ;;
  *) mode=${0##*/} ;;
esac

# Resolve from Git rather than this file: installed copies live in the common hooks directory,
# while each invocation must use the worktree that is actually committing or pushing.
if [ "$mode" = pre-commit ]; then
  root=$(git rev-parse --show-toplevel) || exit 1
  if [ ! -f "${root}/scripts/format-staged.mjs" ]; then
    echo "pre-commit: scripts/format-staged.mjs is missing in this checkout (a branch older than the hook); staged files were not formatted." >&2
    exit 0
  fi
  exec node "${root}/scripts/format-staged.mjs"
fi

# The revert gate. Returns 1 only for a refusal. Reads $mode and $input.
revert_gate() {
# Decide without starting a process wherever possible: transactions that touch no local branch are let
# through immediately.
case "$input" in
  *" refs/heads/"*) ;;
  *) return 0 ;;
esac

nl='
'
config=$(git config --get-regexp '^(branch\..*\.targetbranch|revert-gate\.branch)$') || return 0
fixed= hit= pr=
while IFS=' ' read -r key name; do
  [ -n "${name}" ] || continue
  [ "${key}" != revert-gate.branch ] || fixed="${fixed} ${name}"
  # A development branch that records a target branch (the segment in the key) takes the PR path
  # when it is pushed. Each pre-push line is
  # "<source refspec as written> <local sha> <remote ref> <remote sha>": the first field is matched
  # at line start, but `git push origin HEAD` writes HEAD there, so it is also matched against the
  # third field (remote ref). This only filters it in and leaves the target branch as a candidate for
  # fetching the script; which branch is actually queried is decided by the gate script itself.
  if [ "${mode}" = pre-push ] && [ "${key}" != revert-gate.branch ]; then
    branch=${key#branch.}
    branch=${branch%.targetbranch}
    case "${nl}${input}" in
      *"${nl}refs/heads/${branch} "*|*" refs/heads/${branch} "*) pr="${pr} ${name}" ;;
    esac
  fi
  # The value is a target branch, and it is itself guarded: if this transaction moved it, it must be
  # checked.
  case "${input}${nl}" in
    *" refs/heads/${name}${nl}"*|*" refs/heads/${name} "*) hit="${hit} ${name}" ;;
  esac
done <<EOF_CONFIG
${config}
EOF_CONFIG
[ -n "${hit}${pr}" ] || return 0

command -v python3 >/dev/null 2>&1 || return 0
script=
for name in ${fixed} ${hit} ${pr}; do
  # When the branch does not exist locally (e.g. a fresh clone, or a repo with only the PR flow
  # installed), fall back to its remote-tracking branch — likewise the published version.
  # Only the remote it has configured, plus origin, are trusted: picking "the one same-named branch"
  # by alphabetical order would let someone else's fork decide the gate script.
  remote=$(git config "branch.${name}.remote" 2>/dev/null)
  [ -n "${remote}" ] || remote=origin
  for ref in "refs/heads/${name}" "refs/remotes/${remote}/${name}" "refs/remotes/origin/${name}"; do
    script=$(git cat-file blob "${ref}:.githooks/revert-gate.py" 2>/dev/null) && break
  done
  [ -z "${script}" ] || break
done
[ -n "${script}" ] || return 0
# -I: keep the current directory out of sys.path and ignore PYTHON* environment variables, so a
# same-named .py at the repository root cannot shadow the standard library.
# Only exit code 3 is a refusal by the gate; any other non-zero status (script failed to run, incompatible
# with the local Python) lets the update through.
# Once installed into a clone this file does not get updated with the repository, while the script is read
# from a branch in the repository, so the meaning of exit code 3 must never change.
printf '%s\n' "$input" | python3 -I -c "$script" "$mode" "$@"
[ $? -ne 3 ] || return 1
}

# Stages other than prepared are let through immediately.
# Exiting without draining stdin is fine; git ignores EPIPE for this hook.
if [ "$mode" = reference-transaction ] && [ "$1" != prepared ]; then
  exit 0
fi
input=$(cat) || exit 1
if [ "$mode" = pre-push ]; then
  # The gate's decision comes first: a refusal should show at once, not after a long verification.
  revert_gate "$@" || exit 1
  root=$(git rev-parse --show-toplevel) || exit 1
  if [ ! -f "${root}/scripts/pre-push.mjs" ]; then
    echo "pre-push: scripts/pre-push.mjs is missing in this checkout (a branch older than the hook); push checks were not run." >&2
    exit 0
  fi
  printf '%s\n' "$input" | node "${root}/scripts/pre-push.mjs" "$@" || exit 1
  exit 0
fi
revert_gate "$@" || exit 1
