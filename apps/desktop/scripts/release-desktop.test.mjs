import assert from "node:assert/strict";
import { chmodSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync, existsSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { gitChecked, runResult } from "../../../scripts/lib/git.mjs";
import { holderFloorProblem, parseOptions, refreshProtocolSnapshot, releaseDesktop, versionFiles } from "./release-desktop.mjs";
import { readTagMessage } from "./release-manifest.mjs";

import { assertRepositoryEnvironmentCleared } from "../../../scripts/verify/environment.mjs";

assertRepositoryEnvironmentCleared();

function fixture(t) {
  const dir = mkdtempSync(path.join(os.tmpdir(), "desktop-release-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const root = path.join(dir, "work");
  const remote = path.join(dir, "remote.git");
  mkdirSync(path.join(root, "apps/desktop"), { recursive: true });
  const git = (...args) => gitChecked(root, ...args);
  git("init", "-q", "-b", "main");
  git("config", "user.name", "Test");
  git("config", "user.email", "test@example.com");
  writeFileSync(path.join(root, ".git/info/exclude"), ".worktrees/\n");
  writeFileSync(path.join(root, "apps/desktop/package.json"), '{\n  "name": "desktop",\n  "version": "0.1.0-beta.1"\n}\n');
  writeFileSync(path.join(root, "Cargo.toml"), '[workspace]\nresolver = "2"\n[workspace.package]\nversion = "0.1.0-beta.1"\n');
  writeFileSync(path.join(root, "Cargo.lock"), '# Lockfile\nversion = 4\n\n[[package]]\nname = "octoboard"\nversion = "0.1.0-beta.1"\n\n[[package]]\nname = "octoboardd"\nversion = "0.1.0-beta.1"\n\n[[package]]\nname = "external"\nversion = "0.1.0-beta.1"\n');
  mkdirSync(path.join(root, "apps/daemon/src/holder"), { recursive: true });
  writeFileSync(path.join(root, "apps/daemon/src/holder/compat.rs"), 'pub const FLOOR: &str = "0.0.0";\n');
  // The protocol schema of generation 3, and the snapshot of the last release, which carried generation 2 and no schema.
  mkdirSync(path.join(root, "contracts/protocol/released"), { recursive: true });
  for (const name of ["control-input", "terminal-output"]) writeFileSync(path.join(root, `contracts/protocol/${name}.schema.json`), `${JSON.stringify({ "x-octoboard-protocol": { generation: 3 }, name })}\n`);
  writeFileSync(path.join(root, "contracts/protocol/released/release.json"), '{\n  "version": "0.1.0-beta.1",\n  "generation": 2\n}\n');
  git("add", ".");
  git("commit", "-qm", "base");
  git("init", "--bare", "-q", "-b", "main", remote);
  git("remote", "add", "origin", remote);
  // Seed the remote without running a push: the release itself exercises the push and its hooks.
  git("--git-dir", remote, "fetch", "-q", root, "main:main");
  const calls = [];
  const options = { root, version: "0.1.0-beta.2", notes: "## Fixes\n\n- Keep # headings.\n", log: () => {}, toolchainProblems: () => [], run: (name, args, config) => {
    calls.push({ name, args, cwd: config.cwd });
    if (name === "node" && args.includes("--exact")) return { status: 0, stdout: "test result: ok. 0 passed; 0 failed\ntest result: ok. 1 passed; 0 failed\n" };
    if (name !== "git") return { status: 0 };
    return runResult(name, args, { ...config, stdio: ["pipe", "pipe", "pipe"] });
  } };
  return { root, remote, git, calls, options };
}

test("updates both version sources and only the workspace records in the lockfile", (t) => {
  const { root } = fixture(t);
  const { current, files } = versionFiles(root, "0.1.0-beta.2");
  assert.equal(current, "0.1.0-beta.1");
  assert.equal(JSON.parse(files["apps/desktop/package.json"]).version, "0.1.0-beta.2");
  assert.match(files["Cargo.toml"], /version = "0.1.0-beta.2"/);
  assert.equal(files["Cargo.lock"].split("0.1.0-beta.2").length - 1, 2);
  assert.match(files["Cargo.lock"], /name = "external"\nversion = "0.1.0-beta.1"/);
  assert.match(readFileSync(path.join(root, "Cargo.toml"), "utf8"), /beta\.1/);
});

test("refreshes the protocol snapshot to the current schema files and their generation, dropping stale ones", (t) => {
  const { root } = fixture(t);
  const snapshot = path.join(root, "contracts/protocol/released");
  writeFileSync(path.join(snapshot, "stale.schema.json"), "{}\n");
  refreshProtocolSnapshot(root, "0.1.0-beta.2");
  assert.deepEqual(readdirSync(snapshot).sort(), ["control-input.schema.json", "release.json", "terminal-output.schema.json"]);
  assert.equal(readFileSync(path.join(snapshot, "control-input.schema.json"), "utf8"), readFileSync(path.join(root, "contracts/protocol/control-input.schema.json"), "utf8"));
  assert.deepEqual(JSON.parse(readFileSync(path.join(snapshot, "release.json"), "utf8")), { version: "0.1.0-beta.2", generation: 3 });
});

test("refuses to snapshot schema files that state no single generation, or none", (t) => {
  const { root } = fixture(t);
  writeFileSync(path.join(root, "contracts/protocol/terminal-output.schema.json"), '{"x-octoboard-protocol":{"generation":4}}\n');
  assert.throws(() => refreshProtocolSnapshot(root, "0.1.0-beta.2"), /one protocol generation/);
  rmSync(path.join(root, "contracts/protocol/control-input.schema.json"));
  rmSync(path.join(root, "contracts/protocol/terminal-output.schema.json"));
  assert.throws(() => refreshProtocolSnapshot(root, "0.1.0-beta.2"), /no \.schema\.json files/);
});

test("snapshots only the schema files directly in the schema directory, creating the snapshot directory when it is absent", (t) => {
  const { root } = fixture(t);
  const directory = path.join(root, "contracts/protocol");
  rmSync(path.join(directory, "released"), { recursive: true });
  writeFileSync(path.join(directory, "notes.json"), "{}\n");
  mkdirSync(path.join(directory, "nested.schema.json"));
  mkdirSync(path.join(directory, "sub"));
  writeFileSync(path.join(directory, "sub/inner.schema.json"), '{"x-octoboard-protocol":{"generation":9}}\n');
  refreshProtocolSnapshot(root, "0.1.0-beta.2");
  assert.deepEqual(readdirSync(path.join(directory, "released")).sort(), ["control-input.schema.json", "release.json", "terminal-output.schema.json"]);
  assert.deepEqual(JSON.parse(readFileSync(path.join(directory, "released/release.json"), "utf8")), { version: "0.1.0-beta.2", generation: 3 });
});

test("refuses a schema file that states no generation, and leaves the snapshot as it was", (t) => {
  const { root } = fixture(t);
  const snapshot = path.join(root, "contracts/protocol/released");
  const record = readFileSync(path.join(snapshot, "release.json"), "utf8");
  writeFileSync(path.join(root, "contracts/protocol/terminal-output.schema.json"), '{"name":"terminal-output"}\n');
  assert.throws(() => refreshProtocolSnapshot(root, "0.1.0-beta.2"), /one protocol generation/);
  assert.deepEqual(readdirSync(snapshot), ["release.json"]);
  assert.equal(readFileSync(path.join(snapshot, "release.json"), "utf8"), record);
});

test("writes the record as readable JSON ending in a line break", (t) => {
  const { root } = fixture(t);
  refreshProtocolSnapshot(root, "0.1.0-beta.2");
  const text = readFileSync(path.join(root, "contracts/protocol/released/release.json"), "utf8");
  assert.ok(text.endsWith("}\n"));
  assert.deepEqual(JSON.parse(text), { version: "0.1.0-beta.2", generation: 3 });
});

test("rejects nonincreasing, invalid and metadata-only versions before writing", (t) => {
  const { root } = fixture(t);
  for (const version of ["0.1.0-beta.1", "0.0.9", "01.1.0", "0.1.0-beta.02", "0.1.0-beta..2", "0.1.0+build.2", "v0.1.0", "--force"]) {
    assert.throws(() => versionFiles(root, version), Error, version);
  }
  for (const version of ["0.1.0", "0.1.0-beta.10", "0.1.0-rc.1", "1.0.0-alpha-1"]) assert.doesNotThrow(() => versionFiles(root, version));
});

test("refuses inconsistent source and lock versions", (t) => {
  const { root } = fixture(t);
  const cargo = path.join(root, "Cargo.toml");
  const original = readFileSync(cargo, "utf8");
  writeFileSync(cargo, original.replace("beta.1", "beta.0"));
  assert.throws(() => versionFiles(root, "0.1.0-beta.2"), /disagree/);
  writeFileSync(cargo, original);
  const lock = path.join(root, "Cargo.lock");
  writeFileSync(lock, readFileSync(lock, "utf8").replace('name = "octoboard"', 'name = "other"'));
  assert.throws(() => versionFiles(root, "0.1.0-beta.2"), /Cargo.lock/);
});

test("requires one explicit version and one nonempty source of release notes", (t) => {
  const { root } = fixture(t);
  writeFileSync(path.join(root, "notes.md"), "## Changes\n\nA fix.\n");
  assert.deepEqual(parseOptions(["0.1.0-beta.2", "--notes-file", "notes.md", "--dry-run"], root), {
    version: "0.1.0-beta.2", notes: "## Changes\n\nA fix.", platforms: ["macos", "windows", "linux"], dryRun: true,
  });
  assert.deepEqual(parseOptions(["0.1.0", "--notes", "a", "--platforms", "windows"], root).platforms, ["windows"]);
  assert.deepEqual(parseOptions(["0.1.0", "--notes", "a", "--platforms", "linux"], root).platforms, ["linux"]);
  for (const args of [[], ["0.1.0"], ["0.1.0", "--notes", ""], ["0.1.0", "--notes", "a", "--notes-file", "notes.md"], ["0.1.0", "0.2.0", "--notes", "a"], ["0.1.0", "--notes", "a", "--platforms", "freebsd"]]) {
    assert.throws(() => parseOptions(args, root));
  }
});

test("dry-run leaves the files, refs and remote unchanged", (t) => {
  const { git, calls, options } = fixture(t);
  const before = git("show-ref");
  releaseDesktop({ ...options, dryRun: true });
  assert.deepEqual(calls, []);
  assert.equal(git("show-ref"), before);
  assert.equal(git("status", "--porcelain"), "");
});

test("publishes one version commit and an annotated tag atomically, keeping hooks and Markdown notes", (t) => {
  const { root, remote, git, calls, options } = fixture(t);
  releaseDesktop(options);
  const releaseTag = "desktop-v0.1.0-beta.2";
  assert.equal(git("rev-list", "--count", "HEAD"), "2");
  // The one version commit carries the refreshed protocol snapshot with the version files.
  assert.deepEqual(git("show", "--name-only", "--format=", "HEAD").split("\n").sort(), [
    "Cargo.lock", "Cargo.toml", "apps/desktop/package.json", "contracts/protocol/released/control-input.schema.json", "contracts/protocol/released/release.json", "contracts/protocol/released/terminal-output.schema.json",
  ]);
  assert.deepEqual(JSON.parse(git("show", "HEAD:contracts/protocol/released/release.json")), { version: options.version, generation: 3 });
  assert.equal(git("cat-file", "-t", releaseTag), "tag");
  assert.equal(git("tag", "-l", "--format=%(contents)", releaseTag), options.notes.trim());
  assert.equal(git("--git-dir", remote, "rev-parse", "main"), git("rev-parse", "HEAD"));
  assert.equal(git("--git-dir", remote, "rev-parse", `${releaseTag}^{}`), git("rev-parse", "HEAD"));
  assert.equal(git("status", "--porcelain"), "");
  assert.equal(existsSync(path.join(root, ".worktrees", releaseTag)), false);
  assert.equal(git("branch", "--list", `release/${releaseTag}`), "");
  assert.deepEqual(calls.at(-1).args, ["push", "--atomic", "origin", `${git("rev-parse", "HEAD")}:refs/heads/main`, `refs/tags/${releaseTag}`]);
  assert.ok(calls.some(({ args }) => args.join(" ") === "metadata --locked --format-version 1"));
  assert.ok(calls.some(({ args }) => args.join(" ") === "verify --quick"));
  const releaseCheck = calls.findIndex(({ name, args }) => name === "pnpm" && args.includes("--release"));
  assert.deepEqual(calls[releaseCheck], {
    name: "pnpm", args: ["verify", "--release", "--fresh", "--head", git("rev-parse", "HEAD")], cwd: root,
  });
  const pushCheck = calls.findIndex(({ name, args }) => name === "pnpm" && args.includes("--base"));
  const tagging = calls.findIndex(({ name, args }) => name === "git" && args[0] === "tag");
  assert.ok(releaseCheck < pushCheck && pushCheck < tagging);
  assert.ok(calls.every(({ args }) => !args.includes("--no-verify") && !args.includes("--force")));
});

test("a version for one platform ends its tag message with the platforms line, which reads back", (t) => {
  const { git, options } = fixture(t);
  releaseDesktop({ ...options, platforms: ["macos"] });
  const message = git("tag", "-l", "--format=%(contents)", "desktop-v0.1.0-beta.2");
  assert.equal(message, `${options.notes.trim()}\n\nPlatforms: macos`);
  assert.deepEqual(readTagMessage(message), { notes: options.notes.trim(), platforms: ["macos"] });
});

test("the holder floor must be the greatest stable release below the new version, or the initial value when none is tagged", (t) => {
  const { root } = fixture(t);
  assert.equal(holderFloorProblem(root, "0.1.0-beta.2", []), null);
  assert.equal(holderFloorProblem(root, "0.1.0-beta.2", ["0.1.0", "0.2.0"]), null);
  assert.match(holderFloorProblem(root, "0.2.0-beta.1", ["0.1.0"]), /FLOOR to 0\.1\.0/);
  assert.match(holderFloorProblem(root, "0.3.0", ["0.1.0", "0.2.0"]), /greatest stable release below 0\.3\.0 is 0\.2\.0/);
  writeFileSync(path.join(root, "apps/daemon/src/holder/compat.rs"), 'pub const FLOOR: &str = "0.1.0";\n');
  assert.equal(holderFloorProblem(root, "0.2.0-beta.1", ["0.1.0", "0.2.0"]), null);
  assert.match(holderFloorProblem(root, "0.1.0-beta.2", []), /initial value/);
});

test("a release whose holder floor is stale is refused before any file, ref or command", (t) => {
  const { git, calls, options } = fixture(t);
  git("tag", "desktop-v0.0.5");
  const before = git("show-ref");
  assert.throws(() => releaseDesktop(options), /holder floor[\s\S]*0\.0\.5/);
  assert.deepEqual(calls, []);
  assert.equal(git("show-ref"), before);
});

test("a machine that cannot run the Windows cross-check is refused before any file, ref or command, dry run included", (t) => {
  const { git, calls, options } = fixture(t);
  const before = git("show-ref");
  const toolchainProblems = () => ["cargo-xwin is not installed: run `cargo install --locked cargo-xwin`."];
  for (const dryRun of [false, true]) {
    assert.throws(() => releaseDesktop({ ...options, dryRun, toolchainProblems }), /Windows cross-check[\s\S]*cargo install --locked cargo-xwin/);
  }
  assert.deepEqual(calls, []);
  assert.equal(git("show-ref"), before);
  assert.equal(git("status", "--porcelain"), "");
});

test("dirty worktrees and existing local or remote tags are refused without creating a branch", (t) => {
  const { root, git, calls, options } = fixture(t);
  writeFileSync(path.join(root, "uncommitted"), "keep");
  assert.throws(() => releaseDesktop(options), /uncommitted/);
  rmSync(path.join(root, "uncommitted"));
  git("tag", "desktop-v0.1.0-beta.2");
  assert.throws(() => releaseDesktop(options), /already exists locally/);
  git("push", "origin", "refs/tags/desktop-v0.1.0-beta.2");
  git("tag", "-d", "desktop-v0.1.0-beta.2");
  assert.throws(() => releaseDesktop(options), /already exists on origin/);
  assert.deepEqual(calls, []);
});

const isGuard = (name, args) => name === "node" && args.includes("--bin") && args.at(-1).startsWith("protocol::compatibility::");

test("the protocol guard runs against the previous release's snapshot, before it is refreshed", (t) => {
  const { root, options } = fixture(t);
  const worktree = path.join(root, ".worktrees/desktop-v0.1.0-beta.2");
  let seen;
  const run = options.run;
  releaseDesktop({ ...options, run: (name, args, config) => {
    if (isGuard(name, args)) {
      assert.equal(config.cwd, worktree);
      seen = JSON.parse(readFileSync(path.join(worktree, "contracts/protocol/released/release.json"), "utf8"));
    }
    return run(name, args, config);
  } });
  assert.deepEqual(seen, { version: "0.1.0-beta.1", generation: 2 });
});

test("a protocol break fails before the version commit is made, leaving the snapshot as it was", (t) => {
  const { root, remote, git, options } = fixture(t);
  const before = git("rev-parse", "HEAD");
  const run = options.run;
  assert.throws(() => releaseDesktop({ ...options, run: (name, args, config) => (isGuard(name, args) ? { status: 1 } : run(name, args, config)) }), /node scripts\/cargo\.mjs failed/);
  const worktree = path.join(root, ".worktrees/desktop-v0.1.0-beta.2");
  assert.equal(gitChecked(worktree, "rev-parse", "HEAD"), before);
  assert.deepEqual(JSON.parse(readFileSync(path.join(worktree, "contracts/protocol/released/release.json"), "utf8")), { version: "0.1.0-beta.1", generation: 2 });
  assert.equal(git("rev-parse", "HEAD"), before);
  assert.equal(git("--git-dir", remote, "rev-parse", "main"), before);
  assert.equal(git("tag", "--list", "desktop-v0.1.0-beta.2"), "");
});

test("a guard that matches no test fails the release before the version commit", (t) => {
  const { root, git, options } = fixture(t);
  const before = git("rev-parse", "HEAD");
  const run = options.run;
  assert.throws(() => releaseDesktop({ ...options, run: (name, args, config) => (isGuard(name, args) ? { status: 0, stdout: "test result: ok. 0 passed; 0 failed\n" } : run(name, args, config)) }), /did not run exactly once \(0 tests passed\)/);
  assert.equal(gitChecked(path.join(root, ".worktrees/desktop-v0.1.0-beta.2"), "rev-parse", "HEAD"), before);
  assert.equal(git("tag", "--list", "desktop-v0.1.0-beta.2"), "");
});

test("a guard run that reports more than one passed test fails the release before the version commit", (t) => {
  const { root, git, options } = fixture(t);
  const before = git("rev-parse", "HEAD");
  const run = options.run;
  assert.throws(() => releaseDesktop({ ...options, run: (name, args, config) => (isGuard(name, args) ? { status: 0, stdout: "test result: ok. 2 passed; 0 failed\n" } : run(name, args, config)) }), /did not run exactly once \(2 tests passed\)/);
  assert.equal(gitChecked(path.join(root, ".worktrees/desktop-v0.1.0-beta.2"), "rev-parse", "HEAD"), before);
  assert.equal(git("tag", "--list", "desktop-v0.1.0-beta.2"), "");
});

test("a guard run that reports nothing at all fails the release", (t) => {
  const { options } = fixture(t);
  const run = options.run;
  assert.throws(() => releaseDesktop({ ...options, run: (name, args, config) => (isGuard(name, args) ? { status: 0 } : run(name, args, config)) }), /did not run exactly once \(0 tests passed\)/);
});

test("the guard runs the one test by its exact name", (t) => {
  const { options } = fixture(t);
  let guard;
  const run = options.run;
  releaseDesktop({ ...options, run: (name, args, config) => {
    if (isGuard(name, args)) guard = args;
    return run(name, args, config);
  } });
  assert.ok(guard.includes("--exact"), guard.join(" "));
  assert.equal(guard.at(-1), "protocol::compatibility::the_schema_only_grows_since_the_last_release");
});

test("the guard test the release names exists at its module path in the daemon", () => {
  const [, modulePath] = /PROTOCOL_GUARD_TEST = "protocol::([^"]+)"/.exec(readFileSync(new URL("./release-desktop.mjs", import.meta.url), "utf8"));
  const [module, name] = modulePath.split("::");
  const directory = new URL(`../../daemon/src/protocol/${module}/`, import.meta.url);
  const source = readFileSync(new URL("mod.rs", directory), "utf8");
  assert.match(source, new RegExp(String.raw`#\[test\]\s*fn ${name}\(`));
});

test("a failed quick check retains the release branch and leaves main and the remote unchanged", (t) => {
  const { root, remote, git, options } = fixture(t);
  const before = git("rev-parse", "HEAD");
  const run = options.run;
  assert.throws(() => releaseDesktop({ ...options, run: (name, args, config) =>
    name === "pnpm" && args.includes("--quick") ? { status: 1 } : run(name, args, config),
  }), /failed/);
  assert.equal(git("rev-parse", "HEAD"), before);
  assert.equal(git("--git-dir", remote, "rev-parse", "main"), before);
  assert.ok(existsSync(path.join(root, ".worktrees/desktop-v0.1.0-beta.2")));
  assert.equal(git("tag", "--list", "desktop-v0.1.0-beta.2"), "");
});

test("a failed full check leaves a local version commit without pushing or creating a tag", (t) => {
  const { remote, git, options } = fixture(t);
  const before = git("rev-parse", "HEAD");
  const run = options.run;
  assert.throws(() => releaseDesktop({ ...options, run: (name, args, config) =>
    name === "pnpm" && args.includes("--base") ? { status: 1 } : run(name, args, config),
  }), /failed/);
  assert.notEqual(git("rev-parse", "HEAD"), before);
  assert.equal(git("--git-dir", remote, "rev-parse", "main"), before);
  assert.equal(git("tag", "--list", "desktop-v0.1.0-beta.2"), "");
});

test("failed application release checks prevent tagging and leave both remote refs unchanged", (t) => {
  const { root, remote, git, calls, options } = fixture(t);
  const before = git("rev-parse", "HEAD");
  const run = options.run;
  assert.throws(() => releaseDesktop({ ...options, run: (name, args, config) => {
    const result = run(name, args, config);
    return name === "pnpm" && args.includes("--release") ? { status: 1 } : result;
  } }), /pnpm verify failed/);
  assert.notEqual(git("rev-parse", "HEAD"), before);
  assert.equal(JSON.parse(readFileSync(path.join(root, "apps/desktop/package.json"), "utf8")).version, options.version);
  assert.equal(git("--git-dir", remote, "rev-parse", "main"), before);
  assert.equal(git("--git-dir", remote, "tag", "--list"), "");
  assert.equal(git("tag", "--list", "desktop-v0.1.0-beta.2"), "");
  assert.ok(calls.some(({ args }) => args.includes("--release") && args.includes("--fresh")));
  assert.ok(calls.every(({ name, args }) => name !== "git" || !["tag", "push"].includes(args[0])));
});

test("a commit hook refusal stops preparation and keeps the version changes for inspection", (t) => {
  const { root, git, remote, options } = fixture(t);
  const before = git("rev-parse", "HEAD");
  const hook = path.join(root, ".git/hooks/pre-commit");
  writeFileSync(hook, "#!/bin/sh\nexit 1\n");
  chmodSync(hook, 0o755);
  assert.throws(() => releaseDesktop(options), /git commit failed/);
  assert.equal(git("rev-parse", "HEAD"), before);
  assert.equal(git("--git-dir", remote, "rev-parse", "main"), before);
  assert.match(readFileSync(path.join(root, ".worktrees/desktop-v0.1.0-beta.2/Cargo.toml"), "utf8"), /beta\.2/);
});

test("a remote tag refusal leaves both remote refs unchanged because the push is atomic", (t) => {
  const { remote, git, options } = fixture(t);
  const before = git("rev-parse", "HEAD");
  const hook = path.join(remote, "hooks/update");
  writeFileSync(hook, '#!/bin/sh\ncase "$1" in refs/tags/*) exit 1 ;; esac\n');
  chmodSync(hook, 0o755);
  assert.throws(() => releaseDesktop(options), /git push failed/);
  assert.equal(git("--git-dir", remote, "rev-parse", "main"), before);
  assert.equal(git("--git-dir", remote, "tag", "--list"), "");
  assert.equal(git("cat-file", "-t", "desktop-v0.1.0-beta.2"), "tag");
});

test("the release commit drops a stale snapshot file along with the version files", (t) => {
  const { root, git, options } = fixture(t);
  writeFileSync(path.join(root, "contracts/protocol/released/stale.schema.json"), "{}\n");
  git("add", ".");
  git("commit", "-qm", "stale snapshot file");
  releaseDesktop(options);
  assert.equal(gitChecked(root, "ls-tree", "--name-only", "HEAD", "contracts/protocol/released/").split("\n").sort().join(","), [
    "contracts/protocol/released/control-input.schema.json", "contracts/protocol/released/release.json", "contracts/protocol/released/terminal-output.schema.json",
  ].join(","));
});

test("a file staged beyond the version files and the snapshot refuses the release commit", (t) => {
  const { git, options } = fixture(t);
  const run = options.run;
  const before = git("rev-parse", "HEAD");
  assert.throws(() => releaseDesktop({ ...options, run: (name, args, config) => {
    const result = run(name, args, config);
    if (name === "pnpm" && args[0] === "install") {
      writeFileSync(path.join(config.cwd, "stray.txt"), "not part of a release");
      gitChecked(config.cwd, "add", "stray.txt");
    }
    return result;
  } }), /staged files differ/);
  assert.equal(git("rev-parse", "HEAD"), before);
});

test("a concurrent main commit during verification prevents tagging and pushing", (t) => {
  const { root, remote, git, options } = fixture(t);
  const before = git("rev-parse", "HEAD");
  const run = options.run;
  assert.throws(() => releaseDesktop({ ...options, run: (name, args, config) => {
    if (name === "pnpm" && args.includes("--base")) {
      writeFileSync(path.join(root, "other"), "another task");
      git("add", "other");
      git("commit", "-qm", "other task");
    }
    return run(name, args, config);
  } }), /main changed during push verification/);
  assert.equal(git("--git-dir", remote, "rev-parse", "main"), before);
  assert.equal(git("tag", "--list", "desktop-v0.1.0-beta.2"), "");
});

test("a concurrent main commit after tagging leaves the tag on the checked commit and stops the push", (t) => {
  const { root, remote, git, options } = fixture(t);
  const before = git("rev-parse", "HEAD");
  const run = options.run;
  let checked;
  assert.throws(() => releaseDesktop({ ...options, run: (name, args, config) => {
    const result = run(name, args, config);
    if (name === "git" && args[0] === "tag") {
      checked = git("rev-parse", "HEAD");
      writeFileSync(path.join(root, "other"), "another task");
      git("add", "other");
      git("commit", "-qm", "other task");
    }
    return result;
  } }), /main changed after tagging/);
  assert.equal(git("rev-parse", "desktop-v0.1.0-beta.2^{}"), checked);
  assert.notEqual(git("rev-parse", "HEAD"), checked);
  assert.equal(git("--git-dir", remote, "rev-parse", "main"), before);
  assert.equal(git("--git-dir", remote, "tag", "--list"), "");
});

test("a concurrent main commit during application release checks prevents tagging and pushing", (t) => {
  const { root, remote, git, options } = fixture(t);
  const before = git("rev-parse", "HEAD");
  const run = options.run;
  assert.throws(() => releaseDesktop({ ...options, run: (name, args, config) => {
    if (name === "pnpm" && args.includes("--release")) {
      writeFileSync(path.join(root, "other"), "another task");
      git("add", "other");
      git("commit", "-qm", "other task");
    }
    return run(name, args, config);
  } }), /main changed during release verification/);
  assert.equal(git("--git-dir", remote, "rev-parse", "main"), before);
  assert.equal(git("tag", "--list", "desktop-v0.1.0-beta.2"), "");
});
