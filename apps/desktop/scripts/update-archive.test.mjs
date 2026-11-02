import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { existsSync, linkSync, mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { createUpdateArchive } from "./update-archive.mjs";

const darwin = { skip: process.platform !== "darwin" && "the archive is made with macOS's bsdtar" };

function makeApp(root) {
  const app = path.join(root, "Test.app");
  mkdirSync(path.join(app, "Contents", "MacOS"), { recursive: true });
  const binary = path.join(app, "Contents", "MacOS", "test");
  writeFileSync(binary, "binary");
  symlinkSync("test", path.join(app, "Contents", "MacOS", "alias"));
  execFileSync("xattr", ["-w", "com.octoboard.test", "value", binary]);
  return app;
}

test("the update archive keeps symlinks, has the .app as its one root and carries no xattr headers", darwin, () => {
  const root = mkdtempSync(path.join(os.tmpdir(), "update-archive-"));
  try {
    const archive = createUpdateArchive(makeApp(root));
    const entries = execFileSync("/usr/bin/tar", ["-tzf", archive], { encoding: "utf8" }).trim().split("\n");
    assert.ok(entries.every((entry) => entry === "Test.app/" || entry.startsWith("Test.app/")), entries.join("\n"));
    assert.ok(!entries.some((entry) => entry.includes("._")));
    const listing = execFileSync("/usr/bin/tar", ["-tvzf", archive], { encoding: "utf8" });
    assert.match(listing, /^l.* Test\.app\/Contents\/MacOS\/alias -> test$/m);
    const raw = execFileSync("gzip", ["-dc", archive], { maxBuffer: 64 * 1024 * 1024 }).toString("latin1");
    assert.ok(!raw.includes("xattr"), "a pax xattr header is in the archive");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("an .app holding a hard link is refused and leaves no archive", darwin, () => {
  const root = mkdtempSync(path.join(os.tmpdir(), "update-archive-"));
  try {
    const app = makeApp(root);
    linkSync(path.join(app, "Contents", "MacOS", "test"), path.join(app, "Contents", "MacOS", "twin"));
    assert.throws(() => createUpdateArchive(app), /cannot unpack/);
    assert.ok(!existsSync(`${app}.tar.gz`));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
