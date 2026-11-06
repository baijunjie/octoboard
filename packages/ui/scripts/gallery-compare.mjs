// Explicitly invoked visual comparison; never starts a server or updates screenshot baselines.
import { mkdirSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { acquireSlot, describeWait } from "../../../scripts/verify/lock.mjs";
import { launchBrowser, loadChromium } from "./gallery-browser.mjs";
import { comparePixels, renderReport, selectScenarios, viewportOf } from "./gallery-compare-report.mjs";

const USAGE = `Usage: pnpm gallery:compare --base-url <origin> --url <origin> --output <new-directory> [scenario ...]

Compare two already served galleries and write index.html, results.json and PNG images.
No servers, builds or worktrees are started. This command is not an automatic check.

  scenario              Scenario id or pattern with *; repeatable. Default: union of both catalogs.
  --group <text>        Select groups containing this text on either side.
  --lang <list>         Comma-separated languages offered by both galleries. Default: en.
  --theme <name>        light (default), dark or both.
  --width <pixels>      Viewport width; a scenario's own width wins; both sides share a viewport.
  --threshold <0..255>  Largest RGBA channel difference ignored. Default: 8.
  --timeout <ms>        Per-operation timeout. Default: 60000.
  --playwright <dir>    Alternate Playwright installation (also OCTOBOARD_PLAYWRIGHT).
  --chromium <path>     Existing Chromium executable (also OCTOBOARD_CHROMIUM).
  -h, --help            Show this text.

One page is captured at a time, under the machine-wide heavy-step lock used by pnpm verify.
Exit: 0 no differences; 1 changed/added/removed; 2 render/setup/cleanup error; 128+signal interrupted.
A pixel difference is a review finding, not proof of a regression. Compare on the same machine.
`;

export function parseOptions(args) {
  const { values, positionals } = parseArgs({ args, allowPositionals: true, options: {
    "base-url": { type: "string" }, url: { type: "string" }, output: { type: "string" },
    group: { type: "string" }, lang: { type: "string" }, theme: { type: "string" }, width: { type: "string" },
    threshold: { type: "string" }, timeout: { type: "string" }, playwright: { type: "string" }, chromium: { type: "string" },
    help: { type: "boolean", short: "h" },
  } });
  if (values.help) return { help: true };
  const origin = (key) => {
    if (!values[key]) throw new Error(`--${key} is required`);
    const url = new URL(values[key]);
    if (!["http:", "https:"].includes(url.protocol) || url.username || url.password || url.pathname !== "/" || url.search || url.hash) throw new Error(`--${key} must be an HTTP(S) origin without credentials, path or query`);
    return url.origin;
  };
  const baseUrl = origin("base-url"), url = origin("url");
  if (!values.output) throw new Error("--output is required and must name a new directory");
  const number = (key, fallback, min, max = Number.MAX_SAFE_INTEGER) => {
    const n = values[key] === undefined ? fallback : Number(values[key]);
    if (!Number.isSafeInteger(n) || n < min || n > max) throw new Error(`--${key} must be an integer from ${min} to ${max}`);
    return n;
  };
  const theme = values.theme ?? "light";
  if (!["light", "dark", "both"].includes(theme)) throw new Error("--theme must be light, dark or both");
  const languages = [...new Set((values.lang ?? "en").split(",").map((s) => s.trim()).filter(Boolean))];
  if (!languages.length) throw new Error("--lang names no languages");
  return { baseUrl, url, output: path.resolve(values.output), patterns: positionals, group: values.group, languages,
    themes: theme === "both" ? ["light", "dark"] : [theme], width: values.width === undefined ? undefined : number("width", 1440, 1),
    threshold: number("threshold", 8, 0, 255), timeout: number("timeout", 60_000, 1),
    playwright: values.playwright ?? process.env.OCTOBOARD_PLAYWRIGHT, chromium: values.chromium ?? process.env.OCTOBOARD_CHROMIUM };
}

async function within(timeout, promise, label) {
  let timer;
  try {
    return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`${label} timed out after ${timeout}ms`)), timeout); })]);
  } finally {
    clearTimeout(timer);
  }
}

async function readGallery(browser, origin, timeout) {
  const page = await within(timeout, browser.newPage(), "opening browser page");
  try {
    await page.goto(`${origin}/gallery-frame.html?scenario=`, { waitUntil: "domcontentloaded", timeout });
    return await within(timeout, page.evaluate(async () => {
      const { SCENARIOS } = await import("/src/gallery/fixtures/index.ts");
      const { LANGUAGES } = await import("/src/i18n/languages.ts");
      return { languages: [...LANGUAGES], scenarios: SCENARIOS.map((s) => ({ id: s.id, title: s.title, group: s.group, width: s.width, minWidth: s.minWidth, height: s.height })) };
    }), `reading catalog at ${origin}`);
  } finally {
    await within(Math.min(5000, timeout), page.close(), "closing browser page");
  }
}

async function capture(browser, origin, scenario, job, timeout, output, file) {
  // A new context isolates the starting preferences even when both origins are the same.
  let context;
  const problems = [];
  const result = {};
  try {
    context = await within(timeout, browser.newContext({ viewport: job.viewport, reducedMotion: "reduce", locale: job.language, timezoneId: "UTC", deviceScaleFactor: 1 }), "opening capture context");
    const page = await within(timeout, context.newPage(), "opening capture page");
    page.on("pageerror", (error) => problems.push(String(error)));
    const query = new URLSearchParams({ scenario: scenario.id, theme: job.theme, lang: job.language });
    await page.goto(`${origin}/gallery-frame.html?${query}`, { waitUntil: "domcontentloaded", timeout });
    await page.waitForSelector("html[data-gallery-ready]", { state: "attached", timeout });
    await within(timeout, page.evaluate(async () => { await document.fonts.ready; }), "waiting for fonts");
    await page.waitForTimeout(150);
    const banners = await within(timeout, page.locator("[data-gallery-failure]").allTextContents(), "reading gallery failures");
    problems.push(...banners);
    const buffer = await page.screenshot({ animations: "disabled", caret: "hide", timeout });
    writeFileSync(path.join(output, file), buffer);
    Object.assign(result, { buffer, file });
  } catch (error) {
    problems.push(String(error));
  } finally {
    if (context) {
      try { await within(Math.min(5000, timeout), context.close(), "closing capture context"); } catch (error) { problems.push(`Cleanup failed: ${error}`); result.cleanupFailed = true; }
    }
  }
  if (problems.length) result.error = problems.join("\n");
  return result;
}

async function diffImages(browser, base, current, threshold, timeout) {
  const page = await within(timeout, browser.newPage(), "opening browser page");
  try {
    await within(timeout, page.addScriptTag({ content: `window.galleryComparePixels = ${comparePixels.toString()};` }), "preparing image comparison");
    return await within(timeout, page.evaluate(async ({ base, current, threshold }) => {
      const decode = async (data) => {
        const img = new Image();
        img.src = `data:image/png;base64,${data}`;
        await img.decode();
        const canvas = document.createElement("canvas"); canvas.width = img.width; canvas.height = img.height;
        const ctx = canvas.getContext("2d"); ctx.drawImage(img, 0, 0);
        return { width: img.width, height: img.height, data: ctx.getImageData(0, 0, img.width, img.height).data };
      };
      const a = await decode(base), b = await decode(current);
      const { data, ...result } = window.galleryComparePixels(a, b, threshold);
      const canvas = document.createElement("canvas"); canvas.width = result.width; canvas.height = result.height;
      canvas.getContext("2d").putImageData(new ImageData(data, result.width, result.height), 0, 0);
      return { ...result, png: canvas.toDataURL("image/png").split(",")[1] };
    }, { base: base.toString("base64"), current: current.toString("base64"), threshold }), "comparing screenshots");
  } finally {
    await within(Math.min(5000, timeout), page.close(), "closing browser page");
  }
}

export async function runComparison(options) {
  const started = Date.now();
  // Atomic creation refuses an existing report directory, including an empty one or a symlink.
  mkdirSync(path.dirname(options.output), { recursive: true });
  mkdirSync(options.output);
  const report = { version: 1, startedAt: new Date(started).toISOString(), baseUrl: options.baseUrl, url: options.url, threshold: options.threshold,
    selection: { patterns: options.patterns, group: options.group, languages: options.languages, themes: options.themes, width: options.width }, complete: false, results: [] };
  const save = () => {
    report.durationMs = Date.now() - started;
    writeFileSync(path.join(options.output, "results.json"), JSON.stringify(report, null, 2));
    writeFileSync(path.join(options.output, "index.html"), renderReport(report));
  };
  let browser, launching, release = () => {}, closing, signalCode;
  const close = () => closing ??= (async () => {
    try {
      if (launching) browser = await within(options.timeout, launching, "waiting for Chromium launch");
      if (browser) await within(Math.min(5000, options.timeout), browser.close(), "closing Chromium");
    } finally { release(); }
  })();
  const handlers = new Map(["SIGINT", "SIGTERM", "SIGHUP"].map((signal) => [signal, () => {
    const code = 128 + os.constants.signals[signal];
    if (signalCode) process.exit(code);
    signalCode = code;
    report.complete = false; report.error = `Interrupted by ${signal}`; save();
    void close().finally(() => process.exit(code));
  }]));
  for (const [signal, handler] of handlers) process.on(signal, handler);
  let code = 2, cleanupFailed = false;
  try {
    release = await acquireSlot("gallery screenshot comparison", { root: process.env.OCTOBOARD_COMPARE_STATE_DIR && path.join(process.env.OCTOBOARD_COMPARE_STATE_DIR, "slots"), onWait: (holders, ahead) => console.error(`Waiting for a free heavy-step slot; ${describeWait(holders, ahead)}`) });
    const { chromium, own } = await within(options.timeout, loadChromium(options.playwright), "loading Playwright");
    launching = launchBrowser(chromium, options.chromium, own, options.timeout);
    browser = await launching;
    launching = undefined;
    const base = await readGallery(browser, options.baseUrl, options.timeout);
    const current = await readGallery(browser, options.url, options.timeout);
    for (const language of options.languages) {
      if (!base.languages.includes(language) || !current.languages.includes(language)) throw new Error(`Language ${language} must be offered by both galleries`);
    }
    const selected = selectScenarios(base.scenarios, current.scenarios, options);
    for (const theme of options.themes) for (const language of options.languages) for (const pair of selected) {
      const prefix = String(report.results.length + 1).padStart(5, "0");
      const result = { id: pair.id, title: (pair.current ?? pair.base).title, theme, language, viewport: viewportOf(pair, options.width), images: {},
        presence: { base: Boolean(pair.base), current: Boolean(pair.current) }, status: "pending" };
      report.results.push(result);
      save();
      const a = pair.base && await capture(browser, options.baseUrl, pair.base, result, options.timeout, options.output, `${prefix}.base.png`);
      if (signalCode) return signalCode;
      if (a?.file) result.images.base = a.file;
      const b = !a?.cleanupFailed && pair.current && await capture(browser, options.url, pair.current, result, options.timeout, options.output, `${prefix}.current.png`);
      if (signalCode) return signalCode;
      if (b?.file) result.images.current = b.file;
      const errors = [a?.error && `Base: ${a.error}`, b?.error && `Current: ${b.error}`].filter(Boolean);
      if (errors.length) { result.status = "error"; result.error = errors.join("\n"); } else if (a && b) {
        try {
          const { png, ...diff } = await diffImages(browser, a.buffer, b.buffer, options.threshold, options.timeout);
          if (signalCode) return signalCode;
          Object.assign(result, diff, { status: diff.pixels ? "changed" : "unchanged" });
          result.images.diff = `${prefix}.diff.png`;
          writeFileSync(path.join(options.output, result.images.diff), Buffer.from(png, "base64"));
        } catch (error) {
          if (signalCode) return signalCode;
          result.status = "error"; result.error = String(error);
          save();
          throw error;
        }
      } else result.status = pair.base ? "removed" : "added";
      save();
      if (a?.cleanupFailed || b?.cleanupFailed) throw new Error("Capture cleanup failed; remaining cases were not run");
      console.log(`${result.status.toUpperCase()} ${language}/${theme} ${pair.id}${result.pixels === undefined ? "" : `: ${result.pixels} pixels (${result.percent.toFixed(3)}%)`}`);
    }
    report.complete = true;
    code = report.results.some((r) => r.status === "error") ? 2 : report.results.some((r) => r.status !== "unchanged") ? 1 : 0;
  } catch (error) {
    if (!signalCode) report.error = String(error);
    console.error(report.error);
  } finally {
    try { await close(); } catch (error) { cleanupFailed = true; report.error = [report.error, `Cleanup failed: ${error}`].filter(Boolean).join("\n"); code = 2; }
    save();
    for (const [signal, handler] of handlers) process.off(signal, handler);
  }
  console.log(`Report: ${path.join(options.output, "index.html")}`);
  // A rejected browser close can leave native handles referenced.
  if (cleanupFailed) process.exit(code);
  return code;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const options = parseOptions(process.argv.slice(2));
    if (options.help) console.log(USAGE);
    else process.exitCode = await runComparison(options);
  } catch (error) {
    console.error(String(error));
    process.exitCode = 2;
  }
}
