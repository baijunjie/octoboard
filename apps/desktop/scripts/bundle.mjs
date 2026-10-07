// Where `tauri build` puts its output (the bundle directory and the Tauri CLI binary) and the
// lookup for the bundle file a build produced.
import { existsSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

export const appDir = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const repoRoot = path.dirname(path.dirname(appDir));
// The Tauri crate is a member of the root Cargo workspace, so its bundle lands in the workspace's
// own build directory at the repository root, not under `src-tauri/`.
export const bundleDir = path.join(repoRoot, "target", "release", "bundle");
export const tauriBin = path.join(appDir, "node_modules", ".bin", "tauri");

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
        "the workspace's target/release/bundle and rebuild so only the current version's is there."
    );
  }
  return path.join(dir, matches[0]);
}
