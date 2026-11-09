// @vitest-environment jsdom
import { act, createRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { currentLanguage, setLanguage } from "../../i18n/language";
import type { ShellTerminal } from "../../protocol";
import type { Daemon, State } from "../../store";
import { blurOnRemoval } from "../../test/blurOnRemoval";
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
    held = false;
    holdInput(held: boolean): void {
      this.held = held;
    }
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
  let current = record;
  let isShown = shown;
  const draw = () => act(() => root.render(host(<ShellTerminalView terminal={current} shown={isShown} onClose={() => {}} ref={ref} />)));
  const show = (next: boolean) => {
    isShown = next;
    draw();
  };
  /** The daemon's record of the terminal changing under the open view. */
  const update = (next: ShellTerminal) => {
    current = next;
    draw();
  };
  draw();
  return { container, ref, show, update };
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

it("says the recovered display only when the flag turns on while the tab is shown, not on first sight", () => {
  const { container, update } = mount(true, { ...terminal, screen_inexact: false });
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_MS));
  expect(spoken(container)).toEqual([]);
  update({ ...terminal, screen_inexact: true });
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_MS));
  expect(spoken(container)).toEqual(["Recovered terminal display"]);
  // A redraw with the flag still on does not say it again.
  update({ ...terminal, screen_inexact: true, title: "renamed" });
  expect(spoken(container)).toEqual(["Recovered terminal display"]);
  update({ ...terminal, screen_inexact: false });
  expect(spoken(container)).toEqual([]);
});

it("says nothing for a flag already on as the view opens, as its tab is shown later, or from a hidden tab", () => {
  const first = mount(true, { ...terminal, screen_inexact: true });
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_MS));
  expect(spoken(first.container)).toEqual([]);
  const hidden = mount(false, { ...terminal, screen_inexact: false });
  hidden.update({ ...terminal, screen_inexact: true });
  hidden.show(true);
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_MS));
  expect(spoken(hidden.container)).toEqual([]);
});

const SHELL_TITLE = "Reconnecting to the terminal…";
const SHELL_BODY = "The shell process is still running. Input is unavailable until the connection returns. Reconnection is automatic.";
const SHELL_SENTENCE = "Reconnecting to the terminal. The shell process is still running. Input is unavailable until the connection returns. Reconnection is automatic.";
const pendingShell: ShellTerminal = { ...terminal, holder_link: "pending" };
const SHELL_NAME = "shell";
const shellRegion = (container: HTMLElement) => container.querySelector<HTMLElement>("[role=region][aria-label]");
const inertRoot = (container: HTMLElement) => container.querySelector("#shell-terminal-t > .isolate")!;
const sentenceSpoken = (container: HTMLElement) => regions(container).filter((region) => region.textContent === SHELL_SENTENCE);

it("covers a shell opened pending, attaches nothing, holds its input and keeps its state running", () => {
  const { container } = mount(true, pendingShell);
  expect(FakeController.last.attaches).toEqual([]);
  expect(FakeController.last.held).toBe(true);
  expect(container.querySelectorAll("[role=region][aria-label]")).toHaveLength(1);
  expect(shellRegion(container)!.textContent).toBe(`${SHELL_TITLE}${SHELL_BODY}`);
  // Named by the terminal, not by the title it holds, which VoiceOver would read twice.
  expect(shellRegion(container)!.getAttribute("aria-label")).toBe(SHELL_NAME);
  expect(shellRegion(container)!.hasAttribute("aria-labelledby")).toBe(false);
  expect(shellRegion(container)!.getAttribute("aria-describedby")!.split(" ").map((id) => document.getElementById(id)!.textContent)).toEqual([SHELL_TITLE, SHELL_BODY]);
  expect(inertRoot(container).hasAttribute("inert")).toBe(true);
  expect(container.querySelector("button")).toBeNull();
  expect(container.textContent).not.toContain("The shell has exited");
  act(() => void vi.advanceTimersByTime(600_000));
  expect(FakeController.last.attaches).toEqual([]);
});

it("covers an open shell at once and spends no retry while the socket drops, landing F6 on the cover", () => {
  const { container, ref, update } = mount();
  FakeController.last.settle("open");
  update(pendingShell);
  expect(shellRegion(container)).not.toBeNull();
  expect(FakeController.last.held).toBe(true);
  expect(landing(container)).toEqual([shellRegion(container)]);
  expect(ref.current!.focus()).toBe(true);
  expect(document.activeElement).toBe(shellRegion(container));
  giveUp();
  act(() => void vi.advanceTimersByTime(600_000));
  expect(FakeController.last.attaches).toEqual([false]);
  expect(reconnectOf(container)).toBeUndefined();
  expect(shellRegion(container)!.textContent).toBe(`${SHELL_TITLE}${SHELL_BODY}`);
});

it("moves focus held in the shell to the cover, and back on recovery only while it is still there", () => {
  const { container, update } = mount();
  FakeController.last.settle("open");
  act(() => FakeController.last.term.textarea.focus());
  update(pendingShell);
  expect(document.activeElement).toBe(shellRegion(container));
  update(terminal);
  expect(document.activeElement).toBe(FakeController.last.term.textarea);
  // The person was at the cover, so the recovery attach is theirs.
  expect(FakeController.last.attaches.at(-1)).toBe(true);
  // Focus that went elsewhere stays there, and the attach is in the background.
  const outside = document.body.appendChild(document.createElement("button"));
  update(pendingShell);
  act(() => outside.focus());
  update(terminal);
  expect(document.activeElement).toBe(outside);
  expect(FakeController.last.attaches.at(-1)).toBe(false);
});

it("does not move focus into the input on recovery after a tap", () => {
  const topBar = document.body.appendChild(document.createElement("div"));
  topBar.dataset.region = "topbar";
  const topBarButton = topBar.appendChild(document.createElement("button"));
  const { container, update } = mount();
  FakeController.last.settle("open");
  act(() => FakeController.last.term.textarea.focus());
  update(pendingShell);
  expect(document.activeElement).toBe(shellRegion(container));
  document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "touch" }));
  update(terminal);
  expect(document.activeElement).toBe(topBarButton);
  expect(FakeController.last.attaches.at(-1)).toBe(true);
  document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "mouse" }));
});

it("sends focus to the panel's first control that takes no text after a tap, as Chromium fires focusout on removal", () => {
  const { container, update } = mount();
  const header = container.insertBefore(document.createElement("button"), container.firstChild);
  container.dataset.panelCard = "";
  FakeController.last.settle("open");
  act(() => FakeController.last.term.textarea.focus());
  update(pendingShell);
  document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "touch" }));
  blurOnRemoval();
  update(terminal);
  expect(document.activeElement).toBe(header);
  expect(FakeController.last.attaches.at(-1)).toBe(true);
  document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "mouse" }));
});

it("returns focus to the input and activates on recovery when Chromium fires focusout on removal", () => {
  const { container, ref, update } = mount();
  FakeController.last.settle("open");
  update(pendingShell);
  act(() => void ref.current!.focus());
  expect(document.activeElement).toBe(shellRegion(container));
  blurOnRemoval();
  update(terminal);
  expect(document.activeElement).toBe(FakeController.last.term.textarea);
  expect(FakeController.last.attaches.at(-1)).toBe(true);
});

it("leaves focus on <body> and attaches in the background when the cover was clicked away from onto nothing", async () => {
  const { container, ref, update } = mount();
  FakeController.last.settle("open");
  update(pendingShell);
  act(() => void ref.current!.focus());
  act(() => shellRegion(container)!.blur());
  await Promise.resolve();
  expect(document.activeElement).toBe(document.body);
  update(terminal);
  expect(document.activeElement).toBe(document.body);
  expect(FakeController.last.attaches.at(-1)).toBe(false);
});

it("replaces a Reconnect that was offered, handing the focus it held to the cover", () => {
  const { container, update } = mount();
  giveUp();
  act(() => reconnectOf(container)!.focus());
  update(pendingShell);
  expect(reconnectOf(container)).toBeUndefined();
  expect(document.activeElement).toBe(shellRegion(container));
});

it("attaches once in the background with a fresh budget when pending clears, even after the attempts were spent", () => {
  const { container, update } = mount();
  giveUp();
  update(pendingShell);
  act(() => void vi.advanceTimersByTime(600_000));
  update(terminal);
  expect(FakeController.last.attaches).toHaveLength(MAX_AUTO_RECONNECT_ATTEMPTS + 1 + 1);
  expect(FakeController.last.attaches.at(-1)).toBe(false);
  expect(shellRegion(container)).toBeNull();
  // A drop now is retried again.
  const attached = FakeController.last.attaches.length;
  FakeController.last.settle("closed");
  act(() => void vi.advanceTimersByTime(RECONNECT_DELAY_MS));
  expect(FakeController.last.attaches).toHaveLength(attached + 1);
});

it("attaches a shell opened pending only once it clears", () => {
  const { update } = mount(true, pendingShell);
  update(terminal);
  expect(FakeController.last.attaches).toEqual([false]);
  expect(FakeController.last.held).toBe(false);
});

it("shows the exit notice, and attaches nothing, when a pending shell exits instead", () => {
  const { container, update } = mount();
  FakeController.last.settle("open");
  update(pendingShell);
  const attached = FakeController.last.attaches.length;
  update({ ...terminal, state: "exited", holder_link: "pending" });
  expect(shellRegion(container)).toBeNull();
  expect(container.textContent).toContain("The shell has exited");
  expect(FakeController.last.attaches).toHaveLength(attached);
  expect(FakeController.last.held).toBe(false);
});

it("attaches a shell opened pending that exits meanwhile, once, so that its final screen shows", () => {
  const { container, update } = mount(true, pendingShell);
  expect(FakeController.last.attaches).toEqual([]);
  update({ ...terminal, state: "exited" });
  expect(FakeController.last.attaches).toEqual([false]);
  expect(shellRegion(container)).toBeNull();
  expect(container.textContent).toContain("The shell has exited");
  expect(FakeController.last.held).toBe(false);
});

it("does not say the sentence while the cover holds focus, which reads the same copy", () => {
  const { container, update } = mount();
  FakeController.last.settle("open");
  act(() => FakeController.last.term.textarea.focus());
  update(pendingShell);
  expect(document.activeElement).toBe(shellRegion(container));
  act(() => void vi.advanceTimersByTime(60_000));
  expect(sentenceSpoken(container)).toHaveLength(0);
  // Opened pending, with focus landed on the cover before the delay ends.
  const opened = mount(true, pendingShell);
  act(() => opened.ref.current!.focus());
  expect(document.activeElement).toBe(shellRegion(opened.container));
  act(() => void vi.advanceTimersByTime(60_000));
  expect(sentenceSpoken(opened.container)).toHaveLength(0);
});

it("says the sentence once on entering pending while shown, and not again for retries or renames", () => {
  const { container, update } = mount();
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_MS));
  update(pendingShell);
  expect(sentenceSpoken(container)).toHaveLength(1);
  update({ ...pendingShell, title: "renamed" });
  FakeController.last.settle("closed");
  act(() => void vi.advanceTimersByTime(60_000));
  update({ ...pendingShell, title: "again", busy: true });
  expect(sentenceSpoken(container)).toHaveLength(1);
  expect(spoken(container)).toEqual([SHELL_SENTENCE]);
});

it("says it after the focus delay for a shell opened pending or shown later, and never for a hidden one", () => {
  const opened = mount(true, pendingShell);
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_MS - 1));
  expect(sentenceSpoken(opened.container)).toHaveLength(0);
  act(() => void vi.advanceTimersByTime(1));
  expect(sentenceSpoken(opened.container)).toHaveLength(1);

  const hidden = mount(false);
  hidden.update(pendingShell);
  act(() => void vi.advanceTimersByTime(60_000));
  expect(regions(hidden.container)).toEqual([]);
  hidden.show(true);
  expect(sentenceSpoken(hidden.container)).toHaveLength(0);
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_MS));
  expect(sentenceSpoken(hidden.container)).toHaveLength(1);
});

it("holds the recovered display's sentence until the cover has gone", () => {
  const { container, update } = mount(true, { ...terminal, holder_link: "pending", screen_inexact: false });
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_MS * 2));
  update({ ...pendingShell, screen_inexact: true });
  expect(spoken(container)).toEqual([SHELL_SENTENCE]);
  update({ ...terminal, screen_inexact: true });
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_MS));
  expect(spoken(container)).toEqual(["Recovered terminal display"]);
});
