// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import type { RequestHandler } from "../requests";
import { enter, openTooltip, press, withFakeTimers } from "../components/hint/tooltipTestSupport";
import { sessionOf } from "../fixtures/builders";
import type { Session } from "../protocol";
import { ariaBinding } from "../shortcuts/binding";
import { defaultBinding } from "../shortcuts/commands";
import { restoreAllBindings } from "../shortcuts/store";
import { TestHost } from "../test/TestHost";
import { testDaemon } from "../test/testDaemon";
import { SessionActions } from "./SessionActions";
import type { SidebarHandlers } from "./types";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(session: Session, inspect: RequestHandler) {
  const handlers = { onOpenDialog: vi.fn(), onArchiveSession: vi.fn(), onCopySessionId: vi.fn() } as unknown as SidebarHandlers;
  const request = vi.fn(inspect);
  const daemon = testDaemon({ state: { sessions: new Map([[session.id, session]]) }, request });
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const render = (next: Session) => act(() => root.render(
    <TestHost daemon={daemon}>
      <SessionActions selected session={next} handlers={handlers} accounts={[]} hasBoundSessions={false} rowRef={{ current: container }} />
    </TestHost>,
  ));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  render(session);
  return { container, daemon, request, handlers, render };
}

afterEach(() => {
  restoreAllBindings();
  localStorage.clear();
});

const unused = sessionOf("unused", "console", "project", "Unused", "archived", { has_conversation: false });
const shortcut = (container: HTMLElement, label: string) => container.querySelector<HTMLButtonElement>(`button[aria-label^="${label} "]`);

it.each<[Session["status"], boolean, string | undefined]>([
  ["archived", true, undefined],
  ["archived", false, "Delete"],
  ["idle", false, "Delete"],
  ["working", false, "Delete"],
  ["starting", false, undefined],
  ["idle", true, "Archive"],
])("offers a shortcut for %s, conversation=%s without inspecting native records", (status, has_conversation, label) => {
  const session = { ...unused, status, has_conversation };
  const { container, request, handlers } = mount(session, async () => { throw new Error("No inspection expected"); });
  expect(shortcut(container, "Delete") !== null).toBe(label === "Delete");
  expect(shortcut(container, "Archive") !== null).toBe(label === "Archive");
  expect(request).not.toHaveBeenCalled();
  if (label === "Delete") {
    act(() => shortcut(container, "Delete")!.click());
    expect(handlers.onOpenDialog).toHaveBeenCalledWith({ kind: "delete-session", session });
  }
});

it("offers a bound unused session the user opened the Delete shortcut", () => {
  const { container } = mount({ ...unused, status: "idle", bound_to: "owner", origin: "user" }, async () => ({ type: "ack", id: null }));
  expect(shortcut(container, "Delete")).not.toBeNull();
});

it("removes the delete shortcut as soon as the first conversation is reported", () => {
  const { container, request, render } = mount({ ...unused, status: "idle" }, async () => ({ type: "ack", id: null }));
  expect(shortcut(container, "Delete")).not.toBeNull();
  render({ ...unused, status: "working", has_conversation: true });
  expect(shortcut(container, "Delete")).toBeNull();
  expect(request).not.toHaveBeenCalled();
});

it.each([false, true])("hides removal shortcuts on a locked session, conversation=%s", (has_conversation) => {
  const { container } = mount({ ...unused, status: "idle", has_conversation, locked: true }, async () => ({ type: "ack", id: null }));
  expect(shortcut(container, "Delete")).toBeNull();
  expect(shortcut(container, "Archive")).toBeNull();
});

it.each([true, false])("gives the Copy session ID row's ID its full text as a tooltip only while it is clipped, clipped=%s", async (clipped) => {
  const settle = withFakeTimers();
  // jsdom lays nothing out, so the overflow every label measures is stubbed.
  for (const [name, value] of [["scrollWidth", clipped ? 300 : 100], ["clientWidth", 100]] as const) {
    const own = Object.getOwnPropertyDescriptor(Element.prototype, name)!;
    Object.defineProperty(Element.prototype, name, { configurable: true, get: () => value });
    onTestFinished(() => void Object.defineProperty(Element.prototype, name, own));
  }
  const session = { ...unused, id: "3f2c9a1e-7b4d-4e8a-9c61-0d5b2f8e4a17", status: "idle" as const, has_conversation: true };
  const { container, handlers } = mount(session, async () => ({ type: "ack", id: null }));
  press(container.querySelector<HTMLElement>('button[aria-label^="Actions for session"]')!, "mouse");
  await act(async () => {});
  const item = [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((el) => el.getAttribute("aria-label") === "Copy session ID")!;
  const id = [...item.querySelectorAll<HTMLElement>("[dir=ltr]")].find((el) => el.textContent === session.id)!;

  enter(id);
  settle();
  const tooltip = openTooltip();
  if (clipped) {
    // The bare ID, laid out left to right like the label under a right-to-left language too.
    expect(tooltip?.textContent).toBe(session.id);
    expect(tooltip?.querySelector("[dir=ltr]")?.textContent).toBe(session.id);
  } else {
    expect(tooltip).toBeNull();
  }

  // A press on the ID line is a press of the item, which still copies.
  press(id, "mouse");
  expect(handlers.onCopySessionId).toHaveBeenCalledExactlyOnceWith(session);
});

it("shows menu-only letters instead of selected-session window shortcuts", async () => {
  const session = { ...unused, status: "idle" as const, has_conversation: true, kind: "console" as const };
  const { container, handlers } = mount(session, async () => ({ type: "ack", id: null }));
  for (const letter of ["L", "P", "A", "C"]) {
    act(() => container.dispatchEvent(new KeyboardEvent("keydown", { key: letter.toLowerCase(), code: `Key${letter}`, bubbles: true })));
  }
  expect(handlers.onArchiveSession).not.toHaveBeenCalled();
  expect(handlers.onCopySessionId).not.toHaveBeenCalled();
  press(container.querySelector<HTMLElement>('button[aria-label^="Actions for session"]')!, "mouse");
  await act(async () => {});
  const shown = (label: string) => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((el) => el.textContent?.startsWith(label))!;
  for (const [label, letter] of [["Lock", "L"], ["Pin", "P"], ["Rename", "R"], ["Copy session ID", "C"], ["Change model", "M"], ["Archive", "A"]]) {
    expect(shown(label).querySelector("kbd")?.textContent).toBe(letter);
    expect(shown(label).getAttribute("aria-keyshortcuts")).toBe(letter);
  }
  expect(shown("Focus mode").querySelector("kbd")?.textContent?.endsWith("F")).toBe(true);
  expect(shown("Focus mode").getAttribute("aria-keyshortcuts")).toBe(ariaBinding(defaultBinding("focusMode")));
  expect(shown("Colour").querySelector("kbd")).toBeNull();
  expect(shown("Colour").getAttribute("aria-keyshortcuts")).toBeNull();
  act(() => document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "b", bubbles: true, cancelable: true })));
  expect(document.querySelector('[role="listbox"]')).toBeNull();
  act(() => document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "c", bubbles: true, cancelable: true })));
  expect(handlers.onCopySessionId).toHaveBeenCalledExactlyOnceWith(session);
});
