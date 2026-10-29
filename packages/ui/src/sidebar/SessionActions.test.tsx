// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { enter, openTooltip, press, withFakeTimers } from "../components/tooltipTestSupport";
import { sessionOf } from "../gallery/fixtures/builders";
import type { Session } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { SessionActions } from "./SessionActions";
import type { SidebarHandlers } from "./types";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(session: Session, inspect: Daemon["request"]) {
  const handlers = { onOpenDialog: vi.fn(), onArchiveSession: vi.fn(), onCopySessionId: vi.fn() } as unknown as SidebarHandlers;
  const daemon: Daemon = {
    store: createStateStore({ sessions: new Map([[session.id, session]]) }),
    request: vi.fn(inspect) as never,
    toastError: vi.fn(),
    toastNotice: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const render = (next: Session) => act(() => root.render(
    <DaemonProvider value={daemon}>
      <SessionActions session={next} handlers={handlers} accounts={[]} hasBoundSessions={false} rowRef={{ current: container }} />
    </DaemonProvider>,
  ));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  render(session);
  return { container, daemon, handlers, render };
}

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
  const { container, daemon, handlers } = mount(session, async () => { throw new Error("No inspection expected"); });
  expect(shortcut(container, "Delete") !== null).toBe(label === "Delete");
  expect(shortcut(container, "Archive") !== null).toBe(label === "Archive");
  expect(daemon.request).not.toHaveBeenCalled();
  if (label === "Delete") {
    act(() => shortcut(container, "Delete")!.click());
    expect(handlers.onOpenDialog).toHaveBeenCalledWith({ kind: "delete-session", session });
  }
});

it("offers a bound unused session the user opened the Delete shortcut", () => {
  const { container } = mount({ ...unused, status: "idle", bound_to: "owner", origin: "user" }, async () => ({ type: "ack" }));
  expect(shortcut(container, "Delete")).not.toBeNull();
});

it("removes the delete shortcut as soon as the first conversation is reported", () => {
  const { container, daemon, render } = mount({ ...unused, status: "idle" }, async () => ({ type: "ack" }));
  expect(shortcut(container, "Delete")).not.toBeNull();
  render({ ...unused, status: "working", has_conversation: true });
  expect(shortcut(container, "Delete")).toBeNull();
  expect(daemon.request).not.toHaveBeenCalled();
});

it.each([false, true])("hides removal shortcuts on a locked session, conversation=%s", (has_conversation) => {
  const { container } = mount({ ...unused, status: "idle", has_conversation, locked: true }, async () => ({ type: "ack" }));
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
  const { container, handlers } = mount(session, async () => ({ type: "ack" }));
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
