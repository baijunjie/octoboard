#!/usr/bin/env node
// Runs `cargo test`, `cargo clippy` or `cargo check` over the root Cargo workspace, so both crates build
// once with one feature set: `node scripts/cargo.mjs <test|clippy|check> [cargo arguments]`. Extra arguments
// go to cargo (`pnpm test:rust some_test_name` filters by test name; for `clippy`, which ends in the
// lint flags, they come before them). With `-p <crate>` or `--package <crate>` among them (the
// `--package=<crate>` form is not recognised) only `--workspace` is dropped, so the command covers just that
// crate with its other flags (`--all-targets`) kept; `pnpm verify --quick` uses this for a change to one crate.
//
// The desktop crate's `build.rs` makes `tauri-build` refuse to build when the sidecar named by
// `bundle.externalBin` in `tauri.conf.json` has not been built, which is what a real build wants.
// None of these commands starts the sidecar, so this script overrides that list to empty through
// `TAURI_CONFIG` and they need no daemon build. Only this script sets the override: a plain
// `cargo build` or `pnpm tauri build` still fails without the sidecar.
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const repoRoot = path.dirname(path.dirname(fileURLToPath(import.meta.url)));

const COMMANDS = {
  test: { scope: ["--workspace"], tail: [] },
  clippy: { scope: ["--workspace", "--all-targets"], tail: ["--", "-D", "warnings"] },
  check: { scope: ["--workspace", "--all-targets"], tail: [] },
};

const [name, ...extra] = process.argv.slice(2);
const command = COMMANDS[name];
if (!command) {
  console.error(`Usage: node scripts/cargo.mjs <${Object.keys(COMMANDS).join("|")}> [cargo arguments]`);
  process.exit(2);
}
const scope = extra.some((arg) => arg === "-p" || arg === "--package")
  ? command.scope.filter((arg) => arg !== "--workspace")
  : command.scope;
const result = spawnSync("cargo", [name, ...scope, ...extra, ...command.tail], {
  cwd: repoRoot,
  stdio: "inherit",
  env: { ...process.env, TAURI_CONFIG: JSON.stringify({ bundle: { externalBin: [] } }) },
});
if (result.error) throw result.error;
process.exit(result.status ?? 1);
