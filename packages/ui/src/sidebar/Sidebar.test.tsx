// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { Sidebar } from "./Sidebar";
import type { FocusTarget, SidebarHandlers } from "./types";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// jsdom has no ResizeObserver, which FadeOverflow and HeroUI's ScrollShadow need to mount.
class StubResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
globalThis.ResizeObserver ??= StubResizeObserver as unknown as typeof ResizeObserver;
// Reduced motion keeps the view's slide animation, which jsdom has no `animate` for, from playing.
window.matchMedia = ((query: string) => ({ matches: true, media: query })) as unknown as typeof window.matchMedia;

const main = consoleOf("c-1", "Main");
const web = projectOf("p-web", main.id, "Website");
const hub1 = sessionOf("s-hub-1", main.id, undefined, "Hub", "idle", { colour: "teal" });
const hub2 = sessionOf("s-hub-2", main.id, undefined, "Hub 2", "idle", { colour: "rose" });
const sessions = [hub1, hub2, sessionOf("web-bound", main.id, web.id, "Fix the layout", "idle", { bound_to: hub1.id })];
const sidebarWidth = { width: 280, min: 200, max: 480, setWidth: () => {}, persist: () => {}, reset: () => {} };

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  vi.useRealTimers();
  document.body.replaceChildren();
});

/** Mounts the sidebar in `first`'s focus mode with focus on the control `pick` finds, then moves it
 * to `next`'s, as pressing that control does; returns what holds focus once the hand-off has run,
 * and whether the control that held it is still in the document. */
function focusAfterMove(first: FocusTarget, pick: (nav: HTMLElement) => HTMLElement | null, next: FocusTarget): { active: Element | null; pickedConnected: boolean } {
  const daemon: Daemon = {
    store: createStateStore({}),
    request: vi.fn(),
    toastError: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
  const handlers = { onFocus: vi.fn(), onOpenDialog: vi.fn(), onOpenArchive: vi.fn(), onSelectSession: vi.fn(), onSwitchConsoleSession: vi.fn() } as unknown as SidebarHandlers;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const render = (focus: FocusTarget) =>
    act(() =>
      root.render(
        <DaemonProvider value={daemon}>
          <Sidebar {...handlers} projects={[web]} sessions={sessions} currentConsole={main} focus={focus} open sidebarWidth={sidebarWidth} />
        </DaemonProvider>,
      ),
    );
  render(first);
  const picked = pick(container.querySelector("nav")!);
  act(() => picked?.focus());
  render(next);
  // Focus fell to the body with the control that held it, as in a browser.
  (document.activeElement as HTMLElement | null)?.blur();
  act(() => void vi.advanceTimersByTime(100));
  const active = document.activeElement;
  const pickedConnected = picked?.isConnected ?? false;
  act(() => root.unmount());
  return { active, pickedConnected };
}

it("hands lost focus to the new current chip when one console session's focus mode gives way to another's", () => {
  const { active, pickedConnected } = focusAfterMove(
    { consoleSession: hub1 },
    (nav) => nav.querySelector<HTMLElement>('[role=group] button:not([data-switch-current])'),
    { consoleSession: hub2 },
  );
  expect(active?.getAttribute("aria-label")).toBe("Hub 2");
  expect(active?.hasAttribute("data-switch-current")).toBe(true);
  // The focus mode is keyed, so the chip that was pressed is replaced rather than reused.
  expect(pickedConnected).toBe(false);
});

it("goes to the way out of focus mode when a project's focus mode gives way to a console session's", () => {
  const { active } = focusAfterMove(
    { project: web },
    (nav) => nav.querySelector<HTMLElement>("button[aria-label^='Enter focus mode for']"),
    { consoleSession: hub1 },
  );
  expect(active?.hasAttribute("data-focus-exit")).toBe(true);
});
