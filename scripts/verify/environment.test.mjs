import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { assertRepositoryEnvironmentCleared, withoutRepositoryEnvironment } from "./environment.mjs";

test("verification removes repository locations while retaining injected hooks and user configuration", () => {
  const env = {
    PATH: "/usr/bin:/bin",
    GIT_DIR: "/outer/.git",
    GIT_COMMON_DIR: "/outer/.git",
    GIT_WORK_TREE: "/outer",
    GIT_INDEX_FILE: "/outer/.git/index",
    GIT_CONFIG: "/outer/.git/config",
    GIT_OBJECT_DIRECTORY: "/outer/.git/objects",
    GIT_ALTERNATE_OBJECT_DIRECTORIES: "/other/objects",
    GIT_CONFIG_COUNT: "1",
    GIT_CONFIG_KEY_0: "core.hooksPath",
    GIT_CONFIG_VALUE_0: "/policy/hooks",
    GIT_CONFIG_PARAMETERS: "'user.name=Example'",
    GIT_CONFIG_GLOBAL: "/policy/gitconfig",
  };
  const clean = withoutRepositoryEnvironment(env);
  assert.doesNotThrow(() => assertRepositoryEnvironmentCleared(clean));
  for (const key of ["GIT_DIR", "GIT_COMMON_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_CONFIG", "GIT_OBJECT_DIRECTORY", "GIT_ALTERNATE_OBJECT_DIRECTORIES"]) {
    assert.equal(key in clean, false, key);
    assert.ok(key in env, "the parent's environment must stay unchanged");
  }
  for (const key of ["PATH", "GIT_CONFIG_COUNT", "GIT_CONFIG_KEY_0", "GIT_CONFIG_VALUE_0", "GIT_CONFIG_PARAMETERS", "GIT_CONFIG_GLOBAL"]) {
    assert.equal(clean[key], env[key], key);
  }
});

test("verification refuses a leaking Git environment before it invokes any Git operation or test", () => {
  const script = fileURLToPath(new URL("../verify.mjs", import.meta.url));
  const result = spawnSync(process.execPath, [script, "--release", "--dry-run"], {
    encoding: "utf8",
    env: { ...process.env, GIT_DIR: "/this-repository-must-never-be-opened/.git" },
  });
  assert.equal(result.status, 2, result.stderr);
  assert.match(result.stderr, /Refusing to run verification with repository-local Git variables:.*GIT_DIR/);
  assert.equal(result.stdout, "");
});

test("every foreign Git fixture refuses leaked locations before invoking even the sentinel Git", () => {
  const root = mkdtempSync(path.join(os.tmpdir(), "git-environment-guard-"));
  const bin = path.join(root, "bin");
  const marker = path.join(root, "git-was-invoked");
  mkdirSync(bin);
  const git = path.join(bin, "git");
  writeFileSync(git, '#!/bin/sh\nprintf called >> "$GIT_GUARD_PROBE"\nexit 91\n');
  chmodSync(git, 0o755);
  try {
    for (const relative of [
      "../lib/env.test.mjs", "../hooks/format-staged.test.mjs", "../hooks/commit-msg.test.mjs",
      "../hooks/pre-push.test.mjs", "./rerun.test.mjs", "./changes.test.mjs", "./tree.test.mjs",
      "../../apps/desktop/scripts/release-desktop.test.mjs",
    ]) {
      const file = fileURLToPath(new URL(relative, import.meta.url));
      const env = {
        ...process.env,
        PATH: `${bin}:${process.env.PATH}`,
        GIT_DIR: path.join(root, "outer.git"),
        GIT_GUARD_PROBE: marker,
      };
      delete env.NODE_TEST_CONTEXT;
      const result = spawnSync(process.execPath, ["--test", file], {
        encoding: "utf8",
        env,
      });
      assert.equal(result.status, 1, `${relative}: ${result.stderr}`);
      assert.match(result.stdout + result.stderr, /Refusing to run verification with repository-local Git variables/);
      assert.equal(existsSync(marker), false, `${relative} invoked Git before checking its environment`);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
