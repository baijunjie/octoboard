import { spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { afterAll, expect, test } from "vitest";

const script = fileURLToPath(new URL("./gallery-sweep.mjs", import.meta.url));
const loader = new URL("./test-fixtures/gallery-sweep-loader.mjs", import.meta.url).href;

// Slots and records of clean pages are kept here, not in the machine-wide places: a sweep started by a
// test run that holds the heavy-step slot would otherwise wait for it forever. A run is given a state
// of its own unless it is one of several that share records.
const stateDir = mkdtempSync(path.join(os.tmpdir(), "gallery-sweep-state-"));
afterAll(() => rmSync(stateDir, { recursive: true, force: true }));
const newState = () => mkdtempSync(path.join(stateDir, "state-"));

function run(mode, args = ["--list"], state = newState(), onOutput = () => {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, ["--no-warnings", "--experimental-loader", loader, script, ...args], {
      env: { ...process.env, GALLERY_TEST_MODE: mode, OCTOBOARD_SWEEP_STATE_DIR: state, OCTOBOARD_VERIFY_SLOTS: "1" },
      stdio: ["ignore", "pipe", "pipe"],
    });
    let output = "";
    let signalled = false;
    const timer = setTimeout(() => {
      child.kill("SIGKILL");
      reject(new Error(`Gallery process did not exit: ${output}`));
    }, 12_000);
    child.on("error", reject);
    for (const stream of [child.stdout, child.stderr]) {
      stream.on("data", (chunk) => {
        output += chunk;
        onOutput(output);
        if (mode === "signal" && !signalled && output.includes("ready for signal")) {
          signalled = true;
          child.kill("SIGTERM");
        }
      });
    }
    child.on("close", (code, signal) => {
      clearTimeout(timer);
      resolve({ code, signal, output });
    });
  });
}

test.each([
  ["normal", ["--list"], 0],
  ["normal", ["--lang", "en"], 0],
  ["scenario-failure", ["--lang", "en"], 1],
  ["launch-failure", ["--list"], 2],
  ["listen-failure", ["--list"], 2],
])("%s exits with its result and closes acquired resources", async (mode, args, code) => {
  const result = await run(mode, args);
  expect(result).toMatchObject({ code, signal: null });
  expect(result.output).toContain("closed server");
  if (!["launch-failure", "listen-failure"].includes(mode)) expect(result.output).toContain("closed browser");
});

test.each(["hang-browser", "reject-browser", "hang-server", "reject-server"])("%s cannot strand another resource or report success", async (mode) => {
  const result = await run(mode);
  expect(result).toMatchObject({ code: 2, signal: null });
  expect(result.output).toContain("test\n");
  expect(result.output).toContain("closed server");
  expect(result.output).toContain("closed browser");
  expect(result.output).toContain(`Cleanup failed for ${mode.endsWith("browser") ? "Chromium" : "Vite server"}:`);
}, 15_000);

test("an interrupt closes acquired resources and retains the signal exit code", async () => {
  const result = await run("signal");
  expect(result).toMatchObject({ code: 143, signal: null });
  expect(result.output).toContain("closed server");
});

test("cleanup failure retains a scenario failure's exit code", async () => {
  const result = await run("scenario-failure-reject-browser", ["--lang", "en"]);
  expect(result).toMatchObject({ code: 1, signal: null });
  expect(result.output).toContain("1 failed");
  expect(result.output).toContain("Cleanup failed for Chromium:");
});

test("a page that came out clean is skipped on the same content, until --fresh asks for it again", async () => {
  const state = newState();
  const first = await run("normal", ["--lang", "en"], state);
  expect(first).toMatchObject({ code: 0 });
  expect(first.output).toContain("1 clean, 0 failed");
  const second = await run("normal", ["--lang", "en"], state);
  expect(second).toMatchObject({ code: 0 });
  expect(second.output).toContain("skipping 1 that came out clean");
  expect(second.output).toContain("Nothing to load");
  const fresh = await run("normal", ["--lang", "en", "--fresh"], state);
  expect(fresh.output).toContain("1 clean, 0 failed");
  expect(fresh.output).not.toContain("skipping");
  const otherLanguage = await run("normal", ["--lang", "en,zh-Hans"], state);
  expect(otherLanguage.output).toContain("skipping 1 that came out clean");
  expect(otherLanguage.output).toContain("1 clean, 0 failed");
});

test("a failed page is not recorded", async () => {
  const state = newState();
  expect(await run("scenario-failure", ["--lang", "en"], state)).toMatchObject({ code: 1 });
  const again = await run("scenario-failure", ["--lang", "en"], state);
  expect(again).toMatchObject({ code: 1 });
  expect(again.output).not.toContain("skipping");
});

test("a gallery served from elsewhere neither reads nor writes records", async () => {
  const state = newState();
  await run("normal", ["--lang", "en"], state);
  const url = await run("normal", ["--lang", "en", "--url", "http://localhost:1"], state);
  expect(url.output).toContain("1 clean, 0 failed");
  expect(url.output).not.toContain("skipping");
  const freshState = newState();
  await run("normal", ["--lang", "en", "--url", "http://localhost:1"], freshState);
  expect((await run("normal", ["--lang", "en"], freshState)).output).not.toContain("skipping");
});

test("a page load waits while every heavy-step slot is held", async () => {
  const state = newState();
  const { acquireSlot } = await import("../../../scripts/verify/lock.mjs");
  const release = await acquireSlot("another worktree: ui tests", { root: path.join(state, "slots"), slots: 1 });
  const result = await run("normal", ["--lang", "en", "--fresh"], state, (output) => {
    if (output.includes("Waiting for a free heavy-step slot")) release();
  });
  expect(result).toMatchObject({ code: 0 });
  expect(result.output).toContain("Waiting for a free heavy-step slot; running now: another worktree: ui tests");
}, 20_000);
