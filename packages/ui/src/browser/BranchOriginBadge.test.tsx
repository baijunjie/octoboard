// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { enter, leave, openTooltip } from "../components/tooltipTestSupport";
import { consoleOf, projectOf, sessionOf, snapshotState } from "../gallery/fixtures/builders";
import { isolate } from "../i18n/isolate";
import type { BranchInfo } from "../protocol";
import { BranchSelect } from "./BranchComparison";
import { mountRows } from "./browserTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const origin = (session: string, title: string) => ({ source: "hook_diff" as const, session, title, created_at: 0 });
const branches: BranchInfo[] = [
  { name: "made-by-session", commit: "a".repeat(40), origin: origin("s-live", "Fix the layout") },
  { name: "made-by-archived", commit: "c".repeat(40), origin: origin("s-archived", "Old experiment") },
  { name: "made-by-gone", commit: "d".repeat(40), origin: origin("s-gone", "Tune the weights") },
  { name: "made-in-terminal", commit: "b".repeat(40) },
];

const console_ = consoleOf("c", "Main");
const project = projectOf("p", console_.id, "Website");
const state = snapshotState({
  consoles: [console_],
  projects: [project],
  sessions: [
    sessionOf("s-live", console_.id, project.id, "Fix the layout, renamed", "working"),
    sessionOf("s-archived", console_.id, project.id, "Old experiment", "archived"),
  ],
});

const optionOf = (name: string) => [...document.querySelectorAll<HTMLElement>("[role=option]")].find((option) => option.textContent?.startsWith(name))!;
const ringOf = (name: string) => optionOf(name).querySelector<HTMLElement>("[role=img]")!;
function open(selectedSessionId?: string, daemonState = state, value?: string): void {
  const container = mountRows(
    <BranchSelect label="From" branches={branches} value={value} gone={false} selectedSessionId={selectedSessionId} onChange={() => {}} />,
    { state: daemonState },
  );
  act(() => container.querySelector<HTMLElement>("button")!.click());
}

it("marks only a branch with an origin, naming the session by its quoted current title", () => {
  open();
  const ring = ringOf("made-by-session");
  // The session was renamed since the attribution, and the sidebar shows the new title.
  expect(ring.getAttribute("aria-label")).toBe(`Created by session “${isolate("Fix the layout, renamed")}”`);
  expect(optionOf("made-in-terminal").querySelector("[role=img]")).toBeNull();
});

it("leaves the option's own name to its content, so the ring's name is not duplicated onto it", () => {
  open();
  expect(optionOf("made-by-session").hasAttribute("aria-label")).toBe(false);
  expect(optionOf("made-by-session").textContent).toBe("made-by-sessionaaaaaaa");
});

it.each([
  ["an archived session, which still exists", "made-by-archived", `Created by session “${isolate("Old experiment")}”`],
  ["a session that is gone", "made-by-gone", `Created by session “${isolate("Tune the weights")}”, which no longer exists`],
])("names %s", (_, branch, name) => {
  open();
  expect(ringOf(branch).getAttribute("aria-label")).toBe(name);
});

it("does not call a session gone before the daemon's first snapshot", () => {
  open(undefined, {});
  expect(ringOf("made-by-gone").getAttribute("aria-label")).toBe(`Created by session “${isolate("Tune the weights")}”`);
});

it("draws no badge on the trigger once a branch is chosen", () => {
  const container = mountRows(
    <BranchSelect label="From" branches={branches} value="made-by-session" gone={false} selectedSessionId={undefined} onChange={() => {}} />,
    { state },
  );
  expect(container.querySelector("button")!.textContent).toContain("made-by-session");
  expect(container.querySelector("[role=img]")).toBeNull();
});

it("shows the creator in a tooltip after the delay while the pointer is on the ring, and not over the rest of the row", () => {
  // Fake timers, to show that nothing is left counting down once the tooltip has gone.
  vi.useFakeTimers();
  onTestFinished(() => {
    vi.useRealTimers();
  });
  const settle = () => act(() => void vi.runOnlyPendingTimers());
  open();
  expect(openTooltip()).toBeNull();

  enter(optionOf("made-by-session"));
  settle();
  expect(openTooltip()).toBeNull();
  leave(optionOf("made-by-session"));

  enter(ringOf("made-by-session"));
  expect(openTooltip()).toBeNull();
  settle();
  expect(openTooltip()?.textContent).toBe(`Created by session “${isolate("Fix the layout, renamed")}”`);

  leave(ringOf("made-by-session"));
  settle();
  expect(openTooltip()).toBeNull();
  expect(vi.getTimerCount()).toBe(0);
});

it("takes no focus: the ring is no tab stop and a press on it is cancelled", () => {
  open();
  const ring = ringOf("made-by-session");
  expect(ring.tabIndex).toBe(-1);
  const press = new MouseEvent("mousedown", { bubbles: true, cancelable: true });
  act(() => void ring.dispatchEvent(press));
  expect(press.defaultPrevented).toBe(true);
});

it("names the selected session by its current title, not the snapshot's", () => {
  open("s-live");
  expect(ringOf("made-by-session").getAttribute("aria-label")).toBe(`Created by the selected session “${isolate("Fix the layout, renamed")}”`);
});

it("keeps the snapshot title for a selected session that is gone", () => {
  open("s-gone");
  expect(ringOf("made-by-gone").getAttribute("aria-label")).toBe(`Created by session “${isolate("Tune the weights")}”, which no longer exists`);
});

it("marks the selected session's branches in the accent colour and in words", () => {
  open("s-live");
  const mark = (name: string) => ringOf(name).firstElementChild!;
  expect(mark("made-by-session").className).toContain("border-accent-glyph");
  // The fill is what tells it from the plain ring without the colour.
  expect(mark("made-by-session").className).toContain("bg-accent-glyph");
  expect(mark("made-by-archived").className).toContain("border-muted");
  expect(mark("made-by-archived").className).not.toContain("bg-");
});
