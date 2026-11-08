import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { TAR, fetchConpty, installerLayout, machineArchitecture, packageFiles, parseArguments, verifiedPackage } from "./fetch-conpty.mjs";

function scratch(t) {
  const dir = mkdtempSync(path.join(os.tmpdir(), "conpty-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  return dir;
}

const digest = (bytes) => createHash("sha256").update(bytes).digest("hex");

test("a download that is not the pinned package is refused and kept nowhere", async (t) => {
  const cacheFile = path.join(scratch(t), "cache", "package.nupkg");
  await assert.rejects(
    verifiedPackage({ cacheFile, expected: digest("pinned"), download: async () => Buffer.from("tampered") }),
    /not the pinned package/,
  );
  assert.equal(existsSync(cacheFile), false);
});

test("a verified package is kept, and a rerun reads it without downloading", async (t) => {
  const cacheFile = path.join(scratch(t), "cache", "package.nupkg");
  const bytes = Buffer.from("pinned");
  const first = await verifiedPackage({ cacheFile, expected: digest(bytes), download: async () => bytes });
  assert.deepEqual(readFileSync(cacheFile), bytes);
  const offline = async () => assert.fail("downloaded again");
  assert.deepEqual(await verifiedPackage({ cacheFile, expected: digest(bytes), download: offline }), first);
});

test("a cached file that is not the pinned package is downloaded over", async (t) => {
  const dir = scratch(t);
  const cacheFile = path.join(dir, "package.nupkg");
  writeFileSync(cacheFile, "corrupt");
  const bytes = Buffer.from("pinned");
  await verifiedPackage({ cacheFile, expected: digest(bytes), download: async () => bytes });
  assert.deepEqual(readFileSync(cacheFile), bytes);
});

test("the architecture is the machine's, whatever Node runs as", () => {
  assert.equal(machineArchitecture({ PROCESSOR_ARCHITECTURE: "AMD64", PROCESSOR_ARCHITEW6432: "ARM64" }, "x64"), "arm64");
  assert.equal(machineArchitecture({ PROCESSOR_ARCHITECTURE: "AMD64" }, "x64"), "x64");
  assert.equal(machineArchitecture({}, "arm64"), "arm64");
});

test("arguments name the architecture and any number of directories", () => {
  assert.deepEqual(parseArguments([], "arm64"), { arch: "arm64", into: [], installer: false });
  assert.deepEqual(parseArguments(["--arch", "x64", "--into", "a", "--into", "b"], "arm64"), { arch: "x64", into: ["a", "b"], installer: false });
  assert.throws(() => parseArguments(["--arch", "x86"]), /--arch is one of/);
  assert.throws(() => parseArguments(["--into"]), /needs a value/);
  assert.throws(() => parseArguments(["--force"]), /Unknown argument/);
});

test("the files are taken from the architecture's own folders of the package", () => {
  assert.deepEqual(packageFiles("arm64"), {
    dll: "runtimes/win-arm64/native/conpty.dll",
    host: "build/native/runtimes/arm64/OpenConsole.exe",
  });
});

test("the installer layout is one conpty.dll and an OpenConsole.exe per architecture", () => {
  assert.deepEqual(parseArguments(["--installer"], "arm64"), { arch: "arm64", into: [], installer: true });
  assert.throws(() => parseArguments(["--installer", "--arch", "x64"]), /takes neither/);
  assert.throws(() => parseArguments(["--into", "a", "--installer"]), /takes neither/);
  assert.deepEqual(installerLayout(), [
    { source: "runtimes/win-x64/native/conpty.dll", target: "conpty.dll" },
    { source: "build/native/runtimes/x64/OpenConsole.exe", target: "x64/OpenConsole.exe" },
    { source: "build/native/runtimes/arm64/OpenConsole.exe", target: "arm64/OpenConsole.exe" },
  ]);
});

// A stand-in package with the real one's paths, zipped (a `.nupkg` is a zip) by the system `tar` that `fetchConpty`
// extracts with (`TAR`, by its full path on Windows): bsdtar, as on macOS and Windows, which writes a zip given `--format zip`. GNU tar, the usual `tar` on
// Linux, neither writes nor reads a zip, so where it is the system's the tests that need the package are skipped:
// `fetchConpty` cannot extract a real `.nupkg` there either.
const zipTar = spawnSync(TAR, ["--version"], { encoding: "utf8" }).stdout?.includes("bsdtar") ?? false;
const needsZipTar = zipTar ? {} : { skip: "the system tar is not bsdtar, which fetchConpty needs to read a zip" };

function standInPackage(t) {
  const dir = scratch(t);
  const content = path.join(dir, "content");
  for (const { source } of installerLayout()) {
    mkdirSync(path.dirname(path.join(content, source)), { recursive: true });
    writeFileSync(path.join(content, source), source);
  }
  mkdirSync(path.join(content, "runtimes/win-arm64/native"), { recursive: true });
  writeFileSync(path.join(content, "runtimes/win-arm64/native/conpty.dll"), "runtimes/win-arm64/native/conpty.dll");
  const archive = path.join(dir, "package.zip");
  const packed = spawnSync(TAR, ["--format", "zip", "-cf", archive, "-C", content, "."], { encoding: "utf8" });
  assert.equal(packed.status, 0, packed.stderr);
  const bytes = readFileSync(archive);
  // A zip's local file header, so the test reads the format the real package comes in.
  assert.equal(bytes.subarray(0, 4).toString("binary"), "PK\x03\x04");
  return { root: path.join(dir, "root"), download: async () => bytes, expected: digest(bytes) };
}

test("the installer layout is written under target/conpty-bundle, each file from its architecture", needsZipTar, async (t) => {
  const { root, ...package_ } = standInPackage(t);
  const { targets } = await fetchConpty({ root, installer: true, ...package_ });
  const bundle = path.join(root, "target", "conpty-bundle");
  assert.deepEqual(targets, [bundle]);
  for (const { source, target } of installerLayout()) assert.equal(readFileSync(path.join(bundle, target), "utf8"), source);
});

test("a directory layout puts the machine's files in every directory it is given", needsZipTar, async (t) => {
  const { root, ...package_ } = standInPackage(t);
  const { targets } = await fetchConpty({ root, arch: "arm64", into: ["a", "b/c"], ...package_ });
  assert.deepEqual(targets, [path.join(root, "a"), path.join(root, "b", "c")]);
  for (const dir of targets) {
    assert.equal(readFileSync(path.join(dir, "conpty.dll"), "utf8"), "runtimes/win-arm64/native/conpty.dll");
    assert.equal(readFileSync(path.join(dir, "OpenConsole.exe"), "utf8"), "build/native/runtimes/arm64/OpenConsole.exe");
  }
});
