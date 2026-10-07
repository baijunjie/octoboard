// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it } from "vitest";

import {
  boundArchived,
  boundConsoleSession,
  console_,
  otherConsoleSession,
  otherOwnerArchived,
  unboundArchived,
} from "../gallery/fixtures/archive";
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
 * query, and a way to unmount it. */
function renderArchiveView(): { container: HTMLElement; unmount: () => void } {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() =>
    root.render(
      <ArchiveView
        console={console_}
        boundTo={boundConsoleSession}
        sessions={[...OTHER_SESSIONS, ...boundArchived]}
        onReopen={() => {}}
        onOpenDialog={() => {}}
        dialogOpen={false}
        onClose={() => {}}
      />,
    ),
  );
  return {
    container,
    unmount: () => {
      act(() => root.unmount());
      container.remove();
    },
  };
}

it("lists a console session's own archived bound sessions, not its own archived row or an unrelated session", () => {
  const { container, unmount } = renderArchiveView();
  const titles = Array.from(container.querySelectorAll("li[data-marquee-scope]")).map((li) => li.textContent ?? "");
  expect(titles).toHaveLength(boundArchived.length);
  for (const session of boundArchived) expect(titles.some((t) => t.includes(session.title))).toBe(true);
  for (const session of OTHER_SESSIONS) expect(titles.some((t) => t.includes(session.title))).toBe(false);
  unmount();
});

it("titles the view after the owning console session, not the console", () => {
  const { container, unmount } = renderArchiveView();
  const section = container.querySelector("section[data-region='archive']");
  expect(section?.getAttribute("aria-label")).toBe(`Archived sessions bound to ${boundConsoleSession.title}`);
  unmount();
});
