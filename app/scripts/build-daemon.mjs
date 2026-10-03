#!/usr/bin/env node
// Builds `octoboardd` in release mode and copies it into `src-tauri/binaries/` under the
// target-triple-suffixed name Tauri's `externalBin` requires, so `npm run tauri build` (or
// `tauri dev`, via `beforeDevCommand`/`beforeBuildCommand` in tauri.conf.json) works from a clean
// checkout without a manual copy step.
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
