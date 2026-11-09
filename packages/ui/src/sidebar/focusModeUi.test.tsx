// @vitest-environment jsdom
import { act } from "react";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { enter, openTooltip } from "../components/hint/tooltipTestSupport";
import { consoleOf, projectOf, sessionOf } from "../fixtures/builders";
import { isolate } from "../i18n/isolate";
import { menuOfTrigger, partSelector } from "../primitives";
import { restoreAllBindings, setBinding } from "../shortcuts/store";
import { BROWSER_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { ConsoleSessionFocusView, ProjectFocusView } from "./FocusView";
import { projectFocusIncludeConsoleSessions } from "./projectFocusPreference";
import type { SidebarHandlers } from "./types";
import { fakeTimers, runPendingTimers } from "../test/clock";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
afterEach(() => restoreAllBindings());

const main = consoleOf("c", "Main");
const project = projectOf("p", main.id, "Website");
const owner = sessionOf("owner", main.id, undefined, "Owner", "idle");
const bound = sessionOf("bound", main.id, project.id, "Bound", "idle", { bound_to: owner.id });
const sessions = [owner, bound];

function mount(view: "project" | "console") {
  const handlers = { onFocus: vi.fn(), onOpenDialog: vi.fn() } as unknown as SidebarHandlers;
  const daemon = testDaemon({ state: { projects: new Map([[project.id, project]]), sessions: new Map(sessions.map((session) => [session.id, session])) } });
  return mountHost(view === "project" ? <ProjectFocusView handlers={handlers} console={main} project={project} sessions={[bound]} owners={new Map([[owner.id, owner]])} selectedSessionId={bound.id} /> : <ConsoleSessionFocusView handlers={handlers} console={main} consoleSession={owner} projects={[project]} sessions={sessions} selectedSessionId={bound.id} />, { daemon });
}

it.each(["project", "console"] as const)("omits Enter focus mode from a selected bound card while %s focus is already active", async (view) => {
  act(() => projectFocusIncludeConsoleSessions.set(true));
  onTestFinished(() => act(() => projectFocusIncludeConsoleSessions.set(false)));
  const host = mount(view);
  const trigger = host.querySelector<HTMLButtonElement>(`button[aria-label="Actions for session ${isolate(bound.title)}"]`)!;
  await act(async () => trigger.click());
  const menu = menuOfTrigger(trigger)!;
  const labels = [...menu.querySelectorAll(partSelector.itemLabel)].map((label) => label.textContent);
  expect(labels).toContain("Rename");
  expect(labels).not.toContain("Focus mode");
});

it.each(["project", "console"] as const)("shows the live global focus binding in the %s exit tooltip", (view) => {
  fakeTimers();
  const host = mount(view);
  enter(host.querySelector<HTMLButtonElement>('button[aria-label="Leave focus mode"]')!);
  runPendingTimers();
  const tooltip = openTooltip()!;
  expect(tooltip.textContent).toContain("Leave focus mode");
  expect(tooltip.querySelector("kbd")?.textContent?.endsWith("F")).toBe(true);
  expect(tooltip.querySelector("kbd")?.textContent).not.toBe("F");
  act(() => { expect(setBinding("focusMode", { code: "KeyG", primary: true, alt: true }, BROWSER_HOST)).toBeUndefined(); });
  expect(openTooltip()?.querySelector("kbd")?.textContent?.endsWith("G")).toBe(true);
});
