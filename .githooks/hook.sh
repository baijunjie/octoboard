#!/bin/sh
# octoboard repository hook entry — this line identifies the entry point for install.sh; keep it when editing.
#
# Hook entry point for the repository's two hook stages. Usage: drop it into the hooks directory under
# the name pre-commit / pre-push, or have another hook system call
# `sh .githooks/hook.sh <pre-commit|pre-push> "$@"`.
#   - pre-commit: formats the staged source files (`scripts/hooks/format-staged.mjs`).
#   - pre-push: runs the push verification (`scripts/hooks/pre-push.mjs`) on the pushed lines read from stdin.
# The two scripts are run from the checkout that is committing or pushing. A branch cut before the scripts
# moved into `scripts/hooks/` keeps them at the top of `scripts/`, which is looked at second. A checkout
# that has neither (a branch cut before the script existed) gets a warning and that stage is skipped; a
# script that exists but fails fails the hook.

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
  script=format-staged.mjs
else
  script=pre-push.mjs
fi
if [ -f "${root}/scripts/hooks/${script}" ]; then
  path="${root}/scripts/hooks/${script}"
elif [ -f "${root}/scripts/${script}" ]; then
  path="${root}/scripts/${script}"
else
  if [ "$mode" = pre-commit ]; then
    echo "pre-commit: scripts/hooks/${script} is missing in this checkout (a branch older than the hook); staged files were not formatted." >&2
  else
    echo "pre-push: scripts/hooks/${script} is missing in this checkout (a branch older than the hook); push checks were not run." >&2
  fi
  exit 0
fi
exec node "$path" "$@"
