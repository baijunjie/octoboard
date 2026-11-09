// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { accessibilityActionElement } from "../components/rows/rowPressTestSupport";
import { sessionOf } from "../fixtures/builders";
import type { RequestHandler } from "../requests";
import type { Session } from "../protocol";
import { TestHost } from "../test/TestHost";
import { testDaemon } from "../test/testDaemon";
import { SessionRow } from "./SessionRow";
import type { SidebarHandlers } from "./types";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(session: Session) {
  const handlers = {
    onSelectSession: vi.fn(),
    onOpenDialog: vi.fn(),
    onArchiveSession: vi.fn(),
    onSetPinned: vi.fn(),
    onSetLocked: vi.fn(),
    onCopySessionId: vi.fn(),
  } as unknown as SidebarHandlers;
  const request = vi.fn<RequestHandler>(async () => { throw new Error("No request expected"); });
  const daemon = testDaemon({ state: { sessions: new Map([[session.id, session]]) }, request });
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(
    <TestHost daemon={daemon}>
      <SessionRow handlers={handlers} session={session} selected={false} />
    </TestHost>,
  ));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  return { container, handlers, row: container.querySelector<HTMLElement>("[role=button]")! };
}

const shortcut = (container: HTMLElement, label: string) => container.querySelector<HTMLButtonElement>(`button[aria-label^="${label} "]`);

// WebKit sends an accessibility press (VoiceOver's VO-Space) as a click on the row's first native button, with
// no check on whether it is shown. For an unused session that is otherwise its Delete shortcut, which deletes the
// session outright.
it("opens an unused session on an accessibility press, without asking to delete it", () => {
  const session = sessionOf("unused", "console", "project", "Unused", "idle", { has_conversation: false });
  const { container, handlers, row } = mount(session);
  expect(shortcut(container, "Delete")).not.toBeNull();
  act(() => accessibilityActionElement(row)!.click());
  expect(handlers.onSelectSession).toHaveBeenCalledExactlyOnceWith(session);
  expect(handlers.onOpenDialog).not.toHaveBeenCalled();
});

it("opens a used session on an accessibility press, without archiving it", () => {
  const session = sessionOf("used", "console", "project", "Used", "idle", { has_conversation: true });
  const { container, handlers, row } = mount(session);
  expect(shortcut(container, "Archive")).not.toBeNull();
  act(() => accessibilityActionElement(row)!.click());
  expect(handlers.onSelectSession).toHaveBeenCalledExactlyOnceWith(session);
  expect(handlers.onArchiveSession).not.toHaveBeenCalled();
  expect(handlers.onOpenDialog).not.toHaveBeenCalled();
});

it("still lets the Delete shortcut itself open the delete dialog", () => {
  const session = sessionOf("unused", "console", "project", "Unused", "idle", { has_conversation: false });
  const { container, handlers } = mount(session);
  act(() => shortcut(container, "Delete")!.click());
  expect(handlers.onOpenDialog).toHaveBeenCalledWith({ kind: "delete-session", session });
  expect(handlers.onSelectSession).not.toHaveBeenCalled();
});
