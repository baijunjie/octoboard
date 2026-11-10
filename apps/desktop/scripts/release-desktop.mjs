#!/usr/bin/env node
// Prepares a version in its own worktree (its version files and a refreshed snapshot of the protocol schema), then
// pushes main and its annotated tag together. The tag starts release CI after local application checks; signing,
// notarization and publishing stay there. Failed preparations are left for inspection.
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
import { gitChecked, gitResult, runResult } from "../../../scripts/lib/git.mjs";
import { windowsToolchainProblems } from "../../../scripts/lib/windows-toolchain.mjs";
import { compareVersions, HOLDER_FLOOR_FILE, parsePlatforms, platformsTrailer, readHolderFloor, tag } from "./release-manifest.mjs";

const VERSION_FILES = ["apps/desktop/package.json", "Cargo.toml", "Cargo.lock"];
// The protocol schema and the snapshot of it as last released, which the daemon's compatibility guard
// (`protocol/compatibility/`) compares the current schema with.
const SCHEMA_DIRECTORY = "contracts/protocol";
const SNAPSHOT_DIRECTORY = `${SCHEMA_DIRECTORY}/released`;
const SNAPSHOT_RECORD = "release.json";
const SCHEMA_SUFFIX = ".schema.json";
const PROTOCOL_GUARD_TEST = "protocol::compatibility::the_schema_only_grows_since_the_last_release";
// A release is only made when the daemon's holder floor (`FLOOR`) names the previous stable release, or the initial
// value while there is none.
const INITIAL_HOLDER_FLOOR = "0.0.0";
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

// Refreshes the snapshot of the protocol schema to what `version` carries: the current schema files, and a record of
// the version and of the protocol generation they state.
export function refreshProtocolSnapshot(root, version) {
  const directory = path.join(root, SCHEMA_DIRECTORY);
  const names = readdirSync(directory, { withFileTypes: true }).filter((entry) => entry.isFile() && entry.name.endsWith(SCHEMA_SUFFIX)).map((entry) => entry.name).sort();
  if (names.length === 0) throw new Error(`${SCHEMA_DIRECTORY} holds no ${SCHEMA_SUFFIX} files to snapshot.`);
  const texts = names.map((name) => readFileSync(path.join(directory, name), "utf8"));
  const generations = new Set(texts.map((text) => JSON.parse(text)["x-octoboard-protocol"]?.generation));
  const [generation] = generations;
  if (generations.size !== 1 || !Number.isInteger(generation)) throw new Error(`The schema files in ${SCHEMA_DIRECTORY} do not state one protocol generation.`);
  const snapshot = path.join(root, SNAPSHOT_DIRECTORY);
  mkdirSync(snapshot, { recursive: true });
  for (const entry of readdirSync(snapshot)) if (entry.endsWith(SCHEMA_SUFFIX)) rmSync(path.join(snapshot, entry));
  names.forEach((name, index) => writeFileSync(path.join(snapshot, name), texts[index]));
  writeFileSync(path.join(snapshot, SNAPSHOT_RECORD), `${JSON.stringify({ version, generation }, null, 2)}\n`);
}

// The versions of the stable releases already tagged, locally or on origin.
function stableTaggedVersions(root) {
  const local = gitChecked(root, "tag", "--list", "desktop-v*").split("\n");
  const remote = gitChecked(root, "ls-remote", "--tags", "--refs", "origin", "refs/tags/desktop-v*").split("\n").map((line) => line.split("\t")[1]?.replace("refs/tags/", ""));
  const stable = /^desktop-v(\d+\.\d+\.\d+)$/;
  return [...new Set([...local, ...remote])].map((name) => stable.exec(name ?? "")?.[1]).filter((found) => found !== undefined);
}

// Why `version` cannot be released with the floor the daemon names, or null when the floor is the greatest stable
// release below `version` (the initial value when there is none). `tagged` lists the stable versions already tagged.
export function holderFloorProblem(root, version, tagged) {
  const floor = readHolderFloor(root);
  if (floor === undefined) return `${HOLDER_FLOOR_FILE} has no FLOOR constant to check.`;
  const below = tagged.filter((found) => compareVersions(found, version) < 0).sort(compareVersions);
  const wanted = below.at(-1) ?? INITIAL_HOLDER_FLOOR;
  if (floor === wanted) return null;
  return `The holder floor in ${HOLDER_FLOOR_FILE} is ${floor}, but the greatest stable release below ${version} is ${wanted}${below.length === 0 ? " (the initial value: none is tagged)" : ""}. Set FLOOR to ${wanted} in its own commit first.`;
}

export function parseOptions(args, cwd = process.cwd()) {
  const { values, positionals } = parseArgs({ args, allowPositionals: true, options: {
    notes: { type: "string" },
    "notes-file": { type: "string" },
    platforms: { type: "string" },
    "dry-run": { type: "boolean", default: false },
  } });
  if (positionals.length !== 1 || (values.notes === undefined) === (values["notes-file"] === undefined)) {
    throw new Error('Usage: pnpm --filter @octoboard/desktop release:version <version> (--notes "text" | --notes-file path) [--platforms macos|windows|macos,windows] [--dry-run]');
  }
  const notes = (values.notes ?? readFileSync(path.resolve(cwd, values["notes-file"]), "utf8")).trim();
  if (!notes) throw new Error("The release notes are empty.");
  return { version: positionals[0], notes, platforms: parsePlatforms(values.platforms), dryRun: values["dry-run"] };
}

export function releaseDesktop({ root, version, notes, platforms = parsePlatforms(), dryRun = false, run = runResult, log = console.log, toolchainProblems = windowsToolchainProblems }) {
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
  const floorProblem = holderFloorProblem(root, version, stableTaggedVersions(root));
  if (floorProblem) throw new Error(floorProblem);
  // The release checks end with the Windows cross-check; a machine that cannot run it is refused before any
  // file or ref changes, rather than after main has been fast-forwarded and every other check has run.
  const problems = toolchainProblems();
  if (problems.length > 0) throw new Error(`The Windows cross-check of the release checks cannot run on this machine:\n${problems.map((problem) => `- ${problem}`).join("\n")}`);
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
  // A version for one platform only says so on the last line of its tag's message, which release CI reads.
  const trailer = platformsTrailer(platforms);
  const message = trailer === null ? `${notes.trim()}\n` : `${notes.trim()}\n\n${trailer}\n`;
  log(`release: ${current} -> ${version}; annotated tag ${releaseTag}; platforms: ${platforms.join(", ")}`);
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
  // A fast-forward can have changed the current version and the floor, so validate both again before creating any
  // release files.
  versionFiles(root, version);
  const floorAfter = holderFloorProblem(root, version, stableTaggedVersions(root));
  if (floorAfter) throw new Error(floorAfter);
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
    // The guard runs against the snapshot of the previous release first: once the snapshot is refreshed it compares the
    // schema with itself, so the commits main carries since the last release are only checked here.
    const guard = command(worktree, "node", ["scripts/cargo.mjs", "test", "--bin", "octoboardd", "--", "--exact", PROTOCOL_GUARD_TEST], { stdio: ["ignore", "pipe", "inherit"] });
    log(guard);
    // Cargo succeeds when a filter matches no test, so a renamed guard would otherwise pass silently.
    const passed = [...(guard ?? "").matchAll(/^test result: ok\. (\d+) passed/gm)].reduce((sum, match) => sum + Number(match[1]), 0);
    if (passed !== 1) throw new Error(`${PROTOCOL_GUARD_TEST} did not run exactly once (${passed} tests passed): the daemon test was renamed or moved, so the protocol compatibility guard did not check the release.`);
    refreshProtocolSnapshot(worktree, version);
    command(worktree, "git", ["add", "--all", "--", ...VERSION_FILES, SNAPSHOT_DIRECTORY]);
    // The version files, and nothing but the snapshot besides them: it is staged whole, so a file it did not change is
    // not listed.
    const staged = gitChecked(worktree, "diff", "--cached", "--name-only").split("\n");
    if (!VERSION_FILES.every((file) => staged.includes(file)) || !staged.every((file) => VERSION_FILES.includes(file) || file.startsWith(`${SNAPSHOT_DIRECTORY}/`))) {
      throw new Error("The staged files differ from the three version files and the protocol snapshot. Inspect the release worktree.");
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
    command(root, "git", ["tag", "-a", releaseTag, releaseCommit, "--cleanup=whitespace", "-F", "-"], { input: message, stdio: ["pipe", "inherit", "inherit"] });
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
