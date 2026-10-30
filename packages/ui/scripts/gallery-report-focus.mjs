// Fixture-only report replacement regression. Run against this checkout's gallery:
// node scripts/gallery-report-focus.mjs http://localhost:5187
// OCTOBOARD_CHROMIUM can select an installed Chromium; no daemon requests are made.
import assert from "node:assert/strict";
import { existsSync, mkdirSync, readdirSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { chromium } from "playwright";

const origin = process.argv[2] ?? "http://localhost:5187";
const cache = process.env.PLAYWRIGHT_BROWSERS_PATH ?? (process.platform === "darwin"
  ? path.join(os.homedir(), "Library/Caches/ms-playwright") : path.join(os.homedir(), ".cache/ms-playwright"));
const shell = existsSync(cache) && readdirSync(cache).filter((name) => name.startsWith("chromium_headless_shell-")).sort().at(-1);
const shellPlatform = process.platform === "darwin" ? `mac-${process.arch}` : "linux64";
const fallback = shell && path.join(cache, shell, `chrome-headless-shell-${shellPlatform}/chrome-headless-shell`);
const executablePath = process.env.OCTOBOARD_CHROMIUM
  ?? (!existsSync(chromium.executablePath()) && fallback && existsSync(fallback) ? fallback : undefined);
const browser = await chromium.launch({ executablePath, headless: true });
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
page.setDefaultTimeout(15000);
const results = [];
const screenshots = process.env.OCTOBOARD_REPORT_SCREENSHOTS;
if (screenshots) mkdirSync(screenshots, { recursive: true });
let serial = 0;
const errors = [];
page.on("pageerror", (error) => errors.push(String(error)));
const active = () => page.evaluate(() => {
  const element = document.activeElement;
  const style = getComputedStyle(element);
  return { tag: element.tagName, label: element.getAttribute("aria-label"),
    inReport: !!element.closest("#panel-report"), visible: element.getAttribute("data-focus-visible"),
    outlineStyle: style.outlineStyle, outlineWidth: style.outlineWidth, outlineColor: style.outlineColor,
    shadow: style.boxShadow };
});
// HeroUI animates the ring shadow on focus; wait for a visible spread before any F6.
const settledActive = async () => {
  await page.waitForFunction(() => {
    const element = document.activeElement;
    return element?.getAttribute("data-focus-visible") === "true"
      && getComputedStyle(element).boxShadow.split(/,(?![^()]*\))/).some((shadow) => {
        const spread = shadow.match(/0px 0px 0px ([\d.]+)px/);
        return spread && Number(spread[1]) > 0.5 && !/rgba\([^)]*, 0\)|\/ 0\)/.test(shadow);
      });
  }, undefined, { timeout: 5000 });
  return active();
};
const checkRing = (value) => {
  assert.equal(value.tag, "BUTTON");
  assert.equal(value.inReport, true);
  assert.equal(value.visible, "true");
  const shadows = value.shadow.split(/,(?![^()]*\))/);
  assert.ok(shadows.some((shadow) => {
    const spread = shadow.match(/0px 0px 0px ([\d.]+)px/);
    return spread && Number(spread[1]) > 0.5 && !/rgba\([^)]*, 0\)|\/ 0\)/.test(shadow);
  }), `Expected a visible computed HeroUI ring shadow: ${JSON.stringify(value)}`);
};
const append = async (session = "s-hub-2") => {
  const id = `focus-probe-${++serial}`;
  await page.evaluate(({ id, session }) => document.dispatchEvent(new CustomEvent("gallery:report-page-created", {
    detail: { id, session_id: session, html: `<!doctype html><html><body><h1>${id}</h1><label>Version <input value="2.0"></label><button>Continue</button></body></html>`, created_at: Date.now() },
  })), { id, session });
  return id;
};
const frame = () => page.frameLocator("#panel-report iframe");
const load = async (language, theme, width = 1440) => {
  console.error(`Report fixture: ${language}/${theme}/${width}`);
  await page.setViewportSize({ width, height: 900 });
  await page.goto(`${origin}/gallery-frame.html?scenario=report-two-console-sessions&theme=${theme}&lang=${language}&chrome=1`);
  await page.locator("[data-gallery-ready]").waitFor({ timeout: 60000 });
  assert.deepEqual(await page.locator("[data-gallery-failure]").allTextContents(), [], `${language}/${theme}/${width}`);
  await page.evaluate(() => document.fonts.ready);
  await frame().getByRole("textbox", { name: "Version" }).waitFor();
};
try {
  for (const language of ["en", "zh-Hans", "ar"]) for (const theme of ["light", "dark"]) {
    await load(language, theme);
    const old = await page.locator("#panel-report iframe").elementHandle();
    await frame().getByRole("textbox", { name: "Version" }).focus();
    assert.equal(await frame().getByRole("textbox", { name: "Version" }).evaluate((el) => el === document.activeElement), true);
    assert.equal(await page.evaluate(() => document.activeElement === document.querySelector("#panel-report iframe")), true);
    const id = await append();
    await frame().getByRole("heading", { name: id }).waitFor();
    assert.equal(await old.evaluate((el) => el.isConnected), false);
    const destination = await settledActive();
    checkRing(destination); // Deliberately before F6: recovery alone must not pass this check.
    if (screenshots) await page.screenshot({ path: path.join(screenshots, `${language}-${theme}.png`) });
    await page.keyboard.press("F6");
    assert.notEqual((await active()).tag, "BODY");
    results.push({ language, theme, focusedReplacement: destination, f6: await active() });
  }

  await load("en", "light");
  const outside = page.locator('[data-region="topbar"] button').first();
  await outside.focus();
  const elsewhereId = await append();
  await frame().getByRole("heading", { name: elsewhereId }).waitFor();
  assert.equal(await outside.evaluate((el) => el === document.activeElement), true);
  results.push({ focusElsewhere: await active() });

  await load("en", "light");
  const pager = page.getByRole("button", { name: "Previous page", exact: true });
  await pager.focus();
  const pagerId = await append();
  await frame().getByRole("heading", { name: pagerId }).waitFor();
  assert.equal(await pager.evaluate((el) => el === document.activeElement), true);
  results.push({ pagerFocusPreserved: true });

  await load("en", "light");
  await frame().getByRole("textbox", { name: "Version" }).focus();
  await page.getByRole("button", { name: "Previous page", exact: true }).click();
  checkRing(await settledActive());
  const historical = await page.locator("#panel-report iframe").elementHandle();
  const manualId = await append();
  await page.waitForTimeout(100);
  assert.equal(await historical.evaluate((el) => el.isConnected), true);
  assert.equal(await frame().getByRole("heading", { name: "Hub 2, second" }).count(), 1);
  await page.getByRole("button", { name: "Next page", exact: true }).click();
  assert.equal(await frame().getByRole("textbox", { name: "Version" }).isDisabled(), true);
  await page.getByRole("button", { name: "Next page", exact: true }).click();
  await frame().getByRole("heading", { name: manualId }).waitFor();
  await frame().getByRole("textbox", { name: "Version" }).focus();
  const rearmedId = await append();
  await frame().getByRole("heading", { name: rearmedId }).waitFor();
  checkRing(await settledActive());
  const sessionFrame = await page.locator("#panel-report iframe").elementHandle();
  const otherSessionId = await append("s-hub-1");
  await page.waitForTimeout(100);
  assert.equal(await sessionFrame.evaluate((el) => el.isConnected), true);
  await page.getByText("Hub", { exact: true }).click();
  await frame().getByRole("heading", { name: otherSessionId }).waitFor();
  await page.getByText("Hub 2", { exact: true }).click();
  await frame().getByRole("heading", { name: rearmedId }).waitFor();
  results.push({ manualHistoryPreserved: true, followingRearmed: true, otherSessionIndependent: true });

  await load("en", "light");
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await dialog.waitFor();
  const modalActive = await page.evaluateHandle(() => document.activeElement);
  await append();
  await page.waitForTimeout(100);
  assert.equal(await modalActive.evaluate((el) => el === document.activeElement), true);
  assert.equal(await page.evaluate(() => !!document.activeElement.closest('[role="dialog"]')), true);
  results.push({ modalFocusPreserved: true });

  await load("ar", "dark", 760);
  await frame().getByRole("textbox", { name: "Version" }).focus();
  const narrowId = await append();
  await frame().getByRole("heading", { name: narrowId }).waitFor();
  checkRing(await settledActive());
  if (screenshots) await page.screenshot({ path: path.join(screenshots, "ar-dark-narrow.png") });
  const geometry = await page.evaluate(() => {
    const el = document.activeElement;
    const rect = el.getBoundingClientRect();
    return { left: rect.left, right: rect.right, width: innerWidth, direction: getComputedStyle(document.documentElement).direction };
  });
  assert.equal(geometry.direction, "rtl");
  assert.ok(geometry.left >= 0 && geometry.right <= geometry.width);
  results.push({ narrow: geometry, active: await active() });
  assert.deepEqual(errors, []);
  console.log(JSON.stringify({ method: "Sequential Chromium fixture-only interaction; not native assistive technology evidence", origin, results }, null, 2));
} finally {
  await browser.close();
}
