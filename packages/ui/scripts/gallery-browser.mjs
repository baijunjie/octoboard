import { existsSync, readdirSync } from "node:fs";
import { createRequire } from "node:module";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";

export class SetupError extends Error {}

const firstLine = (error) => String(error?.message ?? error).split("\n")[0];

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

