// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { appConfig } from "./appConfig";
import { ErrorBoundary } from "./ErrorBoundary";
import { errorAlerts } from "./errorAlerts";
import { t } from "./i18n/language";
import type { ExitCapability, PlatformAdapter } from "./platform";
import { PlatformProvider } from "./platform/react";
import { createStateStore, DaemonProvider, type Daemon } from "./store";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** A component that throws on its first render, which is what the boundary is there to catch. */
function Crashing(): React.ReactElement {
  throw new Error("boom", { cause: undefined });
}

/** Mounts the boundary over a component that crashes, measuring every element as `scrollHeight`
 * tall in a `clientHeight` box, since jsdom has no layout and the fade's state follows those. */
async function mountCrashed(sizes: { scrollHeight: number; clientHeight: number }, Child: () => React.ReactElement = Crashing, platform: PlatformAdapter = { kind: "browser" }, daemon?: Daemon, beforeFrame?: (container: HTMLElement) => void): Promise<HTMLElement> {
  const restore = (["scrollHeight", "clientHeight"] as const).map((key) => {
    const original = Object.getOwnPropertyDescriptor(HTMLElement.prototype, key);
    Object.defineProperty(HTMLElement.prototype, key, { configurable: true, get: () => sizes[key] });
    return () => (original ? Object.defineProperty(HTMLElement.prototype, key, original) : delete (HTMLElement.prototype as unknown as Record<string, unknown>)[key]);
  });
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
        <DaemonProvider value={daemon}>
          <ErrorBoundary>
            <Child />
          </ErrorBoundary>
        </DaemonProvider>
      </PlatformProvider>,
    ),
  );
  beforeFrame?.(container);
  // HeroUI writes the fade's state on the next frame.
  await act(() => new Promise<void>((resolve) => requestAnimationFrame(() => resolve())));
  return container;
}

/** The scroller holding the stack of the crash, on the surface under the page's lines. */
function stack(container: HTMLElement): HTMLElement {
  const block = container.querySelector('[data-slot="empty-state"] > [data-slot="surface"] pre')?.closest<HTMLElement>("[data-slot=scroll-shadow]");
  if (!block) throw new Error("the crash screen shows no stack");
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
  const scrolling = scroller(await mountCrashed({ scrollHeight: 1000, clientHeight: 100 }));
  expect(scrolling.getAttribute("role")).toBe("region");
  expect(scrolling.getAttribute("aria-label")).toBeTruthy();
  expect(scrolling.tabIndex).toBe(0);
});

it.each([
  ["the stack", stack],
  ["the screen's column", column],
])("does not make %s a region when it fits", async (_name, scroller) => {
  const fitting = scroller(await mountCrashed({ scrollHeight: 100, clientHeight: 100 }));
  expect(fitting.hasAttribute("role")).toBe(false);
  expect(fitting.hasAttribute("tabindex")).toBe(false);
});

it("says the crash in the application's words on the page and keeps the error's own text on a surface under them, with Try again the only button", async () => {
  const container = await mountCrashed({ scrollHeight: 100, clientHeight: 100 });
  expect(container.querySelector("h1")?.textContent).toBe(t("error.crashed.heading"));
  expect(container.textContent).toContain(t("error.crashed"));
  expect(container.textContent).toContain(t("error.crashed.retry"));
  expect([...container.querySelectorAll("button")].map((button) => button.textContent)).toEqual(["Try again"]);
  expect(stack(container).textContent).toContain("boom");
});

it("opens no error dialog over the page and announces the failure from the page itself, inserted after the first paint", async () => {
  let first: Element | null | undefined;
  const container = await mountCrashed({ scrollHeight: 100, clientHeight: 100 }, Crashing, { kind: "browser" }, undefined, (shown) => {
    first = shown.querySelector('[role="alert"]');
  });
  expect(first).toBeNull();
  expect(errorAlerts.getSnapshot()).toEqual([]);
  const announced = container.querySelector('[role="alert"]')!;
  expect(announced.querySelector("h1")?.textContent).toBe(t("error.crashed.heading"));
  expect(announced.textContent).toContain(t("error.crashed"));
});

it("offers Report issue after Try again, as a link to the prefilled Issue page, and leaves the guidance to the remedy line", async () => {
  const container = await mountCrashed({ scrollHeight: 100, clientHeight: 100 });
  const row = [...container.querySelectorAll('[data-slot="empty-state"] > div')].at(-1)!;
  const [retry, report] = [...row.children];
  expect(retry.textContent).toBe("Try again");
  expect(report.textContent).toBe(t("error.issue.report"));
  const link = report as HTMLAnchorElement;
  expect(link.tagName).toBe("A");
  const prefilled = new URL(link.href).searchParams;
  expect(prefilled.get("title")).toMatch(/^Unexpected error: /);
  expect(prefilled.get("body")).toContain("boom");
  // The remedy line asks for the report, so the surface holds the stack alone.
  expect(container.querySelector('[data-slot="empty-state"] > [data-slot="surface"] p')).toBeNull();
});

it("keeps the report guidance, naming the address, where the platform cannot open links", async () => {
  const container = await mountCrashed({ scrollHeight: 100, clientHeight: 100 }, Crashing, { kind: "tauri" });
  expect(container.querySelector("a[data-external-link]")).toBeNull();
  const guidance = container.querySelector('[data-slot="empty-state"] > [data-slot="surface"] p')!;
  expect(guidance.textContent).toContain("This may be a bug");
  expect(guidance.querySelector("[data-inline-reference]")?.textContent).toBe(appConfig.issueUrl);
});

it("removes the names of the user's records from the report, reading them from the daemon around the boundary", async () => {
  const store = createStateStore({
    homeDir: "/Users/alice",
    projects: new Map([["p", { name: "Zeta Project", path: "/Users/alice/zeta" }]]) as never,
  });
  function CrashingOnProject(): React.ReactElement {
    throw new Error("Cannot open Zeta Project at /Users/alice/zeta/a.ts");
  }
  const container = await mountCrashed({ scrollHeight: 100, clientHeight: 100 }, CrashingOnProject, { kind: "browser" }, { store } as unknown as Daemon);
  const prefilled = new URL(container.querySelector<HTMLAnchorElement>("a[data-external-link]")!.href).searchParams;
  expect(`${prefilled.get("title")}${prefilled.get("body")}`).not.toMatch(/Zeta|alice/);
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
  expect(stack(container).textContent).toBe(shown);
});

it("has no Quit button where the platform can quit, yet answers a quit gesture at once", async () => {
  let onQuitRequested = () => {};
  const confirmQuit = vi.fn(async () => {});
  const exit: ExitCapability = {
    register: (handlers) => {
      onQuitRequested = handlers.onQuitRequested;
      return () => {};
    },
    heartbeat: async () => {},
    confirmQuit,
  };
  const container = await mountCrashed({ scrollHeight: 100, clientHeight: 100 }, Crashing, { kind: "tauri", exit });
  expect([...container.querySelectorAll("button")].map((button) => button.textContent)).toEqual(["Try again"]);
  await act(async () => onQuitRequested());
  expect(confirmQuit).toHaveBeenCalledExactlyOnceWith({ restart: false });
});
