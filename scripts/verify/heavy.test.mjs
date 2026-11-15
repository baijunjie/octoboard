import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtempSync, readdirSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { acquireSlot, HELD_ENV } from "./lock.mjs";

const HEAVY = fileURLToPath(new URL("./heavy.mjs", import.meta.url));
// What the command prints: the marker it was given, so a test can tell it ran under a held slot.
const PRINT_MARKER = ["-e", `console.log("held=" + process.env.${HELD_ENV})`];

function scratch(t) {
  const root = mkdtempSync(path.join(os.tmpdir(), "heavy-test-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  return root;
}

// Runs heavy.mjs over `args` with its slots under `root`, one slot; `onStderr` sees the error output as it grows.
function runHeavy(root, args, { env = {}, onStderr = () => {} } = {}) {
  const base = { ...process.env, OCTOBOARD_HEAVY_SLOT_ROOT: root, OCTOBOARD_VERIFY_SLOTS: "1" };
  delete base[HELD_ENV];
  const child = spawn(process.execPath, [HEAVY, ...args], { env: { ...base, ...env }, stdio: ["ignore", "pipe", "pipe"] });
  let stdout = "";
  let stderr = "";
  child.stdout.on("data", (chunk) => (stdout += chunk));
  child.stderr.on("data", (chunk) => {
    stderr += chunk;
    onStderr(stderr);
  });
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      child.kill("SIGKILL");
      reject(new Error(`heavy.mjs did not exit: ${stderr}`));
    }, 20_000);
    child.on("error", reject);
    child.on("close", (code) => {
      clearTimeout(timer);
      resolve({ code, stdout, stderr });
    });
  });
}

test("a heavy command waits for the slot another run holds, then runs marked as holding it", async (t) => {
  const root = scratch(t);
  const release = await acquireSlot("another run", { root, slots: 1 });
  let released = false;
  const result = await runHeavy(root, [process.execPath, ...PRINT_MARKER], {
    onStderr: (stderr) => {
      if (!released && stderr.includes("waiting for a free heavy-step slot")) {
        assert.match(stderr, /running now: another run/);
        released = true;
        release();
      }
    },
  });
  assert.ok(released, "it waited for the held slot");
  assert.equal(result.code, 0);
  assert.equal(result.stdout.trim(), "held=1");
  assert.deepEqual(readdirSync(root).filter((name) => name !== "queue"), [], "its own slot is released");
});

test("a heavy command started under a held slot runs at once", async (t) => {
  const root = scratch(t);
  const release = await acquireSlot("verify", { root, slots: 1 });
  t.after(release);
  const result = await runHeavy(root, [process.execPath, ...PRINT_MARKER], { env: { [HELD_ENV]: "1" } });
  assert.equal(result.code, 0);
  assert.equal(result.stdout.trim(), "held=1");
  assert.doesNotMatch(result.stderr, /waiting/);
});

test("an argument with a space reaches the command as one argument", async (t) => {
  const root = scratch(t);
  const result = await runHeavy(root, [process.execPath, "-e", "console.log(JSON.stringify(process.argv.slice(1)))", "renders the menu", 'say "hi"']);
  assert.equal(result.code, 0);
  assert.deepEqual(JSON.parse(result.stdout), ["renders the menu", 'say "hi"']);
});

test("the command's exit code is passed on, and a missing command fails", async (t) => {
  const root = scratch(t);
  assert.equal((await runHeavy(root, [process.execPath, "-e", "process.exit(3)"])).code, 3);
  assert.equal((await runHeavy(root, ["octoboard-no-such-command"])).code, 1);
  assert.equal((await runHeavy(root, [])).code, 2);
});
