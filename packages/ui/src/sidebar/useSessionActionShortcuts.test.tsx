// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { sessionOf } from "../gallery/fixtures/builders";
import type { Session } from "../protocol";
import { IS_MAC } from "../useWindowShortcut";
import { useSessionActionShortcuts } from "./useSessionActionShortcuts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const session = sessionOf("selected", "console", "project", "Selected", "idle");
const modifier = IS_MAC ? { metaKey: true } : { ctrlKey: true };

function mount(selected: Session | undefined = session) {
  const handlers = { onSetLocked: vi.fn(), onSetPinned: vi.fn(), onArchiveSession: vi.fn(), onCopySessionId: vi.fn() };
  function Host({ value }: { value: Session | undefined }) {
    useSessionActionShortcuts(value, handlers);
    return <input aria-label="Terminal input" />;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const render = (value: Session | undefined) => act(() => root.render(<Host value={value} />));
  render(selected);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  return { handlers, render, input: container.querySelector("input")! };
}

function press(code: string, init: KeyboardEventInit = {}, target: EventTarget = document.body) {
  const event = new KeyboardEvent("keydown", { code: `Key${code}`, bubbles: true, cancelable: true, shiftKey: true, ...modifier, ...init });
  act(() => { target.dispatchEvent(event); });
  return event;
}

it("acts on the selection from terminal input and follows changed selection and toggle state", () => {
  const { handlers, render, input } = mount();
  const terminalKey = vi.fn();
  input.addEventListener("keydown", terminalKey);
  expect(press("L", {}, input).defaultPrevented).toBe(true);
  expect(handlers.onSetLocked).toHaveBeenLastCalledWith(session, true);
  press("P");
  expect(handlers.onSetPinned).toHaveBeenLastCalledWith({ session }, true);
  const next = { ...session, id: "next", locked: true, pinned: true };
  render(next);
  press("L");
  press("P");
  expect(handlers.onSetLocked).toHaveBeenLastCalledWith(next, false);
  expect(handlers.onSetPinned).toHaveBeenLastCalledWith({ session: next }, false);
  press("C");
  expect(handlers.onCopySessionId).toHaveBeenLastCalledWith(next);
  expect(terminalKey).not.toHaveBeenCalled();
});

it("archives unlocked live sessions, preserves lock protection, and only copies an archived selection", () => {
  const { handlers, render } = mount();
  press("A");
  expect(handlers.onArchiveSession).toHaveBeenCalledExactlyOnceWith(session);
  render({ ...session, locked: true });
  press("A");
  expect(handlers.onArchiveSession).toHaveBeenCalledTimes(1);
  const archived = { ...session, status: "archived" as const };
  render(archived);
  for (const key of ["A", "L", "P"]) press(key);
  expect(handlers.onArchiveSession).toHaveBeenCalledTimes(1);
  expect(handlers.onSetLocked).not.toHaveBeenCalled();
  expect(handlers.onSetPinned).not.toHaveBeenCalled();
  press("C");
  expect(handlers.onCopySessionId).toHaveBeenCalledExactlyOnceWith(archived);
});

it("leaves unrelated modifiers, composition and no-selection keys alone, and suppresses repeat actions", () => {
  const { handlers, render } = mount();
  for (const init of [{ shiftKey: false }, { altKey: true }, { ctrlKey: true, metaKey: true }, { isComposing: true }]) {
    expect(press("L", init).defaultPrevented).toBe(false);
  }
  expect(press("L", { repeat: true }).defaultPrevented).toBe(true);
  expect(handlers.onSetLocked).not.toHaveBeenCalled();
  render(undefined);
  expect(press("L").defaultPrevented).toBe(false);
});

it.each(["dialog", "alertdialog", "menu"])("swallows actions while a %s is open", (role) => {
  const { handlers } = mount();
  const modal = document.body.appendChild(document.createElement("div"));
  modal.setAttribute("role", role);
  onTestFinished(() => modal.remove());
  for (const key of ["L", "P", "A", "C"]) expect(press(key).defaultPrevented).toBe(true);
  for (const handler of Object.values(handlers)) expect(handler).not.toHaveBeenCalled();
});
