#!/bin/sh
# Enable staged formatting (pre-commit) and push verification (pre-push) in this clone: sh .githooks/install.sh
# Copies .githooks/hook.sh into the common hooks directory, which every worktree shares. An existing hook
# is overwritten only when it carries this entry point's marker line (or the marker of the earlier revert-gate
# entry point it replaces); anything else is refused, so a hook that belongs to someone else is never lost.
# Migration from the earlier revert-gate entry point: an installed reference-transaction hook that carries
# either marker is removed (Octoboard's built-in revert gate replaces it), and the revert-gate.branch config
# it read is unset. A reference-transaction hook that carries neither marker is left alone.
# Refuses when core.hooksPath is set in a configuration file: that hook system must call
# `sh .githooks/hook.sh <pre-commit|pre-push> "$@"` itself.
# Run once per clone; repeatable.
set -eu

[ $# -eq 0 ] || {
  echo "install.sh takes no arguments (got: $*); --branches-only, --pr-only and resident guarded branches no longer exist, as the revert gate is Octoboard's built-in one. Nothing was installed." >&2
  exit 1
}

root=$(git rev-parse --show-toplevel)
hooks="$(git rev-parse --git-common-dir)/hooks"
marker='^# (octoboard repository hook entry|revert-gate hook entry)'

# Only core.hooksPath in the configuration files (including included ones) counts. Entries from -c (passed down
# through GIT_CONFIG_PARAMETERS) and GIT_CONFIG_COUNT are temporary and apply only to that one process tree
# (for example injected by a tool that launches the agent session, pointing at a dispatcher that forwards to
# this clone's own hooks); judging by them would refuse to install in every such session.
hookspath=$(env -u GIT_CONFIG_COUNT -u GIT_CONFIG_PARAMETERS git config core.hooksPath || true)
if [ -n "${hookspath}" ]; then
  echo "core.hooksPath is set to ${hookspath}, so the common hooks directory will not take effect; call sh .githooks/hook.sh <pre-commit|pre-push> \"\$@\" from within that hook system. Nothing was installed." >&2
  exit 1
fi

# Check everything before writing anything: bailing out halfway would leave half-installed hooks.
for name in pre-commit pre-push; do
  target="${hooks}/${name}"
  if [ -e "${target}" ] && ! grep -Eq "${marker}" "${target}"; then
    echo "${target} already exists and is not this repository's hook entry point; nothing was installed. Call sh .githooks/hook.sh ${name} \"\$@\" from inside it." >&2
    exit 1
  fi
done

mkdir -p "${hooks}"
for name in pre-commit pre-push; do
  target="${hooks}/${name}"
  cp "${root}/.githooks/hook.sh" "${target}"
  chmod +x "${target}"
done

old="${hooks}/reference-transaction"
if [ -e "${old}" ]; then
  if grep -Eq "${marker}" "${old}"; then
    rm -f "${old}"
    echo "Removed the reference-transaction hook of the earlier revert-gate entry point."
  else
    echo "Note: ${old} is not this repository's hook entry point and was left alone." >&2
  fi
fi

if git config --local --get-all revert-gate.branch >/dev/null 2>&1; then
  git config --local --unset-all revert-gate.branch
  echo "Unset the revert-gate.branch git config of the earlier revert-gate entry point."
fi

echo "Repository hooks enabled: staged formatting and push verification."
