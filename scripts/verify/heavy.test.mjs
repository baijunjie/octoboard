import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { chmodSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
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
function startHeavy(root, args, { env = {}, cwd, onStderr = () => {} } = {}) {
  const base = { ...process.env, OCTOBOARD_HEAVY_SLOT_ROOT: root, OCTOBOARD_VERIFY_SLOTS: "1" };
  delete base[HELD_ENV];
  const child = spawn(process.execPath, [HEAVY, ...args], { env: { ...base, ...env }, cwd, stdio: ["ignore", "pipe", "pipe"] });
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

// Releases `release` (the slot another run holds) once heavy.mjs reports waiting for a slot, for `onStderr`.
function releaseOnWait(release) {
  let released = false;
  return {
    onStderr: (stderr) => {
      if (!released && stderr.includes("waiting for a free heavy-step slot")) {
        released = true;
        release();
      }
    },
    get released() {
      return released;
    },
  };
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

// A `vitest` that only prints what it was started with: its arguments, whether it runs marked as holding the slot,
// and its process group, read from the process table while it is alive.
function fakeVitest(t) {
  const dir = scratch(t);
  const bin = path.join(dir, "vitest");
  writeFileSync(
    bin,
    `#!${process.execPath}
const { execFileSync } = require("node:child_process");
const group = Number(execFileSync("ps", ["-o", "pgid=", "-p", String(process.pid)], { encoding: "utf8" }));
console.log(JSON.stringify({ args: process.argv.slice(2), held: process.env.${HELD_ENV}, pid: process.pid, group }));
`,
  );
  chmodSync(bin, 0o755);
  return bin;
}

// A package directory with the named test files in it, to run heavy.mjs from.
function packageWith(t, names) {
  const dir = scratch(t);
  for (const name of names) {
    mkdirSync(path.dirname(path.join(dir, name)), { recursive: true });
    writeFileSync(path.join(dir, name), "");
  }
  return dir;
}

const unixOnly = { skip: process.platform === "win32" && "the fake vitest is a script with a shebang" };
const printed = (result) => JSON.parse(result.stdout);

test("a run of a few named test files runs at once while another run holds the slot, with its workers capped", unixOnly, async (t) => {
  const root = scratch(t);
  const vitest = fakeVitest(t);
  const cwd = packageWith(t, ["a.test.ts", "src/b.test.tsx"]);
  const release = await acquireSlot("another run", { root, slots: 1 });
  t.after(release);
  const result = await runHeavy(root, [vitest, "run", "a.test.ts", "src/b.test.tsx", "-t", "renders the menu"], { cwd });
  assert.equal(result.code, 0, result.stderr);
  const run = printed(result);
  assert.deepEqual(run.args, ["run", "a.test.ts", "src/b.test.tsx", "-t", "renders the menu", "--maxWorkers=2"]);
  assert.equal(run.held, undefined, "it holds no slot, so what it starts is not marked as holding one");
  assert.match(result.stderr, /heavy: a run of at most 8 named test files; running at once, without waiting for the heavy-step slot/);
  assert.doesNotMatch(result.stderr, /waiting for a free heavy-step slot/);
  assert.deepEqual(slotsHeld(root), ["slot-0"], "only the other run's slot is there");
  assert.equal(JSON.parse(readFileSync(path.join(root, "slot-0", "owner.json"), "utf8")).label, "another run", "its slot is untouched");
});

test("a small run's worker count is capped to what it names, and it takes no slot when none is held", unixOnly, async (t) => {
  const root = scratch(t);
  const vitest = fakeVitest(t);
  const cwd = packageWith(t, ["a.test.ts"]);
  const result = await runHeavy(root, [vitest, "run", "a.test.ts", "--maxWorkers=7"], { cwd });
  assert.equal(result.code, 0, result.stderr);
  assert.deepEqual(printed(result).args, ["run", "a.test.ts", "--maxWorkers=1"]);
  assert.deepEqual(slotsHeld(root), []);
});

test("a small run goes in a process group of its own, and its exit code is passed on", unixOnly, async (t) => {
  const root = scratch(t);
  const cwd = packageWith(t, ["a.test.ts"]);
  const run = printed(await runHeavy(root, [fakeVitest(t), "run", "a.test.ts"], { cwd }));
  assert.equal(run.group, run.pid, "the command leads its own group");
  const failing = path.join(scratch(t), "vitest");
  writeFileSync(failing, `#!${process.execPath}\nprocess.exit(5);\n`);
  chmodSync(failing, 0o755);
  assert.equal((await runHeavy(root, [failing, "run", "a.test.ts"], { cwd })).code, 5);
});

test("a run that is not small still waits for the slot, and gets its arguments as given", unixOnly, async (t) => {
  const cwd = packageWith(t, Array.from({ length: 9 }, (_, index) => `f${index}.test.ts`));
  const nine = Array.from({ length: 9 }, (_, index) => `f${index}.test.ts`);
  const cases = {
    "nine files": ["run", ...nine],
    "no files": ["run"],
    "a directory": ["run", "."],
    "a missing file": ["run", "f0.test.ts", "missing.test.ts"],
    "related": ["related", "f0.test.ts"],
  };
  for (const [name, args] of Object.entries(cases)) {
    const root = scratch(t);
    const vitest = fakeVitest(t);
    const release = await acquireSlot("another run", { root, slots: 1 });
    t.after(release);
    const waiter = releaseOnWait(release);
    const result = await runHeavy(root, [vitest, ...args], { cwd, onStderr: waiter.onStderr });
    assert.ok(waiter.released, `${name} waited for the slot`);
    assert.equal(result.code, 0, `${name}: ${result.stderr}`);
    assert.deepEqual(printed(result).args, args, name);
    assert.equal(printed(result).held, "1", `${name} runs marked as holding the slot`);
    assert.doesNotMatch(result.stderr, /running at once/, name);
  }
});

test("a command that is not vitest waits for the slot even when it names a test file", unixOnly, async (t) => {
  const root = scratch(t);
  const cwd = packageWith(t, ["a.test.ts"]);
  const release = await acquireSlot("another run", { root, slots: 1 });
  t.after(release);
  const waiter = releaseOnWait(release);
  const result = await runHeavy(root, [process.execPath, "-e", "console.log('ran')", "run", "a.test.ts"], { cwd, onStderr: waiter.onStderr });
  assert.ok(waiter.released, "it waited");
  assert.equal(result.stdout.trim(), "ran");
});

test("named files are looked up from the working directory", unixOnly, async (t) => {
  const root = scratch(t);
  const vitest = fakeVitest(t);
  const elsewhere = packageWith(t, []);
  const release = await acquireSlot("another run", { root, slots: 1 });
  t.after(release);
  const waiter = releaseOnWait(release);
  const result = await runHeavy(root, [vitest, "run", "a.test.ts"], { cwd: elsewhere, onStderr: waiter.onStderr });
  assert.ok(waiter.released, "a file that is not there makes it wait");
  assert.deepEqual(printed(result).args, ["run", "a.test.ts"]);
});

test("a small run under an inherited hold runs as it does without one, with its arguments as given", unixOnly, async (t) => {
  const root = scratch(t);
  const cwd = packageWith(t, ["a.test.ts"]);
  const release = await acquireSlot("verify", { root, slots: 1 });
  t.after(release);
  const result = await runHeavy(root, [fakeVitest(t), "run", "a.test.ts"], { cwd, env: { [HELD_ENV]: "1" } });
  assert.equal(result.code, 0, result.stderr);
  assert.deepEqual(printed(result).args, ["run", "a.test.ts"]);
  assert.doesNotMatch(result.stderr, /running at once|waiting for a free heavy-step slot/);
});
