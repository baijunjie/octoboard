#!/usr/bin/env node
// Builds `octoboardd` in release mode and copies it into `src-tauri/binaries/` under the
// target-triple-suffixed name Tauri's `externalBin` requires (for a universal release, both architectures and
// their `lipo` join), so `pnpm tauri build` works from a clean checkout without a manual copy step:
// `beforeBuildCommand` in tauri.conf.json runs it.
//
// `tauri dev` does NOT run it — `beforeDevCommand` starts the frontend dev server and nothing else.
// So a dev run after a change to `apps/daemon/` serves whatever binary is already in `binaries/`: a
// never-built checkout fails loudly on sidecar resolution, but a previously built one silently runs
// the stale daemon. Run this by hand before verifying any daemon-side change through the app.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { UNIVERSAL_ARCHITECTURES as ARCHITECTURES, UNIVERSAL_TARGET as UNIVERSAL } from "./bundle.mjs";

const appDir = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const repoRoot = path.dirname(path.dirname(appDir));
const binariesDir = path.join(appDir, "src-tauri", "binaries");

// A universal release (`pnpm release`, which passes `--target universal-apple-darwin` to `tauri build`) is
// announced to this script, run as `beforeBuildCommand`, through `TAURI_ENV_TARGET_TRIPLE`. Tauri then builds the
// shell once per architecture and each build needs the sidecar named for its own triple, and the bundler wants one
// universal sidecar named `octoboardd-universal-apple-darwin`, so all three are provided.
const universal = process.env.TAURI_ENV_TARGET_TRIPLE === UNIVERSAL || process.argv.includes("--universal");

mkdirSync(binariesDir, { recursive: true });

// Both crates are members of the root Cargo workspace, so the build runs from the repository root
// and writes into the one build directory there.
if (universal) {
  const built = ARCHITECTURES.map((triple) => {
    execFileSync("cargo", ["build", "--release", "--target", triple, "-p", "octoboardd"], {
      cwd: repoRoot,
      stdio: "inherit",
    });
    const dest = path.join(binariesDir, `octoboardd-${triple}`);
    copyFileSync(path.join(repoRoot, "target", triple, "release", "octoboardd"), dest);
    console.log(`copied the ${triple} build -> ${dest}`);
    return dest;
  });
  const dest = path.join(binariesDir, `octoboardd-${UNIVERSAL}`);
  execFileSync("lipo", ["-create", "-output", dest, ...built], { stdio: "inherit" });
  console.log(`joined ${built.length} builds -> ${dest}`);
} else {
  execFileSync("cargo", ["build", "--release", "-p", "octoboardd"], {
    cwd: repoRoot,
    stdio: "inherit",
  });

  // `rustc -vV`'s `host:` line is the running machine's own target triple — the same one Tauri
  // resolves a bare (non-cross-compiling) `tauri build` against.
  const rustcInfo = execFileSync("rustc", ["-vV"], { encoding: "utf8" });
  const hostLine = rustcInfo.split("\n").find((line) => line.startsWith("host: "));
  if (!hostLine) throw new Error("could not determine the host target triple from `rustc -vV`");
  const targetTriple = hostLine.slice("host: ".length).trim();

  const source = path.join(repoRoot, "target", "release", "octoboardd");
  const dest = path.join(binariesDir, `octoboardd-${targetTriple}`);
  copyFileSync(source, dest);
  console.log(`copied ${source} -> ${dest}`);
}
