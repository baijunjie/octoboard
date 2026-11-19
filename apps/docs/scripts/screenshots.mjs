// Captures the screenshots of the Features pages from the UI state gallery, in English, into
// `src/assets/features/<id>.png`, where Astro optimizes them for the pages that show them. What is captured is
// listed in `screenshots/*.mjs` (see `screenshots/entries.mjs` for the entry fields).
//
//   pnpm --filter @octoboard/docs screenshots [--only <id or pattern> ...] [--url <origin>]
//
// The gallery is the real App over the fixture daemon (`packages/ui/src/gallery/README.md`): each entry loads its
// frame page, waits for `data-gallery-ready` and fails when the page shows a `[data-gallery-failure]` overlay. The
// dev server, the browser and the machine-wide heavy-step slot are the ones the UI's gallery sweep uses
// (`packages/ui/scripts/gallery-browser.mjs`, `scripts/verify/lock.mjs`).
import { mkdirSync, readdirSync, renameSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { parseArgs } from "node:util";
import { createRun, firstLine, launchBrowser, loadChromium, loadFrame, readGallery, SetupError, startGalleryServer } from "../../../packages/ui/scripts/gallery-browser.mjs";
import { acquireSlot, describeWait } from "../../../scripts/verify/lock.mjs";
import { frameUrl, loadManifest, selectEntries, viewportProblem } from "../screenshots/manifest.mjs";

const docsDir = path.resolve(import.meta.dirname, "..");
const outDir = path.join(docsDir, "src/assets/features");

const READY_TIMEOUT_MS = 60_000;
/** What a page is given after it says it is ready, for what a step did not wait for to settle before the capture. */
const SETTLE_MS = 300;

const USAGE = `Usage: pnpm --filter @octoboard/docs screenshots [options]

Captures the manifest's screenshots (apps/docs/screenshots/*.mjs) into apps/docs/src/assets/features/.

  --only <pattern>      An entry id, or a pattern with * as a wildcard ("quota-*"). Repeatable. Default: all.
  --url <origin>        Use the gallery served at this origin (a running \`pnpm gallery\`) instead of starting a
                        dev server of this checkout.
  --list                Print the entry ids that would be captured and exit.
  --playwright <dir>    A Playwright other than the UI package's own. Also OCTOBOARD_PLAYWRIGHT.
  --chromium <path>     The Chromium executable to launch. Also OCTOBOARD_CHROMIUM.
  -h, --help            This text.

Exit status: 0 when every entry was captured, 1 when any failed (nothing is written for a failed entry), 2 when the
run could not start, 128 plus the signal's number when interrupted.`;

function parse() {
  let parsed;
  try {
    parsed = parseArgs({
      allowPositionals: false,
      options: {
        only: { type: "string", multiple: true },
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
  const { values } = parsed;
  if (values.help) {
    console.log(USAGE);
    process.exit(0);
  }
  return {
    only: values.only ?? [],
    url: values.url?.replace(/\/+$/, ""),
    list: values.list ?? false,
    playwright: values.playwright ?? process.env.OCTOBOARD_PLAYWRIGHT,
    chromium: values.chromium ?? process.env.OCTOBOARD_CHROMIUM,
  };
}

/** The bounding box of the one visible element `selector` matches, or an Error naming the entry field (`role`). */
async function soleBox(page, selector, role) {
  const target = page.locator(selector);
  const count = await target.count();
  if (count !== 1) throw new Error(`the ${role} selector ${selector} matches ${count} elements, not 1`);
  const box = await target.boundingBox();
  if (!box) throw new Error(`the ${role} selector ${selector} is not visible`);
  return box;
}

/** Leaves only the element `selector` matches visible, over a transparent page, so a clipped capture is the element
 * alone and its rounded corners show nothing of what was behind it (the window, the dimmed backdrop of a modal). Its
 * ancestors stay laid out. Those that reach beyond the capture (the window, a modal's overlay) lose their backgrounds,
 * shadows and filters, while one inside it (the surface a popover draws its menu on) keeps its own look. What `keep`
 * selects (a floating layer the picture is of, such as a tooltip drawn in its own portal) stays visible with everything
 * in it, and the portal around it loses its look as an ancestor does. Everything else is hidden. */
async function isolate(page, selector, padding, keep = []) {
  for (const layer of keep) {
    const marked = await page.locator(layer).evaluateAll((elements) => {
      for (const element of elements) element.setAttribute("data-shot-keep", "");
      return elements.length;
    });
    if (marked === 0) throw new Error(`the keep selector ${layer} matches no element`);
  }
  await page.locator(selector).evaluate((target, pad) => {
    const kept = target.getBoundingClientRect();
    const within = (box) => box.left >= kept.left - pad - 1 && box.top >= kept.top - pad - 1 && box.right <= kept.right + pad + 1 && box.bottom <= kept.bottom + pad + 1;
    target.setAttribute("data-shot-target", "");
    for (let parent = target.parentElement; parent; parent = parent.parentElement) {
      parent.setAttribute(within(parent.getBoundingClientRect()) ? "data-shot-surface" : "data-shot-ancestor", "");
    }
    for (const layer of document.querySelectorAll("[data-shot-keep]")) {
      for (let parent = layer.parentElement; parent && !parent.hasAttribute("data-shot-ancestor") && !parent.hasAttribute("data-shot-surface"); parent = parent.parentElement) {
        parent.setAttribute("data-shot-ancestor", "");
      }
    }
    const style = document.createElement("style");
    style.textContent = `
      [data-shot-ancestor] { background: transparent !important; box-shadow: none !important; backdrop-filter: none !important; filter: none !important; }
      :not([data-shot-ancestor]):not([data-shot-surface]):not([data-shot-target]):not([data-shot-target] *):not([data-shot-keep]):not([data-shot-keep] *) { visibility: hidden !important; }`;
    document.head.append(style);
  }, padding);
}

/** The PNG of one entry, or an Error saying why it cannot be taken. */
async function capture(browser, origin, entry) {
  const context = await browser.newContext({ viewport: { width: entry.width, height: entry.height }, deviceScaleFactor: entry.scale });
  const page = await context.newPage();
  try {
    const failures = await loadFrame(page, frameUrl(origin, entry), { readyTimeoutMs: READY_TIMEOUT_MS, settleMs: SETTLE_MS });
    if (failures.length > 0) throw new Error(`the gallery failed: ${failures.join(" | ")}`);
    if (entry.hover) {
      const box = await soleBox(page, entry.hover, "hover");
      await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
      await page.waitForTimeout(SETTLE_MS);
    }
    if (!entry.clip) return await page.screenshot({ type: "png" });
    const box = await soleBox(page, entry.clip.selector, "clip");
    await isolate(page, entry.clip.selector, entry.clip.padding, entry.keep);
    const pad = entry.clip.padding;
    const x = Math.max(0, box.x - pad);
    const y = Math.max(0, box.y - pad);
    return await page.screenshot({
      type: "png",
      omitBackground: true,
      clip: { x, y, width: Math.min(entry.width, box.x + box.width + pad) - x, height: Math.min(entry.height, box.y + box.height + pad) - y },
    });
  } finally {
    await context.close().catch(() => {});
  }
}

const run = createRun();

/** Written aside and moved into place, so a failed write never leaves half an image under a page's import. The
 * aside file is removed whatever happens; one a killed run left behind is removed by the next run's start. */
function writeImage(file, png) {
  const aside = `${file}.tmp`;
  try {
    writeFileSync(aside, png);
    renameSync(aside, file);
  } finally {
    rmSync(aside, { force: true });
  }
}

async function main() {
  const options = parse();
  // A manifest or pattern mistake is the writer's to fix, so it is reported as the message alone.
  const entries = await loadManifest().then((all) => selectEntries(all, options.only)).catch((error) => {
    throw new SetupError(firstLine(error), { cause: error });
  });
  if (options.list) {
    for (const entry of entries) console.log(entry.id);
    return 0;
  }
  if (entries.length === 0) {
    console.error("The manifest has no entries");
    return 0;
  }
  const { chromium, own } = await loadChromium(options.playwright);
  let release = () => {};
  try {
    const origin = options.url ?? (await startGalleryServer((server) => run.own("Vite server", server))).origin;
    const browser = run.own("Chromium", await launchBrowser(chromium, options.chromium, own));
    const gallery = await readGallery(browser, origin).catch((error) => {
      throw new SetupError(`Could not load the gallery from ${origin}: ${firstLine(error)}`, { cause: error });
    });
    const scenarios = new Map(gallery.scenarios.map((scenario) => [scenario.id, scenario]));
    const unknown = entries.filter((entry) => !scenarios.has(entry.scenario));
    if (unknown.length > 0) {
      throw new SetupError(`The gallery has no scenario for: ${unknown.map((entry) => `${entry.id} (${entry.scenario}, in ${entry.group})`).join(", ")}`);
    }
    const unfit = entries.map((entry) => viewportProblem(entry, scenarios.get(entry.scenario))).filter(Boolean);
    if (unfit.length > 0) throw new SetupError(unfit.join("\n"));
    release = await acquireSlot(`${path.basename(path.resolve(docsDir, "../.."))}: docs screenshots`, {
      onWait: (holders, ahead) => console.error(`Waiting for a free heavy-step slot; ${describeWait(holders, ahead)}`),
    });
    mkdirSync(outDir, { recursive: true });
    for (const name of readdirSync(outDir).filter((name) => name.endsWith(".png.tmp"))) rmSync(path.join(outDir, name));
    let failed = 0;
    for (const entry of entries) {
      if (run.interrupted) return process.exitCode;
      try {
        const png = await capture(browser, origin, entry);
        writeImage(path.join(outDir, `${entry.id}.png`), png);
        console.log(`ok    ${entry.id}  (${(png.length / 1024).toFixed(0)} KiB)`);
      } catch (error) {
        // An interrupt closes the browser under the capture, which is not the entry's failure.
        if (run.interrupted) return process.exitCode;
        failed++;
        console.log(`FAIL  ${entry.id}  (${entry.scenario})\n      ${firstLine(error)}`);
      }
    }
    console.log(`\n${entries.length - failed} captured, ${failed} failed`);
    return failed > 0 ? 1 : 0;
  } finally {
    await run.close();
    release();
  }
}

try {
  process.exitCode = await main();
} catch (error) {
  // When interrupted, the handler is closing everything and exits with the signal's code.
  if (!run.interrupted) {
    console.error(error instanceof SetupError ? error.message : error);
    process.exitCode = 2;
  }
}

await run.finish();
