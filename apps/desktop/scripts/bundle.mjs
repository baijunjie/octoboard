// What the desktop build scripts share around `tauri build`: running it (the Tauri CLI binary, its
// environment and working directory), the universal target and the architectures it is made of, where its
// output lands, the lookup for the bundle file it produced, the version an `.app` says it is, the check that it
// carries the layered icon, the removal of the update archives a build leaves behind, and the arguments and
// environment of a build that makes no update archive.
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, rmSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

export const appDir = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const repoRoot = path.dirname(path.dirname(appDir));
// The Tauri crate is a member of the root Cargo workspace, so its bundle lands in the workspace's
// own build directory at the repository root, not under `src-tauri/`.
export const bundleDir = path.join(repoRoot, "target", "release", "bundle");
// A release is one universal build (both Mac architectures in one `.app`), which lands under its target's own
// directory instead.
export const UNIVERSAL_TARGET = "universal-apple-darwin";
// The architectures a universal build is made of, each a Rust target the build compiles for.
export const UNIVERSAL_ARCHITECTURES = ["aarch64-apple-darwin", "x86_64-apple-darwin"];
export const universalBundleDir = path.join(repoRoot, "target", UNIVERSAL_TARGET, "release", "bundle");
const tauriBin = path.join(appDir, "node_modules", ".bin", "tauri");

// Runs `tauri build` with `args` in the app directory and `env` as its environment, plus
// `scripts/actool/` first on the PATH: under the Node-hosted CLI the bundler starts `actool` with
// stdin closed, which crashes it, and the shim there reopens stdin on /dev/null.
export function tauriBuild(args, env) {
  const shimDir = path.join(appDir, "scripts", "actool");
  const buildEnv = { ...env, PATH: [shimDir, env.PATH].filter(Boolean).join(path.delimiter) };
  execFileSync(tauriBin, ["build", ...args], { cwd: appDir, env: buildEnv, stdio: "inherit" });
}

// `bundle.createUpdaterArtifacts` in `tauri.conf.json` makes `tauri build` sign an update archive and fail
// without the update signing key, which only a release has (`release.mjs`). A build for local checks or local
// installation neither needs the key nor should sign anything with it, so it turns the archive off and drops the
// key variables. The archive goes off through `--config`: the CLI only writes `TAURI_CONFIG` for the app's own
// build and never reads it, so setting that variable does nothing. It goes first, ahead of a `--` that hands the
// rest to cargo. Returns the `[args, env]` pair `tauriBuild` takes.
export function withoutUpdaterArtifacts(args, env) {
  const kept = Object.fromEntries(
    Object.entries(env).filter(([name]) => !name.startsWith("TAURI_SIGNING_PRIVATE_KEY")),
  );
  return [["--config", JSON.stringify({ bundle: { createUpdaterArtifacts: false } }), ...args], kept];
}

// The version the `.app` says it is (`CFBundleShortVersionString`), which is what the build was made from.
export function appVersion(app) {
  return execFileSync(
    "/usr/libexec/PlistBuddy",
    ["-c", "Print :CFBundleShortVersionString", path.join(app, "Contents", "Info.plist")],
    { encoding: "utf8" },
  ).trim();
}

// Removes the update archives and signatures of earlier builds from `dir`. Their names carry no version, and an
// unsigned build makes none, so one left over would be taken for the new build's.
export function removeUpdateArchives(dir) {
  if (!existsSync(dir)) return;
  for (const name of readdirSync(dir)) {
    if (name.includes(".app.tar.gz")) rmSync(path.join(dir, name), { force: true });
  }
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
    execFileSync(
      "/usr/libexec/PlistBuddy",
      ["-c", "Print :CFBundleIconName", path.join(app, "Contents", "Info.plist")],
      { stdio: "ignore" },
    );
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
