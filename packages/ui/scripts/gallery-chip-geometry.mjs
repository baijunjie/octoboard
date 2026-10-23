// Separate browser layout regression checks; scenario initialization stays presentation-only.
// OCTOBOARD_CHROMIUM=/path/to/chrome node scripts/gallery-chip-geometry.mjs http://localhost:5175 [--trust-only]
import assert from "node:assert/strict";
import { chromium } from "playwright";
const origin = process.argv[2] ?? "http://localhost:5175";
const trustOnly = process.argv.includes("--trust-only");
const browser = await chromium.launch({ headless: true, ...(process.env.OCTOBOARD_CHROMIUM && { executablePath: process.env.OCTOBOARD_CHROMIUM }) });
const page = await browser.newPage();
const PADDING = { xs: [0, 4], sm: [1, 6], md: [2, 8], lg: [3, 10] };
const near = (a, b, subject) => assert.ok(Math.abs(a - b) < 0.75, `${subject}: ${a} != ${b}`);
async function open(scenario, language, width) {
  await page.setViewportSize({ width, height: 1000 });
  await page.goto(`${origin}/gallery-frame.html?${new URLSearchParams({ scenario, lang: language, theme: "light" })}`);
  await page.waitForSelector("html[data-gallery-ready]", { timeout: 60_000 });
  assert.equal(await page.locator("[data-gallery-failure]").count(), 0, `${scenario}/${language}: gallery failure`);
}
try {
  for (const language of ["en", "zh-Hans"]) {
    if (!trustOnly) {
      await open("code-colors", language, 1440);
      const samples = await page.locator("[data-prose-size]").evaluateAll((samples) => samples.map((sample) => ({
        size: Number(sample.dataset.proseSize),
        chips: [...sample.querySelectorAll("[data-prose-comparison] [data-inline-chip]")].map((chip) => {
          const style = getComputedStyle(chip), rect = chip.getClientRects()[0], code = chip.querySelector("code");
          return { kind: chip.dataset.chipKind, lineHeight: parseFloat(getComputedStyle(chip.closest("p")).lineHeight), height: rect.height, top: rect.top, size: parseFloat(style.fontSize), display: style.display, verticalAlign: style.verticalAlign, italic: style.fontStyle, codeSize: code && parseFloat(getComputedStyle(code).fontSize) };
        }),
      })));
      assert.equal(samples.length, 3);
      for (const sample of samples) {
        assert.equal(sample.chips.length, 5);
        for (const chip of sample.chips) {
          near(chip.size, sample.size, `${sample.size}px ${chip.kind} font`);
          near(chip.height, sample.chips[0].height, `${sample.size}px ${chip.kind} background height`);
          near(chip.top, sample.chips[0].top, `${sample.size}px ${chip.kind} baseline alignment`);
          assert.ok(chip.height < chip.lineHeight, `${sample.size}px ${chip.kind}: drawn height ${chip.height} must stay below the line height ${chip.lineHeight}`);
          assert.equal(chip.display, "inline");
          assert.equal(chip.verticalAlign, "baseline");
          if (chip.codeSize !== null) near(chip.codeSize, sample.size, `${sample.size}px code font`);
          if (chip.kind === "confirmation") assert.equal(chip.italic, "italic");
        }
      }
      const sizes = await page.locator("[data-chip-line-sample]").evaluateAll((cards) => cards.map((card) => ({
        card: card.dataset.chipLineSample,
        rows: [...card.querySelectorAll("[data-chip-size-sample]")].map((row) => {
          const rowStyle = getComputedStyle(row);
          return {
            size: row.dataset.chipSizeSample,
            fontSize: parseFloat(rowStyle.fontSize),
            lineHeight: parseFloat(rowStyle.lineHeight),
            chips: [...row.querySelectorAll("[data-inline-chip]")].map((chip) => {
              const style = getComputedStyle(chip), label = getComputedStyle(chip.querySelector(".chip__label")), code = chip.querySelector("code");
              return {
                text: chip.textContent, size: chip.dataset.size, display: style.display, verticalAlign: style.verticalAlign,
                font: parseFloat(style.fontSize), weight: style.fontWeight, family: style.fontFamily, lineHeight: parseFloat(style.lineHeight),
                block: parseFloat(style.paddingTop), inline: parseFloat(label.paddingLeft),
                height: Math.max(...[...chip.getClientRects()].map((rect) => rect.height)),
                codeSize: code && parseFloat(getComputedStyle(code).fontSize),
              };
            }),
          };
        }),
        standalone: [...card.querySelectorAll(":scope > [data-inline-chip]")].map((chip) => ({ size: chip.dataset.size, font: parseFloat(getComputedStyle(chip).fontSize), display: getComputedStyle(chip).display, block: parseFloat(getComputedStyle(chip).paddingTop), inline: parseFloat(getComputedStyle(chip.querySelector(".chip__label")).paddingLeft) })),
      })));
      assert.equal(sizes.length, 4);
      for (const card of sizes) {
        for (const row of card.rows) {
          assert.ok(row.chips.length > 0);
          for (const chip of row.chips) {
            const subject = `${card.card} ${row.size} ${chip.text}`;
            assert.equal(chip.size, row.size);
            near(chip.font, row.fontSize, `${subject} font`);
            near(chip.lineHeight, row.lineHeight, `${subject} line height`);
            near(chip.block, PADDING[row.size][0], `${subject} block padding`);
            near(chip.inline, PADDING[row.size][1], `${subject} inline padding`);
            assert.equal(chip.display, "inline");
            assert.equal(chip.verticalAlign, "baseline");
            assert.equal(chip.weight, "400");
            if (chip.codeSize !== null) near(chip.codeSize, row.fontSize, `${subject} code font`);
            assert.ok(chip.height < row.lineHeight, `${subject}: drawn height ${chip.height} must stay below the line height ${row.lineHeight}`);
          }
        }
        for (const chip of card.standalone) {
          near(chip.block, PADDING[chip.size][0], `${card.card} ${chip.size} block padding`);
          near(chip.inline, PADDING[chip.size][1], `${card.card} ${chip.size} inline padding`);
        }
      }
      await open("code-colors", language, 390);
      const sizeRowFailures = await page.locator("[data-chip-size-sample]").evaluateAll((rows) => rows.flatMap((row) => {
        const bounds = row.getBoundingClientRect();
        return [...row.querySelectorAll("[data-inline-chip]")].flatMap((chip) => [...chip.getClientRects()]
          .filter((rect) => rect.left < bounds.left - 1 || rect.right > bounds.right + 1)
          .map(() => `${row.dataset.chipSizeSample} ${chip.textContent}`));
      }));
      assert.deepEqual(sizeRowFailures, [], "Narrow size-sample chips must wrap within their row");
      const narrow = await page.locator("[data-prose-size]").evaluateAll((samples) => samples.map((sample) => {
        const failures = [];
        for (const chip of sample.querySelectorAll("[data-inline-chip]")) {
          const bounds = chip.closest("p").getBoundingClientRect();
          for (const rect of chip.getClientRects()) if (rect.left < bounds.left - 1 || rect.right > bounds.right + 1) failures.push(chip.textContent);
        }
        const rtl = sample.querySelector("[data-prose-rtl]");
        return { failures, direction: getComputedStyle(rtl).direction, technicalDirections: [...rtl.querySelectorAll("[data-inline-chip]:has(code)")].map((chip) => getComputedStyle(chip).direction) };
      }));
      for (const sample of narrow) {
        assert.deepEqual(sample.failures, [], "Narrow chips must wrap within prose");
        assert.equal(sample.direction, "rtl");
        assert.deepEqual(sample.technicalDirections, ["ltr", "ltr"]);
      }
      await open("configuration-import-review", language, 1440);
      const pairs = await page.locator("li").evaluateAll((rows) => rows.flatMap((row) => {
        const chips = [...row.children].filter((element) => element.tagName === "P").map((p) => p.querySelector("[data-inline-chip]:has(code)")).filter(Boolean);
        return chips.length < 2 ? [] : [chips.slice(0, 2).map((chip) => ({ text: chip.textContent, size: parseFloat(getComputedStyle(chip).fontSize), height: chip.getClientRects()[0].height, display: getComputedStyle(chip).display }))];
      }));
      assert.ok(pairs.length > 0, "Import review must show exported/target paths together");
      for (const pair of pairs) {
        for (const path of pair) { near(path.size, 14, `${path.text} font`); assert.equal(path.display, "inline"); }
        near(pair[0].height, pair[1].height, "Exported/target path background heights");
      }
    }
    for (const width of [1440, 390]) {
      await open("trust-prompt", language, width);
      const trust = await page.locator("[role=alertdialog], [role=dialog]").evaluate((dialog) => {
        const chip = [...dialog.querySelectorAll("[data-inline-chip]:has(code)")].find((chip) => chip.textContent === "~/code/website");
        const paragraph = chip?.closest("p");
        const bounds = paragraph?.getBoundingClientRect();
        return {
          blocks: dialog.querySelectorAll("pre, .app-code-block").length,
          path: chip && { size: parseFloat(getComputedStyle(chip).fontSize), display: getComputedStyle(chip).display },
          paragraph: paragraph?.textContent,
          sessionInParagraph: !!paragraph && [...paragraph.querySelectorAll("[data-inline-chip]")].some((chip) => chip.textContent === "Fix the summary layout"),
          contained: !!chip && !!bounds && [...chip.getClientRects()].every((rect) => rect.left >= bounds.left - 1 && rect.right <= bounds.right + 1),
        };
      });
      assert.equal(trust.blocks, 0, "A single trust path must use a chip, not a code block");
      assert.ok(trust.path, "Trust prompt project path must be visible as a chip");
      near(trust.path.size, 14, "Trust path font");
      assert.equal(trust.path.display, "inline");
      assert.ok(trust.paragraph.includes("Claude Code"), "Trust path must complete the requesting agent's sentence");
      assert.ok(trust.sessionInParagraph, "Trust path and requesting session chip must share one paragraph");
      assert.ok(trust.contained, "Trust path must stay within its paragraph at both widths");
    }
    console.log(trustOnly
      ? `${language}: trust question, session and path share one paragraph at 1440px/390px`
      : `${language}: 12px/14px/16px prose geometry, padding sizes below their line heights, 390px wrapping/RTL, import source/target equality, trust sentence at 1440px/390px passed`);
  }
} finally { await browser.close(); }
