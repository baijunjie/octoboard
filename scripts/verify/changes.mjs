const currentBranch = (git) => git("symbolic-ref", "--quiet", "--short", "HEAD");

const upstreamOf = (git, name) => {
  if (!name) return null;
  const remote = git("config", "--get", `branch.${name}.remote`);
  const ref = git("config", "--get", `branch.${name}.merge`);
  return remote && remote !== "." && ref ? { remote, ref } : null;
};

const remoteName = (git) => {
  const remotes = (git("remote") ?? "").split("\n").filter(Boolean);
  return remotes.includes("origin") ? "origin" : remotes[0];
};

// The local branch `branch` is meant to be merged into, or null: the configured `targetBranch` (an explicit
// setting is never second-guessed), else the local branch it tracks, else the local branch the remote's HEAD
// names (`refs/remotes/origin/HEAD` gives `main`).
function localTargetName(git, branch) {
  const configured = git("config", "--get", `branch.${branch}.targetBranch`);
  if (configured) return configured;
  const merge = git("config", "--get", `branch.${branch}.merge`);
  if (git("config", "--get", `branch.${branch}.remote`) === "." && merge?.startsWith("refs/heads/")) {
    return merge.slice("refs/heads/".length);
  }
  const remote = remoteName(git);
  const head = remote && git("symbolic-ref", "--quiet", `refs/remotes/${remote}/HEAD`);
  const prefix = `refs/remotes/${remote}/`;
  return head?.startsWith(prefix) ? head.slice(prefix.length) : null;
}

// Remote changes are measured from the exact base commit, including force pushes. The pre-push
// hook supplies the destination's live tip explicitly; manual runs fetch the upstream first.
export function resolveBase(git, baseArg) {
  if (baseArg) {
    const commit = git("rev-parse", "--verify", "--quiet", `${baseArg}^{commit}`);
    if (!commit) return { reason: `base ${baseArg} is not readable` };
    return { commit };
  }
  const branch = currentBranch(git);
  const target = branch && git("config", "--get", `branch.${branch}.targetBranch`);
  const source = upstreamOf(git, branch) ?? upstreamOf(git, target);
  if (!source) return { reason: "no remote upstream or remote target branch", noTarget: true };
  if (git("fetch", "--no-tags", source.remote, source.ref) == null) return { reason: `cannot fetch ${source.remote} ${source.ref}` };
  const commit = git("rev-parse", "--verify", "FETCH_HEAD^{commit}");
  return commit ? { commit } : { reason: "cannot read fetched remote tip" };
}

// Where the current branch forked from its local target branch: the merge base of HEAD and that branch's tip,
// with `via` naming it. The target's own commits, pushed or not, are already checked, so only the branch's own
// changes since the fork count. Null when there is no local target, or the branch is the target (its merge
// base with itself is HEAD, which would measure nothing).
export function resolveLocalTargetBase(git) {
  const branch = currentBranch(git);
  const target = branch && localTargetName(git, branch);
  if (!target || target === branch) return null;
  const tip = git("rev-parse", "--verify", "--quiet", `refs/heads/${target}^{commit}`);
  const commit = tip && git("merge-base", "HEAD", tip);
  return commit ? { commit, via: `local branch ${target}` } : null;
}

export const OBJECT_ID = /^(?:[a-f0-9]{40}|[a-f0-9]{64})$/i;

// Where `head` forked from the remote's target, as a new branch is measured at push: the merge base of `head`
// and the live tip of the branch's `targetBranch` (an explicit setting of `branch`), else of the remote's HEAD.
// `remote` is a remote name or a URL. The tip is asked of the remote itself and fetched by its ref name when
// missing, so no remote-tracking ref has to be current. Returns `{ commit, via }`, or `{ reason, noTarget }` when
// no base can be established.
export function remoteTargetBase(git, { branch, remote, head }) {
  const target = branch && git("config", "--get", `branch.${branch}.targetBranch`);
  const ref = target ? `refs/heads/${target}` : "HEAD";
  const advertised = git("ls-remote", "--", remote, ref);
  const sha = advertised?.split(/\r?\n/).map((line) => line.split(/\s+/)).find((fields) => fields[1] === ref)?.[0];
  if (!sha || !OBJECT_ID.test(sha) || /^0+$/.test(sha)) return { reason: `cannot fetch ${remote} ${ref}`, noTarget: true };
  const readTip = () => git("rev-parse", "--verify", `${sha}^{commit}`);
  let tip = readTip();
  if (!tip) {
    // By name, as not every server serves a fetch by object ID; if the branch was rewritten since it was
    // advertised, the advertised commit may still be missing, which counts as not fetched.
    git("fetch", "--no-tags", "--no-write-fetch-head", "--", remote, ref);
    tip = readTip();
    if (!tip) return { reason: `cannot fetch ${remote} ${ref}`, noTarget: true };
  }
  const commit = git("merge-base", head, tip);
  return commit && OBJECT_ID.test(commit) ? { commit, via: `${remote} ${ref}` } : { reason: `no common commit with ${remote} ${ref}`, noTarget: true };
}

// `remoteTargetBase` for the checked-out branch and the repository's own remote.
export function resolveRemoteTargetBase(git) {
  const remote = remoteName(git);
  if (!remote) return { reason: "no remote to measure a new branch from", noTarget: true };
  return remoteTargetBase(git, { branch: currentBranch(git), remote, head: "HEAD" });
}

// The base a run measures from. `--base` is taken as given. A quick run precedes a local merge, so a branch
// without an upstream of its own (none, a local branch, or the remote branch of its target) is measured from
// its local target, then from the upstream of its target, then from the remote's target; one with an upstream
// of its own keeps that upstream. Anything else uses the upstream, of the branch or else of its target.
export function chooseBase(git, { quick = false, baseArg } = {}) {
  if (baseArg || !quick) return resolveBase(git, baseArg);
  const branch = currentBranch(git);
  const own = upstreamOf(git, branch);
  const target = branch && localTargetName(git, branch);
  if (own && !(target && own.ref === `refs/heads/${target}`)) return resolveBase(git);
  const base = resolveLocalTargetBase(git) ?? resolveBase(git);
  return base.noTarget ? resolveRemoteTargetBase(git) : base;
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
