import { expect, test } from "vitest";
import { comparePixels, renderReport, selectScenarios, viewportOf } from "./gallery-compare-report.mjs";
import { parseOptions } from "./gallery-compare.mjs";

const image = (width, height, data) => ({ width, height, data: new Uint8ClampedArray(data) });

test("pixel comparison includes alpha, threshold, bounds and missing pixels", () => {
  const base = image(2, 1, [0, 0, 0, 255, 10, 20, 30, 255]);
  expect(comparePixels(base, image(2, 1, [8, 0, 0, 255, 10, 20, 30, 255]))).toMatchObject({ pixels: 0, percent: 0, bounds: null });
  const changed = comparePixels(base, image(2, 1, [9, 0, 0, 255, 10, 20, 30, 246]));
  expect(changed).toMatchObject({ pixels: 2, percent: 100, bounds: { left: 0, top: 0, right: 1, bottom: 0 } });
  expect([...changed.data]).toEqual([255, 0, 0, 255, 255, 0, 0, 255]);
  expect(comparePixels(image(1, 1, [0, 0, 0, 0]), image(2, 1, [0, 0, 0, 0, 0, 0, 0, 0]))).toMatchObject({ pixels: 1, percent: 50, bounds: { left: 1, right: 1 } });
});

test("selection retains added/removed cases and rejects each unmatched explicit pattern", () => {
  const a = [{ id: "removed", group: "Viewer" }, { id: "same", group: "Viewer" }];
  const b = [{ id: "added", group: "Settings" }, { id: "same", group: "Settings" }];
  expect(selectScenarios(a, b).map((s) => s.id)).toEqual(["removed", "same", "added"]);
  expect(selectScenarios(a, b, { group: "viewer", patterns: ["*"] }).map((s) => s.id)).toEqual(["removed", "same"]);
  expect(() => selectScenarios(a, b, { patterns: ["same", "missing"] })).toThrow("No scenario matches: missing");
  expect(() => selectScenarios(a, b, { group: "nothing" })).toThrow("No scenarios selected");
});

test("both sides share a viewport and honor either side's minimum", () => {
  expect(viewportOf({ base: { width: 600, minWidth: 1100 }, current: { width: 800, height: 300 } }, 420)).toEqual({ width: 1100, height: 400 });
  expect(viewportOf({ base: {}, current: {} }, 420)).toEqual({ width: 420, height: 900 });
});

test("report escapes untrusted titles/errors and exposes incomplete/error results", () => {
  const html = renderReport({ results: [{ id: "<script>", title: '"<img>', language: "en", theme: "light", viewport: { width: 100, height: 100 }, status: "error", error: "<svg onload=alert(1)>", images: { current: "00001.current.png" } }], complete: false, threshold: 8, baseUrl: "http://localhost:1", url: "http://localhost:2" });
  expect(html).toContain("Incomplete");
  expect(html).toContain("1 error");
  expect(html).toContain("&lt;script&gt;");
  expect(html).toContain("&lt;svg onload=alert(1)&gt;");
  expect(html).toContain('src="00001.current.png"');
  expect(html).not.toContain("<script>");
  expect(html).not.toContain("<img>");
});

const required = ["--base-url", "http://localhost:5192", "--url", "http://localhost:5191", "--output", "/tmp/gallery-compare-test"];
test("CLI defaults are small and origins/threshold/selection options are validated", () => {
  expect(parseOptions(required)).toMatchObject({ languages: ["en"], themes: ["light"], threshold: 8, patterns: [] });
  expect(parseOptions([...required, "--theme", "both", "--lang", "en,zh-Hans", "viewer-*"])).toMatchObject({ themes: ["light", "dark"], languages: ["en", "zh-Hans"], patterns: ["viewer-*"] });
  expect(() => parseOptions([...required, "--threshold", "256"])).toThrow("--threshold");
  expect(() => parseOptions([...required, "--width", "0"])).toThrow("--width");
  expect(() => parseOptions([...required, "--url", "http://name:password@localhost:1"])).toThrow("origin without credentials");
  expect(() => parseOptions([...required, "--url", "file:///tmp"])).toThrow("HTTP(S)");
  expect(() => parseOptions([...required, "--url", "http://localhost:1/path"])).toThrow("origin without credentials");
});

test("CLI writes report and artifacts, preserves errors, and refuses overwrites", async () => {
  const { spawnSync } = await import("node:child_process");
  const { mkdtempSync, readFileSync, rmSync } = await import("node:fs");
  const os = await import("node:os");
  const path = await import("node:path");
  const root = mkdtempSync(path.join(os.tmpdir(), "gallery-compare-test-"));
  try {
    for (const [mode, code, status] of [["normal", 0, "unchanged"], ["changed", 1, "changed"], ["render-error", 2, "error"], ["context-close-error", 2, "error"], ["diff-close-hang", 2, "error"], ["read-close-hang", 2, undefined]]) {
      const output = path.join(root, mode);
      const args = ["--no-warnings", "--experimental-loader", new URL("./test-fixtures/gallery-compare-loader.mjs", import.meta.url).href, new URL("./gallery-compare.mjs", import.meta.url).pathname, "--base-url", "http://localhost:1", "--url", "http://localhost:2", "--output", output, "--timeout", "100"];
      const result = spawnSync(process.execPath, args, { encoding: "utf8", timeout: 10_000, env: { ...process.env, GALLERY_COMPARE_TEST_MODE: mode, OCTOBOARD_COMPARE_STATE_DIR: path.join(root, "state") } });
      expect(result.status, result.stdout + result.stderr).toBe(code);
      const report = JSON.parse(readFileSync(path.join(output, "results.json"), "utf8"));
      if (status) expect(report).toMatchObject({ complete: !["context-close-error", "diff-close-hang"].includes(mode), results: [{ status }] });
      else expect(report).toMatchObject({ complete: false, results: [] });
      if (mode === "context-close-error") expect(report.results[0].images.base).toBe("00001.base.png");
      if (mode === "diff-close-hang") {
        expect(report.results).toHaveLength(1);
        expect(report.results[0]).toMatchObject({ id: "test", status: "error", images: { base: "00001.base.png", current: "00001.current.png" } });
        expect(result.stdout.match(/captured fixture image/g)).toHaveLength(2);
      }
      if (mode.endsWith("close-hang")) expect(JSON.stringify(report)).toContain("closing browser page timed out");
      expect(result.stdout).toContain("closed browser");
      expect(readFileSync(path.join(output, "index.html"), "utf8")).toContain("Gallery comparison");
      const again = spawnSync(process.execPath, args, { encoding: "utf8", timeout: 10_000 });
      expect(again.status).toBe(2);
      expect(again.stdout).not.toContain("opened browser");
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
}, 40_000);

test("interrupting while queued writes an incomplete report and removes the wait ticket", async () => {
  const { spawn } = await import("node:child_process");
  const { mkdtempSync, readFileSync, readdirSync, rmSync } = await import("node:fs");
  const os = await import("node:os");
  const path = await import("node:path");
  const { acquireSlot } = await import("../../../scripts/verify/lock.mjs");
  const root = mkdtempSync(path.join(os.tmpdir(), "gallery-compare-signal-"));
  const release = await acquireSlot("test holder", { root: path.join(root, "slots"), slots: 1 });
  try {
    const child = spawn(process.execPath, [new URL("./gallery-compare.mjs", import.meta.url).pathname, "--base-url", "http://localhost:1", "--url", "http://localhost:2", "--output", path.join(root, "output")], { env: { ...process.env, OCTOBOARD_COMPARE_STATE_DIR: root, OCTOBOARD_VERIFY_SLOTS: "1" }, stdio: ["ignore", "pipe", "pipe"] });
    let output = "";
    const result = await new Promise((resolve, reject) => {
      const timer = setTimeout(() => { child.kill("SIGKILL"); reject(new Error(`Did not exit: ${output}`)); }, 10_000);
      child.stderr.on("data", (chunk) => { output += chunk; if (output.includes("Waiting for a free heavy-step slot")) child.kill("SIGTERM"); });
      child.on("error", reject);
      child.on("close", (code) => { clearTimeout(timer); resolve(code); });
    });
    expect(result).toBe(143);
    expect(JSON.parse(readFileSync(path.join(root, "output/results.json"), "utf8"))).toMatchObject({ complete: false, error: "Interrupted by SIGTERM" });
    expect(readdirSync(path.join(root, "slots/queue"))).toEqual([]);
  } finally { release(); rmSync(root, { recursive: true, force: true }); }
});

test("interrupting a capture retains its pending status instead of reporting an unchanged result", async () => {
  const { spawn } = await import("node:child_process");
  const { mkdtempSync, readFileSync, rmSync } = await import("node:fs");
  const os = await import("node:os");
  const path = await import("node:path");
  const root = mkdtempSync(path.join(os.tmpdir(), "gallery-compare-pending-"));
  try {
    const child = spawn(process.execPath, ["--no-warnings", "--experimental-loader", new URL("./test-fixtures/gallery-compare-loader.mjs", import.meta.url).href, new URL("./gallery-compare.mjs", import.meta.url).pathname, "--base-url", "http://localhost:1", "--url", "http://localhost:2", "--output", path.join(root, "output")], { env: { ...process.env, GALLERY_COMPARE_TEST_MODE: "interrupt-capture", OCTOBOARD_COMPARE_STATE_DIR: root }, stdio: ["ignore", "pipe", "pipe"] });
    let output = "";
    const code = await new Promise((resolve, reject) => {
      const timer = setTimeout(() => { child.kill("SIGKILL"); reject(new Error(`Did not exit: ${output}`)); }, 10_000);
      child.stdout.on("data", (chunk) => { output += chunk; if (output.includes("ready to interrupt capture")) child.kill("SIGTERM"); });
      child.on("error", reject);
      child.on("close", (code) => { clearTimeout(timer); resolve(code); });
    });
    expect(code).toBe(143);
    expect(JSON.parse(readFileSync(path.join(root, "output/results.json"), "utf8"))).toMatchObject({ complete: false, error: "Interrupted by SIGTERM", results: [{ status: "pending" }] });
    expect(readFileSync(path.join(root, "output/index.html"), "utf8")).toContain("1 pending");
  } finally { rmSync(root, { recursive: true, force: true }); }
});
