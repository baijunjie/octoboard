// @vitest-environment jsdom
import { act } from "react";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import {
  archivedCoordinator,
  boundArchived,
  boundConsoleSession,
  console_,
  otherConsoleSession,
  otherOwnerArchived,
  unboundArchived,
  underArchivedCoordinator,
} from "../fixtures/archiveSessions";
import { expectIndependentMarquees, stubMarquee } from "../components/motion/marqueeTestSupport";
import { errorAlerts } from "../components/alerts/errorAlerts";
import { isolate } from "../i18n/isolate";
import type { DialogRequest } from "../dialogs/dialogRequest";
import type { Session } from "../protocol";
import { closeOpenTooltip, enter, openTooltip } from "../components/hint/tooltipTestSupport";
import { TEST_PROVIDERS } from "../test/testProviders";
import { ArchiveView, OPEN_FOCUS_FRAMES, FALLBACK_FOCUS_FRAMES } from "./ArchiveView";
import { fakeTimers, runOutTimers } from "../test/clock";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The clock's own timer would count as pending work in the marquee checks, which these tests do not use it for.
vi.mock("../format/useNow", () => ({ useNow: () => 0 }));

// Everything the fixture knows about besides `boundConsoleSession`'s own bound, archived sessions:
// its own (idle, not archived) row, a second owner, a session bound to that second owner, and an
// unbound one. Passed in full so the view's own filtering, not the fixture, is what narrows it down.
const OTHER_SESSIONS = [boundConsoleSession, otherConsoleSession, otherOwnerArchived, unboundArchived];

/** Renders the view for `boundConsoleSession`'s own scope and returns its container, for a test to
 * query, and `rerender`, which renders it again with `changes` over the original arguments (changes
 * do not carry over from one call to the next). The root is unmounted when the test ends. */
function renderArchiveView(
  onOpenDialog: (dialog: DialogRequest) => void = () => {},
  onClose: () => void = () => {},
  sessions: Session[] = [...OTHER_SESSIONS, ...boundArchived],
  overrides: Partial<React.ComponentProps<typeof ArchiveView>> = {},
): { container: HTMLElement; rerender: (changes: Partial<React.ComponentProps<typeof ArchiveView>>) => void } {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const render = (changes: Partial<React.ComponentProps<typeof ArchiveView>>) =>
    act(() =>
      root.render(
        <ArchiveView
          console={console_}
          boundTo={boundConsoleSession}
          sessions={sessions}
          consoles={new Map([[console_.id, console_]])}
          projects={new Map()}
          accounts={[]}
          defaultNames={{}}
          providers={TEST_PROVIDERS}
          onSelect={() => {}}
          onReopen={() => {}}
          onOpenDialog={onOpenDialog}
          dialogOpen={false}
          onClose={onClose}
          {...overrides}
          {...changes}
        />,
      ),
    );
  render({});
  return { container, rerender: render };
}

it("lists a console session's own archived bound sessions, not its own archived row or an unrelated session", () => {
  const { container } = renderArchiveView();
  const titles = Array.from(container.querySelectorAll("ul > li:has(button)")).map((li) => li.textContent ?? "");
  expect(titles).toHaveLength(boundArchived.length);
  for (const session of boundArchived) expect(titles.some((t) => t.includes(session.title))).toBe(true);
  for (const session of OTHER_SESSIONS) expect(titles.some((t) => t.includes(session.title))).toBe(false);
});

it("titles the view after the owning console session, not the console", () => {
  const { container } = renderArchiveView();
  const section = container.querySelector("section[data-region='archive']");
  expect(section?.getAttribute("aria-label")).toBe(`Archived sessions bound to ${isolate(boundConsoleSession.title)}`);
});

it("deletes all of a console session's archive by naming that console session", () => {
  const onOpenDialog = vi.fn();
  const { container } = renderArchiveView(onOpenDialog);
  const deleteAll = Array.from(container.querySelectorAll("button")).find((b) => b.textContent === "Delete all");
  act(() => deleteAll?.click());
  expect(onOpenDialog).toHaveBeenCalledWith(
    expect.objectContaining({ kind: "delete-archived", consoleSession: boundConsoleSession, project: undefined }),
  );
});

it("lists the sessions archived under a bound coordinator after it, inset, and names it for assistive technology", () => {
  const { container } = renderArchiveView(undefined, undefined, [boundConsoleSession, archivedCoordinator, ...underArchivedCoordinator, ...boundArchived]);
  const rows = Array.from(container.querySelectorAll<HTMLElement>("ul > li:has(button)"));
  // Newest first, except that the bound coordinator's own sessions follow it rather than being ranked
  // against the console session's.
  expect(rows.map((row) => [row.querySelector("[dir=auto]")?.textContent, row.className.includes("ms-[17px]")])).toEqual([
    [boundArchived[0].title, false],
    [archivedCoordinator.title, false],
    [underArchivedCoordinator[0].title, true],
    [underArchivedCoordinator[1].title, true],
    [boundArchived[1].title, false],
    [boundArchived[2].title, false],
  ]);
  expect(rows[2].querySelector("[role=button]")?.getAttribute("aria-label")).toContain(`Bound to ${isolate(archivedCoordinator.title)}`);
});

it("shows a row's account, model and effort beside its agent mark, which its accessible name ends with", () => {
  const observed: Session = { ...boundArchived[0], observed_model: "claude-opus-5-5", observed_effort: "high" };
  const { container } = renderArchiveView(undefined, undefined, [observed]);
  const row = container.querySelector<HTMLElement>("ul > li:has(button)")!;
  expect(row.textContent).toContain(`${isolate("Default")} · ${isolate("Opus 5.5")} · ${isolate("high")}`);
  expect(row.textContent).not.toContain("Claude Code");
  expect(row.querySelector("[role=button]")?.getAttribute("aria-label")?.endsWith(`${isolate("Opus 5.5")} · ${isolate("high")}`)).toBe(true);
});

it("leads a row's context badge with the ring of the share used where the agent reported a window", () => {
  const sized: Session = { ...boundArchived[0], context: { tokens: 130000, window: 200000, used_percent: 65 } };
  const { container } = renderArchiveView(undefined, undefined, [sized]);
  const row = container.querySelector<HTMLElement>("ul > li:has(button)")!;
  expect(row.querySelector("[data-variant] > .progress-circle")).not.toBeNull();
  expect(row.textContent).toContain("130K");
});

it("shows the model and effort a row's session inherits from its console where it set and showed none", () => {
  const session = boundArchived[0];
  const owner = { ...console_, project_session_launch: { ...console_.project_session_launch, [session.agent]: { compatibility: {}, model: "claude-sonnet-5-5", effort: "low" } } };
  const { container } = renderArchiveView(undefined, undefined, [session], { consoles: new Map([[owner.id, owner]]) });
  expect(container.querySelector("ul > li:has(button)")?.textContent).toContain(`${isolate("Sonnet 5.5")} · ${isolate("low")}`);
});

it("says a row's session runs the default model where nothing names one", () => {
  const { container } = renderArchiveView(undefined, undefined, [boundArchived[0]]);
  expect(container.querySelector("ul > li:has(button)")?.textContent).toContain(`${isolate("Default")} · Default model`);
});

it("selects a row's session when the row is pressed or given Enter, but not when Reopen or Delete is", () => {
  const onSelect = vi.fn();
  const onReopen = vi.fn();
  const onOpenDialog = vi.fn();
  const { container } = renderArchiveView(onOpenDialog, () => {}, undefined, { onSelect, onReopen });
  const row = container.querySelector<HTMLElement>("ul > li [role=button]")!;
  act(() => row.click());
  expect(onSelect).toHaveBeenCalledTimes(1);
  expect(onSelect).toHaveBeenCalledWith(boundArchived[0]);
  act(() => {
    row.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
  });
  expect(onSelect).toHaveBeenCalledTimes(2);
  const reopen = Array.from(row.querySelectorAll("button")).find((b) => b.getAttribute("aria-label")?.startsWith("Reopen"))!;
  act(() => reopen.click());
  expect(onReopen).toHaveBeenCalledWith(boundArchived[0]);
  const remove = row.querySelector<HTMLElement>("button[aria-label^='Delete']")!;
  act(() => remove.click());
  expect(onOpenDialog).toHaveBeenCalledWith({ kind: "delete-session", session: boundArchived[0] });
  expect(onSelect).toHaveBeenCalledTimes(2);
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

it.each(["header", "row"] as const)("runs only the hovered text in the archive %s, leaving buttons and glyphs still", (part) => {
  stubMarquee();
  const { container } = renderArchiveView();
  const surface = container.querySelector<HTMLElement>(part === "header" ? "header" : "ul > li:has(button)")!;
  const labels = Array.from(surface.querySelectorAll<HTMLElement>(".overflow-hidden.whitespace-nowrap"));
  expect(labels).toHaveLength(2);
  expectIndependentMarquees(labels, [surface, ...surface.querySelectorAll("button, svg")]);
});

function advanceFrames(count: number): void {
  act(() => {
    for (let frame = 0; frame < count; frame++) vi.advanceTimersToNextFrame();
  });
}

it("brings focus lost to the page back to the close button, with its ring shown", () => {
  fakeTimers();
  const { container } = renderArchiveView();
  advanceFrames(OPEN_FOCUS_FRAMES);
  expect(document.activeElement).toBe(container.querySelector("section[data-region='archive']"));
  // What a deleted row's button leaves behind, with the last input a pointer press.
  act(() => {
    (document.activeElement as HTMLElement).blur();
    setInteractionModality("pointer");
  });
  expect(document.activeElement).toBe(document.body);
  advanceFrames(FALLBACK_FOCUS_FRAMES);
  expect(document.activeElement).toBe(container.querySelector("button[aria-label='Close']"));
  expect(getInteractionModality()).toBe("keyboard");
});

// A Delete all pressed with the pointer takes no focus, so its dialog hands focus back to the section, which shows no
// ring; once that dialog has deleted rows, focus goes on to Close as if it had been lost. A dialog that deleted
// nothing leaves it there.
it.each([
  { what: "deleted every row", after: OTHER_SESSIONS, lands: "close" },
  { what: "deleted nothing", after: undefined, lands: "section" },
])("moves focus from the section to the close button only once a dialog that deleted rows closes ($what)", ({ after, lands }) => {
  fakeTimers();
  const { container, rerender } = renderArchiveView();
  advanceFrames(OPEN_FOCUS_FRAMES);
  const section = container.querySelector("section[data-region='archive']");
  expect(document.activeElement).toBe(section);
  act(() => setInteractionModality("pointer"));
  rerender({ dialogOpen: true });
  advanceFrames(FALLBACK_FOCUS_FRAMES);
  rerender({ dialogOpen: false, ...(after ? { sessions: after } : {}) });
  advanceFrames(FALLBACK_FOCUS_FRAMES);
  expect(document.activeElement).toBe(lands === "close" ? container.querySelector("button[aria-label='Close']") : section);
  expect(getInteractionModality()).toBe(lands === "close" ? "keyboard" : "pointer");
});

it("leaves focus that is on a control where it is when the fallback runs", () => {
  fakeTimers();
  const { container } = renderArchiveView();
  advanceFrames(OPEN_FOCUS_FRAMES);
  const row = container.querySelector<HTMLElement>("ul > li [role=button]")!;
  act(() => row.focus());
  advanceFrames(FALLBACK_FOCUS_FRAMES);
  expect(document.activeElement).toBe(row);
});

it("opens at the loaded page count and scroll offset it is restored to, and reports where it is scrolled", () => {
  const onViewState = vi.fn();
  // Two of the three rows are shown, so the end-of-list sentinel is observed.
  vi.stubGlobal("IntersectionObserver", class { observe() {} disconnect() {} });
  onTestFinished(() => void vi.unstubAllGlobals());
  const { container } = renderArchiveView(() => {}, () => {}, undefined, {
    restore: { shown: 2, scrollTop: 40 },
    onViewState,
  });
  expect(container.querySelectorAll("ul > li:has([role=button])")).toHaveLength(2);
  const scroller = container.querySelector<HTMLElement>("section[data-region='archive'] > div")!;
  expect(scroller.scrollTop).toBe(40);
  act(() => {
    scroller.scrollTop = 10;
    scroller.dispatchEvent(new Event("scroll", { bubbles: true }));
  });
  expect(onViewState).toHaveBeenLastCalledWith({ shown: 2, scrollTop: 10 });
});

it("offers no lock or unlock control in the archive", () => {
  const { container } = renderArchiveView();
  expect(container.querySelector('button[aria-label="Lock"], button[aria-label^="Unlock "]')).toBeNull();
});

it("always shows a row's Reopen and Delete on a device that cannot hover, and reveals them on hover or focus otherwise", () => {
  const { container } = renderArchiveView();
  const row = container.querySelector<HTMLElement>("ul > li [role=button]")!;
  const reopen = Array.from(row.querySelectorAll("button")).find((b) => b.getAttribute("aria-label")?.startsWith("Reopen"))!;
  const clip = reopen.closest<HTMLElement>(".max-w-0")!;
  expect(clip.className).toContain("[@media(hover:none)]:max-w-none [@media(hover:none)]:overflow-visible [@media(hover:none)]:opacity-100");
  expect(clip.className).toContain("group-hover:max-w-none");
  expect(clip.className).toContain("group-focus-within:max-w-none");
});

// A row's actions are three icon-only buttons, each named by its tooltip; Open in the agent's app is offered where the
// daemon found the agent's app installed.
const CONVERSATION = "0b8f1c52-3d1e-4a7b-9c41-5e6d7f8091a2";
const resumable = (session: Session, extra: Partial<Session> = {}): Session => ({ ...session, agent_session_id: CONVERSATION, has_conversation: true, ...extra });
const rowButtons = (container: HTMLElement) => Array.from(container.querySelector<HTMLElement>("ul > li [role=button]")!.querySelectorAll<HTMLButtonElement>(".sidebar-fills button"));
/** The row button for the action `name`, whose accessible name goes on to say which session it is for. */
const buttonNamed = (container: HTMLElement, name: string) => rowButtons(container).find((button) => button.getAttribute("aria-label")?.startsWith(name));
/** Which action a row button is, by what its accessible name starts with. */
const actionOf = (button: HTMLElement) => ["Open in Claude app", "Open in Codex app", "Reopen", "Delete"].find((action) => button.getAttribute("aria-label")?.startsWith(action));

/** The text of the tooltip a keyboard user opens by focusing `button`. */
function tooltipOnFocus(button: HTMLElement): string | null | undefined {
  act(() => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    button.focus();
  });
  onTestFinished(closeOpenTooltip);
  return document.querySelector("[role=tooltip]")?.textContent;
}

it("gives a row three icon-only buttons, Open in the agent's app, Reopen and Delete, in that order, named for the session, with tooltips that are the action alone", () => {
  const session = resumable(boundArchived[0]);
  const { container } = renderArchiveView(undefined, undefined, [session], { nativeApps: ["claude"], onOpenInApp: () => {} });

  const buttons = rowButtons(container);
  const title = isolate(boundArchived[0].title);
  expect(buttons.map((button) => button.getAttribute("aria-label"))).toEqual([`Open in Claude app: ${title}`, `Reopen session ${title}`, `Delete session ${title}`]);
  expect(buttons.map((button) => button.textContent)).toEqual(["", "", ""]);
  expect(buttons.every((button) => button.querySelector("svg"))).toBe(true);
  expect(buttons.map((button) => button.disabled)).toEqual([false, false, false]);
  buttons.forEach((button, index) => expect(tooltipOnFocus(button)).toBe(["Open in Claude app", "Reopen", "Delete"][index]));
});

it("opens the row's conversation in its agent's app when Open in the app is pressed, without selecting the row or reopening it", () => {
  const session = resumable(boundArchived[0]);
  const onSelect = vi.fn();
  const onReopen = vi.fn();
  const onOpenInApp = vi.fn();
  const { container } = renderArchiveView(undefined, undefined, [session], { nativeApps: ["claude"], onOpenInApp, onSelect, onReopen });

  act(() => buttonNamed(container, "Open in Claude app")!.click());

  expect(onOpenInApp).toHaveBeenCalledExactlyOnceWith(session);
  expect(onSelect).not.toHaveBeenCalled();
  expect(onReopen).not.toHaveBeenCalled();
});

it("offers Open in the app only where the agent has an app that is installed, and the view can open one", () => {
  const session = resumable(boundArchived[0]);
  const names = (overrides: Partial<React.ComponentProps<typeof ArchiveView>>, row: Session = session) =>
    rowButtons(renderArchiveView(undefined, undefined, [row], overrides).container).map(actionOf);
  const onOpenInApp = () => {};

  expect(names({ nativeApps: ["claude"], onOpenInApp })).toEqual(["Open in Claude app", "Reopen", "Delete"]);
  expect(names({ nativeApps: [], onOpenInApp })).toEqual(["Reopen", "Delete"]);
  expect(names({ onOpenInApp })).toEqual(["Reopen", "Delete"]);
  expect(names({ nativeApps: ["codex"], onOpenInApp })).toEqual(["Reopen", "Delete"]);
  expect(names({ nativeApps: ["claude"] })).toEqual(["Reopen", "Delete"]);
  // An agent that declares no app is offered none, whatever the probe says.
  expect(names({ nativeApps: ["grok"], onOpenInApp }, { ...session, agent: "grok" })).toEqual(["Reopen", "Delete"]);
  expect(names({ nativeApps: ["codex"], onOpenInApp }, { ...session, agent: "codex" })).toEqual(["Open in Codex app", "Reopen", "Delete"]);
});

it.each([
  ["it has no conversation yet", {}, { has_conversation: false }],
  ["it recorded no conversation id", {}, { agent_session_id: null }],
])("makes Open in the app unavailable, which does nothing when pressed, when %s", (_name, overrides, sessionChanges) => {
  const onOpenInApp = vi.fn();
  const { container } = renderArchiveView(undefined, undefined, [resumable(boundArchived[0], sessionChanges)], { nativeApps: ["claude"], onOpenInApp, ...overrides });

  const open = buttonNamed(container, "Open in Claude app")!;
  expect(open.getAttribute("aria-disabled")).toBe("true");
  expect(open.disabled).toBe(false);
  // Unavailable, not disabled: it keeps keyboard focus, which is where its tooltip comes from.
  act(() => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    open.focus();
  });
  expect(document.activeElement).toBe(open);
  onTestFinished(closeOpenTooltip);
  act(() => open.click());
  expect(onOpenInApp).not.toHaveBeenCalled();
  // Reopen and Delete are not held back by it.
  expect(buttonNamed(container, "Reopen")!.disabled).toBe(false);
  expect(buttonNamed(container, "Delete")!.disabled).toBe(false);
});

it("presses Open in the app unavailable to explain, raising the refusal's reason as an alert", () => {
  const onOpenInApp = vi.fn();
  const { container } = renderArchiveView(undefined, undefined, [resumable(boundArchived[0], { has_conversation: false })], { nativeApps: ["claude"], onOpenInApp });
  onTestFinished(() => void act(() => errorAlerts.acknowledge()));

  act(() => buttonNamed(container, "Open in Claude app")!.click());

  expect(onOpenInApp).not.toHaveBeenCalled();
  expect(errorAlerts.getSnapshot().map((alert) => [alert.title, alert.message])).toEqual([["Could not open the agent's app", "The session has no conversation yet, so there is nothing to open."]]);
});

it("gives Open in the app, where nothing needs saying, no description of its own, and not the tooltip's either", () => {
  fakeTimers();
  const { container } = renderArchiveView(undefined, undefined, [resumable(boundArchived[0])], { nativeApps: ["claude"], onOpenInApp: () => {} });
  const open = buttonNamed(container, "Open in Claude app")!;

  enter(open);
  runOutTimers();

  expect(openTooltip()?.textContent).toBe("Open in Claude app");
  expect(open.getAttribute("aria-describedby") ?? "").toBe("");
});

/** The open tooltip as the rail avatar's is laid out: a heading, a groove line, the text under it. */
function headedTooltip(): { heading: string | null | undefined; body: string | null | undefined } {
  const line = openTooltip()?.querySelector("hr");
  return { heading: line?.previousElementSibling?.textContent, body: line?.nextElementSibling?.textContent };
}

/** The open headed tooltip's heading element. */
const headingElement = () => openTooltip()?.querySelector("hr")?.previousElementSibling;

/** What the control's `aria-describedby` names, as its description. */
const descriptionOf = (control: HTMLElement) => document.getElementById(control.getAttribute("aria-describedby") ?? "")?.textContent;

it("says why Open in the app does nothing in its tooltip, after its name", () => {
  fakeTimers();
  const { container } = renderArchiveView(undefined, undefined, [resumable(boundArchived[0], { has_conversation: false })], { nativeApps: ["claude"], onOpenInApp: () => {} });
  const open = buttonNamed(container, "Open in Claude app")!;

  enter(open);
  runOutTimers();

  expect(headedTooltip()).toEqual({ heading: "Open in Claude app", body: "The session has no conversation yet, so there is nothing to open." });
  expect(descriptionOf(open)).toBe("The session has no conversation yet, so there is nothing to open.");
  // An action's name keeps the regular weight, as in the tooltip it shows with nothing to add.
  expect(headingElement()?.classList.contains("font-medium")).toBe(false);
});

it("keeps Open in the app enabled where the app may not see the conversation, and says so in its tooltip, after its name", () => {
  fakeTimers();
  const onOpenInApp = vi.fn();
  const { container } = renderArchiveView(undefined, undefined, [resumable(boundArchived[0])], { nativeApps: ["claude"], onOpenInApp, nativeAppsUnseen: new Set([boundArchived[0].id]) });
  const open = buttonNamed(container, "Open in Claude app")!;
  expect(open.disabled).toBe(false);
  expect(open.getAttribute("aria-disabled")).toBeNull();

  enter(open);
  runOutTimers();
  const sentence = "The agent's app may not see this conversation: its latest record is kept in this account's own directory, not the default one.";
  expect(headedTooltip()).toEqual({ heading: "Open in Claude app", body: sentence });
  expect(headingElement()?.classList.contains("font-medium")).toBe(false);
  expect(descriptionOf(open)).toBe(sentence);

  act(() => open.click());
  expect(onOpenInApp).toHaveBeenCalledWith(expect.objectContaining({ id: boundArchived[0].id }));
});

const listed = boundArchived.map((session) => resumable(session));

it("asks to probe the apps for the rows it lists as it opens, and again for only the rows it lists as more load", () => {
  let loadMore!: () => void;
  vi.stubGlobal("IntersectionObserver", class {
    constructor(callback: IntersectionObserverCallback) { loadMore = () => callback([{ isIntersecting: true } as IntersectionObserverEntry], this as unknown as IntersectionObserver); }
    observe() {}
    disconnect() {}
  });
  onTestFinished(() => void vi.unstubAllGlobals());
  const onRefreshNativeApps = vi.fn();
  renderArchiveView(undefined, undefined, listed, { restore: { shown: 1, scrollTop: 0 }, onRefreshNativeApps });

  expect(onRefreshNativeApps).toHaveBeenCalledExactlyOnceWith([listed[0].id]);

  act(() => loadMore());

  expect(onRefreshNativeApps).toHaveBeenCalledTimes(2);
  expect(onRefreshNativeApps).toHaveBeenLastCalledWith(listed.slice(1).map((session) => session.id));
});

it("asks only about rows that have a conversation of an agent with an app, still refreshing which apps are installed as it opens", () => {
  const onRefreshNativeApps = vi.fn();
  const rows = [listed[0], { ...listed[1], has_conversation: false }, { ...listed[2], agent: "grok" }];
  renderArchiveView(undefined, undefined, rows, { onRefreshNativeApps });

  expect(onRefreshNativeApps).toHaveBeenCalledExactlyOnceWith([listed[0].id]);

  const none = vi.fn();
  renderArchiveView(undefined, undefined, [rows[1], rows[2]], { onRefreshNativeApps: none });
  expect(none).toHaveBeenCalledExactlyOnceWith([]);
});

it("asks in requests of at most two hundred rows", () => {
  const rows = Array.from({ length: 250 }, (_, i) => resumable({ ...boundArchived[0], id: `s-many-${i}`, title: `Many ${i}` }));
  const onRefreshNativeApps = vi.fn();
  renderArchiveView(undefined, undefined, rows, { restore: { shown: 250, scrollTop: 0 }, onRefreshNativeApps });

  expect(onRefreshNativeApps.mock.calls.map(([ids]) => ids.length)).toEqual([200, 50]);
});

it("asks about a row archived while the view is open, and about no row twice", () => {
  const onRefreshNativeApps = vi.fn();
  const { rerender } = renderArchiveView(undefined, undefined, [listed[0]], { onRefreshNativeApps });
  expect(onRefreshNativeApps).toHaveBeenCalledExactlyOnceWith([listed[0].id]);

  rerender({ sessions: [listed[0], listed[1]] });

  expect(onRefreshNativeApps).toHaveBeenCalledTimes(2);
  expect(onRefreshNativeApps).toHaveBeenLastCalledWith([listed[1].id]);
});

it("renders the same on a rerender that lists no more rows, asking nothing again", () => {
  const onRefreshNativeApps = vi.fn();
  const { rerender } = renderArchiveView(undefined, undefined, listed, { onRefreshNativeApps });
  expect(onRefreshNativeApps).toHaveBeenCalledExactlyOnceWith(listed.map((session) => session.id));

  rerender({ nativeApps: ["claude"] });

  expect(onRefreshNativeApps).toHaveBeenCalledTimes(1);
});
