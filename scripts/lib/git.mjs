// How the repository's scripts run a command, `git` above all. A command answers with trimmed standard
// output, or null when it cannot run or exits non-zero: the callers probe (a ref that may not exist, a remote
// that may be unreachable) and decide what a missing answer means. `gitChecked` and `gitCheckedWith` are for
// the ones that cannot go on without an answer, `gitResult` for the ones that read the exit status. The verify
// scope's rule for this file (scripts/verify/scope.mjs) names the packages that import it, so a new importer is
// added there, and a release script that imports it puts this file in the release lint
// (scripts/verify/steps.mjs).
import { execFileSync, spawnSync } from "node:child_process";

// Runs any command the way `git` does: trimmed standard output, or null when it cannot run or exits
// non-zero. `options.cwd` and `options.env` are those of `child_process.spawnSync`.
export function run(command, args, options = {}) {
  const result = spawnSync(command, args, { ...options, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
  return result.status === 0 ? result.stdout.trim() : null;
}

export const gitWith = (options, ...args) => run("git", args, options);

export const git = (...args) => gitWith({}, ...args);

// Runs any command and returns the whole `spawnSync` result (`status`, `stdout`, `stderr`), throwing only when
// it cannot start. `options` are those of `spawnSync`; `input` is fed to standard input.
export function runResult(command, args, options = {}) {
  const result = spawnSync(command, args, { maxBuffer: 64 * 1024 * 1024, ...options, encoding: "utf8" });
  if (result.error) throw result.error;
  return result;
}

export const gitResult = (options, ...args) => runResult("git", args, options);

// Like `git` but throws when `git` fails, with its standard error in the message, and returns the output
// as it is (a blob keeps its trailing newline). `options` are those of `child_process.execFileSync`.
export function gitCheckedWith(options, ...args) {
  return execFileSync("git", args, { maxBuffer: 64 * 1024 * 1024, ...options, encoding: "utf8", stdio: ["pipe", "pipe", "pipe"] });
}

// Like `git` but throws when `git` fails, and runs in `cwd`.
export const gitChecked = (cwd, ...args) => gitCheckedWith({ cwd }, ...args).trim();
