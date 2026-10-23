// @vitest-environment jsdom
import { act, createRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import type { ShellTerminal } from "../protocol";
import type { TermStatus } from "./TerminalController";
import { MAX_AUTO_RECONNECT_ATTEMPTS } from "./reconnectPolicy";
import { ShellTerminalView, type ShellTerminalViewHandle } from "./ShellTerminalView";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** A controller whose socket the test settles by hand, with an input element of its own. */
const { FakeController } = vi.hoisted(() => {
  class FakeController {
    static last: FakeController;
    term = { textarea: document.createElement("textarea") };
    constructor(private callbacks: { onStatusChange: (status: TermStatus) => void }) {
      FakeController.last = this;
    }
    mount(element: HTMLElement): void {
      element.append(this.term.textarea);
    }
    setInputLabel(): void {}
    fit(): void {}
    syncSize(): void {}
    dispose(): void {}
    focus(): void {
      this.term.textarea.focus();
    }
    attach(): void {
      this.callbacks.onStatusChange("connecting");
    }
    settle(status: TermStatus): void {
      act(() => this.callbacks.onStatusChange(status));
    }
  }
  return { FakeController };
});
vi.mock("./TerminalController", () => ({ TerminalController: FakeController }));
const daemon = vi.hoisted(() => ({ terminalUrl: () => "", terminalReconnectDelay: () => 0, request: vi.fn(), toastError: vi.fn() }));
const store = vi.hoisted(() => ({ snapshotEpoch: 0, connectionState: "open" }));
vi.mock("../store", () => ({
  useDaemon: () => daemon,
  useDaemonStore: (select: (state: typeof store) => unknown) => select(store),
}));
vi.mock("../platform/react", () => ({ usePlatform: () => ({ links: undefined }) }));
vi.mock("../theme", () => ({ useOctoboardTheme: () => ({ resolved: "light" }) }));
vi.mock("./TerminalEditMenu", () => ({ TerminalEditMenu: () => null }));

const terminal: ShellTerminal = {
  id: "t", project_id: "p", worktree: null, cwd: "/p", title: "shell", renamed: false, state: "running", busy: false, created_at: 1,
};

const roots: Root[] = [];
beforeEach(() => {
  vi.useFakeTimers();
  store.connectionState = "open";
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
  const show = (next: boolean) => act(() => root.render(<ShellTerminalView terminal={record} shown={next} onClose={() => {}} ref={ref} />));
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
