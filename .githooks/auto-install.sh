#!/bin/sh
# Installs the repository hooks on `pnpm install` (the root `prepare` script); `pnpm hooks:install` stays the
# explicit command. It never fails the install: a hook it cannot install is reported as a warning.
#
# The hooks directory is shared by every worktree, and a checkout of an older revision would replace the hooks
# with its older hook.sh, so it installs only from the main working copy with the main branch (the first
# resident guarded branch) checked out. It is skipped in CI and outside this repository's own checkout.
# Resident guarded branches already configured are left as they are (--pr-only installs the hooks alone);
# a clone without them gets the committed .githooks/branches, as `pnpm hooks:install` does.

[ -z "${CI:-}" ] || exit 0
here=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P) || exit 0
[ "$(git rev-parse --show-toplevel 2>/dev/null)" = "${here}" ] || exit 0
[ "$(git rev-parse --path-format=absolute --git-dir)" = "$(git rev-parse --path-format=absolute --git-common-dir)" ] || exit 0

set --
main=$(git config --get-all revert-gate.branch | head -n1)
if [ -n "${main}" ]; then
  set -- --pr-only
  retry="sh .githooks/install.sh --pr-only"
else
  main=$(sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//' -e '/^#/d' -e '/^$/d' "${here}/.githooks/branches" 2>/dev/null | head -n1)
  retry="pnpm hooks:install"
fi
[ -n "${main}" ] && [ "$(git symbolic-ref --quiet --short HEAD)" = "${main}" ] || exit 0

# A hook system that owns core.hooksPath calls hook.sh itself (install.sh --branches-only), so there is
# nothing to install and nothing to warn about.
if env -u GIT_CONFIG_COUNT -u GIT_CONFIG_PARAMETERS git config core.hooksPath >/dev/null; then
  exit 0
fi

if output=$(sh "${here}/.githooks/install.sh" "$@" 2>&1); then
  echo "Repository hooks installed (staged formatting, push verification, revert gate)."
  printf '%s\n' "${output}" | grep '^Note' || true
else
  printf 'warning: repository hooks were not installed; fix the cause and run %s.\n%s\n' "${retry}" "${output}" >&2
fi
exit 0
