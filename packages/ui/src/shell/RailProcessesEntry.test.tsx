// @vitest-environment jsdom
import { act, createElement } from "react";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { DetailsButton } from "../components/alerts/DetailsButton";
import { HOVER_CLOSE_DELAY_MS } from "../components/hint/hintPopover";
import { activate, enter, leave, press, mountForTooltip, withFakeTimers } from "../components/hint/tooltipTestSupport";
import { CodedError } from "../errors/codedError";
import { consoleOf, projectOf, sessionOf } from "../fixtures/builders";
import { setLanguage } from "../i18n/language";
import { POLL_MS } from "../process/processPolling";
import type { ProcessCount } from "../protocol";
import { DaemonProvider, type Daemon } from "../store";
import { testDaemon } from "../test/testDaemon";
import { RailProcessesEntry } from "./RailProcessesEntry";

// The entry hands its rows to the panel entry as the content it renders, so the panel entry renders again only when
// the entry's rows (or another of its props) change.
const panelRenders = vi.hoisted(() => ({ count: 0 }));
vi.mock("./RailPanelEntry", async (original) => {
  const module = await original<typeof import("./RailPanelEntry")>();
  return { ...module, RailPanelEntry: (props: Parameters<typeof module.RailPanelEntry>[0]) => (panelRenders.count++, createElement(module.RailPanelEntry, props)) };
});

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const main = consoleOf("c", "Main");
const project = projectOf("p", main.id, "App");
const hub = sessionOf("hub", main.id, undefined, "Hub", "idle", { kind: "console" });

const COUNTS: ProcessCount[] = [
  { project_id: project.id, trees: 2 },
  { session_id: hub.id, trees: 2 },
  { project_id: "gone", trees: 1 },
];

beforeEach(() => {
  act(() => setLanguage("en"));
  HTMLElement.prototype.animate = vi.fn(() => ({ cancel: vi.fn(), finished: Promise.resolve(), onfinish: null })) as unknown as typeof HTMLElement.prototype.animate;
});

async function mount(initial: ProcessCount[]) {
  withFakeTimers();
  const onOpen = vi.fn(() => true);
  const answer: { counts: ProcessCount[]; failing: boolean; failure?: Error } = { counts: initial, failing: false };
  const request = vi.fn(async (body: { type: string }) => {
    if (answer.failing) throw answer.failure ?? new Error("offline");
    return body.type === "list_all_processes" ? { type: "all_processes_listed", id: null, counts: answer.counts } : { type: "unexpected", id: null };
  });
  const daemon = testDaemon({
    state: { consoles: new Map([[main.id, main]]), projects: new Map([[project.id, project]]), sessions: new Map([[hub.id, hub]]) },
    request: request as unknown as Daemon["request"], showNotice: vi.fn(),
  });
  const container = mountForTooltip(<DaemonProvider value={daemon}><nav><RailProcessesEntry onOpen={onOpen} /><button id="next">Next</button><span id="elsewhere"><DetailsButton>Elsewhere</DetailsButton></span></nav></DaemonProvider>);
  await act(async () => {});
  const advance = (ms: number) => act(async () => void vi.advanceTimersByTime(ms));
  return { container, onOpen, request, answer, advance, daemon };
}

const panel = () => document.querySelector<HTMLElement>('[role="dialog"]');
const button = (container: HTMLElement) => container.querySelector<HTMLButtonElement>("#rail-processes")!;

it("shows nothing while no process is running", async () => {
  const { container } = await mount([]);
  expect(container.querySelector("#rail-processes")).toBeNull();
});

it("counts the trees of the projects and console sessions it knows, and not those of a project it does not", async () => {
  const { container } = await mount(COUNTS);
  expect(button(container).getAttribute("aria-label")).toBe("4 processes are running. Show where they run.");
});

it("names the popover by what it lists, and its rows by everything they show", async () => {
  const { container } = await mount(COUNTS);
  press(button(container), "touch");
  expect(panel()!.getAttribute("aria-label")).toBe("Processes");
  expect(Array.from(panel()!.querySelectorAll("h3"), (heading) => heading.textContent)).toEqual(["Console sessions (1)", "Projects (1)"]);
  const rows = Array.from(panel()!.querySelectorAll("li button"), (row) => row.getAttribute("aria-label")?.replace(/[⁨⁩]/g, ""));
  expect(rows).toEqual([
    "Hub in Main has 2 processes running. Show the processes panel.",
    "App in Main has 2 processes running. Show the processes panel.",
  ]);
  for (const row of panel()!.querySelectorAll("li button")) expect(row.textContent).toContain("Main");
});

it.each([
  { counts: [{ project_id: project.id, trees: 2 }], heading: "Projects (1)" },
  { counts: [{ session_id: hub.id, trees: 2 }], heading: "Console sessions (1)" },
])("omits empty groups when only $heading have running processes", async ({ counts, heading }) => {
  const { container } = await mount(counts);
  press(button(container), "touch");
  expect(Array.from(panel()!.querySelectorAll("h3"), (element) => element.textContent)).toEqual([heading]);
  expect(panel()!.querySelectorAll("section[aria-labelledby]")).toHaveLength(1);
});

it("sends a project's row to its project, and a console session's row to that console session, once the popover has gone", async () => {
  const { container, onOpen, advance } = await mount(COUNTS);
  press(button(container), "touch");
  act(() => panel()!.querySelector<HTMLButtonElement>('button[aria-label*="App"]')!.click());
  await advance(1000);
  expect(onOpen).toHaveBeenLastCalledWith({ project: project.id });
  expect(panel()).toBeNull();
  press(button(container), "touch");
  act(() => panel()!.querySelector<HTMLButtonElement>('button[aria-label*="Hub"]')!.click());
  await advance(1000);
  expect(onOpen).toHaveBeenLastCalledWith({ consoleSession: hub.id });
});

it("focuses the first row when the keyboard opens it", async () => {
  const { container } = await mount(COUNTS);
  activate(button(container));
  expect(document.activeElement).toBe(panel()!.querySelector("li button"));
});

it("warns that the counts may be out of date when a refresh fails, and keeps the rows", async () => {
  const { container, answer, advance } = await mount(COUNTS);
  press(button(container), "touch");
  await advance(0);
  answer.failing = true;
  await advance(POLL_MS.live);
  expect(panel()!.textContent).toContain("Could not refresh the list");
  expect(panel()!.textContent).toContain("The counts shown may be out of date.");
  expect(panel()!.querySelectorAll("li button")).toHaveLength(2);
});

it("says why a refresh failed when the daemon names the cause, ahead of the counts being out of date", async () => {
  const { container, answer, advance } = await mount(COUNTS);
  press(button(container), "touch");
  await advance(0);
  answer.failing = true;
  // Worded as the store words a daemon's error for its caller.
  answer.failure = new CodedError("Counting the processes took too long.", "process_count_timed_out", {});
  await advance(POLL_MS.live);
  expect(panel()!.textContent).toContain("Counting the processes took too long. The counts shown may be out of date.");
  expect(panel()!.querySelectorAll("li button")).toHaveLength(2);
});

it("announces a refresh failure that arrives while the popover is open, and not one already there as it opens", async () => {
  const { container, answer, advance } = await mount(COUNTS);
  await advance(POLL_MS.background);
  answer.failing = true;
  await advance(POLL_MS.background);
  press(button(container), "touch");
  await advance(0);
  expect(panel()!.textContent).toContain("Could not refresh the list");
  expect(panel()!.querySelector("[role=alert]")).toBeNull();

  press(document.querySelector("[data-rail-processes-panel] li button")!, "touch");
  await advance(1000);
  answer.failing = false;
  await advance(POLL_MS.background);
  press(button(container), "touch");
  await advance(0);
  expect(panel()!.textContent).not.toContain("Could not refresh the list");
  answer.failing = true;
  await advance(POLL_MS.live);
  expect(panel()!.querySelector("[role=alert]")?.textContent).toContain("Could not refresh the list");
});

const details = () => document.querySelector<HTMLElement>("[data-details-popover]");
const railPanel = () => document.querySelector<HTMLElement>("[data-rail-processes-panel]");
const escape = (target: Element) => act(() => void target.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })));

it("keeps the popover open while the refresh failure's Details are open, and Escape closes the Details first", async () => {
  const { container, answer, advance } = await mount(COUNTS);
  press(button(container), "touch");
  await advance(0);
  answer.failing = true;
  await advance(POLL_MS.live);
  press(Array.from(panel()!.querySelectorAll("button")).find((control) => control.textContent === "Details")!, "touch");
  await advance(0);
  expect(details()).not.toBeNull();
  expect(railPanel()).not.toBeNull();
  press(details()!, "touch");
  await advance(0);
  expect(railPanel()).not.toBeNull();
  escape(details()!.querySelector('[role="dialog"]')!);
  await advance(1000);
  expect(details()).toBeNull();
  expect(railPanel()).not.toBeNull();
});

it("keeps a hover-opened popover while a press is in the Details of its refresh failure, and Escape closes them first", async () => {
  const { container, answer, advance } = await mount(COUNTS);
  enter(button(container));
  await advance(500);
  expect(railPanel()).not.toBeNull();
  answer.failing = true;
  await advance(POLL_MS.live);
  press(Array.from(railPanel()!.querySelectorAll("button")).find((control) => control.textContent === "Details")!, "mouse");
  await advance(0);
  expect(details()).not.toBeNull();
  press(details()!.querySelector('[role="dialog"]')!, "mouse");
  await advance(0);
  expect(details()).not.toBeNull();
  expect(railPanel()).not.toBeNull();
  escape(details()!.querySelector('[role="dialog"]')!);
  await advance(1000);
  expect(details()).toBeNull();
  expect(railPanel()).not.toBeNull();
});

it("asks faster only while the popover is open, and not again once the last process is gone", async () => {
  const { container, request, answer, advance } = await mount(COUNTS);
  await advance(POLL_MS.live);
  expect(request).toHaveBeenCalledTimes(1);
  press(button(container), "touch");
  await advance(0);
  await advance(POLL_MS.live);
  const live = request.mock.calls.length;
  expect(live).toBe(3);
  answer.counts = [];
  await advance(POLL_MS.live);
  expect(container.querySelector("#rail-processes")).toBeNull();
  answer.counts = COUNTS;
  const gone = request.mock.calls.length;
  await advance(POLL_MS.live);
  expect(request.mock.calls.length).toBe(gone);
  await advance(POLL_MS.background);
  expect(button(container)).not.toBeNull();
  expect(panel()).toBeNull();
});

it("hands focus to the next rail control when the last process is gone", async () => {
  const { container, answer, advance } = await mount(COUNTS);
  button(container).focus();
  answer.counts = [];
  await advance(POLL_MS.background);
  expect(container.querySelector("#rail-processes")).toBeNull();
  expect(document.activeElement).toBe(container.querySelector("#next"));
});

const detailsButton = (scope: ParentNode) => Array.from(scope.querySelectorAll("button")).find((control) => control.textContent === "Details")!;

/** Opens the hover-form popover with its refresh failure showing and the failure's Details pressed open. */
async function hoverOpenWithDetails(mounted: Awaited<ReturnType<typeof mount>>) {
  enter(button(mounted.container));
  await mounted.advance(500);
  mounted.answer.failing = true;
  await mounted.advance(POLL_MS.live);
  press(detailsButton(railPanel()!), "mouse");
  await mounted.advance(0);
  expect(details()).not.toBeNull();
}

it("hands focus to the rail button when a hover-opened popover closes with focus on its Details button", async () => {
  const mounted = await mount(COUNTS);
  await hoverOpenWithDetails(mounted);
  act(() => details()!.querySelector<HTMLElement>('[role="dialog"]')!.focus());
  escape(details()!.querySelector('[role="dialog"]')!);
  await mounted.advance(1000);
  expect(details()).toBeNull();
  act(() => detailsButton(railPanel()!).focus());
  leave(button(mounted.container));
  await mounted.advance(HOVER_CLOSE_DELAY_MS * 2);
  expect(railPanel()).toBeNull();
  expect(document.activeElement).toBe(button(mounted.container));
  // The handoff sets the keyboard's modality; the ring shows only because focus moves after React's commit.
  expect(getInteractionModality()).toBe("keyboard");
  expect(button(mounted.container).hasAttribute("data-focus-visible")).toBe(true);
});

it("leaves focus alone when it was claimed elsewhere by the time a closing popover would hand it to the rail button", async () => {
  const mounted = await mount(COUNTS);
  await hoverOpenWithDetails(mounted);
  act(() => details()!.querySelector<HTMLElement>('[role="dialog"]')!.focus());
  escape(details()!.querySelector('[role="dialog"]')!);
  await mounted.advance(1000);
  act(() => detailsButton(railPanel()!).focus());
  leave(button(mounted.container));
  // Closing the popover queues the handoff; the next control takes the focus before it runs.
  const next = mounted.container.querySelector<HTMLButtonElement>("#next")!;
  const queue = queueMicrotask;
  vi.stubGlobal("queueMicrotask", (callback: () => void) => queue(() => { if (!railPanel()) next.focus(); callback(); }));
  onTestFinished(() => void vi.unstubAllGlobals());
  await mounted.advance(HOVER_CLOSE_DELAY_MS * 2);
  expect(railPanel()).toBeNull();
  expect(document.activeElement).toBe(next);
});

it("keeps a hover-opened popover up past the grace while its Details are, and closes it once they are gone", async () => {
  const mounted = await mount(COUNTS);
  await hoverOpenWithDetails(mounted);
  leave(button(mounted.container));
  await mounted.advance(HOVER_CLOSE_DELAY_MS * 5);
  expect(railPanel()).not.toBeNull();
  escape(details()!.querySelector('[role="dialog"]')!);
  await mounted.advance(1000);
  expect(details()).toBeNull();
  expect(railPanel()).toBeNull();
});

it("is not held open by a Details popover opened elsewhere", async () => {
  const { container, advance } = await mount(COUNTS);
  press(detailsButton(container.querySelector("#elsewhere")!), "mouse");
  await advance(0);
  expect(details()).not.toBeNull();
  enter(button(container));
  await advance(500);
  expect(railPanel()).not.toBeNull();
  leave(button(container));
  await advance(HOVER_CLOSE_DELAY_MS * 5);
  expect(railPanel()).toBeNull();
});

it("leaves focus in a Details popover opened elsewhere alone when the last process is gone", async () => {
  const { container, answer, advance } = await mount(COUNTS);
  press(detailsButton(container.querySelector("#elsewhere")!), "touch");
  await advance(0);
  const dialog = details()!.querySelector<HTMLElement>('[role="dialog"]')!;
  act(() => dialog.focus());
  expect(document.activeElement).toBe(dialog);
  answer.counts = [];
  await advance(POLL_MS.background);
  expect(container.querySelector("#rail-processes")).toBeNull();
  expect(document.activeElement).toBe(dialog);
});

it("does not hand focus to the rail button when a row is pressed, since the jump lands it", async () => {
  const { container, onOpen, advance } = await mount(COUNTS);
  enter(button(container));
  await advance(500);
  const row = railPanel()!.querySelector<HTMLButtonElement>("li button")!;
  act(() => row.focus());
  press(row, "mouse");
  await advance(1000);
  expect(railPanel()).toBeNull();
  expect(onOpen).toHaveBeenCalledTimes(1);
  expect(document.activeElement).not.toBe(button(container));
});

it("takes focus back to the rail button when a pressed row's project or session is gone by the jump", async () => {
  const { container, onOpen, advance } = await mount(COUNTS);
  onOpen.mockReturnValueOnce(false);
  enter(button(container));
  await advance(500);
  const row = railPanel()!.querySelector<HTMLButtonElement>("li button")!;
  act(() => row.focus());
  press(row, "mouse");
  await advance(1000);
  expect(railPanel()).toBeNull();
  expect(onOpen).toHaveBeenCalledTimes(1);
  expect(document.activeElement).toBe(button(container));
});

/** A modal status mark popover, opened from the panel named `owner` (or from outside any panel), portalled in directly
 * under the body. */
function openModalPopover(owner?: string) {
  const popover = document.createElement("div");
  popover.setAttribute("data-status-mark-popover", "");
  if (owner !== undefined) popover.setAttribute("data-popover-owner", owner);
  popover.setAttribute("role", "dialog");
  popover.setAttribute("aria-modal", "true");
  act(() => void document.body.append(popover));
  return popover;
}

it("closes at once when a status mark's popover opened elsewhere on the page comes up, and not for one opened from inside", async () => {
  const { container, advance } = await mount(COUNTS);
  enter(button(container));
  await advance(500);
  expect(railPanel()).not.toBeNull();
  const inside = openModalPopover("rail-processes");
  await advance(0);
  expect(railPanel()).not.toBeNull();
  inside.remove();
  const elsewhere = openModalPopover();
  await advance(0);
  expect(railPanel()).toBeNull();
  elsewhere.remove();
});

it("does not render again for a change to a session it names that alters nothing it shows", async () => {
  const { container, daemon } = await mount(COUNTS);
  const before = panelRenders.count;
  act(() => daemon.store.setState((state) => ({ sessions: new Map(state.sessions).set(hub.id, { ...hub, status: "working" }) })));
  act(() => daemon.store.setState((state) => ({ sessions: new Map(state.sessions).set("other", sessionOf("other", main.id, undefined, "Other", "idle")) })));
  expect(panelRenders.count).toBe(before);
  act(() => daemon.store.setState((state) => ({ sessions: new Map(state.sessions).set(hub.id, { ...hub, title: "Renamed" }) })));
  expect(panelRenders.count).toBeGreaterThan(before);
  press(button(container), "touch");
  expect(panel()!.textContent).toContain("Renamed");
});
