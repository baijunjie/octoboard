// Loads every UI state gallery scenario in a real browser, lets the scenario's own steps run, and reads
// the red `[data-gallery-failure]` overlay (`src/gallery/banner.ts`) the page raises when a step threw, a
// fixture could not be answered as the real daemon would answer, or an exit transition never ended. It is
// the one check that sees a step that no longer runs or a state that no longer renders: neither the type
// checker nor the unit tests load a scenario. A scenario's live changes (its "Live changes" buttons) are
// applied too, after its steps and before the page is ready. See "The UI state gallery" in `packages/ui/README.md`.
//
//   pnpm gallery:sweep [options] [scenario-id-or-pattern ...]
//
// Playwright is a devDependency of this package, so `pnpm install` is everything the script itself needs;
// `--playwright <dir>` / `OCTOBOARD_PLAYWRIGHT` still points it at another installation, for a machine
// whose browser cache matches a different version. It installs no browser: `pnpm exec playwright install
// chromium`, run from this package (the workspace root resolves another `playwright` or none), fetches
// the one this version expects, and `--chromium` names any other already on the machine.
//
// It loads the scenarios it is given, in English unless `--lang` names more, every time. The page loads
// take a heavy-step slot of the machine-wide lock `pnpm verify` also takes (`scripts/verify/lock.mjs`),
// so that sweeps in several worktrees queue instead of together loading the machine.

import { existsSync, readdirSync } from "node:fs";
import { createRequire } from "node:module";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
import { acquireSlot, describeWait } from "../../../scripts/verify/lock.mjs";

const packageDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

/** What a sweep without `--lang` loads. A translation is checked where it is written (each catalog is typed
 * to hold every message and plural category, `src/i18n/catalog.test.ts` checks the placeholders, and
 * `src/gallery/fixtures/fixtures.test.ts` checks that no step names a control in English copy or with
 * the English separator), so the gallery verifies English unless asked for more. */
const DEFAULT_LANGUAGES = ["en"];

/** Every language the app has a catalog for, which `--lang all` sweeps. */
const LANGUAGES_WITH_CATALOGS = ["en", "zh-Hans"];

const USAGE = `Usage: pnpm gallery:sweep [options] [scenario ...]

Loads gallery scenarios in a headless browser, runs their steps and fails on any gallery failure banner.
With no scenario given, every scenario is swept, in English and the light theme unless --lang and --theme
say otherwise.

  scenario              An id, or a pattern with * as a wildcard ("git-*"). Repeatable.
  --group <title>       Only the scenarios of the group whose title contains this text (case-insensitive).
  --lang <list>         Comma-separated languages, each one the app offers, or "all" for every language
                        with a catalog (${LANGUAGES_WITH_CATALOGS.join(", ")}). Default: ${DEFAULT_LANGUAGES.join(", ")}.
  --theme <name>        light (default), dark or both. A scenario's steps do not depend on the theme, so
                        light settles a step that no longer runs; dark when asked for.
  --jobs <n>            Pages loaded at once (default: ${defaultJobs()}).
  --url <origin>        Use the gallery served at this origin (a running \`pnpm gallery\`) instead of
                        starting a dev server of this checkout.
  --list                Print the scenario ids that would be swept and exit.
  --playwright <dir>    A Playwright other than this package's own: a directory holding
                        node_modules/playwright (or playwright-core), or the package itself. Also
                        OCTOBOARD_PLAYWRIGHT.
  --chromium <path>     The Chromium executable to launch. Also OCTOBOARD_CHROMIUM. Without it Playwright's
                        own is used, and failing that the newest headless shell in its browser cache.
  -h, --help            This text.

Page loads wait for a free heavy-step slot (the one \`pnpm verify\` takes; OCTOBOARD_VERIFY_SLOTS sets how many).

Exit status: 0 when every swept scenario is clean, 1 when any failed (a failure that shows only while pages
are loaded together counts), 2 when the sweep could not run, 128 plus the signal's number when interrupted.`;

/** Six tenths of the cores, at most 6. Measured on a 10-core machine, going from 4 pages at a time to 10
 * cut a full sweep's time by less than half and reported no failure the serial run did not, so the extra
 * pages buy little and only crowd the machine the developer is working on. */
function defaultJobs() {
  return Math.max(1, Math.min(6, Math.round(os.availableParallelism() * 0.6)));
}

/** How long one page may take to run its steps before the sweep gives up on it. */
const READY_TIMEOUT_MS = 60_000;
/** What a page is given after it says it is ready, for whatever a step did not wait for (a lazily loaded
 * renderer, a line raised by a timer) to raise its banner line before the banner is read. */
const SETTLE_MS = 150;
/** How long reading a page, or the list of scenarios from one, may take. Playwright puts no limit on it. */
const READ_TIMEOUT_MS = 30_000;
/** How long each owned resource has to close, including after a successful sweep. */
const CLOSE_TIMEOUT_MS = 5_000;

class SetupError extends Error {}

const firstLine = (error) => String(error?.message ?? error).split("\n")[0];
const plural = (count, noun) => `${count} ${noun}${count === 1 ? "" : "s"}`;

/** `promise`, rejected if it takes longer than `ms`. */
function within(ms, promise, what) {
  let timer;
  const limit = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Error(`${what} took more than ${ms / 1000}s`)), ms);
  });
  return Promise.race([promise, limit]).finally(() => clearTimeout(timer));
}

function parse() {
  let parsed;
  try {
    parsed = parseArgs({
      allowPositionals: true,
      options: {
        group: { type: "string" },
        lang: { type: "string" },
        theme: { type: "string" },
        jobs: { type: "string" },
        url: { type: "string" },
        list: { type: "boolean" },
        playwright: { type: "string" },
        chromium: { type: "string" },
        help: { type: "boolean", short: "h" },
      },
    });
  } catch (error) {
    throw new SetupError(`${error.message}\n\n${USAGE}`);
  }
  const { values, positionals } = parsed;
  if (values.help) {
    console.log(USAGE);
    process.exit(0);
  }
  const languages = values.lang === undefined ? DEFAULT_LANGUAGES : values.lang.trim() === "all" ? LANGUAGES_WITH_CATALOGS : [...new Set(values.lang.split(",").map((s) => s.trim()).filter(Boolean))];
  if (languages.length === 0) throw new SetupError("--lang names no language");
  const theme = values.theme ?? "light";
  if (!["light", "dark", "both"].includes(theme)) throw new SetupError(`--theme is light, dark or both, not "${theme}"`);
  const jobs = values.jobs === undefined ? defaultJobs() : Number(values.jobs);
  if (!Number.isInteger(jobs) || jobs < 1) throw new SetupError(`--jobs is a positive whole number, not "${values.jobs}"`);
  return {
    patterns: positionals,
    group: values.group,
    languages,
    themes: theme === "both" ? ["light", "dark"] : [theme],
    jobs,
    url: values.url?.replace(/\/+$/, ""),
    list: values.list ?? false,
    playwright: values.playwright ?? process.env.OCTOBOARD_PLAYWRIGHT,
    chromium: values.chromium ?? process.env.OCTOBOARD_CHROMIUM,
  };
}

// ---- Playwright and the browser ------------------------------------------------------------------

/** Playwright's `chromium`, and whether it came from this package rather than from a directory the
 * developer named: the advice a failed launch gives depends on whose installation it was, and this is
 * the function that chooses. */
async function loadChromium(explicitDir) {
  const attempts = [];
  const firstChromium = async (label, candidates) => {
    for (const { name, resolve } of candidates) {
      try {
        const mod = await import(pathToFileURL(resolve()).href);
        const chromium = mod.chromium ?? mod.default?.chromium;
        if (chromium) return chromium;
        attempts.push(`${label}: ${name}: exports no chromium`);
      } catch (error) {
        attempts.push(`${label}: ${name}: ${firstLine(error)}`);
      }
    }
    return undefined;
  };
  const packages = (require) => ["playwright", "playwright-core"].map((name) => ({ name, resolve: () => require.resolve(name) }));
  if (explicitDir) {
    const dir = path.resolve(explicitDir);
    if (!existsSync(dir)) throw new SetupError(`--playwright / OCTOBOARD_PLAYWRIGHT names ${dir}, which does not exist`);
    // The directory may hold node_modules/playwright, or be the package itself.
    const require = createRequire(path.join(dir, "noop.js"));
    const found = await firstChromium(dir, [...packages(require), { name: "the directory itself", resolve: () => require.resolve(dir) }]);
    if (found) return { chromium: found, own: false };
    throw new SetupError(`No Playwright found under ${dir}\n  ${attempts.join("\n  ")}`);
  }
  const found = await firstChromium("this repository", packages(createRequire(import.meta.url)));
  if (found) return { chromium: found, own: true };
  throw new SetupError(
    [
      "No Playwright resolved here, though this package depends on it. Install the workspace:",
      "",
      "  pnpm install",
      "",
      "To sweep against another installation instead — a machine whose Playwright browser cache holds",
      "another version's binaries — name it with --playwright <dir> or OCTOBOARD_PLAYWRIGHT.",
    ].join("\n"),
  );
}

/** The newest `chromium_headless_shell-*` in Playwright's browser cache, for a Playwright whose own
 * expected revision is not downloaded but an older one is. */
function cachedHeadlessShell() {
  const roots = [
    process.env.PLAYWRIGHT_BROWSERS_PATH,
    path.join(os.homedir(), "Library/Caches/ms-playwright"),
    path.join(os.homedir(), ".cache/ms-playwright"),
    process.env.LOCALAPPDATA && path.join(process.env.LOCALAPPDATA, "ms-playwright"),
  ].filter(Boolean);
  const executable = process.platform === "win32" ? "chrome-headless-shell.exe" : "chrome-headless-shell";
  for (const root of roots) {
    if (!existsSync(root)) continue;
    const revisions = readdirSync(root)
      .map((name) => /^chromium_headless_shell-(\d+)$/.exec(name))
      .filter(Boolean)
      .sort((a, b) => Number(b[1]) - Number(a[1]));
    for (const [name] of revisions) {
      // The folder under the revision is named for the platform (`chrome-headless-shell-mac-arm64`, ...).
      for (const platformDir of readdirSync(path.join(root, name)).filter((entry) => entry.startsWith("chrome-headless-shell-"))) {
        const exe = path.join(root, name, platformDir, executable);
        if (existsSync(exe)) return exe;
      }
    }
  }
  return undefined;
}

/** This script handles the signals itself (`running`, below), so that an interrupted sweep stops quietly
 * rather than reporting the pages it was loading as failures. */
const LAUNCH_OPTIONS = { handleSIGINT: false, handleSIGTERM: false, handleSIGHUP: false };

async function launchAt(chromium, executablePath) {
  try {
    return await chromium.launch({ ...LAUNCH_OPTIONS, executablePath });
  } catch (error) {
    throw new SetupError(`Could not launch ${executablePath}: ${firstLine(error)}`);
  }
}

/** What to do about a Chromium that would not launch. Only a missing executable has a browser to fetch,
 * and only this package's own Playwright is fetched with this package's CLI: the one named by
 * `--playwright` expects a revision that CLI would not download. Anything else is a browser that is there
 * and still would not start, most often for want of its host libraries. */
function launchAdvice(missingExecutable, ownPlaywright) {
  if (!missingExecutable) {
    return [
      "Name a browser already on the machine with --chromium <path> / OCTOBOARD_CHROMIUM. A browser",
      "that is there and still will not start usually wants `playwright install --with-deps chromium`.",
    ];
  }
  if (ownPlaywright) {
    return [
      "Fetch the browser this Playwright expects with `pnpm exec playwright install chromium`, run",
      "from this package, or name one with --chromium <path> / OCTOBOARD_CHROMIUM.",
    ];
  }
  return [
    "This Playwright came from --playwright / OCTOBOARD_PLAYWRIGHT, so fetch its browser with its own",
    "`playwright install chromium` rather than this package's, or name one with --chromium <path>.",
  ];
}

async function launchBrowser(chromium, executablePath, ownPlaywright) {
  if (executablePath) return launchAt(chromium, executablePath);
  try {
    return await chromium.launch(LAUNCH_OPTIONS);
  } catch (error) {
    const missingExecutable = /Executable doesn't exist/.test(String(error.message));
    const cached = missingExecutable ? cachedHeadlessShell() : undefined;
    if (!cached) {
      const advice = launchAdvice(missingExecutable, ownPlaywright);
      throw new SetupError([`Could not launch Chromium: ${firstLine(error)}`, ...advice].join("\n"));
    }
    let expected = "unknown";
    try {
      expected = chromium.executablePath();
    } catch {
      // The diagnostic is optional when Playwright cannot locate its browser.
    }
    console.error(`Playwright's own Chromium is not downloaded (it expects ${expected}); using the newest headless shell in its cache, ${cached}`);
    return launchAt(chromium, cached);
  }
}

// ---- the dev server ------------------------------------------------------------------------------

/** Starts this checkout's Vite dev server on a free port. File watching and HMR are off: nothing edits the
 * tree during a sweep, and a change landing mid-run would reload pages under it; the browser's console is
 * not echoed into this terminal, a failure being read off the page. The modules served are the dev
 * server's own, which is why the gallery is not swept from a build. */
async function startServer() {
  const { createServer } = await import("vite");
  const server = await createServer({
    root: packageDir,
    logLevel: "error",
    server: { port: 0, strictPort: false, open: false, watch: null, hmr: false, forwardConsole: false },
  });
  // Own the server before listening: a failed listen still needs its plugin resources closed.
  running.server = server;
  await server.listen();
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") throw new SetupError("The dev server did not report a port");
  return { origin: `http://localhost:${address.port}`, close: () => server.close() };
}

// ---- scenarios -----------------------------------------------------------------------------------

/** The scenarios and the languages the gallery page itself knows, read off the page the way the page
 * would use them, so that neither list is copied here. */
async function readGallery(browser, origin) {
  const page = await browser.newPage();
  try {
    await page.goto(`${origin}/gallery-frame.html?scenario=`, { waitUntil: "domcontentloaded", timeout: READ_TIMEOUT_MS });
    return await within(
      READ_TIMEOUT_MS,
      page.evaluate(async () => {
        const { SCENARIOS } = await import("/src/gallery/fixtures/index.ts");
        const { LANGUAGES } = await import("/src/i18n/languages.ts");
        return {
          scenarios: SCENARIOS.map((s) => ({ id: s.id, group: s.group, width: s.width, height: s.height, live: (s.liveChanges ?? []).map((change) => change.id) })),
          languages: [...LANGUAGES],
        };
      }),
      "reading the scenarios",
    );
  } finally {
    await page.close().catch(() => {});
  }
}

function checkLanguages(requested, known) {
  const unknown = requested.filter((language) => !known.includes(language));
  if (unknown.length > 0) throw new SetupError(`--lang names no such language: ${unknown.join(", ")}. The app offers ${known.join(", ")}`);
}

function select(all, { patterns, group }) {
  let chosen = all;
  if (group) {
    const needle = group.toLowerCase();
    chosen = chosen.filter((s) => s.group.toLowerCase().includes(needle));
    if (chosen.length === 0) {
      const groups = [...new Set(all.map((s) => s.group))];
      throw new SetupError(`No group contains "${group}". Groups:\n  ${groups.join("\n  ")}`);
    }
  }
  if (patterns.length > 0) {
    const regexps = patterns.map((p) => ({ p, re: new RegExp(`^${p.split("*").map((part) => part.replace(/[.+?^${}()|[\]\\]/g, "\\$&")).join(".*")}$`) }));
    const unmatched = regexps.filter(({ re }) => !chosen.some((s) => re.test(s.id))).map(({ p }) => p);
    if (unmatched.length > 0) throw new SetupError(`No scenario matches: ${unmatched.join(", ")}${group ? ` (within the group "${group}")` : ""}`);
    chosen = chosen.filter((s) => regexps.some(({ re }) => re.test(s.id)));
  }
  if (chosen.length === 0) throw new SetupError("The gallery has no scenarios to sweep");
  return chosen;
}

// ---- one page ------------------------------------------------------------------------------------

/** Behavioral check run after the gallery settles, so the displayed scenario remains archived. It needs a
 * real browser, which a unit test does not have (the Reopen label in each language is held by
 * `TerminalPane.test.tsx`): the floating bar sits over the history view, which covers the terminal beneath
 * it, and DOM clicks bypass hit testing, so it drives the actual pointer and verifies the fixture session
 * becomes live. */
async function checkArchivedReopen(page, language) {
  const labels = await page.evaluate(async (language) => {
    const { format } = await import("/src/i18n/catalog.ts");
    const { sessionAriaLabel } = await import("/src/sessionLabel.ts");
    const { SCENARIOS } = await import("/src/gallery/fixtures/index.ts");
    const session = SCENARIOS.find((scenario) => scenario.id === "history-loaded").state.sessions.get("s-history");
    return {
      reopen: format(language, "sidebar.session.reopen"),
      liveSession: sessionAriaLabel((key, params) => format(language, key, params), language, { ...session, status: "idle" }, []),
    };
  }, language);
  // Found by its name in the terminal pane: the bar's button sits outside the live region that speaks its sentence.
  const button = page.locator("[data-region=terminal]").getByRole("button", { name: labels.reopen, exact: true });
  await button.waitFor({ state: "attached", timeout: READ_TIMEOUT_MS });
  const target = await button.evaluate((button) => {
    const { x, y, width, height } = button.getBoundingClientRect();
    const point = { x: x + width / 2, y: y + height / 2 };
    const hit = document.elementFromPoint(point.x, point.y);
    if (!hit || !button.contains(hit)) throw new Error(`Reopen is covered by ${hit?.className ?? "nothing"}`);
    if (getComputedStyle(button).cursor !== "pointer") throw new Error("Reopen has no pointer cursor");
    return point;
  });
  await page.mouse.click(target.x, target.y);
  await button.waitFor({ state: "hidden", timeout: READ_TIMEOUT_MS });
  // A focus card carries more after the status (its model), so the name is matched as a substring.
  await page.getByRole("button", { name: labels.liveSession }).waitFor({ timeout: READ_TIMEOUT_MS });
}

/** Runs one scenario in its own page of `context` and returns the failure text, or undefined if clean. */
async function sweepOne(context, origin, scenario, language, theme) {
  const page = await context.newPage();
  const problems = [];
  page.on("pageerror", (error) => problems.push(`uncaught: ${firstLine(error)}`));
  try {
    await page.setViewportSize({ width: scenario.width ?? 1440, height: scenario.height ?? 900 });
    const query = new URLSearchParams({ scenario: scenario.id, theme, lang: language });
    // The scenario's live changes, applied after its steps and before the page is ready (the frame runs `live=`
    // first), as a viewer pressing each of its buttons would.
    if (scenario.live.length > 0) query.set("live", scenario.live.join(","));
    await page.goto(`${origin}/gallery-frame.html?${query}`, { waitUntil: "commit" });
    try {
      // The page sets this on <html> once the app has mounted, the scenario's steps have run, failed or not,
      // and its own toasts have been raised, and then after two frames or 200ms, whichever is first; the
      // sweep reads text, not pixels, so it does not depend on the frames having been drawn.
      await page.waitForSelector("html[data-gallery-ready]", { state: "attached", timeout: READY_TIMEOUT_MS });
    } catch (error) {
      // Only a timeout is a page that never became ready; a page, context or browser that went away
      // under the wait is something else, and says so.
      if (error?.name !== "TimeoutError") throw error;
      problems.push(`the page never became ready within ${READY_TIMEOUT_MS / 1000}s`);
    }
    await page.waitForTimeout(SETTLE_MS);
    if (scenario.id === "history-loaded" && language === "en" && problems.length === 0) await checkArchivedReopen(page, language);
    const banner = await within(
      READ_TIMEOUT_MS,
      page.evaluate(() => Array.from(document.querySelectorAll("[data-gallery-failure] > div"), (line) => line.textContent ?? "")),
      "reading the page",
    ).catch((error) => {
      problems.push(`could not read the page: ${firstLine(error)}`);
      return [];
    });
    const text = [...banner, ...problems];
    return text.length > 0 ? text.join(" | ") : undefined;
  } catch (error) {
    return `the page could not be driven: ${firstLine(error)}`;
  } finally {
    await page.close().catch(() => {});
  }
}

/** Runs `jobs` through `concurrency` workers, each with one browser context of its own: a context is a
 * separate localStorage, which a scenario's page rewrites on every load (`prepareStorage`), so two pages in
 * one context would overwrite each other's starting preferences. A worker reuses its context across its
 * scenarios, as one browser tab clicking through the list does, which keeps the HTTP cache of the dev
 * modules warm. */
async function runAll(browser, origin, jobs, concurrency, onDone) {
  const results = new Array(jobs.length);
  let next = 0;
  const worker = async () => {
    const context = await browser.newContext();
    try {
      for (let i = next++; i < jobs.length && !running.interrupted; i = next++) {
        const { scenario, language, theme } = jobs[i];
        const started = Date.now();
        const failure = await sweepOne(context, origin, scenario, language, theme);
        results[i] = { ...jobs[i], failure, ms: Date.now() - started };
        onDone(results[i]);
      }
    } finally {
      await context.close().catch(() => {});
    }
  };
  await Promise.all(Array.from({ length: Math.min(concurrency, jobs.length) }, worker));
  return results;
}

// ---- main ----------------------------------------------------------------------------------------

/** `OCTOBOARD_SWEEP_STATE_DIR` moves the slots out of the machine-wide place: the script's own tests use
 * it, so that they do not wait on a slot held by the test run that started them. */
const stateDir = process.env.OCTOBOARD_SWEEP_STATE_DIR;

const label = (r) => `${r.language}/${r.theme}  ${r.scenario.id}`;

/** What is open, for the signal handlers to close; `interrupted` is set before they close it, so that
 * the pages that fail because their browser went away are not reported. */
const running = { browser: undefined, server: undefined, interrupted: false };
let cleanup;
let cleanupFailed = false;

/** Closing independently keeps a stuck browser from preventing the server from releasing its resources.
 * Share the promise with signal handlers so a close is never started twice. */
function closeResources() {
  return cleanup ??= Promise.all([
    ["Chromium", running.browser],
    ["Vite server", running.server],
  ].map(async ([name, resource]) => {
    if (!resource) return;
    try {
      await within(CLOSE_TIMEOUT_MS, Promise.resolve().then(() => resource.close()), `closing ${name}`);
    } catch (error) {
      cleanupFailed = true;
      console.error(`Cleanup failed for ${name}: ${firstLine(error)}`);
    }
  }));
}

/** A failed close may leave referenced native handles. Report it before ending the process, and wait
 * for piped output to flush so a task runner receives both the sweep result and cleanup diagnostic. */
async function exitAfterOutput(code) {
  const flushed = Promise.all([process.stdout, process.stderr].map((stream) => new Promise((resolve) => stream.write("", resolve))));
  await within(CLOSE_TIMEOUT_MS, flushed, "flushing output").catch(() => {});
  process.exit(code);
}

for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) {
  process.on(signal, () => {
    const code = 128 + os.constants.signals[signal];
    if (running.interrupted) process.exit(code);
    running.interrupted = true;
    process.exitCode = code;
    console.error(`\n${signal}: stopping`);
    void closeResources().then(() => exitAfterOutput(code));
  });
}

async function main() {
  const options = parse();
  const { chromium, own } = await loadChromium(options.playwright);
  const started = Date.now();
  // Held until the browser and the server are closed, so the next holder does not start while they still run.
  let release = () => {};
  try {
    if (!options.url) running.server = await startServer();
    const origin = options.url ?? running.server.origin;
    running.browser = await launchBrowser(chromium, options.chromium, own);
    const browser = running.browser;

    // The first load of a fresh dev server pre-bundles its dependencies, which can reload a page that is
    // already running; doing it once, alone, keeps that out of the concurrent pages.
    const gallery = await readGallery(browser, origin).catch((error) => {
      throw new SetupError(`Could not load the gallery from ${origin}: ${firstLine(error)}`);
    });
    checkLanguages(options.languages, gallery.languages);
    const scenarios = select(gallery.scenarios, options);
    if (options.list) {
      for (const s of scenarios) console.log(s.id);
      return 0;
    }
    const jobs = options.themes.flatMap((theme) => options.languages.flatMap((language) => scenarios.map((scenario) => ({ scenario, language, theme }))));
    const concurrency = Math.min(options.jobs, jobs.length);
    console.error(
      `Sweeping ${plural(scenarios.length, "scenario")} x ${options.languages.join(", ")} x ${options.themes.join(", ")} = ${plural(jobs.length, "load")}; ${concurrency} at a time`,
    );
    release = await acquireSlot(`${path.basename(path.resolve(packageDir, "../.."))}: gallery sweep`, {
      root: stateDir && path.join(stateDir, "slots"),
      onWait: (holders, ahead) => console.error(`Waiting for a free heavy-step slot; ${describeWait(holders, ahead)}`),
    });
    return await sweepPages(browser, origin, jobs, concurrency, started);
  } finally {
    await closeResources();
    release();
  }
}

/** Loads `jobs` and reports; returns the exit status. */
async function sweepPages(browser, origin, jobs, concurrency, started) {
  let done = 0;
  const interactive = process.stderr.isTTY;
  const results = await runAll(browser, origin, jobs, concurrency, (r) => {
    if (running.interrupted) return;
    done++;
    if (r.failure) {
      if (interactive) process.stderr.write("\r\x1b[K");
      console.log(`FAIL  ${label(r)}\n      ${r.failure}`);
    }
    if (interactive) process.stderr.write(`\r${done}/${jobs.length}`);
  });
  if (interactive) process.stderr.write("\r\x1b[K");
  if (running.interrupted) return process.exitCode;

  // A failure seen under load is checked again alone. That is a diagnosis, not a pardon: one that
  // reproduces is a scenario that no longer runs; one that does not is a step that races an animation or
  // a fixture reply that races a render, which only a busy machine shows, and that is a defect of the
  // scenario all the same. Both fail the sweep; the second is named, with what to try.
  const failed = results.filter((r) => r.failure);
  const loadOnly = [];
  if (failed.length > 0 && concurrency > 1) {
    console.error(`Re-running ${plural(failed.length, "failure")} alone to see whether they reproduce...`);
    const again = await runAll(browser, origin, failed, 1, () => {});
    if (running.interrupted) return process.exitCode;
    again.forEach((r, i) => {
      if (!r.failure) loadOnly.push(failed[i]);
    });
  }
  if (loadOnly.length > 0) {
    console.log(`\nFailed while pages loaded together, passed alone (${loadOnly.length}); a failure that needs a busy machine is still one:`);
    for (const r of loadOnly) console.log(`  ${label(r)}\n      ${r.failure}`);
  }
  const seconds = ((Date.now() - started) / 1000).toFixed(1);
  console.log(
    `\n${plural(jobs.length, "load")} in ${seconds}s (${concurrency} at a time): ${jobs.length - failed.length} clean, ${failed.length} failed${loadOnly.length ? ` (${loadOnly.length} only under load)` : ""}`,
  );
  return failed.length > 0 ? 1 : 0;
}

try {
  process.exitCode = await main();
} catch (error) {
  // When interrupted, the handler is closing everything and exits with the signal's code.
  if (!running.interrupted) {
    if (error instanceof SetupError) console.error(error.message);
    else console.error(error);
    process.exitCode = 2;
  }
}

if (!running.interrupted && cleanupFailed) {
  if (!process.exitCode) process.exitCode = 2;
  await exitAfterOutput(process.exitCode);
}
