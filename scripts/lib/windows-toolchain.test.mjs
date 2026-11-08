import assert from "node:assert/strict";
import path from "node:path";
import { test } from "node:test";
import { WINDOWS_TARGET, windowsCheckEnv, windowsToolchainProblems, xwinCacheDir } from "./windows-toolchain.mjs";

const CACHE = "/cache/cargo-xwin";
const DONE = path.join(CACHE, "xwin", "DONE");
const installed = (cmd, args) => {
  if (cmd === "rustup") return `aarch64-apple-darwin\n${WINDOWS_TARGET}\n`;
  return `${cmd} ${args.join(" ")}\n`;
};
const problems = ({ run = installed, env = {}, files = [DONE] } = {}) =>
  windowsToolchainProblems({ env, run, exists: (file) => files.includes(file), cacheDir: CACHE });

test("a complete toolchain with the SDK already downloaded has no problems", () => {
  assert.deepEqual(problems(), []);
});

test("each missing prerequisite is named with how to provide it, all at once", () => {
  const found = problems({ run: () => null, files: [] });
  assert.equal(found.length, 4);
  assert.match(found[0], /cargo install --locked cargo-xwin/);
  assert.match(found[1], new RegExp(`rustup target add ${WINDOWS_TARGET}`));
  assert.match(found[2], /brew install llvm/);
  assert.match(found[3], /XWIN_ACCEPT_LICENSE=1 pnpm check:windows/);
});

test("a missing target is reported even when rustup lists others", () => {
  const run = (cmd, args) => (cmd === "rustup" ? "aarch64-apple-darwin\n" : installed(cmd, args));
  assert.deepEqual(problems({ run }).length, 1);
});

test("Apple's clang, which cannot build for MSVC, does not count as LLVM", () => {
  const run = (cmd, args) => (cmd === "clang" ? "Apple clang version 17.0.0\n" : installed(cmd, args));
  assert.match(problems({ run })[0], /not LLVM's/);
  assert.deepEqual(problems({ run: (cmd, args) => (cmd === "clang" ? "Homebrew clang version 23.1.5\n" : installed(cmd, args)) }), []);
});

test("the license is never accepted on the user's behalf: an empty cache needs the user's own XWIN_ACCEPT_LICENSE", () => {
  assert.equal(problems({ files: [] }).length, 1);
  assert.deepEqual(problems({ files: [], env: { XWIN_ACCEPT_LICENSE: "1" } }), []);
});

test("Homebrew's LLVM, when installed, is moved to the front of the PATH, ahead of Apple's clang", () => {
  const brew = "/opt/homebrew/opt/llvm/bin";
  const has = (file) => file === path.join(brew, "clang");
  assert.equal(windowsCheckEnv({ PATH: "/usr/bin" }, has).PATH, [brew, "/usr/bin"].join(path.delimiter));
  assert.equal(windowsCheckEnv({ PATH: ["/usr/bin", brew].join(path.delimiter) }, has).PATH, [brew, "/usr/bin"].join(path.delimiter));
  const first = { PATH: [brew, "/usr/bin"].join(path.delimiter) };
  assert.equal(windowsCheckEnv(first, has), first);
  const none = { PATH: "/usr/bin" };
  assert.equal(windowsCheckEnv(none, () => false), none);
});

test("the cache directory follows cargo-xwin's", () => {
  assert.equal(xwinCacheDir({ XWIN_CACHE_DIR: "/x" }, "darwin", "/h"), "/x");
  assert.equal(xwinCacheDir({}, "darwin", "/h"), "/h/Library/Caches/cargo-xwin");
  assert.equal(xwinCacheDir({}, "linux", "/h"), "/h/.cache/cargo-xwin");
  assert.equal(xwinCacheDir({ XDG_CACHE_HOME: "/c" }, "linux", "/h"), "/c/cargo-xwin");
});
