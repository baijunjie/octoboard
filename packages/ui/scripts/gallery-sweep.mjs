// Loads every UI state gallery scenario in a real browser, lets the scenario's own steps run, and reads
// the red `[data-gallery-failure]` overlay (`src/fixtures/banner.ts`) the page raises when a step threw, a
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

import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createRun, firstLine, launchBrowser, loadChromium, loadFrame, readGallery, READ_TIMEOUT_MS, SetupError, startGalleryServer, wildcard } from "./gallery-browser.mjs";
import { parseArgs } from "node:util";
import { acquireSlot, describeWait } from "../../../scripts/verify/lock.mjs";

const packageDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

/** What a sweep without `--lang` loads. A translation is checked where it is written (each catalog is typed
 * to hold every message and plural category, `src/i18n/catalog.test.ts` checks the placeholders, and
 * `src/gallery/fixtures/fixtures.test.ts` checks that no step names a control in English copy or with
 * the English separator), so the gallery verifies English unless asked for more. */
const DEFAULT_LANGUAGES = ["en"];

const USAGE = `Usage: pnpm gallery:sweep [options] [scenario ...]

Loads gallery scenarios in a headless browser, runs their steps and fails on any gallery failure banner.
With no scenario given, every scenario is swept, in English and the light theme unless --lang and --theme
say otherwise.

  scenario              An id, or a pattern with * as a wildcard ("git-*"). Repeatable.
  --group <title>       Only the scenarios of the group whose title contains this text (case-insensitive).
  --lang <list>         Comma-separated languages, each one the app offers, or "all" for every language
                        with a catalog in the served app. Default: ${DEFAULT_LANGUAGES.join(", ")}.
  --theme <name>        light (default), dark or both. A scenario's steps do not depend on the theme, so
                        light settles a step that no longer runs; dark when asked for.
  --width <list>        Comma-separated window widths in pixels to load each scenario at, as the page's width
                        control would: a scenario that carries a width of its own keeps it and is loaded once, and one with a
                        minWidth is loaded no narrower than it.
                        Without it every scenario opens at its own width, or 1440. For proving that a scenario's
                        steps hold at a narrower window than the sweep's default (the Reopen pointer check of
                        history-loaded runs only at its own width).
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
const plural = (count, noun) => `${count} ${noun}${count === 1 ? "" : "s"}`;

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
        width: { type: "string" },
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
  const languages = values.lang === undefined ? DEFAULT_LANGUAGES : values.lang.trim() === "all" ? "all" : [...new Set(values.lang.split(",").map((s) => s.trim()).filter(Boolean))];
  if (languages.length === 0) throw new SetupError("--lang names no language");
  const theme = values.theme ?? "light";
  if (!["light", "dark", "both"].includes(theme)) throw new SetupError(`--theme is light, dark or both, not "${theme}"`);
  const jobs = values.jobs === undefined ? defaultJobs() : Number(values.jobs);
  if (!Number.isInteger(jobs) || jobs < 1) throw new SetupError(`--jobs is a positive whole number, not "${values.jobs}"`);
  const widths = values.width === undefined ? undefined : [...new Set(values.width.split(",").map((w) => Number(w.trim())))];
  if (widths && (widths.length === 0 || widths.some((w) => !Number.isInteger(w) || w < 1))) throw new SetupError(`--width is a comma-separated list of positive whole numbers, not "${values.width}"`);
  return {
    patterns: positionals,
    group: values.group,
    languages,
    themes: theme === "both" ? ["light", "dark"] : [theme],
    jobs,
    widths,
    url: values.url?.replace(/\/+$/, ""),
    list: values.list ?? false,
    playwright: values.playwright ?? process.env.OCTOBOARD_PLAYWRIGHT,
    chromium: values.chromium ?? process.env.OCTOBOARD_CHROMIUM,
  };
}

// ---- scenarios -----------------------------------------------------------------------------------

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
    const regexps = patterns.map((p) => ({ p, re: wildcard(p) }));
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
    const { sessionAriaLabel } = await import("/src/sessions/sessionLabel.ts");
    const { sessionShownStatusKind } = await import("/src/sessions/sessionStatus.ts");
    const { FIXTURE_PROVIDERS } = await import("/src/fixtures/fixtureProviders.ts");
    const { SCENARIOS } = await import("/src/gallery/fixtures/index.ts");
    const session = SCENARIOS.find((scenario) => scenario.id === "history-loaded").state.sessions.get("s-history");
    return {
      reopen: format(language, "sidebar.session.reopen"),
      liveSession: sessionAriaLabel((key, params) => format(language, key, params), language, FIXTURE_PROVIDERS, { ...session, status: "idle" }, [], undefined, undefined, sessionShownStatusKind({ ...session, status: "idle" }, new Set(), new Set())),
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
async function sweepOne(context, origin, scenario, language, theme, width) {
  const page = await context.newPage();
  try {
    await page.setViewportSize({ width: width ?? scenario.width ?? 1440, height: scenario.height ?? 900 });
    const query = new URLSearchParams({ scenario: scenario.id, theme, lang: language });
    // The scenario's live changes, applied after its steps and before the page is ready (the frame runs `live=`
    // first), as a viewer pressing each of its buttons would.
    if (scenario.live.length > 0) query.set("live", scenario.live.join(","));
    // The sweep reads text, not pixels, so it does not depend on the frames having been drawn.
    const failures = await loadFrame(page, `${origin}/gallery-frame.html?${query}`, {
      readyTimeoutMs: READY_TIMEOUT_MS,
      settleMs: SETTLE_MS,
      afterSettle: async (problems) => {
        if (scenario.id === "history-loaded" && language === "en" && width === undefined && problems.length === 0) await checkArchivedReopen(page, language);
      },
    });
    return failures.length > 0 ? failures.join(" | ") : undefined;
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
      for (let i = next++; i < jobs.length && !run.interrupted; i = next++) {
        const { scenario, language, theme, width } = jobs[i];
        const started = Date.now();
        const failure = await sweepOne(context, origin, scenario, language, theme, width);
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

const label = (r) => `${r.language}/${r.theme}${r.width ? `/${r.width}px` : ""}  ${r.scenario.id}`;

const run = createRun();

async function main() {
  const options = parse();
  const { chromium, own } = await loadChromium(options.playwright);
  const started = Date.now();
  // Held until the browser and the server are closed, so the next holder does not start while they still run.
  let release = () => {};
  try {
    const origin = options.url ?? (await startGalleryServer((server) => run.own("Vite server", server))).origin;
    const browser = run.own("Chromium", await launchBrowser(chromium, options.chromium, own));

    // The first load of a fresh dev server pre-bundles its dependencies, which can reload a page that is
    // already running; doing it once, alone, keeps that out of the concurrent pages.
    const gallery = await readGallery(browser, origin).catch((error) => {
      throw new SetupError(`Could not load the gallery from ${origin}: ${firstLine(error)}`);
    });
    if (options.languages === "all") options.languages = gallery.catalogLanguages;
    checkLanguages(options.languages, gallery.languages);
    const scenarios = select(gallery.scenarios, options);
    if (options.list) {
      for (const s of scenarios) console.log(s.id);
      return 0;
    }
    // A scenario that carries a width keeps it, as it does on the page, where picking one drops the width chosen
    // before.
    // One that cannot be narrower than its target exists in is loaded at that width, once.
    const widthsOf = (scenario) => (scenario.width || !options.widths ? [undefined] : [...new Set(options.widths.map((width) => Math.max(width, scenario.minWidth ?? 0)))]);
    const jobs = options.themes.flatMap((theme) =>
      options.languages.flatMap((language) => scenarios.flatMap((scenario) => widthsOf(scenario).map((width) => ({ scenario, language, theme, width })))),
    );
    const concurrency = Math.min(options.jobs, jobs.length);
    console.error(
      `Sweeping ${plural(scenarios.length, "scenario")} x ${options.languages.join(", ")} x ${options.themes.join(", ")}${options.widths ? ` x ${options.widths.join(", ")}px` : ""} = ${plural(jobs.length, "load")}; ${concurrency} at a time`,
    );
    release = await acquireSlot(`${path.basename(path.resolve(packageDir, "../.."))}: gallery sweep`, {
      root: stateDir && path.join(stateDir, "slots"),
      onWait: (holders, ahead) => console.error(`Waiting for a free heavy-step slot; ${describeWait(holders, ahead)}`),
    });
    return await sweepPages(browser, origin, jobs, concurrency, started);
  } finally {
    await run.close();
    release();
  }
}

/** Loads `jobs` and reports; returns the exit status. */
async function sweepPages(browser, origin, jobs, concurrency, started) {
  let done = 0;
  const interactive = process.stderr.isTTY;
  const results = await runAll(browser, origin, jobs, concurrency, (r) => {
    if (run.interrupted) return;
    done++;
    if (r.failure) {
      if (interactive) process.stderr.write("\r\x1b[K");
      console.log(`FAIL  ${label(r)}\n      ${r.failure}`);
    }
    if (interactive) process.stderr.write(`\r${done}/${jobs.length}`);
  });
  if (interactive) process.stderr.write("\r\x1b[K");
  if (run.interrupted) return process.exitCode;

  // A failure seen under load is checked again alone. That is a diagnosis, not a pardon: one that
  // reproduces is a scenario that no longer runs; one that does not is a step that races an animation or
  // a fixture reply that races a render, which only a busy machine shows, and that is a defect of the
  // scenario all the same. Both fail the sweep; the second is named, with what to try.
  const failed = results.filter((r) => r.failure);
  const loadOnly = [];
  if (failed.length > 0 && concurrency > 1) {
    console.error(`Re-running ${plural(failed.length, "failure")} alone to see whether they reproduce...`);
    const again = await runAll(browser, origin, failed, 1, () => {});
    if (run.interrupted) return process.exitCode;
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
  if (!run.interrupted) {
    if (error instanceof SetupError) console.error(error.message);
    else console.error(error);
    process.exitCode = 2;
  }
}

await run.finish();
