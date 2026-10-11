// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { CONSOLE_PEEK_DWELL_MS, type ConsolePeek, useConsolePeek } from "./useConsolePeek";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root | undefined;

beforeEach(() => {
  vi.useFakeTimers();
  // Registered first, so it runs after the unmounts that `mount` registers.
  onTestFinished(() => {
    vi.useRealTimers();
    root = undefined;
  });
});

/** Mounts the hook over a stand-in for the sidebar's peek state; `reveal` makes it active, as the
 * real one does. */
function mount(peekable = true) {
  const control = { peekable, active: false, reveal: vi.fn(), keep: vi.fn(), leave: vi.fn() };
  let latest!: ConsolePeek;
  function Probe() {
    latest = useConsolePeek(control);
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const mounted = (root = createRoot(container));
  onTestFinished(() => {
    act(() => mounted.unmount());
    container.remove();
  });
  const render = () => act(() => mounted.render(<Probe />));
  render();
  return { control, render, peek: () => latest };
}
const wait = (ms: number) => act(() => void vi.advanceTimersByTime(ms));

it("floats the sidebar in, showing the console, only after the pointer has rested for the dwell", () => {
  const { control, peek } = mount();
  act(() => peek().hoverConsole("c-1"));
  wait(CONSOLE_PEEK_DWELL_MS - 1);
  expect(control.reveal).not.toHaveBeenCalled();
  expect(peek().consoleId).toBeUndefined();
  wait(1);
  expect(control.reveal).toHaveBeenCalledTimes(1);
  expect(peek().consoleId).toBe("c-1");
});

it("does not float the sidebar in when the pointer leaves the avatar before the dwell is over", () => {
  const { control, peek } = mount();
  act(() => peek().hoverConsole("c-1"));
  wait(CONSOLE_PEEK_DWELL_MS - 1);
  act(() => peek().leaveConsole());
  wait(500);
  expect(control.reveal).not.toHaveBeenCalled();
  expect(control.leave).toHaveBeenCalledTimes(1);
});

it("switches the shown console at once, with no dwell, while the sidebar is out", () => {
  const { control, render, peek } = mount();
  act(() => peek().hoverConsole("c-1"));
  wait(CONSOLE_PEEK_DWELL_MS);
  control.active = true;
  render();
  act(() => peek().leaveConsole());
  act(() => peek().hoverConsole("c-2"));
  expect(peek().consoleId).toBe("c-2");
  expect(control.keep).toHaveBeenCalledTimes(1);
  expect(control.reveal).toHaveBeenCalledTimes(1);
});

it("keeps showing the console after the sidebar slid away, and drops it once the sidebar cannot float", () => {
  const { control, render, peek } = mount();
  act(() => peek().hoverConsole("c-1"));
  wait(CONSOLE_PEEK_DWELL_MS);
  control.active = true;
  render();
  control.active = false;
  render();
  expect(peek().consoleId).toBe("c-1");
  control.peekable = false;
  render();
  expect(peek().consoleId).toBeUndefined();
});

it("does nothing where the sidebar cannot float: docked, or a narrow window", () => {
  const { control, peek } = mount(false);
  act(() => peek().hoverConsole("c-1"));
  wait(CONSOLE_PEEK_DWELL_MS * 2);
  expect(control.reveal).not.toHaveBeenCalled();
  expect(peek().consoleId).toBeUndefined();
});

it("shows only the last of two avatars the pointer crossed within the dwell, revealing once", () => {
  const { control, peek } = mount();
  act(() => peek().hoverConsole("c-1"));
  wait(50);
  act(() => peek().leaveConsole());
  act(() => peek().hoverConsole("c-2"));
  wait(CONSOLE_PEEK_DWELL_MS);
  expect(control.reveal).toHaveBeenCalledTimes(1);
  expect(peek().consoleId).toBe("c-2");
});

it("does not float the sidebar in when it unmounts during the dwell", () => {
  const { control, peek } = mount();
  act(() => peek().hoverConsole("c-1"));
  act(() => root!.unmount());
  wait(CONSOLE_PEEK_DWELL_MS * 2);
  expect(control.reveal).not.toHaveBeenCalled();
});

it("floats the sidebar in at once from the toggle, showing the current console rather than a previewed one", () => {
  const { control, peek } = mount();
  act(() => peek().hoverConsole("c-1"));
  wait(CONSOLE_PEEK_DWELL_MS);
  expect(peek().consoleId).toBe("c-1");
  act(() => peek().reveal());
  expect(control.reveal).toHaveBeenCalledTimes(2);
  expect(peek().consoleId).toBeUndefined();
});

it("does not float the sidebar in from the toggle where it cannot float", () => {
  const { control, peek } = mount(false);
  act(() => peek().reveal());
  expect(control.reveal).not.toHaveBeenCalled();
});

it("cancels a pending dwell when the toggle floats the sidebar in", () => {
  const { control, peek } = mount();
  act(() => peek().hoverConsole("c-1"));
  act(() => peek().reveal());
  wait(CONSOLE_PEEK_DWELL_MS * 2);
  expect(control.reveal).toHaveBeenCalledTimes(1);
  expect(peek().consoleId).toBeUndefined();
});
