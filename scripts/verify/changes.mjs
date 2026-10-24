// What a branch changed, for the commands that decide what to run from it (`pnpm verify`, the gallery
// sweep's choice of languages). The changes are those since the branch's recorded fork point
// (`branch.<name>.forkPoint`), so what a rebase or merge brought in counts as changed too.
//
// `git` is a function taking git's arguments and returning its trimmed output, or null when git
// fails; each caller owns how it runs git.

// The commit the changes are taken from: `baseArg` if given, else the checked-out branch's recorded
// fork point. `{ reason }` says why there is none: no fork point (the target branch itself and a
// detached HEAD included, which also sets `noForkPoint`), one git cannot read, or one that is not an
// ancestor of HEAD.
export function resolveBase(git, baseArg) {
  const branch = git("symbolic-ref", "--quiet", "--short", "HEAD");
  const base = baseArg ?? (branch ? git("config", "--get", `branch.${branch}.forkPoint`) : null);
  if (!base) return { reason: "no recorded fork point", noForkPoint: true };
  const commit = git("rev-parse", "--verify", "--quiet", `${base}^{commit}`);
  if (!commit) return { reason: `base ${base} is not readable` };
  if (git("merge-base", "--is-ancestor", commit, "HEAD") == null) return { reason: `base ${base} is not an ancestor of HEAD` };
  return { commit };
}

// The paths that differ between `base` and `tree` (a tree object, such as `workingTree`'s), or null
// when git cannot say.
export function changedFiles(git, base, tree) {
  const out = git("diff-tree", "-r", "-z", "--name-only", "--no-renames", base, tree);
  return out == null ? null : out.split("\0").filter(Boolean);
}
