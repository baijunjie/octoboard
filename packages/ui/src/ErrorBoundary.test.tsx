// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { ErrorBoundary } from "./ErrorBoundary";
import { press } from "./components/detailsTestSupport";
import { t } from "./i18n/language";
import type { PlatformAdapter } from "./platform";
import { PlatformProvider } from "./platform/react";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** A component that throws on its first render, which is what the boundary is there to catch. */
function Crashing(): React.ReactElement {
  throw new Error("boom", { cause: undefined });
}

/** Mounts the boundary over a component that crashes, measuring every element as `scrollHeight`
 * tall in a `clientHeight` box, since jsdom has no layout and the fade's state follows those. */
async function mountCrashed(sizes: { scrollHeight: number; clientHeight: number }, Child: () => React.ReactElement = Crashing): Promise<HTMLElement> {
  const restore = (["scrollHeight", "clientHeight"] as const).map((key) => {
    const original = Object.getOwnPropertyDescriptor(HTMLElement.prototype, key);
    Object.defineProperty(HTMLElement.prototype, key, { configurable: true, get: () => sizes[key] });
    return () => (original ? Object.defineProperty(HTMLElement.prototype, key, original) : delete (HTMLElement.prototype as unknown as Record<string, unknown>)[key]);
  });
  const platform: PlatformAdapter = { kind: "browser" };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    restore.forEach((undo) => undo());
  });
  // React reports a caught render error on the console; the test asserts the fallback instead.
  const error = vi.spyOn(console, "error").mockImplementation(() => {});
  onTestFinished(() => error.mockRestore());
  act(() =>
    root.render(
      <PlatformProvider value={platform}>
        <ErrorBoundary>
          <Child />
        </ErrorBoundary>
      </PlatformProvider>,
    ),
  );
  // HeroUI writes the fade's state on the next frame.
  await act(() => new Promise<void>((resolve) => requestAnimationFrame(() => resolve())));
  return container;
}

/** The scroller holding the stack of the crash, in the popover the mark after the page's heading opens. */
async function stack(container: HTMLElement): Promise<HTMLElement> {
  press(container.querySelector(`button[aria-label="${t("error.crashed.markLabel")}"]`)!);
  const details = document.querySelector<HTMLElement>(`[role="dialog"][aria-label="${t("error.crashed.markLabel")}"]`)!;
  // HeroUI writes the fade's state on the next frame.
  await act(() => new Promise<void>((resolve) => requestAnimationFrame(() => resolve())));
  const block = details.querySelector("pre")?.closest<HTMLElement>("[data-slot=scroll-shadow]");
  if (!block) throw new Error("the crash screen's Details shows no stack");
  return block;
}

/** The screen's own content column. */
function column(container: HTMLElement): HTMLElement {
  const outer = container.querySelector<HTMLElement>("[data-slot=scroll-shadow]");
  if (!outer) throw new Error("the crash screen has no content column");
  return outer;
}

// Nothing else scrolls either the screen's column or the stack, so each takes focus itself — and
// only while it scrolls, since a tab stop that scrolls nothing is one the user passes for no reason.
// The column's half of this is `StartupScreen`'s, asserted here as the crash screen's.
it.each([
  ["the stack", stack],
  ["the screen's column", column],
])("makes %s a named region while it scrolls", async (_name, scroller) => {
  const scrolling = await scroller(await mountCrashed({ scrollHeight: 1000, clientHeight: 100 }));
  expect(scrolling.getAttribute("role")).toBe("region");
  expect(scrolling.getAttribute("aria-label")).toBeTruthy();
  expect(scrolling.tabIndex).toBe(0);
});

it.each([
  ["the stack", stack],
  ["the screen's column", column],
])("does not make %s a region when it fits", async (_name, scroller) => {
  const fitting = await scroller(await mountCrashed({ scrollHeight: 100, clientHeight: 100 }));
  expect(fitting.hasAttribute("role")).toBe(false);
  expect(fitting.hasAttribute("tabindex")).toBe(false);
});

it("says the crash in the application's words on the page and keeps the error's own text in a mark after the heading, with Try again and Quit the only buttons", async () => {
  const container = await mountCrashed({ scrollHeight: 100, clientHeight: 100 });
  expect(container.querySelector("h1")?.textContent).toBe(t("error.crashed.heading"));
  expect(container.textContent).toContain(t("error.crashed.retry"));
  expect(container.querySelector("pre")).toBeNull();
  expect(container.textContent).not.toContain("boom");
  const buttons = [...container.querySelectorAll("button")];
  expect(buttons.map((button) => button.textContent || button.getAttribute("aria-label"))).toEqual([t("error.crashed.markLabel"), "Try again"]);
  expect(buttons[0].previousElementSibling).toBe(container.querySelector("h1"));
  expect((await stack(container)).textContent).toContain("boom");
});

it.each([
  ["leaves out the name and message, as WebKit's does", "boom", "Crashing@app.js:1:1", "TypeError: boom\nCrashing@app.js:1:1"],
  ["already begins with them, as V8's does", "boom", "TypeError: boom\n    at Crashing (app.js:1:1)", "TypeError: boom\n    at Crashing (app.js:1:1)"],
  ["has a frame that contains the message", "render", "renderWithHooks@app.js:1:1", "TypeError: render\nrenderWithHooks@app.js:1:1"],
  ["belongs to an error with an empty message", "", "renderWithHooks@app.js:1:1", "TypeError\nrenderWithHooks@app.js:1:1"],
  ["is missing", "boom", undefined, "TypeError: boom"],
])("heads the diagnostic block with the error's name and message once when the stack %s", async (_case, message, trace, shown) => {
  function CrashingThus(): React.ReactElement {
    const error = new TypeError(message);
    error.stack = trace;
    throw error;
  }
  const container = await mountCrashed({ scrollHeight: 100, clientHeight: 100 }, CrashingThus);
  expect((await stack(container)).textContent).toBe(shown);
});
