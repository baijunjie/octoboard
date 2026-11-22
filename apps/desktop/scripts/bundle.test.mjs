import assert from "node:assert/strict";
import path from "node:path";
import { test } from "node:test";
import { repoRoot, windowsBundleDir, windowsReleaseDir, windowsTarget } from "./bundle.mjs";

test("each Windows architecture builds for its own Rust target, whose directory holds its bundle", () => {
  assert.equal(windowsTarget("x64"), "x86_64-pc-windows-msvc");
  assert.equal(windowsTarget("arm64"), "aarch64-pc-windows-msvc");
  for (const [arch, target] of [["x64", "x86_64-pc-windows-msvc"], ["arm64", "aarch64-pc-windows-msvc"]]) {
    assert.equal(windowsReleaseDir(arch), path.join(repoRoot, "target", target, "release"));
    assert.equal(windowsBundleDir(arch), path.join(repoRoot, "target", target, "release", "bundle"));
  }
});

test("an architecture the Windows release does not build is refused, naming the ones it does", () => {
  for (const arch of ["x86", "aarch64", "x86_64", "", "ARM64"]) {
    assert.throws(() => windowsTarget(arch), /is not a Windows release architecture; it is one of x64, arm64/, arch);
    assert.throws(() => windowsBundleDir(arch), /is not a Windows release architecture/, arch);
  }
});
