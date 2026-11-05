import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

const script = fileURLToPath(new URL("../verify.mjs", import.meta.url));

test("partial release checks require an explicit valid release group", () => {
  for (const args of [
    ["--release-group"],
    ["--release-group", "frontend"],
    ["--release", "--release-group", "unknown"],
    ["--release", "--full", "--release-group", "native"],
    ["--release", "--quick", "--release-group", "frontend"],
  ]) {
    const result = spawnSync(process.execPath, [script, ...args], { encoding: "utf8" });
    assert.equal(result.status, 2, `${args.join(" ")}: ${result.stderr}`);
    assert.match(result.stderr, /--release-group|--quick cannot/);
  }
});
