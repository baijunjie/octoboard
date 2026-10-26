// What a branch changed, for the commands that decide what to run from it (`pnpm verify`, the gallery
// sweep's choice of languages and whether it loads anything). The changes are those since the
// branch's current merge base with its recorded target branch (`branch.<name>.targetBranch`): the
// target branch is taken as already checked, so what a rebase or merge brought in from it is not
// counted, while the branch's own changes, conflict resolutions included, are.
//
// `git` is a function taking git's arguments and returning its trimmed output, or null when git
// fails; each caller owns how it runs git.

// The commit the changes are taken from: `baseArg` if given, else the merge base of HEAD and the
// checked-out branch's recorded target branch. `{ reason }` says why there is none: for `baseArg`, a
// base git cannot read or one that is not an ancestor of HEAD; otherwise no target branch recorded
// (the target branch itself and a detached HEAD included, which also sets `noTarget`), or no merge
// base with it (a target branch since deleted, or unrelated history).
export function resolveBase(git, baseArg) {
  if (baseArg) {
    const commit = git("rev-parse", "--verify", "--quiet", `${baseArg}^{commit}`);
    if (!commit) return { reason: `base ${baseArg} is not readable` };
    if (git("merge-base", "--is-ancestor", commit, "HEAD") == null) return { reason: `base ${baseArg} is not an ancestor of HEAD` };
    return { commit };
  }
  const branch = git("symbolic-ref", "--quiet", "--short", "HEAD");
  const target = branch ? git("config", "--get", `branch.${branch}.targetBranch`) : null;
  if (!target) return { reason: "no recorded target branch", noTarget: true };
  const commit = git("merge-base", "HEAD", `refs/heads/${target}`);
  if (!commit) return { reason: `no merge base with the target branch ${target}` };
  return { commit };
}

// The paths that differ between `base` and `tree` (a tree object, such as `workingTree`'s), or null
// when git cannot say.
export function changedFiles(git, base, tree) {
  const out = git("diff-tree", "-r", "-z", "--name-only", "--no-renames", base, tree);
  return out == null ? null : out.split("\0").filter(Boolean);
}
