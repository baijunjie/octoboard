#!/usr/bin/env node
// `pnpm release` on Linux: the daemon on its own (the server form) for the host's architecture, as a release archive.
// Builds the UI, then the release daemon embedding it (`embedded-ui`) and able to update itself (`self-update`, which
// only this build turns on) for the architecture's musl target, statically linked so that one
// binary runs on any distribution, checks that it is static, and packs it with the example systemd user unit and the
// license into `target/linux-release/octoboardd_<version>_linux_<arch>.tar.gz`, with a `.sha256` beside it; then signs
// the archive with the update key, unless `--no-sign-update` leaves that to `sign-update`.
//
//   pnpm --filter @octoboard/desktop release [--arch x86_64|aarch64] [--prebuilt-ui] [--no-sign-update]
//
// Each architecture builds on a host of its own architecture (`--arch` defaults to it), with `musl-gcc` (Debian and
// Ubuntu's `musl-tools`) compiling the C code of the dependencies (SQLite, zstd, ring). `--prebuilt-ui` embeds
// `packages/ui/dist` as it is instead of building it, for a build in a container that shares a checkout whose UI was
// built outside it.
import { execFileSync } from "node:child_process";
import { chmodSync, closeSync, copyFileSync, mkdirSync, mkdtempSync, openSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { parseArgs } from "node:util";
import { loadEnv } from "../../../scripts/lib/env.mjs";
import { gitChecked, run as probe } from "../../../scripts/lib/git.mjs";
import { packageVersion, repoRoot, withoutUpdateKey } from "./bundle.mjs";
import { assertStaticBinary, linuxArchOf, linuxReleaseDir, linuxTarget, sha256Line, sha256Of } from "./linux-release.mjs";
import { linuxArchiveName } from "./release-manifest.mjs";
import { signUpdateArchive } from "./update-archive.mjs";

if (process.platform !== "linux") throw new Error(`The Linux release builds on Linux; this is ${process.platform}.`);
const { values: options } = parseArgs({
  options: {
    arch: { type: "string" },
    "prebuilt-ui": { type: "boolean", default: false },
    "no-sign-update": { type: "boolean", default: false },
  },
});
const hostArch = linuxArchOf(process.arch);
if (!hostArch) throw new Error(`There is no Linux release for this host's architecture, ${process.arch}.`);
const arch = options.arch ?? hostArch;
const target = linuxTarget(arch);
if (arch !== hostArch) throw new Error(`This host is ${hostArch}; build the ${arch} release on an ${arch} host.`);
const signUpdate = !options["no-sign-update"];
if (signUpdate) {
  loadEnv(".env.secret");
  if (!process.env.TAURI_SIGNING_PRIVATE_KEY) {
    throw new Error("TAURI_SIGNING_PRIVATE_KEY is not set, and the archive is signed with it. Set it in .env.secret, or pass --no-sign-update.");
  }
}
const version = packageVersion();
// The update key stays out of every build step's environment, as on the other platforms: the UI build and cargo run
// the dependencies' own scripts.
const env = withoutUpdateKey(process.env);
const run = (command, args) => execFileSync(command, args, { cwd: repoRoot, stdio: "inherit", env });
const installed = (command) => probe("sh", ["-c", `command -v ${command}`]) !== null;

// The C compiler for the musl target, which the `cc` crate reads from `CC_<target>`: without one it would compile the
// dependencies' C code against glibc's headers, which then fails to link statically.
const ccVariable = `CC_${target.replaceAll("-", "_")}`;
if (!env[ccVariable]) {
  if (!installed("musl-gcc")) throw new Error(`musl-gcc is not installed (Debian and Ubuntu: \`apt-get install musl-tools\`), and it compiles the C code of the ${target} build.`);
  env[ccVariable] = "musl-gcc";
}
if (installed("rustup")) run("rustup", ["target", "add", target]);
if (!options["prebuilt-ui"]) run("pnpm", ["--filter", "@octoboard/ui", "build"]);
run("cargo", ["build", "--release", "--locked", "-p", "octoboardd", "--features", "embedded-ui,self-update", "--target", target]);

const targetDir = process.env.CARGO_TARGET_DIR ? path.resolve(repoRoot, process.env.CARGO_TARGET_DIR) : path.join(repoRoot, "target");
const binary = path.join(targetDir, target, "release", "octoboardd");
assertStaticBinary(binary, arch);

const name = linuxArchiveName(version, arch);
const archive = path.join(linuxReleaseDir, name);
const folder = name.replace(/\.tar\.gz$/, "");
mkdirSync(linuxReleaseDir, { recursive: true });
for (const file of [archive, `${archive}.sha256`, `${archive}.sig`]) rmSync(file, { force: true });
const staging = mkdtempSync(path.join(os.tmpdir(), "octoboard-linux-"));
try {
  const root = path.join(staging, folder);
  mkdirSync(root);
  copyFileSync(binary, path.join(root, "octoboardd"));
  chmodSync(path.join(root, "octoboardd"), 0o755);
  copyFileSync(path.join(repoRoot, "apps", "daemon", "dist", "systemd", "octoboardd.service"), path.join(root, "octoboardd.service"));
  copyFileSync(path.join(repoRoot, "LICENSE"), path.join(root, "LICENSE"));
  for (const file of ["octoboardd.service", "LICENSE"]) chmodSync(path.join(root, file), 0o644);
  // Owned by root, dated at the commit and compressed without gzip's own name and time stamp, so the archive carries
  // neither the build machine's user nor the time of the build.
  const commitTime = gitChecked(repoRoot, "log", "-1", "--format=%ct");
  const tarball = path.join(staging, "archive.tar");
  run("tar", ["--sort=name", "--owner=0", "--group=0", "--numeric-owner", `--mtime=@${commitTime}`, "-cf", tarball, "-C", staging, folder]);
  const out = openSync(archive, "w");
  try {
    execFileSync("gzip", ["-n", "-9", "-c", tarball], { stdio: ["ignore", out, "inherit"] });
  } finally {
    closeSync(out);
  }
} finally {
  rmSync(staging, { recursive: true, force: true });
}
writeFileSync(`${archive}.sha256`, sha256Line(sha256Of(archive), name));
console.log(`built ${archive}`);
if (signUpdate) console.log(`signed ${signUpdateArchive(archive, version)}`);
