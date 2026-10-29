// Remote changes are measured from the exact base commit, including force pushes. The pre-push
// hook supplies the destination's live tip explicitly; manual runs fetch the upstream first.
export function resolveBase(git, baseArg) {
  if (baseArg) {
    const commit = git("rev-parse", "--verify", "--quiet", `${baseArg}^{commit}`);
    if (!commit) return { reason: `base ${baseArg} is not readable` };
    return { commit };
  }
  const branch = git("symbolic-ref", "--quiet", "--short", "HEAD");
  const upstream = (name) => {
    if (!name) return null;
    const remote = git("config", "--get", `branch.${name}.remote`);
    const ref = git("config", "--get", `branch.${name}.merge`);
    return remote && remote !== "." && ref ? { remote, ref } : null;
  };
  const target = branch && git("config", "--get", `branch.${branch}.targetBranch`);
  const source = upstream(branch) ?? upstream(target);
  if (!source) return { reason: "no remote upstream or remote target branch", noTarget: true };
  if (git("fetch", "--no-tags", source.remote, source.ref) == null) return { reason: `cannot fetch ${source.remote} ${source.ref}` };
  const commit = git("rev-parse", "--verify", "FETCH_HEAD^{commit}");
  return commit ? { commit } : { reason: "cannot read fetched remote tip" };
}

// The paths that differ between `base` and `tree` (a tree object, such as `workingTree`'s), or null
// when git cannot say.
export function changedFiles(git, base, tree) {
  const out = git("diff-tree", "-r", "-z", "--name-only", "--no-renames", base, tree);
  return out == null ? null : out.split("\0").filter(Boolean);
}

// Checks read the current checkout, so it must exactly represent the commit being pushed.
export function requirePushedTree(git, head, tree) {
  const pushed = git("rev-parse", "--verify", `${head}^{commit}`);
  if (!pushed || git("rev-parse", "HEAD") !== pushed || git("rev-parse", `${pushed}^{tree}`) !== tree) {
    throw new Error("push checks require the pushed commit checked out with a clean working tree; commit your changes or push from its clean checkout; when one push updates several branches, push each branch separately from a clean checkout of its commit");
  }
}
