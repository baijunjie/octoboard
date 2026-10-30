// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { App } from "./App";
import { PanelHeader } from "./components/PanelHeader";
import { mountForTooltip, openTooltip } from "./components/tooltipTestSupport";
import { ALL_AGENTS, consoleOf, projectOf, sessionOf, shellTerminalOf, snapshotState } from "./gallery/fixtures/builders";
import { isolate } from "./i18n/isolate";
import { touchEvent } from "./layout/touchTestSupport";
import { defaultStack } from "./panels/stackState";
import { usePanelStacks } from "./panels/usePanelStacks";
import { PlatformProvider } from "./platform/react";
import type { Project, RequestBody } from "./protocol";
import { createStateStore, DaemonProvider, type Daemon } from "./store";
import { ThemeProvider } from "./theme";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
// Each test mounts the whole App, which a machine under load can take longer than the default 5s for.
vi.setConfig({ testTimeout: 20_000 });

vi.mock("./browser/LazyProjectBrowser", () => ({
  LazyProjectBrowser: ({ project }: { project: Project }) => (
    <>
      <PanelHeader title={project.name} />
      <button type="button">in files</button>
    </>
  ),
}));
// The shell's text input stands in for a terminal view, inside the element that marks a shell terminal.
vi.mock("./terminal/ShellTerminalPanel", () => ({
  ShellTerminalPanel: () => (
    <>
      <PanelHeader title="Terminal" />
      <div data-shell-terminal>
        <textarea aria-label="shell input" data-region-landing />
      </div>
    </>
  ),
}));
vi.mock("./terminal/shellTerminals", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./terminal/shellTerminals")>();
  return { ...actual, startShellTerminal: vi.fn(async () => undefined) };
});
vi.mock("./report/ReportPanel", () => ({ ReportPanel: () => <PanelHeader title="Report" /> }));
// Every method of the agent terminal's controller does nothing; its `focus` is where focus handed to
// the agent terminal arrives.
const terminalFocus = vi.hoisted(() => vi.fn());
vi.mock("./terminal/TerminalController", () => ({
  TerminalController: class {
    constructor() {
      return new Proxy(this, { get: (target, key) => (key === "then" ? undefined : key === "focus" ? terminalFocus : key in target ? Reflect.get(target, key) : () => {}) });
    }
  },
}));

/** A window `width` px wide, which `resize` changes: the media queries answer `min-width` by it and
 * tell their listeners when the answer may have changed. */
function stubWidth(width: number, touch = false) {
  let current = width;
  const listeners = new Set<() => void>();
  vi.stubGlobal("matchMedia", (media: string) => {
    const min = Number(/min-width:\s*([\d.]+)px/.exec(media)?.[1]);
    return {
      media,
      get matches() {
        return media === "(hover: none)" ? touch : Number.isFinite(min) && current >= min;
      },
      addEventListener: (_: string, listener: () => void) => listeners.add(listener),
      removeEventListener: (_: string, listener: () => void) => listeners.delete(listener),
      addListener() {},
      removeListener() {},
    };
  });
  return (next: number) =>
    act(() => {
      current = next;
      for (const listener of [...listeners]) listener();
    });
}

function mountApp(width: number, touch = false, agents = false, waiting = false) {
  let stacks!: ReturnType<typeof usePanelStacks>;
  function StackReset() {
    stacks = usePanelStacks();
    return null;
  }
  const resize = stubWidth(width, touch);
  // The test's interactions: taps on a touch screen, a mouse's presses otherwise.
  document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: touch ? "touch" : "mouse", bubbles: true }));
  document.documentElement.style.setProperty("--rail-width", "48px");
  document.documentElement.style.setProperty("--breakpoint-docked", "1168px");
  document.documentElement.style.setProperty("--breakpoint-phone", "782px");
  onTestFinished(() => {
    act(() => {
      stacks.setStack("project", defaultStack("project"));
      stacks.setStack("report", defaultStack("report"));
    });
    vi.unstubAllGlobals();
    document.documentElement.style.removeProperty("--rail-width");
    document.documentElement.style.removeProperty("--breakpoint-docked");
    document.documentElement.style.removeProperty("--breakpoint-phone");
    try {
      localStorage.clear();
    } catch {
      // No storage: nothing was kept.
    }
  });
  const console_ = consoleOf("c-float", "Main");
  const project = projectOf("p-site", console_.id, "Website");
  const sessions = [sessionOf("s-site", console_.id, project.id, "Fix header", "idle"), sessionOf("s-console", console_.id, undefined, "Plan release", waiting ? "waiting_user" : "idle"), sessionOf("s-old", console_.id, project.id, "Old work", "archived")];
  const daemon: Daemon = {
    store: createStateStore({
      ...snapshotState({ consoles: [console_], projects: [project], sessions }),
      ...(agents ? { agentAvailability: new Map(ALL_AGENTS.map((agent) => [agent, { agent, availability: "available" as const, default_account_dir: `/Users/dev/.${agent}` }])) } : {}),
    }),
    request: vi.fn(async (body: RequestBody) => body.type === "list_processes" ? { type: "processes_listed", trees: [] } : { type: "ack" }) as Daemon["request"],
    toastError: vi.fn(), toastNotice: vi.fn(), onToast: () => () => {},
    dismissTrustPrompt: () => {}, reconnect: () => {}, terminalUrl: () => "",
  };
  mountForTooltip(
    <PlatformProvider value={{ kind: "browser" }}>
      <ThemeProvider><DaemonProvider value={daemon}><StackReset /><App /></DaemonProvider></ThemeProvider>
    </PlatformProvider>,
  );
  return { resize, stacks: () => stacks, daemon, project };
}

const pressElement = (element: HTMLElement | null | undefined) => {
  expect(element).toBeTruthy();
  return act(async () => element!.click());
};
const byLabel = (label: string, within: ParentNode = document) => within.querySelector<HTMLElement>(`[aria-label="${label}"]`);
const toggle = (label: string) => byLabel(label, document.querySelector("[data-region=topbar]")!);
const row = (title: string) =>
  Array.from(document.querySelectorAll<HTMLElement>("[data-region=sidebar] [role=button]")).filter((element) => element.textContent?.includes(title)).at(-1);
// A closed panel fades away out of reach (inert) before it leaves: only the ones still open count.
const cards = () => Array.from(document.querySelectorAll("[data-panel-card]")).filter((card) => !card.closest("[inert]")).map((card) => card.id);
const floatingPane = () => document.querySelector("[data-floating-panel]");
/** The scrims on screen, by what pressing one closes. */
// A scrim fading away with its overlay is out of reach (inert): only the ones still up count.
const scrims = () => Array.from(document.querySelectorAll<HTMLElement>("button.fixed.z-30:not([inert])"), (scrim) => scrim.getAttribute("aria-label"));
const terminalInert = () => document.querySelector("[data-region=terminal]")?.closest("[inert]") != null;
const regionOfFocus = () => document.activeElement?.closest("[data-region]")?.getAttribute("data-region");
const f6 = () => act(async () => void window.dispatchEvent(new KeyboardEvent("keydown", { key: "F6", bubbles: true, cancelable: true })));
const escapeFrom = (element: Element) => act(async () => void element.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })));

async function pressMenuItem(label: string) {
  const item = Array.from(document.querySelectorAll<HTMLElement>("[role=menuitem]")).find((element) => element.textContent === label);
  expect(item, label).toBeDefined();
  await act(async () => {
    if (item!.hasAttribute("aria-haspopup")) {
      item!.focus();
      item!.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
    } else {
      item!.click();
    }
  });
}

/** Selects the project's session from the drawer, which closes it: no panel floats yet. */
async function selectProjectSession() {
  await pressElement(toggle("Show sessions"));
  await pressElement(row("Fix header"));
  expect(toggle("Show sessions")).not.toBeNull();
  expect(cards()).toEqual([]);
}

it("floats the panel opened last as the window narrows, else the stack's top one, and widening brings the stack back", async () => {
  const { resize, stacks } = mountApp(1400);
  await pressElement(row("Fix header"));
  await pressElement(toggle("Show terminal panel"));
  expect(cards()).toEqual(["panel-files", "panel-terminal"]);
  await resize(390);
  expect(cards()).toEqual(["panel-terminal"]);
  expect(floatingPane()).not.toBeNull();
  await resize(1400);
  expect(cards()).toEqual(["panel-files", "panel-terminal"]);
  expect(stacks().stacks.project.open).toEqual(["files", "terminal"]);
});

it("floats the stack's top panel as the window narrows when none was opened in this window", async () => {
  const { resize, stacks } = mountApp(1400);
  act(() => stacks().setStack("project", { open: ["files", "terminal"], shares: {} }));
  await pressElement(row("Fix header"));
  await resize(390);
  expect(cards()).toEqual(["panel-files"]);
});

it("shows one panel at a time while floating, leaving the stored stack as it is", async () => {
  const { stacks } = mountApp(390);
  await selectProjectSession();
  const stored = stacks().stacks.project;
  await pressElement(toggle("Show files panel"));
  expect(cards()).toEqual(["panel-files"]);
  await pressElement(toggle("Show terminal panel"));
  expect(cards()).toEqual(["panel-terminal"]);
  expect(toggle("Show files panel")!.getAttribute("aria-expanded")).toBe("false");
  await pressElement(toggle("Hide terminal panel"));
  expect(cards()).toEqual([]);
  expect(stacks().stacks.project).toBe(stored);
});

it("closes every floating panel when the drawer opens, shows none while it is open, and closes the drawer when a panel opens", async () => {
  mountApp(390);
  await selectProjectSession();
  await pressElement(toggle("Show files panel"));
  expect(floatingPane()).not.toBeNull();
  await pressElement(toggle("Show sessions"));
  expect(cards()).toEqual([]);
  expect(scrims()).toEqual(["Close sessions"]);
  // Selecting a session closes the drawer on a phone, and shows no panel of the session's owner.
  await pressElement(row("Plan release"));
  expect(toggle("Show sessions")).not.toBeNull();
  expect(scrims()).toEqual([]);
  expect(cards()).toEqual([]);

  await pressElement(toggle("Show sessions"));
  await pressElement(toggle("Show report panel"));
  expect(toggle("Show sessions")).not.toBeNull();
  expect(cards()).toEqual(["panel-report"]);
  expect(scrims()).toEqual(["Close panel"]);
});

it("floats no panel when the window narrows while the drawer is open", async () => {
  const { resize } = mountApp(900);
  await pressElement(toggle("Show sessions"));
  await pressElement(row("Fix header"));
  // Wide enough to dock the pane, the drawer stays open beside the terminal as a session is picked.
  expect(toggle("Hide sessions")).not.toBeNull();
  expect(cards()).toEqual(["panel-files"]);
  await resize(390);
  expect(cards()).toEqual([]);
  expect(scrims()).toEqual(["Close sessions"]);
  await pressElement(toggle("Hide sessions"));
  expect(cards()).toEqual([]);
});

it("closes the floating panel when Forward arrives at the archive view under it", async () => {
  mountApp(390);
  await selectProjectSession();
  await pressElement(toggle("Show sessions"));
  await pressElement(byLabel(`Actions for project ${isolate("Website")}`));
  await pressMenuItem("View archive");
  await pressMenuItem("View all");
  expect(document.querySelector("[data-region=archive]")).not.toBeNull();
  await pressElement(toggle("Back"));
  await pressElement(toggle("Show files panel"));
  expect(cards()).toEqual(["panel-files"]);
  await pressElement(toggle("Forward"));
  expect(document.querySelector("[data-region=archive]")).not.toBeNull();
  expect(cards()).toEqual([]);
});

it("covers the agent terminal while a panel floats: inert, skipped by F6, focus going into the panel and back on close", async () => {
  mountApp(390);
  await selectProjectSession();
  await pressElement(toggle("Show terminal panel"));
  expect(terminalInert()).toBe(true);
  expect(regionOfFocus()).toBe("panel-terminal");
  expect(document.activeElement?.getAttribute("aria-label")).toBe("shell input");
  const visited = new Set<string | null | undefined>();
  for (let step = 0; step < 6; step++) {
    await f6();
    visited.add(regionOfFocus());
  }
  expect(visited.has("panel-terminal")).toBe(true);
  expect(visited.has("terminal")).toBe(false);

  await act(async () => document.querySelector<HTMLElement>("#panel-terminal [data-region-landing]")!.focus());
  terminalFocus.mockClear();
  await pressElement(byLabel("Close panel", document.getElementById("panel-terminal")!));
  expect(cards()).toEqual([]);
  expect(terminalInert()).toBe(false);
  expect(terminalFocus).toHaveBeenCalled();
});

it("closes the floating panel on a press beside it or on Escape, but leaves Escape to its shell", async () => {
  mountApp(390);
  await selectProjectSession();
  await pressElement(toggle("Show terminal panel"));
  const shell = document.querySelector<HTMLElement>("#panel-terminal [data-region-landing]")!;
  await escapeFrom(shell);
  expect(cards()).toEqual(["panel-terminal"]);
  await escapeFrom(document.querySelector("#panel-terminal [aria-label='Close panel']")!);
  expect(cards()).toEqual([]);

  await pressElement(toggle("Show files panel"));
  expect(regionOfFocus()).toBe("panel-files");
  terminalFocus.mockClear();
  const scrim = Array.from(document.querySelectorAll<HTMLElement>("button.fixed.z-30:not([inert])")).find((button) => button.getAttribute("aria-label") === "Close panel");
  await pressElement(scrim);
  expect(cards()).toEqual([]);
  expect(terminalFocus).toHaveBeenCalled();
});

it("floats no panel in from a toggle the pointer rests on while panels float over the terminal: a toggle opens one only on a press", async () => {
  mountApp(390);
  await selectProjectSession();
  vi.useFakeTimers();
  onTestFinished(() => {
    vi.useRealTimers();
  });
  const button = toggle("Show files panel")!;
  const pointer = (type: string) => new PointerEvent(type, { pointerType: "mouse", bubbles: type !== "pointerenter" });
  await act(async () => {
    button.dispatchEvent(pointer("pointerover"));
    button.dispatchEvent(pointer("pointerenter"));
    await vi.advanceTimersByTimeAsync(400);
  });
  expect(document.querySelector("[data-panel-card]")).toBeNull();
  expect(document.querySelector("[data-pane=aside]")).toBeNull();
  vi.useRealTimers();
  await pressElement(toggle("Show files panel"));
  expect(cards()).toEqual(["panel-files"]);
});

it("moves focus into no field that takes text on a touch screen: a panel opens onto a control, and closing one or picking a session leaves the terminal be", async () => {
  mountApp(390, true);
  terminalFocus.mockClear();
  await pressElement(toggle("Show sessions"));
  await pressElement(row("Fix header"));
  expect(toggle("Show sessions")).not.toBeNull();
  await pressElement(toggle("Show terminal panel"));
  expect(regionOfFocus()).toBe("panel-terminal");
  expect(document.activeElement?.matches("input, textarea")).toBe(false);
  await act(async () => document.querySelector<HTMLElement>("#panel-terminal button")!.focus());
  await pressElement(byLabel("Close panel", document.getElementById("panel-terminal")!));
  expect(cards()).toEqual([]);
  expect(terminalFocus).not.toHaveBeenCalled();
  // Focus that was in the panel is not left to fall to <body>: the top bar takes it.
  expect(regionOfFocus()).toBe("topbar");
});

it("closes the drawer as a session is picked in it on a phone, handing focus to the terminal", async () => {
  mountApp(390);
  await pressElement(toggle("Show sessions"));
  await act(async () => row("Fix header")!.focus());
  terminalFocus.mockClear();
  await pressElement(row("Fix header"));
  expect(toggle("Show sessions")).not.toBeNull();
  expect(scrims()).toEqual([]);
  expect(terminalFocus).toHaveBeenCalled();
});

it("hands focus back, as a dialog opened from the open drawer closes, to the control that opened it there", async () => {
  mountApp(390, false, true);
  await pressElement(toggle("Show sessions"));
  const plus = byLabel("New console session", document.querySelector("[data-region=sidebar]")!)!;
  // A tap presses the control without focusing it.
  await act(async () => {
    plus.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, pointerType: "touch" }));
    plus.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, pointerType: "touch" }));
    plus.click();
  });
  const dialog = document.querySelector("[role=dialog]");
  expect(dialog).not.toBeNull();
  await act(async () => void document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })));
  await act(async () => new Promise((resolve) => setTimeout(resolve, 50)));
  expect(document.querySelector("[role=dialog]")).toBeNull();
  expect(document.activeElement).toBe(plus);
});

it.each([false, true])("lands focus that Back leaves nowhere on the top bar when no session takes it (touch: %s)", async (touch) => {
  mountApp(390, touch);
  await selectProjectSession();
  await act(async () => void (document.activeElement as HTMLElement | null)?.blur());
  await pressElement(toggle("Back"));
  expect(regionOfFocus()).toBe("topbar");
  expect(document.activeElement?.matches(":disabled")).toBe(false);
});

it("records no drawer control for a dialog from a pointer that went down only to scroll the drawer", async () => {
  mountApp(390, false, true);
  await pressElement(toggle("Show sessions"));
  const sessionRow = row("Fix header")!;
  await act(async () => {
    sessionRow.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, pointerType: "touch" }));
    sessionRow.dispatchEvent(new PointerEvent("pointercancel", { bubbles: true, pointerType: "touch" }));
  });
  // The dialog opens from a press that reports no pointer of its own.
  await pressElement(byLabel("New console session", document.querySelector("[data-region=sidebar]")!));
  expect(document.querySelector("[role=dialog]")).not.toBeNull();
  await act(async () => void document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })));
  await act(async () => new Promise((resolve) => setTimeout(resolve, 50)));
  expect(document.activeElement).not.toBe(sessionRow);
});

const rail = () => document.querySelector<HTMLElement>("[data-region=rail]")!;

it("keeps the rail out of reach on a phone until the drawer opens, and takes it away with the drawer", async () => {
  mountApp(390);
  await selectProjectSession();
  expect(rail().hasAttribute("inert")).toBe(true);
  const visited = new Set<string | null | undefined>();
  for (let step = 0; step < 6; step++) {
    await f6();
    visited.add(regionOfFocus());
  }
  expect(visited.has("rail")).toBe(false);

  await pressElement(toggle("Show sessions"));
  expect(rail().hasAttribute("inert")).toBe(false);
  await f6();
  await f6();
  const reached = new Set<string | null | undefined>();
  for (let step = 0; step < 6; step++) {
    await f6();
    reached.add(regionOfFocus());
  }
  expect(reached.has("rail")).toBe(true);

  await pressElement(toggle("Hide sessions"));
  expect(rail().hasAttribute("inert")).toBe(true);
});

it("hands focus on when Escape closes the drawer from a rail control on a phone", async () => {
  mountApp(390);
  await pressElement(toggle("Show sessions"));
  const settings = byLabel("Settings", rail())!;
  await act(async () => settings.focus());
  await escapeFrom(settings);
  expect(toggle("Show sessions")).not.toBeNull();
  expect(rail().hasAttribute("inert")).toBe(true);
  expect(rail().contains(document.activeElement)).toBe(false);
});

it("leaves the rail in the row from the phone breakpoint up", async () => {
  const { resize } = mountApp(390);
  expect(rail().hasAttribute("inert")).toBe(true);
  await resize(800);
  expect(rail().hasAttribute("inert")).toBe(false);
  await resize(390);
  expect(rail().hasAttribute("inert")).toBe(true);
});

it("lets the scrims reach the window's edge on a phone, the rail being away or part of the drawer's card", async () => {
  mountApp(390);
  await selectProjectSession();
  await pressElement(toggle("Show files panel"));
  const scrim = document.querySelector<HTMLElement>("button.fixed.z-30:not([inert])")!;
  expect(scrim.getAttribute("aria-label")).toBe("Close panel");
  expect(scrim.classList.contains("start-0")).toBe(true);
  await pressElement(toggle("Hide files panel"));
  await pressElement(toggle("Show sessions"));
  expect(document.querySelector("button.fixed.z-30:not([inert])")!.className).toContain("start-(--rail-width) max-phone:start-0");
});

/** A finger swiping by `dx` px sideways from the middle of the agent terminal. */
async function swipe(dx: number) {
  const send = (type: string, x: number) =>
    act(async () => {
      const touch = { identifier: 1, clientX: x, clientY: 300 };
      document.body.dispatchEvent(touchEvent(type, type === "touchend" ? [] : [touch], [touch]));
    });
  await send("touchstart", 200);
  await send("touchmove", 200 + dx / 2);
  await send("touchmove", 200 + dx);
  await send("touchend", 200 + dx);
}

it("opens the drawer on a swipe towards the end and closes it on one towards the start, on a phone", async () => {
  mountApp(390, true);
  await swipe(120);
  expect(toggle("Hide sessions")).not.toBeNull();
  await swipe(120);
  expect(toggle("Hide sessions")).not.toBeNull();
  await swipe(-120);
  expect(toggle("Show sessions")).not.toBeNull();
});

it("brings the panel the owner had open last in on a swipe towards the start, else its first, then cycles through its panels, wrapping", async () => {
  const { daemon, project } = mountApp(390, true);
  await act(async () => daemon.store.setState({ shellTerminals: new Map([["shell", shellTerminalOf("shell", project, "Shell", 0)]]) }));
  await selectProjectSession();
  await swipe(-120);
  expect(cards()).toEqual(["panel-files"]);
  await swipe(-120);
  expect(cards()).toEqual(["panel-terminal"]);
  await swipe(-120);
  expect(cards()).toEqual(["panel-files"]);
  await swipe(-120);
  expect(cards()).toEqual(["panel-terminal"]);
  await swipe(120);
  expect(cards()).toEqual([]);
  await swipe(-120);
  expect(cards()).toEqual(["panel-terminal"]);
});

it("closes the floating panel before opening the drawer, so the two are never on screen together", async () => {
  mountApp(390, true);
  await selectProjectSession();
  await swipe(-120);
  expect(cards()).toEqual(["panel-files"]);
  await swipe(120);
  expect(cards()).toEqual([]);
  expect(toggle("Show sessions")).not.toBeNull();
  await swipe(120);
  expect(toggle("Hide sessions")).not.toBeNull();
  expect(cards()).toEqual([]);
});

it("opens no panel on a swipe towards the start when the pane has no owner", async () => {
  mountApp(390, true);
  await swipe(-120);
  expect(cards()).toEqual([]);
  expect(toggle("Show sessions")).not.toBeNull();
});

it("ignores swipes from the phone breakpoint up", async () => {
  const { resize } = mountApp(800, true);
  await swipe(120);
  expect(toggle("Show sessions")).not.toBeNull();
  await resize(390);
  await swipe(120);
  expect(toggle("Hide sessions")).not.toBeNull();
});

/** Focus put on `element` the way a key leaves it, which is what opens its tooltip. */
async function focusByKey(element: HTMLElement) {
  await act(async () => {
    element.blur();
    element.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    element.focus();
  });
  await act(async () => new Promise((resolve) => setTimeout(resolve, 50)));
}

it("shows the scrim for what a swipe opens, leaving focus on the top bar's toggle", async () => {
  mountApp(390);
  await selectProjectSession();
  const sidebarToggle = toggle("Show sessions")!;
  await focusByKey(sidebarToggle);
  await swipe(120);
  expect(scrims()).toEqual(["Close sessions"]);
  expect(document.activeElement).toBe(sidebarToggle);
});

it("lands focus in the panel a swipe opens from a top bar control with its tooltip shown", async () => {
  mountApp(390);
  await selectProjectSession();
  await focusByKey(toggle("Back")!);
  await swipe(-120);
  expect(cards()).toEqual(["panel-files"]);
  expect(document.activeElement).not.toBe(document.body);
  expect(document.activeElement?.closest("[data-floating-panel]")).not.toBeNull();
});

it("closes the drawer on a swipe towards the start from a top bar control, dismissing nothing else", async () => {
  mountApp(390);
  await selectProjectSession();
  await pressElement(toggle("Show sessions"));
  const hideToggle = toggle("Hide sessions")!;
  await focusByKey(hideToggle);
  await swipe(-120);
  expect(toggle("Show sessions")).not.toBeNull();
  expect(cards()).toEqual([]);
  expect(document.activeElement).toBe(hideToggle);
});

it.each(["Settings", "New console"])("hands focus on when a swipe closes the drawer from the rail's %s with its tooltip shown", async (label) => {
  mountApp(390);
  await pressElement(toggle("Show sessions"));
  const control = byLabel(label, rail())!;
  await focusByKey(control);
  expect(openTooltip()).not.toBeNull();
  await swipe(-120);
  expect(toggle("Show sessions")).not.toBeNull();
  expect(document.activeElement).not.toBe(document.body);
  expect(rail().contains(document.activeElement)).toBe(false);
});

it("hands focus on when a swipe closes the drawer from a row of the sidebar with a tooltip shown", async () => {
  mountApp(390);
  await pressElement(toggle("Show sessions"));
  await focusByKey(row("Plan release")!);
  await swipe(-120);
  expect(toggle("Show sessions")).not.toBeNull();
  expect(document.activeElement).not.toBe(document.body);
  expect(document.querySelector("[data-region=sidebar]")!.contains(document.activeElement)).toBe(false);
});

it("leaves focus in the terminal's text field alone when a swipe opens or closes a pane", async () => {
  mountApp(390, true);
  await selectProjectSession();
  const input = document.body.appendChild(document.createElement("textarea"));
  onTestFinished(() => void input.remove());
  await act(async () => input.focus());
  await swipe(120);
  expect(toggle("Hide sessions")).not.toBeNull();
  expect(document.activeElement).toBe(input);
  await swipe(-120);
  expect(toggle("Show sessions")).not.toBeNull();
  expect(document.activeElement).toBe(input);
});

it("closes the drawer when the rail's waiting count picks the next session in it", async () => {
  mountApp(390, true, false, true);
  await pressElement(toggle("Show sessions"));
  await pressElement(byLabel("1 session is waiting for you. Go to the next one.", rail()));
  expect(toggle("Show sessions")).not.toBeNull();
  expect(scrims()).toEqual([]);
});

it("hands focus back to the rail's Settings as Settings opened from it in the drawer closes", async () => {
  mountApp(390, true);
  await pressElement(toggle("Show sessions"));
  const settings = byLabel("Settings", rail())!;
  // A tap presses the control without focusing it.
  await act(async () => {
    settings.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, pointerType: "touch" }));
    settings.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, pointerType: "touch" }));
    settings.click();
  });
  expect(document.querySelector("[role=dialog]")).not.toBeNull();
  await act(async () => void document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })));
  await act(async () => new Promise((resolve) => setTimeout(resolve, 50)));
  expect(document.querySelector("[role=dialog]")).toBeNull();
  expect(document.activeElement).toBe(settings);
  expect(toggle("Hide sessions")).not.toBeNull();
});

it("hands focus back to the rail's Settings as Settings opened from it by a key in the drawer closes", async () => {
  mountApp(390);
  await pressElement(toggle("Show sessions"));
  const settings = byLabel("Settings", rail())!;
  await focusByKey(settings);
  await act(async () => {
    settings.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    settings.dispatchEvent(new KeyboardEvent("keyup", { key: "Enter", bubbles: true }));
  });
  expect(document.querySelector("[role=dialog]")).not.toBeNull();
  await act(async () => void document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })));
  await act(async () => new Promise((resolve) => setTimeout(resolve, 50)));
  expect(document.querySelector("[role=dialog]")).toBeNull();
  expect(document.activeElement).toBe(settings);
  expect(toggle("Hide sessions")).not.toBeNull();
});

it("skips empty process and terminal panels on swipes while their topbar toggles still open them", async () => {
  const { daemon } = mountApp(390, true);
  await selectProjectSession();
  await swipe(-120);
  expect(cards()).toEqual(["panel-files"]);
  await swipe(-120);
  expect(cards()).toEqual(["panel-files"]);
  expect(daemon.request).toHaveBeenCalledWith({ type: "list_processes", project_id: "p-site" });
  await pressElement(toggle("Show processes panel"));
  expect(cards()).toEqual(["panel-processes"]);
  expect(document.getElementById("panel-processes")?.textContent).toContain("No processes");
  await swipe(-120);
  expect(cards()).toEqual(["panel-files"]);
  await pressElement(toggle("Show terminal panel"));
  expect(cards()).toEqual(["panel-terminal"]);
  await swipe(-120);
  expect(cards()).toEqual(["panel-files"]);
});

it("keeps a process panel reachable by swipe while its list is loading or failed", async () => {
  const { daemon } = mountApp(390, true);
  vi.mocked(daemon.request).mockImplementation(async (body) => body.type === "list_processes" ? new Promise<never>(() => {}) : { type: "ack" });
  await selectProjectSession();
  await swipe(-120);
  await swipe(-120);
  expect(cards()).toEqual(["panel-processes"]);
  expect(document.getElementById("panel-processes")?.textContent).toContain("Loading processes");
});

it("updates swipe availability with the selected session's filtered processes", async () => {
  const { daemon, project } = mountApp(390, true);
  const other = sessionOf("s-other", project.console_id, project.id, "Other work", "idle");
  await act(async () => daemon.store.setState((state) => ({ sessions: new Map([...state.sessions, [other.id, other]]) })));
  vi.mocked(daemon.request).mockImplementation(async (body) => body.type === "list_processes" ? {
    type: "processes_listed",
    trees: [{ id: "1", session_id: "s-site", command: "vite", cwd: "/app", cwd_missing: false, kept: false, started_at: Date.now(), process_count: 1, memory_bytes: 0, ports: [] }],
  } : { type: "ack" });
  await selectProjectSession();
  await swipe(-120);
  expect(cards()).toEqual(["panel-files"]);
  await swipe(-120);
  expect(cards()).toEqual(["panel-processes"]);
  await pressElement(toggle("Show sessions"));
  await pressElement(row("Other work"));
  await swipe(-120);
  expect(cards()).toEqual(["panel-files"]);
  await pressElement(toggle("Show processes panel"));
  expect(document.getElementById("panel-processes")?.textContent).toContain("No processes");
});

it("shows all project processes when opened from a project with no session selected", async () => {
  const { daemon } = mountApp(1400);
  vi.mocked(daemon.request).mockImplementation(async (body) => body.type === "list_processes" ? {
    type: "processes_listed",
    trees: [{ id: "all-tree", session_id: "s-site", command: "vite all sessions", cwd: "/app", cwd_missing: false, kept: false, started_at: Date.now(), process_count: 1, memory_bytes: 0, ports: [] }],
  } : { type: "ack" });
  await pressElement(byLabel(`Actions for project ${isolate("Website")}`));
  await pressMenuItem("Open panel");
  await pressMenuItem("Files");
  await pressElement(toggle("Hide files panel"));
  await pressElement(toggle("Show processes panel"));
  const panel = document.getElementById("panel-processes")!;
  expect(panel.querySelector("h2")?.textContent).toBe("Processes");
  expect(panel.textContent).toContain("All sessions");
  expect(panel.textContent).toContain("vite all sessions");
});
