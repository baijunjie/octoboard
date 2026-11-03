// @vitest-environment jsdom
import { act, createRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { currentLanguage, setLanguage } from "../../i18n/language";
import type { ShellTerminal } from "../../protocol";
import type { Daemon, State } from "../../store";
import { TestHost } from "../../test/TestHost";
import { testDaemon } from "../../test/testDaemon";
import type { TermStatus } from "../TerminalController";
import { MAX_AUTO_RECONNECT_ATTEMPTS } from "../reconnectPolicy";
import { ShellTerminalView, type ShellTerminalViewHandle } from "./ShellTerminalView";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** A controller whose socket the test settles by hand, with an input element of its own. */
const { FakeController } = vi.hoisted(() => {
  class FakeController {
    static last: FakeController;
    term = { textarea: document.createElement("textarea") };
    currentSessionId: string | undefined;
    currentStatus: TermStatus = "closed";
    /** Whether each `attach` was the user's. */
    attaches: boolean[] = [];
    fits = 0;
    inputLabel: string | undefined;
    constructor(private callbacks: { onStatusChange: (status: TermStatus) => void }) {
      FakeController.last = this;
    }
    mount(element: HTMLElement): void {
      element.append(this.term.textarea);
    }
    setInputLabel(label: string): void {
      this.inputLabel = label;
    }
    fit(): void {
      this.fits += 1;
    }
    syncSize(): void {}
    dispose(): void {}
    focus(): void {
      this.term.textarea.focus();
    }
    attach(sessionId: string, _url: string, userInitiated: boolean): void {
      this.attaches.push(userInitiated);
      this.currentSessionId = sessionId;
      this.currentStatus = "connecting";
      this.callbacks.onStatusChange("connecting");
    }
    settle(status: TermStatus): void {
      this.currentStatus = status;
      act(() => this.callbacks.onStatusChange(status));
    }
  }
  return { FakeController };
});
vi.mock("../TerminalController", () => ({ TerminalController: FakeController }));
let daemon: Daemon;
const host = (ui: React.ReactNode) => <TestHost daemon={daemon}>{ui}</TestHost>;
/** Updates the daemon's state the way a broadcast would. */
const setState = (state: Partial<State>) => act(() => daemon.store.setState(state));
vi.mock("../../preferences/theme", () => ({ useOctoboardTheme: () => ({ resolved: "light" }) }));
vi.mock("../TerminalEditMenu", () => ({ TerminalEditMenu: () => null }));

const terminal: ShellTerminal = {
  id: "t", project_id: "p", worktree: null, cwd: "/p", title: "shell", renamed: false, state: "running", busy: false, created_at: 1,
};

const roots: Root[] = [];
beforeEach(() => {
  vi.useFakeTimers();
  // Every automatic attempt follows the last at once, so a test spends them without waiting out the backoff.
  daemon = testDaemon({ state: { connectionState: "open" }, terminalReconnectDelay: () => 0 });
});
afterEach(() => {
  vi.useRealTimers();
  for (const root of roots.splice(0)) act(() => root.unmount());
  document.body.replaceChildren();
});

function mount(shown = true, record: ShellTerminal = terminal) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  const ref = createRef<ShellTerminalViewHandle>();
  const show = (next: boolean) => act(() => root.render(host(<ShellTerminalView terminal={record} shown={next} onClose={() => {}} ref={ref} />)));
  show(shown);
  return { container, ref, show };
}

const landing = (container: HTMLElement) => Array.from(container.querySelectorAll("[data-region-landing]"));

it("lands F6 on the input while the shell is connected", () => {
  const { container } = mount();
  FakeController.last.settle("open");
  expect(landing(container)).toEqual([FakeController.last.term.textarea]);
});

/** Spends the automatic attempts, so the cover offers Reconnect. */
function giveUp(): void {
  for (let i = 0; i < MAX_AUTO_RECONNECT_ATTEMPTS; i++) {
    FakeController.last.settle("closed");
    act(() => void vi.advanceTimersByTime(0));
  }
  FakeController.last.settle("closed");
}

const reconnectOf = (container: HTMLElement): HTMLButtonElement | undefined => Array.from(container.querySelectorAll("button")).find((button) => button.textContent === "Reconnect");

it("lands F6 on the cover's Reconnect, not the input under it, once the connection is given up on", () => {
  const { container, ref } = mount();
  giveUp();
  const reconnect = reconnectOf(container)!;
  expect(landing(container)).toEqual([reconnect]);
  expect(ref.current!.focus()).toBe(true);
  expect(document.activeElement).toBe(reconnect);
  // Connected again: back to the input.
  FakeController.last.settle("open");
  expect(landing(container)).toEqual([FakeController.last.term.textarea]);
});

it("marks no landing in a tab that is not shown, and reports no focus once the view has gone", () => {
  const { container, ref } = mount(false);
  expect(landing(container)).toEqual([]);
  const handle = ref.current!;
  act(() => roots.splice(0).forEach((root) => root.unmount()));
  expect(handle.focus()).toBe(false);
});

it("moves focus from the input to Reconnect as it appears", () => {
  const { container } = mount();
  FakeController.last.settle("open");
  act(() => FakeController.last.term.textarea.focus());
  giveUp();
  expect(document.activeElement).toBe(reconnectOf(container)!);
});

it("leaves focus outside the view alone as Reconnect appears", () => {
  const { container } = mount();
  const outside = document.body.appendChild(document.createElement("button"));
  outside.focus();
  giveUp();
  expect(reconnectOf(container)).toBeDefined();
  expect(document.activeElement).toBe(outside);
});

it("keeps focus in the view when the focused Reconnect goes", () => {
  const { container } = mount();
  giveUp();
  act(() => reconnectOf(container)!.focus());
  FakeController.last.settle("open");
  expect(reconnectOf(container)).toBeUndefined();
  expect(document.activeElement).toBe(FakeController.last.term.textarea);
});

it("reports no focus when it did not land in the view", () => {
  const { ref } = mount();
  vi.spyOn(FakeController.last, "focus").mockImplementation(() => {});
  expect(ref.current!.focus()).toBe(false);
});

const FIRST_WRITE_MS = 1500;
const regions = (container: HTMLElement) => Array.from(container.querySelectorAll<HTMLElement>("[role=status]:not([data-slot=spinner])"));
const spoken = (container: HTMLElement) => regions(container).map((region) => region.textContent).filter(Boolean);

it("mounts an exited shell's status empty and writes the sentence after the first write delay, keeping Close reachable", () => {
  const { container } = mount(true, { ...terminal, state: "exited" });
  expect(regions(container).every((region) => region.textContent === "")).toBe(true);
  const close = Array.from(container.querySelectorAll("button")).find((button) => button.textContent === "Close terminal")!;
  expect(close.closest("[aria-hidden=true]")).toBeNull();
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_MS));
  expect(spoken(container)).toEqual(["The shell has exited"]);
});

it("mounts the trouble status empty and writes it after the first write delay, then follows later changes in place", () => {
  const { container } = mount();
  const before = regions(container);
  FakeController.last.settle("closed");
  expect(spoken(container)).toEqual([]);
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_MS));
  expect(spoken(container)).toEqual(["Reconnecting the terminal…"]);
  expect(regions(container)).toEqual(before);
  FakeController.last.settle("open");
  expect(spoken(container)).toEqual([]);
});

it("announces nothing from a tab that is not shown", () => {
  const { container } = mount(false, { ...terminal, state: "exited" });
  FakeController.last.settle("closed");
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_MS));
  expect(spoken(container)).toEqual([]);
  expect(container.querySelector("button")).toBeNull();
});

it("mounts empty status regions as an exited tab is shown, and writes after the focus-move delay", () => {
  const { container, show } = mount(false, { ...terminal, state: "exited" });
  expect(regions(container)).toEqual([]);
  show(true);
  expect(regions(container).every((region) => region.textContent === "")).toBe(true);
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_MS - 1));
  expect(spoken(container)).toEqual([]);
  act(() => void vi.advanceTimersByTime(1));
  expect(spoken(container)).toEqual(["The shell has exited"]);
  // Hidden again and shown again: a fresh empty region, not the text kept from before.
  show(false);
  show(true);
  expect(spoken(container)).toEqual([]);
});

it("mounts empty status regions as a troubled tab is shown", () => {
  const { container, show } = mount(false);
  FakeController.last.settle("closed");
  show(true);
  expect(spoken(container)).toEqual([]);
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_MS));
  expect(spoken(container)).toEqual(["Reconnecting the terminal…"]);
});

// The delay the test daemon's `terminalReconnectDelay` gives every attempt.
const RECONNECT_DELAY_MS = 0;
const failAttempt = () => {
  FakeController.last.settle("closed");
  act(() => void vi.advanceTimersByTime(RECONNECT_DELAY_MS));
};

it("reconnects a running shell whose socket closed, without moving focus", () => {
  const { container } = mount();
  FakeController.last.settle("open");
  act(() => FakeController.last.term.textarea.focus());
  expect(FakeController.last.attaches).toEqual([false]);
  failAttempt();
  expect(FakeController.last.attaches).toEqual([false, false]);
  FakeController.last.settle("open");
  expect(document.activeElement).toBe(FakeController.last.term.textarea);
  expect(reconnectOf(container)).toBeUndefined();
});

it("does not reconnect an exited shell", () => {
  mount(true, { ...terminal, state: "exited" });
  FakeController.last.settle("closed");
  act(() => void vi.advanceTimersByTime(60_000));
  expect(FakeController.last.attaches).toEqual([false]);
});

it("stops retrying after the automatic attempts, and the user's Reconnect starts the count again", () => {
  const { container } = mount();
  giveUp();
  const attaches = FakeController.last.attaches.length;
  expect(attaches).toBe(1 + MAX_AUTO_RECONNECT_ATTEMPTS);
  act(() => void vi.advanceTimersByTime(60_000));
  expect(FakeController.last.attaches.length).toBe(attaches);
  expect(spoken(container)).toEqual(["Terminal disconnected"]);
  act(() => reconnectOf(container)!.click());
  expect(FakeController.last.attaches.at(-1)).toBe(true);
  // The count is back to 0: a failure is retried automatically again.
  failAttempt();
  expect(FakeController.last.attaches.at(-1)).toBe(false);
  expect(FakeController.last.attaches.length).toBe(attaches + 2);
});

it("does not offer Reconnect while the control connection is down", () => {
  const { container, show } = mount();
  giveUp();
  expect(reconnectOf(container)).toBeDefined();
  setState({ connectionState: "closed" });
  show(true);
  expect(reconnectOf(container)).toBeUndefined();
});

it("recovers a shell that gave up when a fresh snapshot arrives, without taking focus", () => {
  const { container, show } = mount();
  giveUp();
  const attaches = FakeController.last.attaches.length;
  setState({ snapshotEpoch: daemon.store.getState().snapshotEpoch + 1 });
  show(true);
  expect(FakeController.last.attaches.length).toBe(attaches + 1);
  expect(FakeController.last.attaches.at(-1)).toBe(false);
  expect(reconnectOf(container)).toBeUndefined();
});

it("leaves a snapshot alone while the automatic attempts are not spent", () => {
  const { show } = mount();
  FakeController.last.settle("closed");
  const attaches = FakeController.last.attaches.length;
  setState({ snapshotEpoch: daemon.store.getState().snapshotEpoch + 1 });
  show(true);
  expect(FakeController.last.attaches.length).toBe(attaches);
});

/** Mounts with a ResizeObserver whose notifications the test sends. */
function mountObserved(props: { shown: boolean; resizing: boolean }) {
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
  const show = (next: { shown: boolean; resizing: boolean }) => act(() => root.render(host(<ShellTerminalView terminal={terminal} {...next} onClose={() => {}} />)));
  show(props);
  return { show, notify: () => observers.forEach((notify) => notify()) };
}

it("holds its size while the panel eases, then fits once", () => {
  const { show, notify } = mountObserved({ shown: true, resizing: true });
  const fits = FakeController.last.fits;
  notify();
  expect(FakeController.last.fits).toBe(fits);
  show({ shown: true, resizing: false });
  expect(FakeController.last.fits).toBe(fits + 1);
  notify();
  expect(FakeController.last.fits).toBe(fits + 2);
});

it("does not fit a hidden tab as the ease ends", () => {
  const { show } = mountObserved({ shown: false, resizing: true });
  const fits = FakeController.last.fits;
  show({ shown: false, resizing: false });
  expect(FakeController.last.fits).toBe(fits);
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
