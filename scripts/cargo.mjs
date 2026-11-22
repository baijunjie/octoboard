#!/usr/bin/env node
// Runs `cargo test`, `cargo clippy` or `cargo check` over the root Cargo workspace, so both crates build
// once with one feature set: `node scripts/cargo.mjs <test|clippy|check> [cargo arguments]`. Extra arguments
// go to cargo (`pnpm test:rust some_test_name` filters by test name; for `clippy`, which ends in the
// lint flags, they come before them). With `-p <crate>` or `--package <crate>` among them (the
// `--package=<crate>` form is not recognised) only `--workspace` is dropped, so the command covers just that
// crate with its other flags (`--all-targets`) kept. Features are then resolved for that crate alone, so a
// dependency whose features differ, and the crate itself, build a second time beside the workspace build; to
// narrow to one crate without that second build, pass its targets (`--bin`, `--lib`) instead, which keeps
// `--workspace`.
//
// The desktop crate's `build.rs` makes `tauri-build` refuse to build when the sidecar named by
// `bundle.externalBin` or a file named by `bundle.resources` (the Windows installer's ConPTY files, which
// `pnpm fetch:conpty` lays out) is missing, which is what a real build wants. None of these commands starts
// the sidecar or bundles anything, so this script overrides both to empty through `TAURI_CONFIG` and they
// need no daemon build and no fetched files. Only this script sets the override: a plain `cargo build` or
// `pnpm tauri build` still fails without them.
//
// `check-windows` cross-checks for both Windows targets the release builds, `x86_64-pc-windows-msvc` and then
// `aarch64-pc-windows-msvc`, through `cargo xwin`. It first checks the toolchain that needs
// (`scripts/lib/windows-toolchain.mjs`) and fails naming how to install each missing piece. Both crates gate, for each
// target: the desktop crate is checked first and then the daemon, and the exit code is non-zero when any fails.
// Once the daemon checks clean, its executable is also linked (`cargo xwin build --bin octoboardd`), since a
// compile check never runs the linker and a link failure (a linker argument, an embedded resource, a native
// library) would otherwise surface only on a Windows machine; the link needs nothing the check does not. The
// desktop crate is compile-checked only.
// `pnpm check:windows desktop` or `pnpm check:windows daemon` narrows the run to one crate (the daemon's still
// linked), and `x64` or `arm64` after it, or alone, to one target: `pnpm check:windows daemon arm64`.
//
// For `aarch64-pc-windows-msvc` a shim `clang` goes first on the PATH (`writeArm64ClangShim`, in `arm64-clang-shim/`
// of cargo-xwin's cache), which `ring`'s build script needs on this target; see there why. It lives beside the cache
// because cargo-xwin links its machine-wide `clang-cl` to it: a shim inside a worktree would leave that link dangling
// once the worktree is removed.
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";
import {
  clangOnPath,
  WINDOWS_TARGETS,
  xwinCacheDir,
  windowsCheckEnv,
  windowsToolchainProblems,
  writeArm64ClangShim,
} from "./lib/windows-toolchain.mjs";

const repoRoot = path.dirname(path.dirname(fileURLToPath(import.meta.url)));

const COMMANDS = {
  test: { scope: ["--workspace"], tail: [] },
  clippy: { scope: ["--workspace", "--all-targets"], tail: ["--", "-D", "warnings"] },
  check: { scope: ["--workspace", "--all-targets"], tail: [] },
};

const WINDOWS_CRATES = { desktop: "octoboard", daemon: "octoboardd" };
const tauriConfig = { TAURI_CONFIG: JSON.stringify({ bundle: { externalBin: [], resources: [] } }) };

const run = (args, env = { ...process.env, ...tauriConfig }) => {
  const result = spawnSync("cargo", args, { cwd: repoRoot, stdio: "inherit", env });
  if (result.error) throw result.error;
  return result.status ?? 1;
};

const [name, ...extra] = process.argv.slice(2);
if (name === "check-windows") {
  const crateArg = WINDOWS_CRATES[extra[0]] ? extra[0] : undefined;
  const afterCrate = crateArg ? extra.slice(1) : extra;
  const archArg = WINDOWS_TARGETS[afterCrate[0]] ? afterCrate[0] : undefined;
  const args = archArg ? afterCrate.slice(1) : afterCrate;
  const misplaced = args.some((arg) => WINDOWS_CRATES[arg] || WINDOWS_TARGETS[arg]);
  if (misplaced || (!crateArg && !archArg && extra[0] !== undefined && !extra[0].startsWith("-"))) {
    console.error("Usage: node scripts/cargo.mjs check-windows [desktop|daemon] [x64|arm64] [cargo arguments]");
    process.exit(2);
  }
  const targets = archArg ? [WINDOWS_TARGETS[archArg]] : Object.values(WINDOWS_TARGETS);
  const windowsEnv = { ...windowsCheckEnv(), ...tauriConfig };
  const problems = windowsToolchainProblems({ env: windowsEnv, targets });
  if (problems.length > 0) {
    console.error(`check-windows: the Windows cross-check cannot run on this machine:\n${problems.map((problem) => `- ${problem}`).join("\n")}`);
    process.exit(1);
  }
  const targetEnv = (target) => {
    const clang = clangOnPath(windowsEnv);
    if (target !== WINDOWS_TARGETS.arm64 || process.platform === "win32" || !clang) return windowsEnv;
    const shim = writeArm64ClangShim(path.join(xwinCacheDir(windowsEnv), "arm64-clang-shim"), clang);
    return { ...windowsEnv, PATH: [shim, windowsEnv.PATH].join(path.delimiter) };
  };
  const checkCrate = (crate, target) =>
    run(["xwin", "check", "--target", target, "--all-targets", "-p", WINDOWS_CRATES[crate], ...args], targetEnv(target));
  const checkDaemon = (target) => {
    const checked = checkCrate("daemon", target);
    if (checked !== 0) return checked;
    return run(["xwin", "build", "--target", target, "-p", WINDOWS_CRATES.daemon, "--bin", WINDOWS_CRATES.daemon, ...args], targetEnv(target));
  };
  const checkTarget = (target) => {
    if (crateArg === "daemon") return checkDaemon(target);
    if (crateArg) return checkCrate(crateArg, target);
    const desktop = checkCrate("desktop", target);
    const daemon = checkDaemon(target);
    return desktop !== 0 ? desktop : daemon;
  };
  const statuses = targets.map((target) => {
    console.error(`check-windows: ${target}`);
    return checkTarget(target);
  });
  process.exit(statuses.find((status) => status !== 0) ?? 0);
}
const command = COMMANDS[name];
if (!command) {
  console.error(`Usage: node scripts/cargo.mjs <${Object.keys(COMMANDS).join("|")}> [cargo arguments]`);
  process.exit(2);
}
const scope = extra.some((arg) => arg === "-p" || arg === "--package")
  ? command.scope.filter((arg) => arg !== "--workspace")
  : command.scope;
process.exit(run([name, ...scope, ...extra, ...command.tail]));
