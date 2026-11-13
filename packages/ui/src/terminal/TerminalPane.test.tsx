// @vitest-environment jsdom
import React, { act, createRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { afterEach, beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";

import { AFTER_FOCUS_MOVE_DELAY_MS, FIRST_WRITE_DELAY_MS, READ_TIME_MS } from "../components/announce/StatusAnnouncer";
import { errorAlerts } from "../components/alerts/errorAlerts";
import { windowAttention } from "../components/announce/windowTestSupport";
import { withModality } from "../components/dialog/dialogTestSupport";
import { startResizeDrag } from "../components/panel/resizeDrag";
import type { PlatformAdapter } from "../platform";
import type { Event, ClientRequestBody } from "../protocol";
import { setAccountSwitchesQueued, type Daemon, type State } from "../store";
import { blurOnRemoval } from "../test/blurOnRemoval";
import { TestHost } from "../test/TestHost";
import { testDaemon } from "../test/testDaemon";
import { sessionOf } from "../fixtures/builders";
import { CATALOGS, format } from "../i18n/catalog";
import { currentLanguage, setLanguage } from "../i18n/language";
import type { Language } from "../i18n/languages";
import { MAX_AUTO_RECONNECT_ATTEMPTS, reconnectDelay } from "./reconnectPolicy";
import { PAST_ANY_RETRY_MS } from "./reconnectTestSupport";
import { resetTerminalFontSize, terminalFontSize } from "./terminalFontSize";
import type { TermStatus } from "./TerminalController";
import { LOADING_LIMIT_MS, TerminalPane, type TerminalPaneHandle } from "./TerminalPane";
import { BROWSER_HOST } from "../test/hosts";
import { fakeTimers } from "../test/clock";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** A controller whose socket the test settles by hand: `attach` leaves it `connecting`. */
const { FakeController } = vi.hoisted(() => {
  class FakeController {
    static last: FakeController;
    currentSessionId: string | undefined;
    currentStatus: TermStatus = "closed";
    constructor(private callbacks: {
      onStatusChange: (status: TermStatus) => void;
      onPaintedChange?: (painted: boolean) => void;
      openLink?: (url: string) => Promise<void>;
      pathLinks?: { open: (target: { path: string; line?: number; view?: { worktree?: string; path: string } }) => Promise<void> };
    }) {
      FakeController.last = this;
    }
    openPath(target: { path: string; line?: number; view?: { worktree?: string; path: string } }): Promise<void> | undefined {
      return this.callbacks.pathLinks?.open(target);
    }
    openLink(url: string): Promise<void> | undefined { return this.callbacks.openLink?.(url); }
    mount(): void {}
    inputLabel: string | undefined;
    setInputLabel(label: string): void {
      this.inputLabel = label;
    }
    fontSize: number | undefined;
    setFontSize(size: number): void {
      this.fontSize = size;
    }
    fits = 0;
    fit(): void {
      this.fits += 1;
    }
    syncSize(): void {}
    dispose(): void {}
    focuses = 0;
    focus(): void {
      this.focuses += 1;
    }
    attaches = 0;
    /** Whether the last attach was the user's, which takes focus. */
    lastUserInitiated: boolean | undefined;
    held = false;
    holdInput(held: boolean): void {
      this.held = held;
    }
    paint(): void {
      act(() => this.callbacks.onPaintedChange?.(true));
    }
    attach(sessionId: string, _url?: string, userInitiated?: boolean): void {
      this.attaches += 1;
      this.lastUserInitiated = userInitiated;
      this.callbacks.onPaintedChange?.(false);
      this.currentSessionId = sessionId;
      this.currentStatus = "connecting";
      this.callbacks.onStatusChange("connecting");
    }
    detaches: unknown[] = [];
    detach(options: unknown): void {
      this.detaches.push(options);
    }
    settle(status: TermStatus): void {
      this.currentStatus = status;
      act(() => this.callbacks.onStatusChange(status));
    }
  }
  return { FakeController };
});
vi.mock("./TerminalController", () => ({ TerminalController: FakeController }));
const request = vi.fn(async (body: ClientRequestBody): Promise<Event> => (body.type === "read_session_history" ? { type: "session_history", id: null, session: "", messages: [] } : { type: "ack", id: null }));
vi.mock("../preferences/theme", () => ({ useOctoboardTheme: () => ({ resolved: "light" }) }));

// One daemon for every render of a test: the pane re-attaches when `terminalUrl` changes identity.
let daemon: Daemon;
const platform: { -readonly [K in keyof PlatformAdapter]: PlatformAdapter[K] } = { kind: "browser", host: BROWSER_HOST };
const host = (ui: React.ReactNode) => <TestHost daemon={daemon} platform={platform}>{ui}</TestHost>;
/** Updates the daemon's state the way a broadcast would. */
const setState = (state: Partial<State>) => act(() => daemon.store.setState(state));

const roots: Root[] = [];
/** Puts the window in or out of the user's attention; every test starts attended, as the announcements being
 * tested are only delivered to a user who is in the window (jsdom would answer from what was focused earlier). */
let attention: ReturnType<typeof windowAttention>;
beforeEach(() => {
  attention = windowAttention();
  fakeTimers();
  platform.links = undefined;
  request.mockClear();
  daemon = testDaemon({ state: { connectionState: "open" }, request, terminalUrl: () => "" });
});
afterEach(() => {
  for (const root of roots.splice(0)) act(() => root.unmount());
  document.body.replaceChildren();
});

const session = sessionOf("s-1", "c-1", undefined, "Work", "working");

function render(root: Root, current = session, exposed = true) {
  act(() => root.render(host(<TerminalPane exposed={exposed} session={current} onResume={async () => true} onAddAgent={() => {}} />)));
}

function mount() {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  render(root);
  return { container, root };
}

/** Fails the attempt in flight, then lets the next one start after the longest backoff delay. */
function failAttempt(): void {
  FakeController.last.settle("closed");
  act(() => void vi.advanceTimersByTime(reconnectDelay(MAX_AUTO_RECONNECT_ATTEMPTS)));
}

/** Lets the status regions pass their first write delay. */
const speak = () => act(() => void vi.advanceTimersByTime(FIRST_WRITE_DELAY_MS));
const advance = (ms: number) => act(() => void vi.advanceTimersByTime(ms));
/** The terminal's own isolated layer, which a cover makes inert; announcers that must stay heard sit outside it. */
const isolatedTerminal = (container: HTMLElement) => container.querySelector("[data-region=terminal] > .isolate")!;
/** The cover's spoken text: its region is the first status region the pane renders. */
const status = (container: HTMLElement) => container.querySelector("[role=status]:not([data-slot=spinner])")?.textContent;

it("keeps the loading cover up past the limit while the first connect hangs", () => {
  const { container } = mount();
  act(() => void vi.advanceTimersByTime(LOADING_LIMIT_MS + 1));
  expect(status(container)).toBe("Loading terminal…");
});

it("keeps the cover up past the loading limit while the connection is failing", () => {
  const { container } = mount();
  speak();
  expect(status(container)).toBe("Loading terminal…");
  failAttempt();
  expect(status(container)).toBe("Reconnecting the terminal…");
  act(() => void vi.advanceTimersByTime(LOADING_LIMIT_MS + 1));
  expect(status(container)).toBe("Reconnecting the terminal…");
  expect(container.querySelector("[data-slot=spinner]")).not.toBeNull();
});

it("still lets a silent, connected terminal go bare after the loading limit", () => {
  const { container } = mount();
  FakeController.last.settle("open");
  act(() => void vi.advanceTimersByTime(LOADING_LIMIT_MS + 1));
  expect(status(container)).toBe("");
});

it("covers a session again when it is shown once more while it reconnects, though its first wait ran out", () => {
  const other = sessionOf("s-2", "c-1", undefined, "Other", "working");
  const { container, root } = mount();
  FakeController.last.settle("open");
  act(() => void vi.advanceTimersByTime(LOADING_LIMIT_MS + 1));
  expect(status(container)).toBe("");
  // Another session is shown, then the first again, whose terminal is not painted yet.
  render(root, other);
  render(root, session);
  expect(status(container)).toBe("Loading terminal…");
  // Its wait begins afresh: the earlier expiry does not uncover it.
  FakeController.last.settle("open");
  act(() => void vi.advanceTimersByTime(LOADING_LIMIT_MS - 1));
  expect(status(container)).toBe("Loading terminal…");
  act(() => void vi.advanceTimersByTime(2));
  expect(status(container)).toBe("");
});

it("does not let the wait of a session left behind uncover the one shown next", () => {
  const other = sessionOf("s-2", "c-1", undefined, "Other", "working");
  const { container, root } = mount();
  FakeController.last.settle("open");
  act(() => void vi.advanceTimersByTime(LOADING_LIMIT_MS - 1));
  render(root, other);
  FakeController.last.settle("open");
  // The first session's limit falls due now, while the second is shown and has waited only a moment.
  act(() => void vi.advanceTimersByTime(2));
  expect(status(container)).toBe("Loading terminal…");
});

it("shows a static disconnected cover with a Reconnect button once the attempts are spent", () => {
  const { container } = mount();
  for (let i = 0; i < 5; i++) failAttempt();
  FakeController.last.settle("closed");
  expect(status(container)).toBe("Terminal disconnected");
  expect(container.querySelector("[data-slot=spinner]")).toBeNull();
  expect(container.querySelector<HTMLButtonElement>("button:not([hidden])")?.textContent).toBe("Reconnect");
});

it("reconnects, with the attempts reset, when Reconnect is pressed", () => {
  const { container } = mount();
  for (let i = 0; i < 5; i++) failAttempt();
  FakeController.last.settle("closed");
  const attaches = FakeController.last.attaches;
  act(() => container.querySelector<HTMLButtonElement>("button:not([hidden])")?.click());
  expect(FakeController.last.attaches).toBe(attaches + 1);
  expect(container.querySelector<HTMLButtonElement>("button:not([hidden])")).toBeNull();
  expect(container.querySelector("[data-slot=spinner]")).not.toBeNull();
});

it("offers no Reconnect button while the daemon connection is not open", () => {
  setState({ connectionState: "closed" });
  const { container } = mount();
  for (let i = 0; i < 5; i++) failAttempt();
  FakeController.last.settle("closed");
  expect(status(container)).toBe("Terminal disconnected");
  expect(container.querySelector<HTMLButtonElement>("button:not([hidden])")).toBeNull();
});

it("focuses the Reconnect button, not the terminal, through the handle while it is offered", () => {
  const handle = createRef<TerminalPaneHandle>();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  act(() => root.render(host(<TerminalPane ref={handle} session={session} onResume={async () => true} onAddAgent={() => {}} />)));
  act(() => handle.current?.focus());
  expect(FakeController.last.focuses).toBe(1);
  for (let i = 0; i < 5; i++) failAttempt();
  FakeController.last.settle("closed");
  act(() => handle.current?.focus());
  expect(FakeController.last.focuses).toBe(1);
  expect(document.activeElement).toBe(container.querySelector<HTMLButtonElement>("button:not([hidden])"));
});

it("does not attach again for a broadcast about a session whose attempts are spent", () => {
  const { root } = mount();
  for (let i = 0; i < 5; i++) failAttempt();
  FakeController.last.settle("closed");
  const attaches = FakeController.last.attaches;
  render(root, { ...session, title: "Renamed" });
  expect(FakeController.last.attaches).toBe(attaches);
});

it("recovers a terminal that gave up when a fresh snapshot arrives", () => {
  const { root } = mount();
  for (let i = 0; i < 5; i++) failAttempt();
  FakeController.last.settle("closed");
  const attaches = FakeController.last.attaches;
  setState({ snapshotEpoch: 1 });
  render(root);
  expect(FakeController.last.attaches).toBe(attaches + 1);
});

it("redraws a running terminal when the font size changes, and at the chosen size when it opens", () => {
  onTestFinished(resetTerminalFontSize);
  mount();
  expect(FakeController.last.fontSize).toBe(15);
  act(() => terminalFontSize.set(19));
  expect(FakeController.last.fontSize).toBe(19);
  mount();
  expect(FakeController.last.fontSize).toBe(19);
});

it("holds its size while a docked pane eases, then fits once", () => {
  const observers: (() => void)[] = [];
  vi.stubGlobal("ResizeObserver", class {
    constructor(callback: () => void) {
      observers.push(callback);
    }
    observe(): void {}
    disconnect(): void {}
  });
  onTestFinished(() => {
    vi.unstubAllGlobals();
  });
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  const show = (resizing: boolean) => act(() => root.render(host(<TerminalPane session={session} resizing={resizing} onResume={async () => true} onAddAgent={() => {}} />)));
  show(true);
  const fits = FakeController.last.fits;
  observers.forEach((notify) => notify());
  expect(FakeController.last.fits).toBe(fits);
  show(false);
  expect(FakeController.last.fits).toBe(fits + 1);
  observers.forEach((notify) => notify());
  expect(FakeController.last.fits).toBe(fits + 2);
});

it("holds the terminal grid through captured drags and fits only changed containers after the last release", () => {
  const observers: (() => void)[] = [];
  vi.stubGlobal("ResizeObserver", class {
    constructor(callback: () => void) { observers.push(callback); }
    observe(): void {}
    disconnect(): void {}
  });
  onTestFinished(() => { vi.unstubAllGlobals(); });
  mount();
  const controller = FakeController.last;
  const fits = controller.fits;
  const attaches = controller.attaches;
  let finish!: () => void;
  let finishOther!: () => void;
  act(() => { finish = startResizeDrag(); });
  // A floating sidebar changes no terminal dimensions, so it needs no catch-up fit.
  act(() => finish());
  expect(controller.fits).toBe(fits);
  act(() => { finish = startResizeDrag(); finishOther = startResizeDrag(); });
  for (let step = 0; step < 35; step++) observers.forEach((notify) => notify());
  expect(controller.fits).toBe(fits);
  act(() => finish());
  expect(controller.fits).toBe(fits);
  act(() => finishOther());
  expect(controller.fits).toBe(fits + 1);
  act(() => { finish(); finishOther(); });
  expect(controller.fits).toBe(fits + 1);
  expect(controller.attaches).toBe(attaches);
});

it("names the terminal's input in the UI language, and follows a language change", () => {
  const language = currentLanguage();
  try {
    act(() => setLanguage("en"));
    mount();
    expect(FakeController.last.inputLabel).toBe("Terminal input");
    act(() => setLanguage("zh-Hans"));
    expect(FakeController.last.inputLabel).toBe("终端输入");
  } finally {
    act(() => setLanguage(language));
  }
});

/** An element standing for xterm's textarea, inside the terminal's mount element. */
function terminalInput(container: HTMLElement): HTMLTextAreaElement {
  const input = container.querySelector("[data-region=terminal] [dir=ltr]")!.appendChild(document.createElement("textarea"));
  input.className = "xterm-helper-textarea";
  return input;
}

/** A top bar region with one control, where focus falls back to; returns the control. */
function addTopBar(): HTMLButtonElement {
  const topBar = document.body.appendChild(document.createElement("div"));
  topBar.dataset.region = "topbar";
  return topBar.appendChild(document.createElement("button"));
}

/** The empty state's action offered by `noAgents: "add"`. */
const emptyAction = (container: HTMLElement) => Array.from(container.querySelectorAll("button")).find((b) => b.textContent === "Add agent");

/** Whether the element holding focus shows the focus ring: `[data-focus-visible]` anywhere in the pane would also
 * match the terminal's own surface, which draws the ring around its input. */
const focusedElementRinged = () => document.activeElement?.hasAttribute("data-focus-visible") ?? false;

/** A pane mounted beside the top bar focus falls back to and a control elsewhere to put focus on:
 * the three cases below all turn on where focus goes while the terminal is empty. `show` renders
 * again with the props it is given, the mount's own where it is given none. */
function mountEmpty(props: Partial<React.ComponentProps<typeof TerminalPane>> = {}) {
  const topBarButton = addTopBar();
  const elsewhere = document.body.appendChild(document.createElement("button"));
  const handle = createRef<TerminalPaneHandle>();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  const show = (next: Partial<React.ComponentProps<typeof TerminalPane>> = props) =>
    act(() => root.render(host(<TerminalPane ref={handle} onResume={async () => true} onAddAgent={() => {}} {...next} />)));
  show();
  return { container, handle, topBarButton, elsewhere, show };
}

it("keeps focus off the covered terminal while no session is selected", () => {
  const { container, handle, topBarButton, elsewhere, show } = mountEmpty();
  expect(container.querySelector("[inert]")?.contains(container.querySelector("[dir=ltr]"))).toBe(true);

  // Focus asked for goes to the top bar from anywhere but `<body>`, which is left alone so that the
  // refocus at mount does not pull focus at launch.
  act(() => handle.current?.focus());
  expect(document.activeElement).toBe(document.body);
  elsewhere.focus();
  act(() => handle.current?.focus());
  expect(document.activeElement).toBe(topBarButton);
  expect(FakeController.last.focuses).toBe(0);

  show({ session });
  expect(container.querySelector("[inert]")).toBeNull();
  act(() => handle.current?.focus());
  expect(FakeController.last.focuses).toBe(1);
});

describe("handing focus on through the handle, as the closing archive view does", () => {
  it.each([
    ["offers an action", { noAgents: "add" as const }, true],
    ["offers none", {}, false],
  ])("lands focus left on <body> with no session on the empty state's action or else the top bar when the empty state %s", (_case, props, action) => {
    const { container, handle, topBarButton } = mountEmpty(props);
    expect(document.activeElement).toBe(document.body);
    // Focus asked for the terminal leaves <body> alone, so that the app's launch pulls it nowhere.
    act(() => handle.current?.focus());
    expect(document.activeElement).toBe(document.body);
    act(() => handle.current?.handOff());
    expect(document.activeElement).toBe(action ? emptyAction(container) : topBarButton);
  });

  it("lands focus left on <body> with a session on the window's restore", () => {
    const restoreFocus = vi.fn();
    const { handle } = mountEmpty({ session, restoreFocus });
    act(() => handle.current?.handOff());
    expect(restoreFocus).toHaveBeenCalledOnce();
  });
});

it("offers Add project in place of the select-a-session line while the console is bare", () => {
  const onAddProject = vi.fn();
  const { container, handle, topBarButton, elsewhere, show } = mountEmpty({ onAddProject });
  const button = container.querySelector<HTMLButtonElement>("button:not([hidden])")!;
  expect(button.textContent).toContain("Add project");
  expect(container.textContent).not.toContain("Select a session");

  // The button is this region's only control, so focus asked for the terminal lands on it rather
  // than going out to the top bar — but never from `<body>`, as at launch.
  act(() => handle.current?.focus());
  expect(document.activeElement).toBe(document.body);
  elsewhere.focus();
  act(() => handle.current?.focus());
  expect(document.activeElement).toBe(button);

  act(() => button.click());
  expect(onAddProject).toHaveBeenCalledTimes(1);

  // A project added anywhere takes the button away while it holds focus; the line is back and
  // focus is handed on rather than left on `<body>`.
  show({});
  expect(container.textContent).toContain("Select a session");
  expect(document.activeElement).toBe(topBarButton);
});

it("offers the contextual session action and hands focus on when it disappears", () => {
  const onPress = vi.fn();
  const { container, handle, topBarButton, elsewhere, show } = mountEmpty({ newSession: { message: "New session", label: "New session", onPress } });
  expect(container.textContent).not.toContain("Select a session");
  const button = container.querySelector<HTMLButtonElement>("button:not([hidden])")!;
  elsewhere.focus();
  act(() => handle.current?.focus());
  expect(document.activeElement).toBe(button);
  act(() => button.click());
  expect(onPress).toHaveBeenCalledOnce();
  show({});
  expect(document.activeElement).toBe(topBarButton);
});

it("shows the installation step without a creation control when no agent is available", () => {
  const { container } = mountEmpty({ noAgents: "install" });
  expect(container.textContent).toContain("install at least one tool");
  // An ordinary absence, so its glyph is a neutral one rather than a warning.
  expect(container.querySelector('[data-slot="empty-state"] svg.lucide-bot-off')).not.toBeNull();
  expect(container.querySelector("svg.lucide-triangle-alert")).toBeNull();
  expect(container.textContent).not.toContain("Select a session");
  expect(container.querySelector<HTMLButtonElement>("button:not([hidden])")).toBeNull();
});

it("offers to add an agent instead of the installation step when installed agents are waiting to be added", () => {
  const onAddAgent = vi.fn();
  const { container } = mountEmpty({ noAgents: "add", onAddAgent });
  expect(container.textContent).toContain("No agent added");
  expect(container.textContent).toContain("Add an agent to open a session.");
  expect(container.textContent).not.toContain("install at least one tool");
  const button = Array.from(container.querySelectorAll("button")).find((b) => b.textContent === "Add agent");
  act(() => button!.click());
  expect(onAddAgent).toHaveBeenCalledOnce();
});

it("covers the terminal with the switching state, inert and ahead of a failing connection, while an account switch holds the session", () => {
  const { container, root } = mount();
  FakeController.last.settle("open");
  const terminal = () => isolatedTerminal(container);
  expect(terminal().hasAttribute("inert")).toBe(false);
  render(root, { ...session, switching_account: true });
  speak();
  expect(status(container)).toBe("Switching account");
  expect(terminal().hasAttribute("inert")).toBe(true);
  expect(container.querySelector("[data-slot=spinner]")).not.toBeNull();
  FakeController.last.settle("closed");
  expect(status(container)).toBe("Switching account");
  render(root, session);
  expect(status(container)).not.toBe("Switching account");
  expect(terminal().hasAttribute("inert")).toBe(false);
});

it("offers no Resume over the cover while an account switch holds an interrupted session", () => {
  const interrupted = sessionOf("s-1", "c-1", undefined, "Work", "interrupted");
  const { container, root } = mount();
  render(root, interrupted);
  expect(container.textContent).toContain("Not running");
  render(root, { ...interrupted, switching_account: true });
  speak();
  expect(status(container)).toBe("Switching account");
  expect(container.textContent).not.toContain("Not running");
  expect([...container.querySelectorAll("button")].map((button) => button.textContent)).not.toContain("Resume");
});

it("covers a starting session with its own message and attaches only once it is up", () => {
  const starting = sessionOf("s-1", "c-1", undefined, "Work", "starting");
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  render(root, starting);
  const controller = FakeController.last;
  expect(controller.attaches).toBe(0);
  speak();
  expect(status(container)).toBe("Starting session…");
  expect(container.querySelector("[data-slot=spinner]")).not.toBeNull();
  // No process to connect to, so no error cover, no Reconnect and no unavailable message.
  act(() => void vi.advanceTimersByTime(LOADING_LIMIT_MS + 1));
  expect(status(container)).toBe("Starting session…");
  expect(container.querySelector<HTMLButtonElement>("button:not([hidden])")).toBeNull();
  expect(controller.attaches).toBe(0);

  render(root, { ...starting, status: "idle" });
  expect(controller.attaches).toBe(1);
  expect(status(container)).toBe("Loading terminal…");
});

it("reports a native terminal opener failure with the shared actionable destination fallback", async () => {
  const cause = new Error("Browser launch failed");
  platform.links = { open: vi.fn().mockRejectedValue(cause) };
  const root = createRoot(document.body.appendChild(document.createElement("div")));
  roots.push(root);
  render(root);
  await act(async () => FakeController.last.openLink("https://example.com/docs"));
  expect(platform.links.open).toHaveBeenCalledExactlyOnceWith("https://example.com/docs");
  expect(errorAlerts.getSnapshot()[0]).toMatchObject({
    message: "The link did not open in a browser. Its address is shown below.",
    classification: { kind: "actionable", recovery: "openExternal" },
    error: { cause },
  });
});

it("hands a path with a view to the file viewer once it has read it, and opens any other on the host", async () => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  const onViewFile = vi.fn();
  const inProject = { ...session, project_id: "p-1" };
  request.mockClear();
  request.mockResolvedValueOnce({ type: "project_file", id: null, project: "p-1", worktree: "w", path: "src/a.ts", source: { kind: "live", version: "1" }, file: { size: 1, kind: "text", media_type: null, text: "x", data: null } } as never);
  act(() => root.render(host(<TerminalPane session={inProject} onResume={async () => true} onAddAgent={() => {}} onViewFile={onViewFile} />)));
  await act(async () => FakeController.last.openPath({ path: "/r/src/a.ts", line: 3, view: { worktree: "w", path: "src/a.ts" } }));
  expect(onViewFile).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ project: "p-1", worktree: "w", path: "src/a.ts", line: 3 }));
  expect(request.mock.calls.map(([body]) => (body as { type: string }).type)).toEqual(["read_project_file"]);
  await act(async () => FakeController.last.openPath({ path: "/tmp/b.png" }));
  expect(onViewFile).toHaveBeenCalledOnce();
  expect(request).toHaveBeenLastCalledWith({ type: "open_path_link", project: "p-1", path: "/tmp/b.png" });
});

const archived = sessionOf("s-old", "c-1", undefined, "Old work", "archived");

function mountArchived(props: Partial<React.ComponentProps<typeof TerminalPane>> = {}) {
  const handle = createRef<TerminalPaneHandle>();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  const show = (next: Partial<React.ComponentProps<typeof TerminalPane>> = props) =>
    act(() => root.render(host(<TerminalPane ref={handle} session={archived} onResume={async () => true} onAddAgent={() => {}} {...next} />)));
  show();
  return { container, handle, show };
}

it("shows an archived session's history over an inert terminal", async () => {
  const { container } = mountArchived();
  await act(async () => {});
  expect(request).toHaveBeenCalledWith({ type: "read_session_history", session: "s-old" });
  expect(container.querySelector("section h2")?.textContent).toBe("Session history");
  expect(container.querySelector("[inert]")?.contains(container.querySelector("[dir=ltr]"))).toBe(true);
  expect(FakeController.last.attaches).toBe(0);
  expect(FakeController.last.detaches).toEqual([{}]);
  expect(container.textContent).toContain("This session is archived");
});

// The gallery's archived-session scenario reaches Reopen by the label in the window's language.
it.each(Object.keys(CATALOGS) as Language[])("offers an archived session's Reopen by its name in %s, and reopens the session when pressed", async (language) => {
  const previous = currentLanguage();
  try {
    act(() => setLanguage(language));
    const onResume = vi.fn(async (_id: string) => true);
    const { container } = mountArchived({ onResume });
    await act(async () => {});
    const buttons = [...container.querySelectorAll<HTMLElement>("[data-region=terminal] button")].filter((button) => button.textContent === format(language, "sidebar.session.reopen"));
    expect(buttons).toHaveLength(1);
    act(() => buttons[0].click());
    expect(onResume).toHaveBeenCalledWith("s-old");
  } finally {
    act(() => setLanguage(previous));
  }
});

it("keeps an interrupted session's saved screen and shows no history for it", () => {
  const { container } = mountArchived({ session: sessionOf("s-int", "c-1", undefined, "Stopped", "interrupted") });
  expect(container.querySelector("section")).toBeNull();
  expect(FakeController.last.detaches).toEqual([{ keepScreenFor: "s-int", savedOutputUrl: "" }]);
});

it("puts focus asked for the terminal on the history's scrolling region when it is archived", async () => {
  request.mockResolvedValueOnce({ type: "session_history", id: null, messages: [{ speaker: "user", text: "Hello", turn: 1 }] } as never);
  const { container, handle } = mountArchived();
  await act(async () => {});
  act(() => handle.current?.focus());
  expect(FakeController.last.focuses).toBe(0);
  expect(document.activeElement).toBe(container.querySelector("[role=region][tabindex]"));
});

it("covers the history with the loading state while a reopen is under way", async () => {
  const { container, show } = mountArchived();
  await act(async () => {});
  show({ resuming: true });
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_DELAY_MS));
  expect(container.querySelector("[inert] section")).not.toBeNull();
  const statuses = [...container.querySelectorAll("[role=status]")].map((node) => node.textContent);
  expect(statuses).toContain("Resuming session…");
});

it("moves focus held inside the pane to the history when the session on screen becomes archived", async () => {
  const { container, show } = mountArchived({ session });
  const held = container.querySelector("[data-region=terminal]")!.appendChild(document.createElement("button"));
  held.focus();
  expect(document.activeElement).toBe(held);
  show({ session: { ...session, status: "archived" } });
  await act(async () => {});
  expect(container.querySelector("section")?.contains(document.activeElement)).toBe(true);
});

describe("selecting another session while focus is in the history", () => {
  /** An archived pane with focus in its history, and the modality restored after the test. */
  async function mountFocusedHistory(modality: "keyboard" | "pointer", props: Partial<React.ComponentProps<typeof TerminalPane>> = {}) {
    withModality(modality);
    const pane = mountArchived(props);
    await act(async () => {});
    act(() => pane.handle.current?.focus());
    expect(pane.container.querySelector("section")?.contains(document.activeElement)).toBe(true);
    return pane;
  }

  it.each([
    ["archived", "pointer", false],
    ["archived", "keyboard", true],
    ["live", "pointer", false],
    ["live", "keyboard", true],
    ["none", "pointer", false],
    ["none", "keyboard", true],
  ] as const)("keeps focus where focus meant for the terminal goes when %s is selected by %s, ringed only after a key", async (target, modality, ringed) => {
    // An archived target is reached through the pane's own focus, so the window's restore is only given for the others;
    // the restore here leaves focus on `<body>` for no selection, as the window's does, which is then handed on.
    const restoreFocus = vi.fn();
    const props = target === "archived" ? {} : { restoreFocus, noAgents: "add" as const };
    const { container, show } = await mountFocusedHistory(modality, props);
    const input = terminalInput(container);
    if (target === "live") restoreFocus.mockImplementation(() => input.focus());
    show(target === "archived" ? { session: sessionOf("s-older", "c-1", undefined, "Older work", "archived") } : { ...props, session: target === "live" ? session : undefined, noSelection: target === "none" });
    await act(async () => {});
    if (target === "live") {
      expect(restoreFocus).toHaveBeenCalledOnce();
      expect(document.activeElement).toBe(input);
    } else if (target === "none") {
      expect(restoreFocus).toHaveBeenCalledOnce();
      expect(document.activeElement).toBe(emptyAction(container));
    } else expect(container.querySelector("section")?.contains(document.activeElement)).toBe(true);
    expect(container.querySelector("[data-focus-visible]") !== null).toBe(ringed);
    expect(getInteractionModality()).toBe(modality);
  });

  it("hands focus left on <body> by no selection to the top bar when the empty state offers no action", async () => {
    const bar = addTopBar();
    const { container, show } = await mountFocusedHistory("pointer");
    show({ session: undefined, noSelection: true });
    await act(async () => {});
    expect(container.querySelector("[data-region=terminal] button")).toBeNull();
    expect(document.activeElement).toBe(bar);
    expect(getInteractionModality()).toBe("pointer");
  });

  it("hands focus on with no ring after a pointer press when the live session selected is held by an account switch, which leaves its terminal inert", async () => {
    const bar = addTopBar();
    const { show } = await mountFocusedHistory("pointer");
    // The held terminal cannot take the focus asked of it (the fake controller moves none, as an inert one would).
    show({ session: { ...session, switching_account: true } });
    await act(async () => {});
    expect(document.activeElement).toBe(bar);
    expect(getInteractionModality()).toBe("pointer");
  });

  it("focuses the live session's terminal itself when none is given to restore, with no ring after a pointer selection", async () => {
    const { container, show } = await mountFocusedHistory("pointer");
    const input = terminalInput(container);
    const focus = vi.spyOn(FakeController.last, "focus").mockImplementation(() => input.focus());
    show({ session });
    await act(async () => {});
    expect(focus).toHaveBeenCalledOnce();
    expect(container.querySelector("section")).toBeNull();
    expect(document.activeElement).toBe(input);
    expect(container.querySelector("[data-focus-visible]")).toBeNull();
    expect(getInteractionModality()).toBe("pointer");
  });

  it("hands focus on with no ring when a selected session's record goes away after a pointer press", async () => {
    const restoreFocus = vi.fn();
    const { show } = await mountFocusedHistory("pointer", { restoreFocus });
    show({ session: undefined, noSelection: false, restoreFocus });
    await act(async () => {});
    expect(restoreFocus).toHaveBeenCalledOnce();
    expect(getInteractionModality()).toBe("pointer");
  });

  it.each([
    ["offers an action", true],
    ["offers no action", false],
  ] as const)("hands focus to where it goes from a live terminal, with no ring after a pointer press, when the selected session's record goes away and the empty state %s", async (_case, offered) => {
    const bar = addTopBar();
    const props = offered ? { noAgents: "add" as const } : {};
    const { container, show } = await mountFocusedHistory("pointer", props);
    show({ ...props, session: undefined, noSelection: false });
    await act(async () => {});
    expect(document.activeElement).toBe(offered ? emptyAction(container) : bar);
    // The test's top bar is a plain button, which draws no ring of its own; the modality is what the ring follows.
    if (offered) expect(focusedElementRinged()).toBe(false);
    expect(getInteractionModality()).toBe("pointer");
  });

  it("does not take focus back for the terminal when focus was not in the history", async () => {
    const restoreFocus = vi.fn();
    const { container, show } = mountArchived({ restoreFocus });
    await act(async () => {});
    const outside = document.body.appendChild(document.createElement("button"));
    act(() => outside.focus());
    const controller = FakeController.last;
    const focuses = controller.focuses;
    show({ session, restoreFocus });
    await act(async () => {});
    expect(restoreFocus).not.toHaveBeenCalled();
    expect(controller.focuses).toBe(focuses);
    expect(document.activeElement).toBe(outside);
    expect(container.querySelector("[data-focus-visible]")).toBeNull();
  });
});

describe("clearing the selection while focus is in a live session's terminal", () => {
  /** A live pane with focus in its terminal and the modality of the input that put it there, restored after the test. */
  function mountFocusedLive(modality: "keyboard" | "pointer", props: Partial<React.ComponentProps<typeof TerminalPane>> = {}) {
    withModality(modality);
    const pane = mountEmpty({ session, ...props });
    const input = terminalInput(pane.container);
    act(() => input.focus());
    expect(document.activeElement).toBe(input);
    return pane;
  }

  it.each([
    ["cleared by the user", "pointer", false, true, "pointer"],
    ["cleared by the user", "keyboard", true, true, "keyboard"],
    ["gone by itself", "pointer", false, false, "pointer"],
    ["gone by itself", "keyboard", true, false, "keyboard"],
  ] as const)("hands focus to the empty state's action when the session is %s after a %s press, ringed: %s", (_case, modality, ringed, noSelection, after) => {
    const { container, show } = mountFocusedLive(modality, { noAgents: "add" });
    show({ session: undefined, noAgents: "add", noSelection });
    expect(document.activeElement).toBe(emptyAction(container));
    expect(focusedElementRinged()).toBe(ringed);
    expect(getInteractionModality()).toBe(after);
  });

  it.each([
    ["cleared by the user", "pointer", true],
    ["gone by itself", "pointer", false],
  ] as const)("hands focus to the top bar when the session is %s and the empty state offers no action, leaving modality %s after a pointer press", (_case, after, noSelection) => {
    const { container, topBarButton, show } = mountFocusedLive("pointer");
    show({ session: undefined, noSelection });
    expect(container.querySelector("[data-region=terminal] button")).toBeNull();
    expect(document.activeElement).toBe(topBarButton);
    expect(getInteractionModality()).toBe(after);
  });

  it.each([
    ["cleared by the user", true],
    ["gone by itself", false],
  ] as const)("leaves a tap's destination to the window's restore, not the empty state's action, when the session is %s, with no ring", (_case, noSelection) => {
    // After a tap the window's `focusTerminal` lands on the top bar rather than on something that takes text; the
    // pane must not move focus on from there to the action it offers.
    const restoreFocus = vi.fn(() => document.querySelector<HTMLElement>("[data-region=topbar] button")!.focus());
    const pane = mountFocusedLive("pointer", { noAgents: "add", restoreFocus });
    const { topBarButton } = pane;
    document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "touch" }));
    onTestFinished(() => void document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "mouse" })));
    pane.show({ session: undefined, noAgents: "add", noSelection, restoreFocus });
    expect(restoreFocus).toHaveBeenCalledOnce();
    expect(document.activeElement).toBe(topBarButton);
    expect(focusedElementRinged()).toBe(false);
    expect(getInteractionModality()).toBe("pointer");
  });

  it.each([
    ["cleared by the user", true],
    ["gone by itself", false],
  ] as const)("hands focus to the window's restore first when the session is %s, then on to the empty state's action if that leaves it on <body>", (_case, noSelection) => {
    // The window's restore leaves `<body>` alone with no session, as at launch.
    const restoreFocus = vi.fn();
    const { container, show } = mountFocusedLive("keyboard", { noAgents: "add", restoreFocus });
    show({ session: undefined, noAgents: "add", noSelection, restoreFocus });
    expect(restoreFocus).toHaveBeenCalledOnce();
    expect(document.activeElement).toBe(emptyAction(container));
  });

  it.each([
    ["cleared by the user", true],
    ["gone by itself", false],
  ] as const)("moves nothing when the session is %s while focus is outside the pane", (_case, noSelection) => {
    const { elsewhere, show } = mountEmpty({ session, noAgents: "add" });
    withModality("pointer");
    act(() => elsewhere.focus());
    show({ session: undefined, noAgents: "add", noSelection });
    expect(document.activeElement).toBe(elsewhere);
    expect(focusedElementRinged()).toBe(false);
    expect(getInteractionModality()).toBe("pointer");
  });
});

describe("while the archive view covers the pane", () => {
  const root = (container: HTMLElement) => container.querySelector("[data-region=terminal]");
  /** Every control in the pane sits under an inert ancestor, so Tab from the view cannot reach it. */
  const unreachable = (container: HTMLElement) => Array.from(container.querySelectorAll("button, a, [tabindex]")).every((control) => control.closest("[inert]") !== null);

  it("keeps the terminal inert, and reachable again once the view is going", () => {
    const { container, show } = mountEmpty({ session, behindArchive: true });
    expect(container.querySelector("[inert]")?.contains(container.querySelector("[dir=ltr]"))).toBe(true);
    show({ session, behindArchive: false });
    expect(container.querySelector("[inert]")).toBeNull();
  });

  it("makes the whole pane inert, so an archived session's controls cannot be reached", async () => {
    const owner = sessionOf("s-owner", "c-1", undefined, "Release captain", "idle");
    setState({ sessions: new Map([[owner.id, owner]]) });
    const { container, show } = mountArchived({ session: { ...archived, bound_to: owner.id }, onSelectSession: () => {}, behindArchive: true });
    await act(async () => {});
    expect(root(container)?.hasAttribute("inert")).toBe(true);
    expect(container.querySelector("header button")).not.toBeNull();
    expect(Array.from(container.querySelectorAll("button")).some((button) => button.textContent === "Reopen")).toBe(true);
    expect(unreachable(container)).toBe(true);
    show({ session: { ...archived, bound_to: owner.id }, onSelectSession: () => {}, behindArchive: false });
    expect(root(container)?.hasAttribute("inert")).toBe(false);
  });

  it("makes the no-process bar's Resume unreachable too", () => {
    const { container } = mountEmpty({ session: sessionOf("s-int", "c-1", undefined, "Stopped", "interrupted"), behindArchive: true });
    expect(Array.from(container.querySelectorAll("button")).some((button) => button.textContent === "Resume")).toBe(true);
    expect(unreachable(container)).toBe(true);
  });

  it("makes a cover's controls unreachable too", () => {
    const { container } = mountEmpty({ session, behindArchive: true });
    for (let i = 0; i < 5; i++) failAttempt();
    FakeController.last.settle("closed");
    expect(Array.from(container.querySelectorAll("button")).some((button) => button.textContent === "Reconnect")).toBe(true);
    expect(unreachable(container)).toBe(true);
  });

  it.each([
    ["the live terminal", false],
    ["the history", true],
  ] as const)("hands focus held in %s to the window's restore as the view opens, leaving the modality alone", async (_where, inHistory) => {
    withModality("pointer");
    const restoreFocus = vi.fn(() => stand.focus());
    const stand = document.body.appendChild(document.createElement("button"));
    const pane = inHistory ? mountArchived({ restoreFocus }) : mountEmpty({ session, restoreFocus });
    await act(async () => {});
    if (inHistory) act(() => pane.handle.current?.focus());
    else act(() => terminalInput(pane.container).focus());
    expect(pane.container.contains(document.activeElement)).toBe(true);
    pane.show({ ...(inHistory ? {} : { session }), restoreFocus, behindArchive: true });
    expect(restoreFocus).toHaveBeenCalledOnce();
    expect(document.activeElement).toBe(stand);
    expect(getInteractionModality()).toBe("pointer");
  });

  it("moves nothing as the view opens when focus is outside the pane", () => {
    withModality("pointer");
    const restoreFocus = vi.fn();
    const { elsewhere, show } = mountEmpty({ session, restoreFocus });
    act(() => elsewhere.focus());
    show({ session, restoreFocus, behindArchive: true });
    expect(restoreFocus).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(elsewhere);
  });
});

it("reaches the history itself through the handle while it is still loading", () => {
  request.mockImplementationOnce(() => new Promise(() => {}));
  const { container, handle } = mountArchived();
  act(() => handle.current?.focus());
  expect(document.activeElement).toBe(container.querySelector("section"));
});

it("keeps focus asked for the terminal off the header's owner link while the history loads", () => {
  const owner = sessionOf("s-owner", "c-1", undefined, "Release captain", "idle");
  setState({ sessions: new Map([[owner.id, owner]]) });
  request.mockImplementationOnce(() => new Promise(() => {}));
  const { container, handle } = mountArchived({ session: { ...archived, bound_to: owner.id }, onSelectSession: () => {} });
  expect(container.querySelector("header button")?.textContent).toContain("Release captain");
  act(() => handle.current?.focus());
  expect(document.activeElement).toBe(container.querySelector("section"));
});

it("hands focus lost with the control that held it to the window's restore, not the inert terminal", async () => {
  const owner = sessionOf("s-owner", "c-1", undefined, "Release captain", "idle");
  setState({ sessions: new Map([[owner.id, owner]]) });
  const target = document.body.appendChild(document.createElement("button"));
  const restoreFocus = vi.fn(() => target.focus());
  const { container, show } = mountArchived({ session: { ...archived, bound_to: owner.id }, onSelectSession: () => {}, restoreFocus });
  await act(async () => {});
  const link = container.querySelector<HTMLElement>("header button")!;
  withModality("pointer");
  act(() => link.focus());
  show({ session: owner, onSelectSession: () => {}, restoreFocus });
  expect(restoreFocus).toHaveBeenCalledOnce();
  expect(document.activeElement).toBe(target);
  expect(container.querySelector("[data-focus-visible]")).toBeNull();
  expect(getInteractionModality()).toBe("pointer");
});

it("falls back to the Reopen button when the history has no control to focus", () => {
  request.mockImplementationOnce(() => new Promise(() => {}));
  const { container, handle } = mountArchived();
  container.querySelector("section")!.replaceChildren();
  container.querySelector("section")!.removeAttribute("tabindex");
  act(() => handle.current?.focus());
  expect(document.activeElement?.textContent).toBe("Reopen");
});

it("hands focus back to the history when a reopen ends with the session still archived", async () => {
  const { container, show } = mountArchived();
  await act(async () => {});
  show({ resuming: true });
  (document.activeElement as HTMLElement | null)?.blur();
  expect(document.activeElement).toBe(document.body);
  show({ resuming: false });
  expect(container.querySelector("section")?.contains(document.activeElement)).toBe(true);
});

const statusRegions = (container: HTMLElement) => [...container.querySelectorAll<HTMLElement>("[role=status]:not([data-slot=spinner])")];

it("mounts the cover's status region empty and writes its first text after the first write delay", () => {
  const { container } = mount();
  const [region] = statusRegions(container);
  expect(region.textContent).toBe("");
  expect(container.textContent).toContain("Loading terminal…");
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_DELAY_MS - 1));
  expect(region.textContent).toBe("");
  act(() => void vi.advanceTimersByTime(1));
  expect(region.textContent).toBe("Loading terminal…");
  // Later changes go into the same region at once.
  FakeController.last.settle("closed");
  expect(statusRegions(container)[0]).toBe(region);
  expect(region.textContent).toBe("Reconnecting the terminal…");
});

it("mounts the switching and resuming statuses empty, then writes them", () => {
  const { container, root } = mount();
  render(root, { ...session, switching_account: true });
  const switching = statusRegions(container)[0];
  expect(switching.textContent).toBe("");
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_DELAY_MS));
  expect(switching.textContent).toBe("Switching account");
  const noProcessPane = mountArchived();
  noProcessPane.show({ resuming: true });
  expect(statusRegions(noProcessPane.container).map((region) => region.textContent)).toEqual(["", "", "", "", ""]);
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_DELAY_MS));
  expect(statusRegions(noProcessPane.container).map((region) => region.textContent)).toContain("Resuming session…");
});

it("covers the terminal for a session this window queued an account switch for, until it leaves the queue", () => {
  const { container } = mount();
  expect(status(container)).not.toBe("Switching account");
  act(() => setAccountSwitchesQueued(daemon.store, [session.id], true));
  speak();
  expect(status(container)).toBe("Switching account");
  act(() => setAccountSwitchesQueued(daemon.store, [session.id], false));
  speak();
  expect(status(container)).not.toBe("Switching account");
});

it("speaks a no-process bar's sentence once, from a region mounted empty, and leaves the button reachable", () => {
  const interrupted = sessionOf("s-1", "c-1", undefined, "Work", "interrupted");
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  render(root, interrupted);
  const spoken = () => statusRegions(container).map((region) => region.textContent);
  expect(spoken().every((text) => text === "")).toBe(true);
  const sentence = [...container.querySelectorAll("[aria-hidden=true]")].find((node) => node.textContent === "Not running");
  expect(sentence).toBeDefined();
  expect(container.querySelector("button")?.closest("[aria-hidden=true]")).toBeNull();
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_DELAY_MS));
  expect(spoken().filter((text) => text === "Not running")).toHaveLength(1);
  // Resume's own cover takes the bar's place without a second copy of the bar's sentence.
  render(root, { ...interrupted, switching_account: true });
  expect(spoken().filter((text) => text === "Not running")).toHaveLength(0);
});

it("speaks an archived bar's sentence and leaves the history's own announcer to its own states", async () => {
  request.mockResolvedValueOnce({ type: "session_history", id: null, messages: [{ speaker: "user", text: "Hello", turn: 1 }] } as never);
  const { container } = mountArchived();
  await act(async () => {});
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_DELAY_MS));
  const spoken = statusRegions(container).map((region) => region.textContent).filter(Boolean);
  expect(spoken).toEqual(["This session is archived"]);
});

it("holds the no-process sentence while a modal is open and writes it after the close and the focus move", () => {
  const interrupted = sessionOf("s-1", "c-1", undefined, "Work", "interrupted");
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  const show = (modalOpen: boolean) => act(() => root.render(host(<TerminalPane session={interrupted} modalOpen={modalOpen} onResume={async () => true} onAddAgent={() => {}} />)));
  const spoken = () => statusRegions(container).map((region) => region.textContent).filter(Boolean);
  show(true);
  speak();
  expect(spoken()).toEqual([]);
  show(false);
  act(() => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS - 1));
  expect(spoken()).toEqual([]);
  act(() => void vi.advanceTimersByTime(1));
  expect(spoken()).toEqual(["Not running"]);
});

it("keeps a sentence already spoken in the region while a modal opens and closes over it", () => {
  const interrupted = sessionOf("s-1", "c-1", undefined, "Work", "interrupted");
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  const show = (modalOpen: boolean) => act(() => root.render(host(<TerminalPane session={interrupted} modalOpen={modalOpen} onResume={async () => true} onAddAgent={() => {}} />)));
  show(false);
  speak();
  const region = () => statusRegions(container).find((node) => node.textContent === "Not running");
  expect(region()).toBeDefined();
  const text = () => statusRegions(container).map((node) => node.textContent).filter(Boolean);
  show(true);
  expect(text()).toEqual(["Not running"]);
  act(() => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS / 3));
  expect(text()).toEqual(["Not running"]);
  show(false);
  expect(text()).toEqual(["Not running"]);
  act(() => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS * 2));
  expect(text()).toEqual(["Not running"]);
});

const RECOVERED = "Recovered terminal display";
/** The recovered-display sentences currently in the status regions. */
const recovered = (container: HTMLElement) => statusRegions(container).map((region) => region.textContent).filter((text) => text === RECOVERED);

it("speaks the recovered display only when the flag turns on while the session is open, not as it opens or for another session", () => {
  const { container, root } = mount();
  speak();
  render(root, { ...session, screen_inexact: false });
  speak();
  expect(recovered(container)).toEqual([]);
  render(root, { ...session, screen_inexact: true });
  advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  expect(recovered(container)).toEqual([]);
  advance(1);
  expect(recovered(container)).toEqual([RECOVERED]);
  // Not again after a redraw with the flag still on.
  render(root, { ...session, screen_inexact: true, title: "Renamed" });
  expect(recovered(container)).toEqual([RECOVERED]);
  render(root, { ...session, screen_inexact: false });
  expect(recovered(container)).toEqual([]);
});

it("does not speak the recovered display for a session found with the flag on, or one selected with it on", () => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  render(root, { ...session, screen_inexact: true });
  speak();
  expect(recovered(container)).toEqual([]);
  render(root, sessionOf("s-2", "c-1", undefined, "Other", "working", { screen_inexact: false }));
  render(root, sessionOf("s-3", "c-1", undefined, "Third", "working", { screen_inexact: true }));
  speak();
  expect(recovered(container)).toEqual([]);
});

describe("the recovered display's announcement, delivered while the window is attended", () => {
  const recoveredRegion = (container: HTMLElement) => statusRegions(container).find((region) => region.textContent === RECOVERED);
  /** Mounts a session with the flag off, as a view open before the flag rises. */
  function mountOpen() {
    const container = document.body.appendChild(document.createElement("div"));
    const root = createRoot(container);
    roots.push(root);
    render(root, { ...session, screen_inexact: false });
    speak();
    return { container, root };
  }

  it("writes it after the focus-move wait of attended time, and empties it again after the read time", () => {
    const { container, root } = mountOpen();
    render(root, { ...session, screen_inexact: true });
    advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
    expect(recovered(container)).toEqual([]);
    advance(1);
    expect(recovered(container)).toEqual([RECOVERED]);
    advance(READ_TIME_MS - 1);
    expect(recovered(container)).toEqual([RECOVERED]);
    advance(1);
    expect(recovered(container)).toEqual([]);
  });

  it("holds a rise while the agent area is inert and does not replay it on later exposure", () => {
    const { container, root } = mountOpen();
    const flagged = { ...session, screen_inexact: true };
    render(root, flagged, false);
    advance(READ_TIME_MS + AFTER_FOCUS_MOVE_DELAY_MS * 2);
    expect(recovered(container)).toEqual([]);
    render(root, flagged);
    advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
    expect(recovered(container)).toEqual([]);
    advance(1);
    expect(recovered(container)).toEqual([RECOVERED]);
    render(root, flagged, false);
    render(root, flagged);
    advance(AFTER_FOCUS_MOVE_DELAY_MS * 2);
    expect(recovered(container)).toEqual([]);
  });

  it("writes nothing while the user is away, and waits the whole time again once they are back", () => {
    const setAttended = attention;
    const { container, root } = mountOpen();
    render(root, { ...session, screen_inexact: true });
    setAttended(false);
    advance(AFTER_FOCUS_MOVE_DELAY_MS * 5);
    expect(recovered(container)).toEqual([]);
    setAttended(true);
    advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
    expect(recovered(container)).toEqual([]);
    advance(1);
    expect(recovered(container)).toEqual([RECOVERED]);
  });

  it("starts the wait over when the user leaves the window and comes back before it ends", () => {
    const setAttended = attention;
    const { container, root } = mountOpen();
    render(root, { ...session, screen_inexact: true });
    advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
    setAttended(false);
    setAttended(true);
    advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
    expect(recovered(container)).toEqual([]);
    advance(1);
    expect(recovered(container)).toEqual([RECOVERED]);
  });

  it("drops it unsaid when another session is selected before the wait ends", () => {
    const { container, root } = mountOpen();
    render(root, { ...session, screen_inexact: true });
    advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
    render(root, sessionOf("s-2", "c-1", undefined, "Other", "working", { screen_inexact: true }));
    advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
    expect(recovered(container)).toEqual([]);
  });

  it("drops it unsaid when the session is gone before the wait ends", () => {
    const { container, root } = mountOpen();
    render(root, { ...session, screen_inexact: true });
    advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
    act(() => root.render(host(<TerminalPane session={undefined} onResume={async () => true} onAddAgent={() => {}} />)));
    advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
    expect(recovered(container)).toEqual([]);
  });

  it("drops it unsaid when the display recovers (the flag turns off) before the wait ends", () => {
    const { container, root } = mountOpen();
    render(root, { ...session, screen_inexact: true });
    advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
    render(root, { ...session, screen_inexact: false });
    advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
    expect(recovered(container)).toEqual([]);
  });

  it("says it again when the flag goes off and comes on again for the same session", () => {
    const { container, root } = mountOpen();
    render(root, { ...session, screen_inexact: true });
    advance(AFTER_FOCUS_MOVE_DELAY_MS);
    expect(recovered(container)).toEqual([RECOVERED]);
    render(root, { ...session, screen_inexact: false });
    expect(recovered(container)).toEqual([]);
    render(root, { ...session, screen_inexact: true });
    expect(recovered(container)).toEqual([]);
    advance(AFTER_FOCUS_MOVE_DELAY_MS);
    expect(recovered(container)).toEqual([RECOVERED]);
  });

  it("sits in a live announcer beside an unrelated modal, outside the inert terminal", () => {
    const { container, root } = mountOpen();
    render(root, { ...session, screen_inexact: true });
    advance(AFTER_FOCUS_MOVE_DELAY_MS);
    const region = recoveredRegion(container)!;
    expect(region.closest("[data-live-announcer=true]")).not.toBeNull();
    expect(isolatedTerminal(container).contains(region)).toBe(false);
    expect(region.closest("[inert]")).toBeNull();
  });
});

describe("a session whose process the daemon cannot reach", () => {
  const pending = { ...session, holder_link: "pending" as const };
  const TITLE = "Reconnecting to the session…";
  const BODY = "The session process is still running. Input is unavailable until the connection returns. Reconnection is automatic.";
  const SENTENCE = "Reconnecting to the session. The session process is still running. Input is unavailable until the connection returns. Reconnection is automatic.";
  const region = (container: HTMLElement) => container.querySelector<HTMLElement>("[role=region][aria-label]");
  const terminal = isolatedTerminal;
  const buttons = (container: HTMLElement) => [...container.querySelectorAll("button:not([hidden])")].map((button) => button.textContent).filter(Boolean);
  const spoken = (container: HTMLElement) => statusRegions(container).map((r) => r.textContent).filter((text) => text === SENTENCE);

  /** Puts focus on an element standing for xterm's input, inside the terminal's mount element. */
  function focusTerminalInput(container: HTMLElement): HTMLTextAreaElement {
    const input = container.querySelector("[data-region=terminal] [dir=ltr]")!.appendChild(document.createElement("textarea"));
    input.focus();
    return input;
  }

  it("covers a terminal opened pending with the title and description alone, attaching nothing and holding its input", () => {
    const container = document.body.appendChild(document.createElement("div"));
    const root = createRoot(container);
    roots.push(root);
    render(root, pending);
    const controller = FakeController.last;
    expect(controller.attaches).toBe(0);
    expect(controller.held).toBe(true);
    expect(container.querySelectorAll("[role=region][aria-label]")).toHaveLength(1);
    const cover = region(container)!;
    expect(cover.textContent).toBe(`${TITLE}${BODY}`);
    // Named by the session, not by the title it holds, which VoiceOver would read twice.
    expect(cover.getAttribute("aria-label")).toBe("Work");
    expect(cover.hasAttribute("aria-labelledby")).toBe(false);
    expect(cover.getAttribute("aria-describedby")!.split(" ").map((id) => document.getElementById(id)!.textContent)).toEqual([TITLE, BODY]);
    expect(cover.querySelector("h2")!.textContent).toBe(TITLE);
    expect(terminal(container).hasAttribute("inert")).toBe(true);
    expect(container.querySelector("[data-slot=spinner]")).not.toBeNull();
    expect(buttons(container)).toEqual([]);
    expect(container.textContent).not.toContain("Not running");
    act(() => void vi.advanceTimersByTime(PAST_ANY_RETRY_MS));
    expect(controller.attaches).toBe(0);
  });

  it("covers an open terminal at once, over its old screen, and spends no retry while the socket drops", () => {
    const { container, root } = mount();
    FakeController.last.settle("open");
    FakeController.last.paint();
    expect(region(container)).toBeNull();
    render(root, pending);
    expect(region(container)).not.toBeNull();
    expect(FakeController.last.held).toBe(true);
    const attaches = FakeController.last.attaches;
    // Past every automatic attempt there is: no attach, no Reconnect, no failure cover.
    FakeController.last.settle("closed");
    act(() => void vi.advanceTimersByTime(PAST_ANY_RETRY_MS));
    FakeController.last.settle("closed");
    expect(FakeController.last.attaches).toBe(attaches);
    expect(buttons(container)).toEqual([]);
    expect(region(container)!.textContent).toBe(`${TITLE}${BODY}`);
    expect(statusRegions(container).map((r) => r.textContent)).not.toContain("Terminal disconnected");
  });

  it("replaces a Reconnect that was offered, and hands the focus it held to the cover", () => {
    const { container, root } = mount();
    for (let i = 0; i < 5; i++) failAttempt();
    FakeController.last.settle("closed");
    const button = container.querySelector<HTMLButtonElement>("button:not([hidden])")!;
    act(() => button.focus());
    render(root, pending);
    expect(buttons(container)).toEqual([]);
    expect(document.activeElement).toBe(region(container));
  });

  it("moves focus held in the terminal to the cover, and back only while it is still there on recovery", () => {
    const { container, root } = mount();
    const input = focusTerminalInput(container);
    // A hardware key, whatever input an earlier test left behind.
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    render(root, pending);
    expect(document.activeElement).toBe(region(container));
    // The move is not the user's, so the ring follows the last input, here a key.
    expect(region(container)!.hasAttribute("data-focus-visible")).toBe(true);
    const focuses = FakeController.last.focuses;
    render(root, session);
    expect(FakeController.last.focuses).toBe(focuses + 1);
    expect(input.isConnected).toBe(true);
    // The person was at the cover, so the recovery attach is theirs: it activates the terminal.
    expect(FakeController.last.lastUserInitiated).toBe(true);
  });

  it("never pulls focus on recovery once it has moved on, and attaches in the background", () => {
    const outside = document.body.appendChild(document.createElement("button"));
    const { container, root } = mount();
    focusTerminalInput(container);
    render(root, pending);
    act(() => outside.focus());
    const focuses = FakeController.last.focuses;
    render(root, session);
    expect(document.activeElement).toBe(outside);
    expect(FakeController.last.focuses).toBe(focuses);
    expect(FakeController.last.lastUserInitiated).toBe(false);
  });

  it("does not count focus a vanished Reconnect left, so a later recovery moves nothing", () => {
    const outside = document.body.appendChild(document.createElement("button"));
    const container = document.body.appendChild(document.createElement("div"));
    const root = createRoot(container);
    roots.push(root);
    const show = (current: typeof session) =>
      act(() => root.render(host(<TerminalPane session={current} onResume={async () => true} onAddAgent={() => {}} restoreFocus={() => outside.focus()} />)));
    show(session);
    for (let i = 0; i < 5; i++) failAttempt();
    FakeController.last.settle("closed");
    act(() => container.querySelector<HTMLButtonElement>("button:not([hidden])")!.focus());
    // The connection heals, the button goes, and the handoff puts focus outside the pane.
    FakeController.last.settle("open");
    FakeController.last.paint();
    expect(document.activeElement).toBe(outside);
    show(pending);
    act(() => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS));
    expect(spoken(container)).toEqual([SENTENCE]);
    const focuses = FakeController.last.focuses;
    show(session);
    expect(document.activeElement).toBe(outside);
    expect(FakeController.last.focuses).toBe(focuses);
    expect(FakeController.last.lastUserInitiated).toBe(false);
  });

  it("keeps the recovery the person's when the cover's removal blurs it the way Chromium does", () => {
    const { container, root } = mount();
    FakeController.last.settle("open");
    FakeController.last.paint();
    focusTerminalInput(container);
    render(root, pending);
    expect(document.activeElement).toBe(region(container));
    blurOnRemoval();
    render(root, session);
    expect(FakeController.last.lastUserInitiated).toBe(true);
  });

  it("sends focus to restoreFocus, not the terminal, on recovery after a tap", () => {
    const restoreFocus = vi.fn();
    const container = document.body.appendChild(document.createElement("div"));
    const root = createRoot(container);
    roots.push(root);
    const show = (current: typeof session) =>
      act(() => root.render(host(<TerminalPane session={current} onResume={async () => true} onAddAgent={() => {}} restoreFocus={restoreFocus} />)));
    show(session);
    focusTerminalInput(container);
    show(pending);
    expect(document.activeElement).toBe(region(container));
    document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "touch" }));
    const focuses = FakeController.last.focuses;
    show(session);
    expect(FakeController.last.focuses).toBe(focuses);
    expect(restoreFocus).toHaveBeenCalled();
    // Still the person's recovery, which activates the terminal.
    expect(FakeController.last.lastUserInitiated).toBe(true);
    document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "mouse" }));
  });

  it("leaves focus outside the pane alone as it begins to be covered", () => {
    const outside = document.body.appendChild(document.createElement("button"));
    const { root } = mount();
    act(() => outside.focus());
    render(root, pending);
    expect(document.activeElement).toBe(outside);
  });

  it("lands focus asked for the terminal, and F6, on the cover while pending", () => {
    const handle = createRef<TerminalPaneHandle>();
    const container = document.body.appendChild(document.createElement("div"));
    const root = createRoot(container);
    roots.push(root);
    act(() => root.render(host(<TerminalPane ref={handle} session={pending} onResume={async () => true} onAddAgent={() => {}} />)));
    act(() => handle.current?.focus());
    expect(document.activeElement).toBe(region(container));
    expect(FakeController.last.focuses).toBe(0);
  });

  it("recovers by attaching once with a fresh budget, even after the attempts were spent, keeping the loading cover until paint", () => {
    const { container, root } = mount();
    for (let i = 0; i < 5; i++) failAttempt();
    FakeController.last.settle("closed");
    expect(status(container)).toBe("Terminal disconnected");
    render(root, pending);
    act(() => void vi.advanceTimersByTime(PAST_ANY_RETRY_MS));
    const attaches = FakeController.last.attaches;
    render(root, session);
    expect(FakeController.last.held).toBe(false);
    expect(FakeController.last.attaches).toBe(attaches + 1);
    expect(region(container)).toBeNull();
    speak();
    expect(status(container)).toBe("Loading terminal…");
    FakeController.last.settle("open");
    expect(status(container)).toBe("Loading terminal…");
    // A broadcast meanwhile attaches nothing more.
    render(root, { ...session, title: "Renamed" });
    expect(FakeController.last.attaches).toBe(attaches + 1);
    FakeController.last.paint();
    expect(status(container)).toBe("");
    // The attempts are fresh: a drop now is retried again.
    FakeController.last.settle("closed");
    act(() => void vi.advanceTimersByTime(reconnectDelay(MAX_AUTO_RECONNECT_ATTEMPTS)));
    expect(FakeController.last.attaches).toBe(attaches + 2);
  });

  it("shows the ended state, and attaches nothing, when the record becomes interrupted instead", () => {
    const { container, root } = mount();
    FakeController.last.settle("open");
    render(root, pending);
    const attaches = FakeController.last.attaches;
    render(root, { ...session, status: "interrupted", holder_link: "pending" });
    expect(region(container)).toBeNull();
    expect(container.textContent).toContain("Not running");
    expect(buttons(container)).toContain("Resume");
    expect(FakeController.last.attaches).toBe(attaches);
    expect(FakeController.last.held).toBe(false);
  });

  it("leaves an ordinary socket failure without the flag as it was", () => {
    const { container } = mount();
    failAttempt();
    expect(status(container)).toBe("Reconnecting the terminal…");
    expect(region(container)).toBeNull();
    for (let i = 0; i < 4; i++) failAttempt();
    FakeController.last.settle("closed");
    expect(buttons(container)).toContain("Reconnect");
  });

  it("speaks the sentence once on entering pending, not again for retries, renames or upserts", () => {
    const { container, root } = mount();
    speak();
    render(root, pending);
    expect(spoken(container)).toEqual([SENTENCE]);
    render(root, { ...pending, title: "Renamed" });
    FakeController.last.settle("closed");
    act(() => void vi.advanceTimersByTime(PAST_ANY_RETRY_MS));
    render(root, { ...pending, title: "Renamed again", status: "idle" });
    expect(spoken(container)).toEqual([SENTENCE]);
    expect(statusRegions(container).map((r) => r.textContent)).not.toContain(TITLE);
  });

  it("does not speak the sentence while the cover holds focus, which reads the same copy", () => {
    const { container, root } = mount();
    focusTerminalInput(container);
    render(root, pending);
    expect(document.activeElement).toBe(region(container));
    speak();
    act(() => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS));
    expect(spoken(container)).toEqual([]);
    // Opened pending with focus landed in the cover before the delay ends: not said either.
    const other = document.body.appendChild(document.createElement("div"));
    const otherRoot = createRoot(other);
    roots.push(otherRoot);
    const handle = createRef<TerminalPaneHandle>();
    act(() => otherRoot.render(host(<TerminalPane ref={handle} session={pending} onResume={async () => true} onAddAgent={() => {}} />)));
    act(() => handle.current?.focus());
    expect(document.activeElement).toBe(region(other));
    act(() => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS));
    expect(spoken(other)).toEqual([]);
  });

  it("speaks it after the focus delay for a terminal opened pending, and for another one selected pending", () => {
    const container = document.body.appendChild(document.createElement("div"));
    const root = createRoot(container);
    roots.push(root);
    render(root, pending);
    speak();
    expect(spoken(container)).toEqual([]);
    act(() => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS));
    expect(spoken(container)).toEqual([SENTENCE]);
    render(root, sessionOf("s-2", "c-1", undefined, "Other", "working"));
    expect(spoken(container)).toEqual([]);
    render(root, sessionOf("s-3", "c-1", undefined, "Third", "working", { holder_link: "pending" }));
    expect(spoken(container)).toEqual([]);
    act(() => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS));
    expect(spoken(container)).toEqual([SENTENCE]);
  });

  /** A pane opened on `current` with nothing yet attached, as a session picked while pending. */
  function open(current: typeof session) {
    const container = document.body.appendChild(document.createElement("div"));
    const root = createRoot(container);
    roots.push(root);
    render(root, current);
    speak();
    return { container, root };
  }

  it("writes the sentence of a pane opened pending after the wait, into a live announcer outside the inert terminal", () => {
    const { container } = open(pending);
    advance(AFTER_FOCUS_MOVE_DELAY_MS - FIRST_WRITE_DELAY_MS - 1);
    expect(spoken(container)).toEqual([]);
    advance(1);
    expect(spoken(container)).toEqual([SENTENCE]);
    const delayed = statusRegions(container).find((r) => r.textContent === SENTENCE)!;
    expect(delayed.closest("[data-live-announcer=true]")).not.toBeNull();
    expect(terminal(container).contains(delayed)).toBe(false);
    expect(delayed.closest("[inert]")).toBeNull();
  });

  it("writes nothing for a pane opened pending while the user is away, and waits the whole time once they are back", () => {
    const setAttended = attention;
    const { container } = open(pending);
    setAttended(false);
    advance(AFTER_FOCUS_MOVE_DELAY_MS * 5);
    expect(spoken(container)).toEqual([]);
    setAttended(true);
    advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
    expect(spoken(container)).toEqual([]);
    advance(1);
    expect(spoken(container)).toEqual([SENTENCE]);
  });

  it("starts the wait over when the user leaves the window and comes back before it ends", () => {
    const setAttended = attention;
    const { container } = open(pending);
    advance(AFTER_FOCUS_MOVE_DELAY_MS - FIRST_WRITE_DELAY_MS - 1);
    setAttended(false);
    setAttended(true);
    advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
    expect(spoken(container)).toEqual([]);
    advance(1);
    expect(spoken(container)).toEqual([SENTENCE]);
  });

  it("empties the sentence on a switch to another pending session and writes it again after the new one's own wait", () => {
    const { container, root } = open(pending);
    advance(AFTER_FOCUS_MOVE_DELAY_MS);
    expect(spoken(container)).toEqual([SENTENCE]);
    render(root, sessionOf("s-3", "c-1", undefined, "Third", "working", { holder_link: "pending" }));
    expect(spoken(container)).toEqual([]);
    advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
    expect(spoken(container)).toEqual([]);
    advance(1);
    expect(spoken(container)).toEqual([SENTENCE]);
  });

  it("does not carry the wait of the first pending session over to the one switched to", () => {
    const { container, root } = open(pending);
    advance(AFTER_FOCUS_MOVE_DELAY_MS - FIRST_WRITE_DELAY_MS - 1);
    render(root, sessionOf("s-3", "c-1", undefined, "Third", "working", { holder_link: "pending" }));
    advance(FIRST_WRITE_DELAY_MS + 1);
    expect(spoken(container)).toEqual([]);
    advance(AFTER_FOCUS_MOVE_DELAY_MS);
    expect(spoken(container)).toEqual([SENTENCE]);
  });

  it("drops the sentence unsaid when another session is selected, or the pending ends, before the wait", () => {
    const first = open(pending);
    advance(AFTER_FOCUS_MOVE_DELAY_MS - FIRST_WRITE_DELAY_MS - 1);
    render(first.root, sessionOf("s-2", "c-1", undefined, "Other", "working"));
    advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
    expect(spoken(first.container)).toEqual([]);

    const second = open(sessionOf("s-4", "c-1", undefined, "Fourth", "working", { holder_link: "pending" }));
    advance(AFTER_FOCUS_MOVE_DELAY_MS - FIRST_WRITE_DELAY_MS - 1);
    render(second.root, sessionOf("s-4", "c-1", undefined, "Fourth", "working"));
    advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
    expect(spoken(second.container)).toEqual([]);
  });

  it("says the sentence again for a session selected pending a second time, as a fresh announcement", () => {
    const { container, root } = open(pending);
    advance(AFTER_FOCUS_MOVE_DELAY_MS);
    expect(spoken(container)).toEqual([SENTENCE]);
    render(root, sessionOf("s-2", "c-1", undefined, "Other", "working"));
    expect(spoken(container)).toEqual([]);
    render(root, pending);
    expect(spoken(container)).toEqual([]);
    advance(AFTER_FOCUS_MOVE_DELAY_MS);
    expect(spoken(container)).toEqual([SENTENCE]);
  });

  it("does not write the sentence when focus is on the cover as the wait ends, nor queue it again for that period", () => {
    const outside = document.body.appendChild(document.createElement("button"));
    const setAttended = attention;
    const handle = createRef<TerminalPaneHandle>();
    const container = document.body.appendChild(document.createElement("div"));
    const root = createRoot(container);
    roots.push(root);
    act(() => root.render(host(<TerminalPane ref={handle} session={pending} onResume={async () => true} onAddAgent={() => {}} />)));
    act(() => handle.current?.focus());
    expect(document.activeElement).toBe(region(container));
    advance(AFTER_FOCUS_MOVE_DELAY_MS);
    expect(spoken(container)).toEqual([]);
    // Focus leaving the cover, and the user leaving and coming back, queue nothing for the same period.
    act(() => outside.focus());
    setAttended(false);
    setAttended(true);
    advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
    expect(spoken(container)).toEqual([]);
  });

  it("writes the sentence when focus was on the cover but has left it by the time the wait ends", () => {
    const outside = document.body.appendChild(document.createElement("button"));
    const handle = createRef<TerminalPaneHandle>();
    const container = document.body.appendChild(document.createElement("div"));
    const root = createRoot(container);
    roots.push(root);
    act(() => root.render(host(<TerminalPane ref={handle} session={pending} onResume={async () => true} onAddAgent={() => {}} />)));
    act(() => handle.current?.focus());
    advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
    act(() => outside.focus());
    advance(1);
    expect(spoken(container)).toEqual([SENTENCE]);
  });

  it("writes the sentence at once into the cover's own region when the shown session turns pending, even with the user away", () => {
    const setAttended = attention;
    const { container, root } = open(session);
    setAttended(false);
    render(root, pending);
    expect(status(container)).toBe(SENTENCE);
    const immediate = statusRegions(container).find((r) => r.textContent === SENTENCE)!;
    expect(immediate.closest("[data-live-announcer=true]")).toBeNull();
    // Written once: nothing more arrives from the attended wait.
    setAttended(true);
    advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
    expect(spoken(container)).toEqual([SENTENCE]);
  });

  it("holds the recovered display's announcement until the cover has gone, and does not repeat one already said", () => {
    const { container, root } = mount();
    speak();
    render(root, { ...pending, screen_inexact: false });
    act(() => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS));
    render(root, { ...pending, screen_inexact: true });
    speak();
    expect(spoken(container)).toEqual([SENTENCE]);
    expect(recovered(container)).toEqual([]);
    render(root, { ...session, screen_inexact: true });
    speak();
    expect(spoken(container)).toEqual([]);
    // The loading cover that follows holds it too, and its own sentence is not overwritten.
    expect(status(container)).toBe("Loading terminal…");
    expect(recovered(container)).toEqual([]);
    // However long the loading cover stays, the wait of attended time has not begun.
    advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
    expect(recovered(container)).toEqual([]);
    FakeController.last.settle("open");
    advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
    expect(recovered(container)).toEqual([]);
    FakeController.last.paint();
    advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
    expect(recovered(container)).toEqual([]);
    advance(1);
    expect(recovered(container)).toEqual([RECOVERED]);
    // Already said, then pending again and clear: said no more.
    advance(READ_TIME_MS);
    render(root, { ...session, screen_inexact: true, holder_link: "pending" });
    render(root, { ...session, screen_inexact: true });
    advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
    expect(recovered(container)).toEqual([]);
  });

  it("does not lose a recovered display queued but not yet said when the session turns pending, and says it once after the covers lift", () => {
    const { container, root } = mount();
    speak();
    render(root, { ...session, screen_inexact: false });
    render(root, { ...session, screen_inexact: true });
    advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
    expect(recovered(container)).toEqual([]);
    render(root, { ...pending, screen_inexact: true });
    advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
    expect(recovered(container)).toEqual([]);
    render(root, { ...session, screen_inexact: true });
    speak();
    FakeController.last.settle("open");
    advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
    expect(recovered(container)).toEqual([]);
    FakeController.last.paint();
    advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
    expect(recovered(container)).toEqual([]);
    advance(1);
    expect(recovered(container)).toEqual([RECOVERED]);
    // Said once: after it is read away and a further wait, nothing comes again.
    advance(READ_TIME_MS);
    expect(recovered(container)).toEqual([]);
    advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
    expect(recovered(container)).toEqual([]);
  });

  it("does not lose a recovered display flagged while the user was away when the session turns pending, and says it once after the user is back", () => {
    const setAttended = attention;
    const { container, root } = mount();
    speak();
    render(root, { ...session, screen_inexact: false });
    setAttended(false);
    render(root, { ...session, screen_inexact: true });
    advance(AFTER_FOCUS_MOVE_DELAY_MS * 5);
    render(root, { ...pending, screen_inexact: true });
    render(root, { ...session, screen_inexact: true });
    speak();
    FakeController.last.settle("open");
    FakeController.last.paint();
    advance(AFTER_FOCUS_MOVE_DELAY_MS * 5);
    expect(recovered(container)).toEqual([]);
    setAttended(true);
    advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
    expect(recovered(container)).toEqual([]);
    advance(1);
    expect(recovered(container)).toEqual([RECOVERED]);
    advance(READ_TIME_MS + AFTER_FOCUS_MOVE_DELAY_MS * 3);
    expect(recovered(container)).toEqual([]);
  });

  it("follows a switch of the selected session during pending and recovery", () => {
    const { container, root } = mount();
    focusTerminalInput(container);
    render(root, pending);
    const other = sessionOf("s-2", "c-1", undefined, "Other", "working");
    render(root, other);
    expect(region(container)).toBeNull();
    expect(FakeController.last.held).toBe(false);
    expect(terminal(container).hasAttribute("inert")).toBe(false);
    // The pick is the user's: the pane's own attach takes focus, not the recovery's background one.
    expect(FakeController.last.lastUserInitiated).toBe(true);
    render(root, { ...other, holder_link: "pending" });
    expect(region(container)).not.toBeNull();
    render(root, { ...pending, id: "s-3" });
    expect(region(container)).not.toBeNull();
    expect(FakeController.last.held).toBe(true);
  });

  it("covers an account switch's session with the pending cover, and uncovers it to the switch's", () => {
    const { container, root } = mount();
    render(root, { ...pending, switching_account: true });
    expect(region(container)).not.toBeNull();
    render(root, { ...session, switching_account: true });
    speak();
    expect(status(container)).toBe("Switching account");
  });
});

describe("the pane's sentences while the agent area is inert", () => {
  const pending = { ...session, holder_link: "pending" as const };
  const SENTENCE = "Reconnecting to the session. The session process is still running. Input is unavailable until the connection returns. Reconnection is automatic.";
  const interrupted = sessionOf("s-1", "c-1", undefined, "Work", "interrupted");
  /** The status regions holding exactly `text`. */
  const said = (container: HTMLElement, text: string) => statusRegions(container).filter((region) => region.textContent === text);
  const spokenText = (container: HTMLElement) => statusRegions(container).map((region) => region.textContent).filter(Boolean);

  /** A pane opened on `current`, the status regions past their first write delay. */
  function openOn(current: typeof session, exposed = true) {
    const container = document.body.appendChild(document.createElement("div"));
    const root = createRoot(container);
    roots.push(root);
    render(root, current, exposed);
    speak();
    return { container, root };
  }

  /** Records every change made to the nodes under `container` from now on. */
  function watchWrites(container: HTMLElement) {
    const observer = new MutationObserver(() => {});
    observer.observe(container, { childList: true, characterData: true, subtree: true });
    onTestFinished(() => observer.disconnect());
    return () => observer.takeRecords().length;
  }

  describe("the pending sentences when exposure lands focus on the cover", () => {
    const focusCover = (container: HTMLElement) => act(() => container.querySelector<HTMLElement>("[role=region][aria-label]")!.focus());

    it("does not write the immediate sentence held while inert when focus is on the cover, which reads the same copy", () => {
      const { container, root } = openOn(session);
      render(root, pending, false);
      render(root, pending);
      focusCover(container);
      advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
      expect(said(container, SENTENCE)).toEqual([]);
      // Not queued again by a later exposure.
      render(root, pending, false);
      render(root, pending);
      advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
      expect(said(container, SENTENCE)).toEqual([]);
    });

    it("does not write the delayed sentence held while inert when focus is on the cover", () => {
      const { container, root } = openOn(pending, false);
      advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
      render(root, pending);
      focusCover(container);
      advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
      expect(said(container, SENTENCE)).toEqual([]);
    });
  });

  describe("the cover's sentence", () => {
    it("holds the sentence of a session that turns pending while inert, and says it once, after the focus-move wait of exposure", () => {
      const { container, root } = openOn(session);
      render(root, pending, false);
      advance(READ_TIME_MS + AFTER_FOCUS_MOVE_DELAY_MS * 3);
      expect(said(container, SENTENCE)).toEqual([]);
      render(root, pending);
      advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
      expect(said(container, SENTENCE)).toEqual([]);
      advance(1);
      expect(said(container, SENTENCE)).toHaveLength(1);
      expect(statusRegions(container)[0].textContent).toBe(SENTENCE);
    });

    it("does not say it again when the area is made inert and exposed again", () => {
      const { container, root } = openOn(session);
      render(root, pending, false);
      render(root, pending);
      advance(AFTER_FOCUS_MOVE_DELAY_MS);
      expect(said(container, SENTENCE)).toHaveLength(1);
      const writes = watchWrites(container);
      render(root, pending, false);
      advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
      render(root, pending);
      advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
      expect(said(container, SENTENCE)).toHaveLength(1);
      expect(writes()).toBe(0);
    });

    it("leaves a sentence already written in its region, neither cleared nor written again, as the area goes inert and back", () => {
      const { container, root } = openOn(session);
      render(root, pending);
      expect(said(container, SENTENCE)).toHaveLength(1);
      const writes = watchWrites(container);
      render(root, pending, false);
      expect(said(container, SENTENCE)).toHaveLength(1);
      advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
      expect(said(container, SENTENCE)).toHaveLength(1);
      render(root, pending);
      advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
      expect(said(container, SENTENCE)).toHaveLength(1);
      expect(writes()).toBe(0);
    });

    it("does not say a pending sentence that went stale before the area was exposed", () => {
      const { container, root } = openOn(session);
      render(root, pending, false);
      render(root, session, false);
      render(root, session);
      advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
      expect(said(container, SENTENCE)).toEqual([]);
    });

    it("holds the loading cover's sentence of a pane opened inert until the area is exposed", () => {
      const { container, root } = openOn(session, false);
      advance(READ_TIME_MS + AFTER_FOCUS_MOVE_DELAY_MS * 3);
      expect(status(container)).toBe("");
      render(root, session);
      advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
      expect(status(container)).toBe("");
      advance(1);
      expect(status(container)).toBe("Loading terminal…");
    });

    it("holds a disconnection that comes while inert, and says the state current at exposure", () => {
      const { container, root } = openOn(session);
      render(root, session, false);
      for (let i = 0; i < 5; i++) failAttempt();
      FakeController.last.settle("closed");
      advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
      expect(status(container)).not.toBe("Terminal disconnected");
      render(root, session);
      advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
      expect(status(container)).not.toBe("Terminal disconnected");
      advance(1);
      expect(status(container)).toBe("Terminal disconnected");
    });

    it("keeps saying a cover's changes at once while the area stays exposed", () => {
      const { container } = openOn(session);
      FakeController.last.settle("closed");
      expect(status(container)).toBe("Reconnecting the terminal…");
    });
  });

  describe("the delayed pending sentence", () => {
    it("holds a sentence due while inert, and says it once, after the focus-move wait of exposure", () => {
      const { container, root } = openOn(pending, false);
      advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
      expect(said(container, SENTENCE)).toEqual([]);
      render(root, pending);
      advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
      expect(said(container, SENTENCE)).toEqual([]);
      advance(1);
      expect(said(container, SENTENCE)).toHaveLength(1);
      expect(said(container, SENTENCE)[0].closest("[data-live-announcer=true]")).not.toBeNull();
    });

    it("does not say it again when the area is made inert and exposed again", () => {
      const { container, root } = openOn(pending, false);
      advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
      render(root, pending);
      advance(AFTER_FOCUS_MOVE_DELAY_MS);
      expect(said(container, SENTENCE)).toHaveLength(1);
      const writes = watchWrites(container);
      render(root, pending, false);
      advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
      render(root, pending);
      advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
      expect(said(container, SENTENCE)).toHaveLength(1);
      expect(writes()).toBe(0);
    });

    it("leaves a sentence already written in its region as the area goes inert and back", () => {
      const { container, root } = openOn(pending);
      advance(AFTER_FOCUS_MOVE_DELAY_MS);
      expect(said(container, SENTENCE)).toHaveLength(1);
      const writes = watchWrites(container);
      render(root, pending, false);
      advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
      expect(said(container, SENTENCE)).toHaveLength(1);
      render(root, pending);
      advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
      expect(said(container, SENTENCE)).toHaveLength(1);
      expect(writes()).toBe(0);
    });

    it("does not say a sentence that went stale while inert, when the pending ended before exposure", () => {
      const { container, root } = openOn(pending, false);
      advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
      render(root, session, false);
      render(root, session);
      advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
      expect(said(container, SENTENCE)).toEqual([]);
    });
  });

  describe("the no-process bar's sentence", () => {
    it("holds it while inert and writes it once the area is exposed and the focus move has been spoken", () => {
      const { container, root } = openOn(interrupted, false);
      advance(READ_TIME_MS + AFTER_FOCUS_MOVE_DELAY_MS * 3);
      expect(spokenText(container)).toEqual([]);
      render(root, interrupted);
      advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
      expect(spokenText(container)).toEqual([]);
      advance(1);
      expect(spokenText(container)).toEqual(["Not running"]);
    });

    it("holds an archived session's sentence the same way", async () => {
      request.mockResolvedValueOnce({ type: "session_history", id: null, messages: [{ speaker: "user", text: "Hello", turn: 1 }] } as never);
      const { container, show } = mountArchived({ exposed: false });
      await act(async () => {});
      advance(READ_TIME_MS + AFTER_FOCUS_MOVE_DELAY_MS * 3);
      expect(spokenText(container)).toEqual([]);
      show({ exposed: true });
      advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
      expect(spokenText(container)).toEqual([]);
      advance(1);
      expect(spokenText(container)).toEqual(["This session is archived"]);
    });

    it("holds the sentences while the archive view covers the pane, with no say of the window's `exposed`", async () => {
      request.mockResolvedValueOnce({ type: "session_history", id: null, messages: [{ speaker: "user", text: "Hello", turn: 1 }] } as never);
      const { container, show } = mountArchived({ behindArchive: true });
      await act(async () => {});
      advance(READ_TIME_MS + AFTER_FOCUS_MOVE_DELAY_MS * 3);
      expect(spokenText(container)).toEqual([]);
      show({ behindArchive: false });
      advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
      expect(spokenText(container)).toEqual([]);
      advance(1);
      expect(spokenText(container)).toEqual(["This session is archived"]);
    });

    it("holds a no-process bar's sentence the same way while the archive view covers the pane", () => {
      const container = document.body.appendChild(document.createElement("div"));
      const root = createRoot(container);
      roots.push(root);
      const show = (behindArchive: boolean) => act(() => root.render(host(<TerminalPane session={interrupted} behindArchive={behindArchive} onResume={async () => true} onAddAgent={() => {}} />)));
      show(true);
      speak();
      advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
      expect(spokenText(container)).toEqual([]);
      show(false);
      advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
      expect(spokenText(container)).toEqual([]);
      advance(1);
      expect(spokenText(container)).toEqual(["Not running"]);
    });

    it("keeps a sentence already written in its region as the area goes inert and back", () => {
      const { container, root } = openOn(interrupted);
      expect(spokenText(container)).toEqual(["Not running"]);
      const writes = watchWrites(container);
      render(root, interrupted, false);
      advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
      expect(spokenText(container)).toEqual(["Not running"]);
      render(root, interrupted);
      advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
      expect(spokenText(container)).toEqual(["Not running"]);
      expect(writes()).toBe(0);
    });

    it("keeps holding it after a modal closes while the area is still inert, and writes it once both are clear", () => {
      const container = document.body.appendChild(document.createElement("div"));
      const root = createRoot(container);
      roots.push(root);
      const show = (modalOpen: boolean, exposed: boolean) =>
        act(() => root.render(host(<TerminalPane session={interrupted} modalOpen={modalOpen} exposed={exposed} onResume={async () => true} onAddAgent={() => {}} />)));
      show(true, false);
      speak();
      show(false, false);
      advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
      expect(spokenText(container)).toEqual([]);
      show(false, true);
      advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
      expect(spokenText(container)).toEqual([]);
      advance(1);
      expect(spokenText(container)).toEqual(["Not running"]);
    });
  });
});

describe("the terminal's focus ring", () => {
  const surface = (container: HTMLElement) => container.querySelector<HTMLElement>("[data-region=terminal]")!;
  const ringed = (container: HTMLElement) => surface(container).hasAttribute("data-focus-visible");
  const modality = (next: "keyboard" | "pointer") => act(() => setInteractionModality(next));
  const key = (target: Element, name: string) =>
    act(() => {
      target.dispatchEvent(new KeyboardEvent("keydown", { key: name, bubbles: true }));
      target.dispatchEvent(new KeyboardEvent("keyup", { key: name, bubbles: true }));
    });
  /** A pane past its cover, with focus outside it and the modality restored after the test. */
  function mountUncovered() {
    withModality(getInteractionModality());
    const outside = document.body.appendChild(document.createElement("button"));
    const { container, root } = mount();
    FakeController.last.settle("open");
    FakeController.last.paint();
    act(() => outside.focus());
    return { container, root, outside, input: terminalInput(container) };
  }

  it("is drawn when focus arrives in the terminal by keyboard, and goes when focus leaves", () => {
    const { container, outside, input } = mountUncovered();
    modality("keyboard");
    act(() => input.focus());
    expect(ringed(container)).toBe(true);
    act(() => outside.focus());
    expect(ringed(container)).toBe(false);
  });

  it("is not drawn when focus arrives after a click, nor when Escape, Tab or a letter is typed in the terminal then", () => {
    const { container, input } = mountUncovered();
    modality("pointer");
    act(() => input.focus());
    expect(ringed(container)).toBe(false);
    for (const name of ["Escape", "Tab", "a"]) {
      key(input, name);
      expect(ringed(container)).toBe(false);
    }
  });

  it("is drawn again when focus comes back by keyboard after a click", () => {
    const { container, outside, input } = mountUncovered();
    modality("pointer");
    act(() => input.focus());
    act(() => outside.focus());
    modality("keyboard");
    act(() => input.focus());
    expect(ringed(container)).toBe(true);
  });

  it.each([
    ["pointer", false],
    ["keyboard", true],
  ] as const)("is drawn only after a key when recovery hands focus from the pending cover back to the input, after a %s had put focus there", (by, drawn) => {
    const { container, root, input } = mountUncovered();
    withModality(by);
    act(() => input.focus());
    if (by === "pointer") expect(ringed(container)).toBe(false);
    render(root, { ...session, holder_link: "pending" });
    expect(document.activeElement).toBe(container.querySelector("[role=region][aria-label]"));
    const focuses = FakeController.last.focuses;
    render(root, session);
    FakeController.last.settle("open");
    FakeController.last.paint();
    // The handoff asked the controller to focus xterm's input; the fake one moves no focus, so do what xterm would.
    expect(FakeController.last.focuses).toBeGreaterThan(focuses);
    act(() => input.focus());
    expect(ringed(container)).toBe(drawn);
  });

  it("is not applied while the loading cover hides the terminal, and is once the first paint has uncovered it", () => {
    const { container } = mount();
    expect(surface(container).className).not.toMatch(/ring-2/);
    FakeController.last.settle("open");
    expect(surface(container).className).not.toMatch(/ring-2/);
    FakeController.last.paint();
    expect(surface(container).className).toMatch(/ring-2/);
  });

  it("is not applied while the reconnecting or the disconnected cover hides the terminal", () => {
    const { container } = mountUncovered();
    expect(surface(container).className).toMatch(/ring-2/);
    FakeController.last.settle("closed");
    expect(status(container) ?? "").not.toBe("Loading terminal…");
    expect(surface(container).className).not.toMatch(/ring-2/);
    for (let i = 0; i < 5; i++) failAttempt();
    FakeController.last.settle("closed");
    expect(surface(container).className).not.toMatch(/ring-2/);
  });

  it("is not applied while the pending cover hides the terminal, and is again once it has cleared and painted", () => {
    const { container, root } = mountUncovered();
    render(root, { ...session, holder_link: "pending" });
    expect(surface(container).className).not.toMatch(/ring-2/);
    render(root, session);
    FakeController.last.settle("open");
    FakeController.last.paint();
    expect(surface(container).className).toMatch(/ring-2/);
  });
});
