// How the repository's scripts run a command, `git` above all. A command answers with trimmed standard
// output, or null when it cannot run or exits non-zero: the callers probe (a ref that may not exist, a remote
// that may be unreachable) and decide what a missing answer means. `gitChecked` is for the ones that cannot go
// on without an answer. The verify scope's rule for this file (scripts/verify/scope.mjs) names the packages
// that import it, so a new importer is added there, and a release script that imports it puts this file in
// the release lint (scripts/verify/steps.mjs).
import { execFileSync, spawnSync } from "node:child_process";

// Runs any command the way `git` does: trimmed standard output, or null when it cannot run or exits
// non-zero. `options.cwd` and `options.env` are those of `child_process.spawnSync`.
export function run(command, args, options = {}) {
  const result = spawnSync(command, args, { ...options, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
  return result.status === 0 ? result.stdout.trim() : null;
}

export const gitWith = (options, ...args) => run("git", args, options);

export const git = (...args) => gitWith({}, ...args);

// Like `git` but throws when `git` fails, and runs in `cwd`.
export function gitChecked(cwd, ...args) {
  return execFileSync("git", args, { cwd, encoding: "utf8" }).trim();
}
