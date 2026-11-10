// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { fakeTimers } from "../test/clock";
import { console_ } from "../fixtures/archiveSessions";
import { ArchiveLayer, EXIT_FALLBACK_MS } from "./ArchiveLayer";

vi.mock("./ArchiveView", () => ({
  ArchiveView: ({ active }: { active: boolean }) => <section data-region="archive" data-active={active}><button>Archive action</button></section>,
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount() {
  fakeTimers();
  const reduced = new EventTarget() as MediaQueryList;
  Object.defineProperty(reduced, "matches", { value: false, writable: true });
  vi.stubGlobal("matchMedia", () => reduced);
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const view = { console: console_, sessions: [], consoles: new Map(), projects: new Map(), accounts: [], defaultNames: {}, providers: [], onSelect() {}, onReopen() {}, onOpenDialog() {}, dialogOpen: false, onClose() {} };
  const show = (open: boolean) => act(() => root.render(<ArchiveLayer view={open ? view : undefined} />));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    vi.unstubAllGlobals();
  });
  show(true);
  const end = (name: string) => {
    const event = new Event("animationend", { bubbles: true });
    Object.defineProperty(event, "animationName", { value: name });
    act(() => container.querySelector("section")!.dispatchEvent(event));
  };
  return { container, reduced, show, end };
}

it("disables a closing archive immediately, then removes it at its own animation end", () => {
  const { container, show, end } = mount();
  show(false);
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
  act(() => vi.advanceTimersByTime(EXIT_FALLBACK_MS - 1));
  show(true);
  act(() => vi.advanceTimersByTime(EXIT_FALLBACK_MS));
  expect(container.querySelector("section")).toBe(section);
  expect(container.querySelector("[inert]")).toBeNull();
  show(false);
  act(() => vi.advanceTimersByTime(EXIT_FALLBACK_MS));
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
