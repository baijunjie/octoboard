#!/usr/bin/env node
// Runs `cargo test`, `cargo clippy` or `cargo check` over the root Cargo workspace, so both crates build
// once with one feature set: `node scripts/cargo.mjs <test|clippy|check> [cargo arguments]`. Extra arguments
// go to cargo (`pnpm test:rust some_test_name` filters by test name; for `clippy`, which ends in the
// lint flags, they come before them). With `-p <crate>` or `--package <crate>` among them (the
// `--package=<crate>` form is not recognised) only `--workspace` is dropped, so the command covers just that
// crate with its other flags (`--all-targets`) kept; `pnpm verify --quick` uses this for a change to one crate.
//
// The desktop crate's `build.rs` makes `tauri-build` refuse to build when the sidecar named by
// `bundle.externalBin` or a file named by `bundle.resources` (the Windows installer's ConPTY files, which
// `pnpm fetch:conpty` lays out) is missing, which is what a real build wants. None of these commands starts
// the sidecar or bundles anything, so this script overrides both to empty through `TAURI_CONFIG` and they
// need no daemon build and no fetched files. Only this script sets the override: a plain `cargo build` or
// `pnpm tauri build` still fails without them.
//
// `check-windows` cross-checks for `x86_64-pc-windows-msvc` through `cargo xwin` (`cargo install cargo-xwin`,
// `rustup target add x86_64-pc-windows-msvc`, and clang/LLVM on the PATH for the C build scripts; it downloads
// the MSVC CRT and Windows SDK on first use, so run it with `XWIN_ACCEPT_LICENSE=1` to accept their license).
// Both crates gate: the desktop crate is checked first and then the daemon, and the exit code is non-zero
// when either fails. `pnpm check:windows desktop` or `pnpm check:windows daemon` checks just one crate.
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const repoRoot = path.dirname(path.dirname(fileURLToPath(import.meta.url)));

const COMMANDS = {
  test: { scope: ["--workspace"], tail: [] },
  clippy: { scope: ["--workspace", "--all-targets"], tail: ["--", "-D", "warnings"] },
  check: { scope: ["--workspace", "--all-targets"], tail: [] },
};

const WINDOWS_CRATES = { desktop: "octoboard", daemon: "octoboardd" };
const WINDOWS_TARGET = "x86_64-pc-windows-msvc";
const env = { ...process.env, TAURI_CONFIG: JSON.stringify({ bundle: { externalBin: [], resources: [] } }) };

const run = (args) => {
  const result = spawnSync("cargo", args, { cwd: repoRoot, stdio: "inherit", env });
  if (result.error) throw result.error;
  return result.status ?? 1;
};

const [name, ...extra] = process.argv.slice(2);
if (name === "check-windows") {
  const [which, ...rest] = extra;
  const only = WINDOWS_CRATES[which] ? which : undefined;
  if (which && !only) {
    console.error("Usage: node scripts/cargo.mjs check-windows [desktop|daemon] [cargo arguments]");
    process.exit(2);
  }
  const checkCrate = (crate) =>
    run([
      "xwin",
      "check",
      "--target",
      WINDOWS_TARGET,
      "--all-targets",
      "-p",
      WINDOWS_CRATES[crate],
      ...(only ? rest : extra),
    ]);
  if (only) process.exit(checkCrate(only));
  const desktop = checkCrate("desktop");
  const daemon = checkCrate("daemon");
  process.exit(desktop !== 0 ? desktop : daemon);
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
