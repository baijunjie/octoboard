// Shared by the tests of `process-group.mjs`, `heavy.mjs` and `lock.mjs`: starts the stand-in command of
// `processTestFixture.mjs`, finds what it started, and makes sure none of it outlives the test, whether the test
// passed or not.
import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const FIXTURE = fileURLToPath(new URL("./processTestFixture.mjs", import.meta.url));
// How long a test waits for something a process does before it gives up. Only a failing test waits this long.
export const PATIENCE_MS = 60_000;
// How long a fixture process lives at most, whatever happens to the test that started it.
export const LIFETIME_MS = 2 * PATIENCE_MS;
const POLL_MS = 25;

// Polls `condition` until it returns something truthy and returns that, or fails after `PATIENCE_MS`. This is the
// only real-time wait the process tests make: it returns as soon as the condition holds, and the condition is the
// very thing the test asserts.
export async function until(condition, what) {
  const deadline = Date.now() + PATIENCE_MS;
  for (;;) {
    const value = await condition();
    if (value) return value;
    if (Date.now() >= deadline) throw new Error(`timed out waiting for ${what}`);
    await new Promise((resolve) => setTimeout(resolve, POLL_MS));
  }
}

// Whether a process with this pid exists (a zombie counts until it is reaped, which the parent does at once).
export function alive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error.code === "EPERM";
  }
}

export const gone = (pid, what = `process ${pid}`) => until(() => !alive(pid), `${what} to be gone`);

// Kills `pid` and its whole group, silently.
function killAll(pid) {
  for (const target of [pid, -pid]) {
    try {
      process.kill(target, "SIGKILL");
    } catch {
      // Gone already.
    }
  }
}

// A scratch directory removed when the test ends, after everything tracked with `track` is killed.
export function scratch(t) {
  const dir = mkdtempSync(path.join(os.tmpdir(), "process-test-"));
  const tracked = new Set();
  t.after(() => {
    for (const pid of tracked) killAll(pid);
    // The fixture's own helpers, from the info file it wrote.
    for (const pid of Object.values(readInfo(path.join(dir, "info.json")) ?? {})) killAll(pid);
    rmSync(dir, { recursive: true, force: true });
  });
  return { dir, info: path.join(dir, "info.json"), track: (pid) => tracked.add(pid) };
}

function readInfo(file) {
  try {
    return JSON.parse(readFileSync(file, "utf8"));
  } catch {
    return null;
  }
}

// The pids the fixture reported once it was ready: `{ pid, sleep, stubborn?, sccache?, sccacheChild? }`.
export const reported = (area) => until(() => readInfo(area.info), "the fixture to report its processes");

// The arguments that run the fixture in `mode`.
export function fixtureArgs(area, mode, extra = {}) {
  return [FIXTURE, JSON.stringify({ mode, dir: area.dir, info: area.info, ...extra })];
}

// Starts the fixture as the leader of a group of its own, outside any runner, the way a runner leaves one
// behind when it is killed. Answers `{ pid, exited }`, `exited` resolving with `{ code, signal }` once the leader
// is gone and reaped.
export function startGroup(area, mode, extra) {
  const child = spawn(process.execPath, fixtureArgs(area, mode, { watchParent: true, ...extra }), { stdio: "ignore", detached: true });
  area.track(child.pid);
  const exited = new Promise((resolve) => child.on("exit", (code, signal) => resolve({ code, signal })));
  return { pid: child.pid, child, exited };
}

// The names of the signals the stand-in logger `name` (`sccache`, `sccache-child`) received, one per line.
function loggedSignals(area, name) {
  try {
    return readFileSync(path.join(area.dir, `${name}.log`), "utf8").split("\n").filter(Boolean);
  } catch {
    return [];
  }
}

// Runs `action` with `PATH` empty, so that `ps` cannot run.
export function withoutPs(action) {
  const saved = process.env.PATH;
  process.env.PATH = "";
  try {
    return action();
  } finally {
    process.env.PATH = saved;
  }
}

// Whether the fixture has reported its processes, for a runner's `armed` option: the stall window of a command
// that is still starting must not run.
export const armedWhenReported = (area) => () => existsSync(area.info);

// Asserts, through `assert`, that the stand-in shared server and what it runs were sent no signal by what ran
// before: it signals each of them itself with SIGHUP, which no runner sends, and checks that it is the only signal
// each saw (signals are handled in the order they arrive, so an earlier one from the runner would be logged before
// it). Without the probe, a process that was signaled and has not died yet would look untouched.
export async function assertSharedServerSpared(area, assert) {
  const info = await reported(area);
  for (const [name, pid] of [["sccache", info.sccache], ["sccache-child", info.sccacheChild]]) {
    assert.ok(alive(pid), `${name} is still running`);
    process.kill(pid, "SIGHUP");
    await until(() => loggedSignals(area, name).length > 0, `${name} to receive the probe`);
    assert.deepEqual(loggedSignals(area, name), ["SIGHUP"], name);
  }
}
