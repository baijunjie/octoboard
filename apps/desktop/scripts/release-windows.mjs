#!/usr/bin/env node
// Builds the distributable Windows installer, x64: `tauri build --target x86_64-pc-windows-msvc --bundles nsis`,
// whose own `beforeBuildCommand` (see `tauri.windows.conf.json`) fetches the ConPTY files, builds the daemon sidecar
// and the frontend first. `release.mjs` starts it on Windows, and the shared `--no-sign-update` flag does what it does
// on macOS: the update signature is left to `sign-update.mjs`, so that the build never holds the update key and a
// build without the flag needs it (`TAURI_SIGNING_PRIVATE_KEY`, see `.env.secret.example`). The update artifact is
// the installer itself, signed with that key after the last step that could change its bytes.
//
// The code-signing mode is `WINDOWS_SIGNING` (`windows-release.mjs` describes the three). `none` and `command` are
// one run. `signpath` is three, each a `--stage`, with a SignPath submission between them: `build` leaves our two
// executables in `target/signpath/binaries/`, `bundle --signed <dir>` puts the signed ones back, bundles the installer
// from them and leaves it in `target/signpath/installer/`, and `finish --signed <dir>` puts the signed installer in
// the bundle directory and checks the result.
//
// Whatever the mode, the finished installer is looked into: see `verifyInstaller` and `verify()` below.
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import path from "node:path";
import { parseArgs } from "node:util";
import { loadEnv } from "../../../scripts/lib/env.mjs";
import {
  appDir,
  assertProducedSince,
  findBundle,
  packageVersion,
  runTauri,
  tauriBuild,
  WINDOWS_TARGET,
  windowsBundleDir,
  windowsReleaseDir,
  withoutUpdateKey,
} from "./bundle.mjs";
import { signUpdateArchive } from "./update-archive.mjs";
import {
  assertSigned,
  checkStage,
  findSigntool,
  removeInstallers,
  reportSignature,
  signConfigArgs,
  signFileScript,
  signingMode,
  submissionDir,
  verifyInstaller,
} from "./windows-release.mjs";

const { values: options } = parseArgs({
  options: {
    "no-sign-update": { type: "boolean", default: false },
    stage: { type: "string" },
    signed: { type: "string" },
  },
});
const signUpdate = !options["no-sign-update"];

// Read only `.env.secret`, as on macOS: a `VITE_` entry in `.env.local` would reach the bundle.
loadEnv(".env.secret");

const mode = signingMode(process.env);
const stage = checkStage(mode, options.stage, options.signed);
const nsisDir = path.join(windowsBundleDir, "nsis");
const binariesDir = path.join(appDir, "src-tauri", "binaries");
// Our own executables, which `signpath` submits and takes back signed: the shell, and the sidecar as the bundler
// finds it (`externalBin`) under its target-triple name.
const ownFiles = [
  { name: "Octoboard.exe", built: path.join(windowsReleaseDir, "Octoboard.exe") },
  { name: "octoboardd.exe", built: path.join(binariesDir, `octoboardd-${WINDOWS_TARGET}.exe`) },
];
const unsignedBinariesDir = path.join(submissionDir, "binaries");
const unsignedInstallerDir = path.join(submissionDir, "installer");

console.log(
  mode === "none"
    ? "WINDOWS_SIGNING is not set: building unsigned. Windows SmartScreen warns on the download; see the \"Release builds\" section of apps/desktop/scripts/README.md."
    : `WINDOWS_SIGNING=${mode}${stage ? `, stage ${stage}` : ""}.`,
);

// The key is first used at the end, but a missing one is refused before the build spends its minutes. The `signpath`
// stages before the last never use it.
if (signUpdate && (stage === null || stage === "finish") && !process.env.TAURI_SIGNING_PRIVATE_KEY) {
  throw new Error(
    "The update is signed with the update key, and TAURI_SIGNING_PRIVATE_KEY is not set. Set it to the path of the key " +
      "file (the one `pnpm tauri signer generate` wrote) in .env.secret, and TAURI_SIGNING_PRIVATE_KEY_PASSWORD too " +
      "when the key has a password; or pass --no-sign-update and sign later with `pnpm sign-update`. See " +
      ".env.secret.example and apps/desktop/scripts/README.md.",
  );
}

const startedAt = Date.now();
const buildEnv = withoutUpdateKey(process.env);
// What follows `--` goes to cargo: a release is built `--locked`, as on macOS.
const locked = ["--", "--locked"];

if (stage === null || stage === "build") {
  addRustTarget();
  // The installer's name carries the version, so one an earlier build of this version left would pass for this one's.
  removeInstallers(nsisDir);
  if (stage === "build") {
    tauriBuild(["--target", WINDOWS_TARGET, "--no-bundle", ...locked], buildEnv);
    for (const { name, built } of ownFiles) copyInto(built, path.join(unsignedBinariesDir, name));
    console.log(`submit the files in ${unsignedBinariesDir} to SignPath, then run --stage bundle --signed <directory>.`);
  } else {
    // `tauri.conf.json` has the build make no update artifact, so the key stays out of the build's environment.
    tauriBuild(["--target", WINDOWS_TARGET, "--bundles", "nsis", ...signConfigArgs(mode, signFileScript), ...locked], buildEnv);
  }
}

if (stage === "bundle") {
  const signtool = requireSigntool();
  removeInstallers(nsisDir);
  const signed = ownFiles.map(({ name }) => path.join(options.signed, name));
  assertSigned(signtool, signed);
  // The sidecar is bundled from `binaries/`, and `tauri-build` also left a copy beside the shell.
  copyInto(signed[0], ownFiles[0].built);
  copyInto(signed[1], ownFiles[1].built);
  if (existsSync(path.join(windowsReleaseDir, "octoboardd.exe"))) copyInto(signed[1], path.join(windowsReleaseDir, "octoboardd.exe"));
  // The patching of the shell with the bundle type would invalidate its signature.
  runTauri(["bundle", "--target", WINDOWS_TARGET, "--bundles", "nsis", "--no-binary-patching"], { env: buildEnv });
  copyInto(findBundle(nsisDir, "-setup.exe"), path.join(unsignedInstallerDir, "installer-setup.exe"));
  console.log(`submit the installer in ${unsignedInstallerDir} to SignPath, then run --stage finish --signed <directory>.`);
}

if (stage === "finish") {
  const signtool = requireSigntool();
  const setup = findBundle(nsisDir, "-setup.exe");
  const signed = findBundle(options.signed, "-setup.exe");
  assertSigned(signtool, [signed]);
  copyFileSync(signed, setup);
}

if (stage === null || stage === "finish") {
  const setup = findBundle(nsisDir, "-setup.exe");
  if (stage === null) assertProducedSince(setup, startedAt);
  verify(setup);
  // Nothing may change the installer's bytes after this.
  if (signUpdate) {
    console.log(`signed ${signUpdateArchive(setup, packageVersion())}`);
    console.log(`found update signature: ${findBundle(nsisDir, "-setup.exe.sig")}`);
  }
}

function addRustTarget() {
  try {
    execFileSync("rustup", ["target", "add", WINDOWS_TARGET], { stdio: "inherit", cwd: appDir });
  } catch (err) {
    throw new Error(
      `Could not add the Rust target ${WINDOWS_TARGET} (${err.message}). The toolchain is managed by rustup: make ` +
        `sure it is on the PATH, add it with \`rustup target add ${WINDOWS_TARGET}\`, then run \`pnpm release\` again.`,
      { cause: err },
    );
  }
}

function copyInto(from, to) {
  mkdirSync(path.dirname(to), { recursive: true });
  copyFileSync(from, to);
}

function requireSigntool() {
  const signtool = findSigntool();
  if (!signtool) throw new Error("signtool.exe was not found (it comes with the Windows SDK); the signatures cannot be checked.");
  return signtool;
}

// Confirms what the build produced instead of assuming it from the mode: outside `none`, the installer and our two
// executables in it must verify as signed (`signtool verify /pa`); in every mode the ConPTY files are checked and
// reported (`verifyInstaller`).
function verify(setup) {
  console.log(`found installer: ${setup}`);
  const signtool = mode === "none" ? findSigntool() : requireSigntool();
  if (mode !== "none") assertSigned(signtool, [setup]);
  reportSignature(signtool, setup);
  verifyInstaller({ setup, mode, signtool });
  if (mode === "signpath") console.log("the uninstaller NSIS generates while it builds the installer is not signed.");
}
