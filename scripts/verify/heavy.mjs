#!/usr/bin/env node
// Runs a heavy command (Rust tests, clippy, the UI's test suites) once it holds a machine-wide slot of
// `lock.mjs`, the one `pnpm verify` takes for its heavy steps: `node scripts/verify/heavy.mjs <command>
// [arguments]`. The package scripts an agent runs to check its own change (`pnpm test:rust`, `pnpm --filter
// @octoboard/ui test`) go through it, so several sessions testing at once queue instead of each compiling
// or running a suite side by side, which ran a 16 GiB machine out of memory.
//
// A command started while a slot is already held for it (`HELD_ENV` set, as `pnpm verify` does around its
// heavy steps) runs at once, so a step does not wait on the slot its own run holds. The command gets the same
// marker, so a heavy script it starts in turn does not wait either. `OCTOBOARD_HEAVY_SLOT_ROOT` moves the
// slots elsewhere, for tests.
//
// A command that took the slot itself runs in a process group of its own (`process-group.mjs`): a signal goes
// to the whole group, nothing of it outlives the run, and a command that stops making progress is reported and
// ended rather than holding the slot. One started under a held slot runs inside the holder's group instead.
import { spawn } from "node:child_process";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { acquireSlot, defaultRoot, describeWait, HELD_ENV } from "./lock.mjs";
import { runInGroup, stallMs } from "./process-group.mjs";

const repoRoot = path.dirname(path.dirname(path.dirname(fileURLToPath(import.meta.url))));
const [command, ...args] = process.argv.slice(2);
if (!command) {
  console.error("Usage: node scripts/verify/heavy.mjs <command> [arguments]");
  process.exit(2);
}

let release = () => {};
const holdsOwnSlot = !process.env[HELD_ENV];
const label = `${path.basename(repoRoot)}: ${[command, ...args].join(" ")}`;
if (holdsOwnSlot) {
  release = await acquireSlot(label, {
    root: process.env.OCTOBOARD_HEAVY_SLOT_ROOT || defaultRoot,
    onWait: (holders, ahead) => console.error(`heavy: waiting for a free heavy-step slot; ${describeWait(holders, ahead)}`),
  });
}

const env = { ...process.env, [HELD_ENV]: "1" };
if (holdsOwnSlot && process.platform !== "win32") {
  const { code } = await runInGroup(command, args, { env, label, stallMs: stallMs(), onGroup: release.recordGroup });
  release();
  process.exit(code);
}

// Windows resolves a package binary (`vitest`) only through its `.cmd` shim, which needs a shell. Node joins
// a shell command's arguments with bare spaces, so each is quoted for `cmd.exe` here and the line is passed
// whole: an argument with a space (`-t "renders the menu"`) stays one argument.
const quoteForCmd = (arg) => `"${arg.replaceAll('"', '""')}"`;
const child = process.platform === "win32"
  ? spawn([command, ...args].map(quoteForCmd).join(" "), { stdio: "inherit", env, shell: true })
  : spawn(command, args, { stdio: "inherit", env });
// A signal goes to the command, and the slot is released only once it has closed, so it is not handed on
// while the command's own processes still run.
let signaled = null;
const forward = (signal) => {
  signaled = signal;
  // Windows can deliver only SIGINT, SIGTERM and SIGKILL to a child; its SIGHUP (the console closing) is sent
  // on as SIGTERM.
  child.kill(process.platform === "win32" && signal === "SIGHUP" ? "SIGTERM" : signal);
};
for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) process.on(signal, forward);
child.on("error", (error) => {
  console.error(`heavy: could not run ${command}: ${error.message}`);
  release();
  process.exit(1);
});
child.on("close", (code, signal) => {
  release();
  const ended = signal ?? signaled;
  process.exit(code ?? (ended ? 128 + (os.constants.signals[ended] ?? 0) : 1));
});
