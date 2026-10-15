// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { console_ } from "../gallery/fixtures/archive";
import { ArchiveLayer } from "./ArchiveLayer";

vi.mock("./ArchiveView", () => ({
  ArchiveView: ({ active }: { active: boolean }) => <section data-region="archive" data-active={active}><button>Archive action</button></section>,
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount() {
  vi.useFakeTimers();
  const reduced = new EventTarget() as MediaQueryList;
  Object.defineProperty(reduced, "matches", { value: false, writable: true });
  vi.stubGlobal("matchMedia", () => reduced);
  const container = document.body.appendChild(document.createElement("div"));
  const terminal = document.body.appendChild(document.createElement("button"));
  const root = createRoot(container);
  const focusTerminal = () => terminal.focus();
  const view = { console: console_, sessions: [], accounts: [], onReopen() {}, onOpenDialog() {}, dialogOpen: false, onClose() {} };
  const show = (open: boolean) => act(() => root.render(<ArchiveLayer view={open ? view : undefined} focusTerminal={focusTerminal} />));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    terminal.remove();
    act(() => vi.runOnlyPendingTimers());
    expect(vi.getTimerCount()).toBe(0);
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });
  show(true);
  const end = (name: string) => {
    const event = new Event("animationend", { bubbles: true });
    Object.defineProperty(event, "animationName", { value: name });
    act(() => container.querySelector("section")!.dispatchEvent(event));
  };
  return { container, terminal, reduced, show, end };
}

it("hands off focus and disables a closing archive immediately, then removes it at its own animation end", () => {
  const { container, terminal, show, end } = mount();
  act(() => container.querySelector("button")!.focus());
  show(false);
  expect(document.activeElement).toBe(terminal);
  expect(container.querySelector("[inert][aria-hidden=true] section[data-active=false]")).not.toBeNull();
  end("unrelated-animation");
  expect(container.querySelector("section")).not.toBeNull();
  end("archive-exit");
  expect(container.innerHTML).toBe("");
});

it("cancels stale removal when reopened and still finishes an exit whose animation event was skipped", () => {
  const { container, show } = mount();
  const section = container.querySelector("section");
  show(false);
  act(() => vi.advanceTimersByTime(100));
  show(true);
  act(() => vi.advanceTimersByTime(500));
  expect(container.querySelector("section")).toBe(section);
  expect(container.querySelector("[inert]")).toBeNull();
  show(false);
  act(() => vi.advanceTimersByTime(300));
  expect(container.innerHTML).toBe("");
});

it("removes the exit immediately when reduced motion is requested, including during the exit", () => {
  const { container, reduced, show } = mount();
  show(false);
  act(() => {
    Object.defineProperty(reduced, "matches", { value: true });
    reduced.dispatchEvent(new Event("change"));
  });
  expect(container.innerHTML).toBe("");
  show(true);
  show(false);
  expect(container.innerHTML).toBe("");
});
