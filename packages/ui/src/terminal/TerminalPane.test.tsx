// @vitest-environment jsdom
import { act, createRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { sessionOf } from "../gallery/fixtures/builders";
import { currentLanguage, setLanguage } from "../i18n/language";
import type { TermStatus } from "./TerminalController";
import { TerminalPane, type TerminalPaneHandle } from "./TerminalPane";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

class StubResizeObserver {
  observe(): void {}
  disconnect(): void {}
}
globalThis.ResizeObserver ??= StubResizeObserver as unknown as typeof ResizeObserver;

/** A controller whose socket the test settles by hand: `attach` leaves it `connecting`. */
const { FakeController } = vi.hoisted(() => {
  class FakeController {
    static last: FakeController;
    currentSessionId: string | undefined;
    currentStatus: TermStatus = "closed";
    constructor(private callbacks: { onStatusChange: (status: TermStatus) => void }) {
      FakeController.last = this;
    }
    mount(): void {}
    inputLabel: string | undefined;
    setInputLabel(label: string): void {
      this.inputLabel = label;
    }
    fit(): void {}
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
    detach(): void {}
    armWake(): void {}
    disarmWake(): void {}
    settle(status: TermStatus): void {
      this.currentStatus = status;
      act(() => this.callbacks.onStatusChange(status));
    }
  }
  return { FakeController };
});
vi.mock("./TerminalController", () => ({ TerminalController: FakeController }));
// One object for every render: the pane re-attaches when `terminalUrl` changes identity.
const daemon = vi.hoisted(() => ({ terminalUrl: () => "" }));
const store = vi.hoisted(() => ({ snapshotEpoch: 0, connectionState: "open" }));
vi.mock("../store", () => ({
  useDaemon: () => daemon,
  useDaemonStore: (select: (state: typeof store) => unknown) => select(store),
}));
vi.mock("../theme", () => ({ useOctoboardTheme: () => ({ resolved: "light" }) }));

const roots: Root[] = [];
beforeEach(() => {
  vi.useFakeTimers();
  store.snapshotEpoch = 0;
  store.connectionState = "open";
});
afterEach(() => {
  vi.useRealTimers();
  for (const root of roots.splice(0)) act(() => root.unmount());
  document.body.replaceChildren();
});

const session = sessionOf("s-1", "c-1", undefined, "Work", "working");

function render(root: Root, current = session) {
  act(() => root.render(<TerminalPane session={current} onResume={async () => true} />));
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

const status = (container: HTMLElement) => container.querySelector(".sr-only")?.textContent;

it("keeps the loading cover up past the limit while the first connect hangs", () => {
  const { container } = mount();
  act(() => void vi.advanceTimersByTime(60_000));
  expect(status(container)).toBe("Loading terminal…");
});

it("keeps the cover up past the loading limit while the connection is failing", () => {
  const { container } = mount();
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
  expect(container.querySelector("button")?.textContent).toBe("Reconnect");
});

it("reconnects, with the attempts reset, when Reconnect is pressed", () => {
  const { container } = mount();
  for (let i = 0; i < 5; i++) failAttempt();
  FakeController.last.settle("closed");
  const attaches = FakeController.last.attaches;
  act(() => container.querySelector("button")?.click());
  expect(FakeController.last.attaches).toBe(attaches + 1);
  expect(container.querySelector("button")).toBeNull();
  expect(container.querySelector("[data-slot=spinner]")).not.toBeNull();
});

it("offers no Reconnect button while the daemon connection is not open", () => {
  store.connectionState = "closed";
  const { container } = mount();
  for (let i = 0; i < 5; i++) failAttempt();
  FakeController.last.settle("closed");
  expect(status(container)).toBe("Terminal disconnected");
  expect(container.querySelector("button")).toBeNull();
});

it("focuses the Reconnect button, not the terminal, through the handle while it is offered", () => {
  const handle = createRef<TerminalPaneHandle>();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  act(() => root.render(<TerminalPane ref={handle} session={session} onResume={async () => true} />));
  act(() => handle.current?.focus());
  expect(FakeController.last.focuses).toBe(1);
  for (let i = 0; i < 5; i++) failAttempt();
  FakeController.last.settle("closed");
  act(() => handle.current?.focus());
  expect(FakeController.last.focuses).toBe(1);
  expect(document.activeElement).toBe(container.querySelector("button"));
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

it("keeps focus off the covered terminal while no session is selected", () => {
  const topBar = document.body.appendChild(document.createElement("div"));
  topBar.dataset.region = "topbar";
  const topBarButton = topBar.appendChild(document.createElement("button"));
  const elsewhere = document.body.appendChild(document.createElement("button"));
  const handle = createRef<TerminalPaneHandle>();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  act(() => root.render(<TerminalPane ref={handle} onResume={async () => true} />));
  expect(container.querySelector("[inert]")?.contains(container.querySelector("[dir=ltr]"))).toBe(true);

  // Focus asked for goes to the top bar from anywhere but `<body>`, which is left alone so that the
  // refocus at mount does not pull focus at launch.
  act(() => handle.current?.focus());
  expect(document.activeElement).toBe(document.body);
  elsewhere.focus();
  act(() => handle.current?.focus());
  expect(document.activeElement).toBe(topBarButton);
  expect(FakeController.last.focuses).toBe(0);

  act(() => root.render(<TerminalPane ref={handle} session={session} onResume={async () => true} />));
  expect(container.querySelector("[inert]")).toBeNull();
  act(() => handle.current?.focus());
  expect(FakeController.last.focuses).toBe(1);
});

it("hands focus to the top bar when the session goes away while the terminal holds it", () => {
  const topBar = document.body.appendChild(document.createElement("div"));
  topBar.dataset.region = "topbar";
  const topBarButton = topBar.appendChild(document.createElement("button"));
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  act(() => root.render(<TerminalPane session={session} onResume={async () => true} />));
  const input = container.querySelector("[dir=ltr]")!.appendChild(document.createElement("textarea"));
  input.focus();
  act(() => root.render(<TerminalPane onResume={async () => true} />));
  expect(document.activeElement).toBe(topBarButton);
});
