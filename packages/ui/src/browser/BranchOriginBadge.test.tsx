// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { enter, leave, openTooltip } from "../components/tooltipTestSupport";
import { consoleOf, projectOf, sessionOf, snapshotState } from "../gallery/fixtures/builders";
import { isolate } from "../i18n/isolate";
import type { BranchInfo } from "../protocol";
import { BranchSelect } from "./BranchSelect";
import { mountRows } from "./browserTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const origin = (session: string, title: string) => ({ responsible_session: session, responsible_title: title, responsible_since: 0 });
const branches: BranchInfo[] = [
  { name: "worked-by-session", commit: "a".repeat(40), origin: origin("s-live", "Fix the layout") },
  { name: "worked-by-task", commit: "e".repeat(40), origin: origin("s-task", "Fix the header") },
  { name: "worked-by-archived", commit: "c".repeat(40), origin: origin("s-archived", "Old experiment") },
  { name: "worked-by-gone", commit: "d".repeat(40), origin: origin("s-gone", "Tune the weights") },
  { name: "unattributed", commit: "b".repeat(40) },
];

const console_ = consoleOf("c", "Main");
const project = projectOf("p", console_.id, "Website");
const state = snapshotState({
  consoles: [console_],
  projects: [project],
  sessions: [
    sessionOf("s-live", console_.id, project.id, "Fix the layout, renamed", "working"),
    sessionOf("s-task", console_.id, project.id, "Fix the header", "idle", { terminal_title: "Center the header" }),
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

it("marks only a branch with a responsible session, naming the session by its current title", () => {
  open();
  const ring = ringOf("worked-by-session");
  // The session was renamed since it became responsible, and the sidebar shows the new title.
  expect(ring.getAttribute("aria-label")).toBe(`Session ${isolate("Fix the layout, renamed")} is working on this branch`);
  expect(optionOf("unattributed").querySelector("[role=img]")).toBeNull();
});

it("names a session with an agent task by its task beside its name, as the sidebar does", () => {
  open();
  const title = `${isolate("Center the header")}｜${isolate("Fix the header")}`;
  expect(ringOf("worked-by-task").getAttribute("aria-label")).toBe(`Session ${isolate(title)} is working on this branch`);
});

it("leaves the option's own name to its content, so the ring's name is not duplicated onto it", () => {
  open();
  expect(optionOf("worked-by-session").hasAttribute("aria-label")).toBe(false);
  expect(optionOf("worked-by-session").textContent).toBe("worked-by-sessionaaaaaaa");
});

it.each([
  ["an archived session, which still exists", "worked-by-archived", `Session ${isolate("Old experiment")} is working on this branch`],
  ["a session that is gone", "worked-by-gone", `Session ${isolate("Tune the weights")} was working on this branch and no longer exists`],
])("names %s", (_, branch, name) => {
  open();
  expect(ringOf(branch).getAttribute("aria-label")).toBe(name);
});

it("does not call a session gone before the daemon's first snapshot", () => {
  open(undefined, {});
  expect(ringOf("worked-by-gone").getAttribute("aria-label")).toBe(`Session ${isolate("Tune the weights")} is working on this branch`);
});

it("draws no badge on the trigger once a branch is chosen", () => {
  const container = mountRows(
    <BranchSelect label="From" branches={branches} value="worked-by-session" gone={false} selectedSessionId={undefined} onChange={() => {}} />,
    { state },
  );
  expect(container.querySelector("button")!.textContent).toContain("worked-by-session");
  expect(container.querySelector("[role=img]")).toBeNull();
});

it("shows the responsible session in a tooltip after the delay while the pointer is on the ring, and not over the rest of the row", () => {
  // Fake timers, to show that nothing is left counting down once the tooltip has gone.
  vi.useFakeTimers();
  onTestFinished(() => {
    vi.useRealTimers();
  });
  const settle = () => act(() => void vi.runOnlyPendingTimers());
  open();
  expect(openTooltip()).toBeNull();

  enter(optionOf("worked-by-session"));
  settle();
  expect(openTooltip()).toBeNull();
  leave(optionOf("worked-by-session"));

  enter(ringOf("worked-by-session"));
  expect(openTooltip()).toBeNull();
  settle();
  expect(openTooltip()?.textContent).toBe("Session Fix the layout, renamed is working on this branch");
  expect(openTooltip()?.querySelector("[data-inline-chip]")?.textContent).toBe("Fix the layout, renamed");

  leave(ringOf("worked-by-session"));
  settle();
  expect(openTooltip()).toBeNull();
  expect(vi.getTimerCount()).toBe(0);
});

it("takes no focus: the ring is no tab stop and a press on it is cancelled", () => {
  open();
  const ring = ringOf("worked-by-session");
  expect(ring.tabIndex).toBe(-1);
  const press = new MouseEvent("mousedown", { bubbles: true, cancelable: true });
  act(() => void ring.dispatchEvent(press));
  expect(press.defaultPrevented).toBe(true);
});

it("names the selected session by its current title, not the recorded one", () => {
  open("s-live");
  expect(ringOf("worked-by-session").getAttribute("aria-label")).toBe(`The selected session ${isolate("Fix the layout, renamed")} is working on this branch`);
});

it("keeps the recorded title for a selected session that is gone", () => {
  open("s-gone");
  expect(ringOf("worked-by-gone").getAttribute("aria-label")).toBe(`Session ${isolate("Tune the weights")} was working on this branch and no longer exists`);
});

it("marks the selected session's branches in the accent colour and in words", () => {
  open("s-live");
  const mark = (name: string) => ringOf(name).firstElementChild!;
  expect(mark("worked-by-session").className).toContain("border-accent-glyph");
  // The fill is what tells it from the plain ring without the colour.
  expect(mark("worked-by-session").className).toContain("bg-accent-glyph");
  expect(mark("worked-by-archived").className).toContain("border-app-muted");
  expect(mark("worked-by-archived").className).not.toContain("bg-");
});
