// @vitest-environment jsdom
import React, { act, type ComponentProps } from "react";
import { createRoot } from "react-dom/client";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { ChromeButton } from "../components/ChromeButton";
import { Rail } from "../components/Rail";
import { consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import { isolate } from "../i18n/isolate";
import { PANE_ID } from "../layout/paneOverlay";
import type { PanePeek } from "../layout/usePaneToggles";
import { PlatformProvider } from "../platform/react";
import type { Session } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { Sidebar } from "./Sidebar";
import type { FocusTarget, SidebarHandlers } from "./types";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// Reduced motion keeps the view's slide animation, which jsdom has no `animate` for, from playing.
window.matchMedia = ((query: string) => ({ matches: true, media: query })) as unknown as typeof window.matchMedia;

const main = consoleOf("c-1", "Main");
const web = projectOf("p-web", main.id, "Website");
const hub1 = sessionOf("s-hub-1", main.id, undefined, "Hub", "idle", { colour: "teal" });
const hub2 = sessionOf("s-hub-2", main.id, undefined, "Hub 2", "idle", { colour: "pink" });
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

beforeEach(() => {
  vi.useFakeTimers();
  // Registered first, so it runs after the unmounts that `mountSidebar` registers.
  onTestFinished(() => void vi.useRealTimers());
  // jsdom loads no stylesheet, so the token the floating card's pointer watch reads is declared here.
  document.documentElement.style.setProperty("--peek-gap", "8px");
});

/** The sidebar's props, and the sessions the stand-in daemon holds, which the sidebar reads from its store. */
type SidebarTestProps = Partial<ComponentProps<typeof Sidebar>> & { sessions?: Session[] };

const sessionMap = (list: Session[]) => new Map(list.map((session) => [session.id, session]));

/** Mounts the sidebar over a stand-in daemon, `beside` next to it; `render` mounts it again with
 * other props. */
function mountSidebar(initial: SidebarTestProps, beside?: React.ReactNode) {
  const daemon: Daemon = {
    store: createStateStore({ sessions: sessionMap(sessions) }),
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
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const render = ({ sessions: held, ...props }: SidebarTestProps) =>
    act(() => {
      if (held) daemon.store.setState({ sessions: sessionMap(held) });
      root.render(
        <PlatformProvider value={{ kind: "browser" }}>
          <DaemonProvider value={daemon}>
            {beside}
            <Sidebar {...handlers} projects={[web]} shownConsole={main} open sidebarWidth={sidebarWidth} {...props} />
          </DaemonProvider>
        </PlatformProvider>,
      );
    });
  render(initial);
  return { container, root, render };
}

it("announces a project filter's empty result once, clears it when matches return, and leaves focus in the field", async () => {
  const { container, root, render } = mountSidebar({});
  const mountedStatus = container.querySelector('[role="status"]');
  expect(mountedStatus?.textContent).toBe("");
  act(() => void container.querySelector<HTMLButtonElement>('button[aria-label="Filter projects"]')!.click());
  const input = document.querySelector<HTMLInputElement>('input[aria-label="Project name contains"]')!;
  expect(input).not.toBeNull();
  act(() => input.focus());
  const type = (value: string) => act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  act(() => void vi.advanceTimersByTime(100));
  type("missing");
  expect(container.textContent).toContain("No matching projects");
  const status = container.querySelector('[role="status"]');
  expect(status).not.toBeNull();
  expect(status).toBe(mountedStatus);
  expect(status!.closest('[aria-hidden="true"]')).toBeNull();
  expect(status!.textContent).toBe("No matching projects");
  expect(document.activeElement).toBe(input);
  const visible = Array.from(container.querySelectorAll("span"))
    .find((node) => node.textContent === "No matching projects")!;
  expect(visible.parentElement!.getAttribute("aria-hidden")).toBe("true");

  const updates: MutationRecord[] = [];
  const observer = new MutationObserver((records) => updates.push(...records));
  observer.observe(status!, { childList: true, characterData: true, subtree: true });
  onTestFinished(() => observer.disconnect());
  type("missing again");
  render({});
  await act(async () => {});
  expect(container.querySelector('[role="status"]')).toBe(status);
  expect(updates).toHaveLength(0);

  type("Website");
  expect(status!.textContent).toBe("");
  type("missing");
  expect(status!.textContent).toBe("No matching projects");
  type("");
  expect(status!.textContent).toBe("");
  expect(document.activeElement).toBe(input);
  render({ projects: [] });
  expect(status!.textContent).toBe("");
  act(() => root.unmount());
  await act(async () => void await vi.runAllTimersAsync());
  expect(vi.getTimerCount()).toBe(0);
});

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
    (nav) => nav.querySelector<HTMLElement>("[role=group] button:not([data-switch-current])"),
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
  expect(hiddenClass.split(" ")).not.toContain("docked:rounded-xl");
  expect(hiddenClass.split(" ")).not.toContain("docked:start-[calc(var(--rail-width)+var(--peek-gap))]");
  act(() => hidden.root.unmount());

  const peeking = mountSidebar({ open: false, peek: { active: true, keep: vi.fn(), leave: vi.fn() } });
  const peekClass = peeking.container.querySelector("nav")!.className;
  expect(peekClass.split(" ")).toContain("docked:rounded-xl");
  expect(peekClass.split(" ")).toContain("docked:start-[calc(var(--rail-width)+var(--peek-gap))]");
  act(() => peeking.root.unmount());
});

it("positions a new hover card without motion, then enables its enter and leave transitions", () => {
  const originalMatchMedia = window.matchMedia;
  window.matchMedia = ((query: string) => ({ matches: false, media: query })) as unknown as typeof window.matchMedia;
  onTestFinished(() => { window.matchMedia = originalMatchMedia; });
  const peek = { active: false, keep: vi.fn(), leave: vi.fn() };
  const { container, render } = mountSidebar({ open: false, peek });
  render({ open: false, peek: { ...peek, active: true } });
  const nav = container.querySelector("nav")!;
  expect(nav.classList.contains("docked:transition-none")).toBe(true);
  expect(nav.classList.contains("docked:invisible")).toBe(true);
  // Fake timers run animation frames at 16ms intervals. The first paints the offscreen card;
  // the next applies the destination with its transition enabled.
  act(() => void vi.advanceTimersByTime(16));
  expect(nav.classList.contains("docked:invisible")).toBe(true);
  act(() => void vi.advanceTimersByTime(16));
  expect(nav.classList.contains("docked:transition-none")).toBe(false);
  expect(nav.classList.contains("docked:invisible")).toBe(false);
  render({ open: false, peek });
  expect(nav.classList.contains("docked:transition-none")).toBe(false);
  expect(nav.classList.contains("docked:invisible")).toBe(true);
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
  const rows = Array.from(container.querySelectorAll<HTMLElement>("[data-sessions] div[role=button]"));
  // The lead session is ranked against the unbound session as ever (idle before interrupted) and
  // its own sessions follow it, ranked among themselves (idle before working).
  expect(rows.map((row) => row.getAttribute("aria-label"))).toEqual([
    `${isolate("Rework the gallery")} session, Claude Code (${isolate("Default")}), Awaiting instructions, Bound to ${isolate("Hub")}`,
    `${isolate("Chase the flaky test")} session, Claude Code (${isolate("Default")}), Awaiting instructions, Bound to ${isolate("Rework the gallery")}`,
    `${isolate("Measure the first paint")} session, Claude Code (${isolate("Default")}), Working, Bound to ${isolate("Rework the gallery")}`,
    `${isolate("Tidy the changelog")} session, Claude Code (${isolate("Default")}), Interrupted`,
  ]);
  const badges = rows.map((row) => row.querySelector(".session-colour-dot") !== null);
  expect(badges).toEqual([true, false, false, false]);
  // The two sessions of the team are inset under their owner, the other rows are not.
  expect(rows.map((row) => row.parentElement!.className.includes("ms-[17px]"))).toEqual([false, true, true, false]);
  act(() => root.unmount());
});

/** The floating sidebar card in a 1440x800 window with a 48px rail and an 8px gap: x 56..336 and
 * y 48..792, with the real toggle and the real rail beside it. `Sidebar` is what arms the pointer
 * watch, and the toggle's and the avatars' attributes are what it matches on. */
function mountPeeking() {
  let slideAway: ReturnType<typeof setTimeout> | undefined;
  const hide = vi.fn();
  const peek = {
    active: true,
    keep: vi.fn(() => clearTimeout(slideAway)),
    leave: vi.fn(() => {
      clearTimeout(slideAway);
      slideAway = setTimeout(() => hide(), SLIDE_AWAY_MS);
    }),
  } satisfies Pick<PanePeek, "active" | "keep" | "leave">;
  // `Rail` reads the platform for its notifications bell, which a browser without the capability omits.
  const beside = (
    <>
      <ChromeButton label="Toggle" onPress={() => {}} controls={PANE_ID.sidebar}>
        <svg />
      </ChromeButton>
      <Rail
        consoles={[main]}
        currentConsoleId={main.id}
        onSelectConsole={vi.fn()}
        onConsoleEnter={vi.fn()}
        onConsoleLeave={vi.fn()}
        hoverOpensSidebar
        onOpenDialog={vi.fn()}
        onSelectSession={vi.fn()}
        onOpenArchive={vi.fn()}
        waitingCount={0}
        onNextWaiting={vi.fn()}
        onOpenSettings={vi.fn()}
        focusTerminal={vi.fn()}
      />
    </>
  );
  const { container } = mountSidebar({ open: false, peek }, beside);
  const pane = container.querySelector<HTMLElement>("[data-pane=sidebar]")!;
  Object.defineProperties(pane, { offsetLeft: { value: 56 }, offsetWidth: { value: 280 }, offsetTop: { value: 48 }, offsetHeight: { value: 744 } });
  pane.getBoundingClientRect = () => ({ top: 48, bottom: 792, left: 56, right: 336 }) as DOMRect;
  const pointer = (type: string, init: MouseEventInit = {}) => {
    const event = new MouseEvent(type, { bubbles: true, ...init });
    Object.defineProperty(event, "pointerType", { value: "mouse" });
    return event;
  };
  const on = (element: Element, clientX: number, clientY: number) =>
    act(() => void element.dispatchEvent(pointer("pointermove", { clientX, clientY })));
  return {
    peek,
    hide,
    leavePane: () => act(() => void pane.dispatchEvent(pointer("pointerout"))),
    avatar: () => container.querySelector("[data-region=rail] button[aria-label*=Main]")!,
    onToggle: (x: number, y: number) => on(container.querySelector("[data-pane-toggle] svg")!, x, y),
    onAvatar: (x: number, y: number) => on(container.querySelector("[data-region=rail] button[aria-label*=Main] span")!, x, y),
    onContent: (x: number, y: number) => on(document.body, x, y),
  };
}
const SLIDE_AWAY_MS = 200;
const wait = (ms: number) => act(() => void vi.advanceTimersByTime(ms));

it("marks a console's avatar on the rail as a place that keeps the floating sidebar up", () => {
  expect(mountPeeking().avatar().hasAttribute("data-sidebar-peek-keep")).toBe(true);
});

it("watches the pointer while its card floats, and lets the card go once the pointer is off it", () => {
  const { peek, hide, onContent } = mountPeeking();
  onContent(400, 300);
  expect(peek.leave).toHaveBeenCalledTimes(1);
  wait(SLIDE_AWAY_MS);
  expect(hide).toHaveBeenCalledTimes(1);
});

it("keeps the card when the pointer goes from its toggle down through the gap above it", () => {
  const { peek, hide, onToggle, onContent } = mountPeeking();
  onToggle(100, 20);
  onToggle(101, 22);
  // The toggle sits in the top bar, which is outside the card, yet the pointer on it is not leaving.
  expect(peek.leave).not.toHaveBeenCalled();
  // The bar's lower edge is the card's top less the gap: the pointer is still over the bar (outside)
  // for a move, then in the gap, which cancels the slide-away the bar's move armed.
  onContent(120, 38);
  onContent(120, 44);
  wait(1000);
  expect(hide).not.toHaveBeenCalled();
});

it("keeps the card when the pointer moves 3px from it into the gap above it", () => {
  const { hide, leavePane, onContent } = mountPeeking();
  onContent(120, 51);
  leavePane();
  onContent(120, 48 - 3);
  wait(1000);
  expect(hide).not.toHaveBeenCalled();
});

it("keeps the card while the pointer rests on a console's avatar on the rail, and lets it go off it", () => {
  const { peek, hide, onAvatar, onContent } = mountPeeking();
  onAvatar(24, 100);
  onAvatar(25, 101);
  wait(1000);
  expect(peek.leave).not.toHaveBeenCalled();
  expect(hide).not.toHaveBeenCalled();
  onContent(24, 300);
  wait(SLIDE_AWAY_MS);
  expect(hide).toHaveBeenCalledTimes(1);
});

it("lets the card go once the pointer is off the card, its gap, its toggle and the avatars", () => {
  const { peek, hide, onToggle, onContent } = mountPeeking();
  onToggle(100, 20);
  expect(peek.leave).not.toHaveBeenCalled();
  onContent(400, 20);
  expect(peek.leave).toHaveBeenCalledTimes(1);
  wait(SLIDE_AWAY_MS);
  expect(hide).toHaveBeenCalledTimes(1);
});


it("updates a session's visible and accessible name as the daemon title changes", () => {
  const session = { ...sessions[2], terminal_title: "Explain the gallery" };
  const { container, render } = mountSidebar({ sessions: [hub1, session] });
  const row = () => container.querySelector<HTMLElement>(`[data-flip="${session.id}"] [role="button"]`)!;
  const first = `${isolate("Explain the gallery")}｜${isolate(session.title)}`;
  expect(row().textContent).toContain(first);
  expect(row().getAttribute("aria-label")).toContain(isolate(first));
  render({ sessions: [hub1, { ...session, terminal_title: "Compare layouts" }] });
  const changed = `${isolate("Compare layouts")}｜${isolate(session.title)}`;
  expect(row().textContent).toContain(changed);
  expect(row().getAttribute("aria-label")).toContain(isolate(changed));
  render({ sessions: [hub1, { ...session, terminal_title: null }] });
  expect(row().textContent).toContain(session.title);
  expect(row().textContent).not.toContain("｜");
});


it("uses the card entry alone for a new console, then animates switches while the card is visible", () => {
  vi.stubGlobal("matchMedia", () => ({ matches: false }));
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => setTimeout(() => callback(0), 16));
  vi.stubGlobal("cancelAnimationFrame", clearTimeout);
  const originalAnimate = HTMLElement.prototype.animate;
  const animate = vi.fn();
  HTMLElement.prototype.animate = animate;
  onTestFinished(() => {
    if (originalAnimate) HTMLElement.prototype.animate = originalAnimate;
    else Reflect.deleteProperty(HTMLElement.prototype, "animate");
    vi.unstubAllGlobals();
  });
  const peek = { active: false, keep: vi.fn(), leave: vi.fn() };
  const other = consoleOf("c-2", "Other");
  const { render } = mountSidebar({ peek, projects: [], sessions: [] });
  render({ shownConsole: other, peek: { ...peek, active: true }, projects: [], sessions: [] });
  expect(animate).not.toHaveBeenCalled();
  act(() => void vi.advanceTimersByTime(16));
  act(() => void vi.advanceTimersByTime(16));
  render({ shownConsole: main, peek: { ...peek, active: true }, projects: [], sessions: [] });
  expect(animate).toHaveBeenCalledOnce();
  expect(animate).toHaveBeenCalledWith(
    [{ transform: "translateY(6px)", opacity: 0 }, {}],
    { duration: 240, easing: "cubic-bezier(0.2, 0, 0, 1)" },
  );
});


it("expands only projects with unarchived sessions and keeps interrupted projects undimmed", () => {
  onTestFinished(() => {
    act(() => vi.runOnlyPendingTimers());
    expect(vi.getTimerCount()).toBe(0);
  });
  const stopped = projectOf("p-stopped", main.id, "Stopped");
  const archived = projectOf("p-archived", main.id, "Archived", { pinned: true });
  const empty = projectOf("p-empty", main.id, "Empty");
  const { container } = mountSidebar({
    projects: [archived, empty, stopped, web],
    sessions: [
      sessionOf("s-stopped", main.id, stopped.id, "Stopped task", "interrupted"),
      sessionOf("s-archived", main.id, archived.id, "Old task", "archived"),
      ...sessions,
    ],
  });
  const press = (label: string) => act(() => container.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!.click());
  press("Collapse all projects");
  press("Expand projects with unarchived sessions");
  const groups = Array.from(container.querySelectorAll<HTMLElement>("[data-flip]")).filter((row) =>
    [stopped.id, archived.id, empty.id, web.id].includes(row.dataset.flip!),
  );
  expect(groups.map((row) => row.dataset.flip)).toEqual([web.id, stopped.id, archived.id, empty.id]);
  expect(groups.map((row) => row.hasAttribute("data-inactive"))).toEqual([false, false, true, true]);
  expect(groups.map((row) => row.querySelector('[aria-expanded="true"]') !== null)).toEqual([true, true, false, false]);
});
