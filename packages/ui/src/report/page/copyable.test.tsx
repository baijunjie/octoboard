// @vitest-environment jsdom
import { act } from "react";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { errorAlerts } from "../../components/alerts/errorAlerts";
import type { PageContent } from "../../protocol";
import { click } from "../../test/dom";
import { fakeTimers, runOutTimers } from "../../test/clock";
import { browserWith, renderPage, type PageOptions } from "./pageTestSupport";

vi.mock("../../preferences/theme", () => ({ useOctoboardTheme: () => ({ resolved: "light" }) }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const originalExecCommand = document.execCommand;
afterEach(() => {
  document.execCommand = originalExecCommand;
});

const copyButton = (host: ParentNode) => host.querySelector<HTMLButtonElement>('button[aria-label^="Copy"], button[aria-label="Copied"]')!;

/** Makes every scroller overflow sideways, as the text of a long line does, for the regions that exist only then. */
function overflowing(): void {
  for (const [key, value] of Object.entries({ clientWidth: 100, clientHeight: 100, scrollWidth: 300, scrollHeight: 100 })) {
    const mock = vi.spyOn(HTMLElement.prototype, key as "clientWidth", "get").mockReturnValue(value);
    onTestFinished(() => mock.mockRestore());
  }
}

/** A page of one block, rendered on the fake clock the copied state's reset needs, with a clipboard that records. */
async function renderBlock(block: PageContent["body"][number], options: PageOptions = {}, writeText = vi.fn(async (_text: string) => {})) {
  fakeTimers();
  const page = await renderPage({ body: [block] }, { ...options, platform: browserWith({ writeText }) });
  await act(async () => {});
  runOutTimers();
  return { ...page, writeText };
}

const PADDED = "  first line\n\n\tsecond line  \n";

it("shows the label and the text of a copyable block with its whitespace kept", async () => {
  const { host } = await renderBlock({ type: "copyable", label: "Token", text: PADDED });
  expect(host.textContent).toContain("Token");
  const pre = host.querySelector("pre")!;
  expect(pre.textContent).toBe(PADDED);
});

it("puts exactly the text of a copyable block on the clipboard, with no label and no trimming, and says it was copied", async () => {
  const { host, writeText } = await renderBlock({ type: "copyable", label: "Token", text: PADDED });
  expect(copyButton(host).getAttribute("aria-label")).toBe("Copy \u2068Token\u2069");
  await click(copyButton(host));
  expect(writeText).toHaveBeenCalledExactlyOnceWith(PADDED);
  expect(copyButton(host).getAttribute("aria-label")).toBe("Copied");
});

it("offers the copy button on a copyable block that has no label", async () => {
  const { host, writeText } = await renderBlock({ type: "copyable", text: "just text" });
  await click(copyButton(host));
  expect(writeText).toHaveBeenCalledExactlyOnceWith("just text");
});

it("copies exactly the code of a code block", async () => {
  const code = "fn main() {\n    println!(\"hi\");\n}\n";
  const { host, writeText } = await renderBlock({ type: "code", label: "main.rs", language: "rust", code });
  expect(host.textContent).toContain("main.rs");
  await click(copyButton(host));
  expect(writeText).toHaveBeenCalledExactlyOnceWith(code);
  expect(copyButton(host).getAttribute("aria-label")).toBe("Copied");
});

it("copies exactly the diff of a diff block, with its lines drawn", async () => {
  const diff = "--- a/f\n+++ b/f\n@@ -1 +1 @@\n-old\n+new\n";
  const { host, writeText } = await renderBlock({ type: "diff", label: "Patch", diff });
  expect(host.textContent).toContain("Patch");
  expect(host.textContent).toContain("-old");
  expect(host.textContent).toContain("+new");
  await click(copyButton(host));
  expect(writeText).toHaveBeenCalledExactlyOnceWith(diff);
});

it.each<[string, PageOptions]>([
  ["a history page", { history: true }],
  ["a submitted page", { submission: {} }],
  ["a page whose submission is out", { pending: true }],
])("keeps the copy button of a block usable on %s", async (_, options) => {
  const { host, writeText } = await renderBlock({ type: "copyable", text: "keep me" }, options);
  expect(copyButton(host).disabled).toBe(false);
  await click(copyButton(host));
  expect(writeText).toHaveBeenCalledExactlyOnceWith("keep me");
});

it.each<[string, PageContent["body"][number]]>([
  ["copyable", { type: "copyable", text: "x" }],
  ["code", { type: "code", code: "x" }],
  ["diff", { type: "diff", diff: "+x" }],
])("keeps the copy button of a %s block usable on a read-only page", async (_, block) => {
  const { host, writeText } = await renderBlock(block, { history: true });
  await click(copyButton(host));
  expect(writeText).toHaveBeenCalledTimes(1);
});

it("raises the error alert with the catalog's copy-failed text when the copy fails, and does not say it was copied", async () => {
  Object.assign(document, { execCommand: vi.fn(() => false) });
  const writeText = vi.fn(async (_text: string) => { throw new Error("denied"); });
  const { host } = await renderBlock({ type: "copyable", text: "secret" }, {}, writeText);
  await click(copyButton(host));
  expect(errorAlerts.getSnapshot().map((alert) => alert.message)).toEqual(["Could not copy the text"]);
  expect(copyButton(host).getAttribute("aria-label")).toBe("Copy");
});

it("gives each block of a page a copy button of its own, each copying its own text", async () => {
  fakeTimers();
  const writeText = vi.fn(async (_text: string) => {});
  const { host } = await renderPage(
    { body: [{ type: "copyable", text: "one" }, { type: "code", code: "two" }, { type: "diff", diff: "+three" }] },
    { platform: browserWith({ writeText }) },
  );
  const buttons = [...host.querySelectorAll<HTMLButtonElement>('button[aria-label="Copy"]')];
  expect(buttons).toHaveLength(3);
  for (const button of buttons) await click(button);
  expect(writeText.mock.calls.map(([text]) => text)).toEqual(["one", "two", "+three"]);
});

it.each<[string, PageContent["body"][number]]>([
  ["a monospace copyable", { type: "copyable", label: "Command", text: "pnpm run --all" }],
  ["a code", { type: "code", label: "Command", code: "pnpm run --all" }],
  ["a diff", { type: "diff", label: "Command", diff: "+pnpm run --all" }],
])("scrolls %s block sideways in a named region of its own, while it overflows, instead of wrapping", async (_, block) => {
  overflowing();
  const { host } = await renderBlock(block);
  const region = host.querySelector<HTMLElement>('[role="region"][aria-label="Command"]')!;
  expect(region.tabIndex).toBe(0);
  expect(region.contains(copyButton(host))).toBe(false);
  expect(host.querySelector(".whitespace-pre-wrap")).toBeNull();
});

it.each<[string, PageContent["body"][number]]>([
  ["a monospace copyable", { type: "copyable", label: "Command", text: "short" }],
  ["a code", { type: "code", label: "Command", code: "short" }],
  ["a diff", { type: "diff", label: "Command", diff: "+short" }],
])("is no tab stop for %s block whose text fits", async (_, block) => {
  const { host } = await renderBlock(block);
  expect(host.querySelector('[role="region"]')).toBeNull();
});

it("keeps a copyable block that is not monospace wrapping, with no scroll region", async () => {
  const { host } = await renderBlock({ type: "copyable", label: "Message", monospace: false, text: "a long sentence" });
  expect(host.querySelector('[role="region"]')).toBeNull();
  expect(host.querySelector("pre")!.className).toContain("whitespace-pre-wrap");
});

it.each<[string, PageContent["body"][number]]>([
  ["a monospace copyable", { type: "copyable", text: "pnpm run --all" }],
  ["a code", { type: "code", code: "pnpm run --all" }],
  ["a diff", { type: "diff", diff: "+pnpm run --all" }],
])("names the scroll region of %s block without a label from the catalog", async (kind, block) => {
  overflowing();
  const { host } = await renderBlock(block);
  const box = host.querySelector<HTMLElement>('[role="region"]')!;
  expect(box.tabIndex).toBe(0);
  expect(box.getAttribute("aria-label")).toBe({ "a monospace copyable": "Text to copy", "a code": "Code", "a diff": "Diff" }[kind]);
});

it("names a block's copy button after the block's label, and a block with none after the action alone", async () => {
  const { host } = await renderBlock({ type: "code", label: "main.rs", code: "x" });
  expect(copyButton(host).getAttribute("aria-label")).toBe("Copy \u2068main.rs\u2069");
});

it.each<[string, PageContent["body"][number]]>([
  ["a monospace copyable", { type: "copyable", label: "Command", text: "pnpm run --all" }],
  ["a code", { type: "code", label: "Command", code: "pnpm run --all" }],
  ["a diff", { type: "diff", label: "Command", diff: "+pnpm run --all" }],
])("holds the copy button of %s block outside its scrolling text, over an opaque backing of the block's own fill", async (_, block) => {
  overflowing();
  const { host } = await renderBlock(block);
  const region = host.querySelector<HTMLElement>('[role="region"][aria-label="Command"]')!;
  const button = copyButton(host);
  expect(region.contains(button)).toBe(false);
  const backing = button.closest<HTMLElement>("[style*=background]")!;
  expect(backing.contains(region)).toBe(false);
  expect(backing.style.backgroundColor).not.toBe("");
  expect(backing.style.backgroundColor).not.toBe("transparent");
  // The block's fill, which the text scrolled beneath the button must not show through, is on the frame around the scroller.
  expect(backing.style.backgroundColor).toBe(region.parentElement!.style.backgroundColor);
});

it("keeps the copy button of a plain-text copyable block over a backing too, with the text wrapping around it", async () => {
  const { host } = await renderBlock({ type: "copyable", label: "Message", monospace: false, text: "a long sentence" });
  const button = copyButton(host);
  expect(button.closest<HTMLElement>("[style*=background]")!.style.backgroundColor).not.toBe("");
  expect(host.querySelector("pre")!.contains(button)).toBe(false);
});
