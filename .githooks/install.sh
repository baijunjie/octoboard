#!/bin/sh
# Enable the revert gate in this clone: sh .githooks/install.sh [--branches-only] [<resident-guarded-branch>...]
# The resident guarded branches (usually the main branch) are written to this clone's git config as
# revert-gate.branch, replacing any existing list; the target branch recorded by each worktree branch is
# guarded automatically on top of that. Branch arguments are used when given; otherwise the committed
# .githooks/branches is read (one branch per line, main branch first, lines starting with # and blank lines
# ignored). At run time the gate reads only the git config, never this file: files in the working tree can be
# changed by the branch being merged, and must not influence what the gate decides.
# --branches-only: write only the resident guarded branch config and install no hooks, for a clone whose
# hooks are owned by a system such as husky (core.hooksPath is set, and .githooks/hook.sh is called from
# within that system).
# Run once per clone; repeatable. All worktrees share the common hooks directory.
set -eu

hooks_too=1
if [ "${1:-}" = --branches-only ]; then
  hooks_too=0
  shift
fi

root=$(git rev-parse --show-toplevel)
hooks="$(git rev-parse --git-common-dir)/hooks"
list="${root}/.githooks/branches"

if [ "${hooks_too}" = 1 ] && [ -n "$(git config core.hooksPath || true)" ]; then
  echo "core.hooksPath is set to $(git config core.hooksPath), so the common hooks directory will not take effect; call sh .githooks/hook.sh <reference-transaction|pre-push> \"\$@\" from within that hook system, then run sh .githooks/install.sh --branches-only to write the resident guarded branches." >&2
  exit 1
fi

if [ $# -gt 0 ]; then
  from="the arguments"
else
  [ -f "${list}" ] || {
    echo "No ${list}; nothing was installed. Use sh .githooks/install.sh <main-branch> to name the resident guarded branches." >&2
    exit 1
  }
  branches=$(sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//' -e '/^#/d' -e '/^$/d' "${list}")
  [ -n "${branches}" ] || {
    echo "${list} lists no branches; nothing was installed." >&2
    exit 1
  }
  set -f
  # shellcheck disable=SC2086 # branch names contain no whitespace, so splitting on lines is enough
  set -- ${branches}
  set +f
  from="${list}"
fi

# A fresh clone usually has only remote-tracking branches besides the default one, so we only require the
# branch to exist locally or on some remote. This catches misspelled names.
for branch in "$@"; do
  git rev-parse --verify -q "refs/heads/${branch}" >/dev/null ||
    [ -n "$(git for-each-ref --count=1 "refs/remotes/*/${branch}")" ] || {
    echo "Branch ${branch} from ${from} was found neither locally nor on any remote; nothing was installed." >&2
    exit 1
  }
done

if [ "${hooks_too}" = 1 ]; then
  # Check everything before writing anything: bailing out halfway would leave half-installed hooks and
  # unwritten resident guarded branches.
  for name in reference-transaction pre-push; do
    target="${hooks}/${name}"
    if [ -e "${target}" ] && ! grep -q '^# revert-gate hook entry' "${target}"; then
      echo "${target} already exists and is not a gate entry point; nothing was installed. Call sh .githooks/hook.sh ${name} \"\$@\" from inside it." >&2
      exit 1
    fi
  done

  mkdir -p "${hooks}"
  for name in reference-transaction pre-push; do
    target="${hooks}/${name}"
    cp "${root}/.githooks/hook.sh" "${target}"
    chmod +x "${target}"
  done
fi

git config --unset-all revert-gate.branch || true
for branch in "$@"; do
  git config --add revert-gate.branch "${branch}"
done

if [ "${hooks_too}" = 1 ]; then
  echo "Revert gate enabled. Resident guarded: $*; the target branch recorded by each worktree branch is guarded as well."
else
  echo "Resident guarded branches written: $* (no hooks installed; the existing hook system calls .githooks/hook.sh)."
fi
# The gate precheck reads the script only from the local branch of the first resident guarded branch, which
# in a fresh clone may exist only on the remote.
first=$1
for branch in "$@"; do
  if ! git rev-parse --verify -q "refs/heads/${branch}" >/dev/null; then
    [ "${branch}" != "${first}" ] ||
      echo "Note: this clone has no local branch ${branch}, which the gate precheck reads the script from; create it first with git branch ${branch} <remote>/${branch}." >&2
    continue
  fi
  git cat-file -e "refs/heads/${branch}:.githooks/revert-gate.py" 2>/dev/null ||
    echo "Note: ${branch} does not have .githooks/revert-gate.py yet; the gate starts checking once it is committed there." >&2
done
