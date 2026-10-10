// @vitest-environment jsdom
import { act, type ComponentProps } from "react";
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
// A lead session (bound to a console session, with sessions of its own), its two sessions and an
// unbound session beside it, to read the project list's three levels off.
const lead = sessionOf("web-lead", main.id, web.id, "Rework the gallery", "idle", { bound_to: hub1.id });
const team = [
  sessionOf("web-team-1", main.id, web.id, "Chase the flaky test", "idle", { bound_to: lead.id }),
  sessionOf("web-team-2", main.id, web.id, "Measure the first paint", "working", { bound_to: lead.id }),
];
const unbound = sessionOf("web-unbound", main.id, web.id, "Tidy the changelog", "interrupted");
const sidebarWidth = { width: 280, min: 200, max: 480, setWidth: () => {}, persist: () => {}, reset: () => {} };

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  vi.useRealTimers();
  document.body.replaceChildren();
});

/** Mounts the sidebar over a stand-in daemon; `render` mounts it again with other props. */
function mountSidebar(initial: Partial<ComponentProps<typeof Sidebar>>) {
  const daemon: Daemon = {
    store: createStateStore({}),
    request: vi.fn(),
    toastError: vi.fn(),
    toastNotice: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
  const handlers = { onFocus: vi.fn(), onOpenDialog: vi.fn(), onOpenArchive: vi.fn(), onSelectSession: vi.fn(), onSwitchConsoleSession: vi.fn() } as unknown as SidebarHandlers;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const render = (props: Partial<ComponentProps<typeof Sidebar>>) =>
    act(() =>
      root.render(
        <DaemonProvider value={daemon}>
          <Sidebar {...handlers} projects={[web]} sessions={sessions} shownConsole={main} open sidebarWidth={sidebarWidth} {...props} />
        </DaemonProvider>,
      ),
    );
  render(initial);
  return { container, root, render };
}

/** Mounts the sidebar in `first`'s focus mode with focus on the control `pick` finds, then moves it
 * to `next`'s, as pressing that control does; returns what holds focus once the hand-off has run,
 * and whether the control that held it is still in the document. */
function focusAfterMove(first: FocusTarget, pick: (nav: HTMLElement) => HTMLElement | null, next: FocusTarget): { active: Element | null; pickedConnected: boolean } {
  const { container, root, render } = mountSidebar({ focus: first });
  const picked = pick(container.querySelector("nav")!);
  act(() => picked?.focus());
  render({ focus: next });
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

it.each([
  [true, 1],
  [false, 0],
])("reports a press inside the sidebar while it is hidden-docked, and only then (peeking: %s)", (peeking, calls) => {
  const onPeekPress = vi.fn();
  const { container, root } = mountSidebar({
    open: false,
    peek: peeking ? { active: false, keep: vi.fn(), leave: vi.fn() } : undefined,
    onPeekPress,
  });
  act(() => void container.querySelector("nav button")!.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true })));
  expect(onPeekPress).toHaveBeenCalledTimes(calls);
  act(() => root.unmount());
});

it("keeps the docked column while hidden, and uses the inset card only while a peek is up", () => {
  const hidden = mountSidebar({ open: false, peek: { active: false, keep: vi.fn(), leave: vi.fn() } });
  const hiddenClass = hidden.container.querySelector("nav")!.className;
  expect(hiddenClass).not.toContain("rounded-xl");
  expect(hiddenClass).not.toContain("peek-gap");
  act(() => hidden.root.unmount());

  const peeking = mountSidebar({ open: false, peek: { active: true, keep: vi.fn(), leave: vi.fn() } });
  const peekClass = peeking.container.querySelector("nav")!.className;
  expect(peekClass).toContain("rounded-xl");
  expect(peekClass).toContain("peek-gap");
  act(() => peeking.root.unmount());
});

it("shows the console it is given, and a hover alone commits nothing", () => {
  const other = consoleOf("c-2", "Other");
  const onPeekPress = vi.fn();
  const { container, root } = mountSidebar({
    shownConsole: other,
    open: false,
    peek: { active: true, keep: vi.fn(), leave: vi.fn() },
    onPeekPress,
  });
  expect(container.querySelector("nav")!.textContent).toContain("Other");
  act(() => void container.querySelector("nav")!.dispatchEvent(new MouseEvent("pointerover", { bubbles: true })));
  expect(onPeekPress).not.toHaveBeenCalled();
  act(() => root.unmount());
});

it("lists a project session's own sessions under it, and the binding badge only for a console session's owner", () => {
  const { container, root } = mountSidebar({ sessions: [hub1, hub2, lead, ...team, unbound] });
  const rows = Array.from(container.querySelectorAll<HTMLElement>("[data-sessions] [role=button][data-marquee-scope]"));
  // The lead session is ranked against the unbound session as ever (idle before interrupted) and
  // its own sessions follow it, ranked among themselves (working before idle).
  expect(rows.map((row) => row.getAttribute("aria-label"))).toEqual([
    "Rework the gallery session, Claude Code (Default), Awaiting instructions, Bound to Hub",
    "Measure the first paint session, Claude Code (Default), Working, Bound to Rework the gallery",
    "Chase the flaky test session, Claude Code (Default), Awaiting instructions, Bound to Rework the gallery",
    "Tidy the changelog session, Claude Code (Default), Interrupted",
  ]);
  const badges = rows.map((row) => row.querySelector('[style*="--console-session"]') !== null);
  expect(badges).toEqual([true, false, false, false]);
  // The two sessions of the team are inset under their owner, the other rows are not.
  expect(rows.map((row) => row.parentElement!.className.includes("ms-4"))).toEqual([false, true, true, false]);
  act(() => root.unmount());
});
