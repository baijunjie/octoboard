#!/bin/sh
# Installs the repository hooks on `pnpm install` (the root `prepare` script); `pnpm hooks:install` stays the
# explicit command. It never fails the install: a hook it cannot install is reported as a warning.
#
# The hooks directory is shared by every worktree, and a checkout of an older revision would replace the hooks
# with its older hook.sh, so it installs only from the main working copy with the main branch (the remote's
# default branch, `main` when no remote names one) checked out. It is skipped in CI and outside this
# repository's own checkout.

[ -z "${CI:-}" ] || exit 0
here=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P) || exit 0
[ "$(git rev-parse --show-toplevel 2>/dev/null)" = "${here}" ] || exit 0
[ "$(git rev-parse --path-format=absolute --git-dir)" = "$(git rev-parse --path-format=absolute --git-common-dir)" ] || exit 0

main=
ref=$(git symbolic-ref --quiet --short refs/remotes/origin/HEAD 2>/dev/null)
if [ -z "${ref}" ]; then
  remote=$(git remote | head -n1)
  [ -z "${remote}" ] || ref=$(git symbolic-ref --quiet --short "refs/remotes/${remote}/HEAD" 2>/dev/null)
fi
[ -z "${ref}" ] || main=${ref#*/}
[ -n "${main}" ] || main=main
[ "$(git symbolic-ref --quiet --short HEAD)" = "${main}" ] || exit 0

# A hook system that owns core.hooksPath calls hook.sh itself, so there is nothing to install and nothing to
# warn about.
if env -u GIT_CONFIG_COUNT -u GIT_CONFIG_PARAMETERS git config core.hooksPath >/dev/null; then
  exit 0
fi

if output=$(sh "${here}/.githooks/install.sh" 2>&1); then
  printf '%s\n' "${output}"
else
  printf 'warning: repository hooks were not installed; fix the cause and run pnpm hooks:install.\n%s\n' "${output}" >&2
fi
exit 0
