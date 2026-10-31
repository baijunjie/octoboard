#!/usr/bin/env node
// Clippy never starts the sidecar, so linting needs no daemon build.
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const repoRoot = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const result = spawnSync("cargo", ["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"], {
  cwd: repoRoot,
  stdio: "inherit",
  env: { ...process.env, TAURI_CONFIG: JSON.stringify({ bundle: { externalBin: [] } }) },
});
if (result.error) throw result.error;
process.exit(result.status ?? 1);
