#!/usr/bin/env node
// Prepares a version in its own worktree, then pushes main and its annotated tag together. The tag starts
// release CI after local application checks; signing, notarization and publishing stay there. Failed
// preparations are left for inspection.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
import { gitChecked, gitResult, runResult } from "../../../scripts/lib/git.mjs";
import { compareVersions, tag } from "./release-manifest.mjs";

const VERSION_FILES = ["apps/desktop/package.json", "Cargo.toml", "Cargo.lock"];
const NUMBER = "(?:0|[1-9][0-9]*)";
const IDENTIFIER = `(?:${NUMBER}|[0-9]*[A-Za-z-][0-9A-Za-z-]*)`;
const VERSION = new RegExp(`^${NUMBER}\\.${NUMBER}\\.${NUMBER}(?:-${IDENTIFIER}(?:\\.${IDENTIFIER})*)?$`);

export function versionFiles(root, version) {
  if (!VERSION.test(version)) throw new Error("Give a semantic version without build metadata, such as 0.1.0-beta.2 or 0.1.0.");
  const files = Object.fromEntries(VERSION_FILES.map((file) => [file, readFileSync(path.join(root, file), "utf8")]));
  const current = JSON.parse(files[VERSION_FILES[0]]).version;
  if (compareVersions(version, current) <= 0) throw new Error(`${version} must be newer than ${current}.`);
  const workspace = /(^\[workspace\.package\][\s\S]*?^version\s*=\s*")([^"]+)(")/m;
  if (files["Cargo.toml"].match(workspace)?.[2] !== current) throw new Error("The desktop and Cargo workspace versions disagree.");
  files["apps/desktop/package.json"] = files["apps/desktop/package.json"].replace(/("version"\s*:\s*")[^"]+(")/, `$1${version}$2`);
  files["Cargo.toml"] = files["Cargo.toml"].replace(workspace, (_all, before, _old, after) => `${before}${version}${after}`);
  // Only workspace package records change. `cargo metadata --locked` below verifies the resulting lockfile
  // without resolving external dependencies to newer versions.
  for (const name of ["octoboard", "octoboardd"]) {
    const record = new RegExp(`(\\[\\[package\\]\\]\\nname = "${name}"\\nversion = ")([^"]+)(")`);
    if (files["Cargo.lock"].match(record)?.[2] !== current) throw new Error(`Cargo.lock has no ${current} record for ${name}.`);
    files["Cargo.lock"] = files["Cargo.lock"].replace(record, (_all, before, _old, after) => `${before}${version}${after}`);
  }
  return { current, files };
}

export function parseOptions(args, cwd = process.cwd()) {
  const { values, positionals } = parseArgs({ args, allowPositionals: true, options: {
    notes: { type: "string" },
    "notes-file": { type: "string" },
    "dry-run": { type: "boolean", default: false },
  } });
  if (positionals.length !== 1 || (values.notes === undefined) === (values["notes-file"] === undefined)) {
    throw new Error('Usage: pnpm --filter @octoboard/desktop release:version <version> (--notes "text" | --notes-file path) [--dry-run]');
  }
  const notes = (values.notes ?? readFileSync(path.resolve(cwd, values["notes-file"]), "utf8")).trim();
  if (!notes) throw new Error("The release notes are empty.");
  return { version: positionals[0], notes, dryRun: values["dry-run"] };
}

export function releaseDesktop({ root, version, notes, dryRun = false, run = runResult, log = console.log }) {
  const git = (...args) => gitChecked(root, ...args);
  const cleanMain = () => {
    if (git("branch", "--show-current") !== "main") throw new Error("Run the release from the main worktree on main.");
    if (git("status", "--porcelain")) throw new Error("The main worktree has uncommitted changes. Commit or separate them first.");
  };
  const command = (cwd, name, args, options = {}) => {
    log(`release: ${name} ${args.join(" ")}`);
    const result = run(name, args, { cwd, stdio: "inherit", ...options });
    if (result.status !== 0) throw new Error(`${name} ${args[0]} failed (exit ${result.status}).`);
    return result.stdout;
  };
  cleanMain();
  if (!notes?.trim()) throw new Error("The release notes are empty.");
  const { current } = versionFiles(root, version);
  const releaseTag = tag(version);
  const branch = `release/${releaseTag}`;
  const worktree = path.join(root, ".worktrees", releaseTag);
  if (gitResult({ cwd: root }, "show-ref", "--verify", "--quiet", `refs/tags/${releaseTag}`).status === 0) {
    throw new Error(`${releaseTag} already exists locally. A release tag is never replaced.`);
  }
  const remote = git("ls-remote", "origin", "refs/heads/main", `refs/tags/${releaseTag}`);
  if (remote.split("\n").some((line) => line.endsWith(`refs/tags/${releaseTag}`))) throw new Error(`${releaseTag} already exists on origin.`);
  if (!remote.split("\n").some((line) => line.endsWith("refs/heads/main"))) throw new Error("origin has no main branch.");
  if (existsSync(worktree) || gitResult({ cwd: root }, "show-ref", "--verify", "--quiet", `refs/heads/${branch}`).status === 0) {
    throw new Error(`${branch} or ${worktree} already exists. Inspect the previous attempt before starting another.`);
  }
  if (git("check-ignore", ".worktrees/") !== ".worktrees/") throw new Error("Exclude .worktrees/ in .git/info/exclude before releasing.");
  log(`release: ${current} -> ${version}; annotated tag ${releaseTag}`);
  log(`release: notes:\n${notes.trim()}`);
  if (dryRun) {
    log("release: --dry-run: no files or refs changed. Would fetch main, prepare and check an isolated version commit, fast-forward main, run fresh application release checks, verify the push, and atomically push main and the tag to origin. All checks precede tagging. Release CI then builds and publishes.");
    return;
  }

  command(root, "git", ["fetch", "origin", "main"]);
  // A diverged main is left to its owner. A simple behind-only main can be fast-forwarded without rewriting history.
  const remoteMain = git("rev-parse", "refs/remotes/origin/main");
  if (gitResult({ cwd: root }, "merge-base", "--is-ancestor", remoteMain, "HEAD").status !== 0) {
    command(root, "git", ["merge", "--ff-only", remoteMain]);
  }
  cleanMain();
  // A fast-forward can have changed the current version, so validate again before creating any release files.
  versionFiles(root, version);
  const start = git("rev-parse", "HEAD");
  mkdirSync(path.dirname(worktree), { recursive: true });
  command(root, "git", ["worktree", "add", "-b", branch, worktree, "main"]);
  command(root, "git", ["config", `branch.${branch}.targetBranch`, "main"]);
  let merged = false;
  try {
    const { files } = versionFiles(worktree, version);
    for (const [file, text] of Object.entries(files)) writeFileSync(path.join(worktree, file), text);
    command(worktree, "cargo", ["metadata", "--locked", "--format-version", "1"], { stdio: ["ignore", "pipe", "inherit"] });
    command(worktree, "pnpm", ["install", "--frozen-lockfile"]);
    command(worktree, "git", ["add", "--", ...VERSION_FILES]);
    if (gitChecked(worktree, "diff", "--cached", "--name-only").split("\n").sort().join("\n") !== [...VERSION_FILES].sort().join("\n")) {
      throw new Error("The staged files differ from the three version files. Inspect the release worktree.");
    }
    command(worktree, "git", ["commit", "-m", `chore(desktop): release ${version}`]);
    command(worktree, "git", ["rebase", "main"]);
    command(worktree, "pnpm", ["verify", "--quick"]);
    cleanMain();
    if (git("rev-parse", "HEAD") !== start) throw new Error("main changed during release preparation. Inspect and recheck the release branch before merging.");
    command(root, "git", ["merge", "--ff-only", branch]);
    merged = true;
    const releaseCommit = git("rev-parse", "HEAD");
    command(root, "git", ["worktree", "remove", worktree]);
    command(root, "git", ["branch", "-d", branch]);
    command(root, "pnpm", ["verify", "--release", "--fresh", "--head", releaseCommit]);
    cleanMain();
    if (git("rev-parse", "HEAD") !== releaseCommit) throw new Error("main changed during release verification. Recheck it before tagging or pushing.");
    command(root, "pnpm", ["verify", "--base", remoteMain, "--head", releaseCommit]);
    cleanMain();
    if (git("rev-parse", "HEAD") !== releaseCommit) throw new Error("main changed during push verification. Recheck it before tagging or pushing.");
    command(root, "git", ["tag", "-a", releaseTag, releaseCommit, "--cleanup=whitespace", "-F", "-"], { input: `${notes.trim()}\n`, stdio: ["pipe", "inherit", "inherit"] });
    cleanMain();
    if (git("rev-parse", "HEAD") !== releaseCommit) throw new Error("main changed after tagging. Inspect the local release tag before pushing.");
    command(root, "git", ["push", "--atomic", "origin", `${releaseCommit}:refs/heads/main`, `refs/tags/${releaseTag}`]);
    log(`release: pushed ${releaseTag} after local application checks. CI will build, sign, notarize and publish; inspect the Release workflow before announcing success.`);
  } catch (error) {
    log(merged
      ? `release: the version commit is on main. History was not reset. Inspect the failure before retrying the checks or push for ${releaseTag}.`
      : `release: preparation stopped; inspect ${branch} at ${worktree}. Nothing was reset or deleted.`);
    throw error;
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  try {
    const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
    releaseDesktop({ root, ...parseOptions(process.argv.slice(2), root) });
  } catch (error) {
    console.error(`release: ${error.message}`);
    process.exitCode = 1;
  }
}
