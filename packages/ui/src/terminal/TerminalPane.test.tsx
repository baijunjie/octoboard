// @vitest-environment jsdom
import React, { act, createRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { errorAlerts } from "../errorAlerts";
import type { PlatformAdapter } from "../platform";
import { sessionOf } from "../gallery/fixtures/builders";
import { currentLanguage, setLanguage } from "../i18n/language";
import type { TermStatus } from "./TerminalController";
import { TerminalPane, type TerminalPaneHandle } from "./TerminalPane";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** A controller whose socket the test settles by hand: `attach` leaves it `connecting`. */
const { FakeController } = vi.hoisted(() => {
  class FakeController {
    static last: FakeController;
    currentSessionId: string | undefined;
    currentStatus: TermStatus = "closed";
    constructor(private callbacks: {
      onStatusChange: (status: TermStatus) => void;
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
    attach(sessionId: string): void {
      this.attaches += 1;
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
// One object for every render: the pane re-attaches when `terminalUrl` changes identity.
const daemon = vi.hoisted(() => ({ terminalUrl: () => "", request: vi.fn(async (_body: unknown) => ({ type: "ack" })), toastError: vi.fn() }));
const store = vi.hoisted(() => ({ snapshotEpoch: 0, connectionState: "open" }));
vi.mock("../store", () => ({
  useDaemon: () => daemon,
  useDaemonStore: (select: (state: typeof store) => unknown) => select(store),
}));
const platform = vi.hoisted(() => ({ links: undefined as PlatformAdapter["links"] }));
vi.mock("../platform/react", () => ({ usePlatform: () => platform }));
vi.mock("../theme", () => ({ useOctoboardTheme: () => ({ resolved: "light" }) }));

const roots: Root[] = [];
beforeEach(() => {
  vi.useFakeTimers();
  platform.links = undefined;
  store.snapshotEpoch = 0;
  store.connectionState = "open";
});
afterEach(() => {
  vi.useRealTimers();
  for (const root of roots.splice(0)) act(() => root.unmount());
  document.body.replaceChildren();
  while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
});

const session = sessionOf("s-1", "c-1", undefined, "Work", "working");

function render(root: Root, current = session) {
  act(() => root.render(<TerminalPane session={current} onResume={async () => true} onAddAgent={() => {}} />));
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
  act(() => void vi.advanceTimersByTime(8000));
}

const FIRST_WRITE_MS = 100;
/** Lets the status regions pass their first write delay. */
const speak = () => act(() => void vi.advanceTimersByTime(FIRST_WRITE_MS));
/** The cover's spoken text: its region is the first status region the pane renders. */
const status = (container: HTMLElement) => container.querySelector("[role=status]:not([data-slot=spinner])")?.textContent;

it("keeps the loading cover up past the limit while the first connect hangs", () => {
  const { container } = mount();
  act(() => void vi.advanceTimersByTime(60_000));
  expect(status(container)).toBe("Loading terminal…");
});

it("keeps the cover up past the loading limit while the connection is failing", () => {
  const { container } = mount();
  speak();
  expect(status(container)).toBe("Loading terminal…");
  failAttempt();
  expect(status(container)).toBe("Reconnecting the terminal…");
  act(() => void vi.advanceTimersByTime(60_000));
  expect(status(container)).toBe("Reconnecting the terminal…");
  expect(container.querySelector("[data-slot=spinner]")).not.toBeNull();
});

it("still lets a silent, connected terminal go bare after the loading limit", () => {
  const { container } = mount();
  FakeController.last.settle("open");
  act(() => void vi.advanceTimersByTime(10_001));
  expect(status(container)).toBe("");
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
  store.connectionState = "closed";
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
  act(() => root.render(<TerminalPane ref={handle} session={session} onResume={async () => true} onAddAgent={() => {}} />));
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
  store.snapshotEpoch = 1;
  render(root);
  expect(FakeController.last.attaches).toBe(attaches + 1);
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
  const show = (resizing: boolean) => act(() => root.render(<TerminalPane session={session} resizing={resizing} onResume={async () => true} onAddAgent={() => {}} />));
  show(true);
  const fits = FakeController.last.fits;
  observers.forEach((notify) => notify());
  expect(FakeController.last.fits).toBe(fits);
  show(false);
  expect(FakeController.last.fits).toBe(fits + 1);
  observers.forEach((notify) => notify());
  expect(FakeController.last.fits).toBe(fits + 2);
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

/** A pane mounted beside the top bar focus falls back to and a control elsewhere to put focus on:
 * the three cases below all turn on where focus goes while the terminal is empty. `show` renders
 * again with the props it is given, the mount's own where it is given none. */
function mountEmpty(props: Partial<React.ComponentProps<typeof TerminalPane>> = {}) {
  const topBar = document.body.appendChild(document.createElement("div"));
  topBar.dataset.region = "topbar";
  const topBarButton = topBar.appendChild(document.createElement("button"));
  const elsewhere = document.body.appendChild(document.createElement("button"));
  const handle = createRef<TerminalPaneHandle>();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  const show = (next: Partial<React.ComponentProps<typeof TerminalPane>> = props) =>
    act(() => root.render(<TerminalPane ref={handle} onResume={async () => true} onAddAgent={() => {}} {...next} />));
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

it("hands focus to the top bar when the session goes away while the terminal holds it", () => {
  const { container, topBarButton, show } = mountEmpty({ session });
  const input = container.querySelector("[dir=ltr]")!.appendChild(document.createElement("textarea"));
  input.focus();
  show({});
  expect(document.activeElement).toBe(topBarButton);
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
  const button = Array.from(container.querySelectorAll("button")).find((b) => b.textContent === "Add agent…");
  act(() => button!.click());
  expect(onAddAgent).toHaveBeenCalledOnce();
});

it("covers the terminal with the switching state, inert and ahead of a failing connection, while an account switch holds the session", () => {
  const { container, root } = mount();
  FakeController.last.settle("open");
  const terminal = () => container.querySelector("[data-region=terminal] > .isolate")!;
  expect(terminal().hasAttribute("inert")).toBe(false);
  render(root, { ...session, switching_account: true });
  speak();
  expect(status(container)).toBe("Switching account…");
  expect(terminal().hasAttribute("inert")).toBe(true);
  expect(container.querySelector("[data-slot=spinner]")).not.toBeNull();
  FakeController.last.settle("closed");
  expect(status(container)).toBe("Switching account…");
  render(root, session);
  expect(status(container)).not.toBe("Switching account…");
  expect(terminal().hasAttribute("inert")).toBe(false);
});

it("offers no Resume over the cover while an account switch holds an interrupted session", () => {
  const interrupted = sessionOf("s-1", "c-1", undefined, "Work", "interrupted");
  const { container, root } = mount();
  render(root, interrupted);
  expect(container.textContent).toContain("Not running");
  render(root, { ...interrupted, switching_account: true });
  speak();
  expect(status(container)).toBe("Switching account…");
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
  act(() => void vi.advanceTimersByTime(60_000));
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
    message: "Could not open the external link.",
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
  daemon.request.mockClear();
  daemon.request.mockResolvedValueOnce({ type: "project_file", project: "p-1", worktree: "w", path: "src/a.ts", source: { kind: "live", version: "1" }, file: { size: 1, kind: "text", media_type: null, text: "x", data: null } } as never);
  act(() => root.render(<TerminalPane session={inProject} onResume={async () => true} onAddAgent={() => {}} onViewFile={onViewFile} />));
  await act(async () => FakeController.last.openPath({ path: "/r/src/a.ts", line: 3, view: { worktree: "w", path: "src/a.ts" } }));
  expect(onViewFile).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ project: "p-1", worktree: "w", path: "src/a.ts", line: 3 }));
  expect(daemon.request.mock.calls.map(([body]) => (body as { type: string }).type)).toEqual(["read_project_file"]);
  await act(async () => FakeController.last.openPath({ path: "/tmp/b.png" }));
  expect(onViewFile).toHaveBeenCalledOnce();
  expect(daemon.request).toHaveBeenLastCalledWith({ type: "open_path_link", project: "p-1", path: "/tmp/b.png" });
});

const archived = sessionOf("s-old", "c-1", undefined, "Old work", "archived");

function mountArchived(props: Partial<React.ComponentProps<typeof TerminalPane>> = {}) {
  const handle = createRef<TerminalPaneHandle>();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  const show = (next: Partial<React.ComponentProps<typeof TerminalPane>> = props) =>
    act(() => root.render(<TerminalPane ref={handle} session={archived} onResume={async () => true} onAddAgent={() => {}} {...next} />));
  show();
  return { container, handle, show };
}

it("shows an archived session's history over an inert terminal", async () => {
  const { container } = mountArchived();
  await act(async () => {});
  expect(daemon.request).toHaveBeenCalledWith({ type: "read_session_history", session: "s-old" });
  expect(container.querySelector("section h2")?.textContent).toBe("Session history");
  expect(container.querySelector("[inert]")?.contains(container.querySelector("[dir=ltr]"))).toBe(true);
  expect(FakeController.last.attaches).toBe(0);
  expect(FakeController.last.detaches).toEqual([{}]);
  expect(container.textContent).toContain("This session is archived");
});

it("keeps an interrupted session's saved screen and shows no history for it", () => {
  const { container } = mountArchived({ session: sessionOf("s-int", "c-1", undefined, "Stopped", "interrupted") });
  expect(container.querySelector("section")).toBeNull();
  expect(FakeController.last.detaches).toEqual([{ keepScreenFor: "s-int", savedOutputUrl: "" }]);
});

it("puts focus asked for the terminal on the history's scrolling region when it is archived", async () => {
  daemon.request.mockResolvedValueOnce({ type: "session_history", messages: [{ speaker: "user", text: "Hello", turn: 1 }] } as { type: string });
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
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_MS));
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

it("reaches the history's first control through the handle while it is still loading", () => {
  daemon.request.mockImplementationOnce(() => new Promise(() => {}));
  const { container, handle } = mountArchived();
  act(() => handle.current?.focus());
  expect(document.activeElement).toBe(container.querySelector("section input"));
});

it("falls back to the Reopen button when the history has no control to focus", () => {
  daemon.request.mockImplementationOnce(() => new Promise(() => {}));
  const { container, handle } = mountArchived();
  container.querySelector("section")!.replaceChildren();
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
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_MS - 1));
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
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_MS));
  expect(switching.textContent).toBe("Switching account…");
  const dormant = mountArchived();
  dormant.show({ resuming: true });
  expect(statusRegions(dormant.container).map((region) => region.textContent)).toEqual(["", "", ""]);
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_MS));
  expect(statusRegions(dormant.container).map((region) => region.textContent)).toContain("Resuming session…");
});

it("speaks a dormant bar's sentence once, from a region mounted empty, and leaves the button reachable", () => {
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
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_MS));
  expect(spoken().filter((text) => text === "Not running")).toHaveLength(1);
  // Resume's own cover takes the bar's place without a second copy of the bar's sentence.
  render(root, { ...interrupted, switching_account: true });
  expect(spoken().filter((text) => text === "Not running")).toHaveLength(0);
});

it("speaks an archived bar's sentence and leaves the history's own announcer to its own states", async () => {
  daemon.request.mockResolvedValueOnce({ type: "session_history", messages: [{ speaker: "user", text: "Hello", turn: 1 }] } as { type: string });
  const { container } = mountArchived();
  await act(async () => {});
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_MS));
  const spoken = statusRegions(container).map((region) => region.textContent).filter(Boolean);
  expect(spoken).toEqual(["This session is archived"]);
});

it("holds the dormant sentence while a modal is open and writes it after the close and the focus move", () => {
  const interrupted = sessionOf("s-1", "c-1", undefined, "Work", "interrupted");
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  const show = (modalOpen: boolean) => act(() => root.render(<TerminalPane session={interrupted} modalOpen={modalOpen} onResume={async () => true} onAddAgent={() => {}} />));
  const spoken = () => statusRegions(container).map((region) => region.textContent).filter(Boolean);
  show(true);
  speak();
  expect(spoken()).toEqual([]);
  show(false);
  act(() => void vi.advanceTimersByTime(1499));
  expect(spoken()).toEqual([]);
  act(() => void vi.advanceTimersByTime(1));
  expect(spoken()).toEqual(["Not running"]);
});

it("keeps a sentence already spoken in the region while a modal opens and closes over it", () => {
  const interrupted = sessionOf("s-1", "c-1", undefined, "Work", "interrupted");
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  const show = (modalOpen: boolean) => act(() => root.render(<TerminalPane session={interrupted} modalOpen={modalOpen} onResume={async () => true} onAddAgent={() => {}} />));
  show(false);
  speak();
  const region = () => statusRegions(container).find((node) => node.textContent === "Not running");
  expect(region()).toBeDefined();
  const text = () => statusRegions(container).map((node) => node.textContent).filter(Boolean);
  show(true);
  expect(text()).toEqual(["Not running"]);
  act(() => void vi.advanceTimersByTime(500));
  expect(text()).toEqual(["Not running"]);
  show(false);
  expect(text()).toEqual(["Not running"]);
  act(() => void vi.advanceTimersByTime(3000));
  expect(text()).toEqual(["Not running"]);
});
