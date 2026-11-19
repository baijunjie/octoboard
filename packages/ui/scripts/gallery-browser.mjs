import { existsSync, readdirSync } from "node:fs";
import { createRequire } from "node:module";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

export class SetupError extends Error {}

export const firstLine = (error) => String(error?.message ?? error).split("\n")[0];

/** A regular expression matching exactly the text `pattern` spells, where `*` stands for any text. */
export function wildcard(pattern) {
  return new RegExp(`^${pattern.split("*").map((part) => part.replace(/[.+?^${}()|[\]\\]/g, "\\$&")).join(".*")}$`);
}

const packageDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

/** How long reading a page, or the list of scenarios from one, may take. Playwright puts no limit on it. */
export const READ_TIMEOUT_MS = 30_000;

/** `promise`, rejected if it takes longer than `ms`. */
export function within(ms, promise, what) {
  let timer;
  const limit = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Error(`${what} took more than ${ms / 1000}s`)), ms);
  });
  return Promise.race([promise, limit]).finally(() => clearTimeout(timer));
}

/** Playwright's `chromium`, and whether it came from this package rather than from a directory the
 * developer named: the advice a failed launch gives depends on whose installation it was, and this is
 * the function that chooses. */
export async function loadChromium(explicitDir) {
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

/** Callers handle the signals themselves, so that an interrupted run stops quietly
 * rather than reporting the pages it was loading as failures. */
const LAUNCH_OPTIONS = { handleSIGINT: false, handleSIGTERM: false, handleSIGHUP: false };

async function launchAt(chromium, executablePath, timeout) {
  try {
    return await chromium.launch({ ...LAUNCH_OPTIONS, executablePath, ...(timeout === undefined ? {} : { timeout }) });
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

export async function launchBrowser(chromium, executablePath, ownPlaywright, timeout) {
  if (executablePath) return launchAt(chromium, executablePath, timeout);
  try {
    return await chromium.launch(timeout === undefined ? LAUNCH_OPTIONS : { ...LAUNCH_OPTIONS, timeout });
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
    return launchAt(chromium, cached, timeout);
  }
}


/** Starts this checkout's Vite dev server on a free port. File watching and HMR are off: nothing edits the
 * tree during a sweep or a capture, and a change landing mid-run would reload pages under it; the browser's
 * console is not echoed into this terminal, a failure being read off the page. The modules served are the dev
 * server's own, which is why the gallery is not driven from a build. `onCreate` receives the server before
 * it listens, so a caller that owns it can close it even when the listen fails. */
export async function startGalleryServer(onCreate) {
  const { createServer } = await import("vite");
  const server = await createServer({
    root: packageDir,
    logLevel: "error",
    server: { port: 0, strictPort: false, open: false, watch: null, hmr: false, forwardConsole: false },
  });
  onCreate?.(server);
  await server.listen();
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") throw new SetupError("The dev server did not report a port");
  return { origin: `http://localhost:${address.port}`, close: () => server.close() };
}

/** The scenarios and the languages the gallery page itself knows, read off the page the way the page
 * would use them, so that neither list is copied here. */
export async function readGallery(browser, origin) {
  const page = await browser.newPage();
  try {
    await page.goto(`${origin}/gallery-frame.html?scenario=`, { waitUntil: "domcontentloaded", timeout: READ_TIMEOUT_MS });
    return await within(
      READ_TIMEOUT_MS,
      page.evaluate(async () => {
        const { SCENARIOS } = await import("/src/gallery/fixtures/index.ts");
        const { LANGUAGES } = await import("/src/i18n/languages.ts");
        const { CATALOGS } = await import("/src/i18n/catalog.ts");
        return {
          scenarios: SCENARIOS.map((s) => ({ id: s.id, group: s.group, width: s.width, minWidth: s.minWidth, height: s.height, live: (s.liveChanges ?? []).map((change) => change.id) })),
          languages: [...LANGUAGES],
          catalogLanguages: Object.keys(CATALOGS),
        };
      }),
      "reading the scenarios",
    );
  } finally {
    await page.close().catch(() => {});
  }
}

/** Loads a gallery frame in `page` and returns what is wrong with it, empty when it is clean: an uncaught error,
 * a page that never says it is ready within `readyTimeoutMs`, and the lines of the `[data-gallery-failure]` overlay
 * (`src/fixtures/banner.ts`), which is read even when the page signals ready. `settleMs` is what the page is given
 * after it says so, for whatever a step did not wait for (a lazily loaded renderer, a line raised by a timer) to
 * show. A page, context or browser that goes away under the load throws instead. `afterSettle` runs once the page
 * has settled, before the overlay is read, and is given the problems found so far. */
export async function loadFrame(page, url, { readyTimeoutMs, settleMs, afterSettle }) {
  const problems = [];
  page.on("pageerror", (error) => problems.push(`uncaught: ${firstLine(error)}`));
  await page.goto(url, { waitUntil: "commit" });
  try {
    // The page sets this on <html> once the app has mounted, the scenario's steps have run, failed or not,
    // and its own notifications have been raised, and then after two frames or 200ms, whichever is first.
    await page.waitForSelector("html[data-gallery-ready]", { state: "attached", timeout: readyTimeoutMs });
  } catch (error) {
    // Only a timeout is a page that never became ready; a page, context or browser that went away
    // under the wait is something else, and says so.
    if (error?.name !== "TimeoutError") throw error;
    problems.push(`the page never became ready within ${readyTimeoutMs / 1000}s`);
  }
  await page.waitForTimeout(settleMs);
  await afterSettle?.(problems);
  const banner = await within(
    READ_TIMEOUT_MS,
    page.evaluate(() => Array.from(document.querySelectorAll("[data-gallery-failure] > div"), (line) => line.textContent ?? "")),
    "reading the page",
  ).catch((error) => {
    problems.push(`could not read the page: ${firstLine(error)}`);
    return [];
  });
  return [...banner, ...problems];
}

/** How long each owned resource has to close, including after a successful run. */
const CLOSE_TIMEOUT_MS = 5_000;

/** The lifecycle of a script that starts a browser and a dev server: what it owns is closed once, whether the run
 * ends or a signal interrupts it. `own` registers a resource (anything with `close()`), `close` closes them
 * independently, so a stuck browser does not keep the server from releasing its resources, `interrupted` is set
 * before they close, so that the pages that fail because their browser went away are not reported, and `finish`
 * ends a run that closed badly. Signals end the process with 128 plus the signal's number. */
export function createRun() {
  const owned = [];
  let cleanup;
  let cleanupFailed = false;
  const run = {
    interrupted: false,
    own(name, resource) {
      owned.push([name, resource]);
      return resource;
    },
    /** Shares one promise with the signal handlers, so a close is never started twice. */
    close() {
      return (cleanup ??= Promise.all(
        owned.map(async ([name, resource]) => {
          try {
            await within(CLOSE_TIMEOUT_MS, Promise.resolve().then(() => resource.close()), `closing ${name}`);
          } catch (error) {
            cleanupFailed = true;
            console.error(`Cleanup failed for ${name}: ${firstLine(error)}`);
          }
        }),
      ));
    },
    /** A failed close may leave referenced native handles. Report it before ending the process, and wait
     * for piped output to flush so a task runner receives both the run's result and the cleanup diagnostic. */
    async exitAfterOutput(code) {
      const flushed = Promise.all([process.stdout, process.stderr].map((stream) => new Promise((resolve) => stream.write("", resolve))));
      await within(CLOSE_TIMEOUT_MS, flushed, "flushing output").catch(() => {});
      process.exit(code);
    },
    /** Called once the run's own work has ended and set `process.exitCode`. */
    async finish() {
      if (run.interrupted || !cleanupFailed) return;
      if (!process.exitCode) process.exitCode = 2;
      await run.exitAfterOutput(process.exitCode);
    },
  };
  for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) {
    process.on(signal, () => {
      const code = 128 + os.constants.signals[signal];
      if (run.interrupted) process.exit(code);
      run.interrupted = true;
      process.exitCode = code;
      console.error(`\n${signal}: stopping`);
      void run.close().then(() => run.exitAfterOutput(code));
    });
  }
  return run;
}
