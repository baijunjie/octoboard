#!/usr/bin/env node
// Runs the Rust tests of both crates of the root Cargo workspace in one `cargo test` invocation, so
// the dependencies they share build once with one feature set. Extra arguments go to `cargo test`
// (`pnpm test:rust some_test_name` filters by test name).
//
// The desktop crate's `build.rs` makes `tauri-build` refuse to build when the sidecar named by
// `bundle.externalBin` in `tauri.conf.json` has not been built, which is what a real build wants.
// The tests never start the sidecar, so this run overrides that list to empty through `TAURI_CONFIG`
// and needs no daemon build. Only the test and clippy scripts set this override: a plain `cargo build` or
// `pnpm tauri build` still fails without the sidecar.
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const repoRoot = path.dirname(path.dirname(fileURLToPath(import.meta.url)));

const result = spawnSync("cargo", ["test", "--workspace", ...process.argv.slice(2)], {
  cwd: repoRoot,
  stdio: "inherit",
  env: { ...process.env, TAURI_CONFIG: JSON.stringify({ bundle: { externalBin: [] } }) },
});
if (result.error) throw result.error;
process.exit(result.status ?? 1);
