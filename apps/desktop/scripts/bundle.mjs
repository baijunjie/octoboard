// What the desktop build scripts share around `tauri build`: running it (the Tauri CLI binary, its
// environment and working directory), where its output lands, the lookup for the bundle file it
// produced, and the check that the `.app` carries the layered icon.
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

export const appDir = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const repoRoot = path.dirname(path.dirname(appDir));
// The Tauri crate is a member of the root Cargo workspace, so its bundle lands in the workspace's
// own build directory at the repository root, not under `src-tauri/`.
export const bundleDir = path.join(repoRoot, "target", "release", "bundle");
const tauriBin = path.join(appDir, "node_modules", ".bin", "tauri");

// Runs `tauri build` with `args` in the app directory and `env` as its environment, plus
// `scripts/actool/` first on the PATH: under the Node-hosted CLI the bundler starts `actool` with
// stdin closed, which crashes it, and the shim there reopens stdin on /dev/null.
export function tauriBuild(args, env) {
  const shimDir = path.join(appDir, "scripts", "actool");
  const buildEnv = { ...env, PATH: [shimDir, env.PATH].filter(Boolean).join(path.delimiter) };
  execFileSync(tauriBin, ["build", ...args], { cwd: appDir, env: buildEnv, stdio: "inherit" });
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
        "the workspace's target/release/bundle and rebuild so only the current version's is there.",
    );
  }
  return path.join(dir, matches[0]);
}
