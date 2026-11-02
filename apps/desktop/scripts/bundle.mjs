// What the desktop build scripts share around `tauri build`: where things are, running it and the environment it
// gets, and finding and checking what it produced. The update archive made from the `.app` is `update-archive.mjs`.
import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, readdirSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

export const appDir = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
export const repoRoot = path.dirname(path.dirname(appDir));
// The Tauri crate is a member of the root Cargo workspace, so its bundle lands in the workspace's
// own build directory at the repository root, not under `src-tauri/`.
export const bundleDir = path.join(repoRoot, "target", "release", "bundle");
// A release is one universal build (both Mac architectures in one `.app`), which lands under its target's own
// directory instead.
export const UNIVERSAL_TARGET = "universal-apple-darwin";
// The architectures a universal build is made of, each a Rust target the build compiles for.
export const UNIVERSAL_ARCHITECTURES = ["aarch64-apple-darwin", "x86_64-apple-darwin"];
export const universalBundleDir = path.join(repoRoot, "target", UNIVERSAL_TARGET, "release", "bundle");
export const tauriBin = path.join(appDir, "node_modules", ".bin", "tauri");

// Runs `tauri build` with `args` in the app directory and `env` as its environment, plus
// `scripts/actool/` first on the PATH: under the Node-hosted CLI the bundler starts `actool` with
// stdin closed, which crashes it, and the shim there reopens stdin on /dev/null.
export function tauriBuild(args, env) {
  const shimDir = path.join(appDir, "scripts", "actool");
  const buildEnv = { ...env, PATH: [shimDir, env.PATH].filter(Boolean).join(path.delimiter) };
  execFileSync(tauriBin, ["build", ...args], { cwd: appDir, env: buildEnv, stdio: "inherit" });
}

// `env` without the update signing key variables, for a build that must never hold the key. `tauri.conf.json` turns
// `bundle.createUpdaterArtifacts` off, so `tauri build` makes no update archive and signs nothing with the key; this
// keeps the key out of the environment of the build's own scripts and dependencies too. `tauriBuild` takes the result.
export function withoutUpdateKey(env) {
  return Object.fromEntries(Object.entries(env).filter(([name]) => !name.startsWith("TAURI_SIGNING_PRIVATE_KEY")));
}

// `env` without any `APPLE_*` variable, for a build that must never be notarized, or Developer ID-signed unless the
// caller adds the identity back. Filtered here and handed to `execFileSync` as an explicit `env`, never by a shell:
// stripping with `UNSET=$(...)` and `env $UNSET ...` is a no-op in zsh, which does not split an unquoted parameter.
export function withoutAppleVariables(env) {
  return Object.fromEntries(Object.entries(env).filter(([name]) => !name.startsWith("APPLE_")));
}

// Fails unless `bundle` was written after `startedAt`. An argument a caller does not know to refuse can leave a
// build's output elsewhere, or produce none, with a bundle from an earlier build still sitting where it is looked
// up; a check on that bundle would then pass on one this build did not make.
export function assertProducedSince(bundle, startedAt) {
  if (statSync(bundle).mtimeMs < startedAt) {
    throw new Error(`${bundle} predates this build, so this build did not produce it; check the arguments given.`);
  }
}

// The value of `key` in the `.app`'s `Info.plist`.
// It throws when the key is absent; PlistBuddy's complaint is captured in the error, not printed.
export function infoPlistValue(app, key) {
  return execFileSync("/usr/libexec/PlistBuddy", ["-c", `Print :${key}`, path.join(app, "Contents", "Info.plist")], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();
}

// Throws unless `codesign` accepts the `.app`'s signature, all nested code included.
export function verifySignature(app) {
  execFileSync("codesign", ["--verify", "--deep", "--strict", app], { stdio: "inherit" });
}

// The `spawnSync` result of `codesign -dv --verbose=4` on `app`, whose report (the `Authority=` lines among them) is
// on `stderr`, success or not. It exits non-zero for a bundle with no signature at all, which callers may expect, so
// it is not thrown on; each reads `error`, `signal` and `status` as it needs.
export function signatureReport(app) {
  return spawnSync("codesign", ["-dv", "--verbose=4", app], { encoding: "utf8" });
}

// The version the `.app` says it is (`CFBundleShortVersionString`), which is what the build was made from.
export function appVersion(app) {
  return infoPlistValue(app, "CFBundleShortVersionString");
}

// The bundler adds the layered icon only when Xcode 26's `actool` compiles it, and otherwise still
// succeeds, leaving an `.app` with the generic icon; so the `.app` is checked for the compiled
// `Assets.car` and the `CFBundleIconName` that points at it.
export function assertLayeredIcon(app) {
  const missing = [];
  if (!existsSync(path.join(app, "Contents", "Resources", "Assets.car"))) {
    missing.push("Contents/Resources/Assets.car");
  }
  try {
    infoPlistValue(app, "CFBundleIconName");
  } catch {
    missing.push("CFBundleIconName in Contents/Info.plist");
  }
  if (missing.length > 0) {
    throw new Error(
      `${app} lacks the layered app icon (missing ${missing.join(" and ")}). Compiling it needs ` +
        "Xcode 26's actool: install Xcode 26 and select it with xcode-select or DEVELOPER_DIR.",
    );
  }
}

// Tauri's bundle output directory holds one file per target's extension, but the `.dmg` one is
// named with the product version — after a version bump without a clean rebuild, both the old and
// the new disk image can be sitting there together, and this would silently pick whichever
// `readdirSync` happens to return first.
export function findBundle(dir, extension) {
  if (!existsSync(dir)) {
    throw new Error(`${dir} does not exist — did \`tauri build\` run its "${extension}" target?`);
  }
  const matches = readdirSync(dir).filter((name) => name.endsWith(extension));
  if (matches.length === 0) throw new Error(`no ${extension} file found in ${dir}`);
  if (matches.length > 1) {
    throw new Error(
      `found ${matches.length} "${extension}" files in ${dir} (${matches.join(", ")}) — clean ` +
        `the bundle directory ${dir} and rebuild so only the current version's is there.`,
    );
  }
  return path.join(dir, matches[0]);
}
