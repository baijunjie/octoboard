import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { chmodSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import {
  clangOnPath,
  WINDOWS_TARGETS,
  windowsCheckEnv,
  windowsToolchainProblems,
  writeArm64ClangShim,
  xwinCacheDir,
} from "./windows-toolchain.mjs";

const CACHE = "/cache/cargo-xwin";
const DONE = path.join(CACHE, "xwin", "DONE");
const installed = (cmd, args) => {
  if (cmd === "rustup") return `aarch64-apple-darwin\n${Object.values(WINDOWS_TARGETS).join("\n")}\n`;
  return `${cmd} ${args.join(" ")}\n`;
};
const problems = ({ run = installed, env = {}, files = [DONE] } = {}) =>
  windowsToolchainProblems({ env, run, exists: (file) => files.includes(file), cacheDir: CACHE });

test("a complete toolchain with the SDK already downloaded has no problems", () => {
  assert.deepEqual(problems(), []);
});

test("each missing prerequisite is named with how to provide it, all at once", () => {
  const found = problems({ run: () => null, files: [] });
  assert.equal(found.length, 5);
  assert.match(found[0], /cargo install --locked cargo-xwin/);
  assert.match(found[1], /rustup target add x86_64-pc-windows-msvc/);
  assert.match(found[2], /rustup target add aarch64-pc-windows-msvc/);
  assert.match(found[3], /brew install llvm/);
  assert.match(found[4], /XWIN_ACCEPT_LICENSE=1 pnpm check:windows/);
});

test("each missing Windows target is reported even when rustup lists others", () => {
  const run = (cmd, args) => (cmd === "rustup" ? "aarch64-apple-darwin\nx86_64-pc-windows-msvc\n" : installed(cmd, args));
  const found = problems({ run });
  assert.equal(found.length, 1);
  assert.match(found[0], /rustup target add aarch64-pc-windows-msvc/);
});

test("a check narrowed to one target asks only for that target", () => {
  const run = (cmd, args) => (cmd === "rustup" ? "x86_64-pc-windows-msvc\n" : installed(cmd, args));
  const narrowed = windowsToolchainProblems({ env: {}, run, exists: (file) => file === DONE, cacheDir: CACHE, targets: [WINDOWS_TARGETS.x64] });
  assert.deepEqual(narrowed, []);
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

test("the first clang on the PATH is found, none when there is none", () => {
  const PATH = ["/a", "/b", "/c"].join(path.delimiter);
  assert.equal(clangOnPath({ PATH }, (file) => file === "/b/clang" || file === "/c/clang"), "/b/clang");
  assert.equal(clangOnPath({ PATH }, () => false), null);
  assert.equal(clangOnPath({}, () => true), null);
});

const posixOnly = process.platform === "win32" ? { skip: "the shim is a POSIX shell script" } : {};

test("the arm64 shim passes /imsvc on as -idirafter as clang, and runs the real clang in cl mode as clang-cl", posixOnly, (t) => {
  const dir = mkdtempSync(path.join(os.tmpdir(), "clang-shim-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  // A stand-in for the real clang that prints each argument it gets on a line of its own.
  const real = path.join(dir, "real clang");
  writeFileSync(real, '#!/bin/sh\nfor arg; do printf "%s\\n" "$arg"; done\n');
  chmodSync(real, 0o755);
  const shim = path.join(writeArm64ClangShim(path.join(dir, "shim"), real), "clang");
  const run = (program, args) => execFileSync(program, args, { encoding: "utf8" }).trimEnd().split("\n");
  assert.deepEqual(run(shim, ["--target=aarch64-pc-windows-msvc", "/imsvc", "/x/crt include", "-c", "a.c", "/imsvc", "/x/um"]), [
    "--target=aarch64-pc-windows-msvc",
    "-idirafter",
    "/x/crt include",
    "-c",
    "a.c",
    "-idirafter",
    "/x/um",
  ]);
  assert.deepEqual(run(shim, ["-O0", "/imsvc"]), ["-O0", "/imsvc"]);
  const clangCl = path.join(dir, "clang-cl");
  symlinkSync(shim, clangCl);
  assert.deepEqual(run(clangCl, ["/imsvc", "/x/um", "/c"]), ["--driver-mode=cl", "/imsvc", "/x/um", "/c"]);
});
