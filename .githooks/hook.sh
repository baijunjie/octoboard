#!/bin/sh
# octoboard repository hook entry — this line identifies the entry point for install.sh; keep it when editing.
#
# Hook entry point for the repository's two hook stages. Usage: drop it into the hooks directory under
# the name pre-commit / pre-push, or have another hook system call
# `sh .githooks/hook.sh <pre-commit|pre-push> "$@"`.
#   - pre-commit: formats the staged source files (`scripts/format-staged.mjs`).
#   - pre-push: runs the push verification (`scripts/pre-push.mjs`) on the pushed lines read from stdin.
# The two scripts are run from the checkout that is committing or pushing. A checkout that does not
# have one (a branch cut before the script existed) gets a warning and that stage is skipped; a script
# that exists but fails fails the hook.

case "$1" in
  pre-commit|pre-push) mode=$1; shift ;;
  *) mode=${0##*/} ;;
esac

case "$mode" in
  pre-commit|pre-push) ;;
  *) exit 0 ;;
esac

# Resolve from Git rather than this file: installed copies live in the common hooks directory,
# while each invocation must use the worktree that is actually committing or pushing.
root=$(git rev-parse --show-toplevel) || exit 1
if [ "$mode" = pre-commit ]; then
  if [ ! -f "${root}/scripts/format-staged.mjs" ]; then
    echo "pre-commit: scripts/format-staged.mjs is missing in this checkout (a branch older than the hook); staged files were not formatted." >&2
    exit 0
  fi
  exec node "${root}/scripts/format-staged.mjs"
fi

if [ "$mode" = pre-push ]; then
  if [ ! -f "${root}/scripts/pre-push.mjs" ]; then
    echo "pre-push: scripts/pre-push.mjs is missing in this checkout (a branch older than the hook); push checks were not run." >&2
    exit 0
  fi
  exec node "${root}/scripts/pre-push.mjs" "$@"
fi
