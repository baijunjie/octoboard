// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { sessionOf } from "../gallery/fixtures/builders";
import type { Session, SessionRecordsPresence } from "../protocol";
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

it.each<[Session["status"], boolean, SessionRecordsPresence, string | undefined]>([
  ["archived", true, "absent", undefined],
  ["archived", false, "present", undefined],
  ["archived", false, "unknown", undefined],
  ["archived", false, "absent", "Delete"],
  ["idle", false, "absent", "Delete"],
  ["idle", false, "present", undefined],
  ["idle", true, "absent", "Archive"],
])("offers only a confirmation-free shortcut for %s, conversation=%s, records=%s", async (status, has_conversation, presence, label) => {
  const session = { ...unused, status, has_conversation };
  const { container, daemon, handlers } = mount(session, async () => ({ type: "session_records_inspected", presence }));
  await act(async () => {});
  expect(shortcut(container, "Delete") !== null).toBe(label === "Delete");
  expect(shortcut(container, "Archive") !== null).toBe(label === "Archive");
  expect(daemon.request).toHaveBeenCalledTimes(has_conversation ? 0 : 1);
  if (label === "Delete") {
    act(() => shortcut(container, "Delete")!.click());
    expect(handlers.onOpenDialog).toHaveBeenCalledWith({ kind: "delete-session", session });
  }
});

it("waits for record inspection and discards absence when the native session changes", async () => {
  let resolve!: (value: { type: "session_records_inspected"; presence: SessionRecordsPresence }) => void;
  const { container, daemon, render } = mount(unused, () => new Promise((done) => { resolve = done; }));
  expect(shortcut(container, "Delete")).toBeNull();
  await act(async () => resolve({ type: "session_records_inspected", presence: "absent" }));
  expect(shortcut(container, "Delete")).not.toBeNull();
  render({ ...unused, title: "Renamed" });
  expect(daemon.request).toHaveBeenCalledTimes(1);
  render({ ...unused, agent_session_id: "another-native-session" });
  expect(shortcut(container, "Delete")).toBeNull();
  expect(daemon.request).toHaveBeenCalledTimes(2);
  await act(async () => resolve({ type: "session_records_inspected", presence: "present" }));
  expect(shortcut(container, "Delete")).toBeNull();
});
