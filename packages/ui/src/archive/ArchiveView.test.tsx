// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import {
  archivedLead,
  boundArchived,
  boundConsoleSession,
  console_,
  otherConsoleSession,
  otherOwnerArchived,
  unboundArchived,
  underArchivedLead,
} from "../gallery/fixtures/archive";
import type { DialogRequest } from "../dialogs/dialogRequest";
import type { Session } from "../protocol";
import { ArchiveView } from "./ArchiveView";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// jsdom has no ResizeObserver; FadeOverflow and HeroUI's ScrollShadow, both in ArchiveView's rows,
// each need one to mount at all.
class StubResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
globalThis.ResizeObserver ??= StubResizeObserver as unknown as typeof ResizeObserver;

// Everything the fixture knows about besides `boundConsoleSession`'s own bound, archived sessions:
// its own (idle, not archived) row, a second owner, a session bound to that second owner, and an
// unbound one. Passed in full so the view's own filtering, not the fixture, is what narrows it down.
const OTHER_SESSIONS = [boundConsoleSession, otherConsoleSession, otherOwnerArchived, unboundArchived];

/** Renders the view for `boundConsoleSession`'s own scope and returns its container, for a test to
 * query. The root is unmounted when the test ends. */
function renderArchiveView(
  onOpenDialog: (dialog: DialogRequest) => void = () => {},
  onClose: () => void = () => {},
  sessions: Session[] = [...OTHER_SESSIONS, ...boundArchived],
): { container: HTMLElement } {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() =>
    root.render(
      <ArchiveView
        console={console_}
        boundTo={boundConsoleSession}
        sessions={sessions}
        accounts={[]}
        onReopen={() => {}}
        onOpenDialog={onOpenDialog}
        dialogOpen={false}
        onClose={onClose}
      />,
    ),
  );
  return { container };
}

it("lists a console session's own archived bound sessions, not its own archived row or an unrelated session", () => {
  const { container } = renderArchiveView();
  const titles = Array.from(container.querySelectorAll("li[data-marquee-scope]")).map((li) => li.textContent ?? "");
  expect(titles).toHaveLength(boundArchived.length);
  for (const session of boundArchived) expect(titles.some((t) => t.includes(session.title))).toBe(true);
  for (const session of OTHER_SESSIONS) expect(titles.some((t) => t.includes(session.title))).toBe(false);
});

it("titles the view after the owning console session, not the console", () => {
  const { container } = renderArchiveView();
  const section = container.querySelector("section[data-region='archive']");
  expect(section?.getAttribute("aria-label")).toBe(`Archived sessions bound to ${boundConsoleSession.title}`);
});

it("deletes all of a console session's archive by naming that console session", () => {
  const onOpenDialog = vi.fn();
  const { container } = renderArchiveView(onOpenDialog);
  const deleteAll = Array.from(container.querySelectorAll("button")).find((b) => b.textContent === "Delete all");
  act(() => deleteAll?.click());
  expect(onOpenDialog).toHaveBeenCalledWith(
    expect.objectContaining({ kind: "delete-archived", consoleSession: boundConsoleSession, project: undefined, count: boundArchived.length }),
  );
});

it("lists the sessions archived under a lead session after it, inset, and names it for assistive technology", () => {
  const { container } = renderArchiveView(undefined, undefined, [boundConsoleSession, archivedLead, ...underArchivedLead, ...boundArchived]);
  const rows = Array.from(container.querySelectorAll<HTMLElement>("li[data-marquee-scope]"));
  // Newest first, except that the lead session's own sessions follow it rather than being ranked
  // against the console session's.
  expect(rows.map((row) => [row.querySelector(".text-sm")?.textContent, row.className.includes("ms-4")])).toEqual([
    [boundArchived[0].title, false],
    [archivedLead.title, false],
    [underArchivedLead[0].title, true],
    [underArchivedLead[1].title, true],
    [boundArchived[1].title, false],
    [boundArchived[2].title, false],
  ]);
  expect(rows[2].querySelector(".sr-only")?.textContent).toBe(`Bound to ${archivedLead.title}`);
});

/** Presses Escape on the control the way a keyboard user gets there — a key press, then the focus,
 * which is what opens its tooltip — and answers how many times the view asked to close. */
function escapeFrom(control: (view: HTMLElement) => HTMLElement | null | undefined): { closes: number; tooltip: boolean } {
  const onClose = vi.fn();
  const { container } = renderArchiveView(() => {}, onClose);
  const focused = control(container.querySelector<HTMLElement>("section[data-region='archive']")!);
  if (!focused) throw new Error("the view did not render the control to focus");
  act(() => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    focused.focus();
  });
  const tooltip = document.querySelector("[role=tooltip]") !== null;
  act(() => {
    focused.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
  });
  return { closes: onClose.mock.calls.length, tooltip };
}

// The close button's tooltip opens on focus, and while it is open react-aria takes Escape before
// the view's own handler sees it.
it("closes on Escape with focus on its close button, whose tooltip is open there", () => {
  expect(escapeFrom((view) => view.querySelector<HTMLElement>("button[aria-label='Close']"))).toEqual({ closes: 1, tooltip: true });
});

// And closes exactly once from the view itself, where there is no tooltip and its own handler runs.
it("closes on Escape once from the view itself", () => {
  expect(escapeFrom((view) => view)).toEqual({ closes: 1, tooltip: false });
});
