import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { syncIdentity } from "./sync-identity.mjs";

function fixture(t) {
  const root = mkdtempSync(path.join(os.tmpdir(), "identity-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(path.join(root, "config"));
  writeFileSync(path.join(root, "config/app.json"), JSON.stringify({ author: { name: "New author $&" }, copyrightYear: 2030 }));
  writeFileSync(path.join(root, "LICENSE"), "MIT License\n\nCopyright (c) 2020 Previous author\n\nPermission text.\n");
  writeFileSync(path.join(root, "Cargo.toml"), '[workspace]\nmembers = []\n[workspace.package]\nversion = "1.0.0"\nauthors = ["Previous author"]\nlicense = "MIT"\n[profile.dev]\ndebug = false\n');
  return { root, read: (file) => readFileSync(path.join(root, file), "utf8") };
}

test("one identity source regenerates required literals, preserving unrelated content", (t) => {
  const { root, read } = fixture(t);
  const license = read("LICENSE");
  const manifest = read("Cargo.toml");
  assert.throws(() => syncIdentity({ root, check: true }), /pnpm identity:sync/);
  assert.equal(read("LICENSE"), license);
  assert.equal(read("Cargo.toml"), manifest);
  assert.deepEqual(syncIdentity({ root }), ["LICENSE", "Cargo.toml"]);
  assert.equal(read("LICENSE"), license.replace("2020 Previous author", () => "2030 New author $&"));
  assert.equal(read("Cargo.toml"), manifest.replace("Previous author", () => "New author $&"));
  assert.deepEqual(syncIdentity({ root, check: true }), []);
  assert.deepEqual(syncIdentity({ root }), []);
});

test("invalid inputs and ambiguous output fields fail before any file is written", (t) => {
  const { root, read } = fixture(t);
  const license = read("LICENSE");
  writeFileSync(path.join(root, "Cargo.toml"), "[workspace.package]\nauthors = []\nauthors = []\n");
  assert.throws(() => syncIdentity({ root }), /one workspace.package authors/);
  assert.equal(read("LICENSE"), license);
  writeFileSync(path.join(root, "config/app.json"), JSON.stringify({ author: { name: "Invalid\nname" }, copyrightYear: 2030 }));
  assert.throws(() => syncIdentity({ root }), /single-line author.name/);
  assert.equal(read("LICENSE"), license);
});
