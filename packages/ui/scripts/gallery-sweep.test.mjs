import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

import { expect, test } from "vitest";

const script = fileURLToPath(new URL("./gallery-sweep.mjs", import.meta.url));
const loader = new URL("./test-fixtures/gallery-sweep-loader.mjs", import.meta.url).href;

function run(mode, args = ["--list"]) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, ["--no-warnings", "--experimental-loader", loader, script, ...args], {
      env: { ...process.env, GALLERY_TEST_MODE: mode },
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
