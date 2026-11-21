#!/usr/bin/env node
// Builds `octoboardd` in release mode, embedding the UI build (`embedded-ui`, which `packages/ui/dist` has to hold
// first: `pnpm build:tauri` builds the UI before this, and a run by hand builds it here when it is missing, since the
// daemon's `build.rs` refuses to build without it), and copies it into `src-tauri/binaries/` under the
// target-triple-suffixed name Tauri's `externalBin` requires (for a universal release, both architectures and
// their `lipo` join; for a Windows target the name ends in `.exe`), so `pnpm tauri build` works from a clean checkout without a manual copy step:
// `beforeBuildCommand` runs it (`pnpm build:tauri`), the one in `tauri.conf.json` and, on Windows, the one in
// `tauri.windows.conf.json`, which replaces it.
//
// The embedded UI is what the daemon serves a browser on its owner socket while Allow remote access is on, and at the
// browser port while Allow browser access is on; the window still loads the UI the shell bundles.
//
// `tauri dev` does NOT run it — `beforeDevCommand` starts the frontend dev server and nothing else.
// So a dev run after a change to `apps/daemon/` serves whatever binary is already in `binaries/`: a
// never-built checkout fails loudly on sidecar resolution, but a previously built one silently runs
// the stale daemon. Run this by hand before verifying any daemon-side change through the app.
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import path from "node:path";
import {
  appDir,
  repoRoot,
  UNIVERSAL_ARCHITECTURES as ARCHITECTURES,
  UNIVERSAL_TARGET as UNIVERSAL,
} from "./bundle.mjs";

const binariesDir = path.join(appDir, "src-tauri", "binaries");

// A universal release (`pnpm release`, which passes `--target universal-apple-darwin` to `tauri build`) is
// announced to this script, run as `beforeBuildCommand`, through `TAURI_ENV_TARGET_TRIPLE`. Tauri then builds the
// shell once per architecture and each build needs the sidecar named for its own triple, and the bundler wants one
// universal sidecar named `octoboardd-universal-apple-darwin`, so all three are provided.
const universal = process.env.TAURI_ENV_TARGET_TRIPLE === UNIVERSAL || process.argv.includes("--universal");

// The Windows release passes its target to `tauri build`, which announces it the same way: the daemon is then built for
// that target, `--locked` like the universal one, and a name with `.exe` is what `externalBin` looks for.
const windowsTarget = process.env.TAURI_ENV_TARGET_TRIPLE?.includes("windows") ? process.env.TAURI_ENV_TARGET_TRIPLE : null;

mkdirSync(binariesDir, { recursive: true });

// `embedded-ui` embeds the UI build, which a checkout where only this script runs (`pnpm build:daemon` by hand) has
// not made yet. An existing build is used as it is: `pnpm build:tauri` has just made it, and by hand a stale one is the
// caller's to rebuild.
if (!existsSync(path.join(repoRoot, "packages", "ui", "dist", "index.html"))) {
  console.log("packages/ui/dist is missing: building the UI first (pnpm --filter @octoboard/ui build)");
  execFileSync("pnpm", ["--filter", "@octoboard/ui", "build"], { cwd: repoRoot, stdio: "inherit", shell: process.platform === "win32" });
}

// Both crates are members of the root Cargo workspace, so the build runs from the repository root and writes into the
// one build directory there. A universal build is a release, built `--locked` from the dependencies `Cargo.lock`
// records: a lockfile that no longer matches the manifests fails at the first compile instead of being rewritten under
// the release.
if (universal) {
  const built = ARCHITECTURES.map((triple) => {
    execFileSync("cargo", ["build", "--release", "--locked", "--target", triple, "-p", "octoboardd", "--features", "embedded-ui"], {
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
} else if (windowsTarget) {
  execFileSync("cargo", ["build", "--release", "--locked", "--target", windowsTarget, "-p", "octoboardd", "--features", "embedded-ui"], {
    cwd: repoRoot,
    stdio: "inherit",
  });
  const source = path.join(repoRoot, "target", windowsTarget, "release", "octoboardd.exe");
  const dest = path.join(binariesDir, `octoboardd-${windowsTarget}.exe`);
  copyFileSync(source, dest);
  console.log(`copied ${source} -> ${dest}`);
} else {
  execFileSync("cargo", ["build", "--release", "-p", "octoboardd", "--features", "embedded-ui"], {
    cwd: repoRoot,
    stdio: "inherit",
  });

  // `rustc -vV`'s `host:` line is the running machine's own target triple — the same one Tauri
  // resolves a bare (non-cross-compiling) `tauri build` against.
  const rustcInfo = execFileSync("rustc", ["-vV"], { encoding: "utf8" });
  const hostLine = rustcInfo.split("\n").find((line) => line.startsWith("host: "));
  if (!hostLine) throw new Error("could not determine the host target triple from `rustc -vV`");
  const targetTriple = hostLine.slice("host: ".length).trim();

  const extension = targetTriple.includes("windows") ? ".exe" : "";
  const source = path.join(repoRoot, "target", "release", `octoboardd${extension}`);
  const dest = path.join(binariesDir, `octoboardd-${targetTriple}${extension}`);
  copyFileSync(source, dest);
  console.log(`copied ${source} -> ${dest}`);
}
