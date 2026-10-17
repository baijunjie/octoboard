// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { sessionOf } from "../gallery/fixtures/builders";
import type { Session } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { SessionActions } from "./SessionActions";
import type { SidebarHandlers } from "./types";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(session: Session, inspect: Daemon["request"]) {
  const handlers = { onOpenDialog: vi.fn(), onArchiveSession: vi.fn() } as unknown as SidebarHandlers;
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
