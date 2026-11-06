import assert from "node:assert/strict";
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync, existsSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { gitChecked, runResult } from "../../../scripts/lib/git.mjs";
import { parseOptions, releaseDesktop, versionFiles } from "./release-desktop.mjs";

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
  git("add", ".");
  git("commit", "-qm", "base");
  git("init", "--bare", "-q", "-b", "main", remote);
  git("remote", "add", "origin", remote);
  // Seed the remote without running a push: the release itself exercises the push and its hooks.
  git("--git-dir", remote, "fetch", "-q", root, "main:main");
  const calls = [];
  const options = { root, version: "0.1.0-beta.2", notes: "## Fixes\n\n- Keep # headings.\n", log: () => {}, run: (name, args, config) => {
    calls.push({ name, args, cwd: config.cwd });
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
    version: "0.1.0-beta.2", notes: "## Changes\n\nA fix.", dryRun: true,
  });
  for (const args of [[], ["0.1.0"], ["0.1.0", "--notes", ""], ["0.1.0", "--notes", "a", "--notes-file", "notes.md"], ["0.1.0", "0.2.0", "--notes", "a"]]) {
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
