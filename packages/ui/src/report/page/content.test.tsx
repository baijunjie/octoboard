// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";

import type { PageContent } from "../../protocol";
import { LAZY_IMPORT_TIMEOUT_MS, settleLazyImport } from "../../test/lazyImport";
import { renderPage } from "./pageTestSupport";

vi.mock("../../preferences/theme", () => ({ useOctoboardTheme: () => ({ resolved: "light" }) }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** The words only a screen reader gets; the page's own status region is empty until it has something to say. */
const hiddenWords = (host: ParentNode) => [...host.querySelectorAll(".sr-only")].map((el) => el.textContent).filter(Boolean);

const block = (component: PageContent["body"][number]) => renderPage({ body: [component] });

/** Waits for the Markdown renderer, which loads on first use, to have drawn what `selector` names. */
const markdownDrawn = (host: ParentNode, selector: string) => vi.waitFor(async () => {
  await settleLazyImport();
  expect(host.querySelector(selector)).not.toBeNull();
}, { timeout: LAZY_IMPORT_TIMEOUT_MS });

describe("callout", () => {
  it("shows its title and its text as Markdown, as a note", async () => {
    const { host } = await block({ type: "callout", title: "Heads up", text: "Mind the **gap**" });
    await markdownDrawn(host, "strong");
    const note = host.querySelector('[role="note"]')!;
    expect(note.textContent).toContain("Heads up");
    expect(note.querySelector("strong")?.textContent).toBe("gap");
  });

  it.each([[undefined, "Information"], ["info", "Information"], ["success", "Success"], ["warning", "Warning"], ["danger", "Error"]] as const)(
    "names its %s tone for a screen reader",
    async (tone, word) => {
      const { host } = await block({ type: "callout", text: "Body", tone });
      expect(hiddenWords(host.querySelector('[role="note"]')!)).toEqual([word]);
    },
  );

  it("shows a callout without a title", async () => {
    const { host } = await block({ type: "callout", text: "Only a body" });
    // The plain fallback is a paragraph too; only the drawn document sits inside `.report-markdown`.
    await markdownDrawn(host, ".report-markdown p");
    expect(host.querySelector('[role="note"] .report-markdown')?.textContent).toContain("Only a body");
  });
});

describe("key_values", () => {
  it("shows each key with its value, in order, as a description list, numbers and booleans in the UI language", async () => {
    const { host } = await block({
      type: "key_values",
      items: [{ key: "Branch", value: "main" }, { key: "Lines", value: 12345 }, { key: "Clean", value: true }, { key: "Dirty", value: false }],
    });
    const list = host.querySelector("dl")!;
    expect([...list.querySelectorAll("dt")].map((el) => el.textContent)).toEqual(["Branch", "Lines", "Clean", "Dirty"]);
    expect([...list.querySelectorAll("dd")].map((el) => el.textContent)).toEqual(["main", "12,345", "Yes", "No"]);
  });
});

describe("stat", () => {
  it("shows its label and value, and the change beside it", async () => {
    const { host } = await block({ type: "stat", label: "Revenue", value: 1234567, change: "+4.2%" });
    expect(host.textContent).toContain("Revenue");
    expect(host.textContent).toContain("1,234,567");
    expect(host.textContent).toContain("+4.2%");
  });

  it.each([["up", "Up"], ["down", "Down"], ["flat", "Unchanged"]] as const)("gives a %s trend a word for a screen reader", async (trend, word) => {
    const { host } = await block({ type: "stat", label: "L", value: 1, change: "2", trend });
    expect(hiddenWords(host)).toEqual([word]);
  });

  it("shows a trend that comes with no change text, and no trend word where there is no trend", async () => {
    const withTrend = await block({ type: "stat", label: "L", value: 1, trend: "up" });
    expect(hiddenWords(withTrend.host)).toEqual(["Up"]);
    const without = await block({ type: "stat", label: "Plain", value: "ok" });
    expect(hiddenWords(without.host)).toEqual([]);
    expect(without.host.textContent).toContain("ok");
  });
});
