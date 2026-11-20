import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { mkdtempSync, readdirSync, readFileSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { acquireSlot, HELD_ENV } from "./lock.mjs";
import { STALL_MS, STALL_SETTING } from "./process-group.mjs";
import { alive, fixtureArgs, gone, PATIENCE_MS, reported, scratch as processScratch, until } from "./processTestSupport.mjs";

const HEAVY = fileURLToPath(new URL("./heavy.mjs", import.meta.url));
// What the command prints: the marker it was given, so a test can tell it ran under a held slot.
const PRINT_MARKER = ["-e", `console.log("held=" + process.env.${HELD_ENV})`];

function scratch(t) {
  const root = mkdtempSync(path.join(os.tmpdir(), "heavy-test-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  return root;
}

// Starts heavy.mjs over `args` with its slots under `root`, one slot; `onStderr` sees the error output as it grows.
// Answers the process and `done`, which resolves with its exit code and output.
function startHeavy(root, args, { env = {}, onStderr = () => {} } = {}) {
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
  const done = new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      child.kill("SIGKILL");
      reject(new Error(`heavy.mjs did not exit: ${stderr}`));
    }, PATIENCE_MS);
    child.on("error", reject);
    child.on("close", (code, signal) => {
      clearTimeout(timer);
      resolve({ code, signal, stdout, stderr });
    });
  });
  return { child, done };
}

const runHeavy = (root, args, options) => startHeavy(root, args, options).done;
const slotsHeld = (root) => readdirSync(root).filter((name) => name !== "queue");

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

// The fixture, which never finishes, as the heavy command.
function idleCommand(area, extra) {
  return [process.execPath, ...fixtureArgs(area, "idle", extra)];
}

// The stall window an environment gives: a run that is not about stalling gets the default, whatever the
// machine's own setting says.
const noStall = { [STALL_SETTING]: String(STALL_MS / 1000) };

// The process group heavy.mjs recorded in its slot for the command it runs.
function recordedGroup(root) {
  return until(() => {
    try {
      return JSON.parse(readFileSync(path.join(root, "slot-0", "owner.json"), "utf8")).group?.pid;
    } catch {
      return undefined;
    }
  }, "the slot to record the command's group");
}

test("a command that makes no progress is reported, ended with everything it started, and the slot freed", async (t) => {
  const root = scratch(t);
  const area = processScratch(t);
  // The fixture's start is not waited for: the window may run out before it has started its helpers, and the outcome
  // is the same. What it started is read from the report, which lists the processes it ended.
  const result = await runHeavy(root, idleCommand(area), { env: { [STALL_SETTING]: "1" } });
  const listed = [...result.stderr.matchAll(/^\s+pid (\d+): /gm)].map((match) => Number(match[1]));
  t.after(() => {
    for (const pid of listed) {
      try {
        process.kill(pid, "SIGKILL");
      } catch {
        // Gone.
      }
    }
  });
  assert.equal(result.code, 124);
  assert.match(result.stderr, /heavy: .*made no progress for 1s/);
  assert.match(result.stderr, /was ended for making no progress \(exit 124\)/);
  assert.ok(listed.length > 0, "the report lists the command's processes");
  for (const pid of listed) await gone(pid, `process ${pid} the report listed`);
  assert.deepEqual(slotsHeld(root), [], "the slot is released");
});

test("a signal sent to heavy.mjs goes to everything the command started, and the command's exit follows it", async (t) => {
  // SIGTERM with a descendant that ignores it: the run still ends it, and nothing outlives the run.
  for (const [signal, code, extra] of [["SIGINT", 130, {}], ["SIGHUP", 129, {}], ["SIGTERM", 143, { stubborn: true }]]) {
    const root = scratch(t);
    const area = processScratch(t);
    const { child, done } = startHeavy(root, idleCommand(area, extra), { env: noStall });
    const info = await reported(area);
    child.kill(signal);
    const result = await done;
    assert.equal(result.code, code, signal);
    await gone(info.pid, `the command after ${signal}`);
    await gone(info.sleep, `what it started after ${signal}`);
    if (info.stubborn) await gone(info.stubborn, "the descendant that ignores the signal");
    assert.deepEqual(slotsHeld(root), [], `the slot is released after ${signal}`);
  }
});

test("what a killed heavy.mjs left running is ended by the next run that takes its slot", async (t) => {
  const root = scratch(t);
  const area = processScratch(t);
  const { child } = startHeavy(root, idleCommand(area), { env: noStall });
  const info = await reported(area);
  assert.equal(await recordedGroup(root), info.pid, "the slot records the command's group");
  child.kill("SIGKILL");
  // Not `done`: the command still holds heavy.mjs's output pipes, so they do not close until it is ended.
  await once(child, "exit");
  assert.ok(alive(info.pid) && alive(info.sleep), "SIGKILL left the command and what it started running");
  (await acquireSlot("next run", { root, slots: 1, pollMs: 25 }))();
  await gone(info.pid, "the command the killed run left");
  await gone(info.sleep, "what it started");
});
