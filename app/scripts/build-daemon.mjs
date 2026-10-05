#!/usr/bin/env node
// Builds `octoboardd` in release mode and copies it into `src-tauri/binaries/` under the
// target-triple-suffixed name Tauri's `externalBin` requires, so `npm run tauri build` works from a
// clean checkout without a manual copy step: `beforeBuildCommand` in tauri.conf.json runs it.
//
// `tauri dev` does NOT run it — `beforeDevCommand` starts the frontend dev server and nothing else.
// So a dev run after a change to `daemon/` serves whatever binary is already in `binaries/`: a
// never-built checkout fails loudly on sidecar resolution, but a previously built one silently runs
// the stale daemon. Run this by hand before verifying any daemon-side change through the app.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const appDir = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const repoRoot = path.dirname(appDir);
const daemonDir = path.join(repoRoot, "daemon");
const binariesDir = path.join(appDir, "src-tauri", "binaries");

execFileSync("cargo", ["build", "--release"], { cwd: daemonDir, stdio: "inherit" });

// `rustc -vV`'s `host:` line is the running machine's own target triple — the same one Tauri
// resolves a bare (non-cross-compiling) `tauri build` against, so there is no cross-compilation
// case to handle here.
const rustcInfo = execFileSync("rustc", ["-vV"], { encoding: "utf8" });
const hostLine = rustcInfo.split("\n").find((line) => line.startsWith("host: "));
if (!hostLine) throw new Error("could not determine the host target triple from `rustc -vV`");
const targetTriple = hostLine.slice("host: ".length).trim();

mkdirSync(binariesDir, { recursive: true });
const source = path.join(daemonDir, "target", "release", "octoboardd");
const dest = path.join(binariesDir, `octoboardd-${targetTriple}`);
copyFileSync(source, dest);
console.log(`copied ${source} -> ${dest}`);
