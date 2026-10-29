import { execFileSync, spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const objectId = /^(?:[a-f0-9]{40}|[a-f0-9]{64})$/i;
const deleted = /^0+$/;

export function parseUpdates(input) {
  return input.split(/\r?\n/).filter((line) => line.trim()).map((line) => {
    const fields = line.trim().split(/\s+/);
    if (fields.length !== 4 || !objectId.test(fields[1]) || !objectId.test(fields[3])) {
      throw new Error(`Invalid pre-push update: ${line}`);
    }
    const [localRef, localSha, remoteRef, remoteSha] = fields;
    return { localRef, localSha, remoteRef, remoteSha };
  });
}

function gitCommand(...args) {
  try {
    return execFileSync("git", args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
  } catch {
    return null;
  }
}

function verifyCommand(args) {
  const result = spawnSync("pnpm", ["verify", ...args], { stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error("Push verification failed; fix the checks before pushing.");
}

export function verifyPush(input, remoteName, remoteUrl, { git = gitCommand, verify = verifyCommand } = {}) {
  // Tags are not verified here: the release workflow checks the tagged commit itself.
  const updates = parseUpdates(input).filter((update) => !deleted.test(update.localSha) && !update.remoteRef.startsWith("refs/tags/"));
  if (!updates.length) return;
  // Git supplies the actual destination URL, which may differ from the fetch URL for this remote.
  const remote = remoteUrl || remoteName;
  if (!remote) throw new Error("Cannot verify a push without its remote destination.");
  const seen = new Set();
  const commit = (sha, fetchMissing = false) => {
    let resolved = git("rev-parse", "--verify", `${sha}^{commit}`);
    if (!resolved && fetchMissing) {
      if (git("fetch", "--no-tags", "--no-write-fetch-head", "--", remote, sha) === null) {
        throw new Error(`Cannot fetch remote push base ${sha}; push refused.`);
      }
      resolved = git("rev-parse", "--verify", `${sha}^{commit}`);
    }
    if (!resolved || !objectId.test(resolved)) throw new Error(`Push object ${sha} is not a readable commit.`);
    return resolved;
  };
  for (const update of updates) {
    const head = commit(update.localSha);
    let base;
    if (!deleted.test(update.remoteSha)) {
      base = commit(update.remoteSha, true);
    } else {
      const branch = update.localRef.startsWith("refs/heads/")
        ? update.localRef.slice("refs/heads/".length)
        : update.localRef === "HEAD" ? git("symbolic-ref", "--quiet", "--short", "HEAD") : null;
      const target = branch ? git("config", "--get", `branch.${branch}.targetBranch`) : null;
      const ref = target ? `refs/heads/${target}` : "HEAD";
      const advertised = git("ls-remote", "--", remote, ref);
      const sha = advertised?.split(/\r?\n/).map((line) => line.split(/\s+/))
        .find((fields) => fields[1] === ref)?.[0];
      // The branch may have forked from an older commit of the target, so measure from the merge base:
      // the target's tip alone would count the target's own newer changes as this push's.
      if (sha && objectId.test(sha) && !deleted.test(sha)) {
        const mergeBase = git("merge-base", head, commit(sha, true));
        if (mergeBase && objectId.test(mergeBase)) base = mergeBase;
      }
    }
    const key = `${base || "full"}:${head}`;
    if (seen.has(key)) continue;
    seen.add(key);
    verify(base ? ["--base", base, "--head", head] : ["--full", "--head", head]);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    verifyPush(readFileSync(0, "utf8"), process.argv[2], process.argv[3]);
  } catch (error) {
    console.error(`pre-push: ${error.message}`);
    process.exitCode = 1;
  }
}
