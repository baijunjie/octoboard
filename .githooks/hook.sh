#!/bin/sh
# revert-gate hook entry — this line identifies the gate entry point; keep it when editing.
#
# Hook entry point for the revert gate. Usage: drop it into the hooks directory under the name
# reference-transaction / pre-push, or have another hook system call
# `sh .githooks/hook.sh <reference-transaction|pre-push> "$@"`.
#
# It runs the gate script as committed on a guarded branch, not the file in the working tree: a merge
# writes the working tree first and only moves the ref at the end, so reading the working tree would let
# the branch being merged decide how the gate judges it. Resident guarded branches (usually the main
# branch) come first, so that an old or modified script on some stale branch cannot become the rule for
# every branch.
# reference-transaction is called at every stage of every ref transaction — rebase, fetch and each commit
# all trigger it — so the shell filters down to "a guarded branch moved" before starting Python. Guarded
# branches = every branch.*.worktreeTarget value plus every revert-gate.branch value.
# If python3 or the script cannot be found, let the update through: a failing hook would fail every ref
# update.

case "$1" in
  reference-transaction|pre-push) mode=$1; shift ;;
  *) mode=${0##*/} ;;
esac

# Decide without starting a process wherever possible: stages other than prepared, and transactions that
# touch no local branch, are let through immediately.
# Exiting without draining stdin is fine; git ignores EPIPE for this hook.
if [ "$mode" = reference-transaction ] && [ "$1" != prepared ]; then
  exit 0
fi
input=$(cat)
case "$input" in
  *" refs/heads/"*) ;;
  *) exit 0 ;;
esac

nl='
'
config=$(git config --get-regexp '^(branch\..*\.worktreetarget|revert-gate\.branch)$') || exit 0
fixed= hit=
while IFS=' ' read -r key name; do
  [ -n "${name}" ] || continue
  [ "${key}" = revert-gate.branch ] && fixed="${fixed} ${name}"
  case "${input}${nl}" in
    *" refs/heads/${name}${nl}"*|*" refs/heads/${name} "*) hit="${hit} ${name}" ;;
  esac
done <<EOF_CONFIG
${config}
EOF_CONFIG
[ -n "${hit}" ] || exit 0

command -v python3 >/dev/null 2>&1 || exit 0
script=
for name in ${fixed} ${hit}; do
  script=$(git cat-file blob "refs/heads/${name}:.githooks/revert-gate.py" 2>/dev/null) && break
done
[ -n "${script}" ] || exit 0
# -I: keep the current directory out of sys.path and ignore PYTHON* environment variables, so a
# same-named .py at the repository root cannot shadow the standard library.
# Only exit code 3 is a refusal by the gate; any other non-zero status (script failed to run, incompatible
# with the local Python) lets the update through.
# Once installed into a clone this file does not get updated with the repository, while the script is read
# from a branch in the repository, so the meaning of exit code 3 must never change.
printf '%s\n' "$input" | python3 -I -c "$script" "$mode" "$@"
[ $? -ne 3 ] || exit 1
