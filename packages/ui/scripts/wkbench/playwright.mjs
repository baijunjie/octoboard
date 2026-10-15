// Runs the same instrument and driver scripts in Playwright's WebKit or in Chrome, for the two things the
// system WKWebView cannot give: a Chrome CPU profile and a run on a machine without the app's engine.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { stallWatch } from "./stall.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const MAC_CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

/** The browser type to launch and its options: `engine` is "webkit" or "chromium". Chrome is the
 * installed one (`chromium` path, else the macOS default) so no browser has to be downloaded. */
export async function launch(engine, { chromium } = {}) {
  const { default: playwright } = await import("playwright");
  if (engine === "webkit") return playwright.webkit.launch();
  const executablePath = chromium ?? process.env.OCTOBOARD_CHROMIUM ?? (fs.existsSync(MAC_CHROME) ? MAC_CHROME : undefined);
  return playwright.chromium.launch({ executablePath });
}

export async function newBenchPage(browser, { throttle } = {}) {
  const context = await browser.newContext({ viewport: { width: 1440, height: 872 }, deviceScaleFactor: 2 });
  await context.addInitScript(fs.readFileSync(path.join(HERE, "instrument.js"), "utf8"));
  const page = await context.newPage();
  const cdp = await context.newCDPSession(page).catch(() => undefined);
  if (throttle && cdp) await cdp.send("Emulation.setCPUThrottlingRate", { rate: Number(throttle) });
  return { page, cdp };
}

/** Runs `script` ("driver.js" or "domcost.js") on `url` and returns the lines it posted, up to its
 * `done`. `onLine` sees each as it arrives. Rejects when the page posts nothing for 150 s or the run
 * outlasts `timeoutMs`. */
export async function runScript({ engine, url, script, chromium, throttle, onLine = () => {}, timeoutMs = 240_000 }) {
  const browser = await launch(engine, { chromium });
  let watch;
  let timer;
  try {
    const { page } = await newBenchPage(browser, { throttle });
    // The drivers post through the WKWebView's message handler; here it becomes a console line.
    await page.addInitScript(() => {
      window.webkit = { messageHandlers: { bench: { postMessage: (s) => console.log("WKBENCH " + s) } } };
    });
    const lines = [];
    let finish;
    const finished = new Promise((resolve) => (finish = resolve));
    const limit = new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error(`no result after ${timeoutMs / 1000} s`)), timeoutMs);
      watch = stallWatch(script, (message) => reject(new Error(message)));
    });
    limit.catch(() => {}); // it may fire before the race below starts; the race still sees it
    page.on("console", (m) => {
      const text = m.text();
      if (!text.startsWith("WKBENCH ") || watch.seen(text.slice(8))) return;
      lines.push(text.slice(8));
      onLine(text.slice(8));
      if (text.includes('"done":true')) finish();
    });
    await page.goto(url);
    // The driver is an async function whose promise would be awaited here, and the limits above must
    // start now; ending on a number keeps it out of the evaluation's result.
    await page.evaluate(`${fs.readFileSync(path.join(HERE, script), "utf8")}\n;0`);
    await Promise.race([finished, limit]);
    return lines;
  } finally {
    clearTimeout(timer);
    watch?.stop();
    await browser.close();
  }
}
